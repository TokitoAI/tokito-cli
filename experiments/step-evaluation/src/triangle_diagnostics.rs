//! Bounded reader-mesh observations. No mutation, repair or UV inference.
use oxideav_step::StepModel;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
const MAX_TRIANGLES: usize = 2_000_000;
const MAX_VERTICES: usize = 1_000_000;
const MAX_DETAILS: usize = 128;
type P = [f64; 3];

fn classify(points: [P; 3], indices: [u32; 3]) -> Result<Option<&'static str>, String> {
    if points
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || v.abs() > 1e12)
    {
        return Err("diagnostic_nonfinite_or_unbounded_position".into());
    }
    let sub = |a: P, b: P| std::array::from_fn::<_, 3, _>(|i| a[i] - b[i]);
    let dot = |a: P, b: P| a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>();
    let mut ab = sub(points[1], points[0]);
    let mut ac = sub(points[2], points[0]);
    let mut bc = sub(points[2], points[1]);
    // Uniform normalization avoids underflow in cross-squared / edge-fourth.
    // Exact collinearity below still uses the original binary64 coordinates.
    let scale = ab
        .iter()
        .chain(&ac)
        .chain(&bc)
        .map(|v| v.abs())
        .fold(0., f64::max);
    if scale > 0. {
        for vector in [&mut ab, &mut ac, &mut bc] {
            for coordinate in vector {
                *coordinate /= scale;
            }
        }
    }
    let normal = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let edge = dot(ab, ab).max(dot(ac, ac)).max(dot(bc, bc));
    if dot(normal, normal) > edge * edge * 1e-24 {
        return Ok(None);
    }
    if indices[0] == indices[1] || indices[1] == indices[2] || indices[0] == indices[2] {
        return Ok(Some("repeated_mesh_index"));
    }
    if points[0] == points[1] || points[1] == points[2] || points[0] == points[2] {
        return Ok(Some("coincident_positions_distinct_indices"));
    }
    for (x, y) in [(0, 1), (1, 2), (2, 0)] {
        if !crate::exact_predicate::collinear(
            [points[0][x], points[0][y]],
            [points[1][x], points[1][y]],
            [points[2][x], points[2][y]],
        )? {
            return Ok(Some("relative_near_zero_exactly_noncollinear"));
        }
    }
    Ok(Some("exactly_collinear_distinct_positions"))
}

