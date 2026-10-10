//! Development-only sampling of the unqualified compatibility candidate.
#![forbid(unsafe_code)]
#[path = "../src/negative_torus.rs"]
mod negative_torus;

use oxideav_step::{GeometryLimits, ReadOptions, StepModel, Tolerance, Transform};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::File, io::Read};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_OCCURRENCES: usize = 256;
const MAX_SAMPLES_PER_FACE: usize = 32;
const MAX_TOTAL_SAMPLES: usize = 8192;

fn visit(
    model: &StepModel,
    occurrence: &oxideav_step::Occurrence,
    parent: &Transform,
    path: &mut Vec<usize>,
    visits: &mut usize,
    faces: &[u64],
    samples: &mut Vec<Value>,
) -> Result<(), String> {
    if path.len() >= 32 || *visits >= MAX_OCCURRENCES || path.contains(&occurrence.part) {
        return Err("occurrence traversal limit or cycle".into());
    }
    *visits += 1;
    let part = model.parts.get(occurrence.part).ok_or("missing part")?;
    let transform = parent.compose(&occurrence.transform);
    path.push(occurrence.part);
    for shape in &part.shapes {
        if shape.triangle_faces.len() != shape.mesh.triangles.len() {
            return Err("triangle provenance mismatch".into());
        }
        let mut indices = BTreeMap::<u64, Vec<usize>>::new();
        for (index, face) in shape.triangle_faces.iter().enumerate() {
            if faces.contains(face) {
                indices.entry(*face).or_default().push(index);
            }
        }
        for (face, triangles) in indices {
            let stride = triangles.len().div_ceil(MAX_SAMPLES_PER_FACE).max(1);
            for &index in triangles.iter().step_by(stride).take(MAX_SAMPLES_PER_FACE) {
                let triangle = shape.mesh.triangles[index];
                let mut points = [[0.0; 3]; 3];
                for (point, vertex) in points.iter_mut().zip(triangle) {
                    let position = *shape
                        .mesh
                        .positions
                        .get(vertex as usize)
                        .ok_or("bad index")?;
                    *point = transform
                        .apply(position)
                        .map(|v| v * model.length_unit_metres * 1000.0);
                }
                let a = std::array::from_fn::<_, 3, _>(|i| points[1][i] - points[0][i]);
                let b = std::array::from_fn::<_, 3, _>(|i| points[2][i] - points[0][i]);
                let cross = [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ];
                let magnitude = cross.iter().map(|v| v * v).sum::<f64>().sqrt();
                if !magnitude.is_finite()
                    || magnitude <= 0.0
                    || points.iter().flatten().any(|v| !v.is_finite())
                {
                    return Err("nonfinite or degenerate sample".into());
                }
                let centroid = std::array::from_fn::<_, 3, _>(|i| {
                    (points[0][i] + points[1][i] + points[2][i]) / 3.0
                });
                samples.push(json!({"sourceFaceId":face,"partIndex":occurrence.part,"occurrencePath":path,"triangleIndex":index,"faceTriangleCount":triangles.len(),"verticesMm":points,"centroidMm":centroid,"windingNormal":cross.map(|v|v/magnitude)}));
                if samples.len() > MAX_TOTAL_SAMPLES {
                    return Err("sample budget exceeded".into());
                }
            }
        }
    }
    for child in &part.children {
        visit(model, child, &transform, path, visits, faces, samples)?;
    }
    path.pop();
    Ok(())
}

fn run() -> Result<Value, String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let controls = args.len() == 2 && args[1] == "--control-faces";
    if args.len() != 1 && !controls {
        return Err("usage: torus_surface_evidence <materialized STEP> [--control-faces]".into());
    }
    let mut bytes = Vec::new();
    File::open(&args[0])
        .map_err(|e| e.to_string())?
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES || bytes.starts_with(b"version https://git-lfs.github.com/spec/v1") {
        return Err("unmaterialized or oversized input".into());
    }
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
    let original = oxideav_ifc::parse_step_with_limits(&bytes, &options.parse_limits)
        .map_err(|e| e.to_string())?;
    let (derived, audit) = negative_torus::normalized_copy(&original, true)?;
    if audit["faceDetailsTruncated"] == true {
        return Err("incomplete affected-face inventory".into());
    }
    let mut faces: Vec<u64> = audit["faceDetails"]
        .as_array()
        .ok_or("missing face inventory")?
        .iter()
        .map(|item| item["faceId"].as_u64().ok_or("invalid face ID"))
        .collect::<Result<_, _>>()?;
    let model = oxideav_step::model_from_file(&derived, &options).map_err(|e| e.to_string())?;
    if controls {
        let mut candidates = std::collections::BTreeSet::new();
        for part in &model.parts {
            if part
                .shapes
                .iter()
                .any(|shape| shape.triangle_faces.iter().any(|id| faces.contains(id)))
            {
                for shape in &part.shapes {
                    candidates.extend(
                        shape
                            .triangle_faces
                            .iter()
                            .copied()
                            .filter(|id| *id != 0 && !faces.contains(id)),
                    );
                }
            }
        }
        faces = candidates.into_iter().take(8).collect();
    }
    let mut samples = Vec::new();
    let mut visits = 0;
    for occurrence in &model.roots {
        visit(
            &model,
            occurrence,
            &Transform::IDENTITY,
            &mut Vec::new(),
            &mut visits,
            &faces,
            &mut samples,
        )?;
    }
    if samples.is_empty() {
        return Err("no affected faces sampled".into());
    }
    let sampled: std::collections::BTreeSet<u64> = samples
        .iter()
        .filter_map(|s| s["sourceFaceId"].as_u64())
        .collect();
    let missing: Vec<u64> = faces
        .iter()
        .copied()
        .filter(|id| !sampled.contains(id))
        .collect();
    let warnings: Vec<String> = model
        .warnings
        .iter()
        .take(128)
        .map(|s| s.chars().take(512).collect())
        .collect();
    Ok(
        json!({"schema":"tokito.torus-surface-evidence.v1","sourceSha256":format!("{:x}",Sha256::digest(&bytes)),"compatibility":audit,"admitted":false,"sampleFrame":"reader_composed_occurrences_unqualified_world","sampleUnits":"mm","maxSamplesPerFacePerShapeOccurrence":MAX_SAMPLES_PER_FACE,"occurrencesVisited":visits,"sampleKind":if controls { "unchanged_same_part_controls" } else { "affected_faces" },"sampledFaceIds":sampled,"missingFaceIds":missing,"maxTotalSamples":MAX_TOTAL_SAMPLES,"warningCount":model.warnings.len(),"warnings":warnings,"samples":samples,"scope":"sparse triangle-centroid and winding-normal observations; not surface equivalence or production export"}),
    )
}

fn main() {
    match run() {
        Ok(report) => println!("{report}"),
        Err(error) => {
            println!("{}", json!({"admitted":false,"error":error}));
            std::process::exit(2);
        }
    }
}
