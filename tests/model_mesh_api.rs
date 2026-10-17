use std::path::PathBuf;
use tokito_cli::document::{ModelReference, SourceLocator};
use tokito_cli::model_mesh::{MeshLimits, MeshMaterial, ModelUnits, parse_vrml_mesh};
use tokito_cli::model_resolver::{ModelResolver, ModelResolverConfig};

const TRIANGLE: &str = "#VRML V2.0 utf8\nShape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,1 0 0,0 1 0] } coordIndex [0 1 2 -1] } }";

fn parse(text: &str) -> Result<tokito_cli::model_mesh::ModelMesh, tokito_cli::EngineError> {
    parse_vrml_mesh(
        text.as_bytes(),
        ModelUnits::Millimetres,
        MeshLimits::default(),
    )
}
fn reject(text: &str, reason: &str) {
    let error = parse(text).unwrap_err().to_string();
    assert!(error.contains(reason), "{error} expected {reason}");
}
fn shape_body() -> &'static str {
    TRIANGLE.split_once('\n').unwrap().1
}
fn scene(body: &str) -> String {
    format!("#VRML V2.0 utf8\n{body}")
}
fn assert_point(actual: [f64; 3], expected: [f64; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }
}

fn planar_mesh_area(mesh: &tokito_cli::model_mesh::ModelMesh) -> f64 {
    mesh.parts
        .iter()
        .flat_map(|part| &part.triangles)
        .map(|triangle| {
            let points = triangle.map(|index| mesh.parts[0].positions_mm[index as usize]);
            ((points[1][0] - points[0][0]) * (points[2][1] - points[0][1])
                - (points[1][1] - points[0][1]) * (points[2][0] - points[0][0]))
                .abs()
                * 0.5
        })
        .sum()
}

#[test]
fn planar_vrml_polygon_faces_are_triangulated_with_area_and_winding_preserved() {
    let quad = scene(
        "Shape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,2 0 0,2 2 0,0 2 0] } coordIndex [0 1 2 3 -1] solid FALSE } }",
    );
    let mesh = parse(&quad).unwrap();
    assert_eq!(mesh.parts[0].triangles.len(), 2);
    assert!((planar_mesh_area(&mesh) - 4.0).abs() < 1e-12);
    assert!(
        mesh.parts[0]
            .flat_normals
            .iter()
            .all(|normal| normal[2] > 0.999)
    );
    assert!(!mesh.parts[0].solid);

    let concave = scene(
        "Shape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,2 0 0,2 1 0,1 1 0,1 2 0,0 2 0] } coordIndex [0 1 2 3 4 5 -1] } }",
    );
    let mesh = parse(&concave).unwrap();
    assert_eq!(mesh.parts[0].triangles.len(), 4);
    assert!((planar_mesh_area(&mesh) - 3.0).abs() < 1e-12);

    let collinear = scene(
        "Shape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,1 0 0,2 0 0,2 1 0,0 1 0] } coordIndex [0 1 2 3 4 -1] } }",
    );
    let mesh = parse(&collinear).unwrap();
    assert_eq!(mesh.parts[0].triangles.len(), 3);
    assert!((planar_mesh_area(&mesh) - 2.0).abs() < 1e-12);

    let reversed = parse(&quad.replace("solid FALSE", "ccw FALSE solid FALSE")).unwrap();
    assert!(
        reversed.parts[0]
            .flat_normals
            .iter()
            .all(|normal| normal[2] < -0.999)
    );
}

