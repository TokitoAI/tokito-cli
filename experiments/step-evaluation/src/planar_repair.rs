//! Opt-in diagnostic repair, never production STEP admission.
use oxideav_ifc::StepFile;
use oxideav_step::{schema::Entity, StepModel};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn aligned(expected: [f64; 3], actual: [f64; 3]) -> bool {
    let agreement = expected.iter().zip(actual).map(|(a, b)| a * b).sum::<f64>();
    expected.iter().chain(&actual).all(|v| v.is_finite()) && agreement.is_finite() && agreement > 0.
}

fn linked(step: &StepFile, primary: u64, owner: u64) -> Result<(), String> {
    if step.instances.len() > 250_000 {
        return Err("repair_source_limit".into());
    }
    let mut links = BTreeMap::<u64, Vec<u64>>::new();
    for inst in step.instances.values() {
        let entity = Entity { inst };
        if entity.is_a("SHAPE_REPRESENTATION_RELATIONSHIP")
            && !entity.is_a("REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION")
        {
            crate::unit_preflight::shape(inst)?;
            let a = entity.reference("rep_1").ok_or("repair_missing_rep_1")?;
            let b = entity.reference("rep_2").ok_or("repair_missing_rep_2")?;
            links.entry(a).or_default().push(b);
            links.entry(b).or_default().push(a);
        }
    }
    let mut pending = vec![primary];
    let mut seen = BTreeSet::new();
    let mut work = 0;
    while let Some(id) = pending.pop() {
        work += 1;
        if work > 100_000 || seen.len() > 512 {
            return Err("repair_rep_link_budget".into());
        }
        if !seen.insert(id) {
            continue;
        }
        let inst = step.get(id).ok_or("repair_dangling_representation")?;
        crate::unit_preflight::shape(inst)?;
        if !(Entity { inst }).is_a("SHAPE_REPRESENTATION") {
            return Err("repair_invalid_representation_link".into());
        }
        if let Some(neighbours) = links.get(&id) {
            pending.extend(neighbours);
        }
    }
    if !seen.contains(&owner) {
        return Err("repair_no_plain_representation_path".into());
    }
    Ok(())
}

