//! Bounded world-space traversal for STEP product-structure occurrences.
//!
//! This audit composes STEP occurrence transforms and converts model units to
//! millimetres once. It reports reader-derived placement observations only;
//! it does not qualify STEP surface completeness or placement in an Altium PCB.

use oxideav_step::{Occurrence, StepModel, Transform};
use serde_json::{json, Value};
use std::collections::HashSet;

const MAX_OCCURRENCES: usize = 100_000;
const MAX_DEPTH: usize = 64;
const MAX_TRIANGLES: usize = 5_000_000;
const MAX_WORLD_VERTICES: usize = 1_000_000;
const MAX_DETAILS: usize = 512;

#[derive(Debug)]
struct AuditState {
    scale_to_mm: f64,
    occurrences: usize,
    triangles: usize,
    vertices: usize,
    referenced_parts: HashSet<usize>,
    minimum: [f64; 3],
    maximum: [f64; 3],
    details: Vec<Value>,
}

impl AuditState {
    fn new(scale_to_mm: f64) -> Self {
        Self {
            scale_to_mm,
            occurrences: 0,
            triangles: 0,
            vertices: 0,
            referenced_parts: HashSet::new(),
            minimum: [f64::INFINITY; 3],
            maximum: [f64::NEG_INFINITY; 3],
            details: Vec::new(),
        }
    }
}

/// Compose each product occurrence into world space and return bounded bounds,
/// occurrence, and triangle observations. `scale_to_mm` converts one model
/// length unit to millimetres and is applied after transform composition.
pub fn audit(model: &StepModel, scale_to_mm: f64) -> Result<Value, String> {
    if !scale_to_mm.is_finite() || scale_to_mm <= 0.0 {
        return Err("invalid_model_unit_scale".into());
    }
    if model.roots.is_empty() {
        return Err("step_model_has_no_root_occurrences".into());
    }

    let mut state = AuditState::new(scale_to_mm);
    let mut path = Vec::new();
    let mut active_parts = Vec::new();
    for (root_index, root) in model.roots.iter().enumerate() {
        path.push(root_index);
        visit(
            model,
            root,
            &Transform::IDENTITY,
            0,
            &mut path,
            &mut active_parts,
            &mut state,
        )?;
        path.pop();
    }
    if state.vertices == 0 {
        return Err("step_occurrences_contain_no_mesh_vertices".into());
    }

    let world_bounds_mm = json!({"min":state.minimum,"max":state.maximum});
    Ok(json!({
        "schema":"tokito.step-occurrence-world.v1",
        "scope":"STEP occurrence transforms composed in model units then scaled once to millimetres; source surface and PCB placement qualification remain separate",
        "transformCompositionResolved":true,
        "lengthScaleModelUnitsToMm":scale_to_mm,
        "rootOccurrences":model.roots.len(),
        "occurrenceCount":state.occurrences,
        "uniqueReferencedParts":state.referenced_parts.len(),
        "repeatedPartOccurrences":state.occurrences.saturating_sub(state.referenced_parts.len()),
        "partDefinitionTriangles":model.triangle_count(),
        "occurrenceTriangles":state.triangles,
        "worldBoundsMm":world_bounds_mm,
        "occurrenceDetails":state.details,
        "occurrenceDetailsTotal":state.occurrences,
        "occurrenceDetailsTruncated":state.occurrences>MAX_DETAILS,
        "maxOccurrenceCount":MAX_OCCURRENCES,
        "maxDepth":MAX_DEPTH,
        "maxOccurrenceTriangles":MAX_TRIANGLES,
        "occurrenceVertices":state.vertices,
        "maxOccurrenceVertices":MAX_WORLD_VERTICES,
        "surfaceFidelityQualified":false,
        "altiumComponentAssociationQualified":false,
        "pcbPlacementQualified":false
    }))
}

