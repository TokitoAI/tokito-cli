//! Original bounded source-face triangulation. No model mutation or admission.
use oxideav_ifc::StepFile;
use oxideav_step::schema::Entity;
use serde_json::{json, Value as Json};
use std::collections::{BTreeMap, BTreeSet};

const MAX_VERTICES: usize = 512;
const MAX_WORK: usize = 10_000_000;
type P2 = [f64; 2];
type P3 = [f64; 3];

fn sub(a: P3, b: P3) -> P3 {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: P3, b: P3) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: P3) -> Result<P3, String> {
    let n = norm(a);
    if !n.is_finite() || n == 0. {
        return Err("zero_or_invalid_direction".into());
    }
    Ok(a.map(|v| v / n))
}
fn orient(a: P2, b: P2, c: P2) -> Result<f64, String> {
    let x = (b[0] - a[0]) * (c[1] - a[1]);
    let y = (b[1] - a[1]) * (c[0] - a[0]);
    let det = x - y;
    if !det.is_finite() {
        return Err("ambiguous_orientation_predicate".into());
    }
    if det.abs() <= (x.abs() + y.abs()).max(f64::MIN_POSITIVE) * 8. * f64::EPSILON {
        if crate::exact_predicate::collinear(a, b, c)? {
            return Ok(0.);
        }
        return Err("ambiguous_orientation_predicate".into());
    }
    Ok(det)
}
fn on_segment(a: P2, b: P2, p: P2) -> bool {
    (0..2).all(|i| p[i] >= a[i].min(b[i]) && p[i] <= a[i].max(b[i]))
}
fn intersects(a: P2, b: P2, c: P2, d: P2) -> Result<bool, String> {
    let ab_c = orient(a, b, c)?;
    let ab_d = orient(a, b, d)?;
    let cd_a = orient(c, d, a)?;
    let cd_b = orient(c, d, b)?;
    Ok((ab_c == 0. && on_segment(a, b, c))
        || (ab_d == 0. && on_segment(a, b, d))
        || (cd_a == 0. && on_segment(c, d, a))
        || (cd_b == 0. && on_segment(c, d, b))
        || ((ab_c > 0.) != (ab_d > 0.) && (cd_a > 0.) != (cd_b > 0.)))
}

/// Ear clipping keeps every boundary vertex, including exactly collinear ones.
/// Such vertices are never clipped as zero-area ears; ears crossing them refuse.
fn triangulate(points: &[P2], max_work: usize) -> Result<Vec<[usize; 3]>, String> {
    if !(3..=MAX_VERTICES).contains(&points.len()) {
        return Err("face_vertex_limit".into());
    }
    let mut work = 0;
    let mut tick = || {
        work += 1;
        if work > max_work {
            Err("face_work_limit".to_string())
        } else {
            Ok(())
        }
    };
    for i in 0..points.len() {
        let next = (i + 1) % points.len();
        if points[i] == points[next] {
            return Err("zero_length_boundary".into());
        }
        for j in i + 1..points.len() {
            if j == next || (j + 1) % points.len() == i {
                continue;
            }
            tick()?;
            if intersects(
                points[i],
                points[next],
                points[j],
                points[(j + 1) % points.len()],
            )? {
                return Err("self_intersecting_boundary".into());
            }
        }
    }
    let origin = points[0];
    let twice_area: f64 = (1..points.len() - 1)
        .map(|i| orient(origin, points[i], points[i + 1]))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum();
    if !twice_area.is_finite() || twice_area == 0. {
        return Err("zero_face_area".into());
    }
    let mut remaining: Vec<usize> = (0..points.len()).collect();
    if twice_area < 0. {
        remaining.reverse();
    }
    let mut triangles = Vec::with_capacity(points.len() - 2);
    while remaining.len() > 3 {
        let mut ear = None;
        for i in 0..remaining.len() {
            tick()?;
            let ids = [
                remaining[(i + remaining.len() - 1) % remaining.len()],
                remaining[i],
                remaining[(i + 1) % remaining.len()],
            ];
            let [a, b, c] = ids.map(|j| points[j]);
            if orient(a, b, c)? <= 0. {
                continue;
            }
            let mut blocked = false;
            for &j in &remaining {
                tick()?;
                if ids.contains(&j) {
                    continue;
                }
                let p = points[j];
                if orient(a, b, p)? >= 0. && orient(b, c, p)? >= 0. && orient(c, a, p)? >= 0. {
                    blocked = true;
                    break;
                }
            }
            if !blocked {
                ear = Some((i, ids));
                break;
            }
        }
        let (index, triangle) = ear.ok_or("no_unambiguous_boundary_preserving_ear")?;
        triangles.push(triangle);
        remaining.remove(index);
    }
    let last = [remaining[0], remaining[1], remaining[2]];
    if orient(points[last[0]], points[last[1]], points[last[2]])? <= 0. {
        return Err("degenerate_final_triangle".into());
    }
    triangles.push(last);
    let mut incidence = BTreeMap::<(usize, usize), (usize, i32)>::new();
    let mut area = 0.;
    for t in &triangles {
        area += orient(points[t[0]], points[t[1]], points[t[2]])?;
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let entry = incidence.entry((a.min(b), a.max(b))).or_default();
            entry.0 += 1;
            entry.1 += if a < b { 1 } else { -1 };
        }
    }
    let boundary: BTreeSet<_> = (0..points.len())
        .map(|i| (i.min((i + 1) % points.len()), i.max((i + 1) % points.len())))
        .collect();
    if triangles.len() != points.len() - 2
        || boundary
            .iter()
            .any(|edge| incidence.get(edge).map(|v| v.0) != Some(1))
        || incidence.iter().any(|(edge, &(count, direction))| {
            !boundary.contains(edge) && (count != 2 || direction != 0)
        })
    {
        return Err("triangulation_boundary_or_incidence_mismatch".into());
    }
    if (area - twice_area.abs()).abs()
        > twice_area.abs() * 128. * f64::EPSILON * points.len() as f64
    {
        return Err("triangulation_area_mismatch".into());
    }
    Ok(triangles)
}

