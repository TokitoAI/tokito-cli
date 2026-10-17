//! Read only the declared length uncertainty on an explicitly selected physical
//! representation context. This value is reported as source metadata and is
//! never used to qualify, repair, or admit geometry.
use oxideav_ifc::{StepFile, Value};
use oxideav_step::schema::Entity;
use serde_json::{json, Value as Json};

pub(crate) fn inspect_context(step: &StepFile, context_id: u64) -> Result<Json, String> {
    let context_inst = step
        .get(context_id)
        .ok_or("missing_uncertainty_representation_context")?;
    crate::unit_preflight::shape(context_inst)?;
    let context = Entity { inst: context_inst };
    if !context.is_a("GEOMETRIC_REPRESENTATION_CONTEXT") {
        return Err("uncertainty_context_is_not_geometric".into());
    }
    if !context.is_a("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT") {
        return Err("selected_context_has_no_global_uncertainty_assignment".into());
    }
    let uncertainty = context
        .list("uncertainty")
        .ok_or("missing_global_uncertainty_list")?;
    if uncertainty.len() != 1 {
        return Err("context_requires_exactly_one_global_length_uncertainty".into());
    }
    let measure_id = uncertainty[0]
        .as_reference()
        .ok_or("global_uncertainty_is_not_reference")?;
    let measure_inst = step
        .get(measure_id)
        .ok_or("dangling_global_uncertainty_measure")?;
    crate::unit_preflight::shape(measure_inst)?;
    let measure = Entity { inst: measure_inst };
    if !measure.is_a("UNCERTAINTY_MEASURE_WITH_UNIT") {
        return Err("global_uncertainty_has_wrong_entity_type".into());
    }
    let value = match measure.attr("value_component") {
        Some(Value::Typed { keyword, args }) if keyword == "LENGTH_MEASURE" && args.len() == 1 => {
            args[0]
                .as_number()
                .filter(|value| value.is_finite() && *value > 0. && *value <= 1e12)
                .ok_or("invalid_global_length_uncertainty_value")?
        }
        _ => return Err("global_uncertainty_requires_LENGTH_MEASURE".into()),
    };
    let unit_id = measure
        .reference("unit_component")
        .ok_or("global_uncertainty_missing_length_unit")?;
    let uncertainty_metres_per_unit =
        crate::unit_preflight::length_unit_metres_per_unit(step, unit_id)?;
    let context_units = crate::unit_preflight::selected_context(step, context_id)?;
    let context_metres_per_unit = context_units["metresPerUnit"]
        .as_f64()
        .filter(|scale| scale.is_finite() && *scale > 0.)
        .ok_or("uncertainty_context_length_units_unresolved")?;
    let value_metres = value * uncertainty_metres_per_unit;
    let value_source_units = value_metres / context_metres_per_unit;
    if !value_metres.is_finite()
        || value_metres <= 0.
        || !value_source_units.is_finite()
        || value_source_units <= 0.
        || value_source_units > 1e12
    {
        return Err("global_length_uncertainty_out_of_bounds".into());
    }
    Ok(json!({
        "schema":"tokito.step-source-geometric-uncertainty.v1",
        "representationContextId":context_id,
        "uncertaintyMeasureId":measure_id,
        "value":value,
        "unitId":unit_id,
        "uncertaintyUnitMetresPerUnit":uncertainty_metres_per_unit,
        "valueMetres":value_metres,
        "contextMetresPerUnit":context_metres_per_unit,
        "valueInContextUnits":value_source_units,
        "usedAsAdmissionTolerance":false,
        "scope":"declared context uncertainty metadata only; not a geometry acceptance tolerance, fit guarantee, or admission decision"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(context: &str, measures: &str, units: &str) -> StepFile {
        oxideav_ifc::parse_step(
            format!(
                "ISO-10303-21;HEADER;FILE_DESCRIPTION((''),'2;1');FILE_NAME('','',(''),(''),'','','');FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;#10=SHAPE_REPRESENTATION('',(),#11);{context}{measures}{units}ENDSEC;END-ISO-10303-21;"
            )
            .as_bytes(),
        )
        .unwrap()
    }

    #[test]
    fn reports_context_uncertainty_after_unit_conversion_without_admission() {
        let input = step(
            "#11=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#13)) GLOBAL_UNIT_ASSIGNED_CONTEXT((#12)) REPRESENTATION_CONTEXT('',''));",
            "#13=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-5),#12,'accuracy','NONE');",
            "#12=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));",
        );
        let report = inspect_context(&input, 11).unwrap();
        assert_eq!(report["valueInContextUnits"], 1e-5);
        assert_eq!(report["valueMetres"], 1e-8);
        assert_eq!(report["usedAsAdmissionTolerance"], false);
    }

    #[test]
    fn converts_uncertainty_unit_independently_of_context_unit() {
        let input = step(
            "#11=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#13)) GLOBAL_UNIT_ASSIGNED_CONTEXT((#12)) REPRESENTATION_CONTEXT('',''));",
            "#13=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-8),#14,'accuracy','NONE');",
            "#12=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));#14=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT($,.METRE.));",
        );
        let report = inspect_context(&input, 11).unwrap();
        assert_eq!(report["valueInContextUnits"], 1e-5);
        assert_eq!(report["valueMetres"], 1e-8);
    }

    #[test]
    fn refuses_missing_ambiguous_or_non_length_uncertainty() {
        let missing = step(
            "#11=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT((#12)) REPRESENTATION_CONTEXT('',''));",
            "",
            "#12=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));",
        );
        assert!(inspect_context(&missing, 11).is_err());

        let ambiguous = step(
            "#11=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#13,#15)) GLOBAL_UNIT_ASSIGNED_CONTEXT((#12)) REPRESENTATION_CONTEXT('',''));",
            "#13=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-5),#12,'accuracy','NONE');#15=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(2.E-5),#12,'accuracy','NONE');",
            "#12=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));",
        );
        assert!(inspect_context(&ambiguous, 11).is_err());

        let angle = step(
            "#11=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#13)) GLOBAL_UNIT_ASSIGNED_CONTEXT((#12)) REPRESENTATION_CONTEXT('',''));",
            "#13=UNCERTAINTY_MEASURE_WITH_UNIT(PLANE_ANGLE_MEASURE(1.E-5),#12,'accuracy','NONE');",
            "#12=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));",
        );
        assert!(inspect_context(&angle, 11).is_err());
    }
}
