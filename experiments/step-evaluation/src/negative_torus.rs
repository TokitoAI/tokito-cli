//! Explicit, non-admitting compatibility experiment; no source-file rewriting.
use oxideav_ifc::{ParsedInstance, StepFile, Value as StepValue};
use oxideav_step::schema::{attribute_order, entity_def, Entity};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

const MAX_DETAILS: usize = 128;
const MAX_TRAVERSAL_WORK: usize = 1_000_000;
const MAX_NEGATIVE_SURFACES: usize = 1024;
const MAX_AFFECTED_FACES: usize = 32_768;

struct WorkBudget {
    remaining: usize,
    used: usize,
}
impl WorkBudget {
    fn charge(&mut self, amount: usize) -> Result<(), String> {
        if amount > self.remaining {
            return Err("compatibility reference traversal work budget exceeded".into());
        }
        self.remaining -= amount;
        self.used += amount;
        Ok(())
    }
}

struct FingerprintWriter(Sha256);
impl std::fmt::Write for FingerprintWriter {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.0.update(text.as_bytes());
        Ok(())
    }
}
fn ast_fingerprint(step: &StepFile) -> String {
    let mut writer = FingerprintWriter(Sha256::new());
    writeln!(&mut writer, "header:{:?}", step.header).expect("hash writer infallible");
    for (id, record) in &step.instances {
        writeln!(&mut writer, "{id}:{record:?}").expect("hash writer infallible");
    }
    format!("{:x}", writer.0.finalize())
}
fn validate_complex_records(inst: &ParsedInstance) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for record in &inst.parts {
        if !seen.insert(&record.keyword) {
            return Err(format!("#{} duplicate complex entity declaration", inst.id));
        }
        let definition = entity_def(&record.keyword).ok_or("unknown complex declaration")?;
        if record.args.len() != definition.attrs.len() {
            return Err(format!("#{} malformed complex attribute count", inst.id));
        }
    }
    Ok(())
}
#[derive(Clone, Copy)]
struct Slot {
    part: Option<usize>,
    arg: usize,
}

fn slot(inst: &ParsedInstance, declaring: &str, attribute: &str) -> Result<Slot, String> {
    if inst.parts.is_empty() {
        let order = attribute_order(&inst.keyword).ok_or("unknown internal mapping")?;
        if inst.args.len() != order.len() {
            return Err(format!("#{} malformed internal attribute count", inst.id));
        }
        let arg = order
            .iter()
            .position(|(entity, name)| *entity == declaring && *name == attribute)
            .ok_or("missing named attribute")?;
        return Ok(Slot { part: None, arg });
    }
    let parts: Vec<_> = inst
        .parts
        .iter()
        .enumerate()
        .filter(|(_, p)| p.keyword == declaring)
        .collect();
    if parts.len() != 1 {
        return Err(format!(
            "#{} ambiguous complex attribute declaration {declaring}",
            inst.id
        ));
    }
    let (part, record) = parts[0];
    let def = entity_def(declaring).ok_or("unknown declaring entity")?;
    if record.args.len() != def.attrs.len() {
        return Err(format!("#{} malformed external attribute count", inst.id));
    }
    let arg = def
        .attrs
        .iter()
        .position(|name| *name == attribute)
        .ok_or("missing declared attribute")?;
    Ok(Slot {
        part: Some(part),
        arg,
    })
}

fn value(inst: &ParsedInstance, slot: Slot) -> &StepValue {
    match slot.part {
        Some(p) => &inst.parts[p].args[slot.arg],
        None => &inst.args[slot.arg],
    }
}
fn value_mut(inst: &mut ParsedInstance, slot: Slot) -> &mut StepValue {
    match slot.part {
        Some(p) => &mut inst.parts[p].args[slot.arg],
        None => &mut inst.args[slot.arg],
    }
}
fn radius(value: &StepValue) -> Result<f64, String> {
    let result = match value {
        StepValue::Typed { keyword, args } if keyword == "LENGTH_MEASURE" && args.len() == 1 => {
            args[0].as_number()
        }
        StepValue::Real(_) | StepValue::Integer(_) => value.as_number(),
        _ => None,
    }
    .ok_or("malformed radius value")?;
    if !result.is_finite() {
        return Err("nonfinite radius".into());
    }
    Ok(result)
}
fn replace_radius(value: &mut StepValue, positive: f64) {
    if let StepValue::Typed { args, .. } = value {
        args[0] = StepValue::Real(positive);
    } else {
        *value = StepValue::Real(positive);
    }
}

