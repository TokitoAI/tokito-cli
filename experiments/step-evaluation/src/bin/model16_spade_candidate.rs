//! Reproduce the bounded Spade face-2875 remesh experiment.
//!
//! This binary is intentionally isolated from the production exporter. It
//! emits UV triangles only and always marks the result unqualified/unadmitted.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use spade::{
    AngleLimit, ConstrainedDelaunayTriangulation, Point2, RefinementParameters, Triangulation,
};
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

const EXPECTED_SOURCE_SHA256: &str =
    "7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f";
const EXPECTED_DIAGNOSTIC_SHA256: &str =
    "6583d6d1b4b48a3ea7d9e1181beeb62d652b2894c7bf3c24b6e1428d0b1f387b";
const EXPECTED_FACE_ID: u64 = 2875;
const EXPECTED_BOUNDARY_SAMPLE_COUNT: usize = 100;
const EXPECTED_UNIQUE_BOUNDARY_VERTEX_COUNT: usize = 96;
const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
const MAX_ADDITIONAL_VERTICES: usize = 20_000;
const MAX_TRIANGLES: usize = 50_000;
const MAX_UV_TRIANGLE_AREA: f64 = 0.00005;
const ANGLE_LIMIT_DEGREES: f64 = 20.0;
const UV_DISTANCE_TOLERANCE: f64 = 1e-9;
const PARAMETER_COVERAGE_TOLERANCE: f64 = 1e-6;
const SPADE_VERSION: &str = "2.15.1";
type EdgeKey = (u64, u64, u64, u64);
type EdgeUseCounts = (usize, usize, usize);

#[derive(Debug)]
struct Candidate {
    loop_uv: Vec<[f64; 2]>,
    triangles_uv: Vec<[[f64; 2]; 3]>,
    source_sha256: String,
    diagnostic_sha256: String,
    generator_source_sha256: String,
    refinement_complete: bool,
    vertices_before: usize,
    vertices_after: usize,
    faces_before: usize,
    faces_after: usize,
    uv_area: f64,
    minimum_uv_angle_degrees: f64,
    topology: Value,
}

struct TriangulationResult {
    triangles_uv: Vec<[[f64; 2]; 3]>,
    refinement_complete: bool,
    vertices_before: usize,
    vertices_after: usize,
    faces_before: usize,
    faces_after: usize,
    uv_area: f64,
    minimum_uv_angle_degrees: f64,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("model16_spade_candidate: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 4 || args[0] != "--research-remesh-model16" {
        return Err(
            "usage: model16-spade-candidate --research-remesh-model16 SOURCE.step DIAGNOSTIC.json NEW_OUTPUT.json"
                .into(),
        );
    }

    let source_path = Path::new(&args[1]);
    let source_bytes = read_bounded(source_path, MAX_SOURCE_BYTES)
        .map_err(|error| format!("source_step_invalid: {error}"))?;
    let actual_source_sha256 = sha256(&source_bytes);
    let input_path = Path::new(&args[2]);
    let output_path = Path::new(&args[3]);
    let input_bytes = read_bounded(input_path, MAX_INPUT_BYTES)?;
    let diagnostic_sha256 = sha256(&input_bytes);
    if diagnostic_sha256 != EXPECTED_DIAGNOSTIC_SHA256 {
        return Err("pinned_model16_diagnostic_digest_mismatch".into());
    }
    let diagnostic: Value = serde_json::from_slice(&input_bytes)
        .map_err(|error| format!("diagnostic_json_invalid: {error}"))?;
    let source_sha256 = validate_diagnostic(&diagnostic, &actual_source_sha256)?;
    let loop_uv = extract_boundary_loop(&diagnostic)?;
    let generator_source_sha256 = sha256(include_bytes!("model16_spade_candidate.rs"));
    let mesh = triangulate(loop_uv.clone())?;
    let topology = topology_report(&loop_uv, &mesh.triangles_uv);