#[test]
fn polygon_vrml_faces_reject_nonplanar_self_intersection_and_over_budget_geometry() {
    reject(
        &scene(
            "Shape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,2 0 0,2 1 0.1,0 1 0] } coordIndex [0 1 2 3 -1] } }",
        ),
        "non-planar",
    );
    reject(
        &scene(
            "Shape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,1 1 0,0 1 0,1 0 0] } coordIndex [0 1 2 3 -1] } }",
        ),
        "self-intersecting or self-touching",
    );
    // This crossing quadrilateral has non-zero signed area; area conservation
    // alone is not enough to show that the source ring is simple.
    reject(
        &scene(
            "Shape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,3 0 0,0 2 0,2 2 0] } coordIndex [0 1 2 3 -1] } }",
        ),
        "self-intersecting or self-touching",
    );
    reject(
        &scene(
            "Shape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,2 0 0,1 0 0,1 1 0] } coordIndex [0 1 2 3 -1] } }",
        ),
        "overlapping adjacent edges",
    );
    let quad = scene(
        "Shape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,2 0 0,2 2 0,0 2 0] } coordIndex [0 1 2 3 -1] } }",
    );
    let error = parse_vrml_mesh(
        quad.as_bytes(),
        ModelUnits::Millimetres,
        MeshLimits {
            max_triangles: 1,
            ..MeshLimits::default()
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("triangle budget"), "{error}");
}

#[test]
fn polygon_triangulation_work_budget_refuses_quadratic_work_before_tessellation() {
    let vertices = 3_000;
    let points = (0..vertices)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / vertices as f64;
            format!("{:.8} {:.8} 0", angle.cos(), angle.sin())
        })
        .collect::<Vec<_>>()
        .join(",");
    let indices = (0..vertices)
        .map(|index| index.to_string())
        .chain(std::iter::once("-1".to_owned()))
        .collect::<Vec<_>>()
        .join(" ");
    let input = scene(&format!(
        "Shape {{ geometry IndexedFaceSet {{ coord Coordinate {{ point [{points}] }} coordIndex [{indices}] }} }}"
    ));
    let error = parse(&input).unwrap_err().to_string();
    assert!(
        error.contains("polygon triangulation work budget"),
        "{error}"
    );
}

#[test]
fn vrml_scene_def_use_reuses_geometry_under_a_new_parent_transform() {
    let text = scene(&format!(
        "DEF TRIANGLE {}\nTransform {{ translation 2 0 0 children [ USE TRIANGLE ] }}",
        shape_body()
    ));
    let mesh = parse(&text).unwrap();
    assert_eq!(mesh.root_shape_count, 2);
    assert_eq!(mesh.parts.len(), 2);
    assert_eq!(mesh.parts[0].source_shape_ordinal, 0);
    assert_eq!(mesh.parts[1].source_shape_ordinal, 1);
    assert_point(mesh.parts[0].positions_mm[0], [0.0, 0.0, 0.0]);
    assert_point(mesh.parts[1].positions_mm[0], [2.0, 0.0, 0.0]);
}

#[test]
fn vrml_scene_def_use_enforces_expansion_budgets_and_identifier_rules() {
    let reused = scene(&format!("DEF TRIANGLE {}\nUSE TRIANGLE", shape_body()));
    let small_budget = MeshLimits {
        max_triangles: 1,
        ..MeshLimits::default()
    };
    let error = parse_vrml_mesh(reused.as_bytes(), ModelUnits::Millimetres, small_budget)
        .unwrap_err()
        .to_string();
    assert!(error.contains("triangle budget exceeded by scene USE"));
    reject(
        &scene("DEF MISSING_USE Shape {}\nUSE UNKNOWN"),
        "undefined scene USE",
    );
    reject(
        &scene("DEF DUPLICATE Shape {}\nDEF DUPLICATE Shape {}"),
        "duplicate VRML DEF identifier",
    );
    reject(
        &scene(&format!(
            "DEF DUPLICATE Group {{ children [ DEF DUPLICATE {} ] }}",
            shape_body()
        )),
        "duplicate VRML DEF identifier",
    );
    reject(
        &scene(
            "DEF COLLISION Shape { appearance Appearance { material DEF COLLISION Material {} } geometry IndexedFaceSet { coord Coordinate { point [0 0 0,1 0 0,0 1 0] } coordIndex [0 1 2 -1] } }",
        ),
        "duplicate VRML DEF identifier",
    );
    reject(
        &scene("DEF 1BAD Shape {}"),
        "unsupported scene DEF identifier",
    );
}

