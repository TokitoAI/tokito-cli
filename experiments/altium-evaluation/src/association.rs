//! Candidate joins only. Explicit IDs bind model records; binary component indices
//! and every placement property remain unqualified observations.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn records<'a>(streams: &'a [Value], path: &str) -> Result<&'a [Value], String> {
    streams
        .iter()
        .find(|s| s["path"] == path)
        .and_then(|s| s["framedRecordInspection"]["records"].as_array())
        .map(Vec::as_slice)
        .ok_or_else(|| format!("association_missing_records:{path}"))
}

pub fn inspect(streams: &[Value]) -> Result<Value, String> {
    let components = records(streams, "/Components6/Data")?;
    let models = records(streams, "/Models/Data")?;
    let bodies = records(streams, "/ComponentBodies6/Data")?;
    let mut ids: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
    for model in models {
        let id = model["properties"]["ID"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("association_missing_model_id")?;
        ids.entry(id).or_default().push(model);
    }
    let mut groups: Vec<Value> = components
        .iter()
        .map(|c| {
            json!({
        "componentOrdinal":c["ordinal"], "sourceIdentity":{
            "namespace":"altium-pcb-uniqueid", "uniqueId":c["properties"]["UNIQUEID"],
            "reference":c["properties"]["SOURCEDESIGNATOR"]},
        "sourceProperties":c["properties"], "bodyCandidates":[],
        "componentIndexSemanticsIndependentlyVerified":false,
        "selectableGeometryExported":false})
        })
        .collect();
    let mut unresolved = Vec::new();
    for body in bodies {
        let mut candidates = Vec::new();
        if let Some(matches) = body["properties"]["MODELID"]
            .as_str()
            .and_then(|id| ids.get(id))
        {
            for model in matches {
                let ordinal = model["ordinal"]
                    .as_u64()
                    .ok_or("association_invalid_model_ordinal")?;
                let path = format!("/Models/{ordinal}");
                let embedded = streams
                    .iter()
                    .find(|s| s["path"] == path)
                    .map(|s| s["embeddedStep"].clone())
                    .unwrap_or(Value::Null);
                candidates.push(
                    json!({"modelOrdinal":ordinal,"sourceProperties":model["properties"],
                    "embeddedStepEvidence":embedded}),
                );
            }
        }
        let hashes: BTreeSet<_> = candidates
            .iter()
            .filter_map(|m| m["embeddedStepEvidence"]["decompressedSha256"].as_str())
            .collect();
        let all_bytes_present = candidates.iter().all(|m| {
            m["embeddedStepEvidence"]["completeZlibStream"] == true
                && m["embeddedStepEvidence"]["decompressedSha256"]
                    .as_str()
                    .is_some_and(|hash| !hash.is_empty())
        });
        let properties_agree = candidates.first().is_some_and(|first| {
            candidates
                .iter()
                .all(|m| m["sourceProperties"] == first["sourceProperties"])
        });
        let source_pose_inputs = body["componentIndexCandidate"]
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| components.get(i))
            .ok_or_else(|| "source_component_unresolved".to_string())
            .and_then(|component| crate::pose_inputs::inspect(&component["properties"], &body["properties"]))
            .unwrap_or_else(|error| json!({"schema":"tokito.altium-source-pose-inputs.v1",
                "numericInputsResolved":false,"error":error,"worldTransform":Value::Null,"poseVerified":false}));
        let candidate = json!({"bodyOrdinal":body["ordinal"],"byteOffset":body["byteOffset"],
            "componentIndexCandidate":body["componentIndexCandidate"],
            "sourceProperties":body["properties"],"remainingPayloadBytes":body["remainingPayloadBytes"],
            "modelCandidates":candidates,"explicitModelIdResolved":!candidates.is_empty(),
            "embeddedByteIdentityAgrees":all_bytes_present && hashes.len()==1,
            "modelPlacementPropertiesAgree":properties_agree,
            "sourcePoseInputs":source_pose_inputs,
            "selectedModelOrdinal":Value::Null,"worldTransform":Value::Null,
            "componentAssociationVerified":false,"geometryQualified":false,"poseVerified":false});
        if let Some(group) = body["componentIndexCandidate"]
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| groups.get_mut(i))
        {
            group["bodyCandidates"]
                .as_array_mut()
                .unwrap()
                .push(candidate);
        } else {
            unresolved.push(candidate);
        }
    }
    Ok(
        json!({"schema":"tokito.altium-candidate-associations.v1", "components":groups,
        "unresolvedComponentCandidates":unresolved,"modelRecords":models.len(),
        "uniqueModelIds":ids.len(),"bodyRecords":bodies.len(),
        "assemblyAdmitted":false,"exportsProduced":[],
        "diagnostics":["body_component_index_semantics_unverified","model_pose_composition_unverified",
            "step_surface_trim_completeness_unverified","source_to_photographed_variant_unverified"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stream(path: &str, records: Value) -> Value {
        json!({"path":path,"framedRecordInspection":{"records":records}})
    }
    #[test]
    fn ambiguous_ids_and_out_of_range_components_are_retained() {
        let streams = vec![
            stream(
                "/Components6/Data",
                json!([{"ordinal":0,"properties":{"UNIQUEID":"A"}}]),
            ),
            stream(
                "/Models/Data",
                json!([
                {"ordinal":0,"properties":{"ID":"M","ROTZ":"0"}},
                {"ordinal":1,"properties":{"ID":"M","ROTZ":"90"}}]),
            ),
            stream(
                "/ComponentBodies6/Data",
                json!([
                {"ordinal":0,"componentIndexCandidate":0,"properties":{"MODELID":"M"}},
                {"ordinal":1,"componentIndexCandidate":99,"properties":{"MODELID":"unknown"}}]),
            ),
        ];
        let report = inspect(&streams).unwrap();
        let body = &report["components"][0]["bodyCandidates"][0];
        assert_eq!(body["modelCandidates"].as_array().unwrap().len(), 2);
        assert_eq!(body["modelPlacementPropertiesAgree"], false);
        assert_eq!(body["embeddedByteIdentityAgrees"], false);
        assert!(body["selectedModelOrdinal"].is_null());
        assert_eq!(
            report["unresolvedComponentCandidates"][0]["explicitModelIdResolved"],
            false
        );
        assert_eq!(report["assemblyAdmitted"], false);
    }
    #[test]
    fn unavailable_record_stream_is_an_explicit_error() {
        assert!(inspect(&[])
            .unwrap_err()
            .contains("association_missing_records"));
    }
    #[test]
    fn a_candidate_without_digest_cannot_agree_with_a_hashed_candidate() {
        let streams = vec![
            stream("/Components6/Data", json!([{"ordinal":0,"properties":{}}])),
            stream(
                "/Models/Data",
                json!([
                {"ordinal":0,"properties":{"ID":"M"}},
                {"ordinal":1,"properties":{"ID":"M"}}]),
            ),
            stream(
                "/ComponentBodies6/Data",
                json!([
                {"ordinal":0,"componentIndexCandidate":0,"properties":{"MODELID":"M"}}]),
            ),
            json!({"path":"/Models/0","embeddedStep":{"completeZlibStream":true,"decompressedSha256":"abc"}}),
            json!({"path":"/Models/1","embeddedStep":{"completeZlibStream":true}}),
        ];
        let report = inspect(&streams).unwrap();
        assert_eq!(
            report["components"][0]["bodyCandidates"][0]["embeddedByteIdentityAgrees"],
            false
        );
    }
}
