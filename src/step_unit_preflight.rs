//! Experimental, strict unit-graph audit. No geometry or production admission.
use oxideav_ifc::{ParsedInstance, StepFile, Value};
use oxideav_step::schema::{Entity, attribute_order, entity_def};
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const MAX_WORK: usize = 100_000;
const MAX_DEPTH: usize = 32;
const MAX_DETAILS: usize = 128;
const MAX_CONTEXTS: usize = 10_000;
struct Audit<'a> {
    step: &'a StepFile,
    work: usize,
}
impl<'a> Audit<'a> {
    fn tick(&mut self) -> Result<(), String> {
        self.work += 1;
        if self.work > MAX_WORK {
            Err("unit_graph_work_budget_exceeded".into())
        } else {
            Ok(())
        }
    }
    fn entity(&mut self, id: u64) -> Result<Entity<'a>, String> {
        self.tick()?;
        let inst = self
            .step
            .get(id)
            .ok_or_else(|| format!("dangling_entity:{id}"))?;
        shape(inst)?;
        Ok(Entity { inst })
    }
    fn dimensions(&mut self, unit: Entity<'a>, derived_allowed: bool) -> Result<(), String> {
        match unit.attr_of("NAMED_UNIT", "dimensions") {
            Some(Value::Derived) if derived_allowed => Ok(()),
            Some(Value::Reference(id)) => {
                let dimensions = self.entity(*id)?;
                if !dimensions.is_a("DIMENSIONAL_EXPONENTS") {
                    return Err("unit_dimensions_wrong_entity".into());
                }
                let names = [
                    "length_exponent",
                    "mass_exponent",
                    "time_exponent",
                    "electric_current_exponent",
                    "thermodynamic_temperature_exponent",
                    "amount_of_substance_exponent",
                    "luminous_intensity_exponent",
                ];
                for (i, name) in names.iter().enumerate() {
                    if dimensions.attr(name).and_then(Value::as_number)
                        != Some(if i == 0 { 1. } else { 0. })
                    {
                        return Err("unit_dimensions_not_length".into());
                    }
                }
                Ok(())
            }
            _ => Err("missing_explicit_length_dimensions".into()),
        }
    }
    fn length(
        &mut self,
        id: u64,
        depth: usize,
        visiting: &mut BTreeSet<u64>,
        chain: &mut Vec<u64>,
    ) -> Result<f64, String> {
        if depth > MAX_DEPTH {
            return Err("unit_graph_depth_budget_exceeded".into());
        }
        if !visiting.insert(id) {
            return Err("unit_graph_cycle".into());
        }
        chain.push(id);
        let unit = self.entity(id)?;
        if unit.is_a("PLANE_ANGLE_UNIT") || unit.is_a("SOLID_ANGLE_UNIT") {
            return Err("angle_unit_used_as_length".into());
        }
        let result = if unit.is_a("SI_UNIT") {
            if unit.is_a("CONVERSION_BASED_UNIT") {
                return Err("ambiguous_unit_definition".into());
            }
            if unit.attr_of("SI_UNIT", "name").and_then(Value::as_enum) != Some("METRE") {
                return Err("SI_name_not_METRE".into());
            }
            self.dimensions(unit, true)?;
            match unit.attr_of("SI_UNIT", "prefix") {
                Some(Value::Unset) => 1.,
                Some(Value::Enum(prefix)) => prefix_scale(prefix).ok_or("unsupported_SI_prefix")?,
                _ => return Err("invalid_SI_prefix".into()),
            }
        } else if unit.is_a("CONVERSION_BASED_UNIT") {
            if !unit.is_a("LENGTH_UNIT") {
                return Err("conversion_unit_missing_LENGTH_UNIT".into());
            }
            self.dimensions(unit, false)?;
            let measure_id = unit
                .reference("conversion_factor")
                .ok_or("missing_conversion_factor")?;
            if !visiting.insert(measure_id) {
                return Err("unit_graph_cycle".into());
            }
            chain.push(measure_id);
            let measure = self.entity(measure_id)?;
            if !measure.is_a("MEASURE_WITH_UNIT") || measure.is_a("PLANE_ANGLE_MEASURE_WITH_UNIT") {
                return Err("conversion_factor_wrong_quantity".into());
            }
            let factor = match measure.attr("value_component") {
                Some(Value::Typed { keyword, args })
                    if keyword == "LENGTH_MEASURE" && args.len() == 1 =>
                {
                    args[0]
                        .as_number()
                        .filter(|v| v.is_finite() && *v > 0.)
                        .ok_or("invalid_length_factor")?
                }
                _ => return Err("conversion_factor_requires_LENGTH_MEASURE".into()),
            };
            let base = measure
                .reference("unit_component")
                .ok_or("missing_conversion_base")?;
            let scale = factor * self.length(base, depth + 1, visiting, chain)?;
            visiting.remove(&measure_id);
            scale
        } else {
            return Err("unsupported_length_unit_entity".into());
        };
        visiting.remove(&id);
        if !result.is_finite() || result <= 0. {
            return Err("nonpositive_or_nonfinite_unit_scale".into());
        }
        Ok(result)
    }
    fn context(&mut self, id: u64) -> Result<Json, String> {
        let ctx = self.entity(id)?;
        if !ctx.is_a("GEOMETRIC_REPRESENTATION_CONTEXT") {
            return Err("context_is_not_geometric".into());
        }
        let dimensions = ctx
            .attr("coordinate_space_dimension")
            .and_then(Value::as_integer)
            .filter(|n| *n == 2 || *n == 3)
            .ok_or("unsupported_coordinate_dimension")?;
        let units = ctx.list("units").ok_or("missing_assigned_units")?;
        let mut seen = BTreeSet::new();
        let mut lengths = Vec::new();
        let mut other = Vec::new();
        for unit in units {
            self.tick()?;
            let unit_id = unit.as_reference().ok_or("unit_entry_is_not_reference")?;
            if !seen.insert(unit_id) {
                return Err("duplicate_context_unit_entry".into());
            }
            let e = self.entity(unit_id)?;
            if e.is_a("LENGTH_UNIT")
                || e.attr_of("SI_UNIT", "name").and_then(Value::as_enum) == Some("METRE")
            {
                lengths.push(unit_id);
            } else if e.is_a("PLANE_ANGLE_UNIT")
                || e.is_a("SOLID_ANGLE_UNIT")
                || matches!(
                    e.attr_of("SI_UNIT", "name").and_then(Value::as_enum),
                    Some("RADIAN" | "STERADIAN")
                )
            {
                other.push(unit_id);
            } else {
                return Err("unsupported_context_unit_quantity".into());
            }
        }
        if lengths.len() != 1 {
            return Err("context_requires_exactly_one_length_unit".into());
        }
        let mut chain = Vec::new();
        let scale = self.length(lengths[0], 0, &mut BTreeSet::new(), &mut chain)?;
        Ok(
            json!({"contextId":id,"coordinateSpaceDimension":dimensions,"lengthUnitId":lengths[0],"unitMeasureChainIds":chain,"metresPerUnit":scale,"outOfScopeAngleUnitIds":other,"resolved":true}),
        )
    }
}
/// Resolve only the explicitly selected physical context; no assembly inference.
// Also exercised by the standalone STEP experiment's extended unit-context
// probes; the production adapter consumes the aggregate inspection report.
#[allow(dead_code)]
pub(crate) fn selected_context(step: &StepFile, id: u64) -> Result<Json, String> {
    Audit { step, work: 0 }.context(id)
}

