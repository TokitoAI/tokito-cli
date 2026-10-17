//! Public Rust API coverage for the first, explicitly partial GLB export slice.
#[path = "support/test_scratch.rs"]
mod test_scratch;
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

        let parent = test_scratch::directory("glb-api-tests");
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
fn additive_model_api_wrappers_agree_and_omit_default_filter_metadata() {
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
    assert!(current.report.unspecified_filter.is_none());
    assert!(!current.report_json().contains("unspecifiedFilter"));
}

#[test]
fn unspecified_filter_uses_only_native_type_and_preserves_specified_models() {
    for footprint_type in ["smd", "through_hole"] {
        let f = AssemblyFixture::new(&format!(
            "(footprint test (layer F.Cu) (at 7 7) (uuid typed-r2) (attr {footprint_type}) (property Reference R2) (model Resistor_SMD.3dshapes/R_0603_1608Metric.wrl))"
        ));
        let filter =
            tokito_cli::model_assembly::ModelAssemblyFilter::default().excluding_unspecified();
        let export = f
            .board
            .export_glb_with_models_filtered(&f.resolver(), GlbExportOptions::default(), &filter)
            .unwrap();
        assert_eq!(export.report.selected_component_count, 1);
        assert_eq!(export.report.filtered_component_count, 1);
        assert_eq!(export.report.component_models_emitted, 1);
        assert!(!export.report.partial_export);
        let coverage = export.report.unspecified_filter.as_ref().unwrap();
        assert_eq!(coverage.components_excluded, 1);
        assert_eq!(coverage.model_references_excluded, 1);
        let glb = gltf::binary::Glb::from_slice(&export.bytes).unwrap();
        let document: serde_json::Value = serde_json::from_slice(&glb.json).unwrap();
        let names: Vec<_> = document["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|node| node["name"].as_str())
            .collect();
        assert!(names.contains(&"ref:R2"));
        assert!(!names.contains(&"ref:R1"));
        let receipt: serde_json::Value = serde_json::from_str(&export.report_json()).unwrap();
        assert_eq!(
            receipt["coverage"]["unspecifiedFilter"],
            document["asset"]["extras"]["tokito"]["coverage"]["unspecifiedFilter"]
        );
    }
}

#[test]
fn unspecified_and_dnp_exclusions_report_overlap_and_skip_missing_models() {
    let f = AssemblyFixture::new(
        "(footprint test (layer F.Cu) (at 7 7) (uuid untyped-dnp) (attr dnp) (property Reference R2) (model missing-one.wrl) (model missing-two.step))
         (footprint test (layer F.Cu) (at 7 7) (uuid typed-dnp) (attr smd dnp) (property Reference R3) (model missing-three.wrl))
         (footprint test (layer F.Cu) (at 7 7) (uuid typed) (attr smd) (property Reference R4) (model Resistor_SMD.3dshapes/R_0603_1608Metric.wrl))",
    );
    let filter = tokito_cli::model_assembly::ModelAssemblyFilter::default()
        .excluding_dnp()
        .excluding_unspecified();
    let export = f
        .board
        .export_glb_with_models_filtered(&f.resolver(), GlbExportOptions::default(), &filter)
        .unwrap();
    assert_eq!(export.report.source_component_count, 4);
    assert_eq!(export.report.selected_component_count, 1);
    assert_eq!(export.report.filtered_component_count, 3);
    assert_eq!(export.dnp_filter.components_excluded, 2);
    assert_eq!(export.dnp_filter.model_references_excluded, 3);
    let unspecified = export.report.unspecified_filter.as_ref().unwrap();
    assert_eq!(unspecified.components_excluded, 2);
    assert_eq!(unspecified.model_references_excluded, 3);
    assert_eq!(unspecified.overlap_with_dnp_components, 1);
    assert_eq!(export.report.filtered_model_references, 4);
    assert_eq!(export.report.component_models_emitted, 1);
    assert!(!export.report.partial_export);
}

#[test]
fn unspecified_classification_accepts_native_modifiers_but_rejects_ambiguous_source() {
    let filter = tokito_cli::model_assembly::ModelAssemblyFilter::default().excluding_unspecified();
    for attributes in [
        "",
        "(attr)",
        "(attr board_only)",
        "(attr virtual)",
        "(attr dnp exclude_from_pos_files exclude_from_bom allow_missing_courtyard allow_soldermask_bridges)",
    ] {
        let f = AssemblyFixture::new(&format!(
            "(footprint test (layer F.Cu) (at 7 7) (uuid untyped) {attributes} (property Reference R2) (property Type smd) (pad 1 smd rect (at 0 0) (size 1 1) (layers F.Cu)))"
        ));
        let selection = filter.selection(&f.board).unwrap();
        assert_eq!(selection.excluded_unspecified_component_ordinals.len(), 2);
        assert!(selection.selected_component_ordinals.is_empty());
    }
    for attributes in [
        "(attr smd through_hole)",
        "(attr smd smd)",
        "(attr smd) (attr through_hole)",
        "(attr (smd))",
        "(attr unknown)",
        "(attr \"smd\")",
        "(attr \"through_hole\")",
        "(attr smd \"board_only\")",
        "(\"attr\" smd)",
    ] {
        let f = AssemblyFixture::new(&format!(
            "(footprint test (layer F.Cu) (at 7 7) (uuid malformed) {attributes} (property Reference R2))"
        ));
        assert!(
            filter
                .selection(&f.board)
                .unwrap_err()
                .to_string()
                .contains("--no-unspecified")
        );
    }
    for uuid in [
        "",
        "(uuid assembly-r1)",
        "(uuid extra) (uuid duplicate)",
        "(uuid extra nested)",
        "(\"uuid\" extra)",
    ] {
        let f = AssemblyFixture::new(&format!(
            "(footprint test (layer F.Cu) (at 7 7) {uuid} (property Reference R2))"
        ));
        assert!(
            filter
                .selection(&f.board)
                .unwrap_err()
                .to_string()
                .contains("--no-unspecified")
        );
    }
    for head in ["\"footprint\"", "\"module\""] {
        let f = AssemblyFixture::new(&format!(
            "({head} test (layer F.Cu) (at 7 7) (uuid quoted-head) (attr smd) (property Reference R2))"
        ));
        assert!(
            filter
                .selection(&f.board)
                .unwrap_err()
                .to_string()
                .contains("--no-unspecified")
        );
    }
    let f = AssemblyFixture::new(
        "(footprint test (layer F.Cu) (at 7 7) (uuid valid-head) (attr smd) (property Reference R2) (property Description \"footprint\"))",
    );
    assert_eq!(
        filter
            .selection(&f.board)
            .unwrap()
            .selected_component_ordinals
            .len(),
        1
    );
    let path = f.root.join("assembly.kicad_pcb");
    let source = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, source.replacen("(kicad_pcb", "(\"kicad_pcb\"", 1)).unwrap();
    let quoted_root = load_kicad_board(&path).unwrap();
    assert!(
        filter
            .selection(&quoted_root)
            .unwrap_err()
            .to_string()
            .contains("--no-unspecified")
    );
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
