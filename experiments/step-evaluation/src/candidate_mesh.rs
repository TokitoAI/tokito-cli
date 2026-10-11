//! Reader-derived STEP occurrence triangles, positioned in mm without admission.
use oxideav_step::{Occurrence, StepModel, Transform};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const MAX_DEPTH: usize = 64;
const MAX_OCCURRENCES: usize = 100_000;
const MAX_VERTICES: usize = 1_000_000;
const MAX_TRIANGLES: usize = 2_000_000;
const MAX_SHAPES: usize = 100_000;
const MAX_METADATA: usize = 100_000;
const MAX_TEXT_BYTES: usize = 1_000_000;

#[derive(Default)]
struct State {
    occurrences: usize,
    shapes: usize,
    vertices: usize,
    triangles: usize,
    face_ids: usize,
    triangle_colors: usize,
    metadata: usize,
    text_bytes: usize,
    output: Vec<Value>,
}
fn charge(counter: &mut usize, amount: usize, max: usize, error: &str) -> Result<(), String> {
    *counter = counter
        .checked_add(amount)
        .filter(|n| *n <= max)
        .ok_or_else(|| error.to_string())?;
    Ok(())
}
impl State {
    fn text(&mut self, text: &str) -> Result<(), String> {
        charge(
            &mut self.text_bytes,
            text.len(),
            MAX_TEXT_BYTES,
            "candidate_metadata_text_limit",
        )
    }
    fn optional_text(&mut self, text: &Option<String>) -> Result<(), String> {
        if let Some(text) = text {
            self.text(text)?;
        }
        Ok(())
    }
}
fn finite_transform(t: &Transform) -> bool {
    t.cols
        .iter()
        .flatten()
        .chain(t.translation.iter())
        .all(|v| v.is_finite())
}
fn color(c: &[f32; 4]) -> Result<(), String> {
    if c.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) {
        return Err("candidate_color_nonfinite_or_out_of_range".into());
    }
    Ok(())
}

pub fn inspect(model: &StepModel, scale_to_mm: f64) -> Result<Value, String> {
    if !scale_to_mm.is_finite() || scale_to_mm <= 0.0 {
        return Err("candidate_invalid_unit_scale".into());
    }
    if model.roots.is_empty() {
        return Err("candidate_missing_roots".into());
    }
    let mut state = State::default();
    charge(
        &mut state.metadata,
        model.warnings.len(),
        MAX_METADATA,
        "candidate_metadata_count_limit",
    )?;
    for warning in &model.warnings {
        state.text(warning)?;
    }
    let mut overrides = Vec::new();
    for style in &model.occurrence_colours {
        charge(
            &mut state.metadata,
            1,
            MAX_METADATA,
            "candidate_metadata_count_limit",
        )?;
        if style.path.len() > MAX_DEPTH + 1 {
            return Err("candidate_style_path_depth_limit".into());
        }
        color(&style.colour)?;
        overrides.push(json!({"sourceOccurrencePath":style.path,"sourceItemId":style.item,"rgba":style.colour}));
    }
    for (index, root) in model.roots.iter().enumerate() {
        visit(
            model,
            root,
            &Transform::IDENTITY,
            0,
            &mut vec![index],
            &mut BTreeSet::new(),
            scale_to_mm,
            &mut state,
        )?;
    }
    if state.vertices == 0 {
        return Err("candidate_no_positioned_vertices".into());
    }
    Ok(
        json!({"schema":"tokito.step-positioned-candidate-mesh.v1","lengthUnit":"mm",
        "lengthScaleModelUnitsToMm":scale_to_mm,"occurrences":state.output,
        "occurrenceCount":state.occurrences,"shapeOccurrenceCount":state.shapes,
        "positionedVertexCount":state.vertices,"positionedTriangleCount":state.triangles,
        "readerWarnings":model.warnings,"readerWarningsPresent":!model.warnings.is_empty(),
        "sourceOccurrenceColorOverrides":overrides,"occurrenceColorApplicationResolved":false,
        "materialPhysicalPropertiesVerified":false,"normalsGenerated":false,
        "surfaceCompletenessVerified":false,"geometryQualified":false,"pcbPlacementQualified":false,
        "limits":{"depth":MAX_DEPTH,"occurrences":MAX_OCCURRENCES,"vertices":MAX_VERTICES,
            "triangles":MAX_TRIANGLES,"shapeOccurrences":MAX_SHAPES,"metadataBytes":MAX_TEXT_BYTES}}),
    )
}

