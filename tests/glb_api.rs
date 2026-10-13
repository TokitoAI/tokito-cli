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

/// Authored layout for API edge cases; model bytes come from the pinned library.
struct AssemblyFixture {
    root: PathBuf,
    board: tokito_cli::PhysicalBoardDocument,
}
impl AssemblyFixture {
    fn new(extra: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/glb-api-tests");
        std::fs::create_dir_all(&parent).unwrap();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut root = None;
        for _ in 0..64 {
            let candidate = parent.join(format!(
                "{}-{timestamp}-{}",
                std::process::id(),
                NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&candidate) {
                Ok(()) => {
                    root = Some(candidate);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create isolated GLB API fixture: {error}"),
            }
        }
        let root = root.expect("reserve a unique GLB API fixture directory");
        let source = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/assembly-e2e/single-resistor.kicad_pcb"),
        )
        .unwrap();
        let path = root.join("assembly.kicad_pcb");
        std::fs::write(
            &path,
            format!(
                "{}\n{extra}\n)",
                source.trim_end().strip_suffix(')').unwrap()
            ),
        )
        .unwrap();
        let board = load_kicad_board(&path).unwrap();
        Self { root, board }
    }
    fn resolver(&self) -> tokito_cli::model_resolver::ModelResolver {
        use tokito_cli::model_resolver::{ModelResolver, ModelResolverConfig};
        let mut config = ModelResolverConfig::new(&self.root);
        config.library_roots.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/open-source/kicad-resistor-models"),
        );
        ModelResolver::new(config).unwrap()
    }
    fn filtered(&self) -> Result<tokito_cli::GlbModelExport, tokito_cli::EngineError> {
        self.board.export_glb_with_model_options(
            &self.resolver(),
            tokito_cli::GlbModelExportOptions {
                allow_partial: false,
                exclude_dnp: true,
            },
        )
    }
}
impl Drop for AssemblyFixture {
    fn drop(&mut self) {
        // This uniquely created directory owns only this test's generated input.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn additive_model_api_preserves_unfiltered_bytes_and_report() {
    let f = AssemblyFixture::new("");
    let legacy = f
        .board
        .export_glb_with_models(&f.resolver(), GlbExportOptions::default())
        .unwrap();
    let current = f
        .board
        .export_glb_with_model_options(&f.resolver(), tokito_cli::GlbModelExportOptions::default())
        .unwrap();
    assert_eq!(legacy.bytes, current.bytes);
    assert_eq!(legacy.report.report_json(), current.report_json());
    assert!(!current.dnp_filter.applied);
}

#[test]
fn dnp_without_a_body_is_excluded_from_strict_retained_coverage() {
    let f = AssemblyFixture::new(
        "(footprint test (layer F.Cu) (at 7 7) (uuid dnp-no-body) (attr dnp) (property Reference R2))",
    );
    assert!(
        f.board
            .export_glb_with_models(&f.resolver(), GlbExportOptions::default())
            .is_err()
    );
    let export = f.filtered().unwrap();
    assert_eq!(export.dnp_filter.components_excluded, 1);
    assert_eq!(export.dnp_filter.model_references_excluded, 0);
    assert_eq!(export.report.component_models_unresolved, 0);
    assert!(!export.report.partial_export);
    gltf::Gltf::from_slice(&export.bytes).unwrap();
}

#[test]
fn native_dnp_multiple_missing_assets_are_excluded_before_resolution() {
    let f = AssemblyFixture::new(
        "(footprint test (layer F.Cu) (at 7 7) (uuid dnp-missing) (attr dnp) (property Reference R2) (model missing-one.wrl) (model missing-two.step))",
    );
    assert!(
        f.board
            .export_glb_with_models(&f.resolver(), GlbExportOptions::default())
            .is_err()
    );
    let export = f.filtered().unwrap();
    assert_eq!(export.dnp_filter.components_excluded, 1);
    assert_eq!(export.dnp_filter.model_references_excluded, 2);
    assert_eq!(export.report.model_references, 3);
    assert_eq!(export.report.model_references_emitted, 1);
    assert_eq!(export.report.model_references_unresolved, 0);
    assert!(!export.report.partial_export);
}

#[test]
fn property_only_dnp_remains_selected_and_resolved() {
    let f = AssemblyFixture::new(
        "(footprint test (layer F.Cu) (at 7 7) (uuid property-dnp) (property DNP yes) (property Reference R2) (model Resistor_SMD.3dshapes/R_0603_1608Metric.wrl))",
    );

    let export = f.filtered().unwrap();
    assert_eq!(export.dnp_filter.components_excluded, 0);
    assert_eq!(export.dnp_filter.model_references_excluded, 0);
    assert_eq!(export.report.component_models_emitted, 2);
    assert_eq!(export.report.component_models_unresolved, 0);
    assert_eq!(export.report.model_references, 2);
    assert_eq!(export.report.model_references_emitted, 2);
    assert_eq!(export.report.model_references_unresolved, 0);
    assert!(!export.report.partial_export);
    gltf::Gltf::from_slice(&export.bytes).unwrap();
}

#[test]
fn dnp_missing_or_ambiguous_identity_fails_closed() {
    for uuid in ["", "(uuid assembly-r1)"] {
        let f = AssemblyFixture::new(&format!(
            "(footprint test (layer F.Cu) (at 7 7) {uuid} (attr dnp) (property Reference R2))"
        ));
        let error = f.filtered().unwrap_err().to_string();
        assert!(
            error.contains("missing or ambiguous canonical UUID"),
            "{error}"
        );
    }
}

#[test]
fn dnp_malformed_or_multiple_uuid_fields_fail_closed() {
    for uuid in ["(uuid dnp-one extra)", "(uuid dnp-one) (uuid dnp-two)"] {
        let f = AssemblyFixture::new(&format!(
            "(footprint test (layer F.Cu) (at 7 7) {uuid} (attr dnp) (property Reference R2))"
        ));
        let error = f.filtered().unwrap_err().to_string();
        assert!(error.contains("source UUID fields"), "{error}");
    }
}