#[test]
fn authentic_pinned_kicad_resistor_has_verified_mesh_and_materials() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/open-source/kicad-resistor-models");
    let model = ModelReference {
        source: SourceLocator {
            kind: "model".into(),
            ordinal: 0,
            component_ordinal: None,
            source_uuid: None,
            source_path: vec![],
        },
        path: Some("Resistor_SMD.3dshapes/R_0603_1608Metric.wrl".into()),
        offset: None,
        legacy_at: None,
        rotation_deg: None,
        scale: None,
        hidden: false,
    };
    let resolver = ModelResolver::new(ModelResolverConfig::new(root)).unwrap();
    let resolution = resolver.resolve(&model);
    let asset = resolution.asset.unwrap();
    assert_eq!(asset.bytes().len(), 14831);
    assert_eq!(
        asset.sha256(),
        "eadfec9e71c67b1dd3910c72b45d811a06f0ab5397d0609354b10e9a3a4c1fdd"
    );
    let mesh = parse_vrml_mesh(
        asset.bytes(),
        ModelUnits::KiCadLegacyWrlDeciinch,
        MeshLimits::default(),
    )
    .unwrap();
    assert_eq!(mesh.root_shape_count, 29);
    assert_eq!(mesh.parts.len(), 26);
    assert_eq!(mesh.material_definitions.len(), 3);
    assert_eq!(
        mesh.parts
            .iter()
            .map(|p| p.positions_mm.len())
            .sum::<usize>(),
        328
    );
    assert_eq!(
        mesh.parts.iter().map(|p| p.triangles.len()).sum::<usize>(),
        276
    );
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for part in &mesh.parts {
        assert!(part.material.is_some());
        assert_eq!(part.source_crease_angle_radians, 0.5);
        assert_eq!(part.triangles.len(), part.flat_normals.len());
        for normal in &part.flat_normals {
            let length = normal[0].hypot(normal[1]).hypot(normal[2]);
            assert!((length - 1.0).abs() < 1e-12);
        }
        for point in &part.positions_mm {
            for axis in 0..3 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
    }
    for (actual, expected) in min.into_iter().zip([-0.315 * 2.54, -0.157 * 2.54, 0.0]) {
        assert!((actual - expected).abs() < 1e-12);
    }
    for (actual, expected) in max
        .into_iter()
        .zip([0.315 * 2.54, 0.157 * 2.54, 0.177 * 2.54])
    {
        assert!((actual - expected).abs() < 1e-12);
    }
    let pin = &mesh.material_definitions["PIN-01"];
    assert_eq!(pin.diffuse_color, [0.824, 0.82, 0.781]);
    assert_eq!(pin.transparency, 0.0);
    assert_eq!(mesh.parts[0].material.as_ref(), Some(pin));
    assert!(
        mesh.diagnostics
            .iter()
            .any(|d| d.contains("smooth shading"))
    );
}
#[test]
fn explicit_units_control_conversion_without_filename_or_size_heuristics() {
    let mm = parse(TRIANGLE).unwrap();
    let legacy = parse_vrml_mesh(
        TRIANGLE.as_bytes(),
        ModelUnits::KiCadLegacyWrlDeciinch,
        MeshLimits::default(),
    )
    .unwrap();
    assert_eq!(mm.parts[0].positions_mm[1], [1.0, 0.0, 0.0]);
    assert_eq!(legacy.parts[0].positions_mm[1], [2.54, 0.0, 0.0]);
    assert_eq!(mm.parts[0].material, None);
    assert_eq!(mm.parts[0].flat_normals, vec![[0.0, 0.0, 1.0]]);
}
#[test]
fn clockwise_sources_are_normalized_and_solid_is_retained() {
    let input = TRIANGLE.replace("coordIndex", "ccw FALSE solid FALSE coordIndex");
    let mesh = parse(&input).unwrap();
    assert_eq!(mesh.parts[0].triangles, vec![[0, 2, 1]]);
    assert_eq!(mesh.parts[0].flat_normals, vec![[0.0, 0.0, -1.0]]);
    assert!(!mesh.parts[0].solid);
}
#[test]
fn malformed_geometry_fails_closed() {
    let cases = [
        (TRIANGLE.replace("0 1 2 -1", "0 1 3 -1"), "out of range"),
        (TRIANGLE.replace("0 1 2 -1", "0 1 2"), "terminator"),
        (
            TRIANGLE.replace("0 1 2 -1", "0 1 2 0 -1"),
            "repeats a coordinate index",
        ),
        (TRIANGLE.replace("0 1 2 -1", "0 1 -1"), "triangular"),
        (
            TRIANGLE.replace("0 1 2 -1", "0 1 -2 -1"),
            "invalid coordinate",
        ),
        (TRIANGLE.replace("0 1 2 -1", "0 1 2.0 -1"), "integers"),
        (
            TRIANGLE.replace("0 1 2 -1", "0 1 1 -1"),
            "repeats a coordinate index",
        ),
        (TRIANGLE.replace("0 1 0", "2 0 0"), "degenerate"),
        (TRIANGLE.replace("1 0 0", "NaN 0 0"), "nonfinite"),
        (TRIANGLE.replace("1 0 0", "1e999 0 0"), "nonfinite"),
        (TRIANGLE.replace("point [", "point [0 "), "invalid number"),
        (
            TRIANGLE.replace("coordIndex [0 1 2 -1]", "coordIndex []"),
            "empty",
        ),
        (TRIANGLE.replace("} }", "}"), "unclosed"),
        (TRIANGLE.replace("point [", "point {"), "unbalanced"),
        (
            TRIANGLE.replace(
                "coordIndex",
                "coord Coordinate { point [0 0 0] } coordIndex",
            ),
            "duplicate",
        ),
    ];
    for (input, reason) in cases {
        reject(&input, reason);
    }
}
#[test]
fn unsupported_vrml_nodes_fields_and_external_resources_are_rejected() {
    for input in [
        "#VRML V2.0 utf8\nTransform { children [] }",
        "#VRML V2.0 utf8\nInline { url [\"https://example.org/model.wrl\"] }",
        "#VRML V2.0 utf8\nGroup { children [] }",
        "#VRML V2.0 utf8\nPROTO MyNode [] {}",
    ] {
        assert!(parse(input).is_err());
    }
    reject(
        &TRIANGLE.replace("coordIndex", "normal Normal { vector [0 0 1] } coordIndex"),
        "unsupported",
    );
    reject(
        &TRIANGLE.replace(
            "coordIndex",
            "texCoord TextureCoordinate { point [0 0] } coordIndex",
        ),
        "unsupported",
    );
    reject(&TRIANGLE.replace("IndexedFaceSet", "Box"), "IndexedFaceSet");
    reject(
        &TRIANGLE.replace("coord Coordinate", "coord DEF C Coordinate"),
        "Coordinate",
    );
    reject(
        "#VRML V2.0 utf8\nShape { appearance Appearance { texture ImageTexture {} } }",
        "unsupported",
    );
    reject(
        &TRIANGLE.replace("coordIndex", "creaseAngle -0.1 coordIndex"),
        "negative",
    );
    reject(
        &TRIANGLE.replace("coordIndex", "ccw true coordIndex"),
        "TRUE",
    );
}

