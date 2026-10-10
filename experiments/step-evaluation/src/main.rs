#![forbid(unsafe_code)]
#![recursion_limit = "256"]
mod negative_torus;
use oxideav_step::{read_step_with, GeometryLimits, ReadOptions, Tolerance};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs::File, io::Read, path::Path, time::Instant};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_WARNING_DETAILS: usize = 128;
const MAX_SHAPE_PROVENANCE_SUMMARIES: usize = 128;

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
fn probe(path: &Path, negative_torus_compat: bool) -> Result<Value, String> {
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
            max_triangles: 1_000_000,
            max_curve_samples: 16_384,
        },
        tolerance: Tolerance {
            absolute: Some(0.000_01),
            ..Tolerance::default()
        },
        keep_invisible: false,
    };
    let (model, compatibility, baseline) = if negative_torus_compat {
        let original = oxideav_ifc::parse_step_with_limits(&bytes, &options.parse_limits)
            .map_err(|e| format!("reader_error: {e}"))?;
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
            read_step_with(&bytes, &options).map_err(|e| format!("reader_error: {e}"))?,
            json!({"enabled":false}),
            Value::Null,
        )
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
        "occurrenceColourOverrides": model.occurrence_colours.len(),
        "localBounds": if positions > nonfinite { json!({"min": bounds, "max": upper}) } else { Value::Null },
        "boundsFrame": "union_of_part_local_frames_not_assembly_world", "boundsUnits": "reader_reported_model_length_units",
        "warningsTotal": model.warnings.len(), "warningDetails": details, "warningDetailsTruncated": model.warnings.len() > MAX_WARNING_DETAILS,
        "toleranceMetres": 0.000_01, "maxTriangles": 1_000_000, "maxCurveSamples": 16_384, "maxInputBytes": MAX_BYTES,
        "durationMilliseconds": start.elapsed().as_millis(), "admitted": false,
        "rejectionReasons": rejection_reasons(model.warnings.len()), "exportsProduced": []
    }))
}

fn main() {
    let mut paths = Vec::new();
    let mut negative_torus_compat = false;
    let mut invalid = false;
    for arg in std::env::args_os().skip(1) {
        if arg == "--negative-torus-compat" {
            if negative_torus_compat {
                invalid = true;
            }
            negative_torus_compat = true;
        } else if arg.to_string_lossy().starts_with('-') {
            invalid = true;
        } else {
            paths.push(arg);
        }
    }
    let result = if paths.len() == 1 && !invalid {
        probe(Path::new(&paths[0]), negative_torus_compat)
    } else {
        Err("usage: tokito-step-evaluation [--negative-torus-compat] <explicit-materialized-step-path>".into())
    };
    let report = result.unwrap_or_else(|error| json!({"probeSchema":"tokito.step-evaluation.v1", "admitted": false, "error": error, "exportsProduced": []}));
    println!("{report}");
    std::process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;
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
