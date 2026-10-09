//! Public physical-geometry regressions. Synthetic shapes are mathematical test
//! inputs and do not establish manufactured board or reference-oracle acceptance.
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokito_cli::topology::{ContourRole, JOIN_TOLERANCE_MM, OutlineDiagnosticCode, OutlineShape};
use tokito_cli::{PhysicalBoardDocument, SvgSide};

fn board_with_outline(outline: &str) -> PhysicalBoardDocument {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/topology-tests");
    fs::create_dir_all(&parent).unwrap();
    let parent = parent.canonicalize().unwrap();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = (0..100)
        .find_map(|attempt| {
            let path = parent.join(format!(
                "synthetic-{}-{stamp}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => Some(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => panic!("cannot create test input: {error}"),
            }
        })
        .unwrap();
    let path = directory.join("synthetic-only.kicad_pcb");
    fs::write(&path, format!("(kicad_pcb (version 20240108) (general (thickness 1.6)) (layers (0 F.Cu signal) (31 B.Cu signal) (44 Edge.Cuts user)) {outline})")).unwrap();
    // Keep ignored, uniquely reserved input data for failure investigation.
    tokito_cli::load_kicad_board(path).unwrap()
}

#[test]
fn svg_refuses_a_closed_but_self_crossing_physical_board() {
    let board = board_with_outline(
        r#"
        (gr_line (start 0 0) (end 10 10) (layer Edge.Cuts) (uuid a))
        (gr_line (start 10 10) (end 0 10) (layer Edge.Cuts) (uuid b))
        (gr_line (start 0 10) (end 10 0) (layer Edge.Cuts) (uuid c))
        (gr_line (start 10 0) (end 0 0) (layer Edge.Cuts) (uuid d))
    "#,
    );
    assert!(
        board.svg(SvgSide::Top, None).is_err(),
        "Matching endpoint degree does not make a crossing contour a valid PCB"
    );
}

fn fixture(relative: &str) -> PhysicalBoardDocument {
    tokito_cli::load_kicad_board(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/open-source")
            .join(relative),
    )
    .unwrap()
}

#[test]
fn routed_porcupine_has_one_curved_outer_and_five_cutouts() {
    let board = fixture("porcupine-routed/porcupine_routed.kicad_pcb");
    let outline = board.outline();
    assert!(outline.is_valid(), "{:?}", outline.diagnostics);
    assert_eq!(
        outline
            .contours
            .iter()
            .filter(|contour| contour.role == ContourRole::Outer)
            .count(),
        1
    );
    assert_eq!(
        outline
            .contours
            .iter()
            .filter(|contour| contour.role == ContourRole::Cutout)
            .count(),
        5
    );
    assert!(
        outline
            .primitives
            .iter()
            .any(|primitive| matches!(primitive.shape, OutlineShape::Arc { .. }))
    );
    assert_eq!(outline.primitives.len(), 37);
    assert_eq!(
        outline
            .primitives
            .iter()
            .filter(|primitive| !matches!(primitive.shape, OutlineShape::Circle { .. }))
            .count(),
        32
    );
    assert_eq!(board.engineering().track_arcs.len(), 607);
    for primitive in &outline.primitives {
        assert!(
            board.source_node(&primitive.source).is_some(),
            "Every derived edge must retain an actual source node"
        );
    }
    for side in [SvgSide::Top, SvgSide::Bottom] {
        assert!(board.svg(side, None).unwrap().contents.contains("<svg"));
    }
}

#[test]
fn real_antenna_endpoint_gaps_are_reported_without_synthetic_welding() {
    let board = fixture("kicad-royalblue54l-nfc/RoyalBlue54L-NFC-Antenna.kicad_pcb");
    let outline = board.outline();
    assert_eq!(outline.join_tolerance_mm, JOIN_TOLERANCE_MM);
    assert_eq!(outline.primitives.len(), 22);
    assert!(
        outline
            .primitives
            .iter()
            .any(|primitive| primitive.source.component_ordinal.is_some())
    );
    assert!(
        !outline.is_valid(),
        "Real published sources can contain outline gaps; provenance does not prove physical validity"
    );
    assert!(
        outline
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == OutlineDiagnosticCode::OpenContour)
    );
    let endpoints = outline
        .primitives
        .iter()
        .flat_map(|primitive| match primitive.shape {
            OutlineShape::Line { start, end } | OutlineShape::Arc { start, end, .. } => {
                vec![start, end]
            }
            OutlineShape::Circle { .. } => vec![],
        })
        .collect::<Vec<_>>();
    let nearest = endpoints
        .iter()
        .enumerate()
        .map(|(index, point)| {
            endpoints
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .map(|(_, other)| (point.x - other.x).hypot(point.y - other.y))
                .fold(f64::INFINITY, f64::min)
        })
        .collect::<Vec<_>>();
    assert!(
        nearest.iter().any(|gap| *gap > 0.003535 && *gap < 0.003536),
        "The published ~3.5355µm gap must remain present in actual source geometry: {nearest:?}"
    );
    assert!(
        outline
            .diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.source.as_ref())
            .all(|source| board.source_node(source).is_some())
    );
    for side in [SvgSide::Top, SvgSide::Bottom] {
        assert!(board.svg(side, None).is_err());
    }
    // Inspection remains available even when fabrication geometry fails.
    assert!(!board.inspection_json().is_empty());
}

