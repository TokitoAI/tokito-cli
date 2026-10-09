//! Public Rust API coverage for the first, explicitly partial GLB export slice.
use std::path::PathBuf;
use tokito_cli::{GlbExportOptions, load_kicad_board};

fn porcupine() -> tokito_cli::PhysicalBoardDocument {
    load_kicad_board(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/open-source/porcupine-routed/porcupine_routed.kicad_pcb"),
    )
    .expect("pinned routed board fixture")
}

#[test]
fn public_export_requires_explicit_partial_consent_and_reports_coverage() {
    let board = porcupine();
    assert!(board.export_glb(GlbExportOptions::default()).is_err());

    let export = board
        .export_glb(GlbExportOptions {
            allow_partial: true,
        })
        .expect("explicit board-shell-only export");
    assert!(export.report.partial_export);
    assert!(export.report.shell_geometry_complete);
    assert!(!export.report.whole_board_digital_twin_complete);
    assert_eq!(export.report.component_models_emitted, 0);
    assert_eq!(
        export.report.component_models_unresolved,
        export.report.component_count
    );

    let receipt: serde_json::Value =
        serde_json::from_str(&export.report.report_json()).expect("coverage JSON");
    assert_eq!(receipt["scope"], "board_shell_only");
    assert_eq!(
        receipt["coverage"]["unresolvedComponentsTotal"],
        export.report.component_count
    );
    assert_eq!(receipt["coverage"]["unresolvedComponentsTruncated"], false);
    assert_eq!(
        receipt["coverage"]["unresolvedComponents"]
            .as_array()
            .unwrap()
            .len(),
        export.report.component_count
    );
    assert!(
        receipt["coverage"]["unresolvedComponents"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| {
                item["uuid"].as_str().is_some_and(|uuid| !uuid.is_empty())
                    && item["reason"]
                        .as_str()
                        .is_some_and(|reason| !reason.is_empty())
            })
    );
    gltf::Gltf::from_slice(&export.bytes).expect("independent Rust GLB parser accepts output");
}
