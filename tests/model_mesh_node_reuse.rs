use tokito_cli::model_mesh::{MeshLimits, ModelUnits, parse_vrml_mesh};

const HEADER: &str = "#VRML V2.0 utf8\n";
const TRIANGLE: &str = "coord Coordinate { point [0 0 0, 1 0 0, 0 1 0] } coordIndex [0 1 2 -1]";

fn parse(body: &str) -> Result<tokito_cli::model_mesh::ModelMesh, tokito_cli::EngineError> {
    parse_vrml_mesh(
        format!("{HEADER}{body}").as_bytes(),
        ModelUnits::Millimetres,
        MeshLimits::default(),
    )
}

fn assert_point(actual: [f64; 3], expected: [f64; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }
}

#[test]
fn coordinate_def_use_reuses_source_points_under_distinct_parent_transforms() {
    let source = "Shape { geometry IndexedFaceSet { coord DEF P Coordinate { point [0 0 0, 1 0 0, 0 1 0] } coordIndex [0 1 2 -1] } }\nTransform { translation 2 3 4 children [ Shape { geometry IndexedFaceSet { coord USE P coordIndex [0 1 2 -1] } } ] }";
    let mesh = parse(source).unwrap();
    assert_eq!(mesh.root_shape_count, 2);
    assert_eq!(mesh.parts.len(), 2);
    assert_point(mesh.parts[0].positions_mm[0], [0.0, 0.0, 0.0]);
    assert_point(mesh.parts[1].positions_mm[0], [2.0, 3.0, 4.0]);
    assert_point(mesh.parts[1].positions_mm[1], [3.0, 3.0, 4.0]);
}

#[test]
fn normal_def_use_reuses_per_face_authored_normals_and_keeps_face_identity() {
    let source = format!(
        "Shape {{ geometry IndexedFaceSet {{ {TRIANGLE} normalPerVertex FALSE normal DEF N Normal {{ vector [0 0 1] }} }} }}\nShape {{ geometry IndexedFaceSet {{ {TRIANGLE} normalPerVertex FALSE normal USE N }} }}"
    );
    let mesh = parse(&source).unwrap();
    assert_eq!(mesh.root_shape_count, 2);
    assert_eq!(mesh.parts.len(), 2);
    for part in &mesh.parts {
        assert_eq!(part.corner_normals, vec![[[0.0, 0.0, 1.0]; 3]]);
    }
}

#[test]
fn normal_use_vectors_follow_non_uniform_parent_transforms() {
    let source = format!(
        "Shape {{ geometry IndexedFaceSet {{ {TRIANGLE} normalPerVertex FALSE normal DEF N Normal {{ vector [0.7071067811865476 0.7071067811865476 0] }} }} }}\nTransform {{ scale 2 1 1 children [ Shape {{ geometry IndexedFaceSet {{ {TRIANGLE} normalPerVertex FALSE normal USE N }} }} ] }}"
    );
    let mesh = parse(&source).unwrap();
    assert_eq!(mesh.parts.len(), 2);
    for normal in mesh.parts[1].corner_normals[0] {
        assert_point(normal, [0.4472135954999579, 0.8944271909999159, 0.0]);
    }
}

#[test]
fn reused_node_names_are_unique_across_geometry_material_and_scene_nodes() {
    let duplicate_coordinate_normal = "Shape { geometry IndexedFaceSet { coord DEF SHARED Coordinate { point [0 0 0, 1 0 0, 0 1 0] } coordIndex [0 1 2 -1] normalPerVertex FALSE normal DEF SHARED Normal { vector [0 0 1] } } }";
    let error = parse(duplicate_coordinate_normal).unwrap_err().to_string();
    assert!(error.contains("duplicate VRML DEF identifier"), "{error}");

    let duplicate_scene_material = format!(
        "DEF SHARED Shape {{ appearance Appearance {{ material DEF SHARED Material {{}} }} geometry IndexedFaceSet {{ {TRIANGLE} }} }}"
    );
    let error = parse(&duplicate_scene_material).unwrap_err().to_string();
    assert!(error.contains("duplicate VRML DEF identifier"), "{error}");
}

#[test]
fn undefined_and_forward_geometry_use_references_fail_closed() {
    for source in [
        "Shape { geometry IndexedFaceSet { coord USE MISSING coordIndex [0 1 2 -1] } }".to_string(),
        format!("Shape {{ geometry IndexedFaceSet {{ {TRIANGLE} normal USE MISSING }} }}"),
        "Shape { geometry IndexedFaceSet { coord USE P coordIndex [0 1 2 -1] } }\nShape { geometry IndexedFaceSet { coord DEF P Coordinate { point [0 0 0, 1 0 0, 0 1 0] } coordIndex [0 1 2 -1] } }".to_string(),
    ] {
        assert!(
            parse(&source).is_err(),
            "accepted unresolved source: {source}"
        );
    }
}

#[test]
fn coordinate_use_expansion_counts_toward_the_geometry_budget() {
    let source = "Shape { geometry IndexedFaceSet { coord DEF P Coordinate { point [0 0 0, 1 0 0, 0 1 0] } coordIndex [0 1 2 -1] } }\nShape { geometry IndexedFaceSet { coord USE P coordIndex [0 1 2 -1] } }";
    let error = parse_vrml_mesh(
        format!("{HEADER}{source}").as_bytes(),
        ModelUnits::Millimetres,
        MeshLimits {
            max_vertices: 5,
            ..MeshLimits::default()
        },
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("vertex budget exceeded by Coordinate USE"),
        "{error}"
    );
}
