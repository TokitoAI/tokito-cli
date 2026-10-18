//! Physical-error-driven, conforming subdivision of one simple sampled UV loop.
//! Support-surface experiment only: fixed trim segments are not qualified CAD edges.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};

type Uv = [f64; 2];
type P3 = [f64; 3];
type Edge = (usize, usize);

const MAX_ROUNDS: usize = 12;
const MAX_VERTICES: usize = 25_000;
const MAX_TRIANGLES: usize = 50_000;
const MAX_SAMPLE_EVALUATIONS: usize = 400_000;

fn edge(a: usize, b: usize) -> Edge {
    (a.min(b), a.max(b))
}
fn middle(a: Uv, b: Uv) -> Uv {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}
fn distance(a: P3, b: P3) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2])
}
fn linear_middle(a: P3, b: P3) -> P3 {
    std::array::from_fn(|i| (a[i] + b[i]) * 0.5)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ChartMetric {
    pub anchor: Uv,
    pub matrix: [[f64; 2]; 2],
}

impl ChartMetric {
    pub(crate) fn from_tangents(anchor: Uv, du: P3, dv: P3) -> Result<Self, String> {
        let norm = |p: P3| p[0].hypot(p[1]).hypot(p[2]);
        let a = norm(du);
        let d = norm(dv);
        if !a.is_finite()
            || !d.is_finite()
            || a <= 0.
            || d <= 0.
            || anchor.iter().any(|v| !v.is_finite())
        {
            return Err("physical_metric_singular_or_nonfinite_tangents".into());
        }
        let cosine = du
            .iter()
            .zip(dv)
            .map(|(u, v)| (u / a) * (v / d))
            .sum::<f64>();
        let sine_squared = 1. - cosine * cosine;
        if !sine_squared.is_finite() || sine_squared <= 1e-12 {
            return Err("physical_metric_singular_or_ill_conditioned_tangents".into());
        }
        // M^T M = J^T J, apart from one common scale that does not affect CDT.
        let scale = a.max(d);
        let matrix = [
            [a / scale, d / scale * cosine],
            [0., d / scale * sine_squared.sqrt()],
        ];
        if matrix[0][0] * matrix[1][1] <= 1e-12 {
            return Err("physical_metric_excessive_axis_anisotropy".into());
        }
        Ok(Self { anchor, matrix })
    }
    fn map(self, p: Uv) -> Uv {
        let d = [p[0] - self.anchor[0], p[1] - self.anchor[1]];
        self.matrix.map(|row| row[0] * d[0] + row[1] * d[1])
    }
}

fn initial_triangles(
    boundary: &[Uv],
    metric: Option<ChartMetric>,
) -> Result<Vec<[usize; 3]>, String> {
    if let Some(m) = metric {
        let determinant = m.matrix[0][0] * m.matrix[1][1] - m.matrix[0][1] * m.matrix[1][0];
        if m.anchor
            .iter()
            .chain(m.matrix.iter().flatten())
            .any(|v| !v.is_finite())
            || !determinant.is_finite()
            || determinant <= 1e-12
        {
            return Err("physical_metric_invalid_chart_transform".into());
        }
    }
    let chart = boundary
        .iter()
        .map(|p| metric.map_or(*p, |m| m.map(*p)))
        .collect::<Vec<_>>();
    if chart.iter().flatten().any(|x| !x.is_finite()) {
        return Err("physical_metric_nonfinite_chart_coordinate".into());
    }
    let points = chart.iter().map(|p| Point2::new(p[0], p[1])).collect();
    let constraints = (0..boundary.len())
        .map(|i| [i, (i + 1) % boundary.len()])
        .collect();
    let cdt = ConstrainedDelaunayTriangulation::<Point2<f64>>::bulk_load_cdt(points, constraints)
        .map_err(|e| format!("physical_refinement_initial_cdt_failed:{e}"))?;
    if cdt.num_vertices() != boundary.len() {
        return Err("physical_refinement_cdt_changed_boundary_inventory".into());
    }
    let index = chart
        .iter()
        .enumerate()
        .map(|(i, p)| ((p[0].to_bits(), p[1].to_bits()), i))
        .collect::<BTreeMap<_, _>>();
    // Determine the bounded domain by topology, never by a near-boundary
    // floating-point centroid predicate. Original loop is preflighted simple.
    let mut outside = BTreeSet::new();
    let mut queue = Vec::new();
    for e in cdt.directed_edges() {
        if e.face().is_outer() && !e.is_constraint_edge() {
            if let Some(face) = e.rev().face().as_inner() {
                queue.push(face.fix());
            }
        }
    }
    while let Some(handle) = queue.pop() {
        if !outside.insert(handle.index()) {
            continue;
        }
        for e in cdt.face(handle).adjacent_edges() {
            if !e.is_constraint_edge() {
                if let Some(face) = e.rev().face().as_inner() {
                    queue.push(face.fix());
                }
            }
        }
    }
    let mut triangles = Vec::new();
    for face in cdt.inner_faces() {
        if outside.contains(&face.index()) {
            continue;
        }
        let p = face.positions().map(|p| [p.x, p.y]);
        let ids = p.map(|p| index.get(&(p[0].to_bits(), p[1].to_bits())).copied());
        let [Some(a), Some(b), Some(c)] = ids else {
            return Err("physical_refinement_cdt_changed_coordinates".into());
        };
        let [p, q, r] = [boundary[a], boundary[b], boundary[c]];
        let signed_area = (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
        if !signed_area.is_finite() || signed_area <= 0. {
            return Err(
                "physical_metric_initial_original_uv_triangle_degenerate_or_reversed".into(),
            );
        }
        triangles.push([a, b, c]);
    }
    if triangles.len() != boundary.len() - 2 {
        return Err("physical_refinement_initial_domain_triangle_count_mismatch".into());
    }
    Ok(triangles)
}

// All subdivisions share exactly one new vertex per selected undirected edge.
// The cyclic templates preserve the parent's winding and do not create T-junctions.
fn split_triangle(t: [usize; 3], midpoints: &BTreeMap<Edge, usize>) -> Vec<[usize; 3]> {
    let [a, b, c] = t;
    let m = [
        midpoints.get(&edge(a, b)).copied(),
        midpoints.get(&edge(b, c)).copied(),
        midpoints.get(&edge(c, a)).copied(),
    ];
    match m {
        [None, None, None] => vec![t],
        [Some(x), Some(y), Some(z)] => vec![[a, x, z], [x, b, y], [z, y, c], [x, y, z]],
        _ => {
            let start = (0..3)
                .find(|i| m[*i].is_some() && m[(*i + 2) % 3].is_none())
                .unwrap();
            let [a, b, c] = [t[start], t[(start + 1) % 3], t[(start + 2) % 3]];
            let x = m[start].unwrap();
            if let Some(y) = m[(start + 1) % 3] {
                vec![[a, x, c], [x, y, c], [x, b, y]]
            } else {
                vec![[a, x, c], [x, b, c]]
            }
        }
    }
}

pub(crate) fn refine<F, O>(
    input: &[Uv],
    tolerance: f64,
    minimum_normal_alignment: f64,
    metric: Option<ChartMetric>,
    mut evaluate: F,
    mut orientation: O,
) -> Result<Value, String>
where
    F: FnMut(Uv) -> Result<P3, String>,
    O: FnMut([Uv; 3], [P3; 3]) -> Result<f64, String>,
{
    if !(tolerance.is_finite()
        && tolerance > 0.
        && minimum_normal_alignment.is_finite()
        && 0. < minimum_normal_alignment
        && minimum_normal_alignment <= 1.)
    {
        return Err("physical_refinement_invalid_tolerance".into());
    }
    let initial = crate::trim_polygon_candidate::triangulate_simple_loop(input)?;
    if input
        .iter()
        .zip(&initial.input_vertex_indices)
        .any(|(p, i)| *p != initial.vertices[*i])
    {
        return Err("physical_refinement_distinct_boundary_sample_would_be_removed".into());
    }
    let input_mapping = initial.input_vertex_indices;
    let removed_duplicates = initial.removed_adjacent_duplicates;
    let winding = 1.; // Spade returns CCW UV faces; no 3D normal/winding flips.
    let boundary_count = initial.vertices.len();
    let mut uv = initial.vertices;
    let mut triangles = initial_triangles(&uv, metric)?;
    let boundary = (0..boundary_count)
        .map(|i| edge(i, (i + 1) % boundary_count))
        .collect::<BTreeSet<_>>();
    let mut sample_count = 0;
    let mut sample = |p: Uv| -> Result<P3, String> {
        sample_count += 1;
        if sample_count > MAX_SAMPLE_EVALUATIONS {
            return Err("physical_refinement_sample_budget_exceeded".into());
        }
        let point = evaluate(p)?;
        if point.iter().any(|x| !x.is_finite()) {
            return Err("physical_refinement_nonfinite_surface_point".into());
        }
        Ok(point)
    };
    let mut points = uv
        .iter()
        .map(|p| sample(*p))
        .collect::<Result<Vec<_>, _>>()?;
    let mut history = Vec::new();
    let mut last_quality;
    let mut rounds = 0;
    let reason;
    loop {
        let mut split = BTreeSet::new();
        let mut edge_errors = BTreeMap::new();
        let mut edge_samples = BTreeMap::new();
        let mut max_interior_error: f64 = 0.;
        let mut max_boundary_error: f64 = 0.;
        let mut min_alignment: f64 = 1.;
        let mut nonpositive = 0;
        let mut normal_failures = 0;
        for &t in &triangles {
            let center_uv = std::array::from_fn(|i| (uv[t[0]][i] + uv[t[1]][i] + uv[t[2]][i]) / 3.);
            let center_point = sample(center_uv)?;
            let linear =
                std::array::from_fn(|i| (points[t[0]][i] + points[t[1]][i] + points[t[2]][i]) / 3.);
            let center_error = distance(center_point, linear);
            if !center_error.is_finite() {
                return Err("physical_refinement_nonfinite_chord_error".into());
            }
            max_interior_error = max_interior_error.max(center_error);
            let alignment = orientation(t.map(|i| uv[i]), t.map(|i| points[i]))?;
            if !alignment.is_finite() || !(-1.000001..=1.000001).contains(&alignment) {
                return Err("physical_refinement_invalid_normal_alignment".into());
            }
            let alignment = alignment.clamp(-1., 1.);
            min_alignment = min_alignment.min(alignment);
            nonpositive += usize::from(alignment <= 0.);
            normal_failures += usize::from(alignment < minimum_normal_alignment);
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let key = edge(a, b);
                let error = if let Some(error) = edge_errors.get(&key) {
                    *error
                } else {
                    let point = sample(middle(uv[a], uv[b]))?;
                    let error = distance(point, linear_middle(points[a], points[b]));
                    if !error.is_finite() {
                        return Err("physical_refinement_nonfinite_chord_error".into());
                    }
                    edge_samples.insert(key, point);
                    edge_errors.insert(key, error);
                    error
                };
                if boundary.contains(&key) {
                    max_boundary_error = max_boundary_error.max(error);
                } else {
                    max_interior_error = max_interior_error.max(error);
                    if error > tolerance
                        || center_error > tolerance
                        || alignment < minimum_normal_alignment
                    {
                        split.insert(key);
                    }
                }
            }
        }
        last_quality = json!({"maximumSampledInteriorChordErrorSourceUnits":max_interior_error,
            "maximumSampledFixedBoundaryChordErrorSourceUnits":max_boundary_error,
            "minimumSampledNormalAlignment":min_alignment,
            "normalThresholdFailureCount":normal_failures,
            "nonpositiveNormalCount":nonpositive});
        history.push(json!({"round":rounds,"vertices":uv.len(),"triangles":triangles.len(),"quality":last_quality}));
        if max_interior_error <= tolerance
            && max_boundary_error <= tolerance
            && normal_failures == 0
        {
            reason = "sampled_targets_met";
            break;
        }
        if split.is_empty() {
            reason = "fixed_boundary_or_unresolved_orientation";
            break;
        }
        if rounds >= MAX_ROUNDS {
            reason = "round_budget_exhausted";
            break;
        }
        let next_triangles = triangles
            .iter()
            .map(|t| {
                1 + [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]
                    .iter()
                    .filter(|(a, b)| split.contains(&edge(*a, *b)))
                    .count()
            })
            .sum::<usize>();
        if uv.len() + split.len() > MAX_VERTICES || next_triangles > MAX_TRIANGLES {
            reason = "mesh_budget_exhausted";
            break;
        }
        let mut midpoints = BTreeMap::new();
        for key in split {
            let p = middle(uv[key.0], uv[key.1]);
            if p == uv[key.0] || p == uv[key.1] {
                return Err("physical_refinement_midpoint_stagnated".into());
            }
            midpoints.insert(key, uv.len());
            uv.push(p);
            points.push(edge_samples[&key]);
        }
        triangles = triangles
            .into_iter()
            .flat_map(|t| split_triangle(t, &midpoints))
            .collect();
        for t in &triangles {
            let [a, b, c] = t.map(|i| uv[i]);
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if !area.is_finite() || area * winding <= 0. {
                return Err("physical_refinement_degenerate_or_reversed_uv_triangle".into());
            }
        }
        if triangles.len() != next_triangles {
            return Err("physical_refinement_triangle_accounting_mismatch".into());
        }
        rounds += 1;
    }
    // Account every incidence: retained original constraints once, all interior
    // edges twice with opposite direction. Fail rather than publish broken topology.
    let mut incidence: BTreeMap<Edge, (usize, isize)> = BTreeMap::new();
    for t in &triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let entry = incidence.entry(edge(a, b)).or_default();
            entry.0 += 1;
            entry.1 += if a < b { 1 } else { -1 };
        }
    }
    if boundary
        .iter()
        .any(|e| incidence.get(e).is_none_or(|v| v.0 != 1))
        || incidence.iter().any(|(e, (count, sense))| {
            if boundary.contains(e) {
                *count != 1
            } else {
                *count != 2 || *sense != 0
            }
        })
    {
        return Err("physical_refinement_nonconforming_topology".into());
    }
    Ok(json!({"schema":"tokito.physical-surface-refinement.v1",
        "status":if reason=="sampled_targets_met" {"sampled_targets_met_unqualified"} else {"incomplete"},
        "termination":reason,"algorithm":"constrained_delaunay_initialization_then_physical_chord_and_signed_normal_driven_conforming_nonboundary_edge_subdivision",
        "initialChartMetric":metric.map(|m|json!({"anchorUv":m.anchor,"normalizedCholeskyMatrix":m.matrix,
            "scope":"single_anchor_affine_preconditioning_only_not_global_metric_or_accuracy_bound",
            "originalUvCoordinatesRetainedByIdentity":true})),
        "chordToleranceSourceUnits":tolerance,"minimumNormalAlignmentTarget":minimum_normal_alignment,
        "boundaryVertexCount":boundary_count,"additionalBoundaryVertexCount":0,
        "originalInputSampleCount":input.len(),"removedExactDuplicateCount":removed_duplicates,
        "inputSampleVertexIndices":input_mapping,
        "originalConstraintSegmentsPreserved":true,"nonconformingEdgeCount":0,
        "rounds":rounds,"roundLimit":MAX_ROUNDS,"vertexLimit":MAX_VERTICES,"triangleLimit":MAX_TRIANGLES,
        "sampleEvaluationCount":sample_count,"sampleEvaluationLimit":MAX_SAMPLE_EVALUATIONS,
        "vertexCount":uv.len(),"triangleCount":triangles.len(),"quality":last_quality,"history":history,
        "uvVertices":uv,"surfacePointsSourceUnits":points,"triangles":triangles,
        "sourceCurveBoundaryGeometryUsedForMeshPoints":false,
        "certifiedSurfaceDeviationBound":false,"completeSolidVerified":false,
        "geometryQualified":false,"admitted":false,"exportsProduced":[]}))
}