    let candidate = Candidate {
        loop_uv,
        triangles_uv: mesh.triangles_uv,
        source_sha256,
        diagnostic_sha256,
        generator_source_sha256,
        refinement_complete: mesh.refinement_complete,
        vertices_before: mesh.vertices_before,
        vertices_after: mesh.vertices_after,
        faces_before: mesh.faces_before,
        faces_after: mesh.faces_after,
        uv_area: mesh.uv_area,
        minimum_uv_angle_degrees: mesh.minimum_uv_angle_degrees,
        topology,
    };
    let output = candidate_json(&candidate);
    let output_bytes = serde_json::to_vec_pretty(&output)
        .map_err(|error| format!("candidate_json_serialize_failed: {error}"))?;
    if output_bytes.len() > MAX_OUTPUT_BYTES {
        return Err("candidate_output_exceeds_32_mib_limit".into());
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)
        .map_err(|error| format!("candidate_output_must_be_new: {error}"))?;
    file.write_all(&output_bytes)
        .map_err(|error| format!("candidate_output_write_failed: {error}"))?;
    file.write_all(b"\n")
        .map_err(|error| format!("candidate_output_write_failed: {error}"))?;

    println!(
        "{}",
        json!({
            "schema": "tokito.spade-model16-candidate-summary.v1",
            "candidateOutput": output_path.display().to_string(),
            "sourceSha256": candidate.source_sha256,
            "diagnosticSha256": candidate.diagnostic_sha256,
            "generatorSourceSha256": candidate.generator_source_sha256,
            "spadeVersion": SPADE_VERSION,
            "refinementComplete": candidate.refinement_complete,
            "vertices": candidate.topology["vertices"],
            "triangles": candidate.triangles_uv.len(),
            "nonmanifoldEdges": candidate.topology["nonmanifoldEdges"],
            "boundaryPolylineCovered": candidate.topology["boundaryPolylineCovered"],
            "minimumUvAngleDegrees": candidate.minimum_uv_angle_degrees,
            "geometryQualified": false,
            "admitted": false,
            "exportsProduced": []
        })
    );
    Ok(())
}

fn read_bounded(path: &Path, maximum_bytes: usize) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("input_metadata_failed: {error}"))?;
    if !metadata.is_file() || metadata.len() > maximum_bytes as u64 {
        return Err("input_not_regular_file_or_exceeds_size_limit".into());
    }
    let bytes = fs::read(path).map_err(|error| format!("diagnostic_read_failed: {error}"))?;
    if bytes.len() > maximum_bytes {
        return Err("input_exceeds_size_limit".into());
    }
    Ok(bytes)
}

fn validate_diagnostic(diagnostic: &Value, actual_source_sha256: &str) -> Result<String, String> {
    if diagnostic["faceId"].as_u64() != Some(EXPECTED_FACE_ID) {
        return Err("unexpected_source_face_id".into());
    }
    let source_sha256 = diagnostic["sourceSha256"]
        .as_str()
        .ok_or("source_digest_missing")?;
    if source_sha256 != EXPECTED_SOURCE_SHA256 || source_sha256 != actual_source_sha256 {
        return Err("pinned_model16_source_digest_mismatch".into());
    }
    let trim = &diagnostic["trimCandidate"];
    let exports_are_empty =
        matches!(trim["exportsProduced"].as_array(), Some(rows) if rows.is_empty());
    if trim["status"].as_str() != Some("candidate_only")
        || trim["admitted"].as_bool() != Some(false)
        || trim["geometryQualified"].as_bool() != Some(false)
        || !exports_are_empty
    {
        return Err("input_must_be_an_unadmitted_source_face_candidate".into());
    }
    Ok(source_sha256.to_owned())
}

fn extract_boundary_loop(diagnostic: &Value) -> Result<Vec<[f64; 2]>, String> {
    let samples = diagnostic["trimCandidate"]["boundarySamples"]
        .as_array()
        .ok_or("source_boundary_samples_missing")?;
    let loop_uv = ordered_boundary_loop(
        samples,
        EXPECTED_BOUNDARY_SAMPLE_COUNT,
        EXPECTED_UNIQUE_BOUNDARY_VERTEX_COUNT,
    )?;
    let mut unique_points = HashSet::with_capacity(loop_uv.len());
    for point in &loop_uv {
        if !unique_points.insert(point_key(*point)) {
            return Err("source_trim_loop_contains_nonadjacent_duplicate_uv".into());
        }
    }
    Ok(loop_uv)
}