#[test]
fn groups_preserve_flat_shape_geometry_and_assign_shape_ordinals() {
    let grouped = scene(&format!(
        "Group {{ children [ Shape {{ appearance Appearance {{ material Material {{ diffuseColor 0.2 0.3 0.4 }} }} }} {shape} ] }}",
        shape = shape_body()
    ));
    let mesh = parse(&grouped).unwrap();
    assert_eq!(mesh.root_shape_count, 2);
    assert_eq!(mesh.parts.len(), 1);
    assert_eq!(mesh.parts[0].source_shape_ordinal, 1);
    assert_eq!(
        mesh.parts[0].positions_mm,
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
    );
    assert_eq!(mesh.parts[0].flat_normals, vec![[0.0, 0.0, 1.0]]);
}

#[test]
fn transform_applies_translation_rotation_and_center_when_children_precede_fields() {
    let input = scene(&format!(
        "Transform {{ children [ {shape} ] center 1 0 0 translation 2 3 0 rotation 0 0 1 1.5707963267948966 }}",
        shape = shape_body()
    ));
    let mesh = parse(&input).unwrap();
    let points = &mesh.parts[0].positions_mm;
    assert_point(points[0], [3.0, 2.0, 0.0]);
    assert_point(points[1], [3.0, 3.0, 0.0]);
    assert_point(points[2], [2.0, 2.0, 0.0]);
    assert_point(mesh.parts[0].flat_normals[0], [0.0, 0.0, 1.0]);
}

