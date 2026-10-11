//! Bounded triangulation of a convex sampled substrate with declared circular or
//! capsule openings. These statistics describe input polygons, not CAD/physical
//! qualification. No dimensions or board-specific coordinates are supplied here.
use earcut::Earcut;
use std::collections::{BTreeMap, BTreeSet};

const MAX_OUTER: usize = 1024;
const MAX_OPENINGS: usize = 512;
const MAX_POINTS: usize = 65_536;
const MAX_CIRCLE_SEGMENTS: usize = 4096;

#[derive(Debug)]
pub struct Mesh2d {
    pub points: Vec<[f64; 2]>,
    pub indices: Vec<u32>,
    /// First ring counterclockwise; opening rings clockwise; no repeated close.
    pub rings: Vec<Vec<[f64; 2]>>,
    pub polygonized_area_mm2: f64,
    pub analytic_cutout_area_mm2: f64,
    pub cutout_polygon_error_mm2: f64,
    pub triangulated_area_mm2: f64,
    pub maximum_cutout_sagitta_mm: f64,
    pub coordinate_quantization_max_mm: f64,
    pub source_polygonized_material_area_mm2: f64,
    pub quantized_polygon_area_delta_mm2: f64,
    pub precision: &'static str,
}

fn finite(p: [f64; 2]) -> bool {
    p.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.)
}
fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn area(ring: &[[f64; 2]]) -> f64 {
    let origin = ring[0];
    ring.iter()
        .zip(ring.iter().cycle().skip(1))
        .take(ring.len())
        .map(|(&a, &b)| cross(sub(a, origin), sub(b, origin)) * 0.5)
        .sum()
}

fn point_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let ab = sub(b, a);
    let length2 = dot(ab, ab);
    let t = if length2 == 0. {
        0.
    } else {
        (dot(sub(p, a), ab) / length2).clamp(0., 1.)
    };
    distance(p, [a[0] + t * ab[0], a[1] + t * ab[1]])
}
fn segment_distance(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> f64 {
    let ab = sub(b, a);
    let cd = sub(d, c);
    let denominator = cross(ab, cd);
    if denominator != 0. {
        let t = cross(sub(c, a), cd) / denominator;
        let u = cross(sub(c, a), ab) / denominator;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
            return 0.;
        }
    }
    point_segment(a, c, d)
        .min(point_segment(b, c, d))
        .min(point_segment(c, a, b))
        .min(point_segment(d, a, b))
}

fn normalize_outer(outer: &[[f64; 2]]) -> Result<Vec<[f64; 2]>, String> {
    if outer.len() < 3 || outer.len() > MAX_OUTER || outer.iter().any(|&p| !finite(p)) {
        return Err("substrate_invalid_outer_input".into());
    }
    let mut ring = outer.to_vec();
    ring.dedup();
    if ring.first() == ring.last() {
        ring.pop();
    }
    if ring.len() < 3 {
        return Err("substrate_degenerate_outer".into());
    }
    for i in 0..ring.len() {
        if ring[..i].contains(&ring[i]) {
            return Err("substrate_repeated_nonconsecutive_outer_vertex".into());
        }
    }
    let signed = area(&ring);
    if !signed.is_finite() || signed == 0. {
        return Err("substrate_degenerate_outer".into());
    }
    if signed < 0. {
        ring.reverse();
    }
    // Supporting-line validation also rejects self-intersecting loops which
    // local turn tests alone can accidentally accept (for example a pentagram).
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        let ab = sub(b, a);
        if distance(a, b) == 0. {
            return Err("substrate_zero_length_outer_edge".into());
        }
        for &p in &ring {
            let turn = cross(ab, sub(p, a));
            let roundoff = f64::EPSILON
                * 64.
                * (ab[0].abs() * (p[1] - a[1]).abs() + ab[1].abs() * (p[0] - a[0]).abs()).max(1.);
            if turn < -roundoff {
                return Err("substrate_outer_not_convex".into());
            }
        }
    }
    Ok(ring)
}

