//! Synthetic cases test importer semantics only. They are not physical boards,
//! manufacturing inputs, model qualifications or AR alignment evidence.

#[path = "support/test_scratch.rs"]
mod test_scratch;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokito_cli::document::{CoordinateFrame, DiagnosticCode, Point2, SourceLocator};
use tokito_cli::{
    PhysicalBoardDocument, PhysicalUnits, SourceCoordinateFrame, SourceNode, SvgSide,
};

struct TestInput {
    root: PathBuf,
    parent: PathBuf,
    board: PathBuf,
}

impl TestInput {
    fn new(source: &str) -> Self {
        let parent = test_scratch::directory("engine-api-tests");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = (0..100)
            .find_map(|attempt| {
                let candidate = parent.join(format!(
                    "synthetic-test-{}-{stamp}-{attempt}",
                    std::process::id()
                ));
                match fs::create_dir(&candidate) {
                    Ok(()) => Some(candidate),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(error) => panic!("cannot reserve unique test directory: {error}"),
                }
            })
            .expect("unique test directory");
        let board = root.join("synthetic-test-only.kicad_pcb");
        fs::write(&board, source).unwrap();
        Self {
            root,
            parent,
            board,
        }
    }

    fn load(&self) -> PhysicalBoardDocument {
        tokito_cli::load_board_input(&self.board).expect("synthetic parser fixture")
    }
}

impl Drop for TestInput {
    fn drop(&mut self) {
        // Delete only our atomically reserved directory after verifying its
        // resolved absolute parent. No user-provided path is used for cleanup.
        if self
            .root
            .canonicalize()
            .ok()
            .is_some_and(|root| root.parent() == Some(self.parent.as_path()))
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn source_node<'a>(board: &'a PhysicalBoardDocument, source: &SourceLocator) -> &'a SourceNode {
    board
        .source_node(source)
        .expect("canonical record source node")
}

fn value_at(node: &SourceNode, index: usize) -> Option<&str> {
    match node {
        SourceNode::List(items) => match items.get(index)? {
            SourceNode::Atom(value) => Some(value),
            SourceNode::List(_) => None,
        },
        SourceNode::Atom(_) => None,
    }
}

const TEST_BOARD: &str = r#"
(kicad_pcb
  (version 20240108) (generator synthetic-test-only)
  (general (thickness 1.6))
  (layers (0 "F.Cu" signal "Front copper") (31 "B.Cu" signal) (44 "Edge.Cuts" user))
  (net 0 "") (net 1 "VCC") (net 2 "GND")
  (footprint "Test:Diode" (layer "F.Cu") (uuid "component-top")
    (at 10 20 90) (property "Reference" "D1") (property "Value" "TEST_ONLY") (attr smd dnp)
    (pad "1" smd rect (uuid "pad-top") (at 2 3 30) (size 1 2) (layers "F.Cu") (net 1 "VCC"))
    (pad "1" thru_hole oval (at 3 4 60) (size 2 3) (drill oval 0.8 1.2 (offset 0.1 0.2)) (layers "*.Cu") (net 2 "GND"))
    (fp_line (start 0 0) (end 2 0) (stroke (width 0.15) (type solid)) (layer "F.Cu") (uuid "graphic-top"))
    (model "${KIPRJMOD}/not-a-real-test-model.step" (offset (xyz 1 2 3)) (rotate (xyz 0 0 90)) (scale (xyz 1 1 1))))
  (footprint "Test:Diode" (layer "B.Cu") (uuid "component-bottom")
    (at 10 20 90) (property "Reference" "D2") (property "Value" "TEST_ONLY")
    (pad "1" smd rect (uuid "pad-bottom") (at 2 3 30) (size 1 2) (layers "B.Cu") (net 1 "VCC")))
  (segment (start 1 2) (end 3 4) (width 0.25) (layer "F.Cu") (net 1) (uuid "track-line"))
  (arc (start 1 0) (mid 0 1) (end -1 0) (width 0.25) (layer "F.Cu") (net 1) (uuid "track-arc"))
  (via (at 4 5) (size 0.8) (drill 0.4) (layers "F.Cu" "B.Cu") (net 2) (uuid "via-1"))
  (zone (net 2) (net_name "GND") (layer "F.Cu") (uuid "zone-1")
    (polygon (pts (xy 1 1) (xy 9 1) (xy 9 9)))
    (filled_polygon (layer "F.Cu") (pts (xy 2 2) (xy 8 2) (xy 8 8))))
  (gr_rect (start 0 0) (end 30 40) (stroke (width 0.05) (type solid)) (fill none) (layer "Edge.Cuts") (uuid "outline-1")))