fn ordered_boundary_loop(
    samples: &[Value],
    expected_sample_count: usize,
    expected_vertex_count: usize,
) -> Result<Vec<[f64; 2]>, String> {
    if samples.len() != expected_sample_count {
        return Err("pinned_source_boundary_sample_count_changed".into());
    }
    let mut ordered = vec![None; samples.len()];
    for sample in samples {
        let index = sample["boundaryIndex"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|index| *index < ordered.len())
            .ok_or("invalid_source_boundary_sample_index")?;
        if ordered[index].is_some() {
            return Err("duplicate_source_boundary_sample_index".into());
        }
        let uv = sample["candidateUv"]
            .as_array()
            .ok_or("invalid_source_boundary_uv")?;
        if uv.len() != 2 {
            return Err("invalid_source_boundary_uv".into());
        }
        let point = [
            uv[0]
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("invalid_boundary_u")?,
            uv[1]
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("invalid_boundary_v")?,
        ];
        ordered[index] = Some(point);
    }

    let mut loop_uv = Vec::with_capacity(samples.len());
    for point in ordered {
        let point = point.ok_or("missing_source_boundary_sample_index")?;
        if loop_uv.last().copied() != Some(point) {
            loop_uv.push(point);
        }
    }
    if loop_uv.first() == loop_uv.last() {
        loop_uv.pop();
    }
    if loop_uv.len() != expected_vertex_count {
        return Err("pinned_unique_boundary_vertex_count_changed".into());
    }
    Ok(loop_uv)
}

fn triangulate(loop_uv: Vec<[f64; 2]>) -> Result<TriangulationResult, String> {
    let points = loop_uv
        .iter()
        .map(|point| Point2::new(point[0], point[1]))
        .collect::<Vec<_>>();
    let edges = (0..points.len())
        .map(|index| [index, (index + 1) % points.len()])
        .collect::<Vec<_>>();
    let mut triangulation =
        ConstrainedDelaunayTriangulation::<Point2<f64>>::bulk_load_cdt(points, edges)
            .map_err(|error| format!("spade_cdt_build_failed: {error}"))?;
    let vertices_before = triangulation.num_vertices();
    let faces_before = triangulation.num_inner_faces();
    let refinement = triangulation.refine(
        RefinementParameters::<f64>::new()
            .exclude_outer_faces(true)
            .with_max_allowed_area(MAX_UV_TRIANGLE_AREA)
            .with_angle_limit(AngleLimit::from_deg(ANGLE_LIMIT_DEGREES))
            .with_max_additional_vertices(MAX_ADDITIONAL_VERTICES),
    );

    let mut triangles_uv = Vec::new();
    let mut uv_area = 0.0;
    let mut minimum_angle = f64::INFINITY;
    for face in triangulation.inner_faces() {
        let positions = face.positions();
        let triangle = [
            [positions[0].x, positions[0].y],
            [positions[1].x, positions[1].y],
            [positions[2].x, positions[2].y],
        ];
        let centroid = [
            (triangle[0][0] + triangle[1][0] + triangle[2][0]) / 3.0,
            (triangle[0][1] + triangle[1][1] + triangle[2][1]) / 3.0,
        ];
        if !point_in_loop(centroid, &loop_uv) {
            continue;
        }
        let determinant = (triangle[1][0] - triangle[0][0]) * (triangle[2][1] - triangle[0][1])
            - (triangle[1][1] - triangle[0][1]) * (triangle[2][0] - triangle[0][0]);
        if !determinant.is_finite() || determinant == 0.0 {
            return Err("spade_candidate_contains_degenerate_uv_triangle".into());
        }
        uv_area += determinant.abs() * 0.5;
        for index in 0..3 {
            minimum_angle = minimum_angle.min(angle_degrees(
                triangle[index],
                triangle[(index + 1) % 3],
                triangle[(index + 2) % 3],
            ));
        }
        triangles_uv.push(triangle);
        if triangles_uv.len() > MAX_TRIANGLES {
            return Err("spade_candidate_exceeds_50000_triangle_limit".into());
        }
    }
    if triangles_uv.is_empty()
        || triangulation.num_vertices()
            > EXPECTED_UNIQUE_BOUNDARY_VERTEX_COUNT + MAX_ADDITIONAL_VERTICES
    {
        return Err("spade_candidate_empty_or_exceeds_vertex_budget".into());
    }

    Ok(TriangulationResult {
        triangles_uv,
        refinement_complete: refinement.refinement_complete,
        vertices_before,
        vertices_after: triangulation.num_vertices(),
        faces_before,
        faces_after: triangulation.num_inner_faces(),
        uv_area,
        minimum_uv_angle_degrees: minimum_angle,
    })
}

