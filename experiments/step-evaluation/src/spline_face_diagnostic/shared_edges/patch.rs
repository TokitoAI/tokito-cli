//! Canonical source boundaries for a two-face, open research patch.
//! The surfaces remain unchanged; no solid or production admission is claimed.
use super::*;
use std::cell::RefCell;

const MAX_CANONICAL: usize = 8192;
const MAX_CURVE_PROBES: usize = 32768;
const MAX_PATCH_VERTICES: usize = 50_000;
const MAX_PATCH_TRIANGLES: usize = 100_000;
type Uv = [f64; 2];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Key {
    Vertex(u64),
    Curve(u64, u64),
    Interior(u64, usize),
}
#[derive(Default)]
struct Registry {
    ids: BTreeMap<Key, usize>,
    points: Vec<P3>,
    records: Vec<Json>,
}
impl Registry {
    fn insert(&mut self, key: Key, p: P3) -> Result<usize, String> {
        if p.iter().any(|x| !x.is_finite()) {
            return Err("patch_nonfinite_canonical_point".into());
        }
        if let Some(&id) = self.ids.get(&key) {
            if self.points[id] != p {
                return Err("patch_conflicting_canonical_identity".into());
            }
            return Ok(id);
        }
        if self.points.len() >= MAX_PATCH_VERTICES {
            return Err("patch_global_vertex_budget".into());
        }
        let id = self.points.len();
        self.points.push(p);
        let origin = match key {
            Key::Vertex(v) => json!({"sourceVertexId":v}),
            Key::Curve(e, t) => json!({"edgeCurveId":e,"curveParameter":f64::from_bits(t)}),
            Key::Interior(f, v) => json!({"sourceFaceId":f,"localMeshVertexIndex":v}),
        };
        self.records
            .push(json!({"id":id,"pointSourceUnits":p,"origin":origin}));
        self.ids.insert(key, id);
        Ok(id)
    }
}
struct Curve {
    id: u64,
    geometry: u64,
    start: u64,
    end: u64,
    same: bool,
    samples: Vec<(f64, P3, usize)>,
    report: Json,
}
struct Face {
    id: u64,
    surface: u64,
    sense: bool,
    uses: Vec<(u64, bool, u64)>,
}
struct Boundary {
    uv: Vec<Uv>,
    global: Vec<usize>,
    views: Vec<Json>,
    maximum_displacement: f64,
}
fn face(step: &StepFile, fid: u64) -> Result<Face, String> {
    let f = entity(step, fid, "ADVANCED_FACE")?;
    let bounds = f
        .list("bounds")
        .filter(|b| b.len() == 1)
        .ok_or("patch_requires_one_outer_bound_no_holes")?;
    let b = entity(
        step,
        bounds[0].as_reference().ok_or("patch_missing_bound")?,
        "FACE_OUTER_BOUND",
    )?;
    let forward = b
        .logical("orientation")
        .ok_or("patch_unknown_bound_sense")?;
    let lp = entity(
        step,
        b.reference("bound").ok_or("patch_missing_loop")?,
        "EDGE_LOOP",
    )?;
    let refs = lp
        .list("edge_list")
        .filter(|e| !e.is_empty() && e.len() <= MAX_FACE_EDGES)
        .ok_or("patch_loop_inventory_limit")?;
    let mut uses = Vec::new();
    for r in refs {
        let id = r.as_reference().ok_or("patch_missing_oriented_use")?;
        let e = entity(step, id, "ORIENTED_EDGE")?;
        uses.push((
            e.reference("edge_element").ok_or("patch_missing_edge")?,
            e.logical("orientation").ok_or("patch_unknown_use_sense")? == forward,
            id,
        ));
    }
    if !forward {
        uses.reverse();
    }
    Ok(Face {
        id: fid,
        surface: f
            .reference("face_geometry")
            .ok_or("patch_missing_surface")?,
        sense: f.logical("same_sense").ok_or("patch_unknown_face_sense")?,
        uses,
    })
}
fn curve_point(
    curve: &ParsedCurve,
    t: f64,
    cache: &mut BTreeMap<u64, P3>,
    work: &mut Work,
    probes: &mut usize,
) -> Result<P3, String> {
    if let Some(&p) = cache.get(&t.to_bits()) {
        return Ok(p);
    }
    *probes += 1;
    if *probes > MAX_CURVE_PROBES {
        return Err("patch_curve_probe_budget".into());
    }
    work.curve_controls = work
        .curve_controls
        .checked_add(curve.control_point_count)
        .filter(|n| *n <= MAX_EDGE_CURVE_CONTROL_POINT_EVALUATIONS)
        .ok_or("patch_curve_control_budget")?;
    let p = curve.curve.point_at(t);
    if p.iter().any(|v| !v.is_finite()) {
        return Err("patch_nonfinite_curve_point".into());
    }
    cache.insert(t.to_bits(), p);
    Ok(p)
}
fn sample_edge(
    step: &StepFile,
    eid: u64,
    target: f64,
    registry: &mut Registry,
    work: &mut Work,
    probes: &mut usize,
) -> Result<Curve, String> {
    let e = entity(step, eid, "EDGE_CURVE")?;
    let geometry = e.reference("edge_geometry").ok_or("patch_missing_curve")?;
    let start = e.reference("edge_start").ok_or("patch_missing_start")?;
    let end = e.reference("edge_end").ok_or("patch_missing_end")?;
    if start == end {
        return Err("patch_closed_edge_parameter_interval_unsupported".into());
    }
    let same = e.logical("same_sense").ok_or("patch_unknown_curve_sense")?;
    let curve = parse_curve(step, geometry)?;
    let source = entity(step, geometry, "B_SPLINE_CURVE_WITH_KNOTS")?;
    let degree = required_integer(source, "degree", 64)?;
    let multiplicities = integer_list(source, "knot_multiplicities", MAX_CURVE_SPANS + 1)?;
    if multiplicities.first() != Some(&(degree + 1)) || multiplicities.last() != Some(&(degree + 1))
    {
        return Err("patch_requires_clamped_full_native_curve_interval".into());
    }
    let mut cache = BTreeMap::new();
    let first = curve_point(&curve, curve.domain.0, &mut cache, work, probes)?;
    let last = curve_point(&curve, curve.domain.1, &mut cache, work, probes)?;
    let natural_vertices = if same { [start, end] } else { [end, start] };
    let residuals = [
        distance(first, vertex(step, natural_vertices[0])?),
        distance(last, vertex(step, natural_vertices[1])?),
    ];
    if residuals != [0., 0.] {
        return Err("patch_full_curve_endpoints_do_not_exactly_match_source_vertices".into());
    }
    let mut leaves = Vec::new();
    let mut parameters = vec![curve.domain.0];
    for interval in curve.breaks.windows(2) {
        let mut stack = vec![(interval[0], interval[1], 0usize)];
        while let Some((a, b, depth)) = stack.pop() {
            let pa = curve_point(&curve, a, &mut cache, work, probes)?;
            let pb = curve_point(&curve, b, &mut cache, work, probes)?;
            let mut error = 0f64;
            for f in [0.25, 0.5, 0.75] {
                let t = a * (1. - f) + b * f;
                let p = curve_point(&curve, t, &mut cache, work, probes)?;
                error = error.max(distance(
                    p,
                    std::array::from_fn(|i| pa[i] * (1. - f) + pb[i] * f),
                ));
            }
            if error > target {
                let m = a * 0.5 + b * 0.5;
                if depth >= 16 || !(a < m && m < b) {
                    return Err("patch_curve_sampling_depth_or_precision_limit".into());
                }
                stack.push((m, b, depth + 1));
                stack.push((a, m, depth + 1));
            } else {
                parameters.push(b);
                leaves.push(json!({"startParameter":a,"endParameter":b,"maximumQuarterChordErrorSourceUnits":error}));
            }
            if registry.points.len() + parameters.len() > MAX_CANONICAL {
                return Err("patch_canonical_sample_budget".into());
            }
        }
    }
    let mut samples = Vec::new();
    for (i, &t) in parameters.iter().enumerate() {
        let p = curve_point(&curve, t, &mut cache, work, probes)?;
        let key = if i == 0 {
            Key::Vertex(natural_vertices[0])
        } else if i + 1 == parameters.len() {
            Key::Vertex(natural_vertices[1])
        } else {
            Key::Curve(eid, t.to_bits())
        };
        samples.push((t, p, registry.insert(key, p)?));
    }
    let report = json!({"edgeCurveId":eid,"curveGeometryId":geometry,"startVertexId":start,"endVertexId":end,"edgeSameSense":same,
        "nativeCurveDomain":curve.domain,"sourceKnotBreaks":curve.breaks,"clampedNativeEndpointsVerified":true,
        "naturalEndpointResidualsSourceUnits":residuals,"sampledChordTargetSourceUnits":target,"leafIntervals":leaves,
        "samples":samples.iter().map(|(t,_,id)|json!({"curveParameter":t,"globalVertexId":id})).collect::<Vec<_>>(),
        "certifiedDeviationBound":false,"selfIntersectionVerified":false});
    Ok(Curve {
        id: eid,
        geometry,
        start,
        end,
        same,
        samples,
        report,
    })
}
fn periods(support: &Support) -> Result<[Option<f64>; 2], String> {
    match support {
        Support::Spline { definition, .. } => Ok([definition.u_period, definition.v_period]),
        _ => Err("patch_requires_spline_supports".into()),
    }
}
fn lifted_uv(mut uv: Uv, previous: Option<Uv>, periods: [Option<f64>; 2]) -> Result<Uv, String> {
    if let Some(prev) = previous {
        for axis in 0..2 {
            let (value, shift) = unwrap_parameter(uv[axis], prev[axis], periods[axis])?;
            if shift.abs() > 1. {
                return Err("patch_periodic_branch_jump".into());
            }
            if periods[axis].is_some_and(|period| {
                ((value - prev[axis]).abs() - 0.5 * period).abs() <= period * 1e-10
            }) {
                return Err("patch_ambiguous_half_period_step".into());
            }
            uv[axis] = value;
        }
    }
    Ok(uv)
}
fn boundary(
    face: &Face,
    curves: &BTreeMap<u64, Curve>,
    support: &mut Support,
    work: &mut Work,
    allowance: f64,
) -> Result<Boundary, String> {
    let periods = periods(support)?;
    let mut result = Boundary {
        uv: Vec::new(),
        global: Vec::new(),
        views: Vec::new(),
        maximum_displacement: 0.,
    };
    let mut first_vertex = None;
    let mut previous_vertex = None;
    for &(eid, orientation, use_id) in &face.uses {
        let c = curves.get(&eid).ok_or("patch_curve_not_prepared")?;
        let (a, b) = if orientation {
            (c.start, c.end)
        } else {
            (c.end, c.start)
        };
        if previous_vertex.is_some_and(|v| v != a) {
            return Err("patch_source_vertex_chain_gap".into());
        }
        first_vertex.get_or_insert(a);
        previous_vertex = Some(b);
        let mut indices = (0..c.samples.len()).collect::<Vec<_>>();
        if c.same != orientation {
            indices.reverse();
        }
        let mut ids = Vec::new();
        for &i in &indices {
            let (_, p, gid) = c.samples[i];
            ids.push(gid);
            if result.global.last() == Some(&gid) {
                continue;
            }
            let closing = result.global.first() == Some(&gid);
            let (uv, _, _) = if closing {
                (result.uv[0], p, Json::Null)
            } else {
                project(support, p, work)?
            };
            let uv = lifted_uv(uv, result.uv.last().copied(), periods)?;
            if closing {
                if uv != result.uv[0] {
                    return Err("patch_noncontractible_uv_loop_unsupported".into());
                }
                continue;
            }
            if result.global.contains(&gid) {
                return Err("patch_repeated_nonclosing_source_identity".into());
            }
            let actual = support_point(support, uv, work)?;
            let displacement = distance(p, actual);
            if !displacement.is_finite() || displacement > allowance {
                return Err("patch_boundary_displacement_exceeds_explicit_allowance".into());
            }
            result.maximum_displacement = result.maximum_displacement.max(displacement);
            result.uv.push(uv);
            result.global.push(gid);
        }
        result.views.push(json!({"edgeCurveId":c.id,"curveGeometryId":c.geometry,"orientedEdgeId":use_id,"globalVertexIds":ids}));
    }
    if first_vertex != previous_vertex || result.uv.len() < 3 || result.uv.len() > 4096 {
        return Err("patch_invalid_source_loop_closure_or_size".into());
    }
    Ok(result)
}
fn support_point(support: &Support, uv: Uv, work: &mut Work) -> Result<P3, String> {
    match support {
        Support::Spline { definition, .. } => {
            if uv[0] < definition.u_domain.0
                || uv[0] > definition.u_domain.1
                || (definition.v_period.is_none()
                    && (uv[1] < definition.v_domain.0 || uv[1] > definition.v_domain.1))
                || uv.iter().any(|x| !x.is_finite())
            {
                return Err("patch_uv_outside_supported_chart".into());
            }
            eval_surface(definition, uv, &mut work.surface_calls, &mut work.controls)
        }
        _ => Err("patch_requires_spline_supports".into()),
    }
}
fn tangents(support: &Support, uv: Uv, work: &RefCell<Work>) -> Result<(P3, P3), String> {
    let Support::Spline { definition, .. } = support else {
        return Err("patch_requires_spline_supports".into());
    };
    let (a, b) = finite_difference_bounds(
        uv[0],
        (definition.u_domain.1 - definition.u_domain.0) * 1e-6,
        definition.u_domain,
        false,
    )
    .ok_or("patch_u_derivative_bounds")?;
    let (c, d) = finite_difference_bounds(
        uv[1],
        (definition.v_domain.1 - definition.v_domain.0) * 1e-6,
        definition.v_domain,
        definition.v_period.is_some(),
    )
    .ok_or("patch_v_derivative_bounds")?;
    let eval = |p| support_point(support, p, &mut work.borrow_mut());
    Ok((
        scale(sub(eval([b, uv[1]])?, eval([a, uv[1]])?), 1. / (b - a)),
        scale(sub(eval([uv[0], d])?, eval([uv[0], c])?), 1. / (d - c)),
    ))
}
fn mesh_face(
    face: &Face,
    boundary: &Boundary,
    support: &Support,
    registry: &mut Registry,
    work: &RefCell<Work>,
    tolerance: f64,
) -> Result<Json, String> {
    let Support::Spline { definition, .. } = support else {
        return Err("patch_requires_spline_supports".into());
    };
    let ranges: [Uv; 2] = std::array::from_fn(|a| {
        [
            boundary
                .uv
                .iter()
                .map(|p| p[a])
                .fold(f64::INFINITY, f64::min),
            boundary
                .uv
                .iter()
                .map(|p| p[a])
                .fold(f64::NEG_INFINITY, f64::max),
        ]
    });
    let anchor = ranges.map(|r| r[0] * 0.5 + r[1] * 0.5);
    let (du, dv) = tangents(support, anchor, work)?;
    let metric = crate::physical_surface_refinement::ChartMetric::from_tangents(anchor, du, dv)?;
    let evaluate = |uv| support_point(support, uv, &mut work.borrow_mut());
    let u = crate::seeded_domain::axis(&definition.u_breaks, ranges[0], definition.u_period)?;
    let v = crate::seeded_domain::axis(&definition.v_breaks, ranges[1], definition.v_period)?;
    let grid = crate::surface_grid::sample(&u, &v, tolerance * 0.5, evaluate)?;
    let points = boundary
        .global
        .iter()
        .map(|&i| registry.points[i])
        .collect::<Vec<_>>();
    let orientation = |uv: [Uv; 3], p: [P3; 3]| {
        let center = std::array::from_fn(|a| (uv[0][a] + uv[1][a] + uv[2][a]) / 3.);
        let (du, dv) = tangents(support, center, work)?;
        Ok(oriented_triangle_normal_alignment(p, du, dv, true))
    };
    let remaining = || {
        let w = work.borrow();
        Ok(MAX_SURFACE_EVALUATIONS.saturating_sub(w.surface_calls).min(
            MAX_DIAGNOSTIC_CONTROL_POINT_EVALUATIONS.saturating_sub(w.controls)
                / definition.control_point_count,
        ))
    };
    let mesh = crate::physical_surface_refinement::refine_canonical_seeded(
        crate::physical_surface_refinement::CanonicalInput {
            seeded: crate::physical_surface_refinement::SeededInput {
                boundary: &boundary.uv,
                grid,
            },
            points: &points,
        },
        tolerance,
        5f64.to_radians().cos(),
        Some(metric),
        evaluate,
        orientation,
        remaining,
    )?;
    let uv: Vec<Uv> =
        serde_json::from_value(mesh["uvVertices"].clone()).map_err(|e| e.to_string())?;
    let xyz: Vec<P3> = serde_json::from_value(mesh["surfacePointsSourceUnits"].clone())
        .map_err(|e| e.to_string())?;
    let mapping: Vec<usize> = serde_json::from_value(mesh["inputSampleVertexIndices"].clone())
        .map_err(|e| e.to_string())?;
    let mut global = vec![None; uv.len()];
    for (i, &local) in mapping.iter().enumerate() {
        let id = boundary.global[i];
        if xyz[local] != registry.points[id] {
            return Err("patch_refinement_changed_canonical_boundary".into());
        }
        if global[local].replace(id).is_some_and(|old| old != id) {
            return Err("patch_conflicting_local_boundary_mapping".into());
        }
    }
    for (i, p) in xyz.iter().enumerate() {
        if global[i].is_none() {
            global[i] = Some(registry.insert(Key::Interior(face.id, i), *p)?);
        }
    }
    let ids = global
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or("patch_incomplete_global_mapping")?;
    let triangles: Vec<[usize; 3]> =
        serde_json::from_value(mesh["triangles"].clone()).map_err(|e| e.to_string())?;
    let triangles = triangles
        .iter()
        .map(|t| {
            let mut t = t.map(|i| ids[i]);
            if !face.sense {
                t.swap(1, 2);
            }
            t
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"faceId":face.id,"surfaceId":face.surface,"faceSameSense":face.sense,"uvVertices":uv,"globalVertexIds":ids,"triangles":triangles,
        "boundaryGlobalVertexIds":boundary.global,"edgeViews":boundary.views,"maximumBoundaryDisplacementSourceUnits":boundary.maximum_displacement,
        "uvRange":ranges,"quality":mesh["quality"],"termination":mesh["termination"],"status":mesh["status"],"surfaceGridInitialization":mesh["surfaceGridInitialization"],
        "vertexCount":mesh["vertexCount"],"triangleCount":mesh["triangleCount"],"rounds":mesh["rounds"],"originalConstraintSegmentsPreserved":mesh["originalConstraintSegmentsPreserved"],
        "canonicalBoundaryUsedBeforeQualityEvaluation":true,"qualityNormalConvention":"natural_surface_orientation_before_explicit_face_sense_triangle_reversal",
        "trimMembershipVerified":false,"geometryQualified":false}),
    )
}
fn topology(
    faces: &[Json],
    curves: &BTreeMap<u64, Curve>,
    vertex_count: usize,
) -> Result<Json, String> {
    let mut incidence: BTreeMap<(usize, usize), Vec<(u64, bool)>> = BTreeMap::new();
    let mut used = BTreeSet::new();
    let mut total = 0usize;
    for f in faces {
        let fid = id(f, "faceId")?;
        let triangles: Vec<[usize; 3]> =
            serde_json::from_value(f["triangles"].clone()).map_err(|e| e.to_string())?;
        total += triangles.len();
        if total > MAX_PATCH_TRIANGLES {
            return Err("patch_triangle_budget".into());
        }
        for t in triangles {
            if t.iter().any(|&i| i >= vertex_count) || t[0] == t[1] || t[1] == t[2] || t[2] == t[0]
            {
                return Err("patch_invalid_global_triangle".into());
            }
            used.extend(t);
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let row = incidence.entry((a.min(b), a.max(b))).or_default();
                row.push((fid, a < b));
                if row.len() > 2 {
                    return Err("patch_nonmanifold_global_edge".into());
                }
            }
        }
    }
    if used.len() != vertex_count || incidence.values().any(|u| u.len() == 2 && u[0].1 == u[1].1) {
        return Err("patch_unused_vertex_or_inconsistent_winding".into());
    }
    let mut expected_outer = BTreeSet::new();
    let mut shared = Vec::new();
    for curve in curves.values() {
        let shared_edge = matches!(curve.id, 3478 | 9149);
        for pair in curve.samples.windows(2) {
            let (a, b) = (pair[0].2, pair[1].2);
            let key = (a.min(b), a.max(b));
            let uses = incidence
                .get(&key)
                .ok_or("patch_source_constraint_missing")?;
            if shared_edge {
                if uses.len() != 2 || uses[0].0 == uses[1].0 || uses[0].1 == uses[1].1 {
                    return Err("patch_shared_edge_not_opposite_two_face_incidence".into());
                }
            } else {
                expected_outer.insert(key);
            }
        }
        if shared_edge {
            shared.push(json!({"edgeCurveId":curve.id,"segmentCount":curve.samples.len()-1,"oppositeTwoFaceIncidenceVerified":true}));
        }
    }
    let outer = incidence
        .iter()
        .filter_map(|(e, u)| (u.len() == 1).then_some(*e))
        .collect::<BTreeSet<_>>();
    if outer != expected_outer {
        return Err("patch_open_boundary_inventory_mismatch".into());
    }
    let mut graph: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &(a, b) in &outer {
        graph.entry(a).or_default().push(b);
        graph.entry(b).or_default().push(a);
    }
    if graph.values().any(|n| n.len() != 2) {
        return Err("patch_boundary_not_disjoint_cycles".into());
    }
    let mut seen = BTreeSet::new();
    let mut cycles = 0;
    for &start in graph.keys() {
        if seen.contains(&start) {
            continue;
        }
        cycles += 1;
        let mut stack = vec![start];
        while let Some(v) = stack.pop() {
            if seen.insert(v) {
                stack.extend(&graph[&v]);
            }
        }
    }
    let euler = vertex_count as isize - incidence.len() as isize + total as isize;
    if euler != 0 || cycles != 2 || shared.len() != 2 {
        return Err("patch_requires_annulus_euler_zero_two_open_cycles".into());
    }
    Ok(
        json!({"vertices":vertex_count,"edges":incidence.len(),"triangles":total,"eulerCharacteristic":euler,"openBoundaryCycles":cycles,
        "openBoundarySegments":outer.len(),"sharedEdges":shared,"canonicalSeamIncidenceVerified":true,"closedSolidVerified":false}),
    )
}
pub(crate) fn inspect(
    step: &StepFile,
    source_topology: Json,
    tolerance: f64,
    allowance: f64,
) -> Result<Json, String> {
    if !tolerance.is_finite()
        || tolerance <= 0.
        || !allowance.is_finite()
        || allowance <= 0.
        || allowance > tolerance * 0.5
    {
        return Err("patch_invalid_chord_tolerance_or_boundary_allowance".into());
    }
    if source_topology["selectedFaceId"] != 2875
        || source_topology["representationId"] != 5957
        || source_topology["ownerId"] != 6218
    {
        return Err("patch_source_topology_identity_mismatch".into());
    }
    let faces = [face(step, 2875)?, face(step, 10475)?];
    let mut work = Work {
        surface_calls: 0,
        controls: 0,
        construction_estimate: 0,
        preparation_calls: 0,
        plane_projections: 0,
        curve_controls: 0,
        grid_points: 0,
        stored_controls: 0,
    };
    let mut supports = BTreeMap::new();
    for face in &faces {
        supports.insert(face.surface, prepare(step, face.surface, &mut work)?);
    }
    let edge_ids = faces
        .iter()
        .flat_map(|f| f.uses.iter().map(|u| u.0))
        .collect::<BTreeSet<_>>();
    if edge_ids != BTreeSet::from([3400, 3478, 4818, 5409, 7239, 9149]) {
        return Err("patch_source_edge_inventory_changed".into());
    }
    let mut registry = Registry::default();
    let mut curves = BTreeMap::new();
    let mut probes = 0;
    let mut curve_storage = 0;
    for eid in edge_ids {
        let e = entity(step, eid, "EDGE_CURVE")?;
        let cid = e.reference("edge_geometry").ok_or("patch_missing_curve")?;
        curve_storage = checked_edge_curve_control_point_total(
            curve_storage,
            preflight_curve_control_point_count(step, cid)?,
        )?;
        curves.insert(
            eid,
            sample_edge(
                step,
                eid,
                tolerance * 0.5,
                &mut registry,
                &mut work,
                &mut probes,
            )?,
        );
    }
    let canonical_count = registry.points.len();
    let mut boundaries = Vec::new();
    for f in &faces {
        boundaries.push(boundary(
            f,
            &curves,
            supports
                .get_mut(&f.surface)
                .ok_or("patch_missing_prepared_support")?,
            &mut work,
            allowance,
        )?);
    }
    let work = RefCell::new(work);
    let mut meshes = Vec::new();
    for (f, b) in faces.iter().zip(&boundaries) {
        meshes.push(mesh_face(
            f,
            b,
            &supports[&f.surface],
            &mut registry,
            &work,
            tolerance,
        )?);
    }
    let topology = topology(&meshes, &curves, registry.points.len())?;
    let w = work.borrow();
    Ok(
        json!({"schema":"tokito.canonical-shared-patch.v1","sourceTopology":source_topology,"sourceFaceIds":[2875,10475],
        "status":if meshes.iter().all(|m|m["termination"]=="sampled_targets_met") {"sampled_targets_met_unqualified"}else{"incomplete"},
        "policy":{"kind":"experimental_source_curve_anchored_mesh_boundary","chordToleranceSourceUnits":tolerance,"boundaryDisplacementAllowanceSourceUnits":allowance,
        "canonicalSourceCurveChordTargetSourceUnits":tolerance*0.5,"sourceUnchanged":true,"productionAcceptancePolicyApproved":false,"sourceUncertaintyUsedAsAllowance":false},
        "edges":curves.values().map(|c|&c.report).collect::<Vec<_>>(),"globalVertices":registry.records,"canonicalBoundaryVertexCount":canonical_count,
        "faces":meshes,"topology":topology,"work":{"curveProbeCount":probes,"curveProbeLimit":MAX_CURVE_PROBES,"curveControlWork":w.curve_controls,
            "retainedCurveControlPoints":curve_storage,"curveControlStorageLimit":MAX_TOTAL_EDGE_CURVE_CONTROL_POINTS,
            "surfaceCallUnits":w.surface_calls,"preparationSurfaceCallEstimate":w.preparation_calls,"surfaceControlWork":w.controls,"preparationControlWorkEstimate":w.construction_estimate,
            "surfaceCallLimit":MAX_SURFACE_EVALUATIONS,"surfaceControlWorkLimit":MAX_DIAGNOSTIC_CONTROL_POINT_EVALUATIONS,"aggregateAcrossBothFaces":true},
        "certifiedDeviationBound":false,"completeTrimCoverageVerified":false,"selfIntersectionVerified":false,"completeSolidVerified":false,
        "sourceCurveSupportGapResolved":false,"geometryQualified":false,"admitted":false,"exportsProduced":[]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn periodic_views_are_local_and_half_period_ambiguity_refuses() {
        let p = [None, Some(1.)];
        assert_eq!(lifted_uv([2., 0.1], Some([2., 0.9]), p).unwrap(), [2., 1.1]);
        assert_eq!(
            lifted_uv([2., 0.9], Some([2., 0.1]), p).unwrap(),
            [2., -0.09999999999999998]
        );
        assert!(lifted_uv([2., 0.5], Some([2., 0.]), p)
            .unwrap_err()
            .contains("half_period"));
        // A closing source point needing a whole period is not the first UV.
        assert_ne!(lifted_uv([2., 0.], Some([2., 0.9]), p).unwrap(), [2., 0.]);
        assert!(lifted_uv([2., 0.], Some([2., 3.9]), p).is_err());
    }
    #[test]
    fn false_bound_reverses_sequence_and_effective_edge_senses() {
        let data = b"ISO-10303-21;HEADER;FILE_DESCRIPTION((''),'2;1');FILE_NAME('','',(''),(''),'','','');FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;#100=ADVANCED_FACE('',(#101),#1,.T.);#101=FACE_OUTER_BOUND('',#102,.F.);#102=EDGE_LOOP('',(#103,#104));#103=ORIENTED_EDGE('',*,*,#11,.T.);#104=ORIENTED_EDGE('',*,*,#12,.F.);ENDSEC;END-ISO-10303-21;";
        // This isolated loop-view test does not dereference surface/edge geometry.
        let step = oxideav_ifc::parse_step(data).unwrap();
        assert_eq!(
            face(&step, 100).unwrap().uses,
            vec![(12, true, 104), (11, false, 103)]
        );
    }
    #[test]
    fn two_complementary_face_disks_form_an_open_annulus() {
        let mut triangles = [Vec::new(), Vec::new()];
        for a in 0..4 {
            let b = (a + 1) % 4;
            let owner = usize::from(a >= 2);
            triangles[owner].push([a, b, 4 + b]);
            triangles[owner].push([a, 4 + b, 4 + a]);
        }
        let faces = [
            json!({"faceId":2875,"triangles":triangles[0]}),
            json!({"faceId":10475,"triangles":triangles[1]}),
        ];
        let ids = [
            (3400, vec![0, 1, 2]),
            (7239, vec![2, 3, 0]),
            (5409, vec![4, 5, 6]),
            (4818, vec![6, 7, 4]),
            (3478, vec![0, 4]),
            (9149, vec![2, 6]),
        ];
        let mut curves = BTreeMap::new();
        for (id, vertices) in ids {
            let samples = vertices
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let a = (v % 4) as f64 * std::f64::consts::FRAC_PI_2;
                    (i as f64, [a.cos(), a.sin(), (v / 4) as f64], v)
                })
                .collect();
            curves.insert(
                id,
                Curve {
                    id,
                    geometry: id,
                    start: vertices[0] as u64,
                    end: *vertices.last().unwrap() as u64,
                    same: true,
                    samples,
                    report: Json::Null,
                },
            );
        }
        let result = topology(&faces, &curves, 8).unwrap();
        assert_eq!(result["eulerCharacteristic"], 0);
        assert_eq!(result["openBoundaryCycles"], 2);
        assert_eq!(result["closedSolidVerified"], false);
        assert!(topology(&faces, &curves, 9).is_err());
        let mut reversed = faces.clone();
        reversed[1]["triangles"] = json!(triangles[1]
            .iter()
            .map(|t| [t[0], t[2], t[1]])
            .collect::<Vec<_>>());
        assert!(topology(&reversed, &curves, 8).is_err());
        curves
            .get_mut(&3478)
            .unwrap()
            .samples
            .insert(1, (0.5, [0., 0., 0.5], 1));
        assert!(topology(&faces, &curves, 8).is_err());
    }
    #[test]
    fn registry_uses_source_identity_not_coordinate_welding() {
        let mut r = Registry::default();
        let p = [1., 2., 3.];
        let a = r.insert(Key::Vertex(7), p).unwrap();
        assert_eq!(r.insert(Key::Vertex(7), p).unwrap(), a);
        assert_ne!(r.insert(Key::Vertex(8), p).unwrap(), a);
        assert_ne!(r.insert(Key::Curve(7, 0.5f64.to_bits()), p).unwrap(), a);
        assert!(r.insert(Key::Vertex(7), [1., 2., 3.1]).is_err());
        assert!(r.insert(Key::Vertex(9), [f64::NAN, 2., 3.]).is_err());
    }
}