#[test]
fn transform_applies_scale_orientation_in_vrml97_matrix_order() {
    let input = scene(&format!(
        "Transform {{ children [ {shape} ] center 1 2 0 translation 3 4 0 scale 2 1 1 scaleOrientation 0 0 2 1.5707963267948966 rotation 0 0 1 1.5707963267948966 }}",
        shape = shape_body()
    ));
    let mesh = parse(&input).unwrap();
    let points = &mesh.parts[0].positions_mm;
    assert_point(points[0], [8.0, 5.0, 0.0]);
    assert_point(points[1], [8.0, 6.0, 0.0]);
    assert_point(points[2], [6.0, 5.0, 0.0]);
    assert_eq!(mesh.parts[0].triangles, vec![[0, 1, 2]]);
    assert_point(mesh.parts[0].flat_normals[0], [0.0, 0.0, 1.0]);

    // The default identity scale orientation must preserve ordinary scaling.
    let identity = scene(&format!(
        "Transform {{ children [ {shape} ] scale 2 1 1 scaleOrientation 0 0 1 0 }}",
        shape = shape_body()
    ));
    let identity = parse(&identity).unwrap();
    assert_point(identity.parts[0].positions_mm[1], [2.0, 0.0, 0.0]);
}

#[test]
fn nested_group_and_transform_compose_and_scale_translation_uses_declared_units() {
    let input = scene(&format!(
        "Transform {{ children [ Group {{ children [ Transform {{ children [ {shape} ] scale 2 1 1 translation 0 4 0 }} ] }} ] translation 10 0 0 }}",
        shape = shape_body()
    ));
    let mesh = parse(&input).unwrap();
    let points = &mesh.parts[0].positions_mm;
    assert_point(points[0], [10.0, 4.0, 0.0]);
    assert_point(points[1], [12.0, 4.0, 0.0]);
    assert_point(points[2], [10.0, 5.0, 0.0]);

    let legacy = scene(&format!(
        "Transform {{ children [ {shape} ] translation 1 0 0 }}",
        shape = shape_body()
    ));
    let legacy = parse_vrml_mesh(
        legacy.as_bytes(),
        ModelUnits::KiCadLegacyWrlDeciinch,
        MeshLimits::default(),
    )
    .unwrap();
    assert_point(legacy.parts[0].positions_mm[0], [2.54, 0.0, 0.0]);
}

#[test]
fn negative_nonzero_scale_transforms_geometry_and_recomputes_face_normal() {
    let input = scene(&format!(
        "Transform {{ children [ {shape} ] scale -1 1 1 }}",
        shape = shape_body()
    ));
    let mesh = parse(&input).unwrap();
    assert_point(mesh.parts[0].positions_mm[1], [-1.0, 0.0, 0.0]);
    assert_eq!(mesh.parts[0].triangles, vec![[0, 2, 1]]);
    assert_point(mesh.parts[0].flat_normals[0], [0.0, 0.0, 1.0]);
}