fn candidate_json(candidate: &Candidate) -> Value {
    json!({
        "schema": "tokito.spade-model16-remesh-candidate.v1",
        "researchOnly": true,
        "scope": "pinned model-16 face 2875 UV remesh experiment; split constraints are allowed; the result does not evaluate inserted boundary points on STEP edge curves, does not certify complete surface deviation, and is not a production mesh",
        "sourceFaceId": EXPECTED_FACE_ID,
        "sourceSha256": candidate.source_sha256,
        "diagnosticSha256": candidate.diagnostic_sha256,
        "generatorSourceSha256": candidate.generator_source_sha256,
        "spadeVersion": SPADE_VERSION,
        "angleLimitDegrees": ANGLE_LIMIT_DEGREES,
        "maxUvTriangleArea": MAX_UV_TRIANGLE_AREA,
        "keepConstraintEdges": false,
        "parameters": {
            "angleLimitDegrees": ANGLE_LIMIT_DEGREES,
            "maxUvTriangleArea": MAX_UV_TRIANGLE_AREA,
            "maxAdditionalVertices": MAX_ADDITIONAL_VERTICES,
            "keepConstraintEdges": false,
            "centroidPointInTrimFilter": true
        },
        "refinementComplete": candidate.refinement_complete,
        "verticesBefore": candidate.vertices_before,
        "verticesAfter": candidate.vertices_after,
        "facesBefore": candidate.faces_before,
        "facesAfter": candidate.faces_after,
        "boundaryVertexCount": candidate.loop_uv.len(),
        "boundaryEdgeCount": candidate.loop_uv.len(),
        "triangleCountInsideTrim": candidate.triangles_uv.len(),
        "uvArea": candidate.uv_area,
        "minUvAngleDegrees": candidate.minimum_uv_angle_degrees,
        "topology": candidate.topology,
        "trimUvLoop": candidate.loop_uv,
        "trianglesUv": candidate.triangles_uv,
        "geometryEvaluation": {
            "sourceEdgeCurveReevaluation": "not_performed",
            "fullSurfaceDeviationBound": "not_performed",
            "independentFreecadComparison": "separate_required_gate",
            "geometryQualified": false
        },
        "geometryQualified": false,
        "admitted": false,
        "exportsProduced": []
    })
}

