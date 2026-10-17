//! Bounded triangulation of one sampled, simple UV trim loop.
//!
//! This is a diagnostic candidate only. It does not support holes, multiple
//! bounds, self-intersections, or production geometry admission.

const MAX_LOOP_VERTICES: usize = 4096;
const MAX_PREDICATE_CHECKS: usize = 10_000_000;
const MAX_REFINED_TRIANGLES: usize = 200_000;
const MAX_REFINEMENT_LEVELS: usize = 5;

#[derive(Debug)]
pub struct Candidate {
    pub vertices: Vec<[f64; 2]>,
    pub triangles: Vec<[usize; 3]>,
    /// For every submitted loop sample, the index of its retained vertex.
    /// Adjacent duplicates and a repeated closing point map to their retained peer.
    pub input_vertex_indices: Vec<usize>,
    pub signed_area: f64,
    pub removed_adjacent_duplicates: usize,
    pub predicate_checks: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct BoundaryCurveSegment {
    pub start_vertex: usize,
    pub end_vertex: usize,
    pub curve_index: usize,
    pub start_parameter: f64,
    pub end_parameter: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct BoundaryVertexPoint {
    pub vertex_index: usize,
    pub point: [f64; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct RefinementOptions {
    pub chord_tolerance: f64,
    pub max_refinement_levels: usize,
    pub max_triangles: usize,
}

#[derive(Debug)]
pub struct RefinedCandidate {
    pub vertices: Vec<[f64; 2]>,
    pub points: Vec<[f64; 3]>,
    pub triangles: Vec<[usize; 3]>,
    /// Stable mapping from each submitted loop sample to its refined mesh vertex.
    pub input_vertex_indices: Vec<usize>,
    pub signed_area: f64,
    pub removed_adjacent_duplicates: usize,
    pub predicate_checks: usize,
    pub refinement_levels: usize,
    pub max_chord_error: f64,
    pub chord_tolerance: f64,
    pub chord_converged: bool,
    pub orientation_converged: Option<bool>,
    pub orientation_check_count: usize,
    pub minimum_normal_alignment: Option<f64>,
    pub nonpositive_normal_count: usize,
    pub converged: bool,
    pub failure_reason: Option<&'static str>,
}

type TriangleOrientationCheck<'a> =
    dyn FnMut([[f64; 2]; 3], [[f64; 3]; 3]) -> Result<f64, String> + 'a;

fn cross(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn distance_squared(a: [f64; 2], b: [f64; 2]) -> f64 {
    (b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)
}

fn within(a: f64, b: f64, value: f64, epsilon: f64) -> bool {
    value >= a.min(b) - epsilon && value <= a.max(b) + epsilon
}

fn on_segment(
    a: [f64; 2],
    b: [f64; 2],
    p: [f64; 2],
    area_epsilon: f64,
    coordinate_epsilon: f64,
) -> bool {
    cross(a, b, p).abs() <= area_epsilon
        && within(a[0], b[0], p[0], coordinate_epsilon)
        && within(a[1], b[1], p[1], coordinate_epsilon)
}

fn segments_intersect(
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
    d: [f64; 2],
    area_epsilon: f64,
    coordinate_epsilon: f64,
) -> bool {
    let ab_c = cross(a, b, c);
    let ab_d = cross(a, b, d);
    let cd_a = cross(c, d, a);
    let cd_b = cross(c, d, b);
    if ((ab_c > area_epsilon && ab_d < -area_epsilon)
        || (ab_c < -area_epsilon && ab_d > area_epsilon))
        && ((cd_a > area_epsilon && cd_b < -area_epsilon)
            || (cd_a < -area_epsilon && cd_b > area_epsilon))
    {
        return true;
    }
    (ab_c.abs() <= area_epsilon && on_segment(a, b, c, area_epsilon, coordinate_epsilon))
        || (ab_d.abs() <= area_epsilon && on_segment(a, b, d, area_epsilon, coordinate_epsilon))
        || (cd_a.abs() <= area_epsilon && on_segment(c, d, a, area_epsilon, coordinate_epsilon))
        || (cd_b.abs() <= area_epsilon && on_segment(c, d, b, area_epsilon, coordinate_epsilon))
}

fn signed_area(points: &[[f64; 2]]) -> f64 {
    // Translate the polygon to its first vertex before summing cross products.
    // The usual uncentered shoelace formula subtracts products proportional to
    // the absolute coordinate offset and can erase a small polygon's area.
    let origin = points[0];
    let mut twice_area = 0.0;
    let mut compensation = 0.0;
    for index in 1..points.len() - 1 {
        let a = [points[index][0] - origin[0], points[index][1] - origin[1]];
        let b = [
            points[index + 1][0] - origin[0],
            points[index + 1][1] - origin[1],
        ];
        let term = a[0] * b[1] - b[0] * a[1];
        // Kahan summation limits rounding loss for large, concave loops.
        let corrected = term - compensation;
        let next = twice_area + corrected;
        compensation = (next - twice_area) - corrected;
        twice_area = next;
    }
    twice_area * 0.5
}

fn point_in_or_on_triangle(
    point: [f64; 2],
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
    winding: f64,
    epsilon: f64,
) -> bool {
    winding * cross(a, b, point) >= -epsilon
        && winding * cross(b, c, point) >= -epsilon
        && winding * cross(c, a, point) >= -epsilon
}

/// Ear-clip a bounded, sampled, simple single-bound UV loop.
///
/// Consecutive duplicates and a repeated closing point are removed. The
/// remaining loop must be simple, non-degenerate, and have no holes.
pub fn triangulate_simple_loop(input: &[[f64; 2]]) -> Result<Candidate, String> {
    if input.len() < 3 || input.len() > MAX_LOOP_VERTICES {
        return Err("trim_candidate_vertex_limit".into());
    }
    if input
        .iter()
        .flatten()
        .any(|value| !value.is_finite() || value.abs() > 1e12)
    {
        return Err("trim_candidate_nonfinite_or_unbounded_uv".into());
    }

    let min_x = input.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let max_x = input.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
    let min_y = input.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let max_y = input.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
    let span = (max_x - min_x).max(max_y - min_y);
    if !span.is_finite() || span <= 0. {
        return Err("trim_candidate_zero_extent".into());
    }
    let duplicate_epsilon = span * 1e-12;
    let orientation_epsilon = span * span * 1e-12;
    let coordinate_epsilon = span * 1e-12;
    let mut vertices = Vec::with_capacity(input.len());
    let mut input_vertex_indices = Vec::with_capacity(input.len());
    let mut removed_adjacent_duplicates = 0usize;
    for point in input.iter().copied() {
        if vertices
            .last()
            .is_some_and(|last| distance_squared(*last, point) <= duplicate_epsilon.powi(2))
        {
            input_vertex_indices.push(vertices.len() - 1);
            removed_adjacent_duplicates += 1;
        } else {
            input_vertex_indices.push(vertices.len());
            vertices.push(point);
        }
    }
    if vertices.len() > 1
        && distance_squared(vertices[0], *vertices.last().expect("nonempty checked"))
            <= duplicate_epsilon.powi(2)
    {
        let removed_index = vertices.len() - 1;
        vertices.pop();
        for mapped_index in &mut input_vertex_indices {
            if *mapped_index == removed_index {
                *mapped_index = 0;
            }
        }
        removed_adjacent_duplicates += 1;
    }
    if vertices.len() < 3 {
        return Err("trim_candidate_has_fewer_than_three_vertices".into());
    }

    let mut predicate_checks = 0usize;
    for left in 0..vertices.len() {
        let left_next = (left + 1) % vertices.len();
        for right in (left + 1)..vertices.len() {
            let right_next = (right + 1) % vertices.len();
            if left == right || left_next == right || right_next == left {
                continue;
            }
            predicate_checks += 1;
            if predicate_checks > MAX_PREDICATE_CHECKS {
                return Err("trim_candidate_predicate_budget_exceeded".into());
            }
            if segments_intersect(
                vertices[left],
                vertices[left_next],
                vertices[right],
                vertices[right_next],
                orientation_epsilon,
                coordinate_epsilon,
            ) {
                return Err("trim_candidate_self_intersection".into());
            }
        }
    }

    let area = signed_area(&vertices);
    if !area.is_finite() || area.abs() <= orientation_epsilon {
        return Err("trim_candidate_zero_or_invalid_area".into());
    }
    let winding = area.signum();
    let mut remaining = (0..vertices.len()).collect::<Vec<_>>();
    let mut triangles = Vec::with_capacity(vertices.len() - 2);
    while remaining.len() > 3 {
        let mut ear = None;
        for cursor in 0..remaining.len() {
            predicate_checks += 1;
            if predicate_checks > MAX_PREDICATE_CHECKS {
                return Err("trim_candidate_predicate_budget_exceeded".into());
            }
            let previous = remaining[(cursor + remaining.len() - 1) % remaining.len()];
            let current = remaining[cursor];
            let next = remaining[(cursor + 1) % remaining.len()];
            if winding * cross(vertices[previous], vertices[current], vertices[next])
                <= orientation_epsilon
            {
                continue;
            }
            let mut contains_vertex = false;
            for &candidate in &remaining {
                if candidate == previous || candidate == current || candidate == next {
                    continue;
                }
                predicate_checks += 1;
                if predicate_checks > MAX_PREDICATE_CHECKS {
                    return Err("trim_candidate_predicate_budget_exceeded".into());
                }
                if point_in_or_on_triangle(
                    vertices[candidate],
                    vertices[previous],
                    vertices[current],
                    vertices[next],
                    winding,
                    orientation_epsilon,
                ) {
                    contains_vertex = true;
                    break;
                }
            }
            if !contains_vertex {
                ear = Some((cursor, [previous, current, next]));
                break;
            }
        }
        let Some((cursor, triangle)) = ear else {
            return Err("trim_candidate_ear_clipping_stalled".into());
        };
        triangles.push(triangle);
        remaining.remove(cursor);
    }
    let final_triangle = [remaining[0], remaining[1], remaining[2]];
    if winding
        * cross(
            vertices[final_triangle[0]],
            vertices[final_triangle[1]],
            vertices[final_triangle[2]],
        )
        <= orientation_epsilon
    {
        return Err("trim_candidate_final_triangle_degenerate".into());
    }
    triangles.push(final_triangle);
    if triangles.len() != vertices.len() - 2 {
        return Err("trim_candidate_triangle_count_mismatch".into());
    }
    Ok(Candidate {
        vertices,
        triangles,
        input_vertex_indices,
        signed_area: area,
        removed_adjacent_duplicates,
        predicate_checks,
    })
}

fn distance3(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3)
        .map(|axis| (a[axis] - b[axis]).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// Uniformly refine a UV triangulation until sampled surface-to-chord error
/// meets the diagnostic tolerance or a hard level/triangle budget is reached.
#[cfg(test)]
pub fn refine_uniform<F>(
    candidate: Candidate,
    chord_tolerance: f64,
    max_refinement_levels: usize,
    max_triangles: usize,
    evaluate_surface: F,
) -> Result<RefinedCandidate, String>
where
    F: FnMut([f64; 2]) -> Result<[f64; 3], String>,
{
    refine_uniform_with_boundary(
        candidate,
        RefinementOptions {
            chord_tolerance,
            max_refinement_levels,
            max_triangles,
        },
        &[],
        &[],
        evaluate_surface,
        |_, _, _, _| Err("unexpected_boundary_midpoint_evaluation".into()),
    )
}

/// Refine a surface triangulation while sampling declared trim-curve edges
/// from their source curve instead of interpolating those edges in UV.
#[cfg(test)]
pub fn refine_uniform_with_boundary<F, B>(
    candidate: Candidate,
    options: RefinementOptions,
    boundary_segments: &[BoundaryCurveSegment],
    boundary_vertices: &[BoundaryVertexPoint],
    evaluate_surface: F,
    mut evaluate_boundary_midpoint: B,
) -> Result<RefinedCandidate, String>
where
    F: FnMut([f64; 2]) -> Result<[f64; 3], String>,
    B: FnMut(
        BoundaryCurveSegment,
        [f64; 2],
        [f64; 2],
        usize,
    ) -> Result<([f64; 2], [f64; 3]), String>,
{
    refine_uniform_internal(
        candidate,
        options,
        boundary_segments,
        boundary_vertices,
        evaluate_surface,
        &mut evaluate_boundary_midpoint,
        None,
    )
}

/// Refine a surface triangulation until both sampled chord error and the
/// supplied oriented-normal checks pass, or a hard level/triangle budget is
/// reached. Normal alignment is signed: nonpositive values are never treated
/// as a passing orientation.
pub fn refine_uniform_with_boundary_and_orientation<F, B, O>(
    candidate: Candidate,
    options: RefinementOptions,
    boundary_segments: &[BoundaryCurveSegment],
    boundary_vertices: &[BoundaryVertexPoint],
    evaluate_surface: F,
    mut evaluate_boundary_midpoint: B,
    mut evaluate_triangle_orientation: O,
) -> Result<RefinedCandidate, String>
where
    F: FnMut([f64; 2]) -> Result<[f64; 3], String>,
    B: FnMut(
        BoundaryCurveSegment,
        [f64; 2],
        [f64; 2],
        usize,
    ) -> Result<([f64; 2], [f64; 3]), String>,
    O: FnMut([[f64; 2]; 3], [[f64; 3]; 3]) -> Result<f64, String>,
{
    refine_uniform_internal(
        candidate,
        options,
        boundary_segments,
        boundary_vertices,
        evaluate_surface,
        &mut evaluate_boundary_midpoint,
        Some(&mut evaluate_triangle_orientation),
    )
}

fn refine_uniform_internal<F, B>(
    mut candidate: Candidate,
    options: RefinementOptions,
    initial_boundary_segments: &[BoundaryCurveSegment],
    initial_boundary_vertices: &[BoundaryVertexPoint],
    mut evaluate_surface: F,
    evaluate_boundary_midpoint: &mut B,
    mut evaluate_triangle_orientation: Option<&mut TriangleOrientationCheck<'_>>,
) -> Result<RefinedCandidate, String>
where
    F: FnMut([f64; 2]) -> Result<[f64; 3], String>,
    B: FnMut(
        BoundaryCurveSegment,
        [f64; 2],
        [f64; 2],
        usize,
    ) -> Result<([f64; 2], [f64; 3]), String>,
{
    if !options.chord_tolerance.is_finite()
        || options.chord_tolerance <= 0.
        || options.max_refinement_levels > MAX_REFINEMENT_LEVELS
        || options.max_triangles == 0
        || options.max_triangles > MAX_REFINED_TRIANGLES
        || candidate.triangles.len() > options.max_triangles
    {
        return Err("trim_candidate_refinement_limits_invalid".into());
    }
    let mut boundary_segments = std::collections::BTreeMap::new();
    if !initial_boundary_segments.is_empty() && candidate.vertices.len() < 3 {
        return Err("trim_candidate_boundary_curve_segment_invalid".into());
    }
    for segment in initial_boundary_segments {
        let vertex_count = candidate.vertices.len();
        if segment.start_vertex >= vertex_count
            || segment.end_vertex >= vertex_count
            || segment.start_vertex == segment.end_vertex
            || !((segment.start_vertex + 1) % vertex_count == segment.end_vertex
                || (segment.end_vertex + 1) % vertex_count == segment.start_vertex)
            || !segment.start_parameter.is_finite()
            || !segment.end_parameter.is_finite()
        {
            return Err("trim_candidate_boundary_curve_segment_invalid".into());
        }
        let key = (
            segment.start_vertex.min(segment.end_vertex),
            segment.start_vertex.max(segment.end_vertex),
        );
        if boundary_segments.insert(key, *segment).is_some() {
            return Err("trim_candidate_boundary_curve_segment_duplicate".into());
        }
    }
    let mut points = candidate
        .vertices
        .iter()
        .copied()
        .map(&mut evaluate_surface)
        .collect::<Result<Vec<_>, _>>()?;
    if points.iter().flatten().any(|value| !value.is_finite()) {
        return Err("trim_candidate_surface_evaluation_nonfinite".into());
    }
    let mut boundary_vertex_points = std::collections::BTreeMap::new();
    for boundary in initial_boundary_vertices {
        if boundary.vertex_index >= candidate.vertices.len()
            || boundary.point.iter().any(|value| !value.is_finite())
        {
            return Err("trim_candidate_boundary_vertex_point_invalid".into());
        }
        if let Some(existing) = boundary_vertex_points.get(&boundary.vertex_index) {
            if existing != &boundary.point {
                return Err("trim_candidate_boundary_vertex_point_conflict".into());
            }
        } else {
            boundary_vertex_points.insert(boundary.vertex_index, boundary.point);
        }
    }
    for (vertex_index, point) in boundary_vertex_points {
        points[vertex_index] = point;
    }
    let mut surface_evaluation_count = points.len();
    for level in 0..=options.max_refinement_levels {
        let level_vertex_count = candidate.vertices.len();
        let mut edge_midpoints = std::collections::BTreeMap::<(usize, usize), usize>::new();
        let mut next_boundary_segments = std::collections::BTreeMap::new();
        for triangle in &candidate.triangles {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let key = (a.min(b), a.max(b));
                if let std::collections::btree_map::Entry::Vacant(entry) = edge_midpoints.entry(key)
                {
                    let boundary_segment = boundary_segments.get(&key).copied();
                    let (uv, point) = if let Some(segment) = boundary_segment {
                        let midpoint_index = candidate.vertices.len();
                        let (uv, point) = evaluate_boundary_midpoint(
                            segment,
                            candidate.vertices[segment.start_vertex],
                            candidate.vertices[segment.end_vertex],
                            midpoint_index,
                        )?;
                        let parameter_midpoint =
                            (segment.start_parameter + segment.end_parameter) * 0.5;
                        let first_child = BoundaryCurveSegment {
                            start_vertex: segment.start_vertex,
                            end_vertex: midpoint_index,
                            curve_index: segment.curve_index,
                            start_parameter: segment.start_parameter,
                            end_parameter: parameter_midpoint,
                        };
                        let second_child = BoundaryCurveSegment {
                            start_vertex: midpoint_index,
                            end_vertex: segment.end_vertex,
                            curve_index: segment.curve_index,
                            start_parameter: parameter_midpoint,
                            end_parameter: segment.end_parameter,
                        };
                        next_boundary_segments.insert(
                            (
                                first_child.start_vertex.min(first_child.end_vertex),
                                first_child.start_vertex.max(first_child.end_vertex),
                            ),
                            first_child,
                        );
                        next_boundary_segments.insert(
                            (
                                second_child.start_vertex.min(second_child.end_vertex),
                                second_child.start_vertex.max(second_child.end_vertex),
                            ),
                            second_child,
                        );
                        (uv, point)
                    } else {
                        let uv = [
                            (candidate.vertices[a][0] + candidate.vertices[b][0]) * 0.5,
                            (candidate.vertices[a][1] + candidate.vertices[b][1]) * 0.5,
                        ];
                        (uv, evaluate_surface(uv)?)
                    };
                    if point.iter().any(|value| !value.is_finite()) {
                        return Err("trim_candidate_surface_evaluation_nonfinite".into());
                    }
                    let index = candidate.vertices.len();
                    candidate.vertices.push(uv);
                    points.push(point);
                    entry.insert(index);
                    surface_evaluation_count = surface_evaluation_count
                        .checked_add(1)
                        .ok_or("trim_candidate_surface_evaluation_count_overflow")?;
                }
            }
        }

        let mut max_chord_error: f64 = 0.0;
        let mut orientation_check_count = 0usize;
        let mut minimum_normal_alignment: Option<f64> = None;
        let mut nonpositive_normal_count = 0usize;
        for triangle in &candidate.triangles {
            let [a, b, c] = *triangle;
            let centroid_uv = [
                (candidate.vertices[a][0] + candidate.vertices[b][0] + candidate.vertices[c][0])
                    / 3.0,
                (candidate.vertices[a][1] + candidate.vertices[b][1] + candidate.vertices[c][1])
                    / 3.0,
            ];
            let centroid = evaluate_surface(centroid_uv)?;
            if centroid.iter().any(|value| !value.is_finite()) {
                return Err("trim_candidate_surface_evaluation_nonfinite".into());
            }
            surface_evaluation_count = surface_evaluation_count
                .checked_add(1)
                .ok_or("trim_candidate_surface_evaluation_count_overflow")?;
            let linear_centroid = std::array::from_fn(|axis| {
                (points[a][axis] + points[b][axis] + points[c][axis]) / 3.0
            });
            max_chord_error = max_chord_error.max(distance3(centroid, linear_centroid));

            if let Some(check) = evaluate_triangle_orientation.as_mut() {
                let alignment = (**check)(
                    [
                        candidate.vertices[a],
                        candidate.vertices[b],
                        candidate.vertices[c],
                    ],
                    [points[a], points[b], points[c]],
                )?;
                if !alignment.is_finite() || !(-1.000_001..=1.000_001).contains(&alignment) {
                    return Err("trim_candidate_triangle_normal_alignment_invalid".into());
                }
                let alignment = alignment.clamp(-1.0, 1.0);
                orientation_check_count = orientation_check_count
                    .checked_add(1)
                    .ok_or("trim_candidate_orientation_check_count_overflow")?;
                minimum_normal_alignment = Some(
                    minimum_normal_alignment.map_or(alignment, |current| current.min(alignment)),
                );
                nonpositive_normal_count += usize::from(alignment <= 0.0);
            }

            for (first, second) in [(a, b), (b, c), (c, a)] {
                let key = (first.min(second), first.max(second));
                let middle = *edge_midpoints
                    .get(&key)
                    .ok_or("trim_candidate_edge_midpoint_missing")?;
                let linear_midpoint =
                    std::array::from_fn(|axis| (points[first][axis] + points[second][axis]) * 0.5);
                max_chord_error = max_chord_error.max(distance3(points[middle], linear_midpoint));
            }
        }
        let chord_converged = max_chord_error <= options.chord_tolerance;
        let orientation_converged = evaluate_triangle_orientation
            .as_ref()
            .map(|_| nonpositive_normal_count == 0);
        let converged = chord_converged && orientation_converged.unwrap_or(true);
        if converged {
            candidate.vertices.truncate(level_vertex_count);
            points.truncate(level_vertex_count);
            return Ok(RefinedCandidate {
                vertices: candidate.vertices,
                points,
                triangles: candidate.triangles,
                input_vertex_indices: candidate.input_vertex_indices,
                signed_area: candidate.signed_area,
                removed_adjacent_duplicates: candidate.removed_adjacent_duplicates,
                predicate_checks: candidate.predicate_checks,
                refinement_levels: level,
                max_chord_error,
                chord_tolerance: options.chord_tolerance,
                chord_converged,
                orientation_converged,
                orientation_check_count,
                minimum_normal_alignment,
                nonpositive_normal_count,
                converged: true,
                failure_reason: None,
            });
        }
        if level == options.max_refinement_levels {
            candidate.vertices.truncate(level_vertex_count);
            points.truncate(level_vertex_count);
            return Ok(RefinedCandidate {
                vertices: candidate.vertices,
                points,
                triangles: candidate.triangles,
                input_vertex_indices: candidate.input_vertex_indices,
                signed_area: candidate.signed_area,
                removed_adjacent_duplicates: candidate.removed_adjacent_duplicates,
                predicate_checks: candidate.predicate_checks,
                refinement_levels: level,
                max_chord_error,
                chord_tolerance: options.chord_tolerance,
                chord_converged,
                orientation_converged,
                orientation_check_count,
                minimum_normal_alignment,
                nonpositive_normal_count,
                converged: false,
                failure_reason: Some(if !chord_converged {
                    "trim_candidate_chord_tolerance_not_reached"
                } else {
                    "trim_candidate_normal_orientation_not_reached"
                }),
            });
        }
        if candidate.triangles.len().saturating_mul(4) > options.max_triangles {
            candidate.vertices.truncate(level_vertex_count);
            points.truncate(level_vertex_count);
            return Ok(RefinedCandidate {
                vertices: candidate.vertices,
                points,
                triangles: candidate.triangles,
                input_vertex_indices: candidate.input_vertex_indices,
                signed_area: candidate.signed_area,
                removed_adjacent_duplicates: candidate.removed_adjacent_duplicates,
                predicate_checks: candidate.predicate_checks,
                refinement_levels: level,
                max_chord_error,
                chord_tolerance: options.chord_tolerance,
                chord_converged,
                orientation_converged,
                orientation_check_count,
                minimum_normal_alignment,
                nonpositive_normal_count,
                converged: false,
                failure_reason: Some("trim_candidate_triangle_budget_exceeded"),
            });
        }
        let mut refined = Vec::with_capacity(candidate.triangles.len() * 4);
        for triangle in candidate.triangles {
            let [a, b, c] = triangle;
            let ab = edge_midpoints[&(a.min(b), a.max(b))];
            let bc = edge_midpoints[&(b.min(c), b.max(c))];
            let ca = edge_midpoints[&(c.min(a), c.max(a))];
            refined.push([a, ab, ca]);
            refined.push([ab, b, bc]);
            refined.push([ca, bc, c]);
            refined.push([ab, bc, ca]);
        }
        candidate.triangles = refined;
        boundary_segments = next_boundary_segments;
    }
    Err("trim_candidate_refinement_unreachable_state".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positive_z_alignment(points: [[f64; 3]; 3]) -> Result<f64, String> {
        let ab = [
            points[1][0] - points[0][0],
            points[1][1] - points[0][1],
            points[1][2] - points[0][2],
        ];
        let ac = [
            points[2][0] - points[0][0],
            points[2][1] - points[0][1],
            points[2][2] - points[0][2],
        ];
        let normal = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let length = normal.iter().map(|value| value * value).sum::<f64>().sqrt();
        if !length.is_finite() || length <= f64::MIN_POSITIVE {
            return Err("planar_test_triangle_normal_invalid".into());
        }
        Ok(normal[2] / length)
    }

    #[test]
    fn triangulates_convex_loop_and_preserves_winding() {
        for points in [
            vec![[0., 0.], [2., 0.], [2., 1.], [0., 1.]],
            vec![[0., 1.], [2., 1.], [2., 0.], [0., 0.]],
        ] {
            let result = triangulate_simple_loop(&points).unwrap();
            assert_eq!(result.vertices.len(), 4);
            assert_eq!(result.triangles.len(), 2);
            assert_eq!(result.signed_area.signum(), signed_area(&points).signum());
            for [a, b, c] in result.triangles {
                assert!(
                    cross(result.vertices[a], result.vertices[b], result.vertices[c]).signum()
                        == result.signed_area.signum()
                );
            }
        }
    }

    #[test]
    fn triangulates_concave_loop_and_removes_duplicate_closure() {
        let points = [[0., 0.], [2., 0.], [2., 2.], [1., 1.], [0., 2.], [0., 0.]];
        let result = triangulate_simple_loop(&points).unwrap();
        assert_eq!(result.vertices.len(), 5);
        assert_eq!(result.triangles.len(), 3);
        assert_eq!(result.removed_adjacent_duplicates, 1);
    }

    #[test]
    fn retained_vertices_map_adjacent_and_closing_source_samples() {
        let points = [[0., 0.], [0., 0.], [2., 0.], [2., 2.], [0., 2.], [0., 0.]];
        let result = triangulate_simple_loop(&points).unwrap();
        assert_eq!(result.vertices.len(), 4);
        assert_eq!(result.input_vertex_indices, [0, 0, 1, 2, 3, 0]);
    }

    #[test]
    fn rejects_self_intersection_degenerate_area_and_nonfinite_coordinates() {
        assert_eq!(
            triangulate_simple_loop(&[[0., 0.], [2., 2.], [0., 2.], [2., 0.]]).unwrap_err(),
            "trim_candidate_self_intersection"
        );
        assert_eq!(
            triangulate_simple_loop(&[[0., 0.], [1., 0.], [2., 0.]]).unwrap_err(),
            "trim_candidate_zero_or_invalid_area"
        );
        assert_eq!(
            triangulate_simple_loop(&[[0., 0.], [1., 0.], [f64::NAN, 1.]]).unwrap_err(),
            "trim_candidate_nonfinite_or_unbounded_uv"
        );
    }

    #[test]
    fn segment_intersection_coordinates_are_scale_invariant() {
        // This simple polygon has two separated collinear boundary segments
        // whose gap is small relative to the total parameter span. Mixing the
        // area epsilon into coordinate comparisons falsely closes that gap at
        // this scale.
        let large = [
            [0.0, 0.0],
            [1_000.0, 0.0],
            [1_000.0, 1_000.0],
            [2_000.0, 1_000.0],
            [2_000.0, 0.0],
            [3_000.0, 0.0],
            [3_000.0, -1_000.0],
            [4_000.0, -1_000.0],
            [4_000.0, 100_000_000.0],
            [-100_000_000.0, 100_000_000.0],
            [-100_000_000.0, -1_000.0],
            [0.0, -1_000.0],
        ];
        let small = large.map(|[x, y]| [x * 1e-8, y * 1e-8]);

        let large_candidate = triangulate_simple_loop(&large).unwrap();
        let small_candidate = triangulate_simple_loop(&small).unwrap();
        assert_eq!(large_candidate.triangles.len(), large.len() - 2);
        assert_eq!(small_candidate.triangles.len(), small.len() - 2);
        assert_eq!(
            large_candidate.signed_area.signum(),
            small_candidate.signed_area.signum()
        );
    }

    #[test]
    fn small_polygon_area_is_stable_at_large_coordinate_offset() {
        let origin = 999_999_999_999.0;
        let translated = [
            [origin, origin],
            [origin + 1.0, origin],
            [origin + 1.0, origin + 1.0],
            [origin, origin + 1.0],
        ];

        let candidate = triangulate_simple_loop(&translated).unwrap();
        assert_eq!(candidate.signed_area, 1.0);
        assert_eq!(candidate.triangles.len(), 2);
    }

    #[test]
    fn sampled_refinement_converges_for_planar_surface_without_changing_topology() {
        let candidate = triangulate_simple_loop(&[[0., 0.], [1., 0.], [1., 1.], [0., 1.]]).unwrap();
        let refined = refine_uniform(candidate, 1e-8, 2, 64, |uv| Ok([uv[0], uv[1], 0.0])).unwrap();
        assert!(refined.converged);
        assert!(refined.chord_converged);
        assert_eq!(refined.orientation_converged, None);
        assert_eq!(refined.refinement_levels, 0);
        assert_eq!(refined.triangles.len(), 2);
        assert!(refined.max_chord_error <= 1e-8);
    }

    #[test]
    fn oriented_refinement_converges_for_a_consistent_planar_patch() {
        let candidate = triangulate_simple_loop(&[[0., 0.], [1., 0.], [1., 1.], [0., 1.]]).unwrap();
        let refined = refine_uniform_with_boundary_and_orientation(
            candidate,
            RefinementOptions {
                chord_tolerance: 1e-8,
                max_refinement_levels: 0,
                max_triangles: 64,
            },
            &[],
            &[],
            |uv| Ok([uv[0], uv[1], 0.0]),
            |_, _, _, _| Err("unexpected_boundary_midpoint_evaluation".into()),
            |_, points| positive_z_alignment(points),
        )
        .unwrap();

        assert!(refined.converged);
        assert!(refined.chord_converged);
        assert_eq!(refined.orientation_converged, Some(true));
        assert_eq!(refined.orientation_check_count, 2);
        assert_eq!(refined.nonpositive_normal_count, 0);
        assert_eq!(refined.minimum_normal_alignment, Some(1.0));
    }

    #[test]
    fn oriented_refinement_rejects_reversed_winding_against_positive_z_face() {
        let candidate = triangulate_simple_loop(&[[0., 0.], [0., 1.], [1., 1.], [1., 0.]]).unwrap();
        let refined = refine_uniform_with_boundary_and_orientation(
            candidate,
            RefinementOptions {
                chord_tolerance: 1e-8,
                max_refinement_levels: 0,
                max_triangles: 64,
            },
            &[],
            &[],
            |uv| Ok([uv[0], uv[1], 0.0]),
            |_, _, _, _| Err("unexpected_boundary_midpoint_evaluation".into()),
            |_, points| positive_z_alignment(points),
        )
        .unwrap();

        assert!(refined.chord_converged);
        assert_eq!(refined.orientation_converged, Some(false));
        assert_eq!(refined.orientation_check_count, 2);
        assert_eq!(refined.nonpositive_normal_count, 2);
        assert_eq!(refined.minimum_normal_alignment, Some(-1.0));
        assert!(!refined.converged);
    }

    #[test]
    fn oriented_refinement_does_not_accept_bad_normals_when_chord_error_passes() {
        let candidate = triangulate_simple_loop(&[[0., 0.], [1., 0.], [1., 1.], [0., 1.]]).unwrap();
        let refined = refine_uniform_with_boundary_and_orientation(
            candidate,
            RefinementOptions {
                chord_tolerance: 1e-8,
                max_refinement_levels: 0,
                max_triangles: 64,
            },
            &[],
            &[],
            |uv| Ok([uv[0], uv[1], 0.0]),
            |_, _, _, _| Err("unexpected_boundary_midpoint_evaluation".into()),
            |_, _| Ok(-1.0),
        )
        .unwrap();

        assert!(!refined.converged);
        assert!(refined.chord_converged);
        assert_eq!(refined.orientation_converged, Some(false));
        assert_eq!(refined.orientation_check_count, 2);
        assert_eq!(refined.nonpositive_normal_count, 2);
        assert_eq!(refined.minimum_normal_alignment, Some(-1.0));
        assert_eq!(
            refined.failure_reason,
            Some("trim_candidate_normal_orientation_not_reached")
        );
    }

    #[test]
    fn oriented_refinement_rejects_nonfinite_alignment_evidence() {
        let candidate = triangulate_simple_loop(&[[0., 0.], [1., 0.], [1., 1.], [0., 1.]]).unwrap();
        let error = refine_uniform_with_boundary_and_orientation(
            candidate,
            RefinementOptions {
                chord_tolerance: 1e-8,
                max_refinement_levels: 0,
                max_triangles: 64,
            },
            &[],
            &[],
            |uv| Ok([uv[0], uv[1], 0.0]),
            |_, _, _, _| Err("unexpected_boundary_midpoint_evaluation".into()),
            |_, _| Ok(f64::NAN),
        )
        .unwrap_err();

        assert_eq!(error, "trim_candidate_triangle_normal_alignment_invalid");
    }

    #[test]
    fn sampled_refinement_uses_source_curve_for_declared_boundary_midpoints() {
        let candidate = triangulate_simple_loop(&[[0., 0.], [1., 1.], [1., 0.]]).unwrap();
        let boundary = [BoundaryCurveSegment {
            start_vertex: 0,
            end_vertex: 1,
            curve_index: 7,
            start_parameter: 0.,
            end_parameter: 1.,
        }];
        let refined = refine_uniform_with_boundary(
            candidate,
            RefinementOptions {
                chord_tolerance: 1e-4,
                max_refinement_levels: 1,
                max_triangles: 64,
            },
            &boundary,
            &[
                BoundaryVertexPoint {
                    vertex_index: 0,
                    point: [0., 0., 0.1],
                },
                BoundaryVertexPoint {
                    vertex_index: 1,
                    point: [1., 1., 0.2],
                },
            ],
            |uv| Ok([uv[0], uv[1], uv[0] * uv[0]]),
            |segment, _, _, _| {
                assert_eq!(segment.curve_index, 7);
                let parameter = (segment.start_parameter + segment.end_parameter) * 0.5;
                let uv = [parameter, parameter * parameter];
                Ok((uv, [uv[0], uv[1], uv[0] * uv[0]]))
            },
        )
        .unwrap();
        assert!(refined
            .vertices
            .iter()
            .any(|uv| { (uv[0] - 0.5).abs() < 1e-12 && (uv[1] - 0.25).abs() < 1e-12 }));
        assert!(!refined
            .vertices
            .iter()
            .any(|uv| (uv[0] - 0.5).abs() < 1e-12 && (uv[1] - 0.5).abs() < 1e-12));
        assert_eq!(refined.points[0], [0., 0., 0.1]);
        assert_eq!(refined.points[1], [1., 1., 0.2]);
    }

    #[test]
    fn sampled_refinement_reports_triangle_budget_without_claiming_convergence() {
        let candidate = triangulate_simple_loop(&[[0., 0.], [1., 0.], [1., 1.], [0., 1.]]).unwrap();
        let refined = refine_uniform(candidate, 1e-9, 2, 2, |uv| {
            Ok([uv[0], uv[1], uv[0] * uv[0]])
        })
        .unwrap();
        assert!(!refined.converged);
        assert_eq!(
            refined.failure_reason,
            Some("trim_candidate_triangle_budget_exceeded")
        );
    }
}
