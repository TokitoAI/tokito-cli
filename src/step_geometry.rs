//! Bounded STEP-to-mesh candidate import for explicitly assigned component models.
//!
//! The output reuses the common placement and GLB pipeline, but this adapter
//! does not establish B-rep closure, source-face completeness, model-to-part
//! fit, or physical qualification. Reader warnings and ambiguous units fail
//! closed; supported output remains an unqualified visualization candidate.

use crate::model_mesh::{MeshMaterial, ModelMesh, ModelMeshPart, ModelUnits};
use crate::{EngineError, step_unit_preflight};
use oxideav_ifc::{StepLimits, Transform, parse_step_with_limits};
use oxideav_step::{GeometryLimits, ReadOptions, Tolerance, model_from_file};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MAX_STEP_BYTES: usize = 32 * 1024 * 1024;
const MAX_STEP_INSTANCES: usize = 250_000;
const MAX_STEP_TRIANGLES: usize = 250_000;
const MAX_STEP_CURVE_SAMPLES: usize = 16_384;
const MAX_STEP_DEPTH: usize = 48;
const MAX_STEP_SHAPES: usize = 20_000;
const MAX_STEP_VERTICES: usize = 200_000;
const MAX_STEP_OCCURRENCES: usize = 100_000;

#[derive(Clone, Debug)]
pub(crate) struct ParsedStepMesh {
    pub mesh: ModelMesh,
    pub source_unit_metres: f64,
    pub diagnostics: Vec<String>,
}

#[derive(Default)]
struct ColourGroup {
    positions: Vec<[f64; 3]>,
    triangles: Vec<[u32; 3]>,
    face_ids: Vec<Option<u64>>,
    indices: BTreeMap<u32, u32>,
}

type GeneratedNormals = (Vec<[f64; 3]>, Vec<[[f64; 3]; 3]>);

fn fail(message: impl Into<String>) -> EngineError {
    EngineError::new(format!("STEP model: {}", message.into()))
}

fn parse_limits() -> StepLimits {
    StepLimits {
        max_input_len: MAX_STEP_BYTES,
        max_instances: MAX_STEP_INSTANCES,
        max_depth: MAX_STEP_DEPTH,
        max_string_len: 1024 * 1024,
    }
}

/// Parse declared STEP geometry under strict unit, resource and warning gates.
/// Lengths are converted to common model millimetres once, then placement is
/// handled by the existing board-coordinate transformer.
pub(crate) fn parse_step_mesh(bytes: &[u8]) -> Result<ParsedStepMesh, EngineError> {
    if bytes.is_empty() || bytes.len() > MAX_STEP_BYTES {
        return Err(fail("input is empty or exceeds the 32 MiB reader cap"));
    }
    if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1") {
        return Err(fail("Git LFS pointer is not materialized"));
    }

    let limits = parse_limits();
    let options = ReadOptions {
        parse_limits: limits.clone(),
        geometry_limits: GeometryLimits {
            max_triangles: MAX_STEP_TRIANGLES,
            max_curve_samples: MAX_STEP_CURVE_SAMPLES,
        },
        tolerance: Tolerance {
            relative: 1e-3,
            absolute: Some(1e-6),
            max_angle: 2.0 * std::f64::consts::PI / 48.0,
        },
        keep_invisible: false,
    };
    let (model, source_unit_metres) = {
        let parsed_file = parse_step_with_limits(bytes, &limits)
            .map_err(|_| fail("ISO 10303-21 structure rejected by bounded parser"))?;
        let audit = step_unit_preflight::inspect(&parsed_file);
        let source_unit_metres = one_declared_length_scale(&audit)?;
        let model = model_from_file(&parsed_file, &options)
            .map_err(|_| fail("STEP geometry could not be tessellated within bounded profile"))?;
        (model, source_unit_metres)
    };
    if !reader_units_match(model.length_unit_metres, source_unit_metres) {
        return Err(fail(
            "reader length unit disagrees with audited STEP contexts",
        ));
    }
    if !model.warnings.is_empty() {
        return Err(fail("reader reported unsupported or incomplete geometry"));
    }
    if model.roots.is_empty() {
        return Err(fail("STEP model has no root product occurrences"));
    }

    let mut diagnostics = vec![
        "STEP reader tessellation is a visualization candidate; physical fit and solid closure are unverified".to_owned(),
    ];
    if !model.occurrence_colours.is_empty() {
        diagnostics.push(
            "STEP occurrence-specific style overrides are not applied to component instances"
                .into(),
        );
    }
    let mut parts = Vec::new();
    let mut active_parts = BTreeSet::new();
    let mut occurrence_count = 0usize;
    let mut shape_count = 0usize;
    let mut expanded_vertices = 0usize;
    let mut emitted_vertices = 0usize;
    let mut expanded_triangles = 0usize;
    for root in &model.roots {
        validate_transform(root.transform)?;
        flatten_occurrence(
            &model,
            root.part,
            root.transform,
            0,
            &mut active_parts,
            &mut occurrence_count,
            &mut shape_count,
            &mut expanded_vertices,
            &mut emitted_vertices,
            &mut expanded_triangles,
            &mut parts,
            &mut diagnostics,
        )?;
    }
    if parts.is_empty() {
        return Err(fail("STEP roots contain no retained triangle geometry"));
    }
    let mesh = ModelMesh {
        source_units: ModelUnits::Millimetres,
        root_shape_count: shape_count,
        material_definitions: BTreeMap::new(),
        parts,
        diagnostics: diagnostics.clone(),
    };
    Ok(ParsedStepMesh {
        mesh,
        source_unit_metres,
        diagnostics,
    })
}

