use tokito_cli::component_pose::{
    BoardSide, KICAD_STEP_BOARD_OFFSET_MM, KicadStepPlacement, ModelTransformMm,
    place_model_mesh_in_glb_m, place_model_point_in_glb_m,
};
use tokito_cli::model_mesh::{MeshLimits, ModelUnits, parse_vrml_mesh};

const TRIANGLE: &str = "#VRML V2.0 utf8\nShape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,1 0 0,0 1 0] } coordIndex [0 1 2 -1] } }";

fn identity_model() -> ModelTransformMm {
    ModelTransformMm {
        scale: [1.0; 3],
        rotation_deg: [0.0; 3],
        offset_mm: [0.0; 3],
    }
}

fn placement(side: BoardSide) -> KicadStepPlacement {
    KicadStepPlacement {
        board_position_mm: [0.0; 2],
        board_origin_mm: [0.0; 2],
        footprint_rotation_deg: 0.0,
        side,
        front_copper_surface_z_mm: 0.8,
        back_copper_surface_z_mm: -0.8,
        model: identity_model(),
    }
}

fn assert_point(actual: [f64; 3], expected: [f64; 3]) {
    for axis in 0..3 {
        assert!(
            (actual[axis] - expected[axis]).abs() < 1e-12,
            "axis {axis}: actual {:?}, expected {:?}",
            actual,
            expected
        );
    }
}

fn triangle_mesh() -> tokito_cli::model_mesh::ModelMesh {
    parse_vrml_mesh(
        TRIANGLE.as_bytes(),
        ModelUnits::Millimetres,
        MeshLimits::default(),
    )
    .expect("valid triangle mesh")
}

#[test]
fn front_pose_matches_asymmetric_hand_calculated_xyz_oracle() {
    let input = KicadStepPlacement {
        board_position_mm: [100.0, -50.0],
        footprint_rotation_deg: 90.0,
        front_copper_surface_z_mm: 1.6,
        model: ModelTransformMm {
            scale: [2.0, 3.0, 4.0],
            rotation_deg: [90.0, 90.0, 90.0],
            offset_mm: [10.0, 20.0, 30.0],
        },
        ..placement(BoardSide::Front)
    };

    // Independent sequential arithmetic:
    // scale (1,2,3)->(2,6,12); -X/-Y/-Z rotations -> (12,-6,2);
    // offset/plane/clearance -> (22,14,33.65); footprint +90 -> (-14,22,33.65).
    assert_point(
        place_model_point_in_glb_m([1.0, 2.0, 3.0], input).unwrap(),
        [0.086, 0.03365, 0.072],
    );
}

#[test]
fn back_pose_flips_about_x_and_maps_origin_to_the_explicit_back_plane() {
    let input = KicadStepPlacement {
        board_position_mm: [10.0, 20.0],
        board_origin_mm: [0.0, 2.0],
        footprint_rotation_deg: 90.0,
        side: BoardSide::Back,
        model: ModelTransformMm {
            offset_mm: [2.0, 0.0, 1.0],
            ..identity_model()
        },
        ..placement(BoardSide::Back)
    };

    // Point plus model offset/plane/clearance -> (3,2,4.85);
    // X180 then Z90 -> (2,3,-4.85); board origin/translation -> expected GLB point.
    assert_point(
        place_model_point_in_glb_m([1.0, 2.0, 3.0], input).unwrap(),
        [0.012, -0.00485, -0.015],
    );
}

#[test]
fn footprint_translation_uses_the_same_centered_y_down_board_origin_as_shell() {
    let input = KicadStepPlacement {
        board_position_mm: [25.0, -12.0],
        board_origin_mm: [5.0, 2.0],
        front_copper_surface_z_mm: 0.8,
        ..placement(BoardSide::Front)
    };
    assert_point(
        place_model_point_in_glb_m([0.0; 3], input).unwrap(),
        [0.020, (0.8 + KICAD_STEP_BOARD_OFFSET_MM) * 0.001, 0.014],
    );
}