#[cfg(test)]
mod tests {
    use super::*;
    const LOOP: [Uv; 4] = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
    #[test]
    fn chart_metric_reproduces_tangent_lengths_up_to_common_scale() {
        let m = ChartMetric::from_tangents([3., 4.], [1., 0., 0.], [2., 3., 0.]).unwrap();
        for d in [[1., 0.], [0., 1.], [2., -3.]] {
            let q = m.map([3. + d[0], 4. + d[1]]);
            let physical = [d[0] + 2. * d[1], 3. * d[1]];
            let observed = (q[0] * q[0] + q[1] * q[1]) * 13.;
            let expected = physical[0] * physical[0] + physical[1] * physical[1];
            assert!((observed - expected).abs() < 1e-10);
        }
        assert!(ChartMetric::from_tangents([0., 0.], [0.; 3], [1., 0., 0.]).is_err());
        assert!(ChartMetric::from_tangents([0., 0.], [1., 0., 0.], [2., 0., 0.]).is_err());
        assert!(ChartMetric::from_tangents([f64::NAN, 0.], [1., 0., 0.], [0., 1., 0.]).is_err());
        let invalid = ChartMetric {
            anchor: [0., 0.],
            matrix: [[1., 0.], [0., -1.]],
        };
        assert!(initial_triangles(&LOOP, Some(invalid)).is_err());
    }

