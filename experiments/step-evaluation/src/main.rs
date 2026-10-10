#![forbid(unsafe_code)]
use oxideav_step::{read_step_with, GeometryLimits, ReadOptions, Tolerance};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path, time::Instant};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_WARNING_DETAILS: usize = 128;

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

fn probe(path: &Path) -> Result<Value, String> {
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
    let model = read_step_with(&bytes, &options).map_err(|e| format!("reader_error: {e}"))?;
    let mut bounds = [f64::INFINITY; 3];
    let mut upper = [f64::NEG_INFINITY; 3];
    let mut positions = 0usize;
    let mut nonfinite = 0usize;
    let mut styled_shapes = 0usize;
    let mut styled_triangles = 0usize;
    let mut provenance = 0usize;
    let mut shapes = 0usize;
    for part in &model.parts {
        for shape in &part.shapes {
            shapes += 1;
            styled_shapes += usize::from(shape.colour.is_some());
            styled_triangles += shape
                .triangle_colours
                .iter()
                .filter(|c| c.is_some())
                .count();
            provenance += shape.triangle_faces.len();
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
        "sourceSha256": hash, "sourceBytes": bytes.len(), "declaredApSchema": format!("{:?}", model.schema),
        "readerReportedLengthUnitMetres": model.length_unit_metres, "unitsIndependentlyPreflighted": false,
        "parts": model.parts.len(), "rootOccurrences": model.roots.len(), "childOccurrences": model.parts.iter().map(|p| p.children.len()).sum::<usize>(),
        "shapes": shapes, "triangles": model.triangle_count(), "positions": positions, "nonfinitePositions": nonfinite,
        "styledShapes": styled_shapes, "styledTriangleEntries": styled_triangles, "faceProvenanceEntries": provenance,
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
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = if args.len() == 1 {
        probe(Path::new(&args[0]))
    } else {
        Err("usage: tokito-step-evaluation <explicit-materialized-step-path>".into())
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
}
