//! Reproduce the bounded Spade face-2875 remesh experiment.
//!
//! This binary is intentionally isolated from the production exporter. It
//! emits UV triangles only and always marks the result unqualified/unadmitted.

#![recursion_limit = "256"]

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
const EXPECTED_FACE_ID: u64 = 2875;
const EXPECTED_BOUNDARY_SAMPLE_COUNT: usize = 100;
const EXPECTED_UNIQUE_BOUNDARY_VERTEX_COUNT: usize = 96;
const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
const MAX_ADDITIONAL_VERTICES: usize = 20_000;
const MAX_TRIANGLES: usize = 50_000;
const MAX_SOURCE_BOUNDARY_VERTICES: usize =
    EXPECTED_UNIQUE_BOUNDARY_VERTEX_COUNT + MAX_ADDITIONAL_VERTICES;
const MAX_ADDITIONAL_BOUNDARY_PROBE_RESIDUAL_SOURCE_UNITS: f64 = 1e-5;
const MAX_UV_TRIANGLE_AREA: f64 = 0.00005;
const ANGLE_LIMIT_DEGREES: f64 = 20.0;
const UV_DISTANCE_TOLERANCE: f64 = 1e-9;
const PARAMETER_COVERAGE_TOLERANCE: f64 = 1e-6;
const EXPECTED_SOURCE_EDGE_COUNT: usize = 4;
const SPADE_VERSION: &str = "2.15.1";
type EdgeKey = (u64, u64, u64, u64);
type EdgeUseCounts = (usize, usize, usize);
type BoundaryIntervalKey = (usize, u64, u64, [u64; 2], [u64; 2]);

#[derive(Clone, Debug)]
struct BoundaryPoint {
    loop_index: usize,
    curve_parameter: f64,
    uv: [f64; 2],
    source_point: [f64; 3],
    oriented_edge_id: u64,
    edge_curve_id: u64,
    curve_geometry_id: u64,
    source_boundary_index: Option<usize>,
    source_sample_index: Option<usize>,
    source_probe_index: Option<usize>,
    source_quality_probe_index: Option<usize>,
}

struct BoundaryLoop {
    points: Vec<BoundaryPoint>,
    quality_probes: Vec<QualityBoundaryProbe>,
    original_sample_count: usize,
    retained_probe_count: usize,
    additional_source_probe_count: usize,
    available_quality_probe_count: usize,
}

#[derive(Clone, Debug)]
struct QualityBoundaryProbe {
    point: BoundaryPoint,
    interval_start_uv: [f64; 2],
    interval_end_uv: [f64; 2],
    chord_midpoint_uv: [f64; 2],
}

#[derive(Debug)]
struct Candidate {
    boundary_points: Vec<BoundaryPoint>,
    loop_uv: Vec<[f64; 2]>,
    original_boundary_sample_count: usize,
    retained_boundary_probe_count: usize,
    additional_source_boundary_probe_count: usize,
    quality_source_boundary_probe_count: usize,
    available_quality_probe_count: usize,
    selected_quality_probe_indices: Vec<usize>,
    initial_boundary_split_count: usize,
    initial_boundary_split_points_uv: Vec<[f64; 2]>,
    triangles_uv: Vec<[[f64; 2]; 3]>,
    source_sha256: String,
    diagnostic_sha256: String,
    generator_source_sha256: String,
    refinement_complete: bool,
    vertices_before: usize,
    vertices_after: usize,
    faces_before: usize,
    faces_after: usize,
    domain_point_in_loop_mismatch_count: usize,
    domain_point_in_loop_mismatch_examples: Vec<Value>,
    uv_area: f64,
    minimum_uv_angle_degrees: f64,
    topology: Value,
}

struct TriangulationResult {
    triangles_uv: Vec<[[f64; 2]; 3]>,
    boundary_vertices_uv: Vec<[f64; 2]>,
    refinement_complete: bool,
    vertices_before: usize,
    vertices_after: usize,
    faces_before: usize,
    faces_after: usize,
    domain_point_in_loop_mismatch_count: usize,
    domain_point_in_loop_mismatch_examples: Vec<Value>,
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
    let diagnostic: Value = serde_json::from_slice(&input_bytes)
        .map_err(|error| format!("diagnostic_json_invalid: {error}"))?;
    let source_sha256 = validate_diagnostic(&diagnostic, &actual_source_sha256)?;
    let mut boundary_loop = extract_boundary_loop(&diagnostic)?;
    let initial_loop_uv = boundary_loop
        .points
        .iter()
        .map(|point| point.uv)
        .collect::<Vec<_>>();
    let initial_mesh = triangulate(initial_loop_uv.clone(), false)?;
    let initial_boundary_split_points_uv = initial_mesh
        .boundary_vertices_uv
        .iter()
        .copied()
        .filter(|point| {
            !initial_loop_uv
                .iter()
                .any(|source| point_key(*point) == point_key(*source))
        })
        .collect::<Vec<_>>();
    let selected_quality_probe_indices = select_quality_probes(
        &boundary_loop.quality_probes,
        &initial_boundary_split_points_uv,
        &initial_loop_uv,
    )?;
    boundary_loop.points = insert_quality_probes(
        boundary_loop.points,
        &boundary_loop.quality_probes,
        &selected_quality_probe_indices,
    )?;
    let loop_uv = boundary_loop
        .points
        .iter()
        .map(|point| point.uv)
        .collect::<Vec<_>>();
    let generator_source_sha256 = sha256(include_bytes!("model16_spade_candidate.rs"));
    let mesh = triangulate(loop_uv.clone(), true)?;
    let topology = topology_report(&loop_uv, &mesh.triangles_uv);

