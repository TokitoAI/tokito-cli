use tokito_cli::model_mesh::{MeshLimits, ModelUnits, parse_vrml_mesh};

fn parse(geometry: &str) -> Result<tokito_cli::model_mesh::ModelMesh, tokito_cli::EngineError> {
    parse_vrml_mesh(
        format!("#VRML V2.0 utf8\nShape {{ geometry IndexedFaceSet {{ {geometry} }} }}").as_bytes(),
        ModelUnits::Millimetres,
        MeshLimits::default(),
    )
}

fn assert_vec(actual: [f64; 3], expected: [f64; 3]) {
    for axis in 0..3 {
        assert!(
            (actual[axis] - expected[axis]).abs() < 1e-12,
            "{actual:?} != {expected:?}"
        );
    }
}

const TRIANGLE: &str = "coord Coordinate { point [0 0 0, 1 0 0, 0 1 0] } coordIndex [0 1 2 -1]";
const THREE_NORMALS: &str = "normal Normal { vector [0 0 1, 0 1 0, 1 0 0] }";

#[test]
fn explicit_per_vertex_defaults_to_coordinate_indices_independent_of_field_order() {
    let mesh = parse(&format!("normalPerVertex TRUE {THREE_NORMALS} {TRIANGLE}")).unwrap();
    let corners = &mesh.parts[0].corner_normals[0];
    assert_vec(corners[0], [0.0, 0.0, 1.0]);
    assert_vec(corners[1], [0.0, 1.0, 0.0]);
    assert_vec(corners[2], [1.0, 0.0, 0.0]);
    assert!(mesh.diagnostics[1].contains("Explicit inline VRML97 Normal vectors"));
}

#[test]
fn explicit_per_vertex_normal_index_maps_authored_vectors() {
    let mesh = parse(&format!(
        "coordIndex [0 1 2 -1] normalIndex [2 0 1 -1] {THREE_NORMALS} coord Coordinate {{ point [0 0 0, 1 0 0, 0 1 0] }}"
    ))
    .unwrap();
    let corners = &mesh.parts[0].corner_normals[0];
    assert_vec(corners[0], [1.0, 0.0, 0.0]);
    assert_vec(corners[1], [0.0, 0.0, 1.0]);
    assert_vec(corners[2], [0.0, 1.0, 0.0]);
}

#[test]
fn polygon_triangulation_keeps_explicit_corner_normals_bound_to_source_vertices() {
    let source = format!(
        "coord Coordinate {{ point [0 0 0, 1 0 0, 1 1 0, 0 1 0] }} coordIndex [0 1 2 3 -1] {THREE_NORMALS} normalPerVertex TRUE normalIndex [2 1 0 2 -1]"
    );
    let mesh = parse(&source).unwrap();
    assert_eq!(mesh.parts[0].triangles.len(), 2);
    for (triangle, corner_normals) in mesh.parts[0]
        .triangles
        .iter()
        .zip(&mesh.parts[0].corner_normals)
    {
        for (coordinate, normal) in triangle.iter().zip(corner_normals) {
            let expected = match coordinate {
                0 | 3 => [1.0, 0.0, 0.0],
                1 => [0.0, 1.0, 0.0],
                2 => [0.0, 0.0, 1.0],
                _ => unreachable!(),
            };
            assert_vec(*normal, expected);
        }
    }
}

const TWO_FACES: &str =
    "coord Coordinate { point [0 0 0, 1 0 0, 0 1 0, 0 0 1] } coordIndex [0 1 2 -1, 1 0 3 -1]";

#[test]
fn explicit_per_face_defaults_to_source_face_order_and_accepts_explicit_indices() {
    let defaults = parse(&format!(
        "{TWO_FACES} normalPerVertex FALSE normal Normal {{ vector [0 0 1, 0 1 0] }}"
    ))
    .unwrap();
    assert!(
        defaults.parts[0].corner_normals[0]
            .iter()
            .all(|normal| *normal == [0.0, 0.0, 1.0])
    );
    assert!(
        defaults.parts[0].corner_normals[1]
            .iter()
            .all(|normal| *normal == [0.0, 1.0, 0.0])
    );

    let indexed = parse(&format!(
        "normalIndex [1 0] normalPerVertex FALSE normal Normal {{ vector [0 0 1, 0 1 0] }} {TWO_FACES}"
    ))
    .unwrap();
    assert!(
        indexed.parts[0].corner_normals[0]
            .iter()
            .all(|normal| *normal == [0.0, 1.0, 0.0])
    );
    assert!(
        indexed.parts[0].corner_normals[1]
            .iter()
            .all(|normal| *normal == [0.0, 0.0, 1.0])
    );
}

#[test]
fn explicit_per_face_polygon_normal_is_repeated_across_earcut_triangles() {
    let mesh = parse(
        "coord Coordinate { point [0 0 0, 1 0 0, 1 1 0, 0 1 0] } coordIndex [0 1 2 3 -1] normalPerVertex FALSE normal Normal { vector [0 0 1, 0 1 0] } normalIndex [1]",
    )
    .unwrap();
    assert_eq!(mesh.parts[0].triangles.len(), 2);
    for triangle_normals in &mesh.parts[0].corner_normals {
        assert!(
            triangle_normals
                .iter()
                .all(|normal| *normal == [0.0, 1.0, 0.0])
        );
    }
}

#[test]
fn ccw_false_keeps_per_vertex_normal_attached_to_its_coordinate() {
    let mesh = parse(&format!(
        "{TRIANGLE} ccw FALSE {THREE_NORMALS} normalPerVertex TRUE"
    ))
    .unwrap();
    assert_eq!(mesh.parts[0].triangles[0], [0, 2, 1]);
    let corners = mesh.parts[0].corner_normals[0];
    assert_vec(corners[0], [0.0, 0.0, 1.0]);
    assert_vec(corners[1], [1.0, 0.0, 0.0]);
    assert_vec(corners[2], [0.0, 1.0, 0.0]);
}

#[test]
fn normal_null_uses_existing_generated_crease_angle_path() {
    let mesh = parse(&format!("{TRIANGLE} normal NULL creaseAngle 1.0")).unwrap();
    assert!(
        mesh.parts[0].corner_normals[0]
            .iter()
            .all(|normal| *normal == mesh.parts[0].flat_normals[0])
    );
}

#[test]
fn malformed_explicit_normal_data_is_rejected() {
    for geometry in [
        format!("{TRIANGLE} {THREE_NORMALS} normalIndex [0 1 -1]"),
        format!("{TRIANGLE} {THREE_NORMALS} normalIndex [0 1 3 -1]"),
        format!("{TRIANGLE} {THREE_NORMALS} normalIndex [0 1 2]"),
        format!(
            "{TRIANGLE} normalPerVertex FALSE normal Normal {{ vector [0 0 1] }} normalIndex [1]"
        ),
        format!(
            "{TRIANGLE} normalPerVertex FALSE normal Normal {{ vector [0 0 1] }} normalIndex [0 0]"
        ),
        format!("{TRIANGLE} normal Normal {{ vector [0 0 0] }}"),
        format!("{TRIANGLE} normal Normal {{ vector [0 0 2] }}"),
        format!("{TRIANGLE} normal DEF N Normal {{ vector [0 0 1] }}"),
        format!("{TRIANGLE} normal Normal {{ vector [] }}"),
    ] {
        assert!(
            parse(&geometry).is_err(),
            "accepted malformed geometry: {geometry}"
        );
    }
}
