//! Coverage comparison against a reference BOM; never joins different board revisions.
use crate::{
    EngineError,
    bom::{BomDocument, BomFormat},
    digital_twin::ValidatedTwin,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn class(reference: &str, value: &str, footprint: &str) -> &'static str {
    if footprint.starts_with("LED_") {
        "led"
    } else if reference.starts_with('R') {
        "resistor"
    } else if reference.starts_with('C') {
        "capacitor"
    } else if reference.starts_with('Q') {
        "transistor"
    } else if reference.starts_with('D') {
        "diode"
    } else if reference.starts_with("SW") {
        "switch"
    } else if footprint.starts_with("PinHeader_") {
        "pin_header"
    } else if value.starts_with("USB_") {
        "connector"
    } else if value.starts_with("ESP32-") {
        "rf_module"
    } else if value.starts_with("AMS1117") {
        "voltage_regulator"
    } else if value.starts_with("CP2102") {
        "interface_ic"
    } else {
        "unclassified_reference"
    }
}

fn compare(reference: &[(String, String)], components: &[Value]) -> Value {
    let mut reference_counts = BTreeMap::<String, usize>::new();
    let mut candidate_counts = BTreeMap::<String, usize>::new();
    let candidate_refs: BTreeSet<_> = components
        .iter()
        .filter_map(|c| c["reference"].as_str())
        .collect();
    let mut absent = Vec::new();
    let mut conflicts = Vec::new();
    for (name, class) in reference {
        *reference_counts.entry(class.clone()).or_default() += 1;
        if !candidate_refs.contains(name.as_str()) {
            absent.push(name);
        }
        if let Some(candidate) = components.iter().find(|c| c["reference"] == name.as_str()) {
            if candidate["componentClassClaim"] != class.as_str() {
                conflicts.push(json!({"reference":name,"referenceClass":class,
                    "candidateClassClaim":candidate["componentClassClaim"],"identityJoinVerified":false}));
            }
        }
    }
    for c in components.iter().filter(|c| c["modelAvailable"] == true) {
        if let Some(class) = c["componentClassClaim"].as_str() {
            *candidate_counts.entry(class.to_owned()).or_default() += 1;
        }
    }
    let coverage: Vec<_> = reference_counts.iter().map(|(class, &expected)| {
        let actual = candidate_counts.get(class).copied().unwrap_or(0);
        json!({"classClaim":class,"referenceBOMCount":expected,"renderedCandidateCount":actual,
            "referenceCountDeficit":expected.saturating_sub(actual),"physicalMissingPartsVerified":false})
    }).collect();
    json!({"schema":"tokito.reference-population-audit.v1","referenceBOMComponents":reference.len(),
        "candidateComponentRecords":components.len(),"renderedCandidateClassCounts":candidate_counts,
        "classCoverageComparison":coverage,"referenceLabelsAbsentFromCandidate":absent,
        "sameLabelClassConflicts":conflicts,"completePopulationVerified":false,
        "samePhysicalBoardVerified":false,"identityJoinsVerified":false,"physicalFitVerified":false,
        "limitations":["Reference BOM is not the photographed specimen BOM. Counts and equal labels do not prove identical parts or positions.",
        "BOM classification is a source-field heuristic; missing photo parts require reviewed source joins."]})
}

pub fn compare_esp32_reference(
    twin: &ValidatedTwin,
    bom: &BomDocument,
) -> Result<Value, EngineError> {
    compare_esp32_index(&twin.index_json(), bom)
}

/// Caller supplies the index minted by validated twin preparation, never user JSON.
pub(crate) fn compare_esp32_index(index: &Value, bom: &BomDocument) -> Result<Value, EngineError> {
    if bom.format() != BomFormat::Esp32Csv {
        return Err(EngineError::new(
            "population comparison requires esp32-csv reference BOM",
        ));
    }
    let mut reference = Vec::new();
    for entry in bom.entries() {
        for name in &entry.references {
            reference.push((
                name.clone(),
                class(
                    name,
                    entry.source_fields["value"].as_str().unwrap_or(""),
                    entry.source_fields["footprint"].as_str().unwrap_or(""),
                )
                .to_owned(),
            ));
        }
    }
    let mut report = compare(
        &reference,
        index["components"]
            .as_array()
            .ok_or_else(|| EngineError::new("missing validated component index"))?,
    );
    report["referenceBOMSha256"] = json!(bom.source_sha256());
    report["candidateSourceHashes"] = index["sourceSha256"].clone();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_classes_and_reused_refdes_do_not_create_identity_joins() {
        let report = compare(
            &[
                ("R1".into(), "resistor".into()),
                ("U2".into(), "voltage_regulator".into()),
            ],
            &[json!({"reference":"U2","componentClassClaim":"interface_ic","modelAvailable":true})],
        );
        assert_eq!(report["referenceLabelsAbsentFromCandidate"], json!(["R1"]));
        assert_eq!(report["sameLabelClassConflicts"][0]["reference"], "U2");
        assert_eq!(report["samePhysicalBoardVerified"], false);
        assert_eq!(
            report["classCoverageComparison"][0]["referenceCountDeficit"],
            1
        );
    }
    #[test]
    fn count_agreement_cannot_certify_population_and_bodyless_records_do_not_count() {
        let reference = vec![("C1".into(), "capacitor".into())];
        let r = compare(
            &reference,
            &[json!({"reference":"C1","componentClassClaim":"capacitor","modelAvailable":true})],
        );
        assert_eq!(r["completePopulationVerified"], false);
        let r = compare(
            &reference,
            &[json!({"reference":"C1","componentClassClaim":"capacitor","modelAvailable":false})],
        );
        assert_eq!(r["classCoverageComparison"][0]["referenceCountDeficit"], 1);
        assert_eq!(class("D2", "LED", "LED_0805"), "led");
    }
}
