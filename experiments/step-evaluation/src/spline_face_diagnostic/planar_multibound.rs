//! Bounded sampled triangulation for one pinned planar STEP face with holes.
//!
//! This is a diagnostic mesh candidate only. It preserves STEP source curves,
//! checks the sampled planar rings and reports its limits; it cannot qualify a
//! component, certify continuous curve error, repair a face, or export GLB.
use super::*;
use earcut::Earcut;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const MAX_BOUNDS: usize = 8;
const MAX_LOOP_EDGES: usize = 128;
const MAX_TOTAL_SAMPLES: usize = 16_384;
const MAX_CURVE_EVALUATIONS: usize = 250_000;
const MAX_INTERSECTION_TESTS: usize = 8_000_000;
const MAX_TRIANGLE_PAIR_TESTS: usize = 2_000_000;
const MAX_SUBDIVISION_DEPTH: usize = 18;

#[derive(Clone, Copy)]
struct Sample {
    point: P3,
    parameter: Option<f64>,
    source_edge_curve_id: Option<u64>,
    source_vertex_id: Option<u64>,
}

struct Ring {
    bound_id: u64,
    loop_id: u64,
    source_entity_type: &'static str,
    source_role: &'static str,
    samples: Vec<Sample>,
    points: Vec<[f64; 2]>,
    signed_area: f64,
    edge_count: usize,
    source_edge_curve_ids: Vec<u64>,
    source_edges: Vec<SourceEdgeSamples>,
}

struct SourceEdgeSamples {
    source_edge_curve_id: u64,
    traversal_start_vertex_id: u64,
    traversal_end_vertex_id: u64,
    points: Vec<P3>,
}

struct PlaneFrame {
    origin: P3,
    x: P3,
    y: P3,
    normal: P3,
}

struct SamplingWork {
    evaluations: usize,
    control_evaluations: usize,
    retained_curve_control_points: usize,
    sample_count: usize,
    max_sampled_chord_error: f64,
}

type TriangulatedFace = (Vec<P3>, Vec<[u32; 3]>, f64, usize, usize);

fn direction(step: &StepFile, id: u64) -> Result<P3, String> {
    let source = entity(step, id, "DIRECTION")?;
    let values = number_list(source, "direction_ratios", 3)?;
    if values.len() != 3 {
        return Err("planar_face_requires_three_axis_values".into());
    }
    unit([values[0], values[1], values[2]])
}

fn plane_frame(step: &StepFile, surface_id: u64) -> Result<PlaneFrame, String> {
    let surface = entity(step, surface_id, "PLANE")?;
    let placement = entity(
        step,
        surface
            .reference("position")
            .ok_or("planar_face_plane_missing_placement")?,
        "AXIS2_PLACEMENT_3D",
    )?;
    let origin = point(
        step,
        placement
            .reference("location")
            .ok_or("planar_face_plane_missing_origin")?,
    )?;
    let normal = direction(
        step,
        placement
            .reference("axis")
            .ok_or("planar_face_plane_missing_normal")?,
    )?;
    let reference = direction(
        step,
        placement
            .reference("ref_direction")
            .ok_or("planar_face_plane_missing_reference_direction")?,
    )?;
    let y = unit(cross(normal, reference))?;
    let x = unit(cross(y, normal))?;
    if origin
        .iter()
        .chain(x.iter())
        .chain(y.iter())
        .chain(normal.iter())
        .any(|v| !v.is_finite())
    {
        return Err("planar_face_nonfinite_plane_frame".into());
    }
    Ok(PlaneFrame {
        origin,
        x,
        y,
        normal,
    })
}

fn project(frame: &PlaneFrame, p: P3) -> ([f64; 2], f64) {
    let d = sub(p, frame.origin);
    ([dot(d, frame.x), dot(d, frame.y)], dot(d, frame.normal))
}

fn vector_direction(step: &StepFile, vector_id: u64) -> Result<P3, String> {
    let vector = entity(step, vector_id, "VECTOR")?;
    let magnitude = vector
        .attr("magnitude")
        .and_then(number)
        .filter(|v| *v > 0.)
        .ok_or("planar_face_line_vector_requires_positive_magnitude")?;
    if !magnitude.is_finite() || magnitude > 1e12 {
        return Err("planar_face_line_vector_magnitude_out_of_range".into());
    }
    direction(
        step,
        vector
            .reference("orientation")
            .ok_or("planar_face_line_vector_missing_direction")?,
    )
}

fn close_enough(a: P3, b: P3) -> bool {
    let scale = a
        .iter()
        .chain(b.iter())
        .fold(1.0_f64, |s, v| s.max(v.abs()));
    distance(a, b) <= 64.0 * f64::EPSILON * scale
}