#[test]
fn complementary_arcs_are_exportable_through_the_public_api() {
    let board = board_with_outline(
        r#"
      (gr_arc (start -2 0) (mid 0 -2) (end 2 0) (layer Edge.Cuts) (uuid upper))
      (gr_arc (start 2 0) (mid 0 2) (end -2 0) (layer Edge.Cuts) (uuid lower))
    "#,
    );
    let outline = board.outline();
    assert!(outline.is_valid(), "{:?}", outline.diagnostics);
    assert_eq!(outline.contours.len(), 1);
    assert!((outline.contours[0].signed_area_mm2 - 4.0 * std::f64::consts::PI).abs() < 1e-10);
    for side in [SvgSide::Top, SvgSide::Bottom] {
        assert!(board.svg(side, None).is_ok());
    }
}

#[test]
fn unknown_edge_cuts_object_blocks_certification_and_keeps_identity() {
    let board = board_with_outline(
        r#"
      (gr_rect (start 0 0) (end 10 10) (layer Edge.Cuts) (uuid perimeter))
      (future_outline (layer Edge.Cuts) (uuid future-object))
    "#,
    );
    let diagnostic = board
        .outline()
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == OutlineDiagnosticCode::UnsupportedPrimitive)
        .unwrap();
    let source = diagnostic.source.as_ref().unwrap();
    assert_eq!(source.kind, "future_outline");
    assert_eq!(source.source_uuid.as_deref(), Some("future-object"));
    assert!(board.source_node(source).is_some());
    assert!(!board.outline().is_valid());
    assert!(board.svg(SvgSide::Top, None).is_err());
}

#[test]
fn local_backside_rectangle_keeps_source_transform_and_stroke_free_bounds() {
    let board = board_with_outline(
        r#"
      (footprint Fixture (layer B.Cu) (at 10 20 90) (uuid fixture)
       (fp_rect (start 0 0) (end 2 3) (stroke (width 9)) (layer Edge.Cuts) (uuid edge)))
    "#,
    );
    let outline = board.outline();
    assert!(outline.is_valid(), "{:?}", outline.diagnostics);
    assert_eq!(outline.primitives.len(), 4);
    let bounds = outline.bounds.unwrap();
    assert!((bounds.min_x - 10.).abs() < 1e-12);
    assert!((bounds.max_x - 13.).abs() < 1e-12);
    assert!((bounds.min_y - 18.).abs() < 1e-12);
    assert!((bounds.max_y - 20.).abs() < 1e-12);
    assert!(
        outline
            .primitives
            .iter()
            .all(|primitive| primitive.source.source_uuid.as_deref() == Some("edge"))
    );
    assert!(board.svg(SvgSide::Bottom, None).is_ok());
}

#[test]
fn circular_cutout_role_and_bounds_ignore_drawing_width() {
    let board = board_with_outline(
        r#"
      (gr_rect (start 0 0) (end 10 8) (stroke (width 20)) (layer Edge.Cuts))
      (gr_circle (center 5 4) (end 6 4) (stroke (width 7)) (layer Edge.Cuts))
    "#,
    );
    let outline = board.outline();
    assert!(outline.is_valid(), "{:?}", outline.diagnostics);
    assert_eq!(
        outline
            .contours
            .iter()
            .filter(|contour| contour.role == ContourRole::Cutout)
            .count(),
        1
    );
    let bounds = outline.bounds.unwrap();
    assert_eq!(
        (bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y),
        (0., 0., 10., 8.)
    );
    assert!(board.svg(SvgSide::Top, None).is_ok());
}

#[test]
fn translated_small_arcs_export_their_actual_radius() {
    let board = board_with_outline(
        r#"
      (gr_arc (start 99999.999 100000) (mid 100000 99999.999) (end 100000.001 100000) (layer Edge.Cuts))
      (gr_arc (start 100000.001 100000) (mid 100000 100000.001) (end 99999.999 100000) (layer Edge.Cuts))
    "#,
    );
    assert!(
        board.outline().is_valid(),
        "{:?}",
        board.outline().diagnostics
    );
    let bounds = board.outline().bounds.unwrap();
    assert!((bounds.max_x - bounds.min_x - 0.002).abs() < 1e-10);
    let svg = board.svg(SvgSide::Top, None).unwrap().contents;
    assert!(
        svg.contains("A 0.001000 0.001000"),
        "Published SVG must preserve the verified 1µm arc radius: {svg}"
    );
    assert!(!svg.contains("0.001633"));
}

#[test]
fn legacy_module_cutout_is_rejected_when_svg_cannot_project_it() {
    let board = board_with_outline(
        r#"
      (gr_rect (start 0 0) (end 10 10) (layer Edge.Cuts))
      (module Legacy (layer F.Cu) (at 0 0)
       (fp_circle (center 5 5) (end 6 5) (layer Edge.Cuts)))
    "#,
    );
    assert!(
        !board.outline().is_valid(),
        "A retained module cutout cannot be certified while the SVG projection omits it"
    );
    assert!(board.svg(SvgSide::Top, None).is_err());
}