#[allow(clippy::too_many_arguments)]
fn visit(
    model: &StepModel,
    occurrence: &Occurrence,
    parent: &Transform,
    depth: usize,
    path: &mut Vec<usize>,
    active_parts: &mut Vec<usize>,
    state: &mut AuditState,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("step_occurrence_depth_limit_exceeded".into());
    }
    if state.occurrences >= MAX_OCCURRENCES {
        return Err("step_occurrence_count_limit_exceeded".into());
    }
    let part = model
        .parts
        .get(occurrence.part)
        .ok_or_else(|| "step_occurrence_part_index_out_of_range".to_owned())?;
    if active_parts.contains(&occurrence.part) {
        return Err("step_occurrence_part_cycle_detected".into());
    }
    if !transform_is_finite(&occurrence.transform) {
        return Err("step_occurrence_transform_nonfinite".into());
    }

    let world = parent.compose(&occurrence.transform);
    if !transform_is_finite(&world) {
        return Err("step_world_transform_nonfinite".into());
    }
    state.occurrences += 1;
    state.referenced_parts.insert(occurrence.part);
    active_parts.push(occurrence.part);

    let mut local_min = [f64::INFINITY; 3];
    let mut local_max = [f64::NEG_INFINITY; 3];
    let mut local_vertices = 0usize;
    let mut local_triangles = 0usize;
    for shape in &part.shapes {
        local_triangles = local_triangles
            .checked_add(shape.mesh.triangles.len())
            .ok_or_else(|| "step_occurrence_triangle_count_overflow".to_owned())?;
        state.triangles = state
            .triangles
            .checked_add(shape.mesh.triangles.len())
            .filter(|count| *count <= MAX_TRIANGLES)
            .ok_or_else(|| "step_occurrence_triangle_limit_exceeded".to_owned())?;
        for triangle in &shape.mesh.triangles {
            if triangle
                .iter()
                .any(|index| *index as usize >= shape.mesh.positions.len())
            {
                return Err("step_mesh_triangle_index_out_of_range".into());
            }
        }
        state.vertices = state
            .vertices
            .checked_add(shape.mesh.positions.len())
            .filter(|count| *count <= MAX_WORLD_VERTICES)
            .ok_or_else(|| "step_occurrence_vertex_limit_exceeded".to_owned())?;
        for point in &shape.mesh.positions {
            let transformed = world.apply(*point);
            let scaled = transformed.map(|value| value * state.scale_to_mm);
            if scaled.iter().any(|value| !value.is_finite()) {
                return Err("step_world_vertex_nonfinite".into());
            }
            for axis in 0..3 {
                local_min[axis] = local_min[axis].min(scaled[axis]);
                local_max[axis] = local_max[axis].max(scaled[axis]);
                state.minimum[axis] = state.minimum[axis].min(scaled[axis]);
                state.maximum[axis] = state.maximum[axis].max(scaled[axis]);
            }
            local_vertices = local_vertices
                .checked_add(1)
                .ok_or_else(|| "step_occurrence_vertex_count_overflow".to_owned())?;
        }
    }
    if state.details.len() < MAX_DETAILS {
        state.details.push(json!({
            "path":path,
            "occurrenceId":occurrence.id,
            "name":occurrence.name,
            "partIndex":occurrence.part,
            "partName":part.name,
            "directTriangles":local_triangles,
            "directBoundsMm":if local_vertices>0 {json!({"min":local_min,"max":local_max})} else {Value::Null}
        }));
    }

    for (child_index, child) in part.children.iter().enumerate() {
        path.push(child_index);
        visit(model, child, &world, depth + 1, path, active_parts, state)?;
        path.pop();
    }
    active_parts.pop();
    Ok(())
}