#[test]
fn invalid_model_and_copper_plane_inputs_are_rejected() {
    let mut input = placement(BoardSide::Front);
    input.model.scale[1] = 0.0;
    assert!(
        place_model_point_in_glb_m([1.0; 3], input)
            .unwrap_err()
            .to_string()
            .contains("zero axis")
    );

    let mut input = placement(BoardSide::Front);
    input.front_copper_surface_z_mm = input.back_copper_surface_z_mm;
    assert!(
        place_model_point_in_glb_m([1.0; 3], input)
            .unwrap_err()
            .to_string()
            .contains("above the back")
    );

    let mut input = placement(BoardSide::Front);
    input.model.offset_mm[0] = f64::NAN;
    assert!(
        place_model_point_in_glb_m([1.0; 3], input)
            .unwrap_err()
            .to_string()
            .contains("non-finite")
    );

    let mut input = placement(BoardSide::Front);
    input.model.scale[0] = f64::MAX;
    assert!(
        place_model_point_in_glb_m([f64::MAX; 3], input)
            .unwrap_err()
            .to_string()
            .contains("overflowed")
    );
}

#[test]
fn mesh_placement_emits_metres_and_recomputes_front_face_normals() {
    let mesh = triangle_mesh();
    let placed = place_model_mesh_in_glb_m(&mesh, placement(BoardSide::Front)).unwrap();
    let part = &placed.parts[0];

    assert_point(part.positions_m[0], [0.0, 0.00085, 0.0]);
    assert_point(part.positions_m[1], [0.001, 0.00085, 0.0]);
    assert_point(part.positions_m[2], [0.0, 0.00085, 0.001]);
    assert_eq!(part.triangles, vec![[0, 2, 1]]);
    assert_point(part.flat_normals[0], [0.0, 1.0, 0.0]);
}

#[test]
fn back_mesh_placement_reverses_outward_normal_away_from_board() {
    let mesh = triangle_mesh();
    let placed = place_model_mesh_in_glb_m(&mesh, placement(BoardSide::Back)).unwrap();
    let part = &placed.parts[0];

    assert_eq!(part.triangles, vec![[0, 2, 1]]);
    assert_point(part.positions_m[0], [0.0, -0.00085, 0.0]);
    assert_point(part.positions_m[2], [0.0, -0.00085, -0.001]);
    assert_point(part.flat_normals[0], [0.0, -1.0, 0.0]);
}

#[test]
fn odd_negative_scale_cancels_frame_reflection_and_keeps_winding() {
    let mesh = triangle_mesh();
    let placement = KicadStepPlacement {
        model: ModelTransformMm {
            scale: [-1.0, 1.0, 1.0],
            ..identity_model()
        },
        ..placement(BoardSide::Front)
    };
    let placed = place_model_mesh_in_glb_m(&mesh, placement).unwrap();
    let part = &placed.parts[0];

    assert_eq!(part.triangles, vec![[0, 1, 2]]);
    assert_point(part.flat_normals[0], [0.0, 1.0, 0.0]);
}

#[test]
fn vrml_reflection_keeps_solid_winding_through_model_placement() {
    let shape = TRIANGLE.split_once('\n').unwrap().1;
    let source = format!("#VRML V2.0 utf8\nTransform {{ children [ {shape} ] scale -1 1 1 }}");
    let mesh = parse_vrml_mesh(
        source.as_bytes(),
        ModelUnits::Millimetres,
        MeshLimits::default(),
    )
    .unwrap();
    assert!(mesh.parts[0].solid);
    assert_eq!(mesh.parts[0].triangles, vec![[0, 2, 1]]);
    assert_point(mesh.parts[0].flat_normals[0], [0.0, 0.0, 1.0]);

    let placement = KicadStepPlacement {
        model: ModelTransformMm {
            scale: [-1.0, 1.0, 1.0],
            ..identity_model()
        },
        ..placement(BoardSide::Front)
    };
    let placed = place_model_mesh_in_glb_m(&mesh, placement).unwrap();
    assert!(placed.parts[0].solid);
    assert_eq!(placed.parts[0].triangles, vec![[0, 2, 1]]);
    assert_point(placed.parts[0].flat_normals[0], [0.0, 1.0, 0.0]);
}

