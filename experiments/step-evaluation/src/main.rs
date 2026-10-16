#![forbid(unsafe_code)]
#![recursion_limit = "256"]
mod candidate_mesh;
mod exact_predicate;
mod face_coverage;
mod face_representation;
mod negative_torus;
mod occurrence_world;
mod planar_face;
mod planar_repair;
mod spline_face_diagnostic;
mod triangle_diagnostics;
mod trim_polygon_candidate;
mod unit_preflight;
use oxideav_step::{GeometryLimits, ReadOptions, Tolerance};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs::File, io::Read, path::Path, time::Instant};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_WARNING_DETAILS: usize = 128;
const MAX_SHAPE_PROVENANCE_SUMMARIES: usize = 128;
const PROBE_MAX_TRIANGLES: usize = 2_000_000;
const PROBE_ABSOLUTE_TOLERANCE_METRES: f64 = 0.000_001;

fn read_bounded(reader: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("input_byte_cap_exceeded".into());
    }
    if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1") {
        return Err("git_lfs_pointer_not_materialized".into());
    }
    Ok(bytes)
}

fn rejection_reasons(warnings: usize) -> Vec<&'static str> {
    let mut reasons = vec![
        "experimental_probe_not_production_adapter",
        "units_not_independently_preflighted",
        "face_completeness_not_verified",
        "assembly_pose_not_verified",
    ];
    if warnings > 0 {
        reasons.push("reader_warnings_or_partial_geometry");
    }
    reasons
}

