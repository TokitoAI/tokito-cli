//! Explicit source representation ownership. No assembly/frame inference.
use oxideav_ifc::StepFile;
use oxideav_step::schema::Entity;
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub fn inspect(step: &StepFile, representation: u64, face: u64) -> Result<Value, String> {
    let inst = step
        .get(representation)
        .ok_or("missing_selected_representation")?;
    crate::unit_preflight::shape(inst)?;
    let rep = Entity { inst };
    if !rep.is_a("SHAPE_REPRESENTATION") {
        return Err("selected_entity_not_shape_representation".into());
    }
    let items = rep.list("items").ok_or("missing_representation_items")?;
    if items.is_empty() || items.len() > 512 {
        return Err("representation_item_limit".into());
    }
    let mut seen = BTreeSet::new();
    let mut owners = Vec::new();
    let mut topology = crate::face_coverage::Audit::new(step);
    for value in items {
        let id = value.as_reference().ok_or("invalid_representation_item")?;
        if !seen.insert(id) {
            return Err("duplicate_representation_item".into());
        }
        let inst = step.get(id).ok_or("dangling_representation_item")?;
        crate::unit_preflight::shape(inst)?;
        let entity = Entity { inst };
        if entity.is_a("AXIS2_PLACEMENT_3D") {
            continue;
        }
        if topology.contains_face(id, face)? {
            owners.push(id);
        }
    }
    if owners.len() != 1 {
        return Err("face_requires_one_owner_in_selected_representation".into());
    }
    let context = rep
        .reference("context_of_items")
        .ok_or("missing_representation_context")?;
    let units = crate::unit_preflight::selected_context(step, context)?;
    if units["coordinateSpaceDimension"] != 3 {
        return Err("face_requires_3d_representation_context".into());
    }
    Ok(
        json!({"representationId":representation,"representationItemId":owners[0],"faceId":face,
        "selectedRepresentationContainsFace":true,"selectedRepresentationLengthUnits":units,
        "allSourceRepresentationOwnersEnumerated":false,"sourceToModelFrameVerified":false,
        "assemblyPoseVerified":false,"geometryQualified":false,"admitted":false,
        "scope":"explicit selected representation topology membership and context units only; other owners and assembly relationships not inferred"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(extra: &str) -> StepFile {
        oxideav_ifc::parse_step(format!("ISO-10303-21;HEADER;FILE_DESCRIPTION((''),'2;1');FILE_NAME('','',(''),(''),'','','');FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;#1=MANIFOLD_SOLID_BREP('',#2);#2=CLOSED_SHELL('',(#3));#3=ADVANCED_FACE('',(),#4,.T.);#10=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#1),#11);#11=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT((#12)) REPRESENTATION_CONTEXT('',''));#12=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));{extra}ENDSEC;END-ISO-10303-21;").as_bytes()).unwrap()
    }
    #[test]
    fn explicit_owner_resolves_units_without_frame_claim() {
        let report = inspect(&fixture(""), 10, 3).unwrap();
        assert_eq!(report["representationItemId"], 1);
        assert_eq!(
            report["selectedRepresentationLengthUnits"]["metresPerUnit"],
            0.001
        );
        assert_eq!(report["sourceToModelFrameVerified"], false);
        assert_eq!(report["admitted"], false);
    }
    #[test]
    fn wrong_owner_and_repeated_items_refuse() {
        assert!(inspect(&fixture(""), 1, 3).is_err());
        assert!(inspect(&fixture(""), 10, 99).is_err());
        assert!(inspect(&fixture("#20=SHAPE_REPRESENTATION('',(#1,#1),#11);"), 20, 3).is_err());
        assert!(inspect(
            &fixture("#20=SHAPE_REPRESENTATION('',(#21),#11);#21=MAPPED_ITEM('',#22,#23);"),
            20,
            3
        )
        .is_err());
        assert!(inspect(&fixture("#20=SHAPE_REPRESENTATION('',(#1),#99);"), 20, 3).is_err());
    }
}