#[test]
fn even_negative_scale_preserves_frame_reflection_and_reverses_winding() {
    let mesh = triangle_mesh();
    let placement = KicadStepPlacement {
        model: ModelTransformMm {
            scale: [-1.0, -1.0, 1.0],
            ..identity_model()
        },
        ..placement(BoardSide::Front)
    };
    let placed = place_model_mesh_in_glb_m(&mesh, placement).unwrap();

    assert_eq!(placed.parts[0].triangles, vec![[0, 2, 1]]);
    assert_point(placed.parts[0].flat_normals[0], [0.0, 1.0, 0.0]);
}

#[test]
fn placed_smooth_normals_use_inverse_transpose_for_component_scale_and_glb_basis() {
    let source = "#VRML V2.0 utf8\nShape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,1 0 0,0 1 0,0 0 1] } coordIndex [0 1 2 -1,1 0 3 -1] creaseAngle 2.0 } }";
    let mesh = parse_vrml_mesh(
        source.as_bytes(),
        ModelUnits::Millimetres,
        MeshLimits::default(),
    )
    .unwrap();
    let placement = KicadStepPlacement {
        model: ModelTransformMm {
            scale: [1.0, 2.0, 1.0],
            ..identity_model()
        },
        ..placement(BoardSide::Front)
    };
    let placed = place_model_mesh_in_glb_m(&mesh, placement).unwrap();
    let expected = [0.0, 2.0 / 5.0f64.sqrt(), 1.0 / 5.0f64.sqrt()];
    assert_point(placed.parts[0].corner_normals[0][0], expected);
    assert_point(placed.parts[0].corner_normals[1][0], expected);
}

#[test]
fn placed_explicit_vrml_normals_use_inverse_transpose_for_component_scale() {
    let authored_normal = 1.0 / 2.0_f64.sqrt();
    let source = format!(
        "#VRML V2.0 utf8\nShape {{ geometry IndexedFaceSet {{ coord Coordinate {{ point [0 0 0,1 0 0,0 1 0,0 0 1] }} coordIndex [0 1 2 -1,1 0 3 -1] normalPerVertex TRUE normal Normal {{ vector [0 {authored_normal} {authored_normal},0 {authored_normal} {authored_normal},0 {authored_normal} {authored_normal},0 {authored_normal} {authored_normal}] }} }} }}"
    );
    let mesh = parse_vrml_mesh(
        source.as_bytes(),
        ModelUnits::Millimetres,
        MeshLimits::default(),
    )
    .unwrap();
    let placement = KicadStepPlacement {
        model: ModelTransformMm {
            scale: [1.0, 2.0, 1.0],
            ..identity_model()
        },
        ..placement(BoardSide::Front)
    };
    let placed = place_model_mesh_in_glb_m(&mesh, placement).unwrap();
    let expected = [0.0, 2.0 / 5.0_f64.sqrt(), 1.0 / 5.0_f64.sqrt()];
    for part in &placed.parts {
        for triangle in &part.corner_normals {
            for normal in triangle {
                assert_point(*normal, expected);
            }
        }
    }
}

#[test]
fn mesh_placement_rejects_invalid_triangle_indices_and_degenerate_results() {
    let mut mesh = triangle_mesh();
    mesh.parts[0].triangles[0][2] = 3;
    assert!(
        place_model_mesh_in_glb_m(&mesh, placement(BoardSide::Front))
            .unwrap_err()
            .to_string()
            .contains("index is out of range")
    );

    let mut mesh = triangle_mesh();
    mesh.parts[0].flat_normals.clear();
    assert!(
        place_model_mesh_in_glb_m(&mesh, placement(BoardSide::Front))
            .unwrap_err()
            .to_string()
            .contains("inconsistent triangle geometry")
    );

    let mut mesh = triangle_mesh();
    mesh.parts[0].corner_normals.clear();
    assert!(
        place_model_mesh_in_glb_m(&mesh, placement(BoardSide::Front))
            .unwrap_err()
            .to_string()
            .contains("inconsistent triangle geometry")
    );

    let mut mesh = triangle_mesh();
    mesh.parts[0].positions_mm[2] = [0.5, 0.0, 0.0];
    assert!(
        place_model_mesh_in_glb_m(&mesh, placement(BoardSide::Front))
            .unwrap_err()
            .to_string()
            .contains("degenerate triangle")
    );
}