fn one_declared_length_scale(audit: &Value) -> Result<f64, EngineError> {
    if audit.get("analysisComplete") != Some(&Value::Bool(true))
        || audit.get("lengthUnitsResolved") != Some(&Value::Bool(true))
    {
        return Err(fail(
            "STEP length-unit context audit failed or was incomplete",
        ));
    }
    let scales = audit
        .get("distinctMetresPerUnit")
        .and_then(Value::as_array)
        .ok_or_else(|| fail("STEP unit audit omitted resolved scale list"))?;
    if scales.len() != 1 {
        return Err(fail(
            "mixed or absent STEP length-unit scales are unsupported",
        ));
    }
    let scale = scales[0]
        .as_f64()
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 1e6)
        .ok_or_else(|| fail("STEP declared length scale is invalid"))?;
    Ok(scale)
}

fn reader_units_match(reader: f64, audited: f64) -> bool {
    if !reader.is_finite() || reader <= 0.0 || !audited.is_finite() || audited <= 0.0 {
        return false;
    }
    let relative_error = (reader - audited).abs() / audited;
    relative_error.is_finite() && relative_error <= 1e-12
}

#[allow(clippy::too_many_arguments)]
fn flatten_occurrence(
    model: &oxideav_step::StepModel,
    part_index: usize,
    world: Transform,
    depth: usize,
    active_parts: &mut BTreeSet<usize>,
    occurrence_count: &mut usize,
    shape_count: &mut usize,
    expanded_vertices: &mut usize,
    emitted_vertices: &mut usize,
    expanded_triangles: &mut usize,
    output: &mut Vec<ModelMeshPart>,
    diagnostics: &mut Vec<String>,
) -> Result<(), EngineError> {
    if depth > MAX_STEP_DEPTH {
        return Err(fail("assembly occurrence depth cap exceeded"));
    }
    *occurrence_count = occurrence_count
        .checked_add(1)
        .filter(|count| *count <= MAX_STEP_OCCURRENCES)
        .ok_or_else(|| fail("assembly occurrence cap exceeded"))?;
    if !active_parts.insert(part_index) {
        return Err(fail("cyclic STEP part occurrence graph"));
    }
    let part = model
        .parts
        .get(part_index)
        .ok_or_else(|| fail("STEP occurrence references missing part"))?;
    for shape in &part.shapes {
        *shape_count = shape_count
            .checked_add(1)
            .filter(|count| *count <= MAX_STEP_SHAPES)
            .ok_or_else(|| fail("STEP shape cap exceeded"))?;
        let next_vertices = expanded_vertices
            .checked_add(shape.mesh.positions.len())
            .filter(|count| *count <= MAX_STEP_VERTICES)
            .ok_or_else(|| fail("expanded STEP source-vertex cap exceeded"))?;
        let next_triangles = expanded_triangles
            .checked_add(shape.mesh.triangles.len())
            .filter(|count| *count <= MAX_STEP_TRIANGLES)
            .ok_or_else(|| fail("expanded STEP triangle cap exceeded"))?;
        *expanded_vertices = next_vertices;
        *expanded_triangles = next_triangles;
        let local_positions = shape
            .mesh
            .positions
            .iter()
            .map(|position| {
                let transformed = world.apply(*position);
                let millimetres =
                    transformed.map(|coordinate| coordinate * model.length_unit_metres * 1000.0);
                if millimetres.iter().all(|coordinate| coordinate.is_finite()) {
                    Ok(millimetres)
                } else {
                    Err(fail(
                        "STEP occurrence transform produced non-finite coordinates",
                    ))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !shape.triangle_colours.is_empty()
            && shape.triangle_colours.len() != shape.mesh.triangles.len()
        {
            return Err(fail(
                "STEP triangle colour array does not match triangle count",
            ));
        }
        if !shape.triangle_faces.is_empty()
            && shape.triangle_faces.len() != shape.mesh.triangles.len()
        {
            diagnostics.push("STEP triangle face provenance count does not match triangle count; flat normals used".into());
        }
        let mut groups = BTreeMap::<Option<[u32; 4]>, ColourGroup>::new();
        let mut group_colours = BTreeMap::<Option<[u32; 4]>, Option<[f32; 4]>>::new();
        for (triangle_index, triangle) in shape.mesh.triangles.iter().enumerate() {
            if triangle
                .iter()
                .any(|index| *index as usize >= local_positions.len())
            {
                return Err(fail("STEP triangle index is outside its position array"));
            }
            let source_colour = shape
                .triangle_colours
                .get(triangle_index)
                .copied()
                .flatten()
                .or(shape.colour);
            validate_colour(source_colour)?;
            let key = source_colour.map(|colour| colour.map(f32::to_bits));
            group_colours.entry(key).or_insert(source_colour);
            let group = groups.entry(key).or_default();
            let mut remapped = [0u32; 3];
            for (corner, source_index) in triangle.iter().enumerate() {
                remapped[corner] = if let Some(index) = group.indices.get(source_index) {
                    *index
                } else {
                    *emitted_vertices = emitted_vertices
                        .checked_add(1)
                        .filter(|count| *count <= MAX_STEP_VERTICES)
                        .ok_or_else(|| fail("expanded STEP render-vertex cap exceeded"))?;
                    let next = u32::try_from(group.positions.len())
                        .map_err(|_| fail("STEP grouped vertex index exceeds GLB range"))?;
                    group
                        .positions
                        .push(local_positions[*source_index as usize]);
                    group.indices.insert(*source_index, next);
                    next
                };
            }
            let a = group.positions[remapped[0] as usize];
            let b = group.positions[remapped[1] as usize];
            let c = group.positions[remapped[2] as usize];
            let cross = triangle_cross(a, b, c)?;
            group.triangles.push(remapped);
            group.face_ids.push(
                shape
                    .triangle_faces
                    .get(triangle_index)
                    .copied()
                    .filter(|id| *id != 0),
            );
            if cross.iter().any(|value| !value.is_finite()) {
                return Err(fail("STEP triangle normal overflow"));
            }
        }
        for (key, group) in groups {
            let colour = group_colours.get(&key).copied().flatten();
            let (flat_normals, corner_normals) = generated_normals(&group)?;
            let material = colour.map(|rgba| MeshMaterial {
                diffuse_color: [rgba[0] as f64, rgba[1] as f64, rgba[2] as f64],
                specular_color: [0.0; 3],
                emissive_color: [0.0; 3],
                ambient_intensity: 0.2,
                shininess: 0.2,
                transparency: 1.0 - rgba[3] as f64,
            });
            let source_shape_ordinal = output.len();
            output.push(ModelMeshPart {
                source_shape_ordinal,
                positions_mm: group.positions,
                triangles: group.triangles,
                flat_normals,
                corner_normals,
                material,
                source_crease_angle_radians: 0.0,
                solid: false,
            });
        }
    }
    for child in &part.children {
        let child_world = world.compose(&child.transform);
        validate_transform(child_world)?;
        flatten_occurrence(
            model,
            child.part,
            child_world,
            depth + 1,
            active_parts,
            occurrence_count,
            shape_count,
            expanded_vertices,
            emitted_vertices,
            expanded_triangles,
            output,
            diagnostics,
        )?;
    }
    active_parts.remove(&part_index);
    Ok(())
}

fn validate_transform(transform: Transform) -> Result<(), EngineError> {
    if transform
        .cols
        .iter()
        .flatten()
        .chain(transform.translation.iter())
        .any(|value| !value.is_finite())
    {
        return Err(fail("STEP occurrence transform is non-finite"));
    }
    let [x, y, z] = transform.cols;
    let determinant = x[0] * (y[1] * z[2] - z[1] * y[2]) - y[0] * (x[1] * z[2] - z[1] * x[2])
        + z[0] * (x[1] * y[2] - y[1] * x[2]);
    if !determinant.is_finite() || determinant.abs() <= f64::MIN_POSITIVE {
        return Err(fail("STEP occurrence transform is singular or overflowing"));
    }
    Ok(())
}

fn validate_colour(colour: Option<[f32; 4]>) -> Result<(), EngineError> {
    if colour.is_some_and(|rgba| {
        rgba.iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    }) {
        return Err(fail(
            "STEP style contains a colour outside finite [0,1] RGBA",
        ));
    }
    Ok(())
}

fn triangle_cross(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Result<[f64; 3], EngineError> {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    if u.iter().chain(&v).any(|value| !value.is_finite()) {
        return Err(fail("STEP triangle coordinate difference overflow"));
    }
    let cross = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let scale = cross.iter().map(|value| value.abs()).fold(0.0, f64::max);
    if !scale.is_finite() || scale <= f64::MIN_POSITIVE {
        return Err(fail("STEP geometry contains a degenerate triangle"));
    }
    Ok(cross)
}

fn generated_normals(group: &ColourGroup) -> Result<GeneratedNormals, EngineError> {
    let mut flat = Vec::with_capacity(group.triangles.len());
    let mut weighted = BTreeMap::<(Option<u64>, Option<usize>, u32), [f64; 3]>::new();
    for (triangle_index, triangle) in group.triangles.iter().enumerate() {
        let cross = triangle_cross(
            group.positions[triangle[0] as usize],
            group.positions[triangle[1] as usize],
            group.positions[triangle[2] as usize],
        )?;
        let magnitude = cross[0].hypot(cross[1]).hypot(cross[2]);
        if !magnitude.is_finite() || magnitude == 0.0 {
            return Err(fail("STEP geometry contains an invalid triangle normal"));
        }
        flat.push(cross.map(|value| value / magnitude));
        let face = group.face_ids[triangle_index];
        let triangle_scope = face.is_none().then_some(triangle_index);
        for index in triangle {
            let sum = weighted
                .entry((face, triangle_scope, *index))
                .or_insert([0.0; 3]);
            for axis in 0..3 {
                sum[axis] += cross[axis];
            }
        }
    }
    let mut corners = Vec::with_capacity(group.triangles.len());
    for (triangle_index, triangle) in group.triangles.iter().enumerate() {
        let face = group.face_ids[triangle_index];
        let triangle_scope = face.is_none().then_some(triangle_index);
        let mut normals = [[0.0; 3]; 3];
        for (corner, index) in triangle.iter().enumerate() {
            let sum = weighted[&(face, triangle_scope, *index)];
            let magnitude = sum[0].hypot(sum[1]).hypot(sum[2]);
            if !magnitude.is_finite() || magnitude == 0.0 {
                normals[corner] = flat[triangle_index];
            } else {
                normals[corner] = sum.map(|value| value / magnitude);
            }
        }
        corners.push(normals);
    }
    Ok((flat, corners))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideav_ifc::Transform;
    use oxideav_step::Occurrence;

    const TRIANGLE: &[u8] = include_bytes!("../tests/fixtures/step/declared-mm-triangle.step");

    fn fixture_model() -> oxideav_step::StepModel {
        oxideav_step::read_step(TRIANGLE).expect("checked-in STEP fixture reads")
    }

    fn flatten_for_test(
        model: &oxideav_step::StepModel,
    ) -> Result<Vec<ModelMeshPart>, EngineError> {
        flatten_with_seeded_budgets(model, 0, 0, 0, 0, 0, 0)
    }

    fn flatten_with_seeded_budgets(
        model: &oxideav_step::StepModel,
        depth: usize,
        mut occurrence_count: usize,
        mut shape_count: usize,
        mut expanded_vertices: usize,
        mut emitted_vertices: usize,
        mut expanded_triangles: usize,
    ) -> Result<Vec<ModelMeshPart>, EngineError> {
        let root = model
            .roots
            .first()
            .ok_or_else(|| fail("test fixture has no root"))?;
        let mut parts = Vec::new();
        flatten_occurrence(
            model,
            root.part,
            root.transform,
            depth,
            &mut BTreeSet::new(),
            &mut occurrence_count,
            &mut shape_count,
            &mut expanded_vertices,
            &mut emitted_vertices,
            &mut expanded_triangles,
            &mut parts,
            &mut Vec::new(),
        )?;
        Ok(parts)
    }

    #[test]
    fn declared_mm_tessellated_surface_is_converted_to_common_mesh_once() {
        let parsed = parse_step_mesh(TRIANGLE).expect("checked-in STEP fixture parses");
        assert_eq!(parsed.source_unit_metres, 0.001);
        assert_eq!(parsed.mesh.root_shape_count, 1);
        assert_eq!(parsed.mesh.parts.len(), 1);
        assert_eq!(parsed.mesh.parts[0].triangles.len(), 2);
        assert!(!parsed.mesh.parts[0].solid);
        let bounds = parsed.mesh.parts[0].positions_mm.iter().fold(
            ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
            |mut bounds, point| {
                for (axis, coordinate) in point.iter().copied().enumerate() {
                    bounds.0[axis] = bounds.0[axis].min(coordinate);
                    bounds.1[axis] = bounds.1[axis].max(coordinate);
                }
                bounds
            },
        );
        assert_eq!(bounds.0, [0.0, 0.0, 0.0]);
        assert_eq!(bounds.1, [1.0, 2.0, 0.0]);
        assert_eq!(parsed.mesh.parts[0].corner_normals.len(), 2);
        assert!(
            parsed
                .mesh
                .diagnostics
                .iter()
                .any(|item| item.contains("unverified"))
        );
    }

    #[test]
    fn declared_unit_is_required_instead_of_reader_default() {
        let missing = TRIANGLE
            .windows(b"GLOBAL_UNIT_ASSIGNED_CONTEXT((#1,#2))".len())
            .position(|window| window == b"GLOBAL_UNIT_ASSIGNED_CONTEXT((#1,#2))")
            .expect("fixture unit context");
        let mut changed = TRIANGLE.to_vec();
        changed.splice(
            missing..missing + b"GLOBAL_UNIT_ASSIGNED_CONTEXT((#1,#2))".len(),
            b"GLOBAL_UNIT_ASSIGNED_CONTEXT((#2))".iter().copied(),
        );
        assert!(parse_step_mesh(&changed).is_err());
    }

    #[test]
    fn tiny_reader_unit_scales_are_compared_relatively() {
        assert!(reader_units_match(1e-15, 1e-15));
        assert!(reader_units_match(1e-15 * (1.0 + 5e-13), 1e-15));
        assert!(!reader_units_match(1e-12, 1e-15));
        assert!(!reader_units_match(1e-15, 1e-12));
        assert!(!reader_units_match(f64::NAN, 1e-3));
    }

    #[test]
    fn input_budget_and_lfs_pointer_are_rejected_before_decode() {
        assert!(parse_step_mesh(b"version https://git-lfs.github.com/spec/v1\n").is_err());
        assert!(parse_step_mesh(&vec![b' '; MAX_STEP_BYTES + 1]).is_err());
    }

    #[test]
    fn occurrence_and_expanded_geometry_budgets_refuse_at_boundary() {
        let model = fixture_model();
        let cases = [
            (MAX_STEP_DEPTH + 1, 0, 0, 0, 0, 0, "occurrence depth cap"),
            (0, MAX_STEP_OCCURRENCES, 0, 0, 0, 0, "occurrence cap"),
            (0, 0, MAX_STEP_SHAPES, 0, 0, 0, "shape cap"),
            (
                0,
                0,
                0,
                MAX_STEP_VERTICES,
                0,
                0,
                "expanded STEP source-vertex cap",
            ),
            (
                0,
                0,
                0,
                0,
                0,
                MAX_STEP_TRIANGLES,
                "expanded STEP triangle cap",
            ),
            (
                0,
                0,
                0,
                0,
                MAX_STEP_VERTICES,
                0,
                "expanded STEP render-vertex cap",
            ),
        ];
        for (depth, occurrences, shapes, source_vertices, render_vertices, triangles, expected) in
            cases
        {
            let error = flatten_with_seeded_budgets(
                &model,
                depth,
                occurrences,
                shapes,
                source_vertices,
                render_vertices,
                triangles,
            )
            .expect_err(expected);
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn nested_repeated_part_occurrences_compose_transforms_in_parent_order() {
        let mut model = fixture_model();
        let root_part = model.roots[0].part;
        let first_child = model.parts.len();
        model.parts.push(model.parts[root_part].clone());
        let second_child = model.parts.len();
        model.parts.push(model.parts[root_part].clone());
        model.roots[0].transform = Transform {
            translation: [10.0, 1.0, 2.0],
            ..Transform::IDENTITY
        };
        model.parts[root_part].children.push(Occurrence {
            part: first_child,
            id: Some(1),
            name: None,
            transform: Transform {
                cols: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
                translation: [3.0, 0.0, 0.0],
            },
            placed_by: Some(1),
        });
        model.parts[first_child].children.push(Occurrence {
            part: second_child,
            id: Some(2),
            name: None,
            transform: Transform {
                translation: [0.0, 4.0, 0.0],
                ..Transform::IDENTITY
            },
            placed_by: Some(2),
        });

        let flattened = flatten_for_test(&model).expect("bounded nested instances flatten");
        assert_eq!(flattened.len(), 3);
        let nested_bounds = flattened[2].positions_mm.iter().fold(
            ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
            |mut bounds, point| {
                for (axis, coordinate) in point.iter().copied().enumerate() {
                    bounds.0[axis] = bounds.0[axis].min(coordinate);
                    bounds.1[axis] = bounds.1[axis].max(coordinate);
                }
                bounds
            },
        );
        for (actual, expected) in nested_bounds
            .0
            .iter()
            .zip([7.0, 1.0, 2.0])
            .chain(nested_bounds.1.iter().zip([9.0, 2.0, 2.0]))
        {
            assert!((actual - expected).abs() <= 1e-10);
        }
    }

    #[test]
    fn cyclic_part_occurrence_graph_is_rejected() {
        let mut model = fixture_model();
        let root_part = model.roots[0].part;
        let child_part = model.parts.len();
        model.parts.push(model.parts[root_part].clone());
        model.parts[root_part].children.push(Occurrence {
            part: child_part,
            id: Some(1),
            name: None,
            transform: Transform::IDENTITY,
            placed_by: Some(1),
        });
        model.parts[child_part].children.push(Occurrence {
            part: root_part,
            id: Some(2),
            name: None,
            transform: Transform::IDENTITY,
            placed_by: Some(2),
        });
        assert!(flatten_for_test(&model).is_err());
    }
}