fn topology_report(loop_uv: &[[f64; 2]], triangles_uv: &[[[f64; 2]; 3]]) -> Value {
    let indexed_triangles = triangles_uv
        .iter()
        .map(|triangle| triangle.map(point_key))
        .collect::<Vec<_>>();
    let mut vertices = HashSet::new();
    let mut edges: HashMap<EdgeKey, EdgeUseCounts> = HashMap::new();
    for triangle in indexed_triangles {
        for point in triangle {
            vertices.insert(point);
        }
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let (low, high, forward) = if a <= b { (a, b, true) } else { (b, a, false) };
            let entry = edges
                .entry((low[0], low[1], high[0], high[1]))
                .or_insert((0, 0, 0));
            entry.0 += 1;
            if forward {
                entry.1 += 1;
            } else {
                entry.2 += 1;
            }
        }
    }
    let boundary = edges
        .iter()
        .filter(|(_, (count, _, _))| *count == 1)
        .map(|(key, _)| *key)
        .collect::<Vec<_>>();
    let nonmanifold_edges = edges.values().filter(|(count, _, _)| *count > 2).count();
    let inconsistent_interior_edges = edges
        .values()
        .filter(|(count, forward, reverse)| *count == 2 && (*forward != 1 || *reverse != 1))
        .count();
    let loop_points_preserved = loop_uv
        .iter()
        .all(|point| vertices.contains(&point_key(*point)));
    let boundary_coverage = boundary_polyline_coverage(&boundary, triangles_uv, loop_uv);
    let euler = vertices.len() as isize - edges.len() as isize + triangles_uv.len() as isize;
    json!({
        "vertices": vertices.len(),
        "edges": edges.len(),
        "triangles": triangles_uv.len(),
        "boundaryEdges": boundary.len(),
        "interiorEdges": edges.values().filter(|(count, _, _)| *count == 2).count(),
        "nonmanifoldEdges": nonmanifold_edges,
        "inconsistentInteriorEdgeWindingCount": inconsistent_interior_edges,
        "boundaryPolylineCovered": boundary_coverage,
        "originalBoundarySamplesPreserved": loop_points_preserved,
        "mappedBoundaryEdges": if boundary_coverage { boundary.len() } else { 0 },
        "eulerCharacteristic": euler
    })
}

fn boundary_polyline_coverage(
    boundary_edges: &[(u64, u64, u64, u64)],
    triangles_uv: &[[[f64; 2]; 3]],
    loop_uv: &[[f64; 2]],
) -> bool {
    let coordinates = triangles_uv
        .iter()
        .flatten()
        .map(|point| (point_key(*point), *point))
        .collect::<HashMap<_, _>>();
    let mut intervals = vec![Vec::<(f64, f64)>::new(); loop_uv.len()];
    for edge in boundary_edges {
        let (Some(&a), Some(&b)) = (
            coordinates.get(&[edge.0, edge.1]),
            coordinates.get(&[edge.2, edge.3]),
        ) else {
            return false;
        };
        let mut best: Option<(f64, usize, f64, f64)> = None;
        for index in 0..loop_uv.len() {
            let start = loop_uv[index];
            let end = loop_uv[(index + 1) % loop_uv.len()];
            let (ta, da) = segment_location(a, start, end);
            let (tb, db) = segment_location(b, start, end);
            if da <= UV_DISTANCE_TOLERANCE
                && db <= UV_DISTANCE_TOLERANCE
                && ta >= -UV_DISTANCE_TOLERANCE
                && tb >= -UV_DISTANCE_TOLERANCE
                && ta <= 1.0 + UV_DISTANCE_TOLERANCE
                && tb <= 1.0 + UV_DISTANCE_TOLERANCE
            {
                let score = da.max(db);
                if best.is_none_or(|(best_score, _, _, _)| score < best_score) {
                    best = Some((score, index, ta.clamp(0.0, 1.0), tb.clamp(0.0, 1.0)));
                }
            }
        }
        let Some((_, index, ta, tb)) = best else {
            return false;
        };
        intervals[index].push((ta.min(tb), ta.max(tb)));
    }

    intervals.iter_mut().all(|spans| {
        spans.sort_by(|left, right| left.0.total_cmp(&right.0));
        let mut cursor = 0.0;
        for (start, end) in spans.iter().copied() {
            if start > cursor + PARAMETER_COVERAGE_TOLERANCE
                || start < cursor - PARAMETER_COVERAGE_TOLERANCE
            {
                return false;
            }
            cursor = cursor.max(end);
        }
        !spans.is_empty() && (cursor - 1.0).abs() <= PARAMETER_COVERAGE_TOLERANCE
    })
}