/// Change only the target face after all planning checks; source and input model
/// remain immutable. Existing positions are reused to retain neighbour seams.
pub fn apply(
    source: &StepFile,
    original: &StepModel,
    face: u64,
    representation: u64,
    tolerance: f64,
) -> Result<(StepModel, Value), String> {
    let source_face = crate::planar_face::build(source, face, tolerance)?;
    let owner = crate::face_representation::inspect(source, representation, face)?;
    let item = owner["representationItemId"]
        .as_u64()
        .ok_or("repair_missing_item")?;
    let unit = owner["selectedRepresentationLengthUnits"]["metresPerUnit"]
        .as_f64()
        .ok_or("repair_missing_source_unit")?;
    if !original.length_unit_metres.is_finite() || original.length_unit_metres <= 0. {
        return Err("repair_invalid_model_unit".into());
    }
    let scale = unit / original.length_unit_metres;
    let model_tolerance = tolerance * scale;
    if !scale.is_finite() || scale <= 0. || !model_tolerance.is_finite() || model_tolerance <= 0. {
        return Err("repair_invalid_scale".into());
    }
    if original.triangle_count() > 2_000_000 {
        return Err("repair_model_budget".into());
    }
    let matches = original
        .parts
        .iter()
        .enumerate()
        .flat_map(|(p, part)| {
            part.shapes
                .iter()
                .enumerate()
                .filter(move |(_, s)| s.item == item)
                .map(move |(s, _)| (p, s))
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err("repair_requires_one_target_shape".into());
    }
    let (part_index, shape_index) = matches[0];
    linked(
        source,
        original.parts[part_index]
            .representation
            .ok_or("repair_missing_primary_representation")?,
        representation,
    )?;
    let shape = &original.parts[part_index].shapes[shape_index];
    if shape.mesh.positions.len() > 1_000_000
        || shape.triangle_faces.len() != shape.mesh.triangles.len()
        || (!shape.triangle_colours.is_empty()
            && shape.triangle_colours.len() != shape.mesh.triangles.len())
    {
        return Err("repair_invalid_geometry_metadata".into());
    }
    let old_indices = shape
        .triangle_faces
        .iter()
        .enumerate()
        .filter(|(_, id)| **id == face)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    if old_indices.is_empty() {
        return Err("repair_missing_face_provenance".into());
    }
    let used = old_indices
        .iter()
        .flat_map(|i| shape.mesh.triangles[*i])
        .collect::<BTreeSet<_>>();
    if used.len() != source_face.vertices.len() {
        return Err("repair_boundary_vertex_count_mismatch".into());
    }
    let mut mapping = Vec::new();
    let mut claimed = BTreeSet::new();
    let mut maximum_distance: f64 = 0.;
    for point in &source_face.vertices {
        let p = point.map(|v| v * scale);
        if p.iter().any(|v| !v.is_finite()) {
            return Err("repair_nonfinite_source_vertex".into());
        }
        let mut nearest = None;
        let mut distance = f64::INFINITY;
        let mut tie = false;
        for index in &used {
            let q = shape
                .mesh
                .positions
                .get(*index as usize)
                .ok_or("repair_invalid_vertex_index")?;
            if q.iter().any(|v| !v.is_finite()) {
                return Err("repair_nonfinite_mesh_vertex".into());
            }
            let d = (0..3).map(|axis| (p[axis] - q[axis]).powi(2)).sum::<f64>();
            if d < distance {
                nearest = Some(*index);
                distance = d;
                tie = false;
            } else if d == distance {
                tie = true;
            }
        }
        let index = nearest.ok_or("repair_no_boundary_match")?;
        if tie || distance.sqrt() > model_tolerance || !claimed.insert(index) {
            return Err("repair_ambiguous_or_distant_boundary_match".into());
        }
        maximum_distance = maximum_distance.max(distance.sqrt());
        mapping.push(index);
    }
    let replacement = source_face
        .triangles
        .iter()
        .map(|t| t.map(|i| mapping[i]))
        .collect::<Vec<_>>();
    // Matching must not collapse or reverse an otherwise valid source triangle.
    for (source_triangle, mapped) in source_face.triangles.iter().zip(&replacement) {
        let normal = |points: [[f64; 3]; 3]| {
            let a = std::array::from_fn::<_, 3, _>(|i| points[1][i] - points[0][i]);
            let b = std::array::from_fn::<_, 3, _>(|i| points[2][i] - points[0][i]);
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let expected = normal(source_triangle.map(|i| source_face.vertices[i].map(|v| v * scale)));
        let actual = normal(mapped.map(|i| shape.mesh.positions[i as usize]));
        if !aligned(expected, actual) {
            return Err("repair_mapped_triangle_orientation".into());
        }
    }
    let colour = if shape.triangle_colours.is_empty() {
        None
    } else {
        let first = shape.triangle_colours[old_indices[0]];
        if old_indices
            .iter()
            .any(|i| shape.triangle_colours[*i] != first)
        {
            return Err("repair_ambiguous_face_colour".into());
        }
        first
    };
    let mut triangles = Vec::new();
    let mut faces = Vec::new();
    let mut colours = Vec::new();
    let mut inserted = false;
    for (i, id) in shape.triangle_faces.iter().enumerate() {
        if *id == face {
            if !inserted {
                triangles.extend_from_slice(&replacement);
                faces.extend(std::iter::repeat_n(face, replacement.len()));
                if !shape.triangle_colours.is_empty() {
                    colours.extend(std::iter::repeat_n(colour, replacement.len()));
                }
                inserted = true;
            }
        } else {
            triangles.push(shape.mesh.triangles[i]);
            faces.push(*id);
            if !shape.triangle_colours.is_empty() {
                colours.push(shape.triangle_colours[i]);
            }
        }
    }
    let mut repaired = original.clone();
    if original.triangle_count() - old_indices.len() + replacement.len() > 2_000_000 {
        return Err("repair_result_triangle_budget".into());
    }
    let target = &mut repaired.parts[part_index].shapes[shape_index];
    target.mesh.triangles = triangles;
    target.triangle_faces = faces;
    target.triangle_colours = colours;
    Ok((
        repaired,
        json!({"schema":"tokito.step-planar-repair.v1","faceId":face,"representationId":representation,"itemId":item,"partIndex":part_index,"shapeIndex":shape_index,"oldFaceTriangles":old_indices.len(),"newFaceTriangles":replacement.len(),"sourceToModelLengthScale":scale,"maximumBoundaryMatchDistanceModelUnits":maximum_distance,"callerToleranceSourceUnits":tolerance,"reusedBoundaryVertices":mapping.len(),"positionsChanged":false,"occurrenceTransformsChanged":false,"unrelatedTrianglesChanged":false,"selectedSourceRepresentation":owner,"sourceFace":source_face.report,"geometryQualified":false,"admitted":false,"scope":"explicit diagnostic candidate repair; whole-solid and component qualification remain independent"}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn orientation_requires_finite_positive_agreement() {
        assert!(aligned([0., 0., 1.], [0., 0., 2.]));
        assert!(!aligned([0., 0., 1.], [0., 0., -1.]));
        assert!(!aligned([1e308, 1e308, 0.], [1e308, -1e308, 0.]));
        assert!(!aligned([1e308, 0., 0.], [1e308, 0., 0.]));
        assert!(!aligned([0.; 3], [0.; 3]));
    }
    fn fixture() -> (StepFile, StepModel) {
        let text=crate::planar_face::tests::source_text(true,true,true,1.).replace("ENDSEC;END-ISO-10303-21;","#80=MANIFOLD_SOLID_BREP('',#81);#81=CLOSED_SHELL('',(#75));#82=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#80),#83);#83=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT((#84)) REPRESENTATION_CONTEXT('',''));#84=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));ENDSEC;END-ISO-10303-21;");
        let source = oxideav_ifc::parse_step(text.as_bytes()).unwrap();
        let mut model = oxideav_step::model_from_file(&source, &Default::default()).unwrap();
        // Synthetic input mesh, not real engineering qualification evidence.
        model.parts[0].representation = Some(82);
        let shape = &mut model.parts[0].shapes[0];
        shape.mesh.positions = vec![[0., 0., 0.], [2., 0., 0.], [2., 1., 0.], [0., 1., 0.]];
        shape.mesh.triangles = vec![[0, 1, 2], [0, 2, 3], [0, 1, 3]];
        shape.triangle_faces = vec![75, 75, 999];
        shape.triangle_colours = vec![Some([1., 0., 0., 1.]); 3];
        (source, model)
    }
    #[test]
    fn preserves_input_positions_unrelated_triangles_metadata_and_occurrences() {
        let (source, model) = fixture();
        let before = format!("{model:?}");
        let (repaired, report) = apply(&source, &model, 75, 82, 1e-9).unwrap();
        assert_eq!(format!("{model:?}"), before);
        let a = &model.parts[0].shapes[0];
        let b = &repaired.parts[0].shapes[0];
        assert_eq!(a.mesh.positions, b.mesh.positions);
        assert_eq!(b.mesh.triangles.last(), Some(&[0, 1, 3]));
        assert_eq!(b.triangle_faces, vec![75, 75, 999]);
        assert_eq!(a.triangle_colours, b.triangle_colours);
        assert_eq!(
            format!("{:?}", model.roots),
            format!("{:?}", repaired.roots)
        );
        assert_eq!(report["reusedBoundaryVertices"], 4);
        assert_eq!(report["admitted"], false);
        let mut empty = model.clone();
        empty.parts[0].shapes[0].triangle_colours.clear();
        assert!(
            apply(&source, &empty, 75, 82, 1e-9).unwrap().0.parts[0].shapes[0]
                .triangle_colours
                .is_empty()
        );
        let mut metres = model.clone();
        metres.length_unit_metres = 1.;
        for p in &mut metres.parts[0].shapes[0].mesh.positions {
            for v in p {
                *v *= 0.001;
            }
        }
        let (scaled, proof) = apply(&source, &metres, 75, 82, 1e-9).unwrap();
        assert_eq!(proof["sourceToModelLengthScale"], 0.001);
        assert_eq!(
            scaled.parts[0].shapes[0].mesh.positions,
            metres.parts[0].shapes[0].mesh.positions
        );
    }
    #[test]
    fn refuses_ambiguous_colour_identity_metadata_and_boundary_without_mutation() {
        let (source, model) = fixture();
        for mutation in 0..5 {
            let mut candidate = model.clone();
            match mutation {
                0 => candidate.parts[0].shapes[0].triangle_colours[1] = None,
                1 => candidate.parts[0].shapes[0].mesh.positions[1][0] += 0.1,
                2 => candidate.parts[0].shapes[0].triangle_faces.clear(),
                3 => {
                    let shape = candidate.parts[0].shapes[0].clone();
                    candidate.parts[0].shapes.push(shape);
                }
                _ => candidate.parts[0].representation = Some(999),
            }
            let before = format!("{candidate:?}");
            assert!(apply(&source, &candidate, 75, 82, 1e-9).is_err());
            assert_eq!(format!("{candidate:?}"), before);
        }
    }
}