    let candidate = Candidate {
        boundary_points: boundary_loop.points,
        loop_uv,
        original_boundary_sample_count: boundary_loop.original_sample_count,
        retained_boundary_probe_count: boundary_loop.retained_probe_count,
        additional_source_boundary_probe_count: boundary_loop.additional_source_probe_count,
        quality_source_boundary_probe_count: selected_quality_probe_indices.len(),
        available_quality_probe_count: boundary_loop.available_quality_probe_count,
        selected_quality_probe_indices,
        initial_boundary_split_count: initial_mesh
            .boundary_vertices_uv
            .iter()
            .filter(|point| {
                !initial_loop_uv
                    .iter()
                    .any(|source| point_key(**point) == point_key(*source))
            })
            .count(),
        initial_boundary_split_points_uv,
        triangles_uv: mesh.triangles_uv,
        source_sha256,
        diagnostic_sha256,
        generator_source_sha256,
        refinement_complete: mesh.refinement_complete,
        vertices_before: mesh.vertices_before,
        vertices_after: mesh.vertices_after,
        faces_before: mesh.faces_before,
        faces_after: mesh.faces_after,
        domain_point_in_loop_mismatch_count: mesh.domain_point_in_loop_mismatch_count,
        domain_point_in_loop_mismatch_examples: mesh.domain_point_in_loop_mismatch_examples,
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
        "sourceBoundaryVertices": candidate.boundary_points.len(),
            "originalBoundarySamples": candidate.original_boundary_sample_count,
            "retainedSourceBoundaryProbes": candidate.retained_boundary_probe_count,
            "additionalSourceCurveBoundaryProbes": candidate.additional_source_boundary_probe_count,
            "qualitySourceCurveBoundaryProbes": candidate.quality_source_boundary_probe_count,
            "availableQualitySourceCurveBoundaryProbes": candidate.available_quality_probe_count,
            "initialBoundarySplitVertices": candidate.initial_boundary_split_count,
            "boundaryEdgeCount": candidate.topology["boundaryEdges"],
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

fn extract_boundary_loop(diagnostic: &Value) -> Result<BoundaryLoop, String> {
    let samples = diagnostic["trimCandidate"]["boundarySamples"]
        .as_array()
        .ok_or("source_boundary_samples_missing")?;
    let original_loop_uv = ordered_boundary_loop(
        samples,
        EXPECTED_BOUNDARY_SAMPLE_COUNT,
        EXPECTED_UNIQUE_BOUNDARY_VERTEX_COUNT,
    )?;

    let mut indexed_samples = vec![None; EXPECTED_BOUNDARY_SAMPLE_COUNT];
    for sample in samples {
        let boundary_index = sample["boundaryIndex"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|index| *index < indexed_samples.len())
            .ok_or("invalid_source_boundary_sample_index")?;
        if indexed_samples[boundary_index].is_some() {
            return Err("duplicate_source_boundary_sample_index".into());
        }
        indexed_samples[boundary_index] = Some(parse_boundary_point(
            sample,
            Some(boundary_index),
            None,
            None,
        )?);
    }
    let base_samples = indexed_samples
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or("missing_source_boundary_sample_index")?;

    let probes = diagnostic["trimCandidate"]["boundaryRefinementProbes"]
        .as_array()
        .ok_or("source_boundary_refinement_probes_missing")?;
    if probes.len() > MAX_ADDITIONAL_VERTICES {
        return Err("source_boundary_refinement_probe_budget_exceeded".into());
    }
    let mut probes_by_edge = HashMap::<usize, Vec<BoundaryPoint>>::new();
    let mut retained_probe_count = 0;
    let mut additional_source_probe_count = 0;
    for (probe_index, probe) in probes.iter().enumerate() {
        let retained = probe["retainedInCandidate"]
            .as_bool()
            .ok_or("source_boundary_probe_retention_status_missing")?;
        let projection_residual = probe["projectionResidualSourceUnits"]
            .as_f64()
            .filter(|residual| residual.is_finite() && *residual >= 0.0);
        let surface_residual = probe["surfaceResidualSourceUnits"]
            .as_f64()
            .filter(|residual| residual.is_finite() && *residual >= 0.0);
        if !include_source_boundary_probe(retained, projection_residual, surface_residual) {
            continue;
        }
        let point = parse_boundary_point(probe, None, Some(probe_index), None)?;
        probes_by_edge
            .entry(point.loop_index)
            .or_default()
            .push(point);
        if retained {
            retained_probe_count += 1;
        } else {
            additional_source_probe_count += 1;
        }
    }

    let quality_probes = diagnostic["trimCandidate"]["boundaryQualityProbes"]
        .as_array()
        .ok_or("source_boundary_quality_probes_missing")?;
    let declared_quality_count = diagnostic["trimCandidate"]["boundaryQualityProbeCount"]
        .as_u64()
        .and_then(|count| usize::try_from(count).ok())
        .ok_or("source_boundary_quality_probe_count_missing")?;
    let declared_quality_residual_limit = diagnostic["trimCandidate"]
        ["boundaryQualityProbeSelectionResidualSourceUnits"]
        .as_f64()
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or("source_boundary_quality_probe_residual_limit_missing")?;
    if quality_probes.len() != declared_quality_count
        || quality_probes.len() > MAX_ADDITIONAL_VERTICES
        || diagnostic["trimCandidate"]["boundaryQualityProbeSelectionIsGeometryAdmissionTolerance"]
            .as_bool()
            != Some(false)
        || declared_quality_residual_limit != MAX_ADDITIONAL_BOUNDARY_PROBE_RESIDUAL_SOURCE_UNITS
    {
        return Err("source_boundary_quality_probe_inventory_invalid".into());
    }
    let mut expected_quality_intervals = HashSet::<BoundaryIntervalKey>::new();
    for loop_index in 0..EXPECTED_SOURCE_EDGE_COUNT {
        let mut edge_points = base_samples
            .iter()
            .filter(|point| point.loop_index == loop_index)
            .cloned()
            .collect::<Vec<_>>();
        edge_points.extend(probes_by_edge.get(&loop_index).cloned().unwrap_or_default());
        let direction = parameter_direction(
            &base_samples
                .iter()
                .filter(|point| point.loop_index == loop_index)
                .cloned()
                .collect::<Vec<_>>(),
        )?;
        edge_points.sort_by(|left, right| {
            let order = left.curve_parameter.total_cmp(&right.curve_parameter);
            if direction < 0.0 {
                order.reverse()
            } else {
                order
            }
        });
        edge_points.dedup_by(|right, left| {
            right.curve_parameter == left.curve_parameter
                && point_key(right.uv) == point_key(left.uv)
        });
        for pair in edge_points.windows(2) {
            if pair[0].curve_parameter == pair[1].curve_parameter {
                return Err("source_boundary_quality_interval_has_duplicate_parameter".into());
            }
            expected_quality_intervals.insert(boundary_interval_key(
                loop_index,
                pair[0].curve_parameter,
                pair[1].curve_parameter,
                pair[0].uv,
                pair[1].uv,
            ));
        }
    }
    let mut quality_boundary_probes = Vec::with_capacity(quality_probes.len());
    for (quality_probe_index, quality_probe) in quality_probes.iter().enumerate() {
        if quality_probe["qualityProbeIndex"].as_u64() != Some(quality_probe_index as u64) {
            return Err("source_boundary_quality_probe_index_invalid".into());
        }
        let start_parameter = quality_probe["intervalStartCurveParameter"]
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or("source_boundary_quality_probe_interval_start_missing")?;
        let end_parameter = quality_probe["intervalEndCurveParameter"]
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or("source_boundary_quality_probe_interval_end_missing")?;
        let actual_parameter = quality_probe["curveParameter"]
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or("source_boundary_quality_probe_parameter_missing")?;
        if start_parameter == end_parameter
            || actual_parameter != (start_parameter + end_parameter) * 0.5
        {
            return Err("source_boundary_quality_probe_parameter_not_interval_midpoint".into());
        }
        let projection_residual = quality_probe["projectionResidualSourceUnits"]
            .as_f64()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or("source_boundary_quality_probe_projection_residual_missing")?;
        let surface_residual = quality_probe["surfaceResidualSourceUnits"]
            .as_f64()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or("source_boundary_quality_probe_surface_residual_missing")?;
        if projection_residual > declared_quality_residual_limit
            || surface_residual > declared_quality_residual_limit
            || quality_probe["projectionConverged"].as_bool().is_none()
            || quality_probe["projectionStartCount"].as_u64() != Some(1)
        {
            return Err("source_boundary_quality_probe_residual_or_projection_unqualified".into());
        }
        let point = parse_boundary_point(quality_probe, None, None, Some(quality_probe_index))?;
        let start_uv = parse_numeric_array::<2>(
            &quality_probe["intervalStartCandidateUv"],
            "source_boundary_quality_probe_interval_start_uv_invalid",
        )?;
        let end_uv = parse_numeric_array::<2>(
            &quality_probe["intervalEndCandidateUv"],
            "source_boundary_quality_probe_interval_end_uv_invalid",
        )?;
        let expected_seed_uv = [
            (start_uv[0] + end_uv[0]) * 0.5,
            (start_uv[1] + end_uv[1]) * 0.5,
        ];
        let actual_seed_uv = parse_numeric_array::<2>(
            &quality_probe["projectionSeedUv"],
            "source_boundary_quality_probe_seed_uv_invalid",
        )?;
        let interval_key = boundary_interval_key(
            point.loop_index,
            start_parameter,
            end_parameter,
            start_uv,
            end_uv,
        );
        if !expected_quality_intervals.remove(&interval_key)
            || point.edge_curve_id != quality_probe["edgeCurveId"].as_u64().unwrap_or(0)
            || actual_seed_uv != expected_seed_uv
            || quality_probe["deviationFromChordMidpointUv"]
                .as_f64()
                .is_none_or(|value| !value.is_finite() || value < 0.0)
        {
            return Err("source_boundary_quality_probe_curve_or_seed_provenance_invalid".into());
        }
        quality_boundary_probes.push(QualityBoundaryProbe {
            point,
            interval_start_uv: start_uv,
            interval_end_uv: end_uv,
            chord_midpoint_uv: expected_seed_uv,
        });
    }
    if !expected_quality_intervals.is_empty() {
        return Err("source_boundary_quality_probe_intervals_incomplete".into());
    }

    let mut ordered_points = Vec::new();
    for loop_index in 0..EXPECTED_SOURCE_EDGE_COUNT {
        let edge_samples = base_samples
            .iter()
            .filter(|sample| sample.loop_index == loop_index)
            .cloned()
            .collect::<Vec<_>>();
        if edge_samples.len() < 2 {
            return Err("source_boundary_edge_sample_count_invalid".into());
        }
        let direction = parameter_direction(&edge_samples)?;
        let first = &edge_samples[0];
        let last = edge_samples.last().expect("two samples checked");
        let mut edge_points = edge_samples.clone();
        for probe in probes_by_edge.remove(&loop_index).unwrap_or_default() {
            if probe.oriented_edge_id != first.oriented_edge_id
                || probe.edge_curve_id != first.edge_curve_id
                || probe.curve_geometry_id != first.curve_geometry_id
                || probe.oriented_edge_id != last.oriented_edge_id
                || probe.edge_curve_id != last.edge_curve_id
                || probe.curve_geometry_id != last.curve_geometry_id
                || !parameter_between(
                    probe.curve_parameter,
                    first.curve_parameter,
                    last.curve_parameter,
                )
            {
                return Err("source_boundary_probe_curve_provenance_mismatch".into());
            }
            edge_points.push(probe);
        }
        edge_points.sort_by(|left, right| {
            let order = left.curve_parameter.total_cmp(&right.curve_parameter);
            if direction < 0.0 {
                order.reverse()
            } else {
                order
            }
        });
        for pair in edge_points.windows(2) {
            if pair[0].curve_parameter == pair[1].curve_parameter
                && point_key(pair[0].uv) != point_key(pair[1].uv)
            {
                return Err("source_boundary_duplicate_parameter_conflict".into());
            }
        }
        append_unique_boundary_points(&mut ordered_points, edge_points)?;
    }
    if !probes_by_edge.is_empty() {
        return Err("source_boundary_probe_edge_index_out_of_range".into());
    }
    if ordered_points.first().map(|point| point_key(point.uv))
        == ordered_points.last().map(|point| point_key(point.uv))
    {
        ordered_points.pop();
    }
    if ordered_points.len() < EXPECTED_UNIQUE_BOUNDARY_VERTEX_COUNT
        || ordered_points.len() > MAX_SOURCE_BOUNDARY_VERTICES
    {
        return Err("source_boundary_vertex_count_out_of_range".into());
    }
    let ordered_uv = ordered_points
        .iter()
        .map(|point| point.uv)
        .collect::<Vec<_>>();
    let mut unique_points = HashSet::with_capacity(ordered_uv.len());
    for point in &ordered_uv {
        if !unique_points.insert(point_key(*point)) {
            return Err("source_trim_loop_contains_nonadjacent_duplicate_uv".into());
        }
    }
    for point in original_loop_uv {
        if !ordered_uv
            .iter()
            .any(|candidate| point_key(*candidate) == point_key(point))
        {
            return Err("source_boundary_refinement_dropped_original_sample".into());
        }
    }
    Ok(BoundaryLoop {
        points: ordered_points,
        quality_probes: quality_boundary_probes,
        retained_probe_count,
        additional_source_probe_count,
        available_quality_probe_count: quality_probes.len(),
        original_sample_count: EXPECTED_UNIQUE_BOUNDARY_VERTEX_COUNT,
    })
}

fn parse_boundary_point(
    value: &Value,
    boundary_index: Option<usize>,
    probe_index: Option<usize>,
    quality_probe_index: Option<usize>,
) -> Result<BoundaryPoint, String> {
    let loop_index = value["loopIndex"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .filter(|index| *index < EXPECTED_SOURCE_EDGE_COUNT)
        .ok_or("invalid_source_boundary_loop_index")?;
    let curve_parameter = value["curveParameter"]
        .as_f64()
        .filter(|parameter| parameter.is_finite())
        .ok_or("invalid_source_boundary_curve_parameter")?;
    let uv = parse_numeric_array::<2>(&value["candidateUv"], "invalid_source_boundary_uv")?;
    let source_point = parse_numeric_array::<3>(
        &value["sourcePointSourceUnits"],
        "invalid_source_boundary_point",
    )?;
    let oriented_edge_id = value["orientedEdgeId"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or("invalid_source_boundary_oriented_edge_id")?;
    let edge_curve_id = value["edgeCurveId"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or("invalid_source_boundary_edge_curve_id")?;
    let curve_geometry_id = value["curveGeometryId"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or("invalid_source_boundary_curve_geometry_id")?;
    let source_sample_index = value["sampleIndex"]
        .as_u64()
        .and_then(|index| usize::try_from(index).ok());
    if boundary_index.is_some() != source_sample_index.is_some() {
        return Err("source_boundary_sample_index_provenance_mismatch".into());
    }
    Ok(BoundaryPoint {
        loop_index,
        curve_parameter,
        uv,
        source_point,
        oriented_edge_id,
        edge_curve_id,
        curve_geometry_id,
        source_boundary_index: boundary_index,
        source_sample_index,
        source_probe_index: probe_index,
        source_quality_probe_index: quality_probe_index,
    })
}

fn parse_numeric_array<const N: usize>(value: &Value, error: &str) -> Result<[f64; N], String> {
    let values = value.as_array().ok_or(error)?;
    if values.len() != N {
        return Err(error.into());
    }
    let mut output = [0.0; N];
    for (index, value) in values.iter().enumerate() {
        output[index] = value
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or(error)?;
    }
    Ok(output)
}

fn parameter_direction(samples: &[BoundaryPoint]) -> Result<f64, String> {
    let first = samples
        .first()
        .ok_or("source_boundary_edge_samples_empty")?;
    for sample in samples.iter().skip(1) {
        if sample.curve_parameter > first.curve_parameter {
            return Ok(1.0);
        }
        if sample.curve_parameter < first.curve_parameter {
            return Ok(-1.0);
        }
    }
    Err("source_boundary_curve_parameter_has_no_direction".into())
}

fn parameter_between(parameter: f64, first: f64, last: f64) -> bool {
    parameter >= first.min(last) && parameter <= first.max(last)
}

fn append_unique_boundary_points(
    output: &mut Vec<BoundaryPoint>,
    points: Vec<BoundaryPoint>,
) -> Result<(), String> {
    for point in points {
        if output
            .last()
            .is_some_and(|previous| point_key(previous.uv) == point_key(point.uv))
        {
            let previous = output.last().expect("last checked");
            if previous.source_point != point.source_point {
                return Err("source_boundary_junction_point_conflict".into());
            }
            continue;
        }
        output.push(point);
    }
    Ok(())
}

fn select_quality_probes(
    quality_probes: &[QualityBoundaryProbe],
    split_vertices: &[[f64; 2]],
    source_boundary: &[[f64; 2]],
) -> Result<Vec<usize>, String> {
    if split_vertices.is_empty() {
        return Err("spade_candidate_angle_refinement_produced_no_boundary_splits".into());
    }
    let mut selected = Vec::with_capacity(split_vertices.len());
    let mut selected_set = HashSet::with_capacity(split_vertices.len());
    for split in split_vertices {
        let matches = quality_probes
            .iter()
            .enumerate()
            .filter_map(|(index, probe)| {
                let distance = (probe.chord_midpoint_uv[0] - split[0])
                    .hypot(probe.chord_midpoint_uv[1] - split[1]);
                (distance <= UV_DISTANCE_TOLERANCE).then_some(index)
            })
            .collect::<Vec<_>>();
        if matches.len() != 1 || !selected_set.insert(matches[0]) {
            return Err("spade_boundary_split_has_ambiguous_source_curve_midpoint".into());
        }
        let probe = &quality_probes[matches[0]];
        let mut matched_interval = false;
        for index in 0..source_boundary.len() {
            let start = source_boundary[index];
            let end = source_boundary[(index + 1) % source_boundary.len()];
            if point_key(start) == point_key(probe.interval_start_uv)
                && point_key(end) == point_key(probe.interval_end_uv)
            {
                matched_interval = true;
                break;
            }
        }
        if !matched_interval {
            return Err("source_curve_quality_probe_interval_not_on_candidate_boundary".into());
        }
        selected.push(matches[0]);
    }
    Ok(selected)
}

fn insert_quality_probes(
    source_boundary: Vec<BoundaryPoint>,
    quality_probes: &[QualityBoundaryProbe],
    selected_indices: &[usize],
) -> Result<Vec<BoundaryPoint>, String> {
    let mut selected_by_interval = HashMap::with_capacity(selected_indices.len());
    for index in selected_indices {
        let probe = quality_probes
            .get(*index)
            .ok_or("selected_source_curve_quality_probe_index_invalid")?;
        let key = (
            point_key(probe.interval_start_uv),
            point_key(probe.interval_end_uv),
        );
        if selected_by_interval.insert(key, probe).is_some() {
            return Err("duplicate_selected_source_curve_quality_probe_interval".into());
        }
    }
    let mut output = Vec::with_capacity(source_boundary.len() + selected_indices.len());
    let mut inserted = HashSet::with_capacity(selected_indices.len());
    for index in 0..source_boundary.len() {
        let start = &source_boundary[index];
        let end = &source_boundary[(index + 1) % source_boundary.len()];
        output.push(start.clone());
        let key = (point_key(start.uv), point_key(end.uv));
        if let Some(probe) = selected_by_interval.get(&key) {
            if probe.point.loop_index >= EXPECTED_SOURCE_EDGE_COUNT
                || !parameter_between(
                    probe.point.curve_parameter,
                    start.curve_parameter,
                    end.curve_parameter,
                ) && start.loop_index == end.loop_index
            {
                return Err(
                    "selected_quality_probe_parameter_not_between_boundary_vertices".into(),
                );
            }
            if !inserted.insert(key) {
                return Err("selected_quality_probe_inserted_more_than_once".into());
            }
            output.push(probe.point.clone());
        }
    }
    if inserted.len() != selected_indices.len() {
        return Err("selected_source_curve_quality_probe_not_inserted".into());
    }
    let mut unique = HashSet::with_capacity(output.len());
    for point in &output {
        if !unique.insert(point_key(point.uv)) {
            return Err("source_curve_quality_probe_created_duplicate_boundary_uv".into());
        }
    }
    Ok(output)
}

fn triangulation_boundary_vertices(triangles: &[[[f64; 2]; 3]]) -> Vec<[f64; 2]> {
    let mut edge_uses = HashMap::<EdgeKey, usize>::new();
    let mut point_by_key = HashMap::<[u64; 2], [f64; 2]>::new();
    for triangle in triangles {
        for point in triangle {
            point_by_key.insert(point_key(*point), *point);
        }
        for (first, second) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let first_key = point_key(first);
            let second_key = point_key(second);
            let (low, high) = if first_key <= second_key {
                (first_key, second_key)
            } else {
                (second_key, first_key)
            };
            *edge_uses
                .entry((low[0], low[1], high[0], high[1]))
                .or_default() += 1;
        }
    }
    let mut boundary = edge_uses
        .iter()
        .filter(|(_, count)| **count == 1)
        .flat_map(|(edge, _)| [[edge.0, edge.1], [edge.2, edge.3]])
        .collect::<HashSet<_>>()
        .into_iter()
        .filter_map(|key| point_by_key.get(&key).copied())
        .collect::<Vec<_>>();
    boundary.sort_by_key(|point| point_key(*point));
    boundary
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

fn triangulate(
    loop_uv: Vec<[f64; 2]>,
    keep_constraint_edges: bool,
) -> Result<TriangulationResult, String> {
    let boundary_vertex_count = loop_uv.len();
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
    let parameters = RefinementParameters::<f64>::new()
        .exclude_outer_faces(true)
        .with_max_allowed_area(MAX_UV_TRIANGLE_AREA)
        .with_angle_limit(AngleLimit::from_deg(ANGLE_LIMIT_DEGREES))
        .with_max_additional_vertices(MAX_ADDITIONAL_VERTICES);
    let parameters = if keep_constraint_edges {
        parameters.keep_constraint_edges()
    } else {
        parameters
    };
    let refinement = triangulation.refine(parameters);
    let excluded_face_indices = refinement
        .excluded_faces
        .iter()
        .map(|face| face.index())
        .collect::<HashSet<_>>();
    let included_face_indices =
        constrained_domain_face_indices(&triangulation, &excluded_face_indices);

    let mut triangles_uv = Vec::new();
    let mut uv_area = 0.0;
    let mut minimum_angle = f64::INFINITY;
    let mut domain_point_in_loop_mismatch_count = 0;
    let mut domain_point_in_loop_mismatch_examples = Vec::new();
    for face in triangulation.inner_faces() {
        let is_in_domain = included_face_indices.contains(&face.index());
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
        let point_in_loop = point_in_loop(centroid, &loop_uv);
        if is_in_domain && !point_in_loop {
            domain_point_in_loop_mismatch_count += 1;
            if domain_point_in_loop_mismatch_examples.len() < 32 {
                let nearest_boundary_distance = loop_uv
                    .iter()
                    .enumerate()
                    .map(|(index, start)| {
                        segment_location(centroid, *start, loop_uv[(index + 1) % loop_uv.len()]).1
                    })
                    .fold(f64::INFINITY, f64::min);
                domain_point_in_loop_mismatch_examples.push(json!({
                    "faceIndex": face.index(),
                    "centroidUv": centroid,
                    "constraintDomainInside": is_in_domain,
                    "pointInLoopInside": point_in_loop,
                    "nearestBoundaryDistanceUv": nearest_boundary_distance
                }));
            }
        }
        if !is_in_domain {
            continue;
        }
        let determinant = (triangle[1][0] - triangle[0][0]) * (triangle[2][1] - triangle[0][1])
            - (triangle[1][1] - triangle[0][1]) * (triangle[2][0] - triangle[0][0]);
        let max_edge_length_squared = (0..3)
            .map(|index| {
                let a = triangle[index];
                let b = triangle[(index + 1) % 3];
                (b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)
            })
            .fold(0.0_f64, f64::max);
        let numerical_area_floor = 64.0 * f64::EPSILON * max_edge_length_squared;
        if !determinant.is_finite() || determinant.abs() <= numerical_area_floor {
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
        || triangulation.num_vertices() > boundary_vertex_count + MAX_ADDITIONAL_VERTICES
    {
        return Err("spade_candidate_empty_or_exceeds_vertex_budget".into());
    }

    Ok(TriangulationResult {
        boundary_vertices_uv: triangulation_boundary_vertices(&triangles_uv),
        triangles_uv,
        refinement_complete: refinement.refinement_complete,
        vertices_before,
        vertices_after: triangulation.num_vertices(),
        faces_before,
        faces_after: triangulation.num_inner_faces(),
        domain_point_in_loop_mismatch_count,
        domain_point_in_loop_mismatch_examples,
        uv_area,
        minimum_uv_angle_degrees: minimum_angle,
    })
}

fn constrained_domain_face_indices(
    triangulation: &ConstrainedDelaunayTriangulation<Point2<f64>>,
    excluded_face_indices: &HashSet<usize>,
) -> HashSet<usize> {
    triangulation
        .inner_faces()
        .filter(|face| !excluded_face_indices.contains(&face.index()))
        .map(|face| face.index())
        .collect()
}

fn include_source_boundary_probe(
    retained_in_candidate: bool,
    projection_residual: Option<f64>,
    surface_residual: Option<f64>,
) -> bool {
    retained_in_candidate
        || matches!(
            (projection_residual, surface_residual),
            (Some(projection), Some(surface))
                if projection <= MAX_ADDITIONAL_BOUNDARY_PROBE_RESIDUAL_SOURCE_UNITS
                    && surface <= MAX_ADDITIONAL_BOUNDARY_PROBE_RESIDUAL_SOURCE_UNITS
        )
}

fn candidate_json(candidate: &Candidate) -> Value {
    json!({
        "schema": "tokito.spade-model16-remesh-candidate.v1",
        "researchOnly": true,
        "scope": "pinned model-16 face 2875 UV remesh experiment; original diagnostic boundary samples and eligible source-curve probes form the initial boundary; each initial Spade boundary split is associated with a unique source interval by its UV chord midpoint, then replaced by the projected source-curve arithmetic parameter midpoint; no geometry admission or export is performed",
        "sourceFaceId": EXPECTED_FACE_ID,
        "sourceSha256": candidate.source_sha256,
        "diagnosticSha256": candidate.diagnostic_sha256,
        "generatorSourceSha256": candidate.generator_source_sha256,
        "spadeVersion": SPADE_VERSION,
        "angleLimitDegrees": ANGLE_LIMIT_DEGREES,
        "maxUvTriangleArea": MAX_UV_TRIANGLE_AREA,
        "keepConstraintEdges": true,
        "originalBoundarySampleCount": candidate.original_boundary_sample_count,
        "retainedSourceBoundaryProbeCount": candidate.retained_boundary_probe_count,
        "additionalSourceCurveBoundaryProbeCount": candidate.additional_source_boundary_probe_count,
        "qualitySourceCurveBoundaryProbeCount": candidate.quality_source_boundary_probe_count,
        "availableQualitySourceCurveBoundaryProbeCount": candidate.available_quality_probe_count,
        "matchedInitialBoundarySplitCount": candidate.initial_boundary_split_count,
        "initialRefinementBoundarySplitPointsUv": candidate.initial_boundary_split_points_uv,
        "matchedQualityProbeIndices": candidate.selected_quality_probe_indices,
        "sourceBoundaryVertexCount": candidate.boundary_points.len(),
        "sourceBoundaryPoints": candidate.boundary_points.iter().map(|point| json!({
            "loopIndex": point.loop_index,
            "curveParameter": point.curve_parameter,
            "candidateUv": point.uv,
            "sourcePointSourceUnits": point.source_point,
            "orientedEdgeId": point.oriented_edge_id,
            "edgeCurveId": point.edge_curve_id,
            "curveGeometryId": point.curve_geometry_id,
            "boundaryIndex": point.source_boundary_index,
            "sampleIndex": point.source_sample_index,
            "probeIndex": point.source_probe_index,
            "qualityProbeIndex": point.source_quality_probe_index
        })).collect::<Vec<_>>(),
        "parameters": {
            "angleLimitDegrees": ANGLE_LIMIT_DEGREES,
            "maxUvTriangleArea": MAX_UV_TRIANGLE_AREA,
            "maxAdditionalVertices": MAX_ADDITIONAL_VERTICES,
            "additionalBoundaryProbeResidualSelectionLimitSourceUnits": MAX_ADDITIONAL_BOUNDARY_PROBE_RESIDUAL_SOURCE_UNITS,
            "additionalBoundaryProbeResidualIsAdmissionTolerance": false,
            "qualitySourceCurveBoundaryProbeCount": candidate.quality_source_boundary_probe_count,
            "availableQualitySourceCurveBoundaryProbeCount": candidate.available_quality_probe_count,
            "qualityProbeSelectionStrategy": "associate_each_initial_spade_boundary_split_to_a_unique_source_interval_by_uv_chord_midpoint_then_insert_projected_curve_interval_midpoint",
            "keepConstraintEdges": true,
            "faceClassification": "complement_of_spade_refinement_excluded_faces",
            "pointInLoopCrossCheck": true
        },
        "refinementComplete": candidate.refinement_complete,
        "verticesBefore": candidate.vertices_before,
        "verticesAfter": candidate.vertices_after,
        "facesBefore": candidate.faces_before,
        "facesAfter": candidate.faces_after,
        "boundaryVertexCount": candidate.topology["boundaryVertices"],
        "boundaryEdgeCount": candidate.topology["boundaryEdges"],
        "domainClassificationPointInLoopMismatchCount": candidate.domain_point_in_loop_mismatch_count,
        "domainClassificationPointInLoopMismatchExamples": candidate.domain_point_in_loop_mismatch_examples,
        "triangleCountInsideTrim": candidate.triangles_uv.len(),
        "uvArea": candidate.uv_area,
        "minUvAngleDegrees": candidate.minimum_uv_angle_degrees,
        "topology": candidate.topology,
        "trimUvLoop": candidate.loop_uv,
        "trianglesUv": candidate.triangles_uv,
        "geometryEvaluation": {
            "sourceEdgeCurveReevaluation": "evaluate_pinned_source_curve_at_arithmetic_parameter_midpoint_then_project_with_diagnostic_surface_solver",
            "boundarySplitRepair": "replace_each_initial_spade_split_with_projected_source_curve_interval_midpoint",
            "initialSpadeBoundarySplitCount": candidate.initial_boundary_split_count,
            "sourceCurveBoundaryMidpointCount": candidate.quality_source_boundary_probe_count,
            "newBoundarySplitCount": candidate.topology["boundaryEdges"].as_u64().and_then(|count| count.checked_sub(candidate.boundary_points.len() as u64)),
            "sourceBoundaryEdgeCountDelta": candidate.topology["boundaryEdges"].as_i64().map(|count| count - candidate.boundary_points.len() as i64),
            "allBoundaryVerticesHaveSourceCurveProvenance": candidate.topology["boundaryVertices"].as_u64() == Some(candidate.boundary_points.len() as u64),
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
    let boundary_vertices = boundary
        .iter()
        .flat_map(|edge| [[edge.0, edge.1], [edge.2, edge.3]])
        .collect::<HashSet<_>>();
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
        "boundaryVertices": boundary_vertices.len(),
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

fn boundary_interval_key(
    loop_index: usize,
    start_parameter: f64,
    end_parameter: f64,
    start_uv: [f64; 2],
    end_uv: [f64; 2],
) -> BoundaryIntervalKey {
    let parameter_bits = |value: f64| {
        if value == 0.0 {
            0.0_f64.to_bits()
        } else {
            value.to_bits()
        }
    };
    (
        loop_index,
        parameter_bits(start_parameter),
        parameter_bits(end_parameter),
        point_key(start_uv),
        point_key(end_uv),
    )
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
    fn source_boundary_probe_selection_uses_both_residuals_for_new_points() {
        assert!(include_source_boundary_probe(true, None, None));
        assert!(include_source_boundary_probe(false, Some(1e-5), Some(1e-5)));
        assert!(!include_source_boundary_probe(
            false,
            Some(1e-5),
            Some(1.0001e-5)
        ));
        assert!(!include_source_boundary_probe(false, None, Some(1e-10)));
    }

    #[test]
    fn boundary_split_reuses_matching_exact_curve_midpoint_and_keeps_constraint() {
        let source = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let probe = QualityBoundaryProbe {
            point: BoundaryPoint {
                loop_index: 0,
                curve_parameter: 0.5,
                uv: [0.5, 0.0],
                source_point: [0.5, 0.0, 0.0],
                oriented_edge_id: 1,
                edge_curve_id: 2,
                curve_geometry_id: 3,
                source_boundary_index: None,
                source_sample_index: None,
                source_probe_index: None,
                source_quality_probe_index: Some(0),
            },
            interval_start_uv: [0.0, 0.0],
            interval_end_uv: [1.0, 0.0],
            chord_midpoint_uv: [0.5, 0.0],
        };
        let selected = select_quality_probes(std::slice::from_ref(&probe), &[[0.5, 0.0]], &source)
            .expect("exact curve midpoint matches the split");
        assert_eq!(selected, vec![0]);
        let points = source
            .iter()
            .enumerate()
            .map(|(index, uv)| BoundaryPoint {
                loop_index: index % EXPECTED_SOURCE_EDGE_COUNT,
                curve_parameter: index as f64,
                uv: *uv,
                source_point: [uv[0], uv[1], 0.0],
                oriented_edge_id: index as u64 + 1,
                edge_curve_id: index as u64 + 10,
                curve_geometry_id: index as u64 + 20,
                source_boundary_index: Some(index),
                source_sample_index: Some(index),
                source_probe_index: None,
                source_quality_probe_index: None,
            })
            .collect();
        let refined = insert_quality_probes(points, &[probe], &selected)
            .expect("source interval midpoint is inserted");
        assert_eq!(refined.len(), 5);
        assert_eq!(refined[1].uv, [0.5, 0.0]);
        assert_eq!(refined[1].source_quality_probe_index, Some(0));
    }

    #[test]
    fn boundary_split_without_a_unique_source_curve_midpoint_refuses() {
        let probe = QualityBoundaryProbe {
            point: BoundaryPoint {
                loop_index: 0,
                curve_parameter: 0.5,
                uv: [0.5, 0.0],
                source_point: [0.5, 0.0, 0.0],
                oriented_edge_id: 1,
                edge_curve_id: 2,
                curve_geometry_id: 3,
                source_boundary_index: None,
                source_sample_index: None,
                source_probe_index: None,
                source_quality_probe_index: Some(0),
            },
            interval_start_uv: [0.0, 0.0],
            interval_end_uv: [1.0, 0.0],
            chord_midpoint_uv: [0.5, 0.0],
        };
        assert_eq!(
            select_quality_probes(
                &[probe],
                &[[0.6, 0.0]],
                &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]
            )
            .unwrap_err(),
            "spade_boundary_split_has_ambiguous_source_curve_midpoint"
        );
    }

    #[test]
    fn spade_excluded_faces_select_square_and_concave_domains() {
        fn assert_domain(loop_uv: &[[f64; 2]], expected_area: f64) {
            let points = loop_uv
                .iter()
                .map(|point| Point2::new(point[0], point[1]))
                .collect::<Vec<_>>();
            let edges = (0..points.len())
                .map(|index| [index, (index + 1) % points.len()])
                .collect::<Vec<_>>();
            let mut triangulation =
                ConstrainedDelaunayTriangulation::<Point2<f64>>::bulk_load_cdt(points, edges)
                    .expect("simple closed constrained polygon");
            let refinement = triangulation.refine(
                RefinementParameters::<f64>::new()
                    .exclude_outer_faces(true)
                    .keep_constraint_edges()
                    .with_max_allowed_area(10.0)
                    .with_max_additional_vertices(0),
            );
            let excluded_face_indices = refinement
                .excluded_faces
                .iter()
                .map(|face| face.index())
                .collect::<HashSet<_>>();
            let included_face_indices =
                constrained_domain_face_indices(&triangulation, &excluded_face_indices);

            let mut selected_area = 0.0;
            let mut selected_count = 0;
            for face in triangulation.inner_faces() {
                let selected_by_constraints = included_face_indices.contains(&face.index());
                let positions = face.positions();
                let centroid = [
                    (positions[0].x + positions[1].x + positions[2].x) / 3.0,
                    (positions[0].y + positions[1].y + positions[2].y) / 3.0,
                ];
                assert_eq!(
                    selected_by_constraints,
                    point_in_loop(centroid, loop_uv),
                    "constraint domain and independent point-in-loop classification disagree"
                );
                if selected_by_constraints {
                    selected_count += 1;
                    selected_area += face.area();
                }
            }
            assert!(selected_count > 0);
            assert!((selected_area - expected_area).abs() < 1e-12);
        }

        assert_domain(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], 1.0);
        assert_domain(
            &[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [1.0, 1.0], [0.0, 2.0]],
            3.0,
        );
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