struct Source<'a> {
    step: &'a StepFile,
    work: usize,
}
impl<'a> Source<'a> {
    fn entity(&mut self, id: u64, kind: &str) -> Result<Entity<'a>, String> {
        self.work += 1;
        if self.work > 10_000 {
            return Err("face_source_work_limit".into());
        }
        let inst = self
            .step
            .get(id)
            .ok_or_else(|| format!("dangling_entity:{id}"))?;
        crate::unit_preflight::shape(inst)?;
        let entity = Entity { inst };
        if !entity.is_a(kind) {
            return Err(format!("unsupported_entity:{id}:expected_{kind}"));
        }
        Ok(entity)
    }
    fn vector(&mut self, id: u64, kind: &str, attr: &str) -> Result<P3, String> {
        let e = self.entity(id, kind)?;
        let values = e.list(attr).ok_or("missing_coordinate_list")?;
        if values.len() != 3 {
            return Err("three_coordinates_required".into());
        }
        let mut result = [0.; 3];
        for (i, v) in values.iter().enumerate() {
            result[i] = v
                .as_number()
                .filter(|v| v.is_finite() && v.abs() <= 1e12)
                .ok_or("invalid_or_unbounded_coordinate")?;
        }
        Ok(result)
    }
    fn point(&mut self, id: u64) -> Result<P3, String> {
        self.vector(id, "CARTESIAN_POINT", "coordinates")
    }
    fn direction(&mut self, id: u64) -> Result<P3, String> {
        unit(self.vector(id, "DIRECTION", "direction_ratios")?)
    }
    fn vertex(&mut self, id: u64) -> Result<P3, String> {
        let v = self.entity(id, "VERTEX_POINT")?;
        self.point(
            v.reference("vertex_geometry")
                .ok_or("missing_vertex_point")?,
        )
    }
}