"#;

#[test]
fn external_consumer_reads_source_semantics_and_public_geometry() {
    let input = TestInput::new(TEST_BOARD);
    let board = input.load();
    let source = board.source();
    assert_eq!(source.path, input.board);
    assert_eq!(source.bytes, TEST_BOARD.len());
    assert_eq!(source.sha256.len(), 64);
    assert_eq!(source.format_version, 20240108);
    assert_eq!(source.units, PhysicalUnits::Millimetres);
    assert_eq!(
        source.coordinate_frame,
        SourceCoordinateFrame::KiCadBoardYDown
    );
    assert_eq!(board.thickness_mm(), Some(1.6));
    assert!(!board.geometry_complete());
    let data = board.engineering();
    assert_eq!(data.layers.len(), 3);
    assert_eq!(data.layers[0].id, Some(0));
    assert_eq!(
        data.layers[0].display_label.as_deref(),
        Some("Front copper")
    );
    assert_eq!(data.nets[1].name.as_deref(), Some("VCC"));
    assert_eq!(data.components.len(), 2);
    assert_eq!(
        data.components[0].source.source_uuid.as_deref(),
        Some("component-top")
    );
    assert_eq!(
        data.components[0].source_board_pose.position_mm,
        Some(Point2 { x: 10.0, y: 20.0 })
    );
    assert!(
        data.components[0]
            .attributes
            .iter()
            .any(|attribute| attribute == "dnp")
    );
    assert_eq!(data.pads.len(), 3);
    assert_eq!(
        data.pads[0].source_local_pose.position_mm,
        Some(Point2 { x: 2.0, y: 3.0 })
    );
    assert_eq!(data.pads[0].size_mm, Some(Point2 { x: 1.0, y: 2.0 }));
    assert_eq!(
        data.pads[1].drill.as_ref().unwrap().size_mm,
        Some(Point2 { x: 0.8, y: 1.2 })
    );
    assert_eq!(
        data.pads[1].drill.as_ref().unwrap().offset_mm,
        Some(Point2 { x: 0.1, y: 0.2 })
    );
    assert_eq!(
        data.pads[0].net.as_ref().unwrap().resolved_name.as_deref(),
        Some("VCC")
    );
    assert_eq!(data.track_segments.len(), 1);
    assert_eq!(
        data.track_segments[0].start_mm,
        Some(Point2 { x: 1.0, y: 2.0 })
    );
    assert_eq!(data.track_arcs.len(), 1);
    assert_eq!(data.track_arcs[0].mid_mm, Some(Point2 { x: 0.0, y: 1.0 }));
    assert_eq!(data.vias.len(), 1);
    assert_eq!(data.vias[0].drill_mm, Some(0.4));
    assert_eq!(data.zones.len(), 1);
    assert_eq!(data.zones[0].boundary_polygons.len(), 1);
    assert_eq!(data.zones[0].cached_fill_polygons.len(), 1);
    assert!(data.zones[0].boundary_polygons[0].is_complete);
    assert_eq!(data.models.len(), 1);
    assert_eq!(
        data.models[0].path.as_deref(),
        Some("${KIPRJMOD}/not-a-real-test-model.step")
    );
    assert_eq!(data.models[0].offset.unwrap().z, 3.0);
    assert!(
        data.graphics
            .iter()
            .any(|graphic| matches!(graphic.frame, CoordinateFrame::ComponentLocal { .. }))
    );
    assert!(board.geometry().item_count() > 0);
    assert!(board.geometry().outline_closed());
    assert!(
        board
            .svg(SvgSide::Top, None)
            .unwrap()
            .contents
            .contains("data-component-id=\"component-top\"")
    );
    assert_eq!(
        value_at(source_node(&board, &data.layers[0].source), 1),
        Some("F.Cu")
    );
    assert_eq!(
        value_at(source_node(&board, &data.pads[1].source), 1),
        Some("1")
    );
    for component in &data.components {
        assert_eq!(
            value_at(source_node(&board, &component.source), 0),
            Some("footprint")
        );
    }
    assert_eq!(
        value_at(source_node(&board, &data.track_arcs[0].source), 0),
        Some("arc")
    );
    assert_eq!(
        value_at(source_node(&board, &data.models[0].source), 0),
        Some("model")
    );
    assert_eq!(
        value_at(
            source_node(&board, &data.zones[0].cached_fill_polygons[0].source),
            0
        ),
        Some("filled_polygon")
    );
    let mut invalid_locator = data.pads[0].source.clone();
    invalid_locator.source_path = vec![usize::MAX];
    assert!(board.source_node(&invalid_locator).is_none());
}

