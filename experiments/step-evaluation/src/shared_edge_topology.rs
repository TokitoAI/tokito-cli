//! Bounded source identity/use audit. No geometric sewing or solid certification.
use oxideav_ifc::{StepFile, Value as StepValue};
use oxideav_step::schema::Entity;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_ENTITIES: usize = 250_000;
const MAX_WORK: usize = 100_000;
const MAX_FACES: usize = 4096;
const MAX_BOUNDS: usize = 128;
const MAX_LOOP_USES: usize = 4096;
struct Source<'a> {
    step: &'a StepFile,
    work: usize,
}
impl<'a> Source<'a> {
    fn entity(&mut self, id: u64, kind: &str) -> Result<Entity<'a>, String> {
        self.work += 1;
        if self.work > MAX_WORK {
            return Err("shared_edge_topology_work_limit".into());
        }
        let inst = self
            .step
            .get(id)
            .ok_or_else(|| format!("dangling_shared_edge_entity:{id}"))?;
        crate::unit_preflight::shape(inst)?;
        let e = Entity { inst };
        if !e.is_a(kind) {
            return Err(format!(
                "unsupported_shared_edge_entity:{id}:expected_{kind}"
            ));
        }
        Ok(e)
    }
    fn vertex(&mut self, id: u64) -> Result<(), String> {
        let e = self.entity(id, "VERTEX_POINT")?;
        let p = self.entity(
            e.reference("vertex_geometry")
                .ok_or("missing_vertex_geometry")?,
            "CARTESIAN_POINT",
        )?;
        let xyz = p.list("coordinates").ok_or("missing_vertex_coordinates")?;
        if xyz.len() != 3
            || xyz
                .iter()
                .any(|x| x.as_number().is_none_or(|n| !n.is_finite()))
        {
            return Err("shared_edge_vertex_requires_finite_3d_point".into());
        }
        Ok(())
    }
}
fn refs(e: Entity<'_>, attr: &str, max: usize) -> Result<Vec<u64>, String> {
    let list = e
        .list(attr)
        .ok_or_else(|| format!("missing_shared_edge_list:{attr}"))?;
    if list.is_empty() || list.len() > max {
        return Err(format!("shared_edge_list_limit:{attr}"));
    }
    let out: Vec<_> = list
        .iter()
        .map(|v| {
            v.as_reference()
                .ok_or("invalid_shared_edge_reference".to_owned())
        })
        .collect::<Result<_, _>>()?;
    if out.iter().copied().collect::<BTreeSet<_>>().len() != out.len() {
        return Err(format!("duplicate_shared_edge_reference:{attr}"));
    }
    Ok(out)
}
fn logical(e: Entity<'_>, name: &str) -> Result<bool, String> {
    e.logical(name)
        .ok_or_else(|| format!("unknown_shared_edge_boolean:{name}"))
}
fn surface_flags(e: Entity<'_>) -> Result<Value, String> {
    if !e.is_a("B_SPLINE_SURFACE") {
        return Ok(
            json!({"available":false,"reason":"not_B_SPLINE_SURFACE","surfaceType":e.inst.keyword}),
        );
    }
    let flag = |name: &str| -> Result<Value, String> {
        match e.attr(name).and_then(StepValue::as_enum) {
            Some("T") => Ok(json!({"value":true,"sourceLogical":"T"})),
            Some("F") => Ok(json!({"value":false,"sourceLogical":"F"})),
            Some("U") => Ok(json!({"value":null,"sourceLogical":"U","status":"unknown"})),
            _ => Err(format!("invalid_source_surface_logical:{name}")),
        }
    };
    Ok(
        json!({"available":true,"u_closed":flag("u_closed")?,"v_closed":flag("v_closed")?,
        "self_intersect":flag("self_intersect")?,"sourceDeclarationsOnly":true,"geometryVerified":false}),
    )
}
struct Edge {
    curve: u64,
    start: u64,
    end: u64,
    sense: bool,
    uses: Vec<Value>,
}