pub struct SourceFace {
    pub vertices: Vec<P3>,
    pub triangles: Vec<[usize; 3]>,
    pub report: Json,
}
pub fn inspect(step: &StepFile, face_id: u64, tolerance: f64) -> Result<Json, String> {
    Ok(build(step, face_id, tolerance)?.report)
}
pub fn build(step: &StepFile, face_id: u64, tolerance: f64) -> Result<SourceFace, String> {
    if !tolerance.is_finite() || tolerance <= 0. {
        return Err("explicit_positive_source_tolerance_required".into());
    }
    let mut source = Source { step, work: 0 };
    let face = source.entity(face_id, "ADVANCED_FACE")?;
    let bounds = face.list("bounds").ok_or("missing_bounds")?;
    if bounds.len() != 1 {
        return Err("single_outer_bound_only".into());
    }
    let bound_id = bounds[0].as_reference().ok_or("invalid_bound_reference")?;
    let bound = source.entity(bound_id, "FACE_OUTER_BOUND")?;
    let bound_sense = bound
        .logical("orientation")
        .ok_or("unknown_bound_orientation")?;
    let face_sense = face.logical("same_sense").ok_or("unknown_face_sense")?;
    let plane_id = face
        .reference("face_geometry")
        .ok_or("missing_face_surface")?;
    let plane = source.entity(plane_id, "PLANE")?;
    let placement = source.entity(
        plane
            .reference("position")
            .ok_or("missing_plane_placement")?,
        "AXIS2_PLACEMENT_3D",
    )?;
    let origin = source.point(
        placement
            .reference("location")
            .ok_or("missing_plane_location")?,
    )?;
    let axis = source.direction(
        placement
            .reference("axis")
            .ok_or("explicit_plane_axis_required")?,
    )?;
    let reference = source.direction(
        placement
            .reference("ref_direction")
            .ok_or("explicit_reference_direction_required")?,
    )?;
    let y = unit(cross(axis, reference))?;
    let x = unit(cross(y, axis))?;
    let loop_id = bound.reference("bound").ok_or("missing_bound_loop")?;
    let loop_entity = source.entity(loop_id, "EDGE_LOOP")?;
    let edges = loop_entity.list("edge_list").ok_or("missing_edge_list")?;
    if !(3..=MAX_VERTICES).contains(&edges.len()) {
        return Err("face_edge_limit".into());
    }
    let mut ids = Vec::new();
    let mut points = Vec::new();
    let mut edge_ids = Vec::new();
    let mut previous_end = None;
    let mut used_edges = BTreeSet::new();
    let mut max_line_residual: f64 = 0.;
    for value in edges {
        let id = value
            .as_reference()
            .ok_or("invalid_oriented_edge_reference")?;
        if !used_edges.insert(id) {
            return Err("duplicate_oriented_edge".into());
        }
        let oriented = source.entity(id, "ORIENTED_EDGE")?;
        let edge_id = oriented
            .reference("edge_element")
            .ok_or("missing_edge_element")?;
        let edge = source.entity(edge_id, "EDGE_CURVE")?;
        // Curve parameter direction does not reverse topological endpoints again.
        let curve_sense = edge.logical("same_sense").ok_or("unknown_curve_sense")?;
        let mut start = edge.reference("edge_start").ok_or("missing_edge_start")?;
        let mut end = edge.reference("edge_end").ok_or("missing_edge_end")?;
        let edge_sense = oriented
            .logical("orientation")
            .ok_or("unknown_edge_orientation")?;
        if !edge_sense {
            std::mem::swap(&mut start, &mut end);
        }
        if previous_end.is_some_and(|previous| previous != start) {
            return Err("source_vertex_chain_gap".into());
        }
        let a = source.vertex(start)?;
        let b = source.vertex(end)?;
        if a == b || ids.contains(&start) {
            return Err("zero_length_or_repeated_source_vertex".into());
        }
        let line = source.entity(
            edge.reference("edge_geometry")
                .ok_or("missing_edge_curve")?,
            "LINE",
        )?;
        let line_origin = source.point(line.reference("pnt").ok_or("missing_line_point")?)?;
        let vector = source.entity(
            line.reference("dir").ok_or("missing_line_vector")?,
            "VECTOR",
        )?;
        if !vector
            .number("magnitude")
            .is_some_and(|n| n.is_finite() && n > 0.)
        {
            return Err("invalid_line_magnitude".into());
        }
        let direction = source.direction(
            vector
                .reference("orientation")
                .ok_or("missing_line_direction")?,
        )?;
        let parameter_direction = dot(sub(b, a), direction);
        if parameter_direction == 0. || ((parameter_direction > 0.) != (curve_sense == edge_sense))
        {
            return Err("line_parameter_and_topological_sense_mismatch".into());
        }
        for p in [a, b] {
            max_line_residual = max_line_residual.max(norm(cross(sub(p, line_origin), direction)));
        }
        ids.push(start);
        points.push(a);
        edge_ids.push(id);
        previous_end = Some(end);
    }
    if previous_end != ids.first().copied() {
        return Err("source_loop_not_closed".into());
    }
    if max_line_residual > tolerance {
        return Err("source_endpoint_line_residual_exceeds_explicit_tolerance".into());
    }
    let max_plane_residual = points
        .iter()
        .map(|p| dot(sub(*p, origin), axis).abs())
        .fold(0., f64::max);
    if max_plane_residual > tolerance {
        return Err("source_plane_residual_exceeds_explicit_tolerance".into());
    }
    let projected: Vec<P2> = points
        .iter()
        .map(|p| {
            let d = sub(*p, origin);
            [dot(d, x), dot(d, y)]
        })
        .collect();
    let signed_twice_area = (1..projected.len() - 1)
        .map(|i| orient(projected[0], projected[i], projected[i + 1]))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum::<f64>();
    if signed_twice_area == 0. || (((signed_twice_area > 0.) == bound_sense) != face_sense) {
        return Err("outer_bound_orientation_inconsistent_with_face_surface".into());
    }
    let mut triangles = triangulate(&projected, MAX_WORK)?;
    if !face_sense {
        for t in &mut triangles {
            t.swap(1, 2);
        }
    }
    let expected_normal = axis.map(|v| if face_sense { v } else { -v });
    for t in &triangles {
        if dot(
            cross(
                sub(points[t[1]], points[t[0]]),
                sub(points[t[2]], points[t[0]]),
            ),
            expected_normal,
        ) <= 0.
        {
            return Err("triangle_source_normal_mismatch".into());
        }
    }
    let report = json!({"schema":"tokito.step-linear-planar-face.v1","faceId":face_id,"surfaceId":plane_id,
        "boundId":bound_id,"loopId":loop_id,"boundOrientation":bound_sense,"faceSameSense":face_sense,
        "sourceVertexIds":ids,"orientedEdgeIds":edge_ids,"verticesSourceUnits":points,
        "projectedVerticesSourceUnits":projected,"triangles":triangles,"triangleCount":triangles.len(),
        "boundarySegmentsPreserved":points.len(),"maximumPlaneResidualSourceUnits":max_plane_residual,
        "projectedAreaSourceUnitsSquared":signed_twice_area.abs()*0.5,
        "outerBoundOrientationVerified":true,
        "maximumLineResidualSourceUnits":max_line_residual,"callerToleranceSourceUnits":tolerance,
        "sourceTraversalWork":source.work,"sourceToModelFrameVerified":false,"sourceLengthUnitsResolved":false,
        "geometryQualified":false,"admitted":false,"exportsProduced":[],
        "scope":"one source-native planar LINE loop triangulation; no reader mesh replacement, representation frame/unit admission, solid closure or component/PCB qualification"});
    Ok(SourceFace {
        vertices: points,
        triangles,
        report,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn source_text(
        edge_sense: bool,
        bound_sense: bool,
        face_sense: bool,
        scale: f64,
    ) -> String {
        let points = [
            [0., 0., 0.],
            [2. * scale, 0., 0.],
            [2. * scale, scale, 0.],
            [0., scale, 0.],
        ];
        let mut data = String::new();
        for (i, p) in points.iter().enumerate() {
            data += &format!(
                "#{}=CARTESIAN_POINT('',({},{},{}));#{}=VERTEX_POINT('',#{});",
                i + 1,
                p[0],
                p[1],
                p[2],
                i + 11,
                i + 1
            );
        }
        for i in 0..4 {
            let next = (i + 1) % 4;
            let (start, end) = if edge_sense { (i, next) } else { (next, i) };
            let d = sub(points[end], points[start]);
            data += &format!("#{}=DIRECTION('',({},{},{}));#{}=VECTOR('',#{},1.);#{}=LINE('',#{},#{});#{}=EDGE_CURVE('',#{},#{},#{},.T.);#{}=ORIENTED_EDGE('',*,*,#{},.{}.);",
                21+i,d[0],d[1],d[2],31+i,21+i,41+i,start+1,31+i,51+i,start+11,end+11,41+i,61+i,51+i,if edge_sense {"T"} else {"F"});
        }
        data += &format!("#25=DIRECTION('',(0.,0.,1.));#26=DIRECTION('',(1.,0.,0.));#90=CARTESIAN_POINT('',(0.,0.,0.));#71=EDGE_LOOP('',(#61,#62,#63,#64));#72=FACE_OUTER_BOUND('',#71,.{}.);#73=AXIS2_PLACEMENT_3D('',#90,#25,#26);#74=PLANE('',#73);#75=ADVANCED_FACE('',(#72),#74,.{}.);",if bound_sense {"T"} else {"F"},if face_sense {"T"} else {"F"});
        format!("ISO-10303-21;HEADER;FILE_DESCRIPTION((''),'2;1');FILE_NAME('','',(''),(''),'','','');FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;{data}ENDSEC;END-ISO-10303-21;")
    }
    fn parsed(text: &str) -> StepFile {
        oxideav_ifc::parse_step(text.as_bytes()).unwrap()
    }
    #[test]
    fn source_identity_senses_and_coordinate_scales_remain_explicit() {
        for edge in [false, true] {
            for bound in [false, true] {
                for face in [false, true] {
                    for scale in [0.001, 1., 1000.] {
                        let step = parsed(&source_text(edge, bound, face, scale));
                        let result = inspect(&step, 75, 1e-9);
                        if bound != face {
                            assert!(result.is_err());
                            continue;
                        }
                        let result = result.unwrap();
                        assert_eq!(result["sourceVertexIds"], json!([11, 12, 13, 14]));
                        assert_eq!(result["triangleCount"], 2);
                        assert_eq!(result["boundarySegmentsPreserved"], 4);
                        assert_eq!(result["boundOrientation"], bound);
                        assert_eq!(result["faceSameSense"], face);
                        assert_eq!(result["sourceToModelFrameVerified"], false);
                        assert_eq!(result["sourceLengthUnitsResolved"], false);
                        assert_eq!(result["admitted"], false);
                        let p = &result["verticesSourceUnits"];
                        for t in result["triangles"].as_array().unwrap() {
                            let vertex = |index: usize| -> P3 {
                                std::array::from_fn(|axis| {
                                    p[t[index].as_u64().unwrap() as usize][axis]
                                        .as_f64()
                                        .unwrap()
                                })
                            };
                            let z = cross(sub(vertex(1), vertex(0)), sub(vertex(2), vertex(0)))[2];
                            assert_eq!(z > 0., face);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn source_unsupported_or_malformed_geometry_refuses() {
        let text = source_text(false, true, true, 1.);
        for changed in [
            text.replace("(#72),#74", "(#72,#72),#74"),
            text.replace("#61,#62,#63,#64", "#61,#63,#62,#64"),
            text.replace("#61,#62,#63,#64", "#61,#61,#63,#64"),
            text.replace("#51,.F.", "#51,.U."),
            text.replace("#41=LINE", "#41=CIRCLE"),
            text.replace(
                "#51=EDGE_CURVE('',#12,#11,#41,.T.)",
                "#51=EDGE_CURVE('',#12,#11,#41,.F.)",
            ),
            text.replace(
                "#73=AXIS2_PLACEMENT_3D('',#90,#25,#26)",
                "#73=AXIS2_PLACEMENT_3D('',#90,#25,#25)",
            ),
            text.replace(
                "#90=CARTESIAN_POINT('',(0.,0.,0.))",
                "#90=CARTESIAN_POINT('',(0.,0.,1.))",
            ),
        ] {
            assert_ne!(changed, text, "test mutation must change fixture");
            assert!(inspect(&parsed(&changed), 75, 1e-9).is_err());
        }
        for tolerance in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(inspect(&parsed(&text), 75, tolerance).is_err());
        }
        assert!(inspect(&parsed(&text), 999, 1e-9).is_err());
    }
    #[test]
    fn preserves_collinear_and_tiny_boundaries_in_both_windings() {
        for mut p in [
            vec![[0., 0.], [1e-9, 0.], [1., 0.], [1., 1.], [0., 1.]],
            vec![[0., 0.], [2., 0.], [2., 2.], [1., 1.], [0., 2.]],
        ] {
            for _ in 0..2 {
                let t = triangulate(&p, MAX_WORK).unwrap();
                assert_eq!(t.len(), p.len() - 2);
                p.reverse();
            }
        }
    }
    #[test]
    fn refuses_crossing_zero_length_and_work_exhaustion() {
        assert!(orient([0., 0.], [134217729., 134217728.], [134217728., 134217727.]).is_err());
        assert!(triangulate(&[[0., 0.], [1., 1.], [0., 1.], [1., 0.]], MAX_WORK).is_err());
        assert!(triangulate(&[[0., 0.], [0., 0.], [1., 1.]], MAX_WORK).is_err());
        assert!(triangulate(&[[0., 0.], [1., 0.], [1., 1.], [0., 1.]], 0).is_err());
    }
}