fn transform_is_finite(transform: &Transform) -> bool {
    transform
        .cols
        .iter()
        .flatten()
        .chain(transform.translation.iter())
        .all(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideav_step::{Part, Shape, TriMesh};

    fn occurrence(part: usize, transform: Transform) -> Occurrence {
        Occurrence {
            part,
            id: None,
            name: None,
            transform,
            placed_by: None,
        }
    }

    fn triangle_part(children: Vec<Occurrence>) -> Part {
        Part {
            name: Some("triangle".into()),
            shapes: vec![Shape {
                name: None,
                item: 1,
                mesh: TriMesh {
                    positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                    triangles: vec![[0, 1, 2]],
                },
                colour: None,
                triangle_colours: Vec::new(),
                triangle_faces: Vec::new(),
                layers: Vec::new(),
            }],
            children,
            ..Part::default()
        }
    }

    fn empty_part(children: Vec<Occurrence>) -> Part {
        Part {
            name: Some("assembly".into()),
            children,
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
            occurrence_colours: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn nested_rotation_translation_composes_parent_before_child_and_scales_once() {
        let parent = Transform {
            cols: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [10.0, 0.0, 0.0],
        };
        let child = Transform {
            translation: [2.0, 0.0, 0.0],
            ..Transform::IDENTITY
        };
        let result = audit(
            &model(
                vec![
                    empty_part(vec![occurrence(1, child)]),
                    triangle_part(Vec::new()),
                ],
                vec![occurrence(0, parent)],
            ),
            2.0,
        )
        .unwrap();
        assert_eq!(result["occurrenceCount"], 2);
        assert_eq!(result["occurrenceTriangles"], 1);
        assert_eq!(result["worldBoundsMm"]["min"], json!([18.0, 4.0, 0.0]));
        assert_eq!(result["worldBoundsMm"]["max"], json!([20.0, 6.0, 0.0]));
    }

    #[test]
    fn repeated_part_roots_keep_each_placed_occurrence() {
        let first = Transform {
            translation: [0.0, 0.0, 0.0],
            ..Transform::IDENTITY
        };
        let second = Transform {
            translation: [10.0, 0.0, 0.0],
            ..Transform::IDENTITY
        };
        let result = audit(
            &model(
                vec![triangle_part(Vec::new())],
                vec![occurrence(0, first), occurrence(0, second)],
            ),
            1.0,
        )
        .unwrap();
        assert_eq!(result["occurrenceCount"], 2);
        assert_eq!(result["uniqueReferencedParts"], 1);
        assert_eq!(result["repeatedPartOccurrences"], 1);
        assert_eq!(result["occurrenceTriangles"], 2);
        assert_eq!(result["worldBoundsMm"]["min"], json!([0.0, 0.0, 0.0]));
        assert_eq!(result["worldBoundsMm"]["max"], json!([11.0, 1.0, 0.0]));
    }

    #[test]
    fn rejects_part_cycles_missing_roots_invalid_indices_and_nonfinite_transforms() {
        let cycle = model(
            vec![empty_part(vec![occurrence(0, Transform::IDENTITY)])],
            vec![occurrence(0, Transform::IDENTITY)],
        );
        assert_eq!(
            audit(&cycle, 1.0).unwrap_err(),
            "step_occurrence_part_cycle_detected"
        );
        assert_eq!(
            audit(
                &model(
                    vec![triangle_part(Vec::new())],
                    vec![occurrence(1, Transform::IDENTITY)]
                ),
                1.0
            )
            .unwrap_err(),
            "step_occurrence_part_index_out_of_range"
        );
        assert_eq!(
            audit(&model(vec![triangle_part(Vec::new())], Vec::new()), 1.0).unwrap_err(),
            "step_model_has_no_root_occurrences"
        );
        let nonfinite = Transform {
            translation: [f64::NAN, 0.0, 0.0],
            ..Transform::IDENTITY
        };
        assert_eq!(
            audit(
                &model(
                    vec![triangle_part(Vec::new())],
                    vec![occurrence(0, nonfinite)]
                ),
                1.0
            )
            .unwrap_err(),
            "step_occurrence_transform_nonfinite"
        );
        assert_eq!(
            audit(
                &model(
                    vec![triangle_part(Vec::new())],
                    vec![occurrence(0, Transform::IDENTITY)]
                ),
                f64::INFINITY
            )
            .unwrap_err(),
            "invalid_model_unit_scale"
        );
    }

    #[test]
    fn caps_zero_triangle_vertex_work_across_repeated_occurrences() {
        let vertices = vec![[0.0, 0.0, 0.0]; MAX_WORLD_VERTICES / 2 + 1];
        let part = Part {
            shapes: vec![Shape {
                name: None,
                item: 1,
                mesh: TriMesh {
                    positions: vertices,
                    triangles: Vec::new(),
                },
                colour: None,
                triangle_colours: Vec::new(),
                triangle_faces: Vec::new(),
                layers: Vec::new(),
            }],
            ..Part::default()
        };
        let result = audit(
            &model(
                vec![part],
                vec![
                    occurrence(0, Transform::IDENTITY),
                    occurrence(0, Transform::IDENTITY),
                ],
            ),
            1.0,
        );
        assert_eq!(result.unwrap_err(), "step_occurrence_vertex_limit_exceeded");
    }

    #[test]
    fn rejects_invalid_triangle_indices_and_nonfinite_world_vertices() {
        let mut invalid_mesh = triangle_part(Vec::new());
        invalid_mesh.shapes[0].mesh.triangles[0] = [0, 1, 3];
        assert_eq!(
            audit(
                &model(vec![invalid_mesh], vec![occurrence(0, Transform::IDENTITY)]),
                1.0
            )
            .unwrap_err(),
            "step_mesh_triangle_index_out_of_range"
        );

        let huge_translation = Transform {
            translation: [f64::MAX, 0.0, 0.0],
            ..Transform::IDENTITY
        };
        assert_eq!(
            audit(
                &model(
                    vec![triangle_part(Vec::new())],
                    vec![occurrence(0, huge_translation)],
                ),
                2.0
            )
            .unwrap_err(),
            "step_world_vertex_nonfinite"
        );
    }

    #[test]
    fn refuses_occurrence_depth_over_budget() {
        let mut parts = Vec::new();
        for index in 0..=MAX_DEPTH + 1 {
            let children = if index == MAX_DEPTH + 1 {
                Vec::new()
            } else {
                vec![occurrence(index + 1, Transform::IDENTITY)]
            };
            parts.push(empty_part(children));
        }
        parts[MAX_DEPTH + 1] = triangle_part(Vec::new());
        assert_eq!(
            audit(&model(parts, vec![occurrence(0, Transform::IDENTITY)]), 1.0).unwrap_err(),
            "step_occurrence_depth_limit_exceeded"
        );
    }
}