#[test]
fn transforms_fail_closed_on_singular_or_unsupported_fields_and_nodes() {
    for body in [
        format!("Transform {{ children [ {} ] scale 1 0 1 }}", shape_body()),
        format!(
            "Transform {{ children [ {} ] rotation 0 0 0 1 }}",
            shape_body()
        ),
        format!(
            "Transform {{ children [ {} ] scaleOrientation 0 0 0 1 }}",
            shape_body()
        ),
        format!(
            "Transform {{ children [ {} ] bboxCenter 0 0 0 }}",
            shape_body()
        ),
        "Group { children [ Inline { url [] } ] }".to_string(),
    ] {
        assert!(parse(&scene(&body)).is_err(), "accepted {body}");
    }
    let limited = scene(&format!("Group {{ children [ {} ] }}", shape_body()));
    let error = parse_vrml_mesh(
        limited.as_bytes(),
        ModelUnits::Millimetres,
        MeshLimits {
            max_shapes: 0,
            ..MeshLimits::default()
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("shape budget"));
}
#[test]
fn material_definitions_references_defaults_and_ranges_are_checked() {
    let declaration = "Shape { appearance Appearance { material DEF M Material { diffuseColor 0.1 0.2 0.3 transparency 0.25 } } }";
    let use_material = TRIANGLE.replace(
        "Shape {",
        "Shape { appearance Appearance { material USE M }",
    );
    let input = format!(
        "#VRML V2.0 utf8\n{declaration}\n{}",
        use_material.split_once('\n').unwrap().1
    );
    let mesh = parse(&input).unwrap();
    assert_eq!(mesh.parts[0].material.as_ref().unwrap().transparency, 0.25);
    assert_eq!(
        mesh.parts[0].material.as_ref().unwrap().shininess,
        MeshMaterial::default().shininess
    );
    reject(&use_material, "undefined");
    reject(
        &input.replace(declaration, &format!("{declaration}\n{declaration}")),
        "duplicate material",
    );
    reject(
        &input.replace("transparency 0.25", "transparency 1.1"),
        "outside",
    );
    reject(
        &input.replace("transparency 0.25", "transparency 0.25 transparency 0"),
        "duplicate",
    );
    reject(&input.replace("diffuseColor", "unknown"), "unsupported");
    reject(
        "#VRML V2.0 utf8\nShape { appearance Appearance { material Material {} } }",
        "no nonempty",
    );
}
#[test]
fn every_input_budget_is_enforced() {
    let low = [
        MeshLimits {
            max_bytes: 1,
            ..MeshLimits::default()
        },
        MeshLimits {
            max_tokens: 1,
            ..MeshLimits::default()
        },
        MeshLimits {
            max_depth: 1,
            ..MeshLimits::default()
        },
        MeshLimits {
            max_shapes: 0,
            ..MeshLimits::default()
        },
        MeshLimits {
            max_vertices: 2,
            ..MeshLimits::default()
        },
        MeshLimits {
            max_triangles: 0,
            ..MeshLimits::default()
        },
    ];
    for limits in low {
        assert!(
            parse_vrml_mesh(TRIANGLE.as_bytes(), ModelUnits::Millimetres, limits)
                .unwrap_err()
                .to_string()
                .contains("budget")
        );
    }
    let material = TRIANGLE.replace(
        "Shape {",
        "Shape { appearance Appearance { material DEF M Material {} }",
    );
    let error = parse_vrml_mesh(
        material.as_bytes(),
        ModelUnits::Millimetres,
        MeshLimits {
            max_material_definitions: 0,
            ..MeshLimits::default()
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("budget"));
}
#[test]
fn header_encoding_and_conversion_overflow_are_rejected() {
    reject(
        &TRIANGLE.replace("#VRML V2.0 utf8", "#VRML V1.0 ascii"),
        "header",
    );
    assert!(parse_vrml_mesh(&[0xff], ModelUnits::Millimetres, MeshLimits::default()).is_err());
    let huge = TRIANGLE.replace("1 0 0", "1e308 0 0");
    let error = parse_vrml_mesh(
        huge.as_bytes(),
        ModelUnits::KiCadLegacyWrlDeciinch,
        MeshLimits::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("overflow"));
}

#[test]
fn callers_cannot_disable_hard_resource_ceilings() {
    let excessive = [
        MeshLimits {
            max_bytes: usize::MAX,
            ..MeshLimits::default()
        },
        MeshLimits {
            max_tokens: usize::MAX,
            ..MeshLimits::default()
        },
        MeshLimits {
            max_depth: usize::MAX,
            ..MeshLimits::default()
        },
        MeshLimits {
            max_shapes: usize::MAX,
            ..MeshLimits::default()
        },
        MeshLimits {
            max_material_definitions: usize::MAX,
            ..MeshLimits::default()
        },
        MeshLimits {
            max_vertices: usize::MAX,
            ..MeshLimits::default()
        },
        MeshLimits {
            max_triangles: usize::MAX,
            ..MeshLimits::default()
        },
    ];
    for limits in excessive {
        let error =
            parse_vrml_mesh(TRIANGLE.as_bytes(), ModelUnits::Millimetres, limits).unwrap_err();
        assert!(error.to_string().contains("hard ceiling"));
    }
}

#[test]
fn diagnostics_are_bounded_and_do_not_echo_source_tokens() {
    let private = "PRIVATE_Customer_Token_41";
    let samples = [
        format!("#VRML V2.0 utf8\n{private} {{}}"),
        TRIANGLE.replace("1 0 0", &format!("{private} 0 0")),
        TRIANGLE.replace("coordIndex", &format!("{private} 0 coordIndex")),
        TRIANGLE.replace("Shape {", &format!("Shape {{ {private} 0")),
        TRIANGLE.replace(
            "Shape {",
            &format!("Shape {{ appearance Appearance {{ {private} 0 }}"),
        ),
        TRIANGLE.replace(
            "Shape {",
            &format!("Shape {{ appearance Appearance {{ material {private} {{}} }}"),
        ),
        TRIANGLE.replace(
            "Shape {",
            &format!("Shape {{ appearance Appearance {{ material USE {private} }}"),
        ),
        TRIANGLE.replace(
            "Shape {",
            &format!("Shape {{ appearance Appearance {{ material Material {{ {private} 0 }} }}"),
        ),
        format!("#VRML V2.0 utf8\nShape {{ {private} 0 {private} 0 }}"),
    ];
    for input in samples {
        let error = parse(&input).unwrap_err().to_string();
        assert!(!error.contains(private), "source token leaked: {error}");
        assert!(error.len() <= 120, "unbounded diagnostic: {error}");
    }
    let declaration =
        format!("Shape {{ appearance Appearance {{ material DEF {private} Material {{}} }} }}");
    let duplicate_definition = format!(
        "#VRML V2.0 utf8\n{declaration}\n{declaration}\n{}",
        TRIANGLE.split_once('\n').unwrap().1
    );
    let duplicate_error = parse(&duplicate_definition).unwrap_err().to_string();
    assert!(duplicate_error.contains("duplicate material DEF"));
    assert!(!duplicate_error.contains(private));
    assert!(duplicate_error.len() <= 120);
    let long = format!("#VRML V2.0 utf8\n{private}{}", "X".repeat(10_000));
    let error = parse(&long).unwrap_err().to_string();
    assert!(error.contains("token byte budget"));
    assert!(!error.contains(private));
    assert!(error.len() <= 120);
}

#[test]
fn hard_token_limit_accepts_256_bytes_and_rejects_257() {
    let material_with_name = |size| {
        TRIANGLE.replace(
            "Shape {",
            &format!(
                "Shape {{ appearance Appearance {{ material DEF {} Material {{}} }}",
                "M".repeat(size)
            ),
        )
    };
    assert!(parse(&material_with_name(256)).is_ok());
    reject(&material_with_name(257), "token byte budget");
}