fn segment_location(point: [f64; 2], start: [f64; 2], end: [f64; 2]) -> (f64, f64) {
    let delta = [end[0] - start[0], end[1] - start[1]];
    let denominator = delta[0] * delta[0] + delta[1] * delta[1];
    if denominator <= f64::EPSILON {
        return (0.0, f64::INFINITY);
    }
    let offset = [point[0] - start[0], point[1] - start[1]];
    let parameter = (offset[0] * delta[0] + offset[1] * delta[1]) / denominator;
    let projected = [
        start[0] + parameter * delta[0],
        start[1] + parameter * delta[1],
    ];
    (
        parameter,
        (point[0] - projected[0]).hypot(point[1] - projected[1]),
    )
}

fn point_in_loop(point: [f64; 2], loop_uv: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let mut previous = loop_uv.len() - 1;
    for current in 0..loop_uv.len() {
        let a = loop_uv[current];
        let b = loop_uv[previous];
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

fn angle_degrees(vertex: [f64; 2], first: [f64; 2], second: [f64; 2]) -> f64 {
    let a = [first[0] - vertex[0], first[1] - vertex[1]];
    let b = [second[0] - vertex[0], second[1] - vertex[1]];
    let denominator = a[0].hypot(a[1]) * b[0].hypot(b[1]);
    if denominator == 0.0 {
        return 0.0;
    }
    let cosine = ((a[0] * b[0] + a[1] * b[1]) / denominator).clamp(-1.0, 1.0);
    cosine.acos().to_degrees()
}

fn point_key(point: [f64; 2]) -> [u64; 2] {
    [
        if point[0] == 0.0 { 0.0_f64 } else { point[0] }.to_bits(),
        if point[1] == 0.0 { 0.0_f64 } else { point[1] }.to_bits(),
    ]
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(index: usize, u: f64, v: f64) -> Value {
        json!({"boundaryIndex": index, "candidateUv": [u, v]})
    }

    #[test]
    fn boundary_loop_deduplicates_only_adjacent_and_closing_samples() {
        let samples = json!([
            sample(0, 0.0, 0.0),
            sample(1, 1.0, 0.0),
            sample(2, 1.0, 0.0),
            sample(3, 1.0, 1.0),
            sample(4, 0.0, 1.0),
            sample(5, 0.0, 0.0)
        ]);
        let loop_uv =
            ordered_boundary_loop(samples.as_array().unwrap(), 6, 4).expect("valid closed loop");
        assert_eq!(
            loop_uv,
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
        );
    }

    #[test]
    fn boundary_loop_rejects_missing_or_duplicate_indices() {
        let duplicate = json!([
            sample(0, 0.0, 0.0),
            sample(0, 1.0, 0.0),
            sample(2, 1.0, 1.0)
        ]);
        assert_eq!(
            ordered_boundary_loop(duplicate.as_array().unwrap(), 3, 2).unwrap_err(),
            "duplicate_source_boundary_sample_index"
        );
        let out_of_range = json!([sample(0, 0.0, 0.0), sample(2, 1.0, 1.0)]);
        assert_eq!(
            ordered_boundary_loop(out_of_range.as_array().unwrap(), 2, 2).unwrap_err(),
            "invalid_source_boundary_sample_index"
        );
    }

    #[test]
    fn uv_loop_predicate_accepts_inside_and_rejects_outside() {
        let square = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        assert!(point_in_loop([0.5, 0.5], &square));
        assert!(!point_in_loop([1.5, 0.5], &square));
    }

    #[test]
    fn diagnostic_source_digest_must_match_the_hashed_step_input() {
        let diagnostic = json!({
            "faceId": EXPECTED_FACE_ID,
            "sourceSha256": EXPECTED_SOURCE_SHA256,
            "trimCandidate": {
                "status": "candidate_only",
                "admitted": false,
                "geometryQualified": false,
                "exportsProduced": []
            }
        });
        assert_eq!(
            validate_diagnostic(&diagnostic, EXPECTED_SOURCE_SHA256).unwrap(),
            EXPECTED_SOURCE_SHA256
        );
        assert_eq!(
            validate_diagnostic(&diagnostic, "00".repeat(32).as_str()).unwrap_err(),
            "pinned_model16_source_digest_mismatch"
        );
    }
}