fn shape_provenance_summary(
    part_index: usize,
    shape_index: usize,
    representation_item_id: u64,
    triangles: usize,
    faces: &[u64],
) -> Value {
    let distinct: HashSet<u64> = faces.iter().copied().filter(|id| *id != 0).collect();
    json!({
        "partIndex": part_index,
        "shapeIndex": shape_index,
        "representationItemId": representation_item_id,
        "triangles": triangles,
        "provenanceEntries": faces.len(),
        "distinctNonzeroSourceFaceIds": distinct.len(),
        "unassignedProvenanceEntries": faces.iter().filter(|id| **id == 0).count(),
        "missingTriangleProvenanceEntries": triangles.saturating_sub(faces.len()),
        "excessProvenanceEntries": faces.len().saturating_sub(triangles),
        "provenanceEntryCountMismatch": faces.len() != triangles,
        "qualification": "reader_observation_not_source_face_completeness"
    })
}
fn probe(
    path: &Path,
    negative_torus_compat: bool,
    include_candidate_mesh: bool,
    repair: Option<(u64, u64, f64)>,
    diagnostic_face: Option<u64>,
) -> Result<Value, String> {
    let start = Instant::now();
    let bytes = read_bounded(File::open(path).map_err(|e| e.to_string())?)?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let options = ReadOptions {
        parse_limits: oxideav_step::StepLimits {
            max_input_len: MAX_BYTES,
            max_instances: 250_000,
            max_depth: 48,
            max_string_len: 1_048_576,
        },
        geometry_limits: GeometryLimits {
            max_triangles: PROBE_MAX_TRIANGLES,
            max_curve_samples: 16_384,
        },
        tolerance: Tolerance {
            absolute: Some(PROBE_ABSOLUTE_TOLERANCE_METRES),
            ..Tolerance::default()
        },
        keep_invisible: false,
    };
    let original = oxideav_ifc::parse_step_with_limits(&bytes, &options.parse_limits)
        .map_err(|e| format!("reader_error: STEP parse error: {e}"))?;
    let declared_units = unit_preflight::inspect(&original);
    let (model, compatibility, baseline) = if negative_torus_compat {
        let baseline = match oxideav_step::model_from_file(&original, &options) {
            Ok(model) => json!({
                "readerReturnedModel": true,
                "warningsTotal": model.warnings.len(),
                "warningDetails": model.warnings.iter().take(MAX_WARNING_DETAILS).map(|s| s.chars().take(512).collect::<String>()).collect::<Vec<_>>(),
                "warningDetailsTruncated": model.warnings.len() > MAX_WARNING_DETAILS,
                "triangles": model.triangle_count(),
                "faceProvenanceEntries": model.parts.iter().flat_map(|p| &p.shapes).map(|s| s.triangle_faces.len()).sum::<usize>(),
            }),
            Err(error) => {
                json!({"readerReturnedModel":false,"error":error.to_string().chars().take(512).collect::<String>()})
            }
        };
        let (derived, compatibility) = match negative_torus::normalized_copy(&original, true) {
            Ok(value) => value,
            Err(error) => {
                return Ok(
                    json!({"probeSchema":"tokito.step-evaluation.v1", "sourceSha256":hash, "sourceBytes":bytes.len(), "admitted":false, "exportsProduced":[], "baselineBeforeCompatibility":baseline, "negativeTorusCompatibility":{"enabled":true,"rejected":true,"error":error},"durationMilliseconds":start.elapsed().as_millis()}),
                )
            }
        };
        let model = match oxideav_step::model_from_file(&derived, &options) {
            Ok(model) => model,
            Err(error) => {
                return Ok(
                    json!({"probeSchema":"tokito.step-evaluation.v1", "sourceSha256":hash, "sourceBytes":bytes.len(), "admitted":false, "exportsProduced":[], "baselineBeforeCompatibility":baseline, "negativeTorusCompatibility":compatibility,"error":format!("reader_error: {error}"),"durationMilliseconds":start.elapsed().as_millis()}),
                )
            }
        };
        (model, compatibility, baseline)
    } else {
        (
            oxideav_step::model_from_file(&original, &options)
                .map_err(|e| format!("reader_error: {e}"))?,
            json!({"enabled":false}),
            Value::Null,
        )
    };
    let (model, repair_report) = if let Some((face, representation, tolerance)) = repair {
        let (repaired, report) =
            planar_repair::apply(&original, &model, face, representation, tolerance)?;
        (repaired, report)
    } else {
        (model, Value::Null)
    };
    let occurrence_world = occurrence_world_report(&model, &declared_units);
    let triangle_diagnostic = diagnostic_face
        .map(|face| triangle_diagnostics::inspect(&model, face))
        .transpose()?;
    let candidate_mesh_data = if include_candidate_mesh {
        let result = occurrence_world["lengthScaleModelUnitsToMm"]
            .as_f64()
            .filter(|_| occurrence_world["transformCompositionResolved"] == true)
            .ok_or_else(|| "independently_resolved_occurrence_units_required".to_string())
            .and_then(|scale| candidate_mesh::inspect(&model, scale));
        match result {
            Ok(mut data) => {
                data["sourceSha256"] = json!(hash);
                data["readerWarningsTotal"] = json!(model.warnings.len());
                data
            }
            Err(error) => json!({"available":false,"error":error,"geometryQualified":false}),
        }
    } else {
        Value::Null
    };
    let mut bounds = [f64::INFINITY; 3];
    let mut upper = [f64::NEG_INFINITY; 3];
    let mut positions = 0usize;
    let mut nonfinite = 0usize;
    let mut styled_shapes = 0usize;
    let mut styled_triangles = 0usize;
    let mut provenance = 0usize;
    let mut shapes = 0usize;
    let mut shape_provenance_summaries = Vec::new();
    let mut unassigned_provenance_entries = 0usize;
    let mut provenance_count_mismatched_shapes = 0usize;
    let mut missing_triangle_provenance_entries = 0usize;
    let mut excess_provenance_entries = 0usize;
    let mut face_audit = face_coverage::Audit::new(&original);
    let mut face_coverage_details = Vec::new();
    let mut unmatched_shape_face_sets = 0usize;
    for (part_index, part) in model.parts.iter().enumerate() {
        for (shape_index, shape) in part.shapes.iter().enumerate() {
            shapes += 1;
            styled_shapes += usize::from(shape.colour.is_some());
            styled_triangles += shape
                .triangle_colours
                .iter()
                .filter(|c| c.is_some())
                .count();
            provenance += shape.triangle_faces.len();
            let triangle_count = shape.mesh.triangles.len();
            let coverage = face_audit.compare(shape.item, triangle_count, &shape.triangle_faces);
            unmatched_shape_face_sets += usize::from(coverage["sourceFacePresenceMatches"] != true);
            if face_coverage_details.len() < MAX_SHAPE_PROVENANCE_SUMMARIES {
                face_coverage_details.push(coverage);
            }
            unassigned_provenance_entries +=
                shape.triangle_faces.iter().filter(|id| **id == 0).count();
            provenance_count_mismatched_shapes +=
                usize::from(shape.triangle_faces.len() != triangle_count);
            missing_triangle_provenance_entries +=
                triangle_count.saturating_sub(shape.triangle_faces.len());
            excess_provenance_entries += shape.triangle_faces.len().saturating_sub(triangle_count);
            if shape_provenance_summaries.len() < MAX_SHAPE_PROVENANCE_SUMMARIES {
                shape_provenance_summaries.push(shape_provenance_summary(
                    part_index,
                    shape_index,
                    shape.item,
                    triangle_count,
                    &shape.triangle_faces,
                ));
            }
            for position in &shape.mesh.positions {
                positions += 1;
                if position.iter().any(|x| !x.is_finite()) {
                    nonfinite += 1;
                    continue;
                }
                for axis in 0..3 {
                    bounds[axis] = bounds[axis].min(position[axis]);
                    upper[axis] = upper[axis].max(position[axis]);
                }
            }
        }
    }
    let details: Vec<String> = model
        .warnings
        .iter()
        .take(MAX_WARNING_DETAILS)
        .map(|s| s.chars().take(512).collect())
        .collect();
    Ok(json!({
        "probeSchema": "tokito.step-evaluation.v1", "reader": "oxideav-step=0.0.1/default-features=false",
        "negativeTorusCompatibility": compatibility, "baselineBeforeCompatibility": baseline,
        "sourceSha256": hash, "sourceBytes": bytes.len(), "declaredApSchema": format!("{:?}", model.schema),
        "readerReportedLengthUnitMetres": model.length_unit_metres, "unitsIndependentlyPreflighted": false,
        "declaredRepresentationLengthUnits": declared_units,
        "assemblyOccurrenceWorld": occurrence_world,
        "candidateMeshData": candidate_mesh_data,
        "parts": model.parts.len(), "rootOccurrences": model.roots.len(), "childOccurrences": model.parts.iter().map(|p| p.children.len()).sum::<usize>(),
        "shapes": shapes, "triangles": model.triangle_count(), "positions": positions, "nonfinitePositions": nonfinite,
        "styledShapes": styled_shapes, "styledTriangleEntries": styled_triangles, "faceProvenanceEntries": provenance,
        "shapeProvenanceSummaries": shape_provenance_summaries,
        "shapeProvenanceSummariesTotal": shapes,
        "shapeProvenanceSummariesTruncated": shapes > MAX_SHAPE_PROVENANCE_SUMMARIES,
        "maxShapeProvenanceSummaries": MAX_SHAPE_PROVENANCE_SUMMARIES,
        "unassignedProvenanceEntries": unassigned_provenance_entries,
        "provenanceEntryCountMismatchedShapes": provenance_count_mismatched_shapes,
        "missingTriangleProvenanceEntries": missing_triangle_provenance_entries,
        "excessProvenanceEntries": excess_provenance_entries,
        "sourceFaceCompletenessVerified": false,
        "experimentalPlanarRepair":repair_report,
        "experimentalFaceDiagnostics":triangle_diagnostic,
        "sourceFaceCoverage": {"shapeDetails":face_coverage_details,"shapeDetailsTruncated":shapes>MAX_SHAPE_PROVENANCE_SUMMARIES,"shapesInspected":shapes,"unmatchedShapeFaceSets":unmatched_shape_face_sets,"allEmittedShapeFaceSetsMatch":shapes>0&&unmatched_shape_face_sets==0,"allSourceShapesCovered":false,"geometryQualified":false},
        "occurrenceColourOverrides": model.occurrence_colours.len(),
        "localBounds": if positions > nonfinite { json!({"min": bounds, "max": upper}) } else { Value::Null },
        "boundsFrame": "union_of_part_local_frames_not_assembly_world", "boundsUnits": "reader_reported_model_length_units",
        "warningsTotal": model.warnings.len(), "warningDetails": details, "warningDetailsTruncated": model.warnings.len() > MAX_WARNING_DETAILS,
        "toleranceMetres": PROBE_ABSOLUTE_TOLERANCE_METRES, "maxTriangles": PROBE_MAX_TRIANGLES, "maxCurveSamples": 16_384, "maxInputBytes": MAX_BYTES,
        "durationMilliseconds": start.elapsed().as_millis(), "admitted": false,
        "rejectionReasons": rejection_reasons(model.warnings.len()), "exportsProduced": []
    }))
}

