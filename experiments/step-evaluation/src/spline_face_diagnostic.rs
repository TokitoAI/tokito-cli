//! Read-only STEP B-spline face / trim correspondence diagnostics.
//!
//! This remains in the experimental evaluator. It evaluates source control
//! data with oxideav-ifc's shared geometry kernel and measures projection
//! residuals; it does not repair, export, or qualify the source model.

use crate::trim_polygon_candidate::{BoundaryCurveSegment, BoundaryVertexPoint, RefinementOptions};
use oxideav_ifc::{kernel, StepFile, Value};
use oxideav_step::schema::Entity;
use serde_json::{json, Value as Json};

const MAX_FACE_EDGES: usize = 128;
const MAX_CONTROL_POINTS: usize = 65_536;
const MAX_TOTAL_EDGE_CURVE_CONTROL_POINTS: usize = 65_536;
const MAX_CURVE_SPANS: usize = 256;
const SAMPLES_PER_CURVE_SPAN: usize = 16;
const MAX_BOUNDARY_SAMPLES: usize = 16_384;
const MAX_BOUNDARY_REFINEMENT_PROBES: usize = 8_192;
const MAX_BOUNDARY_QUALITY_PROBES: usize = 8_192;
const BOUNDARY_QUALITY_PROBE_SELECTION_RESIDUAL_SOURCE_UNITS: f64 = 1e-5;
const MAX_SAMPLE_DETAILS: usize = 128;
const MAX_ORIENTATION_OBSERVATIONS: usize = 100_000;
const MAX_ORIENTATION_DIAGNOSTIC_TRIANGLES: usize = 50_000;
const ORIENTATION_U_BIN_COUNT: usize = 8;
const MAX_BRANCH_SHIFT: f64 = 1_000_000.0;
const MULTISTART_SUBDIVISIONS: usize = 8;
const MAX_AXIS_SEEDS: usize = 64;
const MAX_PROJECTION_ITERATIONS: usize = 24;
// Dimensionless observation threshold, not a geometric admission tolerance.
const PROJECTION_TANGENT_ORTHOGONALITY_TOLERANCE: f64 = 1e-8;
const MAX_SURFACE_EVALUATIONS: usize = 4_000_000;
const MAX_DIAGNOSTIC_CONTROL_POINT_EVALUATIONS: usize = 50_000_000;
const MAX_SHARED_KERNEL_GRID_CONTROL_POINT_EVALUATIONS: usize = 2_000_000;
const MAX_SHARED_KERNEL_INVERSE_CONTROL_POINT_EVALUATIONS: usize = 20_000_000;
const MAX_EDGE_CURVE_CONTROL_POINT_EVALUATIONS: usize = 20_000_000;
const SHARED_KERNEL_INVERSE_SURFACE_EVALUATIONS_PER_ITERATION: usize = 5;
// `oxideav-ifc = 0.0.3` builds inverse seeds with eight samples per
// B-spline knot span, capped at 256 samples per axis (the implementation's
// ceil-based cap can yield slightly more than 256 entries). This bounds the
// linear nearest-sample scan performed once per boundary point.
const SHARED_KERNEL_INVERSE_SAMPLES_PER_SPAN: usize = 8;
const SHARED_KERNEL_INVERSE_MAX_SAMPLES_PER_AXIS: usize = 256;
const MAX_SHARED_KERNEL_INVERSE_DISTANCE_CHECKS: usize = 10_000_000;

type P3 = [f64; 3];

struct SurfaceDefinition {
    evaluator: kernel::Surface,
    u_domain: (f64, f64),
    v_domain: (f64, f64),
    u_breaks: Vec<f64>,
    v_breaks: Vec<f64>,
    u_period: Option<f64>,
    v_period: Option<f64>,
    control_point_count: usize,
    construction_control_point_evaluation_estimate: usize,
    control_net_diagonal: f64,
    report: Json,
}

fn entity<'a>(step: &'a StepFile, id: u64, expected: &str) -> Result<Entity<'a>, String> {
    let inst = step
        .get(id)
        .ok_or_else(|| format!("missing_source_entity:{id}"))?;
    crate::unit_preflight::shape(inst)?;
    let entity = Entity { inst };
    if !entity.is_a(expected) {
        return Err(format!(
            "source_entity_type_mismatch:{id}:expected_{expected}"
        ));
    }
    Ok(entity)
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Typed { args, .. } => args.first().and_then(number),
        other => other.as_number().filter(|value| value.is_finite()),
    }
}

fn integer(value: &Value) -> Option<usize> {
    match value {
        Value::Typed { args, .. } => args.first().and_then(integer),
        other => other
            .as_integer()
            .and_then(|value| usize::try_from(value).ok()),
    }
}

fn required_integer(entity: Entity<'_>, name: &str, limit: usize) -> Result<usize, String> {
    entity
        .attr(name)
        .and_then(integer)
        .filter(|value| *value > 0 && *value <= limit)
        .ok_or_else(|| format!("invalid_or_excessive_{name}"))
}

fn number_list(entity: Entity<'_>, name: &str, max: usize) -> Result<Vec<f64>, String> {
    let items = entity.list(name).ok_or_else(|| format!("missing_{name}"))?;
    if items.is_empty() || items.len() > max {
        return Err(format!("invalid_or_excessive_{name}"));
    }
    items
        .iter()
        .map(|item| {
            number(item)
                .filter(|value| value.abs() <= 1e12)
                .ok_or_else(|| format!("invalid_value_in_{name}"))
        })
        .collect()
}

fn integer_list(entity: Entity<'_>, name: &str, max: usize) -> Result<Vec<usize>, String> {
    let items = entity.list(name).ok_or_else(|| format!("missing_{name}"))?;
    if items.is_empty() || items.len() > max {
        return Err(format!("invalid_or_excessive_{name}"));
    }
    items
        .iter()
        .map(|item| {
            integer(item)
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("invalid_value_in_{name}"))
        })
        .collect()
}

fn point(step: &StepFile, id: u64) -> Result<P3, String> {
    let point = entity(step, id, "CARTESIAN_POINT")?;
    let coordinates = number_list(point, "coordinates", 3)?;
    if coordinates.len() != 3 {
        return Err("three_point_coordinates_required".into());
    }
    Ok([coordinates[0], coordinates[1], coordinates[2]])
}

fn vertex(step: &StepFile, id: u64) -> Result<P3, String> {
    let vertex = entity(step, id, "VERTEX_POINT")?;
    point(
        step,
        vertex
            .reference("vertex_geometry")
            .ok_or("missing_vertex_point")?,
    )
}