/// Resolve one explicitly referenced length-unit entity without inferring an
/// active representation or applying the result to geometry admission.
#[allow(dead_code)]
pub(crate) fn length_unit_metres_per_unit(step: &StepFile, id: u64) -> Result<f64, String> {
    Audit { step, work: 0 }.length(id, 0, &mut BTreeSet::new(), &mut Vec::new())
}

pub(crate) fn shape(inst: &ParsedInstance) -> Result<(), String> {
    if inst.parts.is_empty() {
        if attribute_order(&inst.keyword).is_none_or(|order| order.len() != inst.args.len()) {
            return Err(format!(
                "unsupported_entity_or_wrong_attribute_count:{}",
                inst.id
            ));
        }
    } else {
        let mut seen = BTreeSet::new();
        for part in &inst.parts {
            // The pinned reader's schema omits this attribute-free marker.
            // Its documented complex partial is explicitly recognized here;
            // no guessed units or new production geometry support follows.
            let arity_valid = if part.keyword == "PARAMETRIC_REPRESENTATION_CONTEXT" {
                part.args.is_empty()
            } else {
                entity_def(&part.keyword).is_some_and(|def| def.attrs.len() == part.args.len())
            };
            if !seen.insert(&part.keyword) || !arity_valid {
                return Err(format!(
                    "duplicate_partial_or_wrong_attribute_count:{}",
                    inst.id
                ));
            }
        }
    }
    Ok(())
}
fn prefix_scale(prefix: &str) -> Option<f64> {
    let exponent = match prefix {
        "EXA" => 18,
        "PETA" => 15,
        "TERA" => 12,
        "GIGA" => 9,
        "MEGA" => 6,
        "KILO" => 3,
        "HECTO" => 2,
        "DECA" => 1,
        "DECI" => -1,
        "CENTI" => -2,
        "MILLI" => -3,
        "MICRO" => -6,
        "NANO" => -9,
        "PICO" => -12,
        "FEMTO" => -15,
        "ATTO" => -18,
        _ => return None,
    };
    Some(10f64.powi(exponent))
}