/// Resolve one unambiguous, independently audited physical scale for all
/// geometric representation contexts before reporting occurrence bounds.
fn occurrence_world_report(model: &oxideav_step::StepModel, declared_units: &Value) -> Value {
    let scale = (|| -> Result<f64, String> {
        if declared_units["analysisComplete"] != true
            || declared_units["lengthUnitsResolved"] != true
        {
            return Err("independent_representation_units_incomplete".into());
        }
        let mut scales = declared_units["distinctMetresPerUnit"]
            .as_array()
            .ok_or("independent_unit_scale_summary_missing")?
            .iter()
            .map(|value| {
                value
                    .as_f64()
                    .filter(|scale| scale.is_finite() && *scale > 0.0)
                    .ok_or_else(|| "invalid_independent_length_scale".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if scales.is_empty() {
            return Err("no_independently_resolved_geometric_representation_units".into());
        }
        let reference = scales.pop().expect("non-empty scales checked");
        if scales
            .iter()
            .any(|scale| scale.to_bits() != reference.to_bits())
        {
            return Err("multiple_physical_length_scales_in_one_step_model".into());
        }
        let reader_scale = model.length_unit_metres;
        if !length_scales_agree(reader_scale, reference) {
            return Err("reader_and_independent_length_scales_disagree".into());
        }
        Ok(reference * 1000.0)
    })();
    match scale {
        Ok(scale_to_mm) => match occurrence_world::audit(model, scale_to_mm) {
            Ok(mut report) => {
                report["independentRepresentationUnitsResolved"] = json!(true);
                report["readerScaleAgreesWithIndependentUnits"] = json!(true);
                report["boundsQualifiedAgainstExternalReference"] = json!(false);
                report
            }
            Err(error) => json!({
                "schema":"tokito.step-occurrence-world.v1",
                "available":false,
                "error":error,
                "independentRepresentationUnitsResolved":true,
                "surfaceFidelityQualified":false,
                "altiumComponentAssociationQualified":false,
                "pcbPlacementQualified":false
            }),
        },
        Err(error) => json!({
            "schema":"tokito.step-occurrence-world.v1",
            "available":false,
            "error":error,
            "independentRepresentationUnitsResolved":false,
            "surfaceFidelityQualified":false,
            "altiumComponentAssociationQualified":false,
            "pcbPlacementQualified":false
        }),
    }
}

fn length_scales_agree(reader: f64, independent: f64) -> bool {
    reader.is_finite()
        && independent.is_finite()
        && reader > 0.0
        && independent > 0.0
        && (reader / independent - 1.0).abs() <= 1e-12
}

fn probe_units(path: &Path) -> Result<Value, String> {
    let bytes = read_bounded(File::open(path).map_err(|e| e.to_string())?)?;
    let limits = oxideav_ifc::StepLimits {
        max_input_len: MAX_BYTES,
        max_instances: 250_000,
        max_depth: 48,
        max_string_len: 1_048_576,
    };
    let parsed = oxideav_ifc::parse_step_with_limits(&bytes, &limits)
        .map_err(|error| format!("reader_error: STEP parse error: {error}"))?;
    Ok(json!({
        "probeSchema": "tokito.step-length-unit-preflight.v1",
        "sourceSha256": format!("{:x}", Sha256::digest(&bytes)),
        "sourceBytes": bytes.len(),
        "declaredRepresentationLengthUnits": unit_preflight::inspect(&parsed),
        "scope": "declared representation-context length units, not geometry or assembly admission",
        "admitted": false, "exportsProduced": []
    }))
}

fn probe_spline_face(path: &Path, face_id: u64) -> Result<Value, String> {
    let bytes = read_bounded(File::open(path).map_err(|e| e.to_string())?)?;
    let limits = oxideav_ifc::StepLimits {
        max_input_len: MAX_BYTES,
        max_instances: 250_000,
        max_depth: 48,
        max_string_len: 1_048_576,
    };
    let parsed = oxideav_ifc::parse_step_with_limits(&bytes, &limits)
        .map_err(|error| format!("reader_error: STEP parse error: {error}"))?;
    let mut report = spline_face_diagnostic::inspect(&parsed, face_id)?;
    report["sourceSha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
    report["sourceBytes"] = json!(bytes.len());
    report["admitted"] = json!(false);
    report["exportsProduced"] = json!([]);
    Ok(report)
}

fn main() {
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    if raw
        .first()
        .is_some_and(|arg| arg == "--diagnose-source-face")
    {
        let result = (|| -> Result<Value, String> {
            if raw.len() != 3 {
                return Err(
                    "usage: tokito-step-evaluation --diagnose-source-face POSITIVE_ID STEP_PATH"
                        .into(),
                );
            }
            let face = raw[1]
                .to_str()
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|n| *n > 0)
                .ok_or("invalid_diagnostic_face_id")?;
            probe_spline_face(Path::new(&raw[2]), face)
        })();
        println!("{}",result.unwrap_or_else(|error|json!({"schema":"tokito.step-spline-face-trim-diagnostic.v1","error":error,"admitted":false,"exportsProduced":[]})));
        std::process::exit(2);
    }
    if raw.first().is_some_and(|arg| arg == "--diagnose-face") {
        let result = (|| -> Result<Value, String> {
            if raw.len() != 3 {
                return Err(
                    "usage: tokito-step-evaluation --diagnose-face POSITIVE_ID STEP_PATH".into(),
                );
            }
            let face = raw[1]
                .to_str()
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|n| *n > 0)
                .ok_or("invalid_diagnostic_face_id")?;
            probe(Path::new(&raw[2]), false, false, None, Some(face))
        })();
        println!("{}",result.unwrap_or_else(|error|json!({"probeSchema":"tokito.step-evaluation.v1","error":error,"admitted":false,"exportsProduced":[]})));
        std::process::exit(2);
    }
    if raw
        .first()
        .is_some_and(|arg| arg == "--planar-face" || arg == "--repair-planar-face")
    {
        let result: Result<Value, String> = (|| {
            if !matches!(raw.len(), 5 | 7)
                || raw[2] != "--source-tolerance"
                || (raw.len() == 7 && raw[4] != "--source-representation")
            {
                return Err("usage: tokito-step-evaluation --planar-face ID --source-tolerance POSITIVE_SOURCE_UNITS [--source-representation ID] STEP_PATH".into());
            }
            let face_id = raw[1]
                .to_str()
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|n| *n > 0)
                .ok_or("invalid_face_id")?;
            let tolerance = raw[3]
                .to_str()
                .and_then(|s| s.parse::<f64>().ok())
                .ok_or("invalid_source_tolerance")?;
            if raw[0] == "--repair-planar-face" {
                if raw.len() != 7 {
                    return Err("repair_requires_explicit_source_representation".into());
                }
                let representation = raw[5]
                    .to_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .filter(|n| *n > 0)
                    .ok_or("invalid_source_representation")?;
                return probe(
                    Path::new(&raw[6]),
                    false,
                    true,
                    Some((face_id, representation, tolerance)),
                    None,
                );
            }
            let bytes = read_bounded(
                File::open(Path::new(&raw[raw.len() - 1])).map_err(|e| e.to_string())?,
            )?;
            let parsed = oxideav_ifc::parse_step_with_limits(
                &bytes,
                &oxideav_ifc::StepLimits {
                    max_input_len: MAX_BYTES,
                    max_instances: 250_000,
                    max_depth: 48,
                    max_string_len: 1_048_576,
                },
            )
            .map_err(|e| format!("reader_error: {e}"))?;
            let mut report = planar_face::inspect(&parsed, face_id, tolerance)?;
            if raw.len() == 7 {
                let representation = raw[5]
                    .to_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .filter(|n| *n > 0)
                    .ok_or("invalid_source_representation")?;
                report["selectedSourceRepresentation"] =
                    face_representation::inspect(&parsed, representation, face_id)?;
            }
            report["sourceSha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
            report["declaredRepresentationLengthUnits"] = unit_preflight::inspect(&parsed);
            Ok(report)
        })();
        println!("{}", result.unwrap_or_else(|error| json!({"schema":"tokito.step-linear-planar-face.v1", "error":error, "admitted":false, "exportsProduced":[]})));
        std::process::exit(2);
    }
    let mut paths = Vec::new();
    let mut negative_torus_compat = false;
    let mut units_only = false;
    let mut include_candidate_mesh = false;
    let mut invalid = false;
    for arg in std::env::args_os().skip(1) {
        if arg == "--negative-torus-compat" {
            if negative_torus_compat {
                invalid = true;
            }
            negative_torus_compat = true;
        } else if arg == "--candidate-mesh-data" {
            if include_candidate_mesh {
                invalid = true;
            }
            include_candidate_mesh = true;
        } else if arg == "--units-only" {
            if units_only {
                invalid = true;
            }
            units_only = true;
        } else if arg.to_string_lossy().starts_with('-') {
            invalid = true;
        } else {
            paths.push(arg);
        }
    }
    let result = if paths.len() == 1
        && !invalid
        && !(units_only && (negative_torus_compat || include_candidate_mesh))
    {
        if units_only {
            probe_units(Path::new(&paths[0]))
        } else {
            probe(
                Path::new(&paths[0]),
                negative_torus_compat,
                include_candidate_mesh,
                None,
                None,
            )
        }
    } else {
        Err("usage: tokito-step-evaluation [--negative-torus-compat] [--candidate-mesh-data] <explicit-materialized-step-path> | --units-only <path>".into())
    };
    let error_schema = if units_only {
        "tokito.step-length-unit-preflight.v1"
    } else {
        "tokito.step-evaluation.v1"
    };
    let report = result.unwrap_or_else(|error| json!({"probeSchema":error_schema, "admitted": false, "error": error, "exportsProduced": []}));
    println!("{report}");
    std::process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scale_agreement_is_relative_and_requires_positive_finite_units() {
        for scale in [1e-18, 0.001, 1.0, 1e18] {
            assert!(length_scales_agree(scale, scale));
            assert!(!length_scales_agree(scale * 2.0, scale));
            for invalid in [0.0, -scale, f64::NAN, f64::INFINITY] {
                assert!(!length_scales_agree(invalid, scale));
                assert!(!length_scales_agree(scale, invalid));
            }
        }
        assert!(!length_scales_agree(1e-13, 1e-18));
    }
    #[test]
    fn refuses_lfs_pointer() {
        assert_eq!(
            read_bounded(&b"version https://git-lfs.github.com/spec/v1\noid sha256:x"[..])
                .unwrap_err(),
            "git_lfs_pointer_not_materialized"
        );
    }
    #[test]
    fn caps_stream_before_parser() {
        assert_eq!(
            read_bounded(std::io::repeat(0).take((MAX_BYTES + 10) as u64)).unwrap_err(),
            "input_byte_cap_exceeded"
        );
    }
    #[test]
    fn warnings_add_partial_rejection_and_clean_mesh_still_not_admitted() {
        assert!(rejection_reasons(1).contains(&"reader_warnings_or_partial_geometry"));
        assert!(!rejection_reasons(0).contains(&"reader_warnings_or_partial_geometry"));
        assert!(rejection_reasons(0).contains(&"units_not_independently_preflighted"));
        assert!(rejection_reasons(0).contains(&"experimental_probe_not_production_adapter"));
    }
    #[test]
    fn provenance_summary_counts_repeated_zero_and_missing_entries() {
        let summary = shape_provenance_summary(2, 3, 42, 5, &[7, 7, 8, 0]);
        assert_eq!(summary["partIndex"], 2);
        assert_eq!(summary["shapeIndex"], 3);
        assert_eq!(summary["representationItemId"], 42);
        assert_eq!(summary["distinctNonzeroSourceFaceIds"], 2);
        assert_eq!(summary["unassignedProvenanceEntries"], 1);
        assert_eq!(summary["missingTriangleProvenanceEntries"], 1);
        assert_eq!(summary["excessProvenanceEntries"], 0);
        assert_eq!(summary["provenanceEntryCountMismatch"], true);
        assert_eq!(
            summary["qualification"],
            "reader_observation_not_source_face_completeness"
        );
    }
    #[test]
    fn provenance_summary_reports_excess_entries_and_matched_counts() {
        let excess = shape_provenance_summary(0, 0, 1, 1, &[4, 5]);
        assert_eq!(excess["excessProvenanceEntries"], 1);
        assert_eq!(excess["missingTriangleProvenanceEntries"], 0);
        assert_eq!(excess["provenanceEntryCountMismatch"], true);
        let matched = shape_provenance_summary(0, 1, 2, 2, &[4, 4]);
        assert_eq!(matched["provenanceEntryCountMismatch"], false);
        assert_eq!(matched["distinctNonzeroSourceFaceIds"], 1);
    }
}