fn line_samples(step: &StepFile, edge: Entity<'_>, curve_id: u64) -> Result<Vec<Sample>, String> {
    let curve = entity(step, curve_id, "LINE")?;
    let line_origin = point(
        step,
        curve
            .reference("pnt")
            .ok_or("planar_face_line_missing_origin")?,
    )?;
    let vector_id = curve
        .reference("dir")
        .ok_or("planar_face_line_missing_vector")?;
    let axis = vector_direction(step, vector_id)?;
    let start_id = edge
        .reference("edge_start")
        .ok_or("planar_face_line_missing_edge_start")?;
    let end_id = edge
        .reference("edge_end")
        .ok_or("planar_face_line_missing_edge_end")?;
    let start = vertex(step, start_id)?;
    let end = vertex(step, end_id)?;
    let start_delta = sub(start, line_origin);
    let end_delta = sub(end, line_origin);
    let start_parameter = dot(start_delta, axis);
    let end_parameter = dot(end_delta, axis);
    let start_residual = distance(
        start,
        std::array::from_fn(|i| line_origin[i] + start_parameter * axis[i]),
    );
    let end_residual = distance(
        end,
        std::array::from_fn(|i| line_origin[i] + end_parameter * axis[i]),
    );
    if !start_residual.is_finite()
        || !end_residual.is_finite()
        || !close_enough(
            start,
            std::array::from_fn(|i| line_origin[i] + start_parameter * axis[i]),
        )
        || !close_enough(
            end,
            std::array::from_fn(|i| line_origin[i] + end_parameter * axis[i]),
        )
    {
        return Err("planar_face_line_endpoint_not_on_source_line".into());
    }
    let same_sense = edge
        .logical("same_sense")
        .ok_or("planar_face_line_unknown_curve_sense")?;
    let delta = end_parameter - start_parameter;
    let scale = start_parameter.abs().max(end_parameter.abs()).max(1.0);
    if !delta.is_finite()
        || delta.abs() <= 64.0 * f64::EPSILON * scale
        || (delta > 0.0) != same_sense
    {
        return Err("planar_face_line_parameter_order_disagrees_with_edge_sense".into());
    }
    let (first, second) = if same_sense {
        (start, end)
    } else {
        (end, start)
    };
    Ok(vec![
        Sample {
            point: first,
            parameter: None,
            source_edge_curve_id: None,
            source_vertex_id: None,
        },
        Sample {
            point: second,
            parameter: None,
            source_edge_curve_id: None,
            source_vertex_id: None,
        },
    ])
}