pub fn inspect(step: &StepFile) -> Json {
    let mut audit = Audit { step, work: 0 };
    let mut records = Vec::new();
    let mut failed = 0usize;
    let mut total = 0usize;
    let representations_total = step
        .instances
        .values()
        .filter(|inst| (Entity { inst }).is_a("REPRESENTATION"))
        .count();
    let mut resolved_count = 0usize;
    let mut parametric_count = 0usize;
    let mut parametric_digest = Sha256::new();
    let mut digest = Sha256::new();
    let mut distinct_length_scale_bits = BTreeSet::new();
    let mut analysis_complete = true;
    let mut context_ids = BTreeSet::new();
    for inst in step.instances.values() {
        if !(Entity { inst }).is_a("REPRESENTATION") {
            continue;
        }
        total += 1;
        let result: Result<Json, String> = (|| {
            let rep = audit.entity(inst.id)?;
            let context_id = rep
                .reference("context_of_items")
                .ok_or("representation_missing_context")?;
            let context = audit.entity(context_id)?;
            if context.is_a("PARAMETRIC_REPRESENTATION_CONTEXT") {
                if !rep.is_a("DEFINITIONAL_REPRESENTATION")
                    || !context.is_a("GEOMETRIC_REPRESENTATION_CONTEXT")
                    || context
                        .attr("coordinate_space_dimension")
                        .and_then(Value::as_integer)
                        != Some(2)
                    || context.is_a("GLOBAL_UNIT_ASSIGNED_CONTEXT")
                {
                    return Err("unsupported_parametric_context_profile".into());
                }
                parametric_count += 1;
                for value in [inst.id, context_id] {
                    parametric_digest.update(value.to_le_bytes());
                }
                return Ok(
                    json!({"representationId":inst.id,"contextId":context_id,"outOfScope":true,"reason":"surface_parameter_space_not_physical_length","coordinateSpaceDimension":2,"physicalMetresPerParameter":null,"surfaceMappingVerified":false}),
                );
            }
            if !context.is_a("GEOMETRIC_REPRESENTATION_CONTEXT") {
                return Ok(
                    json!({"representationId":inst.id,"contextId":context_id,"outOfScope":true,"reason":"non_geometric_context"}),
                );
            }
            context_ids.insert(context_id);
            if context_ids.len() > MAX_CONTEXTS {
                return Err("geometric_context_count_budget_exceeded".into());
            }
            let mut resolved = audit.context(context_id)?;
            resolved["representationId"] = json!(inst.id);
            Ok(resolved)
        })();
        let record = match result {
            Ok(record) => record,
            Err(diagnostic) => {
                failed += 1;
                json!({"representationId":inst.id,"contextId":(Entity{inst}).reference("context_of_items"),"resolved":false,"diagnostics":[diagnostic]})
            }
        };
        if record["resolved"] == true {
            // Every resolved geometric representation, ordered by the STEP AST
            // BTreeMap's ascending representation ID. No delimiters/header:
            // four consecutive little-endian u64 values per 32-byte row:
            // representation ID, context ID, unit ID, f64 scale IEEE-754 bits.
            for value in [
                inst.id,
                record["contextId"].as_u64().unwrap(),
                record["lengthUnitId"].as_u64().unwrap(),
                record["metresPerUnit"].as_f64().unwrap().to_bits(),
            ] {
                digest.update(value.to_le_bytes());
            }
            resolved_count += 1;
            distinct_length_scale_bits.insert(
                record["metresPerUnit"]
                    .as_f64()
                    .expect("resolved length-unit record includes its scale")
                    .to_bits(),
            );
        }
        if records.len() < MAX_DETAILS {
            records.push(record);
        }
        if audit.work > MAX_WORK || context_ids.len() > MAX_CONTEXTS {
            analysis_complete = false;
            break;
        }
    }
    json!({"schema":"tokito.step-unit-preflight.v1","scope":"declared physical representation-context length units; explicit 2D definitional parameter spaces classified separately; no guessed active assembly roots","representationsTotal":representations_total,"representationsInspected":total,"analysisComplete":analysis_complete,"resolvedGeometricRepresentations":resolved_count,"distinctMetresPerUnit":distinct_length_scale_bits.iter().map(|bits| f64::from_bits(*bits)).collect::<Vec<_>>(),"resolvedContextRowsSha256":format!("{:x}",digest.finalize()),"resolvedContextRowsDigestFraming":"ascending representationId; each row is 32 bytes: LE u64 representationId, LE u64 contextId, LE u64 lengthUnitId, LE u64 IEEE754 f64 metresPerUnit bits; no header or separators; successful geometric rows only","parametricRepresentationsClassified":parametric_count,"parametricRepresentationRowsSha256":format!("{:x}",parametric_digest.finalize()),"parametricDigestFraming":"ascending representation ID, little-endian u64 representationId then contextId; no header","parametricSurfaceMappingVerified":false,"geometricContextCount":context_ids.len(),"failedRepresentations":failed,"lengthUnitsResolved":analysis_complete&&failed==0&&!context_ids.is_empty()&&audit.work<=MAX_WORK,"records":records,"detailsTruncated":total>MAX_DETAILS,"workSteps":audit.work,"maxWorkSteps":MAX_WORK,"maxConversionDepth":MAX_DEPTH,"maxGeometricContexts":MAX_CONTEXTS,"admitted":false,"geometryQualified":false,"assemblyPoseVerified":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file(data: &str) -> StepFile {
        oxideav_ifc::parse_step(format!("ISO-10303-21;HEADER;FILE_DESCRIPTION((''),'2;1');FILE_NAME('','',(''),(''),'','','');FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));ENDSEC;DATA;{data}ENDSEC;END-ISO-10303-21;").as_bytes()).unwrap()
    }
    fn context(unit: &str) -> String {
        format!(
            "#100=SHAPE_REPRESENTATION('',(),#101);#101=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT(({unit})) REPRESENTATION_CONTEXT('','')); "
        )
    }
    fn resolved(data: &str) -> Json {
        inspect(&file(data))
    }
    #[test]
    fn si_units_ignore_unassigned_decoy_and_complex_part_order() {
        for (prefix, scale) in [(".MILLI.", 0.001), (".CENTI.", 0.01), ("$", 1.)] {
            let r = resolved(
                &(context("#1")
                    + &format!(
                        "#1=(SI_UNIT({prefix},.METRE.) LENGTH_UNIT() NAMED_UNIT(*));#2=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.KILO.,.METRE.));"
                    )),
            );
            assert_eq!(r["lengthUnitsResolved"], true);
            assert_eq!(r["records"][0]["metresPerUnit"], scale);
        }
    }
    fn inch() -> String {
        "#1=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));#2=DIMENSIONAL_EXPONENTS(1.,0.,0.,0.,0.,0.,0.);#3=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#1);#4=(CONVERSION_BASED_UNIT('INCH',#3) LENGTH_UNIT() NAMED_UNIT(#2));".into()
    }
    #[test]
    fn nested_conversion_graph_resolves_inch_without_name_inference() {
        let r = resolved(
            &(context("#6")
                + &inch()
                + "#5=MEASURE_WITH_UNIT(LENGTH_MEASURE(2.),#4);#6=(NAMED_UNIT(#2) LENGTH_UNIT() CONVERSION_BASED_UNIT('arbitrary',#5));"),
        );
        assert_eq!(r["lengthUnitsResolved"], true);
        assert!((r["records"][0]["metresPerUnit"].as_f64().unwrap() - 0.0508).abs() < 1e-15);
        assert_eq!(
            r["records"][0]["unitMeasureChainIds"],
            json!([6, 5, 4, 3, 1])
        );
    }
    #[test]
    fn ambiguous_missing_duplicates_and_bad_dimensions_refuse() {
        for data in [
            context("#1,#1") + &inch(),
            context("#1,#4") + &inch(),
            context("") + &inch(),
            context("#1")
                + &inch()
                    .replace("1.,0.,0.,0.,0.,0.,0.", "0.,0.,0.,0.,0.,0.,0.")
                    .replace("SI_UNIT(.MILLI.,.METRE.)", "SI_UNIT(.MILLI.,.RADIAN.)"),
            context("#4") + &inch().replace("1.,0.,0.,0.,0.,0.,0.", "0.,0.,0.,0.,0.,0.,0."),
            context("#1").replace("CONTEXT(3)", "CONTEXT(4)") + &inch(),
        ] {
            assert_eq!(resolved(&data)["lengthUnitsResolved"], false, "{data}");
        }
    }
    #[test]
    fn wrong_quantity_invalid_factors_and_cycles_refuse() {
        for data in [
            inch().replace("LENGTH_MEASURE(25.4)", "PLANE_ANGLE_MEASURE(25.4)"),
            inch().replace("LENGTH_MEASURE(25.4)", "LENGTH_MEASURE(0.)"),
            inch().replace("LENGTH_MEASURE(25.4)", "LENGTH_MEASURE(-1.)"),
            inch().replace("SI_UNIT(.MILLI.,.METRE.)", "SI_UNIT($,.RADIAN.)"),
            inch().replace("LENGTH_MEASURE(25.4),#1", "LENGTH_MEASURE(25.4),#4"),
        ] {
            assert_eq!(
                resolved(&(context("#4") + &data))["lengthUnitsResolved"],
                false,
                "{data}"
            );
        }
    }
    #[test]
    fn simple_si_entity_and_attribute_count_validation() {
        assert_eq!(
            resolved(&(context("#1") + "#1=SI_UNIT(*,.MILLI.,.METRE.);"))["lengthUnitsResolved"],
            true
        );
        assert_eq!(
            resolved(&(context("#1") + "#1=SI_UNIT(.MILLI.,.METRE.);"))["lengthUnitsResolved"],
            false
        );
    }
    #[test]
    fn different_context_units_are_retained_separately() {
        let data = context("#1")
            + "#1=SI_UNIT(*,.MILLI.,.METRE.);#2=SI_UNIT(*,.CENTI.,.METRE.);#200=ADVANCED_BREP_SHAPE_REPRESENTATION('',(),#201);#201=(GEOMETRIC_REPRESENTATION_CONTEXT(2) GLOBAL_UNIT_ASSIGNED_CONTEXT((#2)) REPRESENTATION_CONTEXT('',''));";
        let report = resolved(&data);
        assert_eq!(report["lengthUnitsResolved"], true);
        assert_eq!(report["geometricContextCount"], 2);
        assert_eq!(report["records"][0]["metresPerUnit"], 0.001);
        assert_eq!(report["records"][1]["metresPerUnit"], 0.01);
        assert_eq!(report["distinctMetresPerUnit"], json!([0.001, 0.01]));
    }
    #[test]
    fn work_and_conversion_depth_budgets_refuse() {
        let step = file(&(context("#4") + &inch()));
        let mut audit = Audit {
            step: &step,
            work: MAX_WORK,
        };
        assert!(audit.context(101).is_err());
        let mut audit = Audit {
            step: &step,
            work: 0,
        };
        assert!(
            audit
                .length(4, MAX_DEPTH + 1, &mut BTreeSet::new(), &mut Vec::new())
                .is_err()
        );
    }
    #[test]
    fn digest_covers_rows_beyond_details_cap() {
        let mut data = String::from("#1=SI_UNIT(*,.MILLI.,.METRE.);#2=SI_UNIT(*,.CENTI.,.METRE.);");
        for n in 0..130 {
            let rep = 100 + n * 2;
            let ctx = rep + 1;
            data.push_str(&format!("#{rep}=SHAPE_REPRESENTATION('',(),#{ctx});#{ctx}=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT((#1)) REPRESENTATION_CONTEXT('',''));"));
        }
        let original = resolved(&data);
        let last_context = 100 + 129 * 2 + 1;
        let changed=resolved(&data.replace(&format!("#{last_context}=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT((#1))"),&format!("#{last_context}=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT((#2))")));
        assert_eq!(original["analysisComplete"], true);
        assert_eq!(original["resolvedGeometricRepresentations"], 130);
        assert_eq!(original["representationsTotal"], 130);
        assert_eq!(original["records"].as_array().unwrap().len(), MAX_DETAILS);
        assert_eq!(original["records"], changed["records"]);
        assert_ne!(
            original["resolvedContextRowsSha256"],
            changed["resolvedContextRowsSha256"]
        );
    }
    #[test]
    fn surface_parameter_spaces_are_not_assigned_physical_scale() {
        let data = context("#1")
            + "#1=SI_UNIT(*,.MILLI.,.METRE.);#200=DEFINITIONAL_REPRESENTATION('',(),#201);#201=(GEOMETRIC_REPRESENTATION_CONTEXT(2) PARAMETRIC_REPRESENTATION_CONTEXT() REPRESENTATION_CONTEXT('',''));";
        let report = resolved(&data);
        assert_eq!(report["lengthUnitsResolved"], true);
        assert_eq!(report["resolvedGeometricRepresentations"], 1);
        assert_eq!(report["parametricRepresentationsClassified"], 1);
        assert_eq!(
            report["records"][1]["reason"],
            "surface_parameter_space_not_physical_length"
        );
        assert!(report["records"][1].get("metresPerUnit").is_none());
        assert_eq!(report["parametricSurfaceMappingVerified"], false);
        for changed in [
            data.replace("DEFINITIONAL_REPRESENTATION", "SHAPE_REPRESENTATION"),
            data.replace("CONTEXT(2)", "CONTEXT(3)"),
            data.replace(
                "PARAMETRIC_REPRESENTATION_CONTEXT()",
                "PARAMETRIC_REPRESENTATION_CONTEXT('extra')",
            ),
            data.replace(
                "PARAMETRIC_REPRESENTATION_CONTEXT()",
                "PARAMETRIC_REPRESENTATION_CONTEXT() GLOBAL_UNIT_ASSIGNED_CONTEXT((#1))",
            ),
            data.replace("PARAMETRIC_REPRESENTATION_CONTEXT() ", ""),
        ] {
            assert_eq!(resolved(&changed)["lengthUnitsResolved"], false);
        }
    }
    #[test]
    fn parameter_only_file_does_not_claim_resolved_physical_units() {
        let report = resolved(
            "#1=DEFINITIONAL_REPRESENTATION('',(),#2);#2=(GEOMETRIC_REPRESENTATION_CONTEXT(2) PARAMETRIC_REPRESENTATION_CONTEXT() REPRESENTATION_CONTEXT('',''));",
        );
        assert_eq!(report["lengthUnitsResolved"], false);
        assert_eq!(report["parametricRepresentationsClassified"], 1);
    }
}