fn opening_ring(
    a: [f64; 2],
    b: [f64; 2],
    radius: f64,
    sagitta: f64,
) -> Result<(Vec<[f64; 2]>, f64), String> {
    // Bound the approximation rather than equating viewport resolution with
    // geometry accuracy. Cancellation at extremely small s/r fails closed.
    let half_angle = (1. - (sagitta / radius).min(1.)).acos();
    if !half_angle.is_finite() || half_angle <= 0. {
        return Err("substrate_sagitta_underflow".into());
    }
    let mut ring = Vec::new();
    let maximum_sagitta = if a == b {
        let steps = ((std::f64::consts::PI / half_angle).ceil() as usize).max(8);
        if steps > MAX_CIRCLE_SEGMENTS {
            return Err("substrate_circle_segment_limit".into());
        }
        for i in 0..steps {
            let angle = 2. * std::f64::consts::PI * i as f64 / steps as f64;
            ring.push([a[0] + radius * angle.cos(), a[1] + radius * angle.sin()]);
        }
        radius * (1. - (std::f64::consts::PI / steps as f64).cos())
    } else {
        let steps = ((std::f64::consts::PI / (2. * half_angle)).ceil() as usize).max(4);
        if steps * 2 > MAX_CIRCLE_SEGMENTS {
            return Err("substrate_slot_segment_limit".into());
        }
        let direction = (b[1] - a[1]).atan2(b[0] - a[0]);
        for (center, start) in [
            (b, direction - std::f64::consts::FRAC_PI_2),
            (a, direction + std::f64::consts::FRAC_PI_2),
        ] {
            for i in 0..=steps {
                let angle = start + std::f64::consts::PI * i as f64 / steps as f64;
                ring.push([
                    center[0] + radius * angle.cos(),
                    center[1] + radius * angle.sin(),
                ]);
            }
        }
        radius * (1. - (std::f64::consts::PI / (2. * steps as f64)).cos())
    };
    if ring.iter().any(|&p| !finite(p)) || area(&ring) <= 0. {
        return Err("substrate_invalid_opening_polygon".into());
    }
    ring.reverse();
    Ok((ring, maximum_sagitta))
}