fn bspline_samples(
    step: &StepFile,
    edge: Entity<'_>,
    curve_id: u64,
    target: f64,
    work: &mut SamplingWork,
) -> Result<Vec<Sample>, String> {
    let curve = parse_curve(step, curve_id)?;
    let source = entity(step, curve_id, "B_SPLINE_CURVE_WITH_KNOTS")?;
    let degree = required_integer(source, "degree", 32)?;
    let multiplicities = integer_list(source, "knot_multiplicities", MAX_CURVE_SPANS + 1)?;
    if multiplicities.first() != Some(&(degree + 1)) || multiplicities.last() != Some(&(degree + 1))
    {
        return Err("planar_face_requires_clamped_full_native_curve".into());
    }
    let start_id = edge
        .reference("edge_start")
        .ok_or("planar_face_curve_missing_start")?;
    let end_id = edge
        .reference("edge_end")
        .ok_or("planar_face_curve_missing_end")?;
    let same = edge
        .logical("same_sense")
        .ok_or("planar_face_curve_unknown_sense")?;
    let expected = if same {
        [start_id, end_id]
    } else {
        [end_id, start_id]
    };
    let mut cache: HashMap<u64, P3> = HashMap::new();
    let mut evaluate = |t: f64, work: &mut SamplingWork| -> Result<P3, String> {
        if let Some(&p) = cache.get(&t.to_bits()) {
            return Ok(p);
        }
        work.evaluations = work
            .evaluations
            .checked_add(1)
            .filter(|n| *n <= MAX_CURVE_EVALUATIONS)
            .ok_or("planar_face_curve_evaluation_budget")?;
        work.control_evaluations = work
            .control_evaluations
            .checked_add(curve.control_point_count)
            .filter(|n| *n <= MAX_EDGE_CURVE_CONTROL_POINT_EVALUATIONS)
            .ok_or("planar_face_curve_control_work_budget")?;
        let p = curve.curve.point_at(t);
        if p.iter().any(|v| !v.is_finite() || v.abs() > 1e12) {
            return Err("planar_face_curve_nonfinite_or_unbounded_sample".into());
        }
        cache.insert(t.to_bits(), p);
        Ok(p)
    };
    let first = evaluate(curve.domain.0, work)?;
    let last = evaluate(curve.domain.1, work)?;
    if !close_enough(first, vertex(step, expected[0])?)
        || !close_enough(last, vertex(step, expected[1])?)
    {
        return Err("planar_face_bspline_full_domain_endpoints_do_not_match_vertices".into());
    }
    let mut parameters = vec![curve.domain.0];
    for span in curve.breaks.windows(2) {
        let mut stack = vec![(span[0], span[1], 0usize)];
        while let Some((a, b, depth)) = stack.pop() {
            let pa = evaluate(a, work)?;
            let pb = evaluate(b, work)?;
            let mut error: f64 = 0.0;
            for fraction in [0.25, 0.5, 0.75] {
                let t = a * (1.0 - fraction) + b * fraction;
                let p = evaluate(t, work)?;
                let chord = std::array::from_fn(|i| pa[i] * (1.0 - fraction) + pb[i] * fraction);
                error = error.max(distance(p, chord));
            }
            if error > target {
                let mid = a * 0.5 + b * 0.5;
                if depth >= MAX_SUBDIVISION_DEPTH || !(a < mid && mid < b) {
                    return Err("planar_face_curve_subdivision_depth_or_precision_limit".into());
                }
                stack.push((mid, b, depth + 1));
                stack.push((a, mid, depth + 1));
            } else {
                work.max_sampled_chord_error = work.max_sampled_chord_error.max(error);
                parameters.push(b);
                if work
                    .sample_count
                    .checked_add(parameters.len())
                    .filter(|n| *n <= MAX_TOTAL_SAMPLES)
                    .is_none()
                {
                    return Err("planar_face_sample_budget".into());
                }
            }
        }
    }
    let samples = parameters
        .into_iter()
        .map(|t| {
            Ok(Sample {
                point: evaluate(t, work)?,
                parameter: Some(t),
                source_edge_curve_id: None,
                source_vertex_id: None,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    work.sample_count = work
        .sample_count
        .checked_add(samples.len())
        .filter(|n| *n <= MAX_TOTAL_SAMPLES)
        .ok_or("planar_face_sample_budget")?;
    Ok(samples)
}

fn edge_samples(
    step: &StepFile,
    edge_id: u64,
    target: f64,
    work: &mut SamplingWork,
) -> Result<(u64, u64, bool, Vec<Sample>), String> {
    let edge = entity(step, edge_id, "EDGE_CURVE")?;
    let start = edge
        .reference("edge_start")
        .ok_or("planar_face_edge_missing_start")?;
    let end = edge
        .reference("edge_end")
        .ok_or("planar_face_edge_missing_end")?;
    if start == end {
        return Err("planar_face_closed_edge_parameter_interval_unsupported".into());
    }
    let same = edge
        .logical("same_sense")
        .ok_or("planar_face_edge_unknown_sense")?;
    let curve_id = edge
        .reference("edge_geometry")
        .ok_or("planar_face_edge_missing_geometry")?;
    let curve = step.get(curve_id).ok_or("planar_face_curve_missing")?;
    crate::unit_preflight::shape(curve)?;
    if curve.keyword == "B_SPLINE_CURVE_WITH_KNOTS"
        || curve.keyword == "RATIONAL_B_SPLINE_CURVE_WITH_KNOTS"
    {
        let count = preflight_curve_control_point_count(step, curve_id)?;
        work.retained_curve_control_points = work
            .retained_curve_control_points
            .checked_add(count)
            .filter(|n| *n <= MAX_TOTAL_EDGE_CURVE_CONTROL_POINTS)
            .ok_or("planar_face_aggregate_curve_control_storage_budget")?;
    }
    let mut samples = match curve.keyword.as_str() {
        "LINE" => line_samples(step, edge, curve_id)?,
        "B_SPLINE_CURVE_WITH_KNOTS" | "RATIONAL_B_SPLINE_CURVE_WITH_KNOTS" => {
            bspline_samples(step, edge, curve_id, target, work)?
        }
        other => return Err(format!("planar_face_unsupported_edge_curve:{other}")),
    };
    let natural_vertices = if same { [start, end] } else { [end, start] };
    if let Some(first) = samples.first_mut() {
        first.source_edge_curve_id = Some(edge_id);
        first.source_vertex_id = Some(natural_vertices[0]);
    }
    if let Some(last) = samples.last_mut() {
        last.source_edge_curve_id = Some(edge_id);
        last.source_vertex_id = Some(natural_vertices[1]);
    }
    let interior_end = samples.len().saturating_sub(1);
    for sample in samples.iter_mut().take(interior_end).skip(1) {
        sample.source_edge_curve_id = Some(edge_id);
    }
    Ok((start, end, same, samples))
}

fn polygon_area(points: &[[f64; 2]]) -> Result<f64, String> {
    if points.len() < 3 {
        return Err("planar_face_ring_has_fewer_than_three_samples".into());
    }
    let origin = points[0];
    let twice = (0..points.len())
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            (a[0] - origin[0]) * (b[1] - origin[1]) - (b[0] - origin[0]) * (a[1] - origin[1])
        })
        .sum::<f64>();
    let area = twice * 0.5;
    if !area.is_finite() || area == 0.0 {
        return Err("planar_face_ring_has_zero_or_nonfinite_area".into());
    }
    Ok(area)
}

fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn on_segment(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> bool {
    orient(a, b, p) == 0.0
        && p[0] >= a[0].min(b[0])
        && p[0] <= a[0].max(b[0])
        && p[1] >= a[1].min(b[1])
        && p[1] <= a[1].max(b[1])
}

fn segments_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let (o1, o2, o3, o4) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    if (o1 > 0.0 && o2 < 0.0 || o1 < 0.0 && o2 > 0.0)
        && (o3 > 0.0 && o4 < 0.0 || o3 < 0.0 && o4 > 0.0)
    {
        return true;
    }
    (o1 == 0.0 && on_segment(a, b, c))
        || (o2 == 0.0 && on_segment(a, b, d))
        || (o3 == 0.0 && on_segment(c, d, a))
        || (o4 == 0.0 && on_segment(c, d, b))
}

fn point_in_ring(point: [f64; 2], ring: &[[f64; 2]]) -> Option<bool> {
    let mut inside = false;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        if on_segment(a, b, point) {
            return None;
        }
        if (a[1] > point[1]) != (b[1] > point[1]) {
            let x = (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0];
            if point[0] < x {
                inside = !inside;
            }
        }
    }
    Some(inside)
}

fn validate_ring_intersections(ring: &[[f64; 2]], work: &mut usize) -> Result<(), String> {
    for a in 0..ring.len() {
        for b in a + 1..ring.len() {
            *work = work
                .checked_add(1)
                .filter(|n| *n <= MAX_INTERSECTION_TESTS)
                .ok_or("planar_face_intersection_work_budget")?;
            if b == a + 1 || (a == 0 && b + 1 == ring.len()) {
                continue;
            }
            if segments_intersect(
                ring[a],
                ring[(a + 1) % ring.len()],
                ring[b],
                ring[(b + 1) % ring.len()],
            ) {
                return Err("planar_face_ring_self_intersection_or_touch".into());
            }
        }
    }
    Ok(())
}

fn validate_ring_pair(a: &[[f64; 2]], b: &[[f64; 2]], work: &mut usize) -> Result<(), String> {
    for i in 0..a.len() {
        for j in 0..b.len() {
            *work = work
                .checked_add(1)
                .filter(|n| *n <= MAX_INTERSECTION_TESTS)
                .ok_or("planar_face_intersection_work_budget")?;
            if segments_intersect(a[i], a[(i + 1) % a.len()], b[j], b[(j + 1) % b.len()]) {
                return Err("planar_face_rings_cross_or_touch".into());
            }
        }
    }
    Ok(())
}

fn collect_ring(
    step: &StepFile,
    face_id: u64,
    bound_id: u64,
    frame: &PlaneFrame,
    target: f64,
    work: &mut SamplingWork,
) -> Result<Ring, String> {
    let bound = entity(step, bound_id, "FACE_BOUND")?;
    let (source_entity_type, role) = match bound.inst.keyword.as_str() {
        "FACE_OUTER_BOUND" => ("FACE_OUTER_BOUND", "outer"),
        "FACE_BOUND" => ("FACE_BOUND", "inner"),
        _ => return Err("planar_face_unsupported_bound_kind".into()),
    };
    let bound_sense = bound
        .logical("orientation")
        .ok_or("planar_face_bound_unknown_sense")?;
    let loop_id = bound
        .reference("bound")
        .ok_or("planar_face_bound_missing_loop")?;
    let loop_entity = entity(step, loop_id, "EDGE_LOOP")?;
    let refs = loop_entity
        .list("edge_list")
        .filter(|v| !v.is_empty() && v.len() <= MAX_LOOP_EDGES)
        .ok_or("planar_face_loop_edge_limit")?;
    let mut oriented = refs
        .iter()
        .map(|item| {
            item.as_reference()
                .ok_or("planar_face_invalid_oriented_edge_reference")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !bound_sense {
        oriented.reverse();
    }
    let mut samples = Vec::new();
    let mut edge_ids = BTreeSet::new();
    let mut ordered_edge_ids = Vec::new();
    let mut previous_vertex = None;
    let mut first_vertex = None;
    let mut source_edges = Vec::with_capacity(oriented.len());
    for oriented_id in oriented {
        let use_entity = entity(step, oriented_id, "ORIENTED_EDGE")?;
        let orientation = use_entity
            .logical("orientation")
            .ok_or("planar_face_use_unknown_sense")?
            == bound_sense;
        let edge_id = use_entity
            .reference("edge_element")
            .ok_or("planar_face_use_missing_edge")?;
        if !edge_ids.insert(edge_id) {
            return Err("planar_face_duplicate_edge_in_loop".into());
        }
        ordered_edge_ids.push(edge_id);
        let (start, end, same, mut edge_points) = edge_samples(step, edge_id, target, work)?;
        let (traversal_start, traversal_end) = if orientation {
            (start, end)
        } else {
            (end, start)
        };
        if previous_vertex.is_some_and(|previous| previous != traversal_start) {
            return Err("planar_face_source_edge_chain_gap".into());
        }
        first_vertex.get_or_insert(traversal_start);
        previous_vertex = Some(traversal_end);
        if same != orientation {
            edge_points.reverse();
        }
        let expected_start = vertex(step, traversal_start)?;
        let expected_end = vertex(step, traversal_end)?;
        if edge_points
            .first()
            .is_none_or(|sample| !close_enough(sample.point, expected_start))
            || edge_points
                .last()
                .is_none_or(|sample| !close_enough(sample.point, expected_end))
        {
            return Err("planar_face_oriented_curve_endpoint_mismatch".into());
        }
        source_edges.push(SourceEdgeSamples {
            source_edge_curve_id: edge_id,
            traversal_start_vertex_id: traversal_start,
            traversal_end_vertex_id: traversal_end,
            points: edge_points.iter().map(|sample| sample.point).collect(),
        });
        if !samples.is_empty() {
            edge_points.remove(0);
        }
        samples.extend(edge_points);
        if samples.len() > MAX_TOTAL_SAMPLES {
            return Err("planar_face_sample_budget".into());
        }
    }
    if first_vertex != previous_vertex || samples.len() < 4 {
        return Err("planar_face_loop_not_closed_or_too_short".into());
    }
    if !close_enough(
        samples[0].point,
        samples.last().ok_or("planar_face_loop_empty")?.point,
    ) {
        return Err("planar_face_loop_endpoint_coordinates_disagree".into());
    }
    samples.pop();
    let mut points = Vec::with_capacity(samples.len());
    let mut max_plane_residual: f64 = 0.0;
    for sample in &samples {
        let (uv, residual) = project(frame, sample.point);
        if uv.iter().any(|v| !v.is_finite()) || !residual.is_finite() {
            return Err("planar_face_nonfinite_projected_boundary".into());
        }
        max_plane_residual = max_plane_residual.max(residual.abs());
        points.push(uv);
    }
    let scale = points.iter().flatten().fold(1.0_f64, |s, v| s.max(v.abs()));
    if max_plane_residual > 128.0 * f64::EPSILON * scale {
        return Err("planar_face_boundary_not_on_declared_plane_at_numeric_precision".into());
    }
    let area = polygon_area(&points)?;
    let _ = face_id;
    Ok(Ring {
        bound_id,
        loop_id,
        source_entity_type,
        source_role: role,
        samples,
        points,
        signed_area: area,
        edge_count: edge_ids.len(),
        source_edge_curve_ids: ordered_edge_ids,
        source_edges,
    })
}

fn outer_ring_index(rings: &[Ring]) -> Result<usize, String> {
    let outer_indices = rings
        .iter()
        .enumerate()
        .filter_map(|(index, ring)| (ring.source_role == "outer").then_some(index))
        .collect::<Vec<_>>();
    if outer_indices.len() != 1 {
        return Err("planar_face_requires_exactly_one_face_outer_bound".into());
    }
    let outer_index = outer_indices[0];
    if rings.iter().enumerate().any(|(index, ring)| {
        index != outer_index
            && (ring.source_entity_type != "FACE_BOUND" || ring.source_role != "inner")
    }) {
        return Err("planar_face_non_outer_bound_role_unsupported".into());
    }
    Ok(outer_index)
}

fn triangulate(rings: &[Ring], same_sense: bool) -> Result<TriangulatedFace, String> {
    let mut points = Vec::<[f64; 2]>::new();
    let mut xyz = Vec::<P3>::new();
    let mut holes = Vec::<u32>::new();
    for (index, ring) in rings.iter().enumerate() {
        if index > 0 {
            holes.push(u32::try_from(points.len()).map_err(|_| "planar_face_hole_index_overflow")?);
        }
        points.extend(ring.points.iter().copied());
        xyz.extend(ring.samples.iter().map(|sample| sample.point));
    }
    let mut indices = Vec::<u32>::new();
    Earcut::<f64>::new().earcut(points.iter().copied(), &holes, &mut indices);
    if indices.is_empty() || !indices.len().is_multiple_of(3) {
        return Err("planar_face_triangulator_returned_empty_or_malformed_mesh".into());
    }
    let expected_area = rings[0].signed_area.abs()
        - rings
            .iter()
            .skip(1)
            .map(|r| r.signed_area.abs())
            .sum::<f64>();
    if !expected_area.is_finite() || expected_area <= 0.0 {
        return Err("planar_face_holes_exceed_outer_area".into());
    }
    let mut triangles = Vec::with_capacity(indices.len() / 3);
    let mut triangle_area = 0.0;
    let mut boundary_edges = BTreeSet::new();
    let mut offset = 0usize;
    for ring in rings {
        for i in 0..ring.points.len() {
            let a = u32::try_from(offset + i).map_err(|_| "planar_face_boundary_index_overflow")?;
            let b = u32::try_from(offset + (i + 1) % ring.points.len())
                .map_err(|_| "planar_face_boundary_index_overflow")?;
            boundary_edges.insert((a.min(b), a.max(b)));
        }
        offset += ring.points.len();
    }
    let mut edge_uses: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for chunk in indices.chunks_exact(3) {
        let mut tri = [chunk[0], chunk[1], chunk[2]];
        let get = |i: u32| {
            points
                .get(i as usize)
                .copied()
                .ok_or("planar_face_triangulator_index_out_of_range")
        };
        let signed = orient(get(tri[0])?, get(tri[1])?, get(tri[2])?) * 0.5;
        if !signed.is_finite() || signed == 0.0 {
            return Err("planar_face_triangulator_returned_degenerate_triangle".into());
        }
        triangle_area += signed.abs();
        if (signed > 0.0) != same_sense {
            tri.swap(1, 2);
        }
        for i in 0..3 {
            let a = tri[i];
            let b = tri[(i + 1) % 3];
            *edge_uses.entry((a.min(b), a.max(b))).or_default() += 1;
        }
        let center = [
            (points[tri[0] as usize][0] + points[tri[1] as usize][0] + points[tri[2] as usize][0])
                / 3.0,
            (points[tri[0] as usize][1] + points[tri[1] as usize][1] + points[tri[2] as usize][1])
                / 3.0,
        ];
        if point_in_ring(center, &rings[0].points) != Some(true)
            || rings
                .iter()
                .skip(1)
                .any(|hole| point_in_ring(center, &hole.points) != Some(false))
        {
            return Err("planar_face_triangulator_emitted_triangle_outside_material".into());
        }
        triangles.push(tri);
    }
    let area_tolerance = expected_area.abs() * 1e-11
        + f64::EPSILON * expected_area.abs().max(1.0) * points.len() as f64 * 128.0;
    if (triangle_area - expected_area).abs() > area_tolerance {
        return Err("planar_face_triangle_area_disagrees_with_sampled_rings".into());
    }
    for edge in &boundary_edges {
        if edge_uses.get(edge) != Some(&1) {
            return Err("planar_face_triangulation_boundary_incidence_mismatch".into());
        }
    }
    if edge_uses.iter().any(|(edge, count)| {
        if boundary_edges.contains(edge) {
            *count != 1
        } else {
            *count != 2
        }
    }) {
        return Err("planar_face_triangulation_nonmanifold_edge_incidence".into());
    }
    let triangle_pair_tests = triangles
        .len()
        .checked_mul(triangles.len().saturating_sub(1))
        .and_then(|n| n.checked_div(2))
        .filter(|n| *n <= MAX_TRIANGLE_PAIR_TESTS)
        .ok_or("planar_face_triangle_pair_work_budget")?;
    for left in 0..triangles.len() {
        for right in left + 1..triangles.len() {
            if triangle_interiors_overlap(&points, triangles[left], triangles[right]) {
                return Err("planar_face_triangulation_triangle_interiors_overlap".into());
            }
        }
    }
    Ok((
        xyz,
        triangles,
        triangle_area,
        edge_uses.len(),
        triangle_pair_tests,
    ))
}

fn triangle_interiors_overlap(points: &[[f64; 2]], a: [u32; 3], b: [u32; 3]) -> bool {
    let scale = a
        .iter()
        .chain(b.iter())
        .flat_map(|index| points[*index as usize])
        .fold(1.0_f64, |s, v| s.max(v.abs()));
    let projection_tolerance = 128.0 * f64::EPSILON * scale * scale;
    for triangle in [a, b] {
        for i in 0..3 {
            let p = points[triangle[i] as usize];
            let q = points[triangle[(i + 1) % 3] as usize];
            let axis = [p[1] - q[1], q[0] - p[0]];
            let project_triangle = |indices: [u32; 3]| {
                let projected = indices.map(|index| {
                    let point = points[index as usize];
                    point[0] * axis[0] + point[1] * axis[1]
                });
                let minimum = projected.iter().copied().fold(f64::INFINITY, f64::min);
                let maximum = projected.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                (minimum, maximum)
            };
            let (a_min, a_max) = project_triangle(a);
            let (b_min, b_max) = project_triangle(b);
            let overlap = a_max.min(b_max) - a_min.max(b_min);
            if overlap <= projection_tolerance {
                return false;
            }
        }
    }
    true
}

/// Create an explicitly non-production triangulation candidate for one planar
/// face with one source outer bound and zero or more generic `FACE_BOUND`s.
pub(crate) fn inspect(step: &StepFile, face_id: u64, target: f64) -> Result<Json, String> {
    if !target.is_finite() || target <= 0.0 || target > 1.0 {
        return Err("planar_face_chord_target_must_be_positive_and_at_most_one_source_unit".into());
    }
    let face = entity(step, face_id, "ADVANCED_FACE")?;
    let same_sense = face
        .logical("same_sense")
        .ok_or("planar_face_unknown_surface_sense")?;
    let surface_id = face
        .reference("face_geometry")
        .ok_or("planar_face_missing_surface")?;
    let frame = plane_frame(step, surface_id)?;
    let bound_refs = face
        .list("bounds")
        .filter(|v| !v.is_empty() && v.len() <= MAX_BOUNDS)
        .ok_or("planar_face_bound_count_limit")?;
    let bound_ids = bound_refs
        .iter()
        .map(|v| {
            v.as_reference()
                .ok_or("planar_face_invalid_bound_reference")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut work = SamplingWork {
        evaluations: 0,
        control_evaluations: 0,
        retained_curve_control_points: 0,
        sample_count: 0,
        max_sampled_chord_error: 0.0,
    };
    let mut rings = bound_ids
        .iter()
        .map(|&id| collect_ring(step, face_id, id, &frame, target, &mut work))
        .collect::<Result<Vec<_>, _>>()?;
    let outer_index = outer_ring_index(&rings)?;
    if outer_index != 0 {
        rings.swap(0, outer_index);
    }
    let mut intersection_tests = 0usize;
    for ring in &rings {
        validate_ring_intersections(&ring.points, &mut intersection_tests)?;
    }
    for i in 0..rings.len() {
        for j in i + 1..rings.len() {
            validate_ring_pair(&rings[i].points, &rings[j].points, &mut intersection_tests)?;
        }
    }
    for hole in rings.iter().skip(1) {
        if hole
            .points
            .iter()
            .any(|&p| point_in_ring(p, &rings[0].points) != Some(true))
        {
            return Err("planar_face_non_outer_bound_not_strictly_inside_outer_candidate".into());
        }
    }
    for i in 1..rings.len() {
        for j in i + 1..rings.len() {
            if point_in_ring(rings[i].points[0], &rings[j].points) != Some(false)
                || point_in_ring(rings[j].points[0], &rings[i].points) != Some(false)
            {
                return Err(
                    "planar_face_nested_or_overlapping_inner_bound_candidates_unsupported".into(),
                );
            }
        }
    }
    let boundary_sample_count = rings.iter().try_fold(0usize, |n, ring| {
        n.checked_add(ring.samples.len())
            .filter(|v| *v <= MAX_TOTAL_SAMPLES)
            .ok_or("planar_face_sample_budget")
    })?;
    let (vertices, triangles, triangle_area, unique_edges, triangle_pair_tests) =
        triangulate(&rings, same_sense)?;
    Ok(json!({
        "schema":"tokito.planar-multibound-face-candidate.v1",
        "sourceFaceId":face_id,"sourceSurfaceId":surface_id,"surfaceType":"PLANE","faceSameSense":same_sense,
        "rings":rings.iter().map(|ring| json!({"boundId":ring.bound_id,"loopId":ring.loop_id,"sourceEntityType":ring.source_entity_type,"sourceRole":ring.source_role,
            "triangulationTreatment":if ring.source_role == "outer" {"outer"} else {"interior_hole"},
            "sourceEdgeCount":ring.edge_count,"sourceEdgeCurveIds":ring.source_edge_curve_ids,
            "sourceEdges":ring.source_edges.iter().map(|edge| json!({"sourceEdgeCurveId":edge.source_edge_curve_id,
                "traversalStartVertexId":edge.traversal_start_vertex_id,"traversalEndVertexId":edge.traversal_end_vertex_id,
                "samplePointsSourceUnits":edge.points})).collect::<Vec<_>>(),
            "sampleCount":ring.samples.len(),"signedAreaSourceUnitsSquared":ring.signed_area,
            "projectedBoundarySourceUnits":ring.points,"sourceCurveParameters":ring.samples.iter().map(|sample|sample.parameter).collect::<Vec<_>>(),
            "sampleProvenance":ring.samples.iter().map(|sample|json!({"sourceEdgeCurveId":sample.source_edge_curve_id,"sourceVertexId":sample.source_vertex_id,"curveParameter":sample.parameter,"pointSourceUnits":sample.point})).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "mesh":{"verticesSourceUnits":vertices,"triangles":triangles,"triangleAreaSourceUnitsSquared":triangle_area,"uniqueEdgeCount":unique_edges},
        "sampledPolygonMaterialAreaSourceUnitsSquared":rings[0].signed_area.abs()-rings.iter().skip(1).map(|r|r.signed_area.abs()).sum::<f64>(),
        "sampling":{"requestedQuarterMidpointChordTargetSourceUnits":target,"maximumObservedQuarterMidpointChordErrorSourceUnits":work.max_sampled_chord_error,
            "curveEvaluations":work.evaluations,"curveControlPointWork":work.control_evaluations,"retainedCurveControlPoints":work.retained_curve_control_points,
            "boundarySampleCount":boundary_sample_count,"evaluatedBSplineSampleCount":work.sample_count,
            "certifiedContinuousCurveErrorBound":false,"planarityCheck":"source-boundary samples checked against declared PLANE at floating point scale"},
        "validation":{"allSourceLoopsClosedByVertexIdentity":true,"sampledRingsSimpleAndNonTouching":true,"innerBoundsStrictlyInsideOuterCandidate":true,
            "triangulationAreaMatchesSampledRings":true,"boundaryAndInteriorEdgeIncidenceVerified":true,"centroidMaterialChecks":true,
            "triangleInteriorsDoNotOverlap":true,"exactlyOneSourceOuterBound":true,"trianglePairTests":triangle_pair_tests,"intersectionTests":intersection_tests},
        "geometryQualified":false,"completeFaceGeometryCertified":false,"completeSolidVerified":false,"admitted":false,"exportsProduced":[],
        "scope":"single sampled planar face candidate; with exactly one declared FACE_OUTER_BOUND, remaining FACE_BOUND loops have inner-face semantics under ISO 10303-42; sampled geometry is still diagnostic only, with no STEP source mutation, full component qualification, production export, or continuous deviation certificate"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(points: &[[f64; 2]]) -> Vec<Ring> {
        vec![Ring {
            bound_id: 1,
            loop_id: 2,
            source_entity_type: "FACE_OUTER_BOUND",
            source_role: "outer",
            samples: points
                .iter()
                .map(|p| Sample {
                    point: [p[0], p[1], 0.0],
                    parameter: None,
                    source_edge_curve_id: None,
                    source_vertex_id: None,
                })
                .collect(),
            points: points.to_vec(),
            signed_area: polygon_area(points).unwrap(),
            edge_count: points.len(),
            source_edge_curve_ids: (0..points.len() as u64).collect(),
            source_edges: Vec::new(),
        }]
    }

    #[test]
    fn ring_geometry_detects_self_crossing_and_boundary_touch() {
        let mut work = 0;
        assert!(
            validate_ring_intersections(&[[0., 0.], [2., 0.], [2., 2.], [0., 2.]], &mut work)
                .is_ok()
        );
        assert!(
            validate_ring_intersections(&[[0., 0.], [2., 2.], [0., 2.], [2., 0.]], &mut work)
                .is_err()
        );
        assert_eq!(
            point_in_ring([1., 1.], &[[0., 0.], [2., 0.], [2., 2.], [0., 2.]]),
            Some(true)
        );
        assert_eq!(
            point_in_ring([0., 1.], &[[0., 0.], [2., 0.], [2., 2.], [0., 2.]]),
            None
        );
    }

    #[test]
    fn sampled_face_triangulation_preserves_hole_area_and_boundary_incidence() {
        let outer = [[0., 0.], [8., 0.], [8., 8.], [0., 8.]];
        let hole = [[2., 2.], [2., 6.], [6., 6.], [6., 2.]];
        let mut rings = ring(&outer);
        rings.push(Ring {
            bound_id: 3,
            loop_id: 4,
            source_entity_type: "FACE_BOUND",
            source_role: "inner",
            samples: hole
                .iter()
                .map(|p| Sample {
                    point: [p[0], p[1], 0.],
                    parameter: None,
                    source_edge_curve_id: None,
                    source_vertex_id: None,
                })
                .collect(),
            points: hole.to_vec(),
            signed_area: polygon_area(&hole).unwrap(),
            edge_count: 4,
            source_edge_curve_ids: vec![10, 11, 12, 13],
            source_edges: Vec::new(),
        });
        let (vertices, triangles, area, edges, pair_tests) = triangulate(&rings, true).unwrap();
        assert_eq!(vertices.len(), 8);
        assert!(!triangles.is_empty());
        assert_eq!(area, 48.0);
        assert_eq!(edges, 16);
        assert!(pair_tests > 0);
    }

    #[test]
    fn explicit_outer_bound_gives_generic_face_bounds_inner_semantics() {
        let mut rings = ring(&[[0., 0.], [8., 0.], [8., 8.], [0., 8.]]);
        rings.push(Ring {
            bound_id: 3,
            loop_id: 4,
            source_entity_type: "FACE_BOUND",
            source_role: "inner",
            samples: Vec::new(),
            points: Vec::new(),
            signed_area: 0.0,
            edge_count: 0,
            source_edge_curve_ids: Vec::new(),
            source_edges: Vec::new(),
        });
        assert_eq!(outer_ring_index(&rings), Ok(0));
    }

    #[test]
    fn planar_face_bound_roles_require_exactly_one_declared_outer() {
        let mut no_outer = ring(&[[0., 0.], [8., 0.], [8., 8.], [0., 8.]]);
        no_outer[0].source_entity_type = "FACE_BOUND";
        no_outer[0].source_role = "inner";
        assert_eq!(
            outer_ring_index(&no_outer).unwrap_err(),
            "planar_face_requires_exactly_one_face_outer_bound"
        );

        let mut two_outers = ring(&[[0., 0.], [8., 0.], [8., 8.], [0., 8.]]);
        two_outers.push(Ring {
            bound_id: 3,
            loop_id: 4,
            source_entity_type: "FACE_OUTER_BOUND",
            source_role: "outer",
            samples: Vec::new(),
            points: Vec::new(),
            signed_area: 0.0,
            edge_count: 0,
            source_edge_curve_ids: Vec::new(),
            source_edges: Vec::new(),
        });
        assert_eq!(
            outer_ring_index(&two_outers).unwrap_err(),
            "planar_face_requires_exactly_one_face_outer_bound"
        );
    }

    #[test]
    fn triangle_overlap_check_rejects_positive_area_and_allows_shared_edge_touching() {
        let points = [[0., 0.], [4., 0.], [0., 4.], [1., 1.], [0., -4.]];
        assert!(triangle_interiors_overlap(&points, [0, 1, 2], [0, 1, 3]));
        assert!(!triangle_interiors_overlap(&points, [0, 1, 2], [1, 0, 4]));
    }

    #[test]
    fn sampled_face_triangulation_refuses_a_hole_outside_or_crossing_outer_ring() {
        let outer = [[0., 0.], [4., 0.], [4., 4.], [0., 4.]];
        let outside = [[5., 5.], [6., 5.], [6., 6.], [5., 6.]];
        let mut work = 0;
        assert!(validate_ring_pair(&outer, &outside, &mut work).is_ok());
        assert_eq!(point_in_ring(outside[0], &outer), Some(false));
        let crossing = [[3., 3.], [5., 3.], [5., 5.], [3., 5.]];
        assert!(validate_ring_pair(&outer, &crossing, &mut work).is_err());
    }
}