fn check_reachable_references(
    step: &StepFile,
    root: u64,
    budget: &mut WorkBudget,
) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        budget.charge(1)?;
        if !seen.insert(id) {
            continue;
        }
        if !step.instances.contains_key(&id) {
            return Err(format!("dangling topology reference #{id} from #{root}"));
        }
        let references = step.references_of(id);
        budget.charge(references.len())?;
        pending.extend(references);
    }
    Ok(())
}

/// Derive a separate AST with positive torus magnitude and opposite face sense.
/// Only direct elementary torus faces/PCURVE uses are accepted; wrappers reject.
pub fn normalized_copy(step: &StepFile, enabled: bool) -> Result<(StepFile, Value), String> {
    if !enabled {
        return Ok((step.clone(), json!({"enabled": false})));
    }
    let mut budget = WorkBudget {
        remaining: MAX_TRAVERSAL_WORK,
        used: 0,
    };
    let mut surfaces = BTreeMap::new();
    for inst in step.instances.values() {
        let entity = Entity { inst };
        if !entity.is_a("TOROIDAL_SURFACE") {
            continue;
        }
        let major_slot = slot(inst, "TOROIDAL_SURFACE", "major_radius")?;
        let major = radius(value(inst, major_slot)).map_err(|e| format!("#{} {e}", inst.id))?;
        if major >= 0.0 {
            continue;
        }
        if entity.is_a("DEGENERATE_TOROIDAL_SURFACE")
            || (!inst.parts.is_empty()
                && inst.parts.iter().any(|p| {
                    ![
                        "REPRESENTATION_ITEM",
                        "GEOMETRIC_REPRESENTATION_ITEM",
                        "SURFACE",
                        "ELEMENTARY_SURFACE",
                        "TOROIDAL_SURFACE",
                    ]
                    .contains(&p.keyword.as_str())
                }))
        {
            return Err(format!(
                "#{} unsupported negative torus subtype/mapping",
                inst.id
            ));
        }
        validate_complex_records(inst)?;
        let minor_slot = slot(inst, "TOROIDAL_SURFACE", "minor_radius")?;
        let minor = radius(value(inst, minor_slot))?;
        if minor <= 0.0 {
            return Err(format!("#{} unsupported nonpositive minor radius", inst.id));
        }
        let position = entity
            .reference("position")
            .ok_or("missing torus position")?;
        if !Entity::get(step, position).is_some_and(|e| e.is_a("AXIS2_PLACEMENT_3D")) {
            return Err(format!(
                "#{} missing/unsupported torus position #{position}",
                inst.id
            ));
        }
        check_reachable_references(step, inst.id, &mut budget)?;
        if surfaces.len() >= MAX_NEGATIVE_SURFACES {
            return Err("negative surface count cap exceeded".into());
        }
        surfaces.insert(inst.id, (major_slot, major, minor));
    }
    let mut faces = BTreeMap::new();
    let mut use_counts: BTreeMap<u64, usize> = surfaces.keys().map(|id| (*id, 0)).collect();
    for inst in step.instances.values() {
        let entity = Entity { inst };
        let targets: BTreeSet<_> = step
            .references_of(inst.id)
            .into_iter()
            .filter(|id| surfaces.contains_key(id))
            .collect();
        for target in targets {
            if step
                .references_of(inst.id)
                .iter()
                .filter(|id| **id == target)
                .count()
                != 1
            {
                return Err(format!(
                    "#{} ambiguous repeated negative torus reference #{target}",
                    inst.id
                ));
            }
            if entity.is_a("FACE_SURFACE") && entity.reference("face_geometry") == Some(target) {
                if !inst.parts.is_empty()
                    && inst.parts.iter().any(|p| {
                        ![
                            "REPRESENTATION_ITEM",
                            "TOPOLOGICAL_REPRESENTATION_ITEM",
                            "FACE",
                            "FACE_SURFACE",
                            "ADVANCED_FACE",
                        ]
                        .contains(&p.keyword.as_str())
                    })
                {
                    return Err(format!("#{} unsupported complex face mapping", inst.id));
                }
                validate_complex_records(inst)?;
                let sense_slot = slot(inst, "FACE_SURFACE", "same_sense")?;
                let sense = oxideav_step::schema::logical(value(inst, sense_slot))
                    .ok_or_else(|| format!("#{} unknown/malformed face same_sense", inst.id))?;
                check_reachable_references(step, inst.id, &mut budget)?;
                if faces.len() >= MAX_AFFECTED_FACES {
                    return Err("affected face count cap exceeded".into());
                }
                faces.insert(inst.id, (target, sense_slot, sense));
                *use_counts.get_mut(&target).expect("surface indexed") += 1;
            } else if entity.is_a("PCURVE") && entity.reference("basis_surface") == Some(target) {
                check_reachable_references(step, inst.id, &mut budget)?;
            } else {
                return Err(format!(
                    "#{} unsupported/ambiguous reference to negative torus #{target}",
                    inst.id
                ));
            }
        }
    }
    if let Some((&id, _)) = use_counts.iter().find(|(_, count)| **count == 0) {
        return Err(format!("#{id} negative torus has no direct face use"));
    }
    let mut derived = step.clone();
    let mut surface_details = Vec::new();
    let mut face_details = Vec::new();
    for (&id, &(slot, major, minor)) in &surfaces {
        replace_radius(
            value_mut(
                derived.instances.get_mut(&id).expect("cloned surface"),
                slot,
            ),
            major.abs(),
        );
        if surface_details.len() < MAX_DETAILS {
            surface_details.push(json!({"surfaceId":id,"originalMajorRadius":major,"derivedMajorRadius":major.abs(),"minorRadiusUnchanged":minor}));
        }
    }
    for (&id, &(surface, slot, original)) in &faces {
        *value_mut(derived.instances.get_mut(&id).expect("cloned face"), slot) =
            StepValue::Enum(if original { "F" } else { "T" }.into());
        if face_details.len() < MAX_DETAILS {
            face_details.push(json!({"faceId":id,"surfaceId":surface,"originalSameSense":original,"derivedSameSense":!original}));
        }
    }
    let derived_fingerprint = ast_fingerprint(&derived);
    Ok((
        derived,
        json!({"enabled":true,"policy":"negative_major_magnitude_and_reversed_direct_face_same_sense_experimental","sourceBytesRewritten":false,"astFingerprintRepresentation":"oxideav_ifc_0_0_3_sorted_step_ast_debug_v1_not_step_bytes","originalAstSha256":ast_fingerprint(step),"derivedAstSha256":derived_fingerprint,"sourceAstModified":false,"adjustedSurfaceCount":surfaces.len(),"adjustedFaceCount":faces.len(),"surfaceDetails":surface_details,"faceDetails":face_details,"surfaceDetailsTruncated":surfaces.len()>MAX_DETAILS,"faceDetailsTruncated":faces.len()>MAX_DETAILS,"maxDetailsPerKind":MAX_DETAILS,"referenceTraversalWorkSteps":budget.used,"maxReferenceTraversalWorkSteps":MAX_TRAVERSAL_WORK,"maxNegativeSurfaces":MAX_NEGATIVE_SURFACES,"maxAffectedFaces":MAX_AFFECTED_FACES,"geometryEquivalenceVerified":false,"faceOrientationEquivalenceVerified":false,"candidateLimitations":"Candidate toggles reader face same_sense for measurement; OCCT separates edge-loop sense from final face orientation. Normal/winding equivalence is unverified."}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideav_ifc::parse_step;
    fn parse(records: &str) -> StepFile {
        parse_step(format!("ISO-10303-21;HEADER;FILE_DESCRIPTION(('synthetic compatibility test'),'2;1');FILE_NAME('test','2026-10-10T00:00:00',('test'),('test'),'test','test','');FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;{records}ENDSEC;END-ISO-10303-21;").as_bytes()).unwrap()
    }
    const POSITION: &str = "#1=CARTESIAN_POINT('',(0.,0.,0.));#2=AXIS2_PLACEMENT_3D('',#1,$,$);";
    #[test]
    fn simple_mapping_and_no_flag_preserve_original() {
        let source = parse(&format!(
            "{POSITION}#3=TOROIDAL_SURFACE('',#2,-1.,2.);#4=ADVANCED_FACE('',(),#3,.T.);"
        ));
        let (unchanged, report) = normalized_copy(&source, false).unwrap();
        assert_eq!(unchanged, source);
        assert_eq!(report["enabled"], false);
        let (derived, report) = normalized_copy(&source, true).unwrap();
        assert_eq!(
            Entity::get(&source, 3).unwrap().number("major_radius"),
            Some(-1.)
        );
        assert_eq!(
            Entity::get(&derived, 3).unwrap().number("major_radius"),
            Some(1.)
        );
        assert_eq!(
            Entity::get(&derived, 4).unwrap().logical("same_sense"),
            Some(false)
        );
        assert_eq!(report["adjustedFaceCount"], 1);
    }
    #[test]
    fn complex_mapping_and_typed_radius_are_resolved_by_name() {
        let source = parse(&format!("{POSITION}#3=(ELEMENTARY_SURFACE(#2) GEOMETRIC_REPRESENTATION_ITEM() REPRESENTATION_ITEM('') SURFACE() TOROIDAL_SURFACE(LENGTH_MEASURE(-1.),2.));#4=(ADVANCED_FACE() FACE(()) FACE_SURFACE(#3,.F.) REPRESENTATION_ITEM('') TOPOLOGICAL_REPRESENTATION_ITEM());"));
        let (derived, _) = normalized_copy(&source, true).unwrap();
        assert_eq!(
            Entity::get(&derived, 3).unwrap().number("major_radius"),
            Some(1.)
        );
        assert_eq!(
            Entity::get(&derived, 4).unwrap().logical("same_sense"),
            Some(true)
        );
    }
    #[test]
    fn dangling_and_malformed_topology_reject_without_mutation() {
        for records in [
            format!(
                "{POSITION}#3=TOROIDAL_SURFACE('',#2,-1.,2.);#4=ADVANCED_FACE('',(#99),#3,.T.);"
            ),
            format!("{POSITION}#3=TOROIDAL_SURFACE('',#2,-1.,2.);#4=ADVANCED_FACE('',(),#3,.U.);"),
            format!("{POSITION}#3=TOROIDAL_SURFACE('',#2,-1.,2.);#4=OFFSET_SURFACE('',#3,1.,.F.);"),
            format!("{POSITION}#3=TOROIDAL_SURFACE('',#2,-1.,2.);"),
            format!(
                "{POSITION}#3=TOROIDAL_SURFACE('',#2,-1.,'bad');#4=ADVANCED_FACE('',(),#3,.T.);"
            ),
        ] {
            let source = parse(&records);
            let original = source.clone();
            assert!(normalized_copy(&source, true).is_err());
            assert_eq!(source, original);
        }
    }
    #[test]
    fn shared_reference_budget_exhausts_across_calls() {
        let source = parse(POSITION);
        let mut budget = WorkBudget {
            remaining: 3,
            used: 0,
        };
        check_reachable_references(&source, 2, &mut budget).unwrap();
        assert_eq!(budget.used, 3);
        let error = check_reachable_references(&source, 1, &mut budget).unwrap_err();
        assert!(error.contains("work budget exceeded"));
        assert_eq!(budget.used, 3);
    }
}