#[test]
fn saved_bottom_coordinates_are_not_flipped_twice_and_pad_locators_remain_distinct() {
    let input = TestInput::new(TEST_BOARD);
    let board = input.load();
    let data = board.engineering();
    assert_eq!(
        data.pads[0].board_position_mm,
        Some(Point2 { x: 13.0, y: 18.0 })
    );
    assert_eq!(
        data.pads[2].board_position_mm,
        data.pads[0].board_position_mm
    );
    assert_eq!(data.pads[0].source_absolute_angle_deg, Some(30.0));
    assert_eq!(data.pads[2].source_absolute_angle_deg, Some(30.0));
    assert_eq!(data.pads[0].number, data.pads[1].number);
    assert_ne!(
        data.pads[0].source.source_path,
        data.pads[1].source.source_path
    );
    assert_ne!(
        data.pads[0].source.source_path,
        data.pads[2].source.source_path
    );
    assert_eq!(
        data.pads[0].source.component_ordinal,
        data.pads[1].source.component_ordinal
    );
    assert_ne!(
        data.pads[0].source.component_ordinal,
        data.pads[2].source.component_ordinal
    );
    let before = data.pads[2].board_position_mm;
    let bottom = board.svg(SvgSide::Bottom, None).unwrap();
    assert!(bottom.contents.contains("scale(-1 1)"));
    assert_eq!(board.engineering().pads[2].board_position_mm, before);
}

#[test]
fn malformed_and_unsupported_source_records_remain_observable() {
    let input = TestInput::new(
        r#"(kicad_pcb (version 20240108) (layers (0 F.Cu signal))
      (footprint Test (layer F.Cu) (uuid component-unsupported) (at 1 2)
        (property Reference U1) (pad 1 smd custom (uuid custom-pad) (at 2 3) (size 1 1) (layers F.Cu)
          (primitives (gr_circle (center 0 0) (end 1 0))))
        (pad 2 smd rect (uuid malformed-pad) (at NaN 4) (size 1 1) (layers F.Cu)))
      (zone (layer F.Cu) (polygon (pts (xy 0 0) (xy NaN 1) (xy 2 2)))))"#,
    );
    let board = input.load();
    let data = board.engineering();
    assert_eq!(data.pads.len(), 2);
    assert_eq!(data.pads[0].shape.as_deref(), Some("custom"));
    assert_eq!(
        data.pads[0].source.source_uuid.as_deref(),
        Some("custom-pad")
    );
    assert_eq!(data.pads[0].custom_primitive_count, 1);
    assert_eq!(data.pads[1].source_local_pose.position_mm, None);
    assert_eq!(data.pads[1].board_position_mm, None);
    assert_eq!(data.zones[0].boundary_polygons[0].points_mm[1], None);
    assert!(!data.zones[0].boundary_polygons[0].is_complete);
    assert!(
        data.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::UnsupportedShape)
    );
    assert!(
        data.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::UnsupportedPrimitive)
    );
    assert!(
        data.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidNumber)
    );
    let invalid_point = data
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.field == "xy" && diagnostic.code == DiagnosticCode::InvalidNumber
        })
        .unwrap();
    assert_eq!(
        value_at(source_node(&board, &invalid_point.source), 1),
        Some("NaN")
    );
    assert!(!board.geometry_complete());
}

#[test]
fn canonical_property_precedence_matches_existing_inspection_projection() {
    let input = TestInput::new(
        r#"(kicad_pcb (version 20240108) (layers (0 F.Cu signal))
      (footprint Test (layer F.Cu) (uuid component-first) (at 0 0)
        (fp_text reference OLD (at 0 0) (layer F.Cu))
        (property Reference D1) (property Reference D2) (property Value "") (property Value later))
      (footprint Test (layer F.Cu) (uuid component-missing) (at 1 1)
        (fp_text reference LEGACY (at 0 0) (layer F.Cu))
        (property Reference) (property Reference blocked)))"#,
    );
    let board = input.load();
    let inspection: serde_json::Value = serde_json::from_str(&board.inspection_json()).unwrap();
    let components = &board.engineering().components;
    assert_eq!(components[0].reference.as_deref(), Some("D1"));
    assert_eq!(components[0].value.as_deref(), Some(""));
    assert_eq!(inspection["footprints"][0]["reference"], "D1");
    assert_eq!(inspection["footprints"][0]["value"], "");
    assert_eq!(components[1].reference, None);
    assert_eq!(inspection["footprints"][1]["reference"], "");
    assert!(
        board
            .engineering()
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::ConflictingProperty)
    );
}