pub(crate) fn inspect(
    step: &StepFile,
    representation: u64,
    owner: u64,
    face: u64,
) -> Result<Value, String> {
    if step.instances.len() > MAX_ENTITIES {
        return Err("shared_edge_global_inventory_limit".into());
    }
    // Existing ownership audit enforces exactly one selected representation item
    // containing the face and validates its physical representation context.
    let ownership = crate::face_representation::inspect(step, representation, face)?;
    if ownership["representationItemId"] != owner {
        return Err("selected_shared_edge_owner_mismatch".into());
    }
    let mut src = Source { step, work: 0 };
    let solid = src.entity(owner, "MANIFOLD_SOLID_BREP")?;
    if solid.is_a("BREP_WITH_VOIDS") || solid.inst.keyword != "MANIFOLD_SOLID_BREP" {
        return Err("shared_edge_requires_simple_manifold_solid_without_voids".into());
    }
    let shell_id = solid
        .reference("outer")
        .ok_or("missing_shared_edge_outer_shell")?;
    let shell = src.entity(shell_id, "CLOSED_SHELL")?;
    if shell.inst.keyword != "CLOSED_SHELL" {
        return Err("shared_edge_requires_unoriented_closed_shell".into());
    }
    let faces = refs(shell, "cfs_faces", MAX_FACES)?;
    if faces.iter().filter(|&&id| id == face).count() != 1 {
        return Err("selected_face_requires_unique_shell_membership".into());
    }
    let mut edges: BTreeMap<u64, Edge> = BTreeMap::new();
    let mut selected = BTreeSet::new();
    let mut face_reports = Vec::new();
    let mut bounds_seen = BTreeSet::new();
    let mut loops_seen = BTreeSet::new();
    let mut uses_seen = BTreeSet::new();
    for &face_id in &faces {
        let f = src.entity(face_id, "ADVANCED_FACE")?;
        if f.inst.keyword != "ADVANCED_FACE" {
            return Err("unsupported_oriented_or_complex_face".into());
        }
        let sense = logical(f, "same_sense")?;
        let surface_id = f
            .reference("face_geometry")
            .ok_or("missing_shared_edge_face_geometry")?;
        let surface = src.entity(surface_id, "SURFACE")?;
        let surface_type = surface.inst.keyword.clone();
        let flags = surface_flags(surface)?;
        let mut bound_reports = Vec::new();
        for bound_id in refs(f, "bounds", MAX_BOUNDS)? {
            if !bounds_seen.insert(bound_id) {
                return Err("duplicate_bound_use_across_shell".into());
            }
            let bound = src.entity(bound_id, "FACE_BOUND")?;
            let role = match bound.inst.keyword.as_str() {
                "FACE_OUTER_BOUND" => "outer",
                "FACE_BOUND" => "inner_or_unspecified",
                _ => return Err("unsupported_complex_face_bound".into()),
            };
            let bound_sense = logical(bound, "orientation")?;
            let loop_id = bound.reference("bound").ok_or("missing_shared_edge_loop")?;
            if !loops_seen.insert(loop_id) {
                return Err("duplicate_loop_use_across_shell".into());
            }
            let lp = src.entity(loop_id, "EDGE_LOOP")?;
            let oriented_ids = refs(lp, "edge_list", MAX_LOOP_USES)?;
            bound_reports.push(
                json!({"boundId":bound_id,"boundRole":role,"boundOrientation":bound_sense,
                "loopId":loop_id,"orientedEdgeUseCount":oriented_ids.len()}),
            );
            for oriented_id in oriented_ids {
                if !uses_seen.insert(oriented_id) {
                    return Err("duplicate_oriented_edge_use_across_shell".into());
                }
                let oriented = src.entity(oriented_id, "ORIENTED_EDGE")?;
                if !matches!(oriented.attr("edge_start"), Some(StepValue::Derived))
                    || !matches!(oriented.attr("edge_end"), Some(StepValue::Derived))
                {
                    return Err("oriented_edge_endpoints_must_be_derived".into());
                }
                let orientation = logical(oriented, "orientation")?;
                let edge_id = oriented
                    .reference("edge_element")
                    .ok_or("missing_shared_edge_element")?;
                let edge = src.entity(edge_id, "EDGE_CURVE")?;
                let start = edge
                    .reference("edge_start")
                    .ok_or("missing_shared_edge_start")?;
                let end = edge
                    .reference("edge_end")
                    .ok_or("missing_shared_edge_end")?;
                let curve = edge
                    .reference("edge_geometry")
                    .ok_or("missing_shared_edge_geometry")?;
                let edge_sense = logical(edge, "same_sense")?;
                src.vertex(start)?;
                src.vertex(end)?;
                src.entity(curve, "CURVE")?;
                if face_id == face {
                    selected.insert(edge_id);
                }
                let entry = edges.entry(edge_id).or_insert_with(|| Edge {
                    curve,
                    start,
                    end,
                    sense: edge_sense,
                    uses: Vec::new(),
                });
                if (entry.curve, entry.start, entry.end, entry.sense)
                    != (curve, start, end, edge_sense)
                {
                    return Err("inconsistent_shared_edge_identity".into());
                }
                entry.uses.push(json!({"faceId":face_id,"surfaceId":surface_id,"surfaceType":surface_type,
                    "faceSameSense":sense,"boundId":bound_id,"boundRole":role,"boundOrientation":bound_sense,
                    "loopId":loop_id,"orientedEdgeId":oriented_id,"orientation":orientation}));
            }
        }
        face_reports.push(
            json!({"faceId":face_id,"surfaceId":surface_id,"surfaceType":surface_type,
            "faceSameSense":sense,"sourceSurfaceFlags":flags,"bounds":bound_reports}),
        );
    }
    let selected_edges:Vec<_>=selected.into_iter().map(|id| {
        let e=&edges[&id];
        let opposite=e.uses.len()==2 && e.uses[0]["orientation"]!=e.uses[1]["orientation"];
        json!({"edgeCurveId":id,"curveGeometryId":e.curve,"startVertexId":e.start,"endVertexId":e.end,
            "edgeSameSense":e.sense,"useCount":e.uses.len(),"exactlyTwoUses":e.uses.len()==2,
            "twoUsesHaveOppositeOrientation":opposite,"uses":e.uses,
            "edgeCurveTolerance":{"provided":false,"reason":"EDGE_CURVE_schema_has_no_tolerance_attribute"}})
    }).collect();
    let mut inventory = BTreeMap::new();
    for kind in ["PCURVE", "SURFACE_CURVE", "SEAM_CURVE"] {
        let mut ids = Vec::new();
        let mut count = 0;
        for (&id, inst) in &step.instances {
            if (Entity { inst }).is_a(kind) {
                crate::unit_preflight::shape(inst)?;
                count += 1;
                if ids.len() < 32 {
                    ids.push(id);
                }
            }
        }
        inventory.insert(
            kind,
            json!({"count":count,"sampleEntityIds":ids,"sampleTruncated":count>ids.len(),
            "scope":"global type inventory only; no edge association or geometry inferred"}),
        );
    }
    Ok(
        json!({"status":"source_topology_enumerated","selectedFaceId":face,"representationId":representation,
        "ownerId":owner,"shellId":shell_id,"faceCount":faces.len(),"sourceEdgeCurveCount":edges.len(),
        "orientedEdgeUseCount":uses_seen.len(),"selectedEdges":selected_edges,"faces":face_reports,
        "ownership":ownership,"curveTypeInventory":inventory,"inventoryEntityCount":step.instances.len(),
        "workSteps":src.work,"maxWorkSteps":MAX_WORK,"maxEntities":MAX_ENTITIES,
        "maxFaces":MAX_FACES,"maxBoundsPerFace":MAX_BOUNDS,"maxUsesPerLoop":MAX_LOOP_USES,
        "contextUncertainty":"separate context reader; not inferred as per-edge tolerance",
        "geometryQualified":false,"closedSolidVerified":false,"admitted":false,"exportsProduced":[],
        "scope":"source identity and oriented uses only; no trim, position, continuity, manifold or solid qualification"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(extra: &str) -> StepFile {
        oxideav_ifc::parse_step(format!("ISO-10303-21;HEADER;FILE_DESCRIPTION((''),'2;1');FILE_NAME('','',(''),(''),'','','');FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;
        #1=MANIFOLD_SOLID_BREP('',#2);#2=CLOSED_SHELL('',(#3,#4));
        #3=ADVANCED_FACE('',(#5),#30,.T.);#4=ADVANCED_FACE('',(#6),#30,.F.);
        #5=FACE_OUTER_BOUND('',#7,.T.);#6=FACE_OUTER_BOUND('',#8,.T.);
        #7=EDGE_LOOP('',(#11,#12));#8=EDGE_LOOP('',(#13,#14));
        #11=ORIENTED_EDGE('',*,*,#20,.T.);#12=ORIENTED_EDGE('',*,*,#21,.T.);
        #13=ORIENTED_EDGE('',*,*,#20,.F.);#14=ORIENTED_EDGE('',*,*,#21,.F.);
        #20=EDGE_CURVE('',#22,#23,#31,.T.);#21=EDGE_CURVE('',#23,#22,#31,.F.);
        #22=VERTEX_POINT('',#24);#23=VERTEX_POINT('',#25);
        #24=CARTESIAN_POINT('',(0.,0.,0.));#25=CARTESIAN_POINT('',(1.,0.,0.));
        #30=PLANE('',#32);#31=LINE('',#24,#33);#32=AXIS2_PLACEMENT_3D('',#24,$,$);
        #33=VECTOR('',#34,1.);#34=DIRECTION('',(1.,0.,0.));
        #40=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#1),#41);
        #41=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT((#42)) REPRESENTATION_CONTEXT('',''));
        #42=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));
        {extra} ENDSEC;END-ISO-10303-21;").as_bytes()).unwrap()
    }
    fn modified(from: &str, to: &str) -> StepFile {
        let mut s = fixture("");
        // Parser writer is unnecessary: mutate only the bounded analytic fixture AST.
        let parsed=oxideav_ifc::parse_step(format!("ISO-10303-21;HEADER;FILE_DESCRIPTION((''),'2;1');FILE_NAME('','',(''),(''),'','','');FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;{to}ENDSEC;END-ISO-10303-21;").as_bytes()).unwrap();
        let id = from.parse::<u64>().unwrap();
        s.instances.insert(id, parsed.get(id).unwrap().clone());
        s
    }
    #[test]
    fn preserves_two_different_edges_to_same_adjacent_face_and_reversed_uses() {
        let r = inspect(&fixture(""), 40, 1, 3).unwrap();
        let edges = r["selectedEdges"].as_array().unwrap();
        assert_eq!(edges.len(), 2);
        for e in edges {
            assert_eq!(e["useCount"], 2);
            assert_eq!(e["twoUsesHaveOppositeOrientation"], true);
            assert_eq!(e["uses"][1]["faceId"], 4);
            assert_eq!(e["edgeCurveTolerance"]["provided"], false);
        }
        assert_ne!(edges[0]["edgeCurveId"], edges[1]["edgeCurveId"]);
        assert_eq!(r["geometryQualified"], false);
        assert_eq!(r["admitted"], false);
    }
    #[test]
    fn retains_inner_bound_and_source_logical_unknown() {
        let mut s = modified("3", "#3=ADVANCED_FACE('',(#5,#9),#35,.T.);");
        let extra=fixture("#9=FACE_BOUND('',#10,.F.);#10=EDGE_LOOP('',(#15));#15=ORIENTED_EDGE('',*,*,#21,.F.);#35=B_SPLINE_SURFACE_WITH_KNOTS('',1,1,((#24,#25),(#24,#25)),.UNSPECIFIED.,.F.,.U.,.T.,(2,2),(2,2),(0.,1.),(0.,1.),.UNSPECIFIED.);");
        for id in [9, 10, 15, 35] {
            s.instances.insert(id, extra.get(id).unwrap().clone());
        }
        let r = inspect(&s, 40, 1, 3).unwrap();
        assert_eq!(r["faces"][0]["bounds"].as_array().unwrap().len(), 2);
        assert_eq!(
            r["faces"][0]["bounds"][1]["boundRole"],
            "inner_or_unspecified"
        );
        assert_eq!(
            r["faces"][0]["sourceSurfaceFlags"]["self_intersect"]["value"],
            true
        );
        assert_eq!(
            r["faces"][0]["sourceSurfaceFlags"]["v_closed"]["sourceLogical"],
            "U"
        );
        assert!(r["faces"][0]["sourceSurfaceFlags"]["v_closed"]["value"].is_null());
        assert_eq!(r["selectedEdges"][1]["useCount"], 3);
    }
    #[test]
    fn duplicate_dangling_unsupported_and_wrong_ownership_refuse() {
        for (id, record) in [
            ("2", "#2=CLOSED_SHELL('',(#3,#3));"),
            ("7", "#7=EDGE_LOOP('',(#11,#11));"),
            ("11", "#11=ORIENTED_EDGE('',*,*,#99,.T.);"),
            ("2", "#2=OPEN_SHELL('',(#3,#4));"),
            ("7", "#7=VERTEX_LOOP('',#22);"),
            ("11", "#11=ORIENTED_EDGE('',*,*,#11,.T.);"),
        ] {
            assert!(
                inspect(&modified(id, record), 40, 1, 3).is_err(),
                "{record}"
            );
        }
        assert!(inspect(&fixture(""), 40, 999, 3).is_err());
        assert!(inspect(&fixture(""), 40, 1, 999).is_err());
        assert!(inspect(&fixture("#50=MANIFOLD_SOLID_BREP('',#2);#51=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#1,#50),#41);"),51,1,3).is_err());
    }
    #[test]
    fn global_type_inventory_is_not_an_edge_association() {
        let s=fixture("#70=PCURVE('',#30,#71);#71=DEFINITIONAL_REPRESENTATION('',(#31),#41);#72=SURFACE_CURVE('',#31,(#70),.PCURVE_S1.);#73=SEAM_CURVE('',#31,(#70),.PCURVE_S1.);");
        let r = inspect(&s, 40, 1, 3).unwrap();
        assert_eq!(r["curveTypeInventory"]["PCURVE"]["count"], 1);
        assert_eq!(r["curveTypeInventory"]["SURFACE_CURVE"]["count"], 2);
        assert_eq!(r["curveTypeInventory"]["SEAM_CURVE"]["count"], 1);
    }
}