#[allow(clippy::too_many_arguments)]
fn visit(
    model: &StepModel,
    occurrence: &Occurrence,
    parent: &Transform,
    depth: usize,
    path: &mut Vec<usize>,
    active: &mut BTreeSet<usize>,
    scale: f64,
    state: &mut State,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("candidate_depth_limit".into());
    }
    charge(
        &mut state.occurrences,
        1,
        MAX_OCCURRENCES,
        "candidate_occurrence_limit",
    )?;
    let part = model
        .parts
        .get(occurrence.part)
        .ok_or("candidate_part_index_out_of_range")?;
    if !active.insert(occurrence.part) {
        return Err("candidate_part_cycle".into());
    }
    if !finite_transform(&occurrence.transform) {
        return Err("candidate_nonfinite_transform".into());
    }
    let world = parent.compose(&occurrence.transform);
    if !finite_transform(&world) {
        return Err("candidate_nonfinite_composed_transform".into());
    }
    state.optional_text(&occurrence.name)?;
    state.optional_text(&part.name)?;
    state.optional_text(&part.product_id)?;
    let mut shapes = Vec::new();
    for (shape_index, shape) in part.shapes.iter().enumerate() {
        charge(
            &mut state.shapes,
            1,
            MAX_SHAPES,
            "candidate_shape_occurrence_limit",
        )?;
        charge(
            &mut state.vertices,
            shape.mesh.positions.len(),
            MAX_VERTICES,
            "candidate_vertex_limit",
        )?;
        charge(
            &mut state.triangles,
            shape.mesh.triangles.len(),
            MAX_TRIANGLES,
            "candidate_triangle_limit",
        )?;
        charge(
            &mut state.face_ids,
            shape.triangle_faces.len(),
            MAX_TRIANGLES,
            "candidate_face_provenance_limit",
        )?;
        charge(
            &mut state.triangle_colors,
            shape.triangle_colours.len(),
            MAX_TRIANGLES,
            "candidate_color_provenance_limit",
        )?;
        // Provenance arrays also charge aggregate work, including malformed extras.
        charge(
            &mut state.metadata,
            shape.layers.len(),
            MAX_METADATA,
            "candidate_metadata_count_limit",
        )?;
        state.optional_text(&shape.name)?;
        for layer in &shape.layers {
            state.text(layer)?;
        }
        if let Some(c) = &shape.colour {
            color(c)?;
        }
        for c in shape.triangle_colours.iter().flatten() {
            color(c)?;
        }
        for triangle in &shape.mesh.triangles {
            if triangle
                .iter()
                .any(|i| *i as usize >= shape.mesh.positions.len())
            {
                return Err("candidate_triangle_index_out_of_range".into());
            }
        }
        let mut vertices = Vec::with_capacity(shape.mesh.positions.len());
        for vertex in &shape.mesh.positions {
            if vertex.iter().any(|v| !v.is_finite()) {
                return Err("candidate_nonfinite_source_vertex".into());
            }
            let vertex = world.apply(*vertex).map(|v| v * scale);
            if vertex.iter().any(|v| !v.is_finite()) {
                return Err("candidate_nonfinite_positioned_vertex".into());
            }
            vertices.push(vertex);
        }
        let faces: BTreeSet<_> = shape.triangle_faces.iter().copied().collect();
        let triangles = shape.mesh.triangles.len();
        shapes.push(json!({"shapeIndex":shape_index,"sourceItemId":shape.item,"name":shape.name,
            "verticesMm":vertices,"triangles":shape.mesh.triangles,"sourceTriangleFaceIds":shape.triangle_faces,
            "distinctReportedFaceCount":faces.len(),"sourceItemRgba":shape.colour,
            "sourceTriangleRgba":shape.triangle_colours,"sourceLayers":shape.layers,
            "triangleFaceProvenanceCount":shape.triangle_faces.len(),
            "triangleFaceProvenanceAligned":shape.triangle_faces.len()==triangles,
            "triangleColorProvenanceAligned":shape.triangle_colours.is_empty() || shape.triangle_colours.len()==triangles,
            "diagnostics":{
                "faceProvenanceCountMismatch":shape.triangle_faces.len()!=triangles,
                "colorProvenanceCountMismatch":!shape.triangle_colours.is_empty() && shape.triangle_colours.len()!=triangles,
                "emptyTriangleGeometry":triangles==0},"geometryQualified":false}));
    }
    state.output.push(json!({"occurrencePath":path,"sourceOccurrenceId":occurrence.id,
        "sourcePlacedById":occurrence.placed_by,"name":occurrence.name,"partIndex":occurrence.part,
        "partName":part.name,"sourceProductId":part.product_id,"sourcePartDefinitionId":part.definition,
        "sourceRepresentationId":part.representation,"shapes":shapes}));
    for (index, child) in part.children.iter().enumerate() {
        path.push(index);
        visit(model, child, &world, depth + 1, path, active, scale, state)?;
        path.pop();
    }
    active.remove(&occurrence.part);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideav_step::{Part, Shape, TriMesh};
    fn occ(part: usize, transform: Transform) -> Occurrence {
        Occurrence {
            part,
            transform,
            id: Some(7),
            placed_by: Some(8),
            name: None,
        }
    }
    fn part(children: Vec<Occurrence>) -> Part {
        Part {
            children,
            shapes: vec![Shape {
                item: 42,
                name: None,
                mesh: TriMesh {
                    positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                    triangles: vec![[0, 1, 2]],
                },
                colour: Some([1.0, 0.0, 0.0, 1.0]),
                triangle_colours: vec![],
                triangle_faces: vec![77],
                layers: vec![],
            }],
            ..Part::default()
        }
    }
    fn model(parts: Vec<Part>, roots: Vec<Occurrence>) -> StepModel {
        StepModel {
            schema: oxideav_step::ApSchema::Ap214,
            header: Default::default(),
            length_unit_metres: 0.001,
            parts,
            roots,
            occurrence_colours: vec![],
            warnings: vec!["observed reader omission".into()],
        }
    }
    #[test]
    fn nested_nonidentity_rotations_repeats_and_one_scale_preserve_faces_and_warnings() {
        let rotation = Transform {
            cols: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [10.0, 0.0, 0.0],
        };
        let child = Transform {
            translation: [2.0, 0.0, 0.0],
            ..rotation
        };
        let m = model(
            vec![
                Part {
                    children: vec![occ(1, child), occ(1, child)],
                    ..Part::default()
                },
                part(vec![]),
            ],
            vec![occ(0, rotation)],
        );
        let r = inspect(&m, 2.0).unwrap();
        assert_eq!(r["occurrenceCount"], 3);
        assert_eq!(r["positionedTriangleCount"], 2);
        assert_eq!(
            r["occurrences"][1]["shapes"][0]["verticesMm"],
            json!([[20.0, 4.0, 0.0], [18.0, 4.0, 0.0], [20.0, 2.0, 0.0]])
        );
        assert_eq!(
            r["occurrences"][2]["shapes"][0]["sourceTriangleFaceIds"],
            json!([77])
        );
        assert_eq!(r["readerWarnings"], json!(["observed reader omission"]));
    }
    #[test]
    fn invalid_indices_cycles_and_nonfinite_inputs_refuse() {
        let mut p = part(vec![]);
        p.shapes[0].mesh.triangles[0] = [0, 1, 3];
        assert!(
            inspect(&model(vec![p], vec![occ(0, Transform::IDENTITY)]), 1.0)
                .unwrap_err()
                .contains("triangle_index")
        );
        assert!(inspect(
            &model(
                vec![part(vec![occ(0, Transform::IDENTITY)])],
                vec![occ(0, Transform::IDENTITY)]
            ),
            1.0
        )
        .unwrap_err()
        .contains("cycle"));
        let m = model(vec![part(vec![])], vec![occ(0, Transform::IDENTITY)]);
        assert!(inspect(&m, f64::INFINITY).is_err());
        let mut m = m;
        m.parts[0].shapes[0].mesh.positions[0][0] = f64::NAN;
        assert!(inspect(&m, 1.0).is_err());
    }
    #[test]
    fn budgets_and_provenance_mismatches_are_explicit() {
        let mut state = State {
            vertices: MAX_VERTICES,
            ..State::default()
        };
        assert!(charge(&mut state.vertices, 1, MAX_VERTICES, "cap").is_err());
        let mut p = part(vec![]);
        p.shapes[0].triangle_faces.clear();
        let r = inspect(&model(vec![p], vec![occ(0, Transform::IDENTITY)]), 1.0).unwrap();
        assert_eq!(
            r["occurrences"][0]["shapes"][0]["diagnostics"]["faceProvenanceCountMismatch"],
            true
        );
        let m = model(vec![part(vec![])], vec![occ(0, Transform::IDENTITY)]);
        let mut state = State {
            occurrences: MAX_OCCURRENCES,
            ..State::default()
        };
        assert!(visit(
            &m,
            &m.roots[0],
            &Transform::IDENTITY,
            0,
            &mut vec![0],
            &mut BTreeSet::new(),
            1.0,
            &mut state
        )
        .is_err());
        assert!(visit(
            &m,
            &m.roots[0],
            &Transform::IDENTITY,
            MAX_DEPTH + 1,
            &mut vec![0],
            &mut BTreeSet::new(),
            1.0,
            &mut State::default()
        )
        .is_err());
    }
    #[test]
    fn traversal_checks_vertex_triangle_and_metadata_aggregate_limits() {
        let m = model(vec![part(vec![])], vec![occ(0, Transform::IDENTITY)]);
        for mut state in [
            State {
                vertices: MAX_VERTICES,
                ..State::default()
            },
            State {
                triangles: MAX_TRIANGLES,
                ..State::default()
            },
            State {
                face_ids: MAX_TRIANGLES,
                ..State::default()
            },
        ] {
            assert!(visit(
                &m,
                &m.roots[0],
                &Transform::IDENTITY,
                0,
                &mut vec![0],
                &mut BTreeSet::new(),
                1.0,
                &mut state
            )
            .is_err());
        }
        let huge = Transform {
            translation: [f64::MAX, 0.0, 0.0],
            ..Transform::IDENTITY
        };
        assert!(inspect(&model(vec![part(vec![])], vec![occ(0, huge)]), 2.0)
            .unwrap_err()
            .contains("positioned_vertex"));
        let mut m = m;
        m.roots[0].transform.cols[0][0] = f64::INFINITY;
        assert!(inspect(&m, 1.0)
            .unwrap_err()
            .contains("nonfinite_transform"));
    }
}