    #[test]
    fn chart_preconditioned_cdt_keeps_original_coordinates_and_constraints() {
        let m = ChartMetric::from_tangents([0.5, 0.5], [0.03, 0., 0.], [0., 1., 0.]).unwrap();
        let r = refine(
            &LOOP,
            1e-6,
            0.99,
            Some(m),
            |p| Ok([p[0] * 0.03, p[1], 0.]),
            |_, _| Ok(1.),
        )
        .unwrap();
        assert_eq!(r["termination"], "sampled_targets_met");
        assert_eq!(r["uvVertices"], json!(LOOP));
        assert_eq!(r["originalConstraintSegmentsPreserved"], true);
        assert_eq!(
            r["initialChartMetric"]["originalUvCoordinatesRetainedByIdentity"],
            true
        );
    }
    #[test]
    fn plane_preserves_constraints_without_refinement() {
        let r = refine(
            &LOOP,
            1e-6,
            0.99,
            None,
            |p| Ok([p[0], p[1], 0.]),
            |_, _| Ok(1.),
        )
        .unwrap();
        assert_eq!(r["termination"], "sampled_targets_met");
        assert_eq!(r["rounds"], 0);
        assert_eq!(r["triangleCount"], 2);
        assert_eq!(r["additionalBoundaryVertexCount"], 0);
        assert_eq!(r["admitted"], false);
    }
    #[test]
    fn concave_domain_and_uv_rescaling_keep_the_physical_plane() {
        let shape = [[0., 0.], [2., 0.], [2., 1.], [1., 1.], [1., 2.], [0., 2.]];
        for factor in [1., 1000.] {
            let uv = shape.map(|p| [p[0] * factor, p[1] / factor]);
            let r = refine(
                &uv,
                1e-6,
                0.99,
                None,
                |p| Ok([p[0] / factor, p[1] * factor, 0.]),
                |_, _| Ok(1.),
            )
            .unwrap();
            assert_eq!(r["termination"], "sampled_targets_met");
            assert_eq!(r["triangleCount"], 4);
            assert_eq!(r["originalConstraintSegmentsPreserved"], true);
            let metric = ChartMetric::from_tangents(
                [factor, 1. / factor],
                [1. / factor, 0., 0.],
                [0., factor, 0.],
            )
            .unwrap();
            let r = refine(
                &uv,
                1e-6,
                0.99,
                Some(metric),
                |p| Ok([p[0] / factor, p[1] * factor, 0.]),
                |_, _| Ok(1.),
            )
            .unwrap();
            assert_eq!(r["termination"], "sampled_targets_met");
            assert_eq!(r["originalConstraintSegmentsPreserved"], true);
        }
    }
    #[test]
    fn curved_interior_refines_and_reports_fixed_boundary_obstruction() {
        let r = refine(
            &LOOP,
            0.01,
            0.5,
            None,
            |p| Ok([p[0], p[1], 0.2 * p[0] * p[0]]),
            |_, _| Ok(1.),
        )
        .unwrap();
        assert!(r["rounds"].as_u64().unwrap() > 0);
        assert_eq!(r["status"], "incomplete");
        assert!(
            r["quality"]["maximumSampledFixedBoundaryChordErrorSourceUnits"]
                .as_f64()
                .unwrap()
                > 0.01
        );
        assert_eq!(r["originalConstraintSegmentsPreserved"], true);
    }
    #[test]
    fn all_edge_split_templates_preserve_area_and_winding() {
        let coords = [[0., 0.], [2., 0.], [0., 2.], [1., 0.], [1., 1.], [0., 1.]];
        for mask in 0..8 {
            let map = [(edge(0, 1), 3), (edge(1, 2), 4), (edge(2, 0), 5)]
                .into_iter()
                .enumerate()
                .filter_map(|(i, p)| (mask & (1 << i) != 0).then_some(p))
                .collect();
            let triangles = split_triangle([0, 1, 2], &map);
            let area: f64 = triangles
                .iter()
                .map(|t| {
                    let [a, b, c] = t.map(|i| coords[i]);
                    let s = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
                    assert!(s > 0.);
                    s * 0.5
                })
                .sum();
            assert_eq!(area, 2.);
        }
    }
    #[test]
    fn invalid_geometry_and_evaluator_failures_refuse() {
        assert!(refine(
            &LOOP,
            0.01,
            0.,
            None,
            |p| Ok([p[0], p[1], 0.]),
            |_, _| Ok(0.)
        )
        .is_err());
        let near_duplicate = [[0., 0.], [1e-14, 0.], [1., 0.], [1., 1.], [0., 1.]];
        assert_eq!(
            refine(
                &near_duplicate,
                0.01,
                0.9,
                None,
                |p| Ok([p[0], p[1], 0.]),
                |_, _| Ok(1.)
            )
            .unwrap_err(),
            "physical_refinement_distinct_boundary_sample_would_be_removed"
        );
        assert_eq!(
            refine(&LOOP, 0.01, 0.9, None, |_| Ok([1e308; 3]), |_, _| Ok(1.)).unwrap_err(),
            "physical_refinement_nonfinite_chord_error"
        );
        assert!(refine(&LOOP, f64::NAN, 0.9, None, |_| Ok([0.; 3]), |_, _| Ok(1.)).is_err());
        assert!(refine(&LOOP, 0.01, 0.9, None, |_| Ok([f64::NAN; 3]), |_, _| Ok(1.)).is_err());
        assert_eq!(
            refine(
                &LOOP,
                0.01,
                0.9,
                None,
                |_| Err("test_resource_refusal".into()),
                |_, _| Ok(1.)
            )
            .unwrap_err(),
            "test_resource_refusal"
        );
        let r = refine(
            &LOOP,
            0.1,
            0.99,
            None,
            |p| Ok([p[0], p[1], 0.]),
            |_, _| Ok(-1.),
        )
        .unwrap();
        assert_ne!(r["termination"], "sampled_targets_met");
        assert_eq!(r["admitted"], false);
    }
}