pub fn inspect(model: &StepModel, face: u64) -> Result<Value, String> {
    if face == 0 {
        return Err("positive_diagnostic_face_required".into());
    }
    let total = model.parts.iter().flat_map(|p| &p.shapes).try_fold(
        (0usize, 0usize),
        |(triangles, vertices), s| {
            Ok::<_, String>((
                triangles
                    .checked_add(s.mesh.triangles.len())
                    .filter(|n| *n <= MAX_TRIANGLES)
                    .ok_or("diagnostic_triangle_budget")?,
                vertices
                    .checked_add(s.mesh.positions.len())
                    .filter(|n| *n <= MAX_VERTICES)
                    .ok_or("diagnostic_vertex_budget")?,
            ))
        },
    )?;
    let mut count = 0;
    let mut counts = BTreeMap::new();
    let mut details = Vec::new();
    let mut vertices = BTreeSet::new();
    let mut affected = BTreeSet::new();
    let mut selected_shapes = BTreeSet::new();
    for (part_index, part) in model.parts.iter().enumerate() {
        for (shape_index, shape) in part.shapes.iter().enumerate() {
            if shape.triangle_faces.len() != shape.mesh.triangles.len() {
                return Err("diagnostic_face_metadata_mismatch".into());
            }
            for (ordinal, id) in shape.triangle_faces.iter().enumerate() {
                if *id != face {
                    continue;
                }
                selected_shapes.insert((part_index, shape_index));
                count += 1;
                let triangle = shape.mesh.triangles[ordinal];
                let mut points = [[0.; 3]; 3];
                for i in 0..3 {
                    points[i] = *shape
                        .mesh
                        .positions
                        .get(triangle[i] as usize)
                        .ok_or("diagnostic_invalid_triangle_index")?;
                    vertices.insert((part_index, shape_index, triangle[i]));
                }
                if let Some(kind) = classify(points, triangle)? {
                    *counts.entry(kind).or_insert(0usize) += 1;
                    for index in triangle {
                        affected.insert((part_index, shape_index, index));
                    }
                    if details.len() < MAX_DETAILS {
                        details.push(json!({"partIndex":part_index,"shapeIndex":shape_index,"itemId":shape.item,"primaryRepresentationId":part.representation,"triangleOrdinal":ordinal,"meshIndices":triangle,"positionsPartLocalModelUnits":points,"classification":kind}));
                    }
                }
            }
        }
    }
    if count == 0 {
        return Err("diagnostic_face_not_emitted".into());
    }
    let bad = counts.values().sum::<usize>();
    Ok(
        json!({"schema":"tokito.step-face-triangle-diagnostics.v1","faceId":face,"selectedFaceTriangles":count,"selectedShapeCount":selected_shapes.len(),"distinctSelectedMeshVertices":vertices.len(),"affectedMeshVertices":affected.len(),"degenerateOrNearZeroTriangles":bad,"classificationCounts":counts,"details":details,"detailsTruncated":bad>MAX_DETAILS,"maxDetails":MAX_DETAILS,"relativeSquaredAreaThreshold":1e-24,"coordinateFrame":"part_local_reader_model_units","readerReportedMetresPerModelUnit":model.length_unit_metres,"modelTrianglesInspected":total.0,"modelVerticesBudgeted":total.1,"uvCoordinatesAvailable":false,"surfaceCauseEstablished":false,"sourceSurfaceEvaluationVerified":false,"geometryQualified":false,"admitted":false,"exportsProduced":[],"scope":"observations of emitted triangles only; no deletion, repair, world-transform application, UV inversion or causal conclusion"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> StepModel {
        let text = crate::planar_face::tests::source_text(true, true, true, 1.).replace("ENDSEC;END-ISO-10303-21;", "#80=MANIFOLD_SOLID_BREP('',#81);#81=CLOSED_SHELL('',(#75));#82=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#80),#83);#83=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT((#84)) REPRESENTATION_CONTEXT('',''));#84=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));ENDSEC;END-ISO-10303-21;");
        let source = oxideav_ifc::parse_step(text.as_bytes()).unwrap();
        let mut model = oxideav_step::model_from_file(&source, &Default::default()).unwrap();
        let shape = &mut model.parts[0].shapes[0];
        shape.mesh.positions = vec![[0.; 3], [1., 0., 0.], [0., 1., 0.]];
        shape.mesh.triangles = vec![[0, 1, 2]];
        shape.triangle_faces = vec![75];
        model
    }
    #[test]
    fn bounded_details_preserve_input_and_keep_admission_closed() {
        let mut model = fixture();
        let shape = &mut model.parts[0].shapes[0];
        shape.mesh.triangles = vec![[0, 0, 1]; MAX_DETAILS + 1];
        shape.triangle_faces = vec![75; MAX_DETAILS + 1];
        let before = format!("{model:?}");
        let report = inspect(&model, 75).unwrap();
        assert_eq!(report["selectedFaceTriangles"], MAX_DETAILS + 1);
        assert_eq!(report["details"].as_array().unwrap().len(), MAX_DETAILS);
        assert_eq!(report["detailsTruncated"], true);
        assert_eq!(
            report["classificationCounts"]["repeated_mesh_index"],
            MAX_DETAILS + 1
        );
        assert_eq!(report["admitted"], false);
        assert_eq!(report["geometryQualified"], false);
        assert_eq!(report["exportsProduced"], json!([]));
        assert_eq!(format!("{model:?}"), before);
    }
    #[test]
    fn malformed_identity_indices_and_resource_excess_are_refused() {
        let model = fixture();
        assert_eq!(
            inspect(&model, 0).unwrap_err(),
            "positive_diagnostic_face_required"
        );
        assert_eq!(
            inspect(&model, 999).unwrap_err(),
            "diagnostic_face_not_emitted"
        );
        let mut malformed = model.clone();
        malformed.parts[0].shapes[0].triangle_faces.clear();
        assert_eq!(
            inspect(&malformed, 75).unwrap_err(),
            "diagnostic_face_metadata_mismatch"
        );
        let mut invalid_index = model.clone();
        invalid_index.parts[0].shapes[0].mesh.triangles[0][2] = u32::MAX;
        assert_eq!(
            inspect(&invalid_index, 75).unwrap_err(),
            "diagnostic_invalid_triangle_index"
        );
        let mut excessive = model.clone();
        excessive.parts[0].shapes[0]
            .mesh
            .positions
            .resize(MAX_VERTICES + 1, [0.; 3]);
        assert_eq!(
            inspect(&excessive, 75).unwrap_err(),
            "diagnostic_vertex_budget"
        );
        excessive.parts[0].shapes[0].mesh.positions.clear();
        excessive.parts[0].shapes[0]
            .mesh
            .triangles
            .resize(MAX_TRIANGLES + 1, [0; 3]);
        assert_eq!(
            inspect(&excessive, 75).unwrap_err(),
            "diagnostic_triangle_budget"
        );
    }
    #[test]
    fn classifications_are_explicit_and_exact() {
        assert_eq!(
            classify([[0.; 3], [1., 0., 0.], [0., 1., 0.]], [0, 1, 2]).unwrap(),
            None
        );
        assert_eq!(
            classify([[0.; 3]; 3], [0, 0, 1]).unwrap(),
            Some("repeated_mesh_index")
        );
        assert_eq!(
            classify([[0.; 3]; 3], [0, 1, 2]).unwrap(),
            Some("coincident_positions_distinct_indices")
        );
        assert_eq!(
            classify([[0.; 3], [1., 2., 3.], [2., 4., 6.]], [0, 1, 2]).unwrap(),
            Some("exactly_collinear_distinct_positions")
        );
        assert_eq!(
            classify([[0.; 3], [1., 0., 0.], [2., 1e-13, 0.]], [0, 1, 2]).unwrap(),
            Some("relative_near_zero_exactly_noncollinear")
        );
        assert!(classify([[f64::NAN; 3]; 3], [0, 1, 2]).is_err());
        assert!(classify([[1e13; 3]; 3], [0, 1, 2]).is_err());
    }
    #[test]
    fn relative_criterion_is_scale_invariant() {
        for scale in [1e-100, 1., 1e10] {
            assert_eq!(
                classify([[0.; 3], [scale, 0., 0.], [0., scale, 0.]], [0, 1, 2]).unwrap(),
                None
            );
            assert_eq!(
                classify(
                    [[0.; 3], [scale, 0., 0.], [2. * scale, 1e-13 * scale, 0.]],
                    [0, 1, 2]
                )
                .unwrap(),
                Some("relative_near_zero_exactly_noncollinear")
            );
        }
    }
}