#[test]
fn empty_or_malformed_present_uuid_remains_unselectable_and_diagnosed() {
    let input = TestInput::new(
        r#"(kicad_pcb (version 20240108) (layers (0 F.Cu signal))
      (footprint Test (layer F.Cu) (uuid "") (at 0 0) (property Reference U1))
      (footprint Test (layer F.Cu) (uuid) (tstamp must-not-be-used) (at 1 1) (property Reference U2)))"#,
    );
    let board = input.load();
    let data = board.engineering();
    assert_eq!(data.components[0].source.source_uuid.as_deref(), Some(""));
    assert_eq!(data.components[1].source.source_uuid, None);
    for component in &data.components {
        assert!(data.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::MissingIdentity
                && diagnostic.source.source_path == component.source.source_path
        }));
    }
    let inspection: serde_json::Value = serde_json::from_str(&board.inspection_json()).unwrap();
    assert_eq!(inspection["footprints"][0]["uuid"], "");
    assert_eq!(inspection["footprints"][1]["uuid"], "");
    assert!(board.placements_csv().is_err());
}

#[test]
fn malformed_scalar_compatibility_does_not_invent_a_complete_physical_pose() {
    let input = TestInput::new(
        r#"(kicad_pcb (version 20240108) (layers (0 F.Cu signal))
      (footprint Test (layer F.Cu) (uuid invalid-pose) (at NaN 20 broken)
        (property Reference U1) (property DNP yes) (attr exclude_from_pos_files)))"#,
    );
    let board = input.load();
    let component = &board.engineering().components[0];
    assert_eq!(component.source_board_pose.position_mm, None);
    assert_eq!(component.source_board_pose.effective_rotation_deg(), None);
    let inspection: serde_json::Value = serde_json::from_str(&board.inspection_json()).unwrap();
    assert!(inspection["footprints"][0]["x_mm"].is_null());
    assert_eq!(inspection["footprints"][0]["y_mm"], 20.0);
    assert_eq!(inspection["footprints"][0]["dnp"], true);
    assert_eq!(inspection["footprints"][0]["excluded_from_pos_files"], true);
    assert!(board.placements_csv().is_err());
}

#[test]
fn public_native_package_intake_reconstructs_the_same_physical_document() {
    let input = TestInput::new(TEST_BOARD);
    let original = input.load();
    let package = input.root.join("package-test-only");
    original.build_package(&package).unwrap();
    let from_package = tokito_cli::load_board_input(&package).unwrap();
    let verified = tokito_cli::verify_package(&package).unwrap();
    assert_eq!(original.engineering(), from_package.engineering());
    assert_eq!(original.source().sha256, verified.source().sha256);
    assert_eq!(
        original.svg(SvgSide::Top, None).unwrap().contents,
        from_package.svg(SvgSide::Top, None).unwrap().contents
    );
    assert_eq!(
        original.placements_csv().unwrap(),
        from_package.placements_csv().unwrap()
    );
}

#[test]
fn pinned_real_fixture_component_counts_are_available_to_a_rust_consumer() {
    let fixtures = [
        (
            "sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb",
            20,
            29,
            0,
            0,
        ),
        (
            "raspberry-pi-pico-community/RPI-PICO-R3a-PUBLIC - Official.kicad_pcb",
            57,
            232,
            546,
            111,
        ),
        (
            "arduino-uno-r3-community/Arduino UNO.kicad_pcb",
            56,
            265,
            831,
            75,
        ),
        ("nxp-mcxa-esc/MR-ESC-MCXA-AM32.kicad_pcb", 97, 353, 491, 128),
    ];
    for (fixture, components, pads, segments, vias) in fixtures {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/open-source")
            .join(fixture);
        let board = tokito_cli::load_kicad_board(&path).unwrap();
        let data = board.engineering();
        assert_eq!(data.components.len(), components, "{fixture}");
        assert_eq!(data.pads.len(), pads, "{fixture}");
        assert_eq!(data.track_segments.len(), segments, "{fixture}");
        assert_eq!(data.vias.len(), vias, "{fixture}");
        assert_eq!(
            data.components
                .iter()
                .filter(|component| component.source.source_uuid.is_some())
                .count(),
            components,
            "{fixture}"
        );
    }
}