fn inside_convex(point: [f64; 2], ring: &[[f64; 2]]) -> bool {
    ring.iter()
        .zip(ring.iter().cycle().skip(1))
        .take(ring.len())
        .all(|(&a, &b)| cross(sub(b, a), sub(point, a)) >= -1e-10)
}
fn inside_ring(point: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let mut inside = false;
    for (&a, &b) in ring
        .iter()
        .zip(ring.iter().cycle().skip(1))
        .take(ring.len())
    {
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}

fn edge(a: u32, b: u32) -> (u32, u32) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Make Earcut's collinear hole bridges conforming. Earcut may retain a long
/// bridge on one side and a chain of shorter bridges on the other. Reusing the
/// existing intermediate vertices closes that topological T-junction without
/// adding, moving, or welding any source ring position.
fn conform_edges(
    points: &[[f64; 2]],
    rings: &[Vec<[f64; 2]>],
    indices: &mut Vec<u32>,
) -> Result<(), String> {
    // Earcut can also emit arithmetic slivers along an otherwise collinear
    // bridge. The 3D exporter drops their zero-area scaled cross products;
    // remove only determinant values within the local arithmetic error bound
    // here, then conform the resulting bridge chain before extrusion.
    let mut nondegenerate = Vec::with_capacity(indices.len());
    for triangle in indices.chunks_exact(3) {
        if triangle.iter().any(|&i| i as usize >= points.len()) {
            return Err("substrate_triangle_index_out_of_range".into());
        }
        let a = points[triangle[0] as usize];
        let ab = sub(points[triangle[1] as usize], a);
        let ac = sub(points[triangle[2] as usize], a);
        let arithmetic_bound =
            16. * f64::EPSILON * (ab[0].abs() * ac[1].abs() + ab[1].abs() * ac[0].abs());
        if cross(ab, ac).abs() > arithmetic_bound {
            nondegenerate.extend_from_slice(triangle);
        }
    }
    *indices = nondegenerate;
    let mut boundary = BTreeSet::new();
    let mut offset = 0usize;
    for ring in rings {
        for i in 0..ring.len() {
            boundary.insert(edge(
                (offset + i) as u32,
                (offset + (i + 1) % ring.len()) as u32,
            ));
        }
        offset += ring.len();
    }
    let mut work = 0usize;
    for _ in 0..8 {
        let mut counts = BTreeMap::<(u32, u32), usize>::new();
        for tri in indices.chunks_exact(3) {
            if tri.iter().any(|&i| i as usize >= points.len()) {
                return Err("substrate_triangle_index_out_of_range".into());
            }
            for i in 0..3 {
                *counts.entry(edge(tri[i], tri[(i + 1) % 3])).or_default() += 1;
            }
        }
        if counts.values().any(|&n| n > 2) {
            return Err("substrate_nonmanifold_triangle_edge".into());
        }
        let orphaned: Vec<_> = counts
            .iter()
            .filter_map(|(&e, &n)| (n == 1 && !boundary.contains(&e)).then_some(e))
            .collect();
        if orphaned.is_empty() {
            if boundary.iter().any(|e| counts.get(e) != Some(&1)) {
                return Err("substrate_ring_boundary_not_preserved".into());
            }
            return Ok(());
        }
        let mut splits = BTreeMap::<(u32, u32), Vec<u32>>::new();
        for &(a, b) in &orphaned {
            let origin = points[a as usize];
            let end = points[b as usize];
            let ab = sub(end, origin);
            let length2 = dot(ab, ab);
            if length2 == 0. {
                return Err("substrate_zero_length_triangle_edge".into());
            }
            let mut interior = Vec::<(f64, u32)>::new();
            for (i, &p) in points.iter().enumerate() {
                work += 1;
                if work > 8_000_000 {
                    return Err("substrate_conforming_edge_work_limit".into());
                }
                if i == a as usize || i == b as usize {
                    continue;
                }
                if p[0] < origin[0].min(end[0])
                    || p[0] > origin[0].max(end[0])
                    || p[1] < origin[1].min(end[1])
                    || p[1] > origin[1].max(end[1])
                {
                    continue;
                }
                let ap = sub(p, origin);
                let t = dot(ap, ab) / length2;
                if t <= 0. || t >= 1. {
                    continue;
                }
                // Only arithmetic roundoff is allowed: this bound follows the
                // local determinant's two products, not a CAD distance tolerance.
                // Coordinates remain exactly the original f64 ring vertices.
                let det = cross(ab, ap);
                let roundoff =
                    16. * f64::EPSILON * (ab[0].abs() * ap[1].abs() + ab[1].abs() * ap[0].abs());
                if det.abs() <= roundoff {
                    interior.push((t, i as u32));
                }
            }
            if !interior.is_empty() {
                interior.sort_by(|a, b| a.0.total_cmp(&b.0));
                splits.insert((a, b), interior.into_iter().map(|(_, i)| i).collect());
            }
        }
        if splits.is_empty() {
            return Err("substrate_unresolved_nonconforming_edge".into());
        }
        let mut next = Vec::with_capacity(indices.len());
        for tri in indices.chunks_exact(3) {
            let mut split = None;
            for side in 0..3 {
                let a = tri[side];
                let b = tri[(side + 1) % 3];
                if let Some(interior) = splits.get(&edge(a, b)) {
                    split = Some((side, interior));
                    break;
                }
            }
            if let Some((side, interior)) = split {
                let a = tri[side];
                let b = tri[(side + 1) % 3];
                let opposite = tri[(side + 2) % 3];
                let mut chain = interior.clone();
                if a > b {
                    chain.reverse();
                }
                let mut previous = a;
                for current in chain.into_iter().chain(std::iter::once(b)) {
                    next.extend([previous, current, opposite]);
                    previous = current;
                }
            } else {
                next.extend_from_slice(tri);
            }
            if next.len() > 6 * MAX_POINTS {
                return Err("substrate_conforming_triangle_budget".into());
            }
        }
        *indices = next;
    }
    Err("substrate_conforming_iteration_limit".into())
}

/// Input XY and radii are in millimetres. Circle centres use identical a/b;
/// slots describe the centreline capsule endpoints. No source frame is guessed.
pub fn triangulate(
    outer: &[[f64; 2]],
    openings: &[([f64; 2], [f64; 2], f64)],
    sagitta_mm: f64,
) -> Result<Mesh2d, String> {
    if !sagitta_mm.is_finite() || sagitta_mm <= 0. || sagitta_mm > 0.001 {
        return Err("substrate_invalid_sagitta".into());
    }
    if openings.len() > MAX_OPENINGS {
        return Err("substrate_opening_count_limit".into());
    }
    let outer = normalize_outer(outer)?;
    let mut minimum_source_clearance = f64::INFINITY;
    for (i, &(a, b, radius)) in openings.iter().enumerate() {
        if !finite(a) || !finite(b) || !radius.is_finite() || radius <= 0. || radius > 1_000_000. {
            return Err("substrate_invalid_opening".into());
        }
        // A capsule is the Minkowski sum of its segment and a disk. Requiring
        // both endpoints to clear every convex supporting line by its radius
        // therefore proves the full capsule stays strictly inside the polygon.
        for (&p, &q) in outer
            .iter()
            .zip(outer.iter().cycle().skip(1))
            .take(outer.len())
        {
            let edge = sub(q, p);
            let length = distance(p, q);
            if cross(edge, sub(a, p)) / length <= radius
                || cross(edge, sub(b, p)) / length <= radius
            {
                return Err("substrate_opening_touches_or_crosses_outer".into());
            }
            minimum_source_clearance = minimum_source_clearance
                .min(cross(edge, sub(a, p)) / length - radius)
                .min(cross(edge, sub(b, p)) / length - radius);
        }
        for &(c, d, other_radius) in &openings[..i] {
            if segment_distance(a, b, c, d) <= radius + other_radius {
                return Err("substrate_openings_touch_or_overlap".into());
            }
            minimum_source_clearance =
                minimum_source_clearance.min(segment_distance(a, b, c, d) - radius - other_radius);
        }
    }
    let mut rings = vec![outer];
    let mut analytic_cutout_area_mm2 = 0.;
    let mut maximum_cutout_sagitta_mm: f64 = 0.;
    let mut point_count = rings[0].len();
    for &(a, b, radius) in openings {
        let (ring, sagitta) = opening_ring(a, b, radius, sagitta_mm)?;
        point_count += ring.len();
        if point_count > MAX_POINTS {
            return Err("substrate_point_budget".into());
        }
        maximum_cutout_sagitta_mm = maximum_cutout_sagitta_mm.max(sagitta);
        analytic_cutout_area_mm2 +=
            std::f64::consts::PI * radius * radius + 2. * radius * distance(a, b);
        rings.push(ring);
    }
    let polygonized_cutout_area: f64 = rings.iter().skip(1).map(|r| area(r).abs()).sum();
    let source_polygonized_material_area_mm2 = area(&rings[0]) - polygonized_cutout_area;
    let mut coordinate_quantization_max_mm: f64 = 0.;
    for ring in &mut rings {
        for point in ring.iter_mut() {
            let original = *point;
            *point = point.map(|value| ((value * 0.001) as f32 as f64) * 1000.);
            if !finite(*point) {
                return Err("substrate_delivery_quantization_nonfinite".into());
            }
            coordinate_quantization_max_mm =
                coordinate_quantization_max_mm.max(distance(original, *point));
        }
        let mut sorted = ring.clone();
        sorted.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
        if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("substrate_delivery_quantization_collapsed_ring".into());
        }
        // Convexity validates self-intersection and zero-length edges without
        // altering quantized vertex ordering or removing collinear samples.
        normalize_outer(ring)?;
    }
    // Every segment moves by at most the maximum endpoint displacement. This
    // conservative bound proves quantization cannot close an accepted source
    // gap between distinct rings or between an opening and the outer ring.
    if 2. * coordinate_quantization_max_mm >= minimum_source_clearance {
        return Err("substrate_delivery_quantization_may_close_clearance".into());
    }
    let polygonized_area_mm2 =
        area(&rings[0]) - rings.iter().skip(1).map(|r| area(r).abs()).sum::<f64>();
    let quantized_polygon_area_delta_mm2 =
        polygonized_area_mm2 - source_polygonized_material_area_mm2;
    if !polygonized_area_mm2.is_finite() || polygonized_area_mm2 <= 0. {
        return Err("substrate_nonpositive_material_area".into());
    }
    let mut points = Vec::with_capacity(point_count);
    let mut holes = Vec::<u32>::new();
    for (i, ring) in rings.iter().enumerate() {
        if i > 0 {
            holes.push(points.len() as u32);
        }
        points.extend_from_slice(ring);
    }
    let mut indices = Vec::<u32>::new();
    Earcut::<f64>::new().earcut(points.iter().copied(), &holes, &mut indices);
    if indices.is_empty() || !indices.len().is_multiple_of(3) || indices.len() > 6 * MAX_POINTS {
        return Err("substrate_invalid_triangulation_count".into());
    }
    conform_edges(&points, &rings, &mut indices)?;
    let mut triangulated_area_mm2 = 0.;
    for triangle in indices.chunks_exact_mut(3) {
        if triangle.iter().any(|&i| i as usize >= points.len()) {
            return Err("substrate_triangle_index_out_of_range".into());
        }
        let [a, b, c] = [
            points[triangle[0] as usize],
            points[triangle[1] as usize],
            points[triangle[2] as usize],
        ];
        let signed_twice = cross(sub(b, a), sub(c, a));
        if !signed_twice.is_finite() || signed_twice == 0. {
            return Err("substrate_degenerate_triangle".into());
        }
        if signed_twice < 0. {
            triangle.swap(1, 2);
        }
        triangulated_area_mm2 += signed_twice.abs() * 0.5;
        let center = [(a[0] + b[0] + c[0]) / 3., (a[1] + b[1] + c[1]) / 3.];
        if !inside_convex(center, &rings[0]) {
            return Err("substrate_triangle_outside_outer".into());
        }
        for (&(p, q, radius), ring) in openings.iter().zip(rings.iter().skip(1)) {
            // Only the capsule bounding box can intersect its polygon.
            if center[0] >= p[0].min(q[0]) - radius
                && center[0] <= p[0].max(q[0]) + radius
                && center[1] >= p[1].min(q[1]) - radius
                && center[1] <= p[1].max(q[1]) + radius
                && inside_ring(center, ring)
            {
                return Err("substrate_triangle_inside_opening".into());
            }
        }
    }
    // Numerical equality to the *polygonized* area, distinct from the declared
    // arc approximation error. This does not establish surface/physical fidelity.
    let numerical_area_tolerance = 1e-9 * polygonized_area_mm2.max(1.);
    if (triangulated_area_mm2 - polygonized_area_mm2).abs() > numerical_area_tolerance {
        return Err("substrate_triangulation_area_mismatch".into());
    }
    Ok(Mesh2d {
        points,
        indices,
        rings,
        polygonized_area_mm2,
        analytic_cutout_area_mm2,
        cutout_polygon_error_mm2: analytic_cutout_area_mm2 - polygonized_cutout_area,
        triangulated_area_mm2,
        maximum_cutout_sagitta_mm,
        coordinate_quantization_max_mm,
        source_polygonized_material_area_mm2,
        quantized_polygon_area_delta_mm2,
        precision: "GLB_FLOAT32_METRES",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn square() -> Vec<[f64; 2]> {
        vec![[0., 0.], [10., 0.], [10., 10.], [0., 10.]]
    }
    #[test]
    fn collinear_bridge_chain_is_split_without_moving_positions() {
        let ring = vec![[0., 0.], [3., 0.], [3., 3.], [0., 3.]];
        let mut points = ring.clone();
        points.push([2., 2.]);
        let before = points.clone();
        let mut indices = vec![0, 2, 3, 0, 1, 4, 4, 1, 2];
        conform_edges(&points, &[ring], &mut indices).unwrap();
        assert_eq!(points, before);
        assert_eq!(indices.len(), 12);
        let mut counts = BTreeMap::new();
        for tri in indices.chunks_exact(3) {
            for i in 0..3 {
                *counts.entry(edge(tri[i], tri[(i + 1) % 3])).or_insert(0) += 1;
            }
        }
        assert_eq!(counts[&edge(0, 4)], 2);
        assert_eq!(counts[&edge(2, 4)], 2);
        assert!(!counts.contains_key(&edge(0, 2)));
    }
    #[test]
    fn arithmetic_sliver_is_replaced_by_conforming_chain() {
        let ring = vec![[0., 0.], [3., 0.], [3., 3.], [0., 3.]];
        let mut points = ring.clone();
        points.push([2., 2. + f64::EPSILON * 2.]);
        let before = points.clone();
        // Every indexed edge initially pairs, hiding an arithmetic-area sliver
        // which would disappear during the metre-scaled extrusion.
        let mut indices = vec![0, 2, 3, 0, 1, 4, 4, 1, 2, 0, 4, 2];
        conform_edges(&points, &[ring], &mut indices).unwrap();
        assert_eq!(points, before);
        let mut counts = BTreeMap::new();
        for tri in indices.chunks_exact(3) {
            for i in 0..3 {
                *counts.entry(edge(tri[i], tri[(i + 1) % 3])).or_insert(0) += 1;
            }
        }
        assert!(!counts.contains_key(&edge(0, 2)));
        assert_eq!(counts[&edge(0, 4)], 2);
        assert_eq!(counts[&edge(2, 4)], 2);
    }
    #[test]
    fn circle_has_real_opening_and_declared_error() {
        let mesh = triangulate(&square(), &[([5., 5.], [5., 5.], 1.)], 0.001).unwrap();
        assert_eq!(mesh.rings.len(), 2);
        assert!(area(&mesh.rings[0]) > 0. && area(&mesh.rings[1]) < 0.);
        assert!((mesh.analytic_cutout_area_mm2 - std::f64::consts::PI).abs() < 1e-12);
        assert!(
            (mesh.source_polygonized_material_area_mm2
                - (100. - std::f64::consts::PI)
                - mesh.cutout_polygon_error_mm2)
                .abs()
                < 1e-10
        );
        assert!(mesh.maximum_cutout_sagitta_mm <= 0.001 + 1e-12);
        assert!((mesh.triangulated_area_mm2 - mesh.polygonized_area_mm2).abs() < 1e-10);
    }
    #[test]
    fn capsule_area_includes_rectangle_and_two_semicircles() {
        let mesh = triangulate(&square(), &[([3., 5.], [7., 5.], 1.)], 0.001).unwrap();
        assert!((mesh.analytic_cutout_area_mm2 - (8. + std::f64::consts::PI)).abs() < 1e-12);
        assert!(
            (mesh.source_polygonized_material_area_mm2
                - (92. - std::f64::consts::PI)
                - mesh.cutout_polygon_error_mm2)
                .abs()
                < 1e-10
        );
    }
    #[test]
    fn reversed_and_duplicate_closed_outer_normalizes() {
        let mut ring = square();
        ring.reverse();
        ring.push(ring[0]);
        ring.insert(1, ring[0]);
        let mesh = triangulate(&ring, &[], 0.001).unwrap();
        assert_eq!(mesh.rings[0].len(), 4);
        assert!((mesh.source_polygonized_material_area_mm2 - 100.).abs() < 1e-12);
        assert_eq!(mesh.precision, "GLB_FLOAT32_METRES");
        assert!(mesh.coordinate_quantization_max_mm > 0.);
        for point in &mesh.points {
            for &coordinate in point {
                assert_eq!(((coordinate * 0.001) as f32 as f64) * 1000., coordinate);
            }
        }
    }
    #[test]
    fn circles_slots_touching_or_crossing_refuse() {
        for openings in [
            vec![([4., 5.], [4., 5.], 1.), ([6., 5.], [6., 5.], 1.)],
            vec![([3., 3.], [7., 7.], 0.2), ([3., 7.], [7., 3.], 0.2)],
            vec![([5., 5.], [5., 5.], 1.), ([5., 5.], [5., 5.], 0.5)],
        ] {
            assert!(triangulate(&square(), &openings, 0.001)
                .unwrap_err()
                .contains("touch_or_overlap"));
        }
    }
    #[test]
    fn offboard_and_tangent_openings_refuse() {
        for opening in [
            ([1., 5.], [1., 5.], 1.),
            ([-2., 5.], [-2., 5.], 1.),
            ([5., 5.], [10., 5.], 0.1),
        ] {
            assert!(triangulate(&square(), &[opening], 0.001)
                .unwrap_err()
                .contains("crosses_outer"));
        }
    }
    #[test]
    fn invalid_values_concavity_and_budgets_refuse() {
        for radius in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(triangulate(&square(), &[([5., 5.], [5., 5.], radius)], 0.001).is_err());
        }
        for sagitta in [0., -1., 0.002, f64::NAN] {
            assert!(triangulate(&square(), &[], sagitta).is_err());
        }
        assert!(triangulate(
            &[[0., 0.], [10., 0.], [5., 5.], [10., 10.], [0., 10.]],
            &[],
            0.001
        )
        .is_err());
        assert!(triangulate(&square(), &[([5., 5.], [5., 5.], 0.01); 513], 0.001).is_err());
        assert!(triangulate(&[[0., 0.], [f64::NAN, 0.], [0., 1.]], &[], 0.001).is_err());
    }
}