fn distance(a: P3, b: P3) -> f64 {
    (0..3)
        .map(|axis| (a[axis] - b[axis]).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn source_boundary_curve_segments(
    boundary_samples: &[(usize, usize, f64, P3)],
    input_vertex_indices: &[usize],
    edge_count: usize,
) -> Result<Vec<BoundaryCurveSegment>, String> {
    if boundary_samples.len() != input_vertex_indices.len()
        || boundary_samples.len() < 3
        || edge_count < 3
        || boundary_samples[0].0 != 0
    {
        return Err("trim_candidate_boundary_provenance_count_mismatch".into());
    }
    let mut segments = Vec::with_capacity(boundary_samples.len());
    for index in 0..boundary_samples.len() - 1 {
        let (curve_index, sample_index, start_parameter, _) = boundary_samples[index];
        let (next_curve_index, next_sample_index, end_parameter, _) = boundary_samples[index + 1];
        let start_vertex = input_vertex_indices[index];
        let end_vertex = input_vertex_indices[index + 1];
        if curve_index == next_curve_index {
            if next_sample_index != sample_index + 1 {
                return Err("trim_candidate_source_curve_sample_gap".into());
            }
            if start_vertex != end_vertex {
                segments.push(BoundaryCurveSegment {
                    start_vertex,
                    end_vertex,
                    curve_index,
                    start_parameter,
                    end_parameter,
                });
            }
        } else if next_curve_index != curve_index + 1 || start_vertex != end_vertex {
            return Err("trim_candidate_source_edge_junction_mismatch".into());
        }
    }
    let (last_curve_index, _, _, _) = boundary_samples
        .last()
        .ok_or("empty_source_boundary_samples")?;
    let first_curve_index = boundary_samples[0].0;
    if *last_curve_index + 1 != edge_count
        || first_curve_index != 0
        || input_vertex_indices[boundary_samples.len() - 1] != input_vertex_indices[0]
    {
        return Err("trim_candidate_source_loop_junction_mismatch".into());
    }
    if segments.is_empty() || segments.len() > MAX_BOUNDARY_SAMPLES {
        return Err("trim_candidate_source_boundary_segment_count_invalid".into());
    }
    Ok(segments)
}

fn parse_surface(step: &StepFile, id: u64) -> Result<SurfaceDefinition, String> {
    let source = entity(step, id, "B_SPLINE_SURFACE")?;
    if !source.is_a("B_SPLINE_SURFACE_WITH_KNOTS") {
        return Err("unsupported_surface_requires_bspline_with_knots".into());
    }
    let u_degree = required_integer(source, "u_degree", 32)?;
    let v_degree = required_integer(source, "v_degree", 32)?;
    let rows = source
        .list("control_points_list")
        .ok_or("missing_surface_control_net")?;
    if rows.len() < 2 || rows.len() > MAX_CONTROL_POINTS {
        return Err("invalid_surface_control_row_count".into());
    }
    let mut controls = Vec::with_capacity(rows.len());
    let mut total_controls = 0usize;
    for row in rows {
        let points = row.as_list().ok_or("invalid_surface_control_row")?;
        total_controls = total_controls
            .checked_add(points.len())
            .filter(|count| *count <= MAX_CONTROL_POINTS)
            .ok_or("surface_control_point_budget_exceeded")?;
        if points.len() < 2 {
            return Err("invalid_surface_control_column_count".into());
        }
        let mut control_row = Vec::with_capacity(points.len());
        for control in points {
            control_row.push(point(
                step,
                control
                    .as_reference()
                    .ok_or("surface_control_point_must_be_reference")?,
            )?);
        }
        controls.push(control_row);
    }

    let weights = if source.is_a("RATIONAL_B_SPLINE_SURFACE") {
        let rows = source
            .list("weights_data")
            .ok_or("missing_surface_weights")?;
        if rows.len() != controls.len() {
            return Err("surface_weight_row_count_mismatch".into());
        }
        let mut all_weights = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let values = row.as_list().ok_or("invalid_surface_weight_row")?;
            if values.len() != controls[index].len() {
                return Err("surface_weight_column_count_mismatch".into());
            }
            let parsed = values
                .iter()
                .map(|value| {
                    number(value)
                        .filter(|weight| *weight > 0. && *weight <= 1e12)
                        .ok_or_else(|| "invalid_surface_weight".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            all_weights.push(parsed);
        }
        Some(all_weights)
    } else {
        None
    };

    let u_multiplicities = integer_list(source, "u_multiplicities", 512)?;
    let v_multiplicities = integer_list(source, "v_multiplicities", 512)?;
    let u_knots = number_list(source, "u_knots", 512)?;
    let v_knots = number_list(source, "v_knots", 512)?;
    let u_closed = source.logical("u_closed").ok_or("unknown_u_closed_flag")?;
    let v_closed = source.logical("v_closed").ok_or("unknown_v_closed_flag")?;

    let surface = kernel::BSplineSurface::new(
        u_degree,
        v_degree,
        &controls,
        weights.as_deref(),
        (&u_knots, &u_multiplicities),
        (&v_knots, &v_multiplicities),
        Some(u_closed),
        Some(v_closed),
    )
    .map_err(|error| format!("invalid_surface_definition:{error}"))?;
    let u_breaks = surface.u_breaks();
    let v_breaks = surface.v_breaks();
    let u_grid_samples = shared_kernel_inverse_axis_sample_count(&u_breaks)?;
    let v_grid_samples = shared_kernel_inverse_axis_sample_count(&v_breaks)?;
    let construction_control_point_evaluation_estimate =
        shared_kernel_grid_control_point_evaluation_estimate(
            u_grid_samples,
            v_grid_samples,
            total_controls,
        )?;
    let u_domain = surface.u_domain();
    let v_domain = surface.v_domain();
    let degrees = surface.degrees();
    let bounds = controls.iter().flatten().fold(
        ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
        |(mut lo, mut hi), p| {
            for axis in 0..3 {
                lo[axis] = lo[axis].min(p[axis]);
                hi[axis] = hi[axis].max(p[axis]);
            }
            (lo, hi)
        },
    );
    let control_net_diagonal = distance(bounds.0, bounds.1);
    if !control_net_diagonal.is_finite() || control_net_diagonal <= 0. {
        return Err("invalid_surface_control_net_extent".into());
    }
    let surface = kernel::Surface::bspline(surface);
    let periods = surface.periods();
    let seam = seam_diagnostics(&surface, v_closed, u_domain, v_domain)?;
    Ok(SurfaceDefinition {
        evaluator: surface,
        u_domain,
        v_domain,
        u_breaks: u_breaks.clone(),
        v_breaks: v_breaks.clone(),
        u_period: periods.0,
        v_period: periods.1,
        control_point_count: total_controls,
        construction_control_point_evaluation_estimate,
        control_net_diagonal,
        report: json!({
            "entityId": id,
            "entityType": if source.is_a("RATIONAL_B_SPLINE_SURFACE") { "RATIONAL_B_SPLINE_SURFACE_WITH_KNOTS" } else { "B_SPLINE_SURFACE_WITH_KNOTS" },
            "degrees": [degrees.0, degrees.1],
            "controlNetShape": [controls.len(), controls.first().map_or(0, Vec::len)],
            "controlPointCount": total_controls,
            "sharedKernelInverseGridControlPointEvaluationEstimate": construction_control_point_evaluation_estimate,
            "sharedKernelInverseGridControlPointEvaluationBudget": MAX_SHARED_KERNEL_GRID_CONTROL_POINT_EVALUATIONS,
            "rational": weights.is_some(),
            "uClosed": u_closed,
            "vClosed": v_closed,
            "uDomain": u_domain,
            "vDomain": v_domain,
            "uBreaks": u_breaks,
            "vBreaks": v_breaks,
            "uPeriod": periods.0,
            "vPeriod": periods.1,
            "controlNetDiagonalSourceUnits": control_net_diagonal,
            "vSeam": seam
        }),
    })
}

fn seam_diagnostics(
    surface: &kernel::Surface,
    v_closed: bool,
    u_domain: (f64, f64),
    v_domain: (f64, f64),
) -> Result<Json, String> {
    if !v_closed {
        return Ok(json!({"closedFlag": false, "evaluated": false}));
    }
    let (v0, v1) = v_domain;
    let span = v1 - v0;
    if !span.is_finite() || span <= 0. {
        return Err("invalid_v_parameter_domain".into());
    }
    let (u0, u1) = u_domain;
    let delta = span * 1e-6;
    if delta <= 0. || !delta.is_finite() || delta >= span * 0.25 {
        return Err("invalid_seam_probe_step".into());
    }
    let mut max_position_gap: f64 = 0.;
    let mut max_tangent_angle: f64 = 0.;
    let mut undefined_tangent_probe_count = 0usize;
    let probes = 16usize;
    for index in 0..=probes {
        let u = u0 + (u1 - u0) * (index as f64 / probes as f64);
        let start = surface.point_at([u, v0]);
        let end = surface.point_at([u, v1]);
        let start_inner = surface.point_at([u, v0 + delta]);
        let end_inner = surface.point_at([u, v1 - delta]);
        if [start, end, start_inner, end_inner]
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err("nonfinite_seam_surface_evaluation".into());
        }
        max_position_gap = max_position_gap.max(distance(start, end));
        match (unit(sub(start_inner, start)), unit(sub(end, end_inner))) {
            (Ok(tangent_a), Ok(tangent_b)) => {
                let cosine = dot(tangent_a, tangent_b).clamp(-1., 1.);
                max_tangent_angle = max_tangent_angle.max(cosine.acos());
            }
            _ => undefined_tangent_probe_count += 1,
        }
    }
    Ok(json!({
        "closedFlag": true,
        "evaluated": true,
        "probeCount": probes + 1,
        "maximumEndpointPositionGapSourceUnits": max_position_gap,
        "maximumOneSidedTangentAngleRadians": max_tangent_angle,
        "undefinedTangentProbeCount": undefined_tangent_probe_count,
        "finiteDifferenceFractionOfVSpan": 1e-6,
        "qualification": "sampled_diagnostic_only"
    }))
}

fn unit(vector: P3) -> Result<P3, String> {
    let length = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
    if !length.is_finite() || length <= 0. {
        return Err("zero_or_invalid_seam_tangent".into());
    }
    Ok(vector.map(|value| value / length))
}

fn sub(a: P3, b: P3) -> P3 {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: P3, b: P3) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

fn oriented_triangle_normal_alignment(
    points: [P3; 3],
    tangent_u: P3,
    tangent_v: P3,
    face_same_sense: bool,
) -> f64 {
    let face_sense_sign = if face_same_sense { 1. } else { -1. };
    let expected_normal = scale(cross(tangent_u, tangent_v), face_sense_sign);
    let triangle_normal = cross(sub(points[1], points[0]), sub(points[2], points[0]));
    let denominator =
        (dot(expected_normal, expected_normal) * dot(triangle_normal, triangle_normal)).sqrt();
    if !denominator.is_finite() || denominator <= 0. {
        return 0.;
    }
    dot(expected_normal, triangle_normal) / denominator
}

#[derive(Clone, Copy, Debug)]
struct OrientationObservation {
    uv_centroid: [f64; 2],
    alignment: f64,
    triangle_normal: Option<P3>,
    parameter_normal: Option<P3>,
}

#[derive(Default)]
struct AlignmentStatistics {
    count: usize,
    positive_count: usize,
    nonpositive_count: usize,
    undefined_parameter_normal_count: usize,
    sum: f64,
    minimum: Option<f64>,
    maximum: Option<f64>,
}

impl AlignmentStatistics {
    fn add(&mut self, alignment: f64, parameter_normal_defined: bool) -> Result<(), String> {
        if !alignment.is_finite() || !(-1.000_001..=1.000_001).contains(&alignment) {
            return Err("trim_candidate_orientation_continuity_alignment_invalid".into());
        }
        let alignment = alignment.clamp(-1.0, 1.0);
        self.count = self
            .count
            .checked_add(1)
            .ok_or("trim_candidate_orientation_continuity_count_overflow")?;
        self.positive_count += usize::from(alignment > 0.0);
        self.nonpositive_count += usize::from(alignment <= 0.0);
        self.undefined_parameter_normal_count += usize::from(!parameter_normal_defined);
        self.sum += alignment;
        self.minimum = Some(self.minimum.map_or(alignment, |value| value.min(alignment)));
        self.maximum = Some(self.maximum.map_or(alignment, |value| value.max(alignment)));
        Ok(())
    }

    fn report(&self) -> Json {
        json!({
            "count": self.count,
            "positiveCount": self.positive_count,
            "nonpositiveCount": self.nonpositive_count,
            "undefinedParameterNormalCount": self.undefined_parameter_normal_count,
            "minimumAlignment": self.minimum,
            "maximumAlignment": self.maximum,
            "meanAlignment": (self.count > 0).then_some(self.sum / self.count as f64)
        })
    }
}

#[derive(Default)]
struct DotStatistics {
    compared_pair_count: usize,
    nonpositive_dot_pair_count: usize,
    undefined_pair_count: usize,
    sum: f64,
    minimum: Option<f64>,
    maximum: Option<f64>,
}

impl DotStatistics {
    fn add(&mut self, first: Option<P3>, second: Option<P3>) -> Result<(), String> {
        let (Some(first), Some(second)) = (first, second) else {
            self.undefined_pair_count += 1;
            return Ok(());
        };
        let value = dot(first, second);
        if !value.is_finite() {
            return Err("trim_candidate_orientation_continuity_dot_nonfinite".into());
        }
        let value = value.clamp(-1.0, 1.0);
        self.compared_pair_count = self
            .compared_pair_count
            .checked_add(1)
            .ok_or("trim_candidate_orientation_continuity_pair_count_overflow")?;
        self.nonpositive_dot_pair_count += usize::from(value <= 0.0);
        self.sum += value;
        self.minimum = Some(self.minimum.map_or(value, |current| current.min(value)));
        self.maximum = Some(self.maximum.map_or(value, |current| current.max(value)));
        Ok(())
    }

    fn report(&self) -> Json {
        json!({
            "comparedPairCount": self.compared_pair_count,
            "nonpositiveDotPairCount": self.nonpositive_dot_pair_count,
            "undefinedPairCount": self.undefined_pair_count,
            "minimumDot": self.minimum,
            "maximumDot": self.maximum,
            "meanDot": (self.compared_pair_count > 0)
                .then_some(self.sum / self.compared_pair_count as f64)
        })
    }
}

#[derive(Default)]
struct EdgeUse {
    count: usize,
    first_triangle: Option<usize>,
    second_triangle: Option<usize>,
    first_direction: Option<bool>,
    second_direction: Option<bool>,
}

fn record_orientation_observation(
    observations: &std::cell::RefCell<Vec<OrientationObservation>>,
    observation: OrientationObservation,
) -> Result<(), String> {
    let mut observations = observations.borrow_mut();
    if observations.len() >= MAX_ORIENTATION_OBSERVATIONS {
        return Err("trim_candidate_orientation_observation_budget_exceeded".into());
    }
    observations.push(observation);
    Ok(())
}

fn orientation_continuity_report(
    uv_vertices: &[[f64; 2]],
    triangles: &[[usize; 3]],
    observations: &[OrientationObservation],
    u_domain: (f64, f64),
    cached_observation_count: usize,
) -> Result<Json, String> {
    let u_span = u_domain.1 - u_domain.0;
    if triangles.is_empty()
        || triangles.len() > MAX_ORIENTATION_DIAGNOSTIC_TRIANGLES
        || observations.len() != triangles.len()
        || cached_observation_count < observations.len()
        || cached_observation_count > MAX_ORIENTATION_OBSERVATIONS
        || !u_domain.0.is_finite()
        || !u_domain.1.is_finite()
        || !u_span.is_finite()
        || u_span <= 0.0
    {
        return Err("trim_candidate_orientation_continuity_input_invalid".into());
    }

    let mut winding_positive_count = 0usize;
    let mut winding_negative_count = 0usize;
    let mut winding_zero_count = 0usize;
    let mut u_bins = (0..ORIENTATION_U_BIN_COUNT)
        .map(|_| AlignmentStatistics::default())
        .collect::<Vec<_>>();
    let mut geometric_normal_dot_by_u = (0..ORIENTATION_U_BIN_COUNT)
        .map(|_| DotStatistics::default())
        .collect::<Vec<_>>();
    let mut parameter_normal_dot_by_u = (0..ORIENTATION_U_BIN_COUNT)
        .map(|_| DotStatistics::default())
        .collect::<Vec<_>>();
    let mut edge_uses = std::collections::BTreeMap::<(usize, usize), EdgeUse>::new();

    for (triangle_index, (triangle, observation)) in triangles.iter().zip(observations).enumerate()
    {
        let [a, b, c] = *triangle;
        if a >= uv_vertices.len()
            || b >= uv_vertices.len()
            || c >= uv_vertices.len()
            || a == b
            || b == c
            || c == a
        {
            return Err("trim_candidate_orientation_continuity_triangle_index_invalid".into());
        }
        let uv_a = uv_vertices[a];
        let uv_b = uv_vertices[b];
        let uv_c = uv_vertices[c];
        let signed_double_area =
            (uv_b[0] - uv_a[0]) * (uv_c[1] - uv_a[1]) - (uv_b[1] - uv_a[1]) * (uv_c[0] - uv_a[0]);
        if !signed_double_area.is_finite()
            || observation
                .uv_centroid
                .iter()
                .any(|value| !value.is_finite())
        {
            return Err("trim_candidate_orientation_continuity_uv_nonfinite".into());
        }
        match signed_double_area.partial_cmp(&0.0) {
            Some(std::cmp::Ordering::Greater) => winding_positive_count += 1,
            Some(std::cmp::Ordering::Less) => winding_negative_count += 1,
            _ => winding_zero_count += 1,
        }

        let u_fraction = ((observation.uv_centroid[0] - u_domain.0) / u_span).clamp(0.0, 1.0);
        let u_bin = ((u_fraction * ORIENTATION_U_BIN_COUNT as f64).floor() as usize)
            .min(ORIENTATION_U_BIN_COUNT - 1);
        u_bins[u_bin].add(
            observation.alignment,
            observation.parameter_normal.is_some(),
        )?;

        for (first, second) in [(a, b), (b, c), (c, a)] {
            let key = (first.min(second), first.max(second));
            let direction = first < second;
            let edge = edge_uses.entry(key).or_default();
            match edge.count {
                0 => {
                    edge.first_triangle = Some(triangle_index);
                    edge.first_direction = Some(direction);
                }
                1 => {
                    edge.second_triangle = Some(triangle_index);
                    edge.second_direction = Some(direction);
                }
                _ => {}
            }
            edge.count = edge
                .count
                .checked_add(1)
                .ok_or("trim_candidate_orientation_continuity_edge_count_overflow")?;
        }
    }

    let mut boundary_edge_count = 0usize;
    let mut interior_edge_count = 0usize;
    let mut nonmanifold_edge_count = 0usize;
    let mut inconsistent_interior_edge_winding_count = 0usize;
    let mut geometric_normal_dot = DotStatistics::default();
    let mut parameter_normal_dot = DotStatistics::default();
    for (edge_key, edge) in &edge_uses {
        match edge.count {
            1 => boundary_edge_count += 1,
            2 => {
                interior_edge_count += 1;
                if edge.first_direction == edge.second_direction {
                    inconsistent_interior_edge_winding_count += 1;
                }
                let first = edge
                    .first_triangle
                    .ok_or("trim_candidate_orientation_continuity_first_triangle_missing")?;
                let second = edge
                    .second_triangle
                    .ok_or("trim_candidate_orientation_continuity_second_triangle_missing")?;
                let edge_u = (uv_vertices[edge_key.0][0] + uv_vertices[edge_key.1][0]) * 0.5;
                if !edge_u.is_finite() {
                    return Err("trim_candidate_orientation_continuity_edge_u_nonfinite".into());
                }
                let edge_u_bin = ((((edge_u - u_domain.0) / u_span).clamp(0.0, 1.0)
                    * ORIENTATION_U_BIN_COUNT as f64)
                    .floor() as usize)
                    .min(ORIENTATION_U_BIN_COUNT - 1);
                for statistics in [
                    &mut geometric_normal_dot,
                    &mut geometric_normal_dot_by_u[edge_u_bin],
                ] {
                    statistics.add(
                        observations[first].triangle_normal,
                        observations[second].triangle_normal,
                    )?;
                }
                for statistics in [
                    &mut parameter_normal_dot,
                    &mut parameter_normal_dot_by_u[edge_u_bin],
                ] {
                    statistics.add(
                        observations[first].parameter_normal,
                        observations[second].parameter_normal,
                    )?;
                }
            }
            _ => nonmanifold_edge_count += 1,
        }
    }

    let u_bin_reports = u_bins
        .iter()
        .enumerate()
        .map(|(u_bin, statistics)| {
            let u_min = u_domain.0
                + (u_domain.1 - u_domain.0) * u_bin as f64 / ORIENTATION_U_BIN_COUNT as f64;
            let u_max = u_domain.0
                + (u_domain.1 - u_domain.0) * (u_bin + 1) as f64 / ORIENTATION_U_BIN_COUNT as f64;
            json!({
                "uBin": u_bin,
                "uRange": [u_min, u_max],
                "parameterNormalAlignment": statistics.report()
            })
        })
        .collect::<Vec<_>>();
    let adjacent_normal_u_bin_reports = (0..ORIENTATION_U_BIN_COUNT)
        .map(|u_bin| {
            json!({
                "uBin": u_bin,
                "adjacentTrianglePairCount": geometric_normal_dot_by_u[u_bin].compared_pair_count
                    + geometric_normal_dot_by_u[u_bin].undefined_pair_count,
                "geometricTriangleNormalDot": geometric_normal_dot_by_u[u_bin].report(),
                "parameterNormalDot": parameter_normal_dot_by_u[u_bin].report()
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "schema": "tokito.step-trim-candidate-orientation-continuity.v1",
        "diagnosticOnly": true,
        "admissionAffecting": false,
        "sampleScope": "final_refinement_level_only",
        "finalTriangleCount": triangles.len(),
        "uDomain": [u_domain.0, u_domain.1],
        "cachedObservationCountAcrossRefinementLevels": cached_observation_count,
        "observationCacheLimit": MAX_ORIENTATION_OBSERVATIONS,
        "uvWindingSigns": {
            "positive": winding_positive_count,
            "negative": winding_negative_count,
            "zero": winding_zero_count
        },
        "parameterNormalAlignmentByU8": u_bin_reports,
        "adjacentNormalContinuityByU8": adjacent_normal_u_bin_reports,
        "edgeAdjacency": {
            "uniqueEdgeCount": edge_uses.len(),
            "boundaryEdgeCount": boundary_edge_count,
            "interiorEdgeCount": interior_edge_count,
            "nonmanifoldEdgeCount": nonmanifold_edge_count,
            "inconsistentInteriorEdgeWindingCount": inconsistent_interior_edge_winding_count,
            "adjacentTrianglePairCount": interior_edge_count,
            "geometricTriangleNormalDot": geometric_normal_dot.report(),
            "parameterNormalDot": parameter_normal_dot.report()
        }
    }))
}

struct ParsedCurve {
    curve: kernel::BSplineCurve,
    domain: (f64, f64),
    breaks: Vec<f64>,
    rational: bool,
    control_point_count: usize,
}

fn parse_curve(step: &StepFile, id: u64) -> Result<ParsedCurve, String> {
    let source = entity(step, id, "B_SPLINE_CURVE")?;
    if !source.is_a("B_SPLINE_CURVE_WITH_KNOTS") {
        return Err(format!(
            "unsupported_edge_curve_requires_bspline_with_knots:{id}"
        ));
    }
    let degree = required_integer(source, "degree", 32)?;
    let references = source
        .list("control_points_list")
        .ok_or("missing_curve_control_points")?;
    if references.len() < 2 || references.len() > MAX_CONTROL_POINTS {
        return Err("invalid_curve_control_point_count".into());
    }
    let mut controls = Vec::with_capacity(references.len());
    for reference in references {
        controls.push(point(
            step,
            reference
                .as_reference()
                .ok_or("curve_control_point_must_be_reference")?,
        )?);
    }
    let knots = number_list(source, "knots", 512)?;
    let multiplicities = integer_list(source, "knot_multiplicities", 512)?;
    let weights = if source.is_a("RATIONAL_B_SPLINE_CURVE") {
        let values = number_list(source, "weights_data", MAX_CONTROL_POINTS)?;
        if values.len() != controls.len() || values.iter().any(|weight| *weight <= 0.) {
            return Err("invalid_curve_weights".into());
        }
        Some(values)
    } else {
        None
    };
    let curve = kernel::BSplineCurve::new(
        degree,
        &controls,
        weights.as_deref(),
        &knots,
        &multiplicities,
    )
    .map_err(|error| format!("invalid_curve_definition:{error}"))?;
    let domain = curve.domain();
    let breaks = curve.breaks();
    if breaks.len() < 2 || breaks.len() > MAX_CURVE_SPANS + 1 {
        return Err("invalid_or_excessive_curve_span_count".into());
    }
    let rational = weights.is_some();
    Ok(ParsedCurve {
        curve,
        domain,
        breaks,
        rational,
        control_point_count: references.len(),
    })
}

fn preflight_curve_control_point_count(step: &StepFile, id: u64) -> Result<usize, String> {
    let source = entity(step, id, "B_SPLINE_CURVE")?;
    if !source.is_a("B_SPLINE_CURVE_WITH_KNOTS") {
        return Err(format!(
            "unsupported_edge_curve_requires_bspline_with_knots:{id}"
        ));
    }
    let count = source
        .list("control_points_list")
        .ok_or("missing_curve_control_points")?
        .len();
    if !(2..=MAX_CONTROL_POINTS).contains(&count) {
        return Err("invalid_curve_control_point_count".into());
    }
    Ok(count)
}

fn checked_edge_curve_control_point_total(current: usize, next: usize) -> Result<usize, String> {
    current
        .checked_add(next)
        .filter(|total| *total <= MAX_TOTAL_EDGE_CURVE_CONTROL_POINTS)
        .ok_or_else(|| "edge_curve_control_point_storage_budget_exceeded".into())
}

fn sample_curve(
    curve: &kernel::BSplineCurve,
    breaks: &[f64],
    maximum_total_samples: usize,
) -> Result<Vec<(f64, P3)>, String> {
    let expected = breaks
        .len()
        .saturating_sub(1)
        .checked_mul(SAMPLES_PER_CURVE_SPAN)
        .and_then(|count| count.checked_add(1))
        .ok_or("curve_sample_count_overflow")?;
    if expected > maximum_total_samples {
        return Err("boundary_sample_budget_exceeded".into());
    }
    let mut samples = Vec::with_capacity(expected);
    for span in breaks.windows(2) {
        let (start, end) = (span[0], span[1]);
        if !start.is_finite() || !end.is_finite() || end <= start {
            return Err("invalid_curve_knot_span".into());
        }
        for index in 0..SAMPLES_PER_CURVE_SPAN {
            let t = start + (end - start) * (index as f64 / SAMPLES_PER_CURVE_SPAN as f64);
            let point = curve.point_at(t);
            if point
                .iter()
                .any(|value| !value.is_finite() || value.abs() > 1e12)
            {
                return Err("nonfinite_or_unbounded_curve_sample".into());
            }
            samples.push((t, point));
        }
    }
    let end = *breaks.last().ok_or("missing_curve_domain_end")?;
    let point = curve.point_at(end);
    if point
        .iter()
        .any(|value| !value.is_finite() || value.abs() > 1e12)
    {
        return Err("nonfinite_or_unbounded_curve_sample".into());
    }
    samples.push((end, point));
    Ok(samples)
}

fn axis_seeds(breaks: &[f64], subdivisions: usize) -> Result<Vec<f64>, String> {
    if breaks.len() < 2 || breaks.len() > MAX_AXIS_SEEDS {
        return Err("projection_axis_knot_span_limit".into());
    }
    let start = breaks[0];
    let end = *breaks.last().ok_or("missing_projection_domain")?;
    if !start.is_finite() || !end.is_finite() || end <= start {
        return Err("invalid_projection_domain".into());
    }
    let mut values = (0..=subdivisions)
        .map(|index| start + (end - start) * index as f64 / subdivisions as f64)
        .chain(breaks.iter().copied())
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    values.dedup_by(|a, b| *a == *b);
    if values.len() > MAX_AXIS_SEEDS {
        return Err("projection_axis_seed_limit".into());
    }
    Ok(values)
}

fn shared_kernel_inverse_axis_sample_count(breaks: &[f64]) -> Result<usize, String> {
    if breaks.len() < 2 || breaks.len() > MAX_AXIS_SEEDS {
        return Err("shared_kernel_inverse_knot_span_limit".into());
    }
    let spans = breaks.len() - 1;
    let per_span = SHARED_KERNEL_INVERSE_SAMPLES_PER_SPAN.min(
        SHARED_KERNEL_INVERSE_MAX_SAMPLES_PER_AXIS
            .div_ceil(spans)
            .max(1),
    );
    spans
        .checked_mul(per_span)
        .and_then(|samples| samples.checked_add(1))
        .ok_or_else(|| "shared_kernel_inverse_sample_count_overflow".into())
}

fn shared_kernel_inverse_distance_check_count(
    boundary_samples: usize,
    u_breaks: &[f64],
    v_breaks: &[f64],
) -> Result<(usize, usize, usize), String> {
    let u_count = shared_kernel_inverse_axis_sample_count(u_breaks)?;
    let v_count = shared_kernel_inverse_axis_sample_count(v_breaks)?;
    let grid = u_count
        .checked_mul(v_count)
        .ok_or_else(|| "shared_kernel_inverse_grid_size_overflow".to_string())?;
    let comparisons = boundary_samples
        .checked_mul(grid)
        .ok_or_else(|| "shared_kernel_inverse_work_estimate_overflow".to_string())?;
    if comparisons > MAX_SHARED_KERNEL_INVERSE_DISTANCE_CHECKS {
        return Err("shared_kernel_inverse_work_budget_exceeded".into());
    }
    Ok((u_count, v_count, comparisons))
}

fn shared_kernel_grid_control_point_evaluation_estimate(
    u_samples: usize,
    v_samples: usize,
    control_point_count: usize,
) -> Result<usize, String> {
    u_samples
        .checked_mul(v_samples)
        .and_then(|grid_samples| grid_samples.checked_mul(control_point_count))
        .filter(|count| *count <= MAX_SHARED_KERNEL_GRID_CONTROL_POINT_EVALUATIONS)
        .ok_or_else(|| "shared_kernel_surface_construction_work_budget_exceeded".into())
}

fn shared_kernel_inverse_control_point_evaluation_estimate(
    boundary_samples: usize,
    control_point_count: usize,
    construction_estimate: usize,
) -> Result<usize, String> {
    let per_sample = MAX_PROJECTION_ITERATIONS
        .checked_mul(SHARED_KERNEL_INVERSE_SURFACE_EVALUATIONS_PER_ITERATION)
        .and_then(|count| count.checked_add(1))
        .ok_or_else(|| "shared_kernel_inverse_iteration_estimate_overflow".to_string())?;
    boundary_samples
        .checked_mul(per_sample)
        .and_then(|count| count.checked_mul(control_point_count))
        .and_then(|count| count.checked_add(construction_estimate))
        .filter(|count| *count <= MAX_SHARED_KERNEL_INVERSE_CONTROL_POINT_EVALUATIONS)
        .ok_or_else(|| "shared_kernel_inverse_control_point_work_budget_exceeded".to_string())
}

fn finite_difference_bounds(
    value: f64,
    step: f64,
    domain: (f64, f64),
    periodic: bool,
) -> Option<(f64, f64)> {
    if !value.is_finite() || !step.is_finite() || step <= 0. {
        return None;
    }
    let bounds = if periodic {
        (value - step, value + step)
    } else {
        ((value - step).max(domain.0), (value + step).min(domain.1))
    };
    (bounds.0.is_finite() && bounds.1.is_finite() && bounds.1 > bounds.0).then_some(bounds)
}

fn eval_surface(
    definition: &SurfaceDefinition,
    uv: [f64; 2],
    evaluations: &mut usize,
    control_point_evaluations: &mut usize,
) -> Result<P3, String> {
    *evaluations = evaluations
        .checked_add(1)
        .filter(|count| *count <= MAX_SURFACE_EVALUATIONS)
        .ok_or("multistart_surface_evaluation_budget_exceeded")?;
    *control_point_evaluations = control_point_evaluations
        .checked_add(definition.control_point_count)
        .filter(|count| *count <= MAX_DIAGNOSTIC_CONTROL_POINT_EVALUATIONS)
        .ok_or("multistart_control_point_evaluation_budget_exceeded")?;
    let (u0, u1) = definition.u_domain;
    let (v0, v1) = definition.v_domain;
    if definition.u_period.is_some() {
        return Err("u_periodic_multistart_not_supported".into());
    }
    let u = uv[0].clamp(u0, u1);
    let v_span = v1 - v0;
    if !v_span.is_finite() || v_span <= 0. {
        return Err("invalid_projection_v_domain".into());
    }
    let v = if definition.v_period.is_some() {
        v0 + (uv[1] - v0).rem_euclid(v_span)
    } else {
        uv[1].clamp(v0, v1)
    };
    let point = definition.evaluator.point_at([u, v]);
    if point.iter().any(|coordinate| !coordinate.is_finite()) {
        return Err("nonfinite_multistart_surface_evaluation".into());
    }
    Ok(point)
}

#[derive(Clone, Copy, Debug)]
struct ProjectionCandidate {
    uv: [f64; 2],
    residual: f64,
    converged: bool,
    termination: &'static str,
    iterations: usize,
    stationarity: ProjectionStationarity,
    residual_tolerance: f64,
}

#[derive(Clone, Copy, Debug)]
struct ProjectionStationarity {
    status: &'static str,
    tangent_residual_cosines: Option<[f64; 2]>,
    normalized_jacobian_determinant: Option<f64>,
}

impl ProjectionCandidate {
    fn solver_diagnostic(self) -> Json {
        json!({
            "termination": self.termination,
            "iterations": self.iterations,
            "iterationLimit": MAX_PROJECTION_ITERATIONS,
            "residualToleranceSourceUnits": self.residual_tolerance,
            "residualToleranceMet": self.residual <= self.residual_tolerance,
            "stationarityObservation": self.stationarity.status,
            "tangentResidualCosines": self.stationarity.tangent_residual_cosines,
            "tangentOrthogonalityObservationThreshold": PROJECTION_TANGENT_ORTHOGONALITY_TOLERANCE,
            "normalizedJacobianDeterminant": self.stationarity.normalized_jacobian_determinant,
            "globalMinimumProven": false,
            "authorizesGeometryAdmission": false
        })
    }
}

// Observe the selected final point only. A small gradient at an interior point
// is not a global-minimum proof; at a domain boundary we make no KKT claim.
// These extra evaluations are charged to the existing hard work budgets.
fn observe_projection_stationarity(
    definition: &SurfaceDefinition,
    candidate: &ProjectionCandidate,
    target: P3,
    evaluations: &mut usize,
    control_point_evaluations: &mut usize,
) -> Result<ProjectionStationarity, String> {
    let unavailable = |status| ProjectionStationarity {
        status,
        tangent_residual_cosines: None,
        normalized_jacobian_determinant: None,
    };
    if candidate.residual <= candidate.residual_tolerance {
        return Ok(unavailable("not_evaluated_residual_met"));
    }
    let uv = candidate.uv;
    let step = |domain: (f64, f64)| (domain.1 - domain.0) * 1e-6;
    let Some((um, up)) =
        finite_difference_bounds(uv[0], step(definition.u_domain), definition.u_domain, false)
    else {
        return Ok(unavailable("derivative_bounds_invalid"));
    };
    let Some((vm, vp)) = finite_difference_bounds(
        uv[1],
        step(definition.v_domain),
        definition.v_domain,
        definition.v_period.is_some(),
    ) else {
        return Ok(unavailable("derivative_bounds_invalid"));
    };
    let mut sample = |p| eval_surface(definition, p, evaluations, control_point_evaluations);
    let du = scale(
        sub(sample([up, uv[1]])?, sample([um, uv[1]])?),
        1. / (up - um),
    );
    let dv = scale(
        sub(sample([uv[0], vp])?, sample([uv[0], vm])?),
        1. / (vp - vm),
    );
    let residual = sub(sample(uv)?, target);
    let length = |v: P3| v[0].hypot(v[1]).hypot(v[2]);
    let lengths = [length(du), length(dv), length(residual)];
    if lengths.iter().any(|n| !n.is_finite() || *n <= 0.) {
        return Ok(unavailable("singular_or_nonfinite_derivatives"));
    }
    let du = scale(du, 1. / lengths[0]);
    let dv = scale(dv, 1. / lengths[1]);
    let residual = scale(residual, 1. / lengths[2]);
    let cosines = [dot(du, residual), dot(dv, residual)];
    let tangent_cosine = dot(du, dv);
    let determinant = (1. - tangent_cosine * tangent_cosine).clamp(0., 1.);
    if !determinant.is_finite() || cosines.iter().any(|x| !x.is_finite()) {
        return Ok(unavailable("singular_or_nonfinite_derivatives"));
    }
    let boundary = uv[0] <= definition.u_domain.0
        || uv[0] >= definition.u_domain.1
        || (definition.v_period.is_none()
            && (uv[1] <= definition.v_domain.0 || uv[1] >= definition.v_domain.1));
    let status = if determinant <= 1e-14 {
        "singular_jacobian"
    } else if boundary {
        "domain_boundary_no_stationarity_claim"
    } else if cosines
        .iter()
        .all(|x| x.abs() <= PROJECTION_TANGENT_ORTHOGONALITY_TOLERANCE)
    {
        "interior_tangent_orthogonality_observed"
    } else {
        "tangent_orthogonality_not_observed"
    };
    Ok(ProjectionStationarity {
        status,
        tangent_residual_cosines: Some(cosines),
        normalized_jacobian_determinant: Some(determinant),
    })
}

fn multistart_inverse(
    definition: &SurfaceDefinition,
    target: P3,
    seed_u: &[f64],
    seed_v: &[f64],
    evaluations: &mut usize,
    control_point_evaluations: &mut usize,
) -> Result<(ProjectionCandidate, usize), String> {
    let u_span = definition.u_domain.1 - definition.u_domain.0;
    let v_span = definition.v_domain.1 - definition.v_domain.0;
    let h_u = (u_span * 1e-6).max(f64::EPSILON * u_span * 32.);
    let h_v = (v_span * 1e-6).max(f64::EPSILON * v_span * 32.);
    let mut best: Option<ProjectionCandidate> = None;
    let mut starts = 0usize;
    for &seed_u_value in seed_u {
        for &seed_v_value in seed_v {
            starts += 1;
            let mut uv = [seed_u_value, seed_v_value];
            let mut point = eval_surface(definition, uv, evaluations, control_point_evaluations)?;
            let mut residual_vector = sub(point, target);
            let mut error = dot(residual_vector, residual_vector);
            let mut converged = false;
            let mut termination = "iteration_limit";
            let mut iterations = 0;
            for _ in 0..MAX_PROJECTION_ITERATIONS {
                iterations += 1;
                let Some((um, up)) =
                    finite_difference_bounds(uv[0], h_u, definition.u_domain, false)
                else {
                    termination = "derivative_bounds_invalid";
                    break;
                };
                let Some((vm, vp)) = finite_difference_bounds(
                    uv[1],
                    h_v,
                    definition.v_domain,
                    definition.v_period.is_some(),
                ) else {
                    termination = "derivative_bounds_invalid";
                    break;
                };
                let pm_u = eval_surface(
                    definition,
                    [um, uv[1]],
                    evaluations,
                    control_point_evaluations,
                )?;
                let pp_u = eval_surface(
                    definition,
                    [up, uv[1]],
                    evaluations,
                    control_point_evaluations,
                )?;
                let pm_v = eval_surface(
                    definition,
                    [uv[0], vm],
                    evaluations,
                    control_point_evaluations,
                )?;
                let pp_v = eval_surface(
                    definition,
                    [uv[0], vp],
                    evaluations,
                    control_point_evaluations,
                )?;
                let du = scale(sub(pp_u, pm_u), 1. / (up - um));
                let dv = scale(sub(pp_v, pm_v), 1. / (vp - vm));
                let a = dot(du, du);
                let b = dot(du, dv);
                let c = dot(dv, dv);
                let g0 = dot(du, residual_vector);
                let g1 = dot(dv, residual_vector);
                let determinant = a * c - b * b;
                if !determinant.is_finite()
                    || determinant <= (a * c).abs() * 1e-14
                    || a <= 0.
                    || c <= 0.
                {
                    termination = "singular_jacobian";
                    break;
                }
                let step_u = (-g0 * c + b * g1) / determinant;
                let step_v = (b * g0 - a * g1) / determinant;
                if !step_u.is_finite() || !step_v.is_finite() {
                    termination = "nonfinite_step";
                    break;
                }
                let mut accepted = None;
                let mut factor = 1.;
                for _ in 0..8 {
                    let candidate_uv = [
                        (uv[0] + factor * step_u)
                            .clamp(definition.u_domain.0, definition.u_domain.1),
                        if definition.v_period.is_some() {
                            uv[1] + factor * step_v
                        } else {
                            (uv[1] + factor * step_v)
                                .clamp(definition.v_domain.0, definition.v_domain.1)
                        },
                    ];
                    let candidate_point = eval_surface(
                        definition,
                        candidate_uv,
                        evaluations,
                        control_point_evaluations,
                    )?;
                    let candidate_residual = sub(candidate_point, target);
                    let candidate_error = dot(candidate_residual, candidate_residual);
                    if candidate_error < error {
                        accepted = Some((
                            candidate_uv,
                            candidate_point,
                            candidate_residual,
                            candidate_error,
                        ));
                        break;
                    }
                    factor *= 0.5;
                }
                let Some((candidate_uv, candidate_point, candidate_residual, candidate_error)) =
                    accepted
                else {
                    converged = error.sqrt() <= definition.control_net_diagonal * 1e-10;
                    termination = if converged {
                        "residual_met"
                    } else {
                        "no_decreasing_step"
                    };
                    break;
                };
                let previous_error = error;
                uv = candidate_uv;
                point = candidate_point;
                residual_vector = candidate_residual;
                error = candidate_error;
                if error.sqrt() <= definition.control_net_diagonal * 1e-10 {
                    converged = true;
                    termination = "residual_met";
                    break;
                }
                let improvement = previous_error - error;
                let stagnation_tolerance = previous_error.max(f64::MIN_POSITIVE) * 1e-14;
                if improvement <= stagnation_tolerance {
                    // A small absolute improvement is not proof of convergence:
                    // the residual must satisfy the geometric tolerance above.
                    termination = "stagnation";
                    break;
                }
            }
            let residual = distance(point, target);
            if !residual.is_finite() {
                return Err("nonfinite_multistart_projection_residual".into());
            }
            let candidate = ProjectionCandidate {
                uv,
                residual,
                converged,
                termination,
                iterations,
                residual_tolerance: definition.control_net_diagonal * 1e-10,
                stationarity: ProjectionStationarity {
                    status: "not_evaluated",
                    tangent_residual_cosines: None,
                    normalized_jacobian_determinant: None,
                },
            };
            if best.is_none_or(|current| candidate.residual < current.residual) {
                best = Some(candidate);
            }
        }
    }
    let mut best = best.ok_or_else(|| "multistart_projection_had_no_seed".to_string())?;
    best.stationarity = observe_projection_stationarity(
        definition,
        &best,
        target,
        evaluations,
        control_point_evaluations,
    )?;
    Ok((best, starts))
}

fn scale(point: P3, factor: f64) -> P3 {
    point.map(|coordinate| coordinate * factor)
}

fn unwrap_parameter(raw: f64, previous: f64, period: Option<f64>) -> Result<(f64, f64), String> {
    let Some(period) = period else {
        return Ok((raw, 0.));
    };
    if !period.is_finite() || period <= 0. {
        return Err("invalid_parameter_period".into());
    }
    let shift = ((previous - raw) / period).round();
    if !shift.is_finite() || shift.abs() > MAX_BRANCH_SHIFT {
        return Err("parameter_branch_shift_limit".into());
    }
    let value = raw + shift * period;
    if !value.is_finite() {
        return Err("nonfinite_unwrapped_parameter".into());
    }
    Ok((value, shift))
}

/// Isolated source-pinned physical refinement experiment. The mesh samples the
/// supporting surface and does not replace or qualify source trim curves.
pub(crate) fn inspect_physical_refinement(
    step: &StepFile,
    face_id: u64,
    tolerance: f64,
    metric_preconditioning: bool,
) -> Result<Json, String> {
    let mut report = inspect(step, face_id)?;
    let surface_id = report["surfaceId"]
        .as_u64()
        .ok_or("physical_research_surface_id_missing")?;
    let surface = parse_surface(step, surface_id)?;
    let face_sense = report["faceSameSense"]
        .as_bool()
        .ok_or("physical_research_face_sense_missing")?;
    let rows = report["trimCandidate"]["boundarySamples"]
        .as_array()
        .ok_or("physical_research_source_boundary_missing")?;
    let input = rows
        .iter()
        .map(|row| {
            let uv = row["candidateUv"]
                .as_array()
                .filter(|v| v.len() == 2)
                .ok_or("physical_research_uv_missing")?;
            Ok([
                uv[0].as_f64().ok_or("physical_research_uv_invalid")?,
                uv[1].as_f64().ok_or("physical_research_uv_invalid")?,
            ])
        })
        .collect::<Result<Vec<_>, String>>()?;
    let evaluations = std::cell::RefCell::new(0);
    let control_evaluations = std::cell::RefCell::new(0);
    let evaluate = |uv: [f64; 2]| {
        if uv.iter().any(|x| !x.is_finite())
            || uv[0] < surface.u_domain.0
            || uv[0] > surface.u_domain.1
        {
            return Err("physical_research_uv_outside_source_domain".into());
        }
        if surface.v_period.is_none() && (uv[1] < surface.v_domain.0 || uv[1] > surface.v_domain.1)
        {
            return Err("physical_research_uv_outside_source_domain".into());
        }
        eval_surface(
            &surface,
            uv,
            &mut evaluations.borrow_mut(),
            &mut control_evaluations.borrow_mut(),
        )
    };
    let orientation = |uv: [[f64; 2]; 3], points: [P3; 3]| -> Result<f64, String> {
        let center: [f64; 2] = std::array::from_fn(|i| (uv[0][i] + uv[1][i] + uv[2][i]) / 3.);
        let Some((um, up)) = finite_difference_bounds(
            center[0],
            (surface.u_domain.1 - surface.u_domain.0) * 1e-6,
            surface.u_domain,
            false,
        ) else {
            return Ok(0.);
        };
        let Some((vm, vp)) = finite_difference_bounds(
            center[1],
            (surface.v_domain.1 - surface.v_domain.0) * 1e-6,
            surface.v_domain,
            surface.v_period.is_some(),
        ) else {
            return Ok(0.);
        };
        let du = scale(
            sub(evaluate([up, center[1]])?, evaluate([um, center[1]])?),
            1. / (up - um),
        );
        let dv = scale(
            sub(evaluate([center[0], vp])?, evaluate([center[0], vm])?),
            1. / (vp - vm),
        );
        Ok(oriented_triangle_normal_alignment(
            points, du, dv, face_sense,
        ))
    };
    let metric = if metric_preconditioning {
        let anchor: [f64; 2] = std::array::from_fn(|axis| {
            let lo = input.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
            let hi = input
                .iter()
                .map(|p| p[axis])
                .fold(f64::NEG_INFINITY, f64::max);
            (lo + hi) * 0.5
        });
        let hu = (surface.u_domain.1 - surface.u_domain.0) * 1e-6;
        let hv = (surface.v_domain.1 - surface.v_domain.0) * 1e-6;
        let (um, up) = finite_difference_bounds(anchor[0], hu, surface.u_domain, false)
            .ok_or("physical_metric_u_derivative_bounds_invalid")?;
        let (vm, vp) =
            finite_difference_bounds(anchor[1], hv, surface.v_domain, surface.v_period.is_some())
                .ok_or("physical_metric_v_derivative_bounds_invalid")?;
        let du = scale(
            sub(evaluate([up, anchor[1]])?, evaluate([um, anchor[1]])?),
            1. / (up - um),
        );
        let dv = scale(
            sub(evaluate([anchor[0], vp])?, evaluate([anchor[0], vm])?),
            1. / (vp - vm),
        );
        Some(crate::physical_surface_refinement::ChartMetric::from_tangents(anchor, du, dv)?)
    } else {
        None
    };
    let candidate = crate::physical_surface_refinement::refine(
        &input,
        tolerance,
        5f64.to_radians().cos(),
        metric,
        evaluate,
        orientation,
    );
    report["physicalRefinementCandidate"] = match candidate {
        Ok(value) => value,
        Err(error) => {
            json!({"status":"refused","error":error,"geometryQualified":false,"admitted":false,"exportsProduced":[]})
        }
    };
    report["physicalRefinementCandidate"]["sourceCurveToSupportSurfaceGapResolved"] = json!(false);
    report["physicalRefinementCandidate"]["surfaceEvaluationCount"] = json!(*evaluations.borrow());
    report["physicalRefinementCandidate"]["controlPointEvaluationCount"] =
        json!(*control_evaluations.borrow());
    Ok(report)
}

/// Inspect one source ADVANCED_FACE whose surface and four-dimensional
/// source trims are available as B-spline entities.
pub fn inspect(step: &StepFile, face_id: u64) -> Result<Json, String> {
    if face_id == 0 {
        return Err("positive_diagnostic_face_required".into());
    }
    let face = entity(step, face_id, "ADVANCED_FACE")?;
    let face_sense = face
        .logical("same_sense")
        .ok_or("unknown_face_surface_sense")?;
    let bounds = face.list("bounds").ok_or("missing_face_bounds")?;
    if bounds.len() != 1 {
        return Err("diagnostic_requires_one_face_bound".into());
    }
    let bound_id = bounds[0]
        .as_reference()
        .ok_or("invalid_face_bound_reference")?;
    let bound = entity(step, bound_id, "FACE_OUTER_BOUND")?;
    let bound_sense = bound
        .logical("orientation")
        .ok_or("unknown_face_bound_orientation")?;
    let loop_id = bound.reference("bound").ok_or("missing_edge_loop")?;
    let loop_entity = entity(step, loop_id, "EDGE_LOOP")?;
    let oriented_ids = loop_entity.list("edge_list").ok_or("missing_edge_list")?;
    if oriented_ids.len() < 3 || oriented_ids.len() > MAX_FACE_EDGES {
        return Err("invalid_or_excessive_face_edge_count".into());
    }
    let surface_id = face
        .reference("face_geometry")
        .ok_or("missing_face_surface")?;
    let surface = parse_surface(step, surface_id)?;
    let surface_report = surface.report.clone();
    let u_period = surface.u_period;
    let v_period = surface.v_period;
    let seed_u = axis_seeds(&surface.u_breaks, MULTISTART_SUBDIVISIONS)?;
    let seed_v = axis_seeds(&surface.v_breaks, MULTISTART_SUBDIVISIONS)?;

    struct EdgeData {
        oriented_id: u64,
        edge_id: u64,
        curve_id: u64,
        curve_same_sense: bool,
        oriented_sense: bool,
        loop_sense: bool,
        start_vertex: u64,
        end_vertex: u64,
        curve_domain: (f64, f64),
        curve_breaks: Vec<f64>,
        curve_control_point_count: usize,
        curve_type: &'static str,
        curve: kernel::BSplineCurve,
    }

    let mut edges = Vec::with_capacity(oriented_ids.len());
    let mut seen = std::collections::BTreeSet::new();
    let mut total_edge_curve_control_points = 0usize;
    for item in oriented_ids {
        let oriented_id = item
            .as_reference()
            .ok_or("invalid_oriented_edge_reference")?;
        if !seen.insert(oriented_id) {
            return Err("duplicate_oriented_edge".into());
        }
        let oriented = entity(step, oriented_id, "ORIENTED_EDGE")?;
        let oriented_sense = oriented
            .logical("orientation")
            .ok_or("unknown_oriented_edge_sense")?;
        let edge_id = oriented
            .reference("edge_element")
            .ok_or("missing_edge_element")?;
        let edge = entity(step, edge_id, "EDGE_CURVE")?;
        let curve_same_sense = edge
            .logical("same_sense")
            .ok_or("unknown_edge_curve_sense")?;
        let start_vertex = edge.reference("edge_start").ok_or("missing_edge_start")?;
        let end_vertex = edge.reference("edge_end").ok_or("missing_edge_end")?;
        let curve_id = edge
            .reference("edge_geometry")
            .ok_or("missing_edge_curve_geometry")?;
        let preflight_control_point_count = preflight_curve_control_point_count(step, curve_id)?;
        total_edge_curve_control_points = checked_edge_curve_control_point_total(
            total_edge_curve_control_points,
            preflight_control_point_count,
        )?;
        let parsed_curve = parse_curve(step, curve_id)?;
        if parsed_curve.control_point_count != preflight_control_point_count {
            return Err("edge_curve_control_point_preflight_mismatch".into());
        }
        let curve_type = if parsed_curve.rational {
            "RATIONAL_B_SPLINE_CURVE_WITH_KNOTS"
        } else {
            "B_SPLINE_CURVE_WITH_KNOTS"
        };
        edges.push(EdgeData {
            oriented_id,
            edge_id,
            curve_id,
            curve_same_sense,
            oriented_sense,
            // A false bound reverses both the sequence and each edge's
            // effective traversal. This identity keeps endpoint chaining explicit.
            loop_sense: oriented_sense == bound_sense,
            start_vertex,
            end_vertex,
            curve_domain: parsed_curve.domain,
            curve_breaks: parsed_curve.breaks,
            curve_control_point_count: parsed_curve.control_point_count,
            curve_type,
            curve: parsed_curve.curve,
        });
    }
    let edge_curve_control_point_evaluations = edges.iter().try_fold(0usize, |total, edge| {
        let sample_count = edge
            .curve_breaks
            .len()
            .saturating_sub(1)
            .checked_mul(SAMPLES_PER_CURVE_SPAN)
            .and_then(|count| count.checked_add(1))
            .ok_or_else(|| "edge_curve_work_estimate_overflow".to_string())?;
        total
            .checked_add(
                sample_count
                    .checked_mul(edge.curve_control_point_count)
                    .ok_or_else(|| "edge_curve_work_estimate_overflow".to_string())?,
            )
            .ok_or_else(|| "edge_curve_work_estimate_overflow".to_string())
    })?;
    if edge_curve_control_point_evaluations > MAX_EDGE_CURVE_CONTROL_POINT_EVALUATIONS {
        return Err("edge_curve_evaluation_work_budget_exceeded".into());
    }
    if !bound_sense {
        edges.reverse();
    }

    let mut boundary_samples = Vec::<(usize, usize, f64, P3)>::new();
    let mut edge_reports = Vec::with_capacity(edges.len());
    let mut previous_vertex = None;
    let mut first_vertex = None;
    let mut max_endpoint_residual: f64 = 0.;
    let mut total_sample_budget = MAX_BOUNDARY_SAMPLES;
    for (loop_index, edge) in edges.iter().enumerate() {
        let (loop_start, loop_end) = if edge.loop_sense {
            (edge.start_vertex, edge.end_vertex)
        } else {
            (edge.end_vertex, edge.start_vertex)
        };
        if previous_vertex.is_some_and(|previous| previous != loop_start) {
            return Err("source_oriented_edge_chain_gap".into());
        }
        if first_vertex.is_none() {
            first_vertex = Some(loop_start);
        }
        previous_vertex = Some(loop_end);

        let mut samples = sample_curve(&edge.curve, &edge.curve_breaks, total_sample_budget)?;
        total_sample_budget = total_sample_budget
            .checked_sub(samples.len())
            .ok_or("boundary_sample_budget_exceeded")?;
        let natural_start = samples.first().ok_or("empty_curve_sample_run")?.1;
        let natural_end = samples.last().ok_or("empty_curve_sample_run")?.1;
        let (expected_natural_start, expected_natural_end) = if edge.curve_same_sense {
            (
                vertex(step, edge.start_vertex)?,
                vertex(step, edge.end_vertex)?,
            )
        } else {
            (
                vertex(step, edge.end_vertex)?,
                vertex(step, edge.start_vertex)?,
            )
        };
        let start_residual = distance(natural_start, expected_natural_start);
        let end_residual = distance(natural_end, expected_natural_end);
        if !start_residual.is_finite() || !end_residual.is_finite() {
            return Err("nonfinite_edge_endpoint_residual".into());
        }
        max_endpoint_residual = max_endpoint_residual.max(start_residual).max(end_residual);
        let curve_follows_loop = edge.curve_same_sense == edge.loop_sense;
        if !curve_follows_loop {
            samples.reverse();
        }
        for (sample_index, (parameter, point)) in samples.into_iter().enumerate() {
            boundary_samples.push((loop_index, sample_index, parameter, point));
            if boundary_samples.len() > MAX_BOUNDARY_SAMPLES {
                return Err("boundary_sample_budget_exceeded".into());
            }
        }
        edge_reports.push(json!({
            "loopIndex": loop_index,
            "orientedEdgeId": edge.oriented_id,
            "edgeCurveId": edge.edge_id,
            "curveGeometryId": edge.curve_id,
            "curveGeometryType": edge.curve_type,
            "curveParameterDomain": edge.curve_domain,
            "curveBreaks": edge.curve_breaks,
            "topologicalStartVertexId": edge.start_vertex,
            "topologicalEndVertexId": edge.end_vertex,
            "orientedEdgeSense": edge.oriented_sense,
            "edgeCurveSameSense": edge.curve_same_sense,
            "effectiveLoopSense": edge.loop_sense,
            "naturalCurveStartResidualSourceUnits": start_residual,
            "naturalCurveEndResidualSourceUnits": end_residual,
            "curveSamplesPerSpan": SAMPLES_PER_CURVE_SPAN,
            "sampleCount": (edge.curve_breaks.len() - 1) * SAMPLES_PER_CURVE_SPAN + 1
        }));
    }
    if previous_vertex != first_vertex {
        return Err("source_oriented_edge_loop_not_closed".into());
    }
    if boundary_samples.is_empty() {
        return Err("empty_source_boundary_samples".into());
    }
    let (shared_inverse_u_samples, shared_inverse_v_samples, shared_inverse_distance_checks) =
        shared_kernel_inverse_distance_check_count(
            boundary_samples.len(),
            &surface.u_breaks,
            &surface.v_breaks,
        )?;
    let shared_inverse_control_point_evaluations =
        shared_kernel_inverse_control_point_evaluation_estimate(
            boundary_samples.len(),
            surface.control_point_count,
            surface.construction_control_point_evaluation_estimate,
        )?;

    let mut max_surface_residual: f64 = 0.;
    let mut sum_squared_residual = 0.;
    let mut max_multistart_residual: f64 = 0.;
    let mut sum_squared_multistart_residual = 0.;
    let mut multistart_converged_samples = 0usize;
    let mut multistart_start_count = 0usize;
    let mut surface_evaluation_count = 0usize;
    let mut control_point_evaluation_count = 0usize;
    let mut candidate_surface_evaluation_count = 0usize;
    let mut pole_count = 0usize;
    let mut max_u_step: f64 = 0.;
    let mut max_v_step: f64 = 0.;
    let mut u_branch_shifts = 0usize;
    let mut v_branch_shifts = 0usize;
    let mut ambiguous_half_periods = 0usize;
    let mut candidate_u_branch_shifts = 0usize;
    let mut candidate_v_branch_shifts = 0usize;
    let mut candidate_ambiguous_half_periods = 0usize;
    let mut candidate_max_u_step: f64 = 0.;
    let mut candidate_max_v_step: f64 = 0.;
    let mut previous_uv: Option<[f64; 2]> = None;
    let mut previous_candidate_uv: Option<[f64; 2]> = None;
    let mut candidate_uv_vertices = Vec::with_capacity(boundary_samples.len());
    let mut candidate_projection_details = Vec::with_capacity(boundary_samples.len());
    let mut detail_samples = Vec::with_capacity(MAX_SAMPLE_DETAILS);
    for (edge_index, sample_index, parameter, source_point) in &boundary_samples {
        let (raw_uv, pole) = surface.evaluator.inverse(*source_point);
        if raw_uv.iter().any(|value| !value.is_finite()) {
            return Err("surface_inverse_returned_nonfinite_parameter".into());
        }
        pole_count += usize::from(pole);
        let projected = surface.evaluator.point_at(raw_uv);
        if projected.iter().any(|value| !value.is_finite()) {
            return Err("surface_evaluation_returned_nonfinite_point".into());
        }
        let residual = distance(projected, *source_point);
        if !residual.is_finite() {
            return Err("nonfinite_surface_boundary_residual".into());
        }
        max_surface_residual = max_surface_residual.max(residual);
        sum_squared_residual += residual * residual;

        let (best_candidate, starts) = multistart_inverse(
            &surface,
            *source_point,
            &seed_u,
            &seed_v,
            &mut surface_evaluation_count,
            &mut control_point_evaluation_count,
        )?;
        multistart_start_count = multistart_start_count
            .checked_add(starts)
            .filter(|count| *count <= MAX_SURFACE_EVALUATIONS)
            .ok_or("multistart_candidate_budget_exceeded")?;
        max_multistart_residual = max_multistart_residual.max(best_candidate.residual);
        sum_squared_multistart_residual += best_candidate.residual * best_candidate.residual;
        multistart_converged_samples += usize::from(best_candidate.converged);
        let candidate_uv = if let Some(previous) = previous_candidate_uv {
            let (u, u_shift) = unwrap_parameter(best_candidate.uv[0], previous[0], u_period)?;
            let (v, v_shift) = unwrap_parameter(best_candidate.uv[1], previous[1], v_period)?;
            candidate_u_branch_shifts += usize::from(u_shift != 0.);
            candidate_v_branch_shifts += usize::from(v_shift != 0.);
            candidate_max_u_step = candidate_max_u_step.max((u - previous[0]).abs());
            candidate_max_v_step = candidate_max_v_step.max((v - previous[1]).abs());
            if let Some(period) = u_period {
                candidate_ambiguous_half_periods +=
                    usize::from(((u - previous[0]).abs() - period * 0.5).abs() <= period * 1e-10);
            }
            if let Some(period) = v_period {
                candidate_ambiguous_half_periods +=
                    usize::from(((v - previous[1]).abs() - period * 0.5).abs() <= period * 1e-10);
            }
            [u, v]
        } else {
            best_candidate.uv
        };
        candidate_uv_vertices.push(candidate_uv);
        candidate_projection_details.push(best_candidate);
        previous_candidate_uv = Some(candidate_uv);

        let mut unwrapped = raw_uv;
        if let Some(previous) = previous_uv {
            let (u, u_shift) = unwrap_parameter(raw_uv[0], previous[0], u_period)?;
            let (v, v_shift) = unwrap_parameter(raw_uv[1], previous[1], v_period)?;
            unwrapped = [u, v];
            u_branch_shifts += usize::from(u_shift != 0.);
            v_branch_shifts += usize::from(v_shift != 0.);
            if let Some(period) = u_period {
                ambiguous_half_periods += usize::from(
                    ((unwrapped[0] - previous[0]).abs() - period * 0.5).abs() <= period * 1e-10,
                );
            }
            if let Some(period) = v_period {
                ambiguous_half_periods += usize::from(
                    ((unwrapped[1] - previous[1]).abs() - period * 0.5).abs() <= period * 1e-10,
                );
            }
            max_u_step = max_u_step.max((unwrapped[0] - previous[0]).abs());
            max_v_step = max_v_step.max((unwrapped[1] - previous[1]).abs());
        }
        previous_uv = Some(unwrapped);
        if detail_samples.len() < MAX_SAMPLE_DETAILS {
            detail_samples.push(json!({
                "loopIndex": edge_index,
                "sampleIndex": sample_index,
                "curveParameter": parameter,
                "sourcePoint": source_point,
                "rawUv": raw_uv,
                "unwrappedUv": unwrapped,
                "candidateUnwrappedUv": candidate_uv,
                "readerInverseResidualSourceUnits": residual,
                "multiStartBestUv": best_candidate.uv,
                "multiStartBestResidualSourceUnits": best_candidate.residual,
                "multiStartBestConverged": best_candidate.converged,
                "projectionSolver": best_candidate.solver_diagnostic(),
                "multiStartSeedCount": starts,
                "poleFlag": pole
            }));
        }
    }
    let candidate_loop_closure_uv = if let (Some(first), Some(last)) =
        (candidate_uv_vertices.first(), candidate_uv_vertices.last())
    {
        [last[0] - first[0], last[1] - first[1]]
    } else {
        return Err("trim_candidate_boundary_trace_empty".into());
    };
    let trim_candidate_result = (|| -> Result<Json, String> {
        let candidate =
            crate::trim_polygon_candidate::triangulate_simple_loop(&candidate_uv_vertices)?;
        let boundary_segments = source_boundary_curve_segments(
            &boundary_samples,
            &candidate.input_vertex_indices,
            edges.len(),
        )?;
        let mut boundary_vertex_points_by_index = std::collections::BTreeMap::new();
        for (boundary_index, (curve_index, sample_index, _, source_point)) in
            boundary_samples.iter().enumerate()
        {
            let edge = edges
                .get(*curve_index)
                .ok_or("trim_candidate_boundary_curve_index_invalid")?;
            let expected_samples = edge
                .curve_breaks
                .len()
                .saturating_sub(1)
                .checked_mul(SAMPLES_PER_CURVE_SPAN)
                .and_then(|count| count.checked_add(1))
                .ok_or("trim_candidate_boundary_sample_count_overflow")?;
            let canonical_point = if *sample_index == 0 {
                let vertex_id = if edge.loop_sense {
                    edge.start_vertex
                } else {
                    edge.end_vertex
                };
                vertex(step, vertex_id)?
            } else if *sample_index + 1 == expected_samples {
                let vertex_id = if edge.loop_sense {
                    edge.end_vertex
                } else {
                    edge.start_vertex
                };
                vertex(step, vertex_id)?
            } else {
                *source_point
            };
            let vertex_index = candidate.input_vertex_indices[boundary_index];
            if let Some(existing) = boundary_vertex_points_by_index.get(&vertex_index) {
                if existing != &canonical_point {
                    return Err("trim_candidate_boundary_vertex_point_conflict".into());
                }
            } else {
                boundary_vertex_points_by_index.insert(vertex_index, canonical_point);
            }
        }
        let boundary_vertices = boundary_vertex_points_by_index
            .into_iter()
            .map(|(vertex_index, point)| BoundaryVertexPoint {
                vertex_index,
                point,
            })
            .collect::<Vec<_>>();
        let mut boundary_refinement_probes = Vec::<(usize, Json)>::new();
        let candidate_surface_evaluation_count_cell = std::cell::RefCell::new(0usize);
        let control_point_evaluation_count_cell =
            std::cell::RefCell::new(control_point_evaluation_count);
        let orientation_observations =
            std::cell::RefCell::new(Vec::<OrientationObservation>::new());
        let normal_u_step = ((surface.u_domain.1 - surface.u_domain.0).abs() * 1e-5)
            .max(f64::EPSILON * (surface.u_domain.1 - surface.u_domain.0).abs() * 64.);
        let normal_v_step = ((surface.v_domain.1 - surface.v_domain.0).abs() * 1e-5)
            .max(f64::EPSILON * (surface.v_domain.1 - surface.v_domain.0).abs() * 64.);
        let refined = crate::trim_polygon_candidate::refine_uniform_with_boundary_and_orientation(
            candidate,
            RefinementOptions {
                chord_tolerance: 0.005,
                max_refinement_levels: 4,
                max_triangles: 50_000,
            },
            &boundary_segments,
            &boundary_vertices,
            |uv| {
                eval_surface(
                    &surface,
                    uv,
                    &mut candidate_surface_evaluation_count_cell.borrow_mut(),
                    &mut control_point_evaluation_count_cell.borrow_mut(),
                )
            },
            |segment, start_uv, end_uv, candidate_vertex_index| {
                if boundary_refinement_probes.len() >= MAX_BOUNDARY_REFINEMENT_PROBES {
                    return Err("trim_candidate_boundary_refinement_probe_budget_exceeded".into());
                }
                let edge = edges
                    .get(segment.curve_index)
                    .ok_or("trim_candidate_boundary_curve_index_invalid")?;
                let curve_parameter = (segment.start_parameter + segment.end_parameter) * 0.5;
                let source_point = edge.curve.point_at(curve_parameter);
                if source_point
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > 1e12)
                {
                    return Err("trim_candidate_boundary_curve_midpoint_nonfinite".into());
                }
                let seed_uv = [
                    (start_uv[0] + end_uv[0]) * 0.5,
                    (start_uv[1] + end_uv[1]) * 0.5,
                ];
                let (projection, projection_starts) = multistart_inverse(
                    &surface,
                    source_point,
                    &[seed_uv[0]],
                    &[seed_uv[1]],
                    &mut surface_evaluation_count,
                    &mut control_point_evaluation_count_cell.borrow_mut(),
                )?;
                if projection_starts != 1 {
                    return Err("trim_candidate_boundary_projection_start_count_invalid".into());
                }
                let (u, _) = unwrap_parameter(projection.uv[0], seed_uv[0], u_period)?;
                let (v, _) = unwrap_parameter(projection.uv[1], seed_uv[1], v_period)?;
                let candidate_uv = [u, v];
                let surface_point = eval_surface(
                    &surface,
                    candidate_uv,
                    &mut candidate_surface_evaluation_count_cell.borrow_mut(),
                    &mut control_point_evaluation_count_cell.borrow_mut(),
                )?;
                let surface_residual = distance(surface_point, source_point);
                if !surface_residual.is_finite() {
                    return Err("trim_candidate_boundary_projection_residual_nonfinite".into());
                }
                boundary_refinement_probes.push((
                    candidate_vertex_index,
                    json!({
                        "candidateVertexIndex": candidate_vertex_index,
                        "loopIndex": segment.curve_index,
                        "orientedEdgeId": edge.oriented_id,
                        "edgeCurveId": edge.edge_id,
                        "curveGeometryId": edge.curve_id,
                        "curveParameter": curve_parameter,
                        "sourcePointSourceUnits": source_point,
                        "projectionSeedUv": seed_uv,
                        "candidateUv": candidate_uv,
                        "surfacePointSourceUnits": surface_point,
                        "meshPointSourceUnits": source_point,
                        "surfaceResidualSourceUnits": surface_residual,
                        "projectionResidualSourceUnits": projection.residual,
                        "projectionConverged": projection.converged,
                        "projectionSolver": projection.solver_diagnostic(),
                        "projectionStartCount": projection_starts
                    }),
                ));
                Ok((candidate_uv, source_point))
            },
            |uv_triangle, point_triangle| {
                let uv_center = [
                    (uv_triangle[0][0] + uv_triangle[1][0] + uv_triangle[2][0]) / 3.,
                    (uv_triangle[0][1] + uv_triangle[1][1] + uv_triangle[2][1]) / 3.,
                ];
                let Some((u_min, u_max)) =
                    finite_difference_bounds(uv_center[0], normal_u_step, surface.u_domain, false)
                else {
                    record_orientation_observation(
                        &orientation_observations,
                        OrientationObservation {
                            uv_centroid: uv_center,
                            alignment: 0.,
                            triangle_normal: unit(cross(
                                sub(point_triangle[1], point_triangle[0]),
                                sub(point_triangle[2], point_triangle[0]),
                            ))
                            .ok(),
                            parameter_normal: None,
                        },
                    )?;
                    return Ok(0.);
                };
                let Some((v_min, v_max)) = finite_difference_bounds(
                    uv_center[1],
                    normal_v_step,
                    surface.v_domain,
                    surface.v_period.is_some(),
                ) else {
                    record_orientation_observation(
                        &orientation_observations,
                        OrientationObservation {
                            uv_centroid: uv_center,
                            alignment: 0.,
                            triangle_normal: unit(cross(
                                sub(point_triangle[1], point_triangle[0]),
                                sub(point_triangle[2], point_triangle[0]),
                            ))
                            .ok(),
                            parameter_normal: None,
                        },
                    )?;
                    return Ok(0.);
                };
                let point_u_min = eval_surface(
                    &surface,
                    [u_min, uv_center[1]],
                    &mut candidate_surface_evaluation_count_cell.borrow_mut(),
                    &mut control_point_evaluation_count_cell.borrow_mut(),
                )?;
                let point_u_max = eval_surface(
                    &surface,
                    [u_max, uv_center[1]],
                    &mut candidate_surface_evaluation_count_cell.borrow_mut(),
                    &mut control_point_evaluation_count_cell.borrow_mut(),
                )?;
                let point_v_min = eval_surface(
                    &surface,
                    [uv_center[0], v_min],
                    &mut candidate_surface_evaluation_count_cell.borrow_mut(),
                    &mut control_point_evaluation_count_cell.borrow_mut(),
                )?;
                let point_v_max = eval_surface(
                    &surface,
                    [uv_center[0], v_max],
                    &mut candidate_surface_evaluation_count_cell.borrow_mut(),
                    &mut control_point_evaluation_count_cell.borrow_mut(),
                )?;
                let tangent_u = scale(sub(point_u_max, point_u_min), 1. / (u_max - u_min));
                let tangent_v = scale(sub(point_v_max, point_v_min), 1. / (v_max - v_min));
                let alignment = oriented_triangle_normal_alignment(
                    point_triangle,
                    tangent_u,
                    tangent_v,
                    face_sense,
                );
                let face_sense_sign = if face_sense { 1. } else { -1. };
                let parameter_normal =
                    unit(scale(cross(tangent_u, tangent_v), face_sense_sign)).ok();
                let triangle_normal = unit(cross(
                    sub(point_triangle[1], point_triangle[0]),
                    sub(point_triangle[2], point_triangle[0]),
                ))
                .ok();
                record_orientation_observation(
                    &orientation_observations,
                    OrientationObservation {
                        uv_centroid: uv_center,
                        alignment,
                        triangle_normal,
                        parameter_normal,
                    },
                )?;
                Ok(alignment)
            },
        )?;
        let orientation_observations = orientation_observations.into_inner();
        let final_level_observation_start = orientation_observations
            .len()
            .checked_sub(refined.orientation_check_count)
            .ok_or("trim_candidate_orientation_observation_count_mismatch")?;
        let final_level_orientation_observations =
            &orientation_observations[final_level_observation_start..];
        candidate_surface_evaluation_count = candidate_surface_evaluation_count_cell.into_inner();
        control_point_evaluation_count = control_point_evaluation_count_cell.into_inner();
        let support_surface_points = refined
            .vertices
            .iter()
            .copied()
            .map(|uv| {
                eval_surface(
                    &surface,
                    uv,
                    &mut candidate_surface_evaluation_count,
                    &mut control_point_evaluation_count,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        if refined.input_vertex_indices.len() != boundary_samples.len()
            || candidate_projection_details.len() != boundary_samples.len()
        {
            return Err("trim_candidate_boundary_provenance_count_mismatch".into());
        }
        let orientation_continuity = orientation_continuity_report(
            &refined.vertices,
            &refined.triangles,
            final_level_orientation_observations,
            surface.u_domain,
            orientation_observations.len(),
        )?;
        let candidate_boundary_samples = boundary_samples
            .iter()
            .enumerate()
            .map(
                |(boundary_index, (loop_index, sample_index, parameter, source_point))| {
                    let candidate_vertex_index = refined.input_vertex_indices[boundary_index];
                    let candidate_uv = refined.vertices[candidate_vertex_index];
                    let surface_point = support_surface_points[candidate_vertex_index];
                    let mesh_point = refined.points[candidate_vertex_index];
                    let edge = &edges[*loop_index];
                    let projection = candidate_projection_details[boundary_index];
                    json!({
                        "boundaryIndex": boundary_index,
                        "loopIndex": loop_index,
                        "sampleIndex": sample_index,
                        "orientedEdgeId": edge.oriented_id,
                        "edgeCurveId": edge.edge_id,
                        "curveGeometryId": edge.curve_id,
                        "curveParameter": parameter,
                        "sourcePointSourceUnits": source_point,
                        "candidateUv": candidate_uv,
                        "candidateVertexIndex": candidate_vertex_index,
                        "surfacePointSourceUnits": surface_point,
                        "surfaceResidualSourceUnits": distance(surface_point, *source_point),
                        "candidateMeshPointSourceUnits": mesh_point,
                        "meshToSourceTrimResidualSourceUnits": distance(mesh_point, *source_point),
                        "projectionResidualSourceUnits": projection.residual,
                        "projectionConverged": projection.converged,
                        "projectionSolver": projection.solver_diagnostic()
                    })
                },
            )
            .collect::<Vec<_>>();
        let boundary_refinement_probes = boundary_refinement_probes
            .into_iter()
            .map(|(candidate_vertex_index, mut probe)| {
                probe["retainedInCandidate"] =
                    json!(candidate_vertex_index < refined.vertices.len());
                probe
            })
            .collect::<Vec<_>>();
        // Record exact source-curve samples at the midpoint of every adjacent
        // candidate boundary interval. The Spade experiment may need a boundary
        // split to meet its angle target; it must be able to replace that split
        // with a point evaluated on the STEP curve, never an unprovenanced chord
        // midpoint. These probes are geometric evidence only and do not qualify
        // the source model or relax the admission gates.
        let mut quality_samples_by_edge = vec![Vec::<([f64; 2], f64)>::new(); edges.len()];
        for row in &candidate_boundary_samples {
            let loop_index = row["loopIndex"]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|index| *index < edges.len())
                .ok_or("boundary_quality_sample_edge_invalid")?;
            let curve_parameter = row["curveParameter"]
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("boundary_quality_sample_parameter_invalid")?;
            let uv = row["candidateUv"]
                .as_array()
                .filter(|values| values.len() == 2)
                .map(|values| {
                    [
                        values[0].as_f64().unwrap_or(f64::NAN),
                        values[1].as_f64().unwrap_or(f64::NAN),
                    ]
                })
                .filter(|uv| uv.iter().all(|value| value.is_finite()))
                .ok_or("boundary_quality_sample_uv_invalid")?;
            quality_samples_by_edge[loop_index].push((uv, curve_parameter));
        }
        let mut eligible_refinement_probe_count = 0usize;
        for row in &boundary_refinement_probes {
            let retained = row["retainedInCandidate"]
                .as_bool()
                .ok_or("boundary_quality_probe_retention_status_missing")?;
            let projection_residual = row["projectionResidualSourceUnits"].as_f64();
            let surface_residual = row["surfaceResidualSourceUnits"].as_f64();
            let selected_by_residual = matches!(
                (projection_residual, surface_residual),
                (Some(projection), Some(surface))
                    if projection.is_finite()
                        && surface.is_finite()
                        && projection >= 0.0
                        && surface >= 0.0
                        && projection <= BOUNDARY_QUALITY_PROBE_SELECTION_RESIDUAL_SOURCE_UNITS
                        && surface <= BOUNDARY_QUALITY_PROBE_SELECTION_RESIDUAL_SOURCE_UNITS
            );
            if !retained && !selected_by_residual {
                continue;
            }
            let loop_index = row["loopIndex"]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|index| *index < edges.len())
                .ok_or("boundary_quality_probe_edge_invalid")?;
            let curve_parameter = row["curveParameter"]
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("boundary_quality_probe_parameter_invalid")?;
            let uv = row["candidateUv"]
                .as_array()
                .filter(|values| values.len() == 2)
                .map(|values| {
                    [
                        values[0].as_f64().unwrap_or(f64::NAN),
                        values[1].as_f64().unwrap_or(f64::NAN),
                    ]
                })
                .filter(|uv| uv.iter().all(|value| value.is_finite()))
                .ok_or("boundary_quality_probe_uv_invalid")?;
            quality_samples_by_edge[loop_index].push((uv, curve_parameter));
            eligible_refinement_probe_count += 1;
        }
        let mut boundary_quality_probes = Vec::new();
        for (loop_index, edge) in edges.iter().enumerate() {
            let edge_samples = boundary_samples
                .iter()
                .filter(|(sample_edge_index, _, _, _)| *sample_edge_index == loop_index)
                .collect::<Vec<_>>();
            let first_parameter = edge_samples
                .first()
                .map(|(_, _, parameter, _)| *parameter)
                .ok_or("boundary_quality_edge_samples_missing")?;
            let last_parameter = edge_samples
                .last()
                .map(|(_, _, parameter, _)| *parameter)
                .ok_or("boundary_quality_edge_samples_missing")?;
            let direction = if last_parameter > first_parameter {
                1.0
            } else if last_parameter < first_parameter {
                -1.0
            } else {
                return Err("boundary_quality_edge_parameter_direction_missing".into());
            };
            let samples = &mut quality_samples_by_edge[loop_index];
            samples.sort_by(|left, right| {
                let order = left.1.total_cmp(&right.1);
                if direction < 0.0 {
                    order.reverse()
                } else {
                    order
                }
            });
            samples.dedup_by(|right, left| right.1 == left.1 && right.0 == left.0);
            for pair in samples.windows(2) {
                let (start_uv, start_parameter) = pair[0];
                let (end_uv, end_parameter) = pair[1];
                if start_parameter == end_parameter {
                    return Err("boundary_quality_duplicate_parameter_conflict".into());
                }
                let curve_parameter = (start_parameter + end_parameter) * 0.5;
                if !curve_parameter.is_finite()
                    || curve_parameter < edge.curve_domain.0
                    || curve_parameter > edge.curve_domain.1
                {
                    return Err("boundary_quality_midpoint_outside_source_curve_domain".into());
                }
                if boundary_quality_probes.len() >= MAX_BOUNDARY_QUALITY_PROBES {
                    return Err("boundary_quality_probe_budget_exceeded".into());
                }
                let source_point = edge.curve.point_at(curve_parameter);
                if source_point
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > 1e12)
                {
                    return Err("boundary_quality_source_curve_point_invalid".into());
                }
                let seed_uv = [
                    (start_uv[0] + end_uv[0]) * 0.5,
                    (start_uv[1] + end_uv[1]) * 0.5,
                ];
                let (projection, projection_starts) = multistart_inverse(
                    &surface,
                    source_point,
                    &[seed_uv[0]],
                    &[seed_uv[1]],
                    &mut candidate_surface_evaluation_count,
                    &mut control_point_evaluation_count,
                )?;
                if projection_starts != 1 {
                    return Err("boundary_quality_projection_start_count_invalid".into());
                }
                let (u, _) = unwrap_parameter(projection.uv[0], seed_uv[0], u_period)?;
                let (v, _) = unwrap_parameter(projection.uv[1], seed_uv[1], v_period)?;
                let candidate_uv = [u, v];
                let surface_point = eval_surface(
                    &surface,
                    candidate_uv,
                    &mut candidate_surface_evaluation_count,
                    &mut control_point_evaluation_count,
                )?;
                let surface_residual = distance(surface_point, source_point);
                if !surface_residual.is_finite() {
                    return Err("boundary_quality_surface_residual_invalid".into());
                }
                let chord_midpoint = seed_uv;
                let chord_midpoint_deviation_uv = (candidate_uv[0] - chord_midpoint[0])
                    .hypot(candidate_uv[1] - chord_midpoint[1]);
                boundary_quality_probes.push(json!({
                    "qualityProbeIndex": boundary_quality_probes.len(),
                    "loopIndex": loop_index,
                    "orientedEdgeId": edge.oriented_id,
                    "edgeCurveId": edge.edge_id,
                    "curveGeometryId": edge.curve_id,
                    "intervalStartCurveParameter": start_parameter,
                    "intervalEndCurveParameter": end_parameter,
                    "intervalStartCandidateUv": start_uv,
                    "intervalEndCandidateUv": end_uv,
                    "curveParameter": curve_parameter,
                    "sourcePointSourceUnits": source_point,
                    "projectionSeedUv": seed_uv,
                    "candidateUv": candidate_uv,
                    "surfacePointSourceUnits": surface_point,
                    "projectionResidualSourceUnits": projection.residual,
                    "surfaceResidualSourceUnits": surface_residual,
                    "projectionConverged": projection.converged,
                        "projectionSolver": projection.solver_diagnostic(),
                    "projectionStartCount": projection_starts,
                    "deviationFromChordMidpointUv": chord_midpoint_deviation_uv
                }));
            }
        }
        Ok(json!({
            "status": "candidate_only",
            "algorithm": "bounded_simple_loop_ear_clipping_with_uniform_refinement_and_source_curve_boundary_midpoint_projection",
            "vertexCount": refined.vertices.len(),
            "triangleCount": refined.triangles.len(),
            "signedUvArea": refined.signed_area,
            "removedAdjacentDuplicateCount": refined.removed_adjacent_duplicates,
            "predicateCheckCount": refined.predicate_checks,
            "refinementLevels": refined.refinement_levels,
            "maxSampledChordErrorSourceUnits": refined.max_chord_error,
            "chordToleranceSourceUnits": refined.chord_tolerance,
            "chordConverged": refined.chord_converged,
            "orientationConverged": refined.orientation_converged,
            "orientationCheckCount": refined.orientation_check_count,
            "minimumTriangleNormalAlignment": refined.minimum_normal_alignment,
            "nonpositiveTriangleNormalCount": refined.nonpositive_normal_count,
            "orientationContinuity": orientation_continuity,
            "refinementConverged": refined.converged,
            "refinementFailureReason": refined.failure_reason,
            "surfaceEvaluationCount": candidate_surface_evaluation_count,
            "boundaryRefinementProbeCount": boundary_refinement_probes.len(),
            "boundaryRefinementProbeBudget": MAX_BOUNDARY_REFINEMENT_PROBES,
            "boundaryQualityProbeCount": boundary_quality_probes.len(),
            "boundaryQualityProbeBudget": MAX_BOUNDARY_QUALITY_PROBES,
            "boundaryQualityProbeSelectionResidualSourceUnits": BOUNDARY_QUALITY_PROBE_SELECTION_RESIDUAL_SOURCE_UNITS,
            "boundaryQualityProbeSelectionIsGeometryAdmissionTolerance": false,
            "boundaryQualityInputRefinementProbeCount": eligible_refinement_probe_count,
            "uvVertices": refined.vertices,
            "surfacePointsSourceUnits": support_surface_points,
            "meshPointsSourceUnits": refined.points,
            "triangles": refined.triangles,
            "boundaryTrace": {
                "sampleCount": candidate_uv_vertices.len(),
                "sourceSampleOrderPreserved": true,
                "candidateLoopClosureUv": candidate_loop_closure_uv,
                "maximumUnwrappedUStep": candidate_max_u_step,
                "maximumUnwrappedVStep": candidate_max_v_step,
                "uPeriodicBranchShiftCount": candidate_u_branch_shifts,
                "vPeriodicBranchShiftCount": candidate_v_branch_shifts,
                "ambiguousHalfPeriodStepCount": candidate_ambiguous_half_periods
            },
            "boundarySamples": candidate_boundary_samples,
            "boundaryRefinementProbes": boundary_refinement_probes,
            "boundaryQualityProbes": boundary_quality_probes,
            "sourceMeshModified": false,
            "geometryQualified": false,
            "admitted": false,
            "exportsProduced": []
        }))
    })();
    let trim_candidate = match trim_candidate_result {
        Ok(report) => report,
        Err(reason) => json!({
            "status": "rejected",
            "reason": reason,
            "surfaceEvaluationCount": candidate_surface_evaluation_count,
            "sourceMeshModified": false,
            "geometryQualified": false,
            "admitted": false,
            "exportsProduced": []
        }),
    };
    let trim_candidate_generated = trim_candidate["status"] == "candidate_only";

    Ok(json!({
        "schema": "tokito.step-spline-face-trim-diagnostic.v1",
        "faceId": face_id,
        "surfaceId": surface_id,
        "boundId": bound_id,
        "loopId": loop_id,
        "faceSameSense": face_sense,
        "boundOrientation": bound_sense,
        "sourceOrientedEdgeLoopClosed": true,
        "sourceOrientedEdgeIds": edge_reports.iter().map(|edge| edge["orientedEdgeId"].clone()).collect::<Vec<_>>(),
        "sourceEdgeCurveControlPointStorageCount": total_edge_curve_control_points,
        "sourceEdgeCurveControlPointStorageBudget": MAX_TOTAL_EDGE_CURVE_CONTROL_POINTS,
        "sourceEdgeCurveControlPointEvaluationEstimate": edge_curve_control_point_evaluations,
        "sourceEdgeCurveControlPointEvaluationBudget": MAX_EDGE_CURVE_CONTROL_POINT_EVALUATIONS,
        "surface": surface_report,
        "edges": edge_reports,
        "trimCandidate": trim_candidate,
        "trimProjection": {
            "boundarySampleCount": boundary_samples.len(),
            "sampleDetails": detail_samples,
            "sampleDetailsTruncated": boundary_samples.len() > MAX_SAMPLE_DETAILS,
            "maxSampleDetails": MAX_SAMPLE_DETAILS,
            "curveSamplesPerKnotSpan": SAMPLES_PER_CURVE_SPAN,
            "maximumCurveEndpointResidualSourceUnits": max_endpoint_residual,
            "readerInverseMaximumResidualSourceUnits": max_surface_residual,
            "readerInverseRootMeanSquareResidualSourceUnits": (sum_squared_residual / boundary_samples.len() as f64).sqrt(),
            "multiStartMaximumResidualSourceUnits": max_multistart_residual,
            "multiStartRootMeanSquareResidualSourceUnits": (sum_squared_multistart_residual / boundary_samples.len() as f64).sqrt(),
            "multiStartConvergedSampleCount": multistart_converged_samples,
            "multiStartSampleCount": boundary_samples.len(),
            "multiStartSeedsPerSample": seed_u.len() * seed_v.len(),
            "multiStartTotalCandidateStarts": multistart_start_count,
            "multiStartSurfaceEvaluationCount": surface_evaluation_count,
            "diagnosticControlPointEvaluationCount": control_point_evaluation_count,
            "diagnosticControlPointEvaluationBudget": MAX_DIAGNOSTIC_CONTROL_POINT_EVALUATIONS,
            "candidateSurfaceEvaluationCount": candidate_surface_evaluation_count,
            "sharedKernelInverseGridDimensions": [shared_inverse_u_samples, shared_inverse_v_samples],
            "sharedKernelInverseDistanceChecks": shared_inverse_distance_checks,
            "sharedKernelInverseDistanceCheckBudget": MAX_SHARED_KERNEL_INVERSE_DISTANCE_CHECKS,
            "sharedKernelInverseControlPointEvaluationEstimate": shared_inverse_control_point_evaluations,
            "sharedKernelInverseControlPointEvaluationBudget": MAX_SHARED_KERNEL_INVERSE_CONTROL_POINT_EVALUATIONS,
            "surfaceProjectionPoleCount": pole_count,
            "maximumUnwrappedUStep": max_u_step,
            "maximumUnwrappedVStep": max_v_step,
            "uPeriodicBranchShiftCount": u_branch_shifts,
            "vPeriodicBranchShiftCount": v_branch_shifts,
            "ambiguousHalfPeriodStepCount": ambiguous_half_periods,
            "candidatePeriodicBranchTrace": {
                "uPeriodicBranchShiftCount": candidate_u_branch_shifts,
                "vPeriodicBranchShiftCount": candidate_v_branch_shifts,
                "ambiguousHalfPeriodStepCount": candidate_ambiguous_half_periods,
                "maximumUnwrappedUStep": candidate_max_u_step,
                "maximumUnwrappedVStep": candidate_max_v_step,
                "loopClosureUv": candidate_loop_closure_uv
            }
        },
        "evaluation": {
            "inputGeometry": "source B-spline control points, knot vectors, rational weights, and oriented 3D B-spline edge curves",
            "projectionAndSurfaceEvaluator": "pinned oxideav-ifc shared kernel used by the reader; not an independent implementation",
            "inverseSearch": "bounded multi-start finite-difference least-squares projection implemented in this diagnostic; uses the shared surface point evaluator",
            "inverseSearchAlgorithmDiffersFromReader": true,
            "inverseConvergenceStatusAvailable": false,
            "diagnosticSolverTerminationAvailable": true,
            "diagnosticStationarityMethod": "bounded finite differences at selected final UV; normalized tangent/residual dot products; no constrained stationarity claim at domain boundaries",
            "diagnosticStationarityDoesNotChangeLegacyConvergenceOrAdmission": true,
            "inverseBranchesEnumerated": false,
            "surfaceCauseEstablished": false,
            "repairAttempted": false,
            "trimCandidateGenerated": trim_candidate_generated,
            "independentReferenceComparisonPerformed": false,
            "geometryQualified": false,
            "admitted": false,
            "exportsProduced": [],
            "scope": "bounded source-edge-to-source-surface residuals plus a research-only simple-loop ear-clipping candidate; no source mesh mutation, independent qualification, or production admission"
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_continuity_reports_uv_bins_and_shared_edge_normals() {
        let vertices = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
        let triangles = [[0, 1, 2], [0, 2, 3]];
        let observations = [
            OrientationObservation {
                uv_centroid: [2. / 3., 1. / 3.],
                alignment: 1.,
                triangle_normal: Some([0., 0., 1.]),
                parameter_normal: Some([0., 0., 1.]),
            },
            OrientationObservation {
                uv_centroid: [1. / 3., 2. / 3.],
                alignment: 1.,
                triangle_normal: Some([0., 0., 1.]),
                parameter_normal: Some([0., 0., 1.]),
            },
        ];

        let report = orientation_continuity_report(
            &vertices,
            &triangles,
            &observations,
            (0., 1.),
            observations.len(),
        )
        .unwrap();

        assert_eq!(report["diagnosticOnly"], true);
        assert_eq!(report["admissionAffecting"], false);
        assert_eq!(report["uvWindingSigns"]["positive"], 2);
        assert_eq!(report["uvWindingSigns"]["negative"], 0);
        assert_eq!(report["edgeAdjacency"]["uniqueEdgeCount"], 5);
        assert_eq!(report["edgeAdjacency"]["boundaryEdgeCount"], 4);
        assert_eq!(report["edgeAdjacency"]["interiorEdgeCount"], 1);
        assert_eq!(report["edgeAdjacency"]["nonmanifoldEdgeCount"], 0);
        assert_eq!(
            report["edgeAdjacency"]["geometricTriangleNormalDot"]["minimumDot"],
            1.
        );
        assert_eq!(
            report["edgeAdjacency"]["parameterNormalDot"]["comparedPairCount"],
            1
        );
        assert_eq!(report["parameterNormalAlignmentByU8"][2]["uBin"], 2);
        assert_eq!(
            report["parameterNormalAlignmentByU8"][2]["parameterNormalAlignment"]["count"],
            1
        );
        assert_eq!(report["parameterNormalAlignmentByU8"][5]["uBin"], 5);
        assert_eq!(
            report["parameterNormalAlignmentByU8"][5]["parameterNormalAlignment"]["positiveCount"],
            1
        );
    }

    #[test]
    fn orientation_continuity_reports_undefined_normal_fallbacks() {
        let vertices = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
        let triangles = [[0, 1, 2], [0, 2, 3]];
        let observations = [
            OrientationObservation {
                uv_centroid: [2. / 3., 1. / 3.],
                alignment: 0.,
                triangle_normal: None,
                parameter_normal: None,
            },
            OrientationObservation {
                uv_centroid: [1. / 3., 2. / 3.],
                alignment: 1.,
                triangle_normal: Some([0., 0., 1.]),
                parameter_normal: Some([0., 0., 1.]),
            },
        ];

        let report = orientation_continuity_report(
            &vertices,
            &triangles,
            &observations,
            (0., 1.),
            observations.len(),
        )
        .unwrap();

        let undefined_alignment =
            &report["parameterNormalAlignmentByU8"][5]["parameterNormalAlignment"];
        assert_eq!(undefined_alignment["count"], 1);
        assert_eq!(undefined_alignment["nonpositiveCount"], 1);
        assert_eq!(undefined_alignment["undefinedParameterNormalCount"], 1);

        let adjacency = &report["edgeAdjacency"];
        assert_eq!(adjacency["adjacentTrianglePairCount"], 1);
        assert_eq!(
            adjacency["geometricTriangleNormalDot"]["comparedPairCount"],
            0
        );
        assert_eq!(
            adjacency["geometricTriangleNormalDot"]["undefinedPairCount"],
            1
        );
        assert_eq!(adjacency["parameterNormalDot"]["comparedPairCount"], 0);
        assert_eq!(adjacency["parameterNormalDot"]["undefinedPairCount"], 1);
        assert_eq!(
            report["adjacentNormalContinuityByU8"][4]["adjacentTrianglePairCount"],
            1
        );
    }

    #[test]
    fn orientation_continuity_rejects_unbounded_or_mismatched_observations() {
        let vertices = [[0., 0.], [1., 0.], [0., 1.]];
        let triangles = [[0, 1, 2]];
        let observations = [OrientationObservation {
            uv_centroid: [1. / 3., 1. / 3.],
            alignment: 1.,
            triangle_normal: Some([0., 0., 1.]),
            parameter_normal: Some([0., 0., 1.]),
        }];

        assert_eq!(
            orientation_continuity_report(
                &vertices,
                &triangles,
                &observations,
                (0., 1.),
                MAX_ORIENTATION_OBSERVATIONS + 1,
            )
            .unwrap_err(),
            "trim_candidate_orientation_continuity_input_invalid"
        );
        assert_eq!(
            orientation_continuity_report(&vertices, &triangles, &[], (0., 1.), 1).unwrap_err(),
            "trim_candidate_orientation_continuity_input_invalid"
        );
    }

    #[test]
    fn orientation_continuity_does_not_invent_pairs_for_nonmanifold_edges() {
        let vertices = [[0., 0.], [1., 0.], [0., 1.], [0., -1.], [0.5, 1.]];
        let triangles = [[0, 1, 2], [1, 0, 3], [0, 1, 4]];
        let observations = [
            OrientationObservation {
                uv_centroid: [1. / 3., 1. / 3.],
                alignment: 1.,
                triangle_normal: Some([0., 0., 1.]),
                parameter_normal: Some([0., 0., 1.]),
            },
            OrientationObservation {
                uv_centroid: [1. / 3., -1. / 3.],
                alignment: 1.,
                triangle_normal: Some([0., 0., 1.]),
                parameter_normal: Some([0., 0., 1.]),
            },
            OrientationObservation {
                uv_centroid: [0.5, 1. / 3.],
                alignment: 1.,
                triangle_normal: Some([0., 0., 1.]),
                parameter_normal: Some([0., 0., 1.]),
            },
        ];

        let report = orientation_continuity_report(
            &vertices,
            &triangles,
            &observations,
            (-1., 1.),
            observations.len(),
        )
        .unwrap();

        assert_eq!(report["edgeAdjacency"]["boundaryEdgeCount"], 6);
        assert_eq!(report["edgeAdjacency"]["interiorEdgeCount"], 0);
        assert_eq!(report["edgeAdjacency"]["nonmanifoldEdgeCount"], 1);
        assert_eq!(report["edgeAdjacency"]["adjacentTrianglePairCount"], 0);
        assert_eq!(
            report["edgeAdjacency"]["geometricTriangleNormalDot"]["comparedPairCount"],
            0
        );
    }

    #[test]
    fn oriented_triangle_normal_uses_face_sense_without_double_counting_uv_winding() {
        let origin = [0., 0., 0.];
        let x = [1., 0., 0.];
        let y = [0., 1., 0.];
        let tangent_u = x;
        let tangent_v = y;

        assert_eq!(
            oriented_triangle_normal_alignment([origin, x, y], tangent_u, tangent_v, true),
            1.
        );
        assert_eq!(
            oriented_triangle_normal_alignment([origin, y, x], tangent_u, tangent_v, false),
            1.
        );
        assert_eq!(
            oriented_triangle_normal_alignment([origin, x, y], tangent_u, tangent_v, false),
            -1.
        );
        assert_eq!(
            oriented_triangle_normal_alignment([origin, y, x], tangent_u, tangent_v, true),
            -1.
        );
        assert_eq!(
            oriented_triangle_normal_alignment(
                [origin, origin, origin],
                tangent_u,
                tangent_v,
                true
            ),
            0.
        );
    }

    #[test]
    fn aggregate_edge_curve_storage_budget_rejects_before_curve_allocation() {
        assert_eq!(
            checked_edge_curve_control_point_total(MAX_TOTAL_EDGE_CURVE_CONTROL_POINTS - 2, 2)
                .unwrap(),
            MAX_TOTAL_EDGE_CURVE_CONTROL_POINTS
        );
        assert_eq!(
            checked_edge_curve_control_point_total(MAX_TOTAL_EDGE_CURVE_CONTROL_POINTS - 1, 2)
                .unwrap_err(),
            "edge_curve_control_point_storage_budget_exceeded"
        );
        assert_eq!(
            checked_edge_curve_control_point_total(usize::MAX, 1).unwrap_err(),
            "edge_curve_control_point_storage_budget_exceeded"
        );
    }

    #[test]
    fn periodic_unwrap_chooses_nearest_branch_both_directions() {
        let (positive, positive_shift) = unwrap_parameter(0.1, 9.9, Some(10.)).unwrap();
        let (negative, negative_shift) = unwrap_parameter(9.9, 0.1, Some(10.)).unwrap();
        let (aperiodic, no_shift) = unwrap_parameter(0.4, 0.1, None).unwrap();
        assert!((positive - 10.1).abs() < 1e-12 && positive_shift == 1.);
        assert!((negative + 0.1).abs() < 1e-12 && negative_shift == -1.);
        assert!((aperiodic - 0.4).abs() < 1e-12 && no_shift == 0.);
    }

    #[test]
    fn periodic_unwrap_rejects_invalid_periods_and_extreme_shifts() {
        for period in [Some(0.), Some(-1.), Some(f64::NAN), Some(f64::INFINITY)] {
            assert!(unwrap_parameter(0., 1., period).is_err());
        }
        assert!(unwrap_parameter(0., 1e20, Some(1e-9)).is_err());
    }

    fn projection_test_plane(
        u_domain: (f64, f64),
        v_domain: (f64, f64),
        collapsed: bool,
    ) -> SurfaceDefinition {
        let x = if collapsed { 0. } else { 1. };
        let controls = vec![
            vec![[0., 0., 0.], [0., 1., 0.]],
            vec![[x, 0., 0.], [x, 1., 0.]],
        ];
        let spline = kernel::BSplineSurface::new(
            1,
            1,
            &controls,
            None,
            (&[u_domain.0, u_domain.1], &[2, 2]),
            (&[v_domain.0, v_domain.1], &[2, 2]),
            Some(false),
            Some(false),
        )
        .unwrap();
        SurfaceDefinition {
            evaluator: kernel::Surface::bspline(spline),
            u_domain,
            v_domain,
            u_breaks: vec![u_domain.0, u_domain.1],
            v_breaks: vec![v_domain.0, v_domain.1],
            u_period: None,
            v_period: None,
            control_point_count: 4,
            construction_control_point_evaluation_estimate: 81,
            control_net_diagonal: 2f64.sqrt(),
            report: json!({}),
        }
    }

    #[test]
    fn stationary_off_surface_evidence_is_invariant_under_uv_rescaling() {
        for (u_domain, v_domain) in [((0., 1.), (0., 1.)), ((10., 1010.), (3., 3.02))] {
            let plane = projection_test_plane(u_domain, v_domain, false);
            let (candidate, _) = multistart_inverse(
                &plane,
                [0.3, 0.6, 1e-4],
                &[u_domain.0],
                &[v_domain.0],
                &mut 0,
                &mut 0,
            )
            .unwrap();
            assert!(!candidate.converged);
            assert!((candidate.residual - 1e-4).abs() < 1e-12);
            assert_eq!(
                candidate.stationarity.status, "interior_tangent_orthogonality_observed",
                "{candidate:?}"
            );
            assert!(candidate
                .stationarity
                .tangent_residual_cosines
                .unwrap()
                .iter()
                .all(|c| c.abs() <= PROJECTION_TANGENT_ORTHOGONALITY_TOLERANCE));
        }
    }

    #[test]
    fn domain_boundary_and_singular_surface_do_not_claim_stationarity() {
        for (collapsed, target, status) in [
            (
                false,
                [0.3, 1.2, 0.],
                "domain_boundary_no_stationarity_claim",
            ),
            (true, [0.3, 0.6, 0.], "singular_or_nonfinite_derivatives"),
        ] {
            let plane = projection_test_plane((0., 1.), (0., 1.), collapsed);
            let (candidate, _) =
                multistart_inverse(&plane, target, &[0.5], &[0.5], &mut 0, &mut 0).unwrap();
            assert!(!candidate.converged);
            assert_eq!(candidate.stationarity.status, status, "{candidate:?}");
            if collapsed {
                assert_eq!(candidate.termination, "singular_jacobian");
            }
        }
    }

    #[test]
    fn stationarity_is_measured_at_supplied_final_uv_and_obeys_work_budget() {
        let plane = projection_test_plane((0., 1.), (0., 1.), false);
        let mut candidate = ProjectionCandidate {
            uv: [0.2, 0.4],
            residual: 1.,
            converged: false,
            termination: "iteration_limit",
            iterations: MAX_PROJECTION_ITERATIONS,
            residual_tolerance: plane.control_net_diagonal * 1e-10,
            stationarity: ProjectionStationarity {
                status: "not_evaluated",
                tangent_residual_cosines: None,
                normalized_jacobian_determinant: None,
            },
        };
        let target = [0.3, 0.6, 1e-4];
        let observation =
            observe_projection_stationarity(&plane, &candidate, target, &mut 0, &mut 0).unwrap();
        assert_eq!(observation.status, "tangent_orthogonality_not_observed");
        candidate.uv = [0.3, 0.6];
        let mut evaluations = 0;
        let mut controls = 0;
        let observation = observe_projection_stationarity(
            &plane,
            &candidate,
            target,
            &mut evaluations,
            &mut controls,
        )
        .unwrap();
        assert_eq!(
            observation.status,
            "interior_tangent_orthogonality_observed"
        );
        assert_eq!(evaluations, 5);
        assert_eq!(controls, 20);
        let mut exhausted_surfaces = MAX_SURFACE_EVALUATIONS;
        let mut exhausted_controls = MAX_DIAGNOSTIC_CONTROL_POINT_EVALUATIONS;
        assert_eq!(
            observe_projection_stationarity(
                &plane,
                &candidate,
                target,
                &mut exhausted_surfaces,
                &mut 0
            )
            .unwrap_err(),
            "multistart_surface_evaluation_budget_exceeded"
        );
        assert_eq!(
            observe_projection_stationarity(
                &plane,
                &candidate,
                target,
                &mut 0,
                &mut exhausted_controls
            )
            .unwrap_err(),
            "multistart_control_point_evaluation_budget_exceeded"
        );
    }

    #[test]
    fn bounded_multistart_recovers_known_point_on_bilinear_surface() {
        let controls = vec![
            vec![[0., 0., 0.], [0., 1., 0.]],
            vec![[1., 0., 0.], [1., 1., 0.]],
        ];
        let spline = kernel::BSplineSurface::new(
            1,
            1,
            &controls,
            None,
            (&[0., 1.], &[2, 2]),
            (&[0., 1.], &[2, 2]),
            Some(false),
            Some(false),
        )
        .unwrap();
        let target = spline.point_at(0.3, 0.6);
        let mut irreducibly_offset_target = spline.point_at(0.30000001, 0.6);
        irreducibly_offset_target[2] += 1e-6;
        let definition = SurfaceDefinition {
            evaluator: kernel::Surface::bspline(spline),
            u_domain: (0., 1.),
            v_domain: (0., 1.),
            u_breaks: vec![0., 1.],
            v_breaks: vec![0., 1.],
            u_period: None,
            v_period: None,
            control_point_count: 4,
            construction_control_point_evaluation_estimate: 81,
            control_net_diagonal: 2f64.sqrt(),
            report: json!({}),
        };
        let u = axis_seeds(&definition.u_breaks, MULTISTART_SUBDIVISIONS).unwrap();
        let v = axis_seeds(&definition.v_breaks, MULTISTART_SUBDIVISIONS).unwrap();
        let mut evaluations = 0;
        let mut control_point_evaluations = 0;
        let (candidate, starts) = multistart_inverse(
            &definition,
            target,
            &u,
            &v,
            &mut evaluations,
            &mut control_point_evaluations,
        )
        .unwrap();
        assert_eq!(starts, u.len() * v.len());
        assert!(candidate.residual < 1e-9, "{candidate:?}");
        assert!(candidate.converged);
        assert_eq!(candidate.termination, "residual_met");
        assert_eq!(candidate.stationarity.status, "not_evaluated_residual_met");
        assert!((candidate.uv[0] - 0.3).abs() < 1e-8);
        assert!((candidate.uv[1] - 0.6).abs() < 1e-8);
        let (offset_candidate, _) = multistart_inverse(
            &definition,
            irreducibly_offset_target,
            &u,
            &v,
            &mut evaluations,
            &mut control_point_evaluations,
        )
        .unwrap();
        assert!(offset_candidate.residual > 1e-7, "{offset_candidate:?}");
        assert!(!offset_candidate.converged, "{offset_candidate:?}");
        assert_eq!(
            offset_candidate.stationarity.status, "interior_tangent_orthogonality_observed",
            "{offset_candidate:?}"
        );
        let solver = offset_candidate.solver_diagnostic();
        assert_eq!(solver["residualToleranceMet"], false);
        assert_eq!(solver["globalMinimumProven"], false);
        assert_eq!(solver["authorizesGeometryAdmission"], false);
        assert!(serde_json::to_string(&solver).is_ok());
        assert!(evaluations <= MAX_SURFACE_EVALUATIONS);
        assert_eq!(control_point_evaluations, evaluations * 4);
    }

    #[test]
    fn shared_kernel_inverse_scan_estimate_matches_pinned_sampling_rule() {
        assert_eq!(
            shared_kernel_inverse_axis_sample_count(&[0., 1.]).unwrap(),
            9
        );
        assert_eq!(
            shared_kernel_inverse_axis_sample_count(&[0., 1., 2.]).unwrap(),
            17
        );
        let many_breaks = (0..MAX_AXIS_SEEDS)
            .map(|value| value as f64)
            .collect::<Vec<_>>();
        assert_eq!(
            shared_kernel_inverse_axis_sample_count(&many_breaks).unwrap(),
            316
        );
        assert_eq!(
            shared_kernel_inverse_distance_check_count(100, &[0., 1.], &[0., 1., 2.]).unwrap(),
            (9, 17, 15_300)
        );
        assert_eq!(
            shared_kernel_inverse_distance_check_count(
                MAX_BOUNDARY_SAMPLES,
                &many_breaks,
                &many_breaks
            ),
            Err("shared_kernel_inverse_work_budget_exceeded".into())
        );
    }

    #[test]
    fn shared_kernel_control_point_work_is_bounded_before_grid_and_inverse_work() {
        assert_eq!(
            shared_kernel_grid_control_point_evaluation_estimate(9, 17, 54).unwrap(),
            8_262
        );
        assert_eq!(
            shared_kernel_inverse_control_point_evaluation_estimate(100, 54, 8_262).unwrap(),
            661_662
        );
        assert_eq!(
            shared_kernel_grid_control_point_evaluation_estimate(316, 316, 65_536),
            Err("shared_kernel_surface_construction_work_budget_exceeded".into())
        );
        assert_eq!(
            shared_kernel_inverse_control_point_evaluation_estimate(1_000, 65_536, 1),
            Err("shared_kernel_inverse_control_point_work_budget_exceeded".into())
        );
    }

    #[test]
    fn finite_difference_intervals_use_the_clipped_nonperiodic_extent() {
        let low = finite_difference_bounds(0., 0.01, (0., 1.), false).unwrap();
        let high = finite_difference_bounds(1., 0.01, (0., 1.), false).unwrap();
        let periodic = finite_difference_bounds(0., 0.01, (0., 1.), true).unwrap();
        assert_eq!(low, (0., 0.01));
        assert_eq!(high, (0.99, 1.));
        assert_eq!(periodic, (-0.01, 0.01));
    }

    #[test]
    fn nonperiodic_multistart_returns_v_inside_the_evaluated_domain() {
        let controls = vec![
            vec![[0., 0., 0.], [0., 1., 0.]],
            vec![[1., 0., 0.], [1., 1., 0.]],
        ];
        let spline = kernel::BSplineSurface::new(
            1,
            1,
            &controls,
            None,
            (&[0., 1.], &[2, 2]),
            (&[0., 1.], &[2, 2]),
            Some(false),
            Some(false),
        )
        .unwrap();
        let definition = SurfaceDefinition {
            evaluator: kernel::Surface::bspline(spline),
            u_domain: (0., 1.),
            v_domain: (0., 1.),
            u_breaks: vec![0., 1.],
            v_breaks: vec![0., 1.],
            u_period: None,
            v_period: None,
            control_point_count: 4,
            construction_control_point_evaluation_estimate: 81,
            control_net_diagonal: 2f64.sqrt(),
            report: json!({}),
        };
        let u = axis_seeds(&definition.u_breaks, MULTISTART_SUBDIVISIONS).unwrap();
        let v = axis_seeds(&definition.v_breaks, MULTISTART_SUBDIVISIONS).unwrap();
        let mut evaluations = 0;
        let mut control_point_evaluations = 0;
        let (candidate, _) = multistart_inverse(
            &definition,
            [0.3, 1.2, 0.],
            &u,
            &v,
            &mut evaluations,
            &mut control_point_evaluations,
        )
        .unwrap();
        assert!((0. ..=1.).contains(&candidate.uv[0]));
        assert!((0. ..=1.).contains(&candidate.uv[1]));
        assert!(
            distance(
                eval_surface(
                    &definition,
                    candidate.uv,
                    &mut evaluations,
                    &mut control_point_evaluations,
                )
                .unwrap(),
                [0.3, 1.2, 0.]
            ) >= 0.2 - 1e-9
        );
    }
}
