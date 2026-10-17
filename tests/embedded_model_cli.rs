use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(char::from(TABLE[usize::from(a >> 2)]));
        output.push(char::from(TABLE[usize::from(((a & 3) << 4) | (b >> 4))]));
        output.push(if chunk.len() > 1 {
            char::from(TABLE[usize::from(((b & 15) << 2) | (c >> 6))])
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            char::from(TABLE[usize::from(c & 63)])
        } else {
            '='
        });
    }
    output
}

fn write_embedded_board(directory: &Path, checksum: &str) -> PathBuf {
    write_embedded_board_with_checksums(directory, checksum, checksum)
}

fn write_embedded_board_with_checksums(
    directory: &Path,
    local_checksum: &str,
    board_checksum: &str,
) -> PathBuf {
    let compressed = include_bytes!("fixtures/embedded-models/step-sample.zst");
    let encoded = base64(compressed);
    let payload = encoded
        .as_bytes()
        .chunks(76)
        .map(|line| std::str::from_utf8(line).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!(
        "(kicad_pcb (version 20250114) (layers (0 F.Cu signal)) (footprint Test:U1 (uuid comp-1) (at 1 2) (property Reference U1) (model kicad-embed://chip.step) (embedded_files (file (name \"chip.step\") (type model) (checksum \"{local_checksum}\"))) ) (embedded_files (file (name \"chip.step\") (type model) (checksum \"{board_checksum}\") (data |\n{payload}\n|))))"
    );
    let path = directory.join("embedded.kicad_pcb");
    fs::write(&path, source).unwrap();
    path
}

fn write_embedded_wrl_board(directory: &Path) -> (PathBuf, String) {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let model_path = repo.join("tests/fixtures/open-source/kicad-resistor-models/Resistor_SMD.3dshapes/R_0603_1608Metric.wrl");
    let _model = fs::read(model_path).unwrap();
    let sha256 = "eadfec9e71c67b1dd3910c72b45d811a06f0ab5397d0609354b10e9a3a4c1fdd";
    // This is the old MMH3-V1 checksum for these exact WRL bytes. The optional
    // native CLI check verifies that KiCad accepts the legacy hash path.
    let checksum = "B53FCF54458A8A78ECD7EC4D28E78CB0";
    let compressed_path = repo.join("tests/fixtures/open-source/kicad-resistor-models/Resistor_SMD.3dshapes/R_0603_1608Metric.wrl.zst");
    let compressed = fs::read(compressed_path).unwrap();
    let encoded = base64(&compressed);
    let payload = encoded
        .as_bytes()
        .chunks(76)
        .map(|line| std::str::from_utf8(line).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!(
        "(kicad_pcb (version 20240108) (general (thickness 1.6)) (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal) (44 \"Edge.Cuts\" user)) (setup (stackup (layer \"F.Cu\" (type \"copper\") (thickness 0.035)) (layer \"dielectric 1\" (type \"core\") (thickness 1.53)) (layer \"B.Cu\" (type \"copper\") (thickness 0.035)))) (gr_rect (start 0 0) (end 10 10) (stroke (width 0.05) (type default)) (fill none) (layer \"Edge.Cuts\") (uuid \"edge\")) (footprint \"Resistor_SMD:R_0603_1608Metric\" (layer \"F.Cu\") (at 5 5 90) (uuid \"assembly-r1\") (property \"Reference\" \"R1\") (property \"Value\" \"10k\") (model \"kicad-embed://R_0603_1608Metric.wrl\") (embedded_files (file (name \"R_0603_1608Metric.wrl\") (type model) (checksum \"{checksum}\")))) (embedded_files (file (name \"R_0603_1608Metric.wrl\") (type model) (checksum \"{checksum}\") (data |\n{payload}\n|))))"
    );
    let path = directory.join("embedded-wrl.kicad_pcb");
    fs::write(&path, source).unwrap();
    (path, sha256.to_owned())
}

fn run_with_extra(binary: &str, board: &Path, directory: &Path, extra: &[&str]) -> Value {
    let output = Command::new(binary)
        .args([
            "resolve",
            "models",
            board.to_str().unwrap(),
            "--project-root",
            directory.to_str().unwrap(),
        ])
        .args(extra)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn run(binary: &str, board: &Path, directory: &Path) -> Value {
    run_with_extra(binary, board, directory, &[])
}

#[test]
fn cli_resolves_embedded_payload_and_reports_bad_checksum_without_geometry_claims() {
    let directory = std::env::temp_dir().join(format!(
        "tokito-embedded-cli-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let good_checksum = "25bdbce8f3930e778844aff9e6deb9dd4aa5f01315979d7f7eebc68bbc301313";
    let board = write_embedded_board(&directory, good_checksum);
    let binary = env!("CARGO_BIN_EXE_tokito-cli");
    let report = run(binary, &board, &directory);
    assert_eq!(report["resolvedAssets"], 1, "{report}");
    assert_eq!(report["capturedBytes"], 32);
    assert_eq!(report["models"][0]["status"], "resolved");
    assert_eq!(report["models"][0]["format"], "step");
    assert_eq!(report["models"][0]["origin"], "embedded_board");
    assert_eq!(report["models"][0]["embedded_name"], "chip.step");
    assert_eq!(report["models"][0]["sha256"], good_checksum);
    assert_eq!(
        report["models"][0]["ownerComponent"]["sourceComponentOrdinal"],
        2
    );
    assert_eq!(
        report["models"][0]["ownerComponent"]["componentUuid"],
        "comp-1"
    );
    assert_eq!(report["models"][0]["ownerComponent"]["reference"], "U1");
    assert_eq!(
        report["models"][0]["ownerComponent"]["selectableIdentityReady"],
        true
    );
    assert_eq!(report["modelReferencesWithUniqueOwnerIdentity"], 1);
    assert_eq!(report["modelReferencesWithoutUniqueOwnerIdentity"], 0);
    assert_eq!(report["geometryValidated"], false);
    assert_eq!(report["componentSelectionReady"], false);
    assert_eq!(report["mobileImportReady"], false);

    let extracted = directory.join("embedded-source-models");
    let extraction = run_with_extra(
        binary,
        &board,
        &directory,
        &["--extract-dir", extracted.to_str().unwrap()],
    );
    assert_eq!(extraction["assetExport"]["partialExport"], false);
    assert_eq!(extraction["assetExport"]["resolvedModelPayloads"], 1);
    assert_eq!(extraction["assetExport"]["payloadBytes"], 32);
    assert_eq!(extraction["assetExport"]["uniqueAssetBytes"], 32);
    assert_eq!(extraction["assetExport"]["geometryDecoded"], false);
    assert_eq!(extraction["assetExport"]["componentSelectionReady"], false);
    assert_eq!(extraction["assetExport"]["mobileImportReady"], false);
    assert_eq!(extraction["models"][0]["ownerComponent"]["reference"], "U1");
    assert_eq!(
        extraction["models"][0]["extraction"]["status"],
        "exported_source_payload"
    );
    let extracted_manifest: Value =
        serde_json::from_slice(&fs::read(extracted.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        extracted_manifest["sourceBoardSha256"],
        report["sourceSha256"]
    );
    assert_eq!(
        extracted_manifest["models"][0]["ownerComponent"]["componentUuid"],
        "comp-1"
    );
    let asset_path = extracted_manifest["models"][0]["extraction"]["assetPath"]
        .as_str()
        .unwrap();
    let exported_bytes = fs::read(extracted.join(asset_path)).unwrap();
    assert_eq!(exported_bytes.len(), 32);
    assert_eq!(
        extracted_manifest["models"][0]["extraction"]["sha256"],
        good_checksum
    );
    assert_eq!(
        extraction["models"][0]["extraction"]["placementApplied"],
        false
    );
    assert_eq!(extraction["mobileImportReady"], false);

    let (wrl_board, wrl_sha256) = write_embedded_wrl_board(&directory);
    let wrl_report = run(binary, &wrl_board, &directory);
    assert_eq!(wrl_report["resolvedAssets"], 1, "{wrl_report}");
    assert_eq!(wrl_report["models"][0]["format"], "wrl");
    assert_eq!(wrl_report["models"][0]["sha256"], wrl_sha256);
    let tokito_glb = directory.join("tokito-embedded.glb");
    let tokito_export = Command::new(binary)
        .args(["export", "glb"])
        .arg(&wrl_board)
        .args(["--output"])
        .arg(&tokito_glb)
        .args(["--assemble-wrl", "--project-root"])
        .arg(&directory)
        .arg("--json-report")
        .output()
        .unwrap();
    assert!(
        tokito_export.status.success(),
        "{}",
        String::from_utf8_lossy(&tokito_export.stderr)
    );
    let tokito_report: Value = serde_json::from_slice(&tokito_export.stdout).unwrap();
    assert_eq!(tokito_report["coverage"]["componentModelsEmitted"], 1);
    let tokito_bytes = fs::read(tokito_glb).unwrap();
    assert!(tokito_bytes.starts_with(b"glTF") && tokito_bytes.len() > 100);
    let tokito_document = gltf::Gltf::from_slice(&tokito_bytes).unwrap();
    assert!(tokito_document.document.meshes().count() > 0);
    if let Ok(kicad_cli) = std::env::var("TOKITO_KICAD_CLI") {
        let glb = directory.join("native-embedded.glb");
        let native = Command::new(kicad_cli)
            .args(["pcb", "export", "glb", "--no-board-body", "--output"])
            .arg(&glb)
            .arg(&wrl_board)
            .output()
            .unwrap();
        assert!(
            native.status.success(),
            "{}",
            String::from_utf8_lossy(&native.stderr)
        );
        let stderr = String::from_utf8_lossy(&native.stderr);
        assert!(!stderr.contains("Checksum error"), "{stderr}");
        assert!(!String::from_utf8_lossy(&native.stdout).contains("File not found"));
        let bytes = fs::read(glb).unwrap();
        assert!(bytes.starts_with(b"glTF") && bytes.len() > 100);
        let native_document = gltf::Gltf::from_slice(&bytes).unwrap();
        assert!(native_document.document.meshes().count() > 0);
    }

    let bad_board = write_embedded_board(&directory, &"0".repeat(64));
    let bad_report = run(binary, &bad_board, &directory);
    assert_eq!(bad_report["resolvedAssets"], 0);
    assert_eq!(
        bad_report["models"][0]["status"],
        "embedded_checksum_mismatch"
    );
    assert_eq!(bad_report["models"][0]["sha256"], Value::Null);

    let refused_output = directory.join("incomplete-must-refuse");
    let refused = Command::new(binary)
        .args(["resolve", "models"])
        .arg(&bad_board)
        .args(["--project-root"])
        .arg(&directory)
        .args(["--extract-dir"])
        .arg(&refused_output)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(!refused_output.exists());
    let partial_output = directory.join("explicit-partial");
    let partial = run_with_extra(
        binary,
        &bad_board,
        &directory,
        &[
            "--extract-dir",
            partial_output.to_str().unwrap(),
            "--allow-partial",
        ],
    );
    assert_eq!(partial["assetExport"]["partialExport"], true);
    assert_eq!(partial["assetExport"]["unexportedModelReferences"], 1);
    assert_eq!(
        partial["models"][0]["extraction"]["status"],
        "source_model_unresolved"
    );
    let partial_manifest: Value =
        serde_json::from_slice(&fs::read(partial_output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(partial_manifest["partialExport"], true);
    assert_eq!(partial_manifest["componentSelectionReady"], false);

    let conflict_board =
        write_embedded_board_with_checksums(&directory, good_checksum, &"0".repeat(64));
    let conflict_report = run(binary, &conflict_board, &directory);
    assert_eq!(conflict_report["resolvedAssets"], 0);
    assert_eq!(
        conflict_report["models"][0]["status"],
        "embedded_metadata_conflict"
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_marks_duplicate_footprint_uuid_as_not_selection_ready() {
    let directory = std::env::temp_dir().join(format!(
        "tokito-embedded-identity-cli-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let checksum = "25bdbce8f3930e778844aff9e6deb9dd4aa5f01315979d7f7eebc68bbc301313";
    let board = write_embedded_board(&directory, checksum);
    let mut source = fs::read_to_string(&board).unwrap();
    let board_payload = source
        .rfind(" (embedded_files (file (name \"chip.step\")")
        .expect("board-level payload table exists");
    source.insert_str(
        board_payload,
        " (footprint Test:U2 (uuid comp-1) (at 3 4) (property Reference U2))",
    );
    fs::write(&board, source).unwrap();

    let report = run(env!("CARGO_BIN_EXE_tokito-cli"), &board, &directory);
    assert_eq!(
        report["models"][0]["ownerComponent"]["componentUuid"],
        "comp-1"
    );
    assert_eq!(report["models"][0]["ownerComponent"]["reference"], "U1");
    assert_eq!(
        report["models"][0]["ownerComponent"]["selectableIdentityReady"],
        false
    );
    assert_eq!(
        report["models"][0]["ownerComponent"]["identityIssues"][0],
        "ambiguous_component_uuid"
    );
    assert_eq!(report["modelReferencesWithUniqueOwnerIdentity"], 0);
    assert_eq!(report["modelReferencesWithoutUniqueOwnerIdentity"], 1);
    assert_eq!(report["componentSelectionReady"], false);

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_marks_duplicate_footprint_reference_as_not_selection_ready() {
    let directory = std::env::temp_dir().join(format!(
        "tokito-embedded-reference-cli-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let checksum = "25bdbce8f3930e778844aff9e6deb9dd4aa5f01315979d7f7eebc68bbc301313";
    let board = write_embedded_board(&directory, checksum);
    let mut source = fs::read_to_string(&board).unwrap();
    let board_payload = source
        .rfind(" (embedded_files (file (name \"chip.step\")")
        .expect("board-level payload table exists");
    source.insert_str(
        board_payload,
        " (footprint Test:U2 (uuid comp-2) (at 3 4) (property Reference U1))",
    );
    fs::write(&board, source).unwrap();

    let report = run(env!("CARGO_BIN_EXE_tokito-cli"), &board, &directory);
    assert_eq!(
        report["models"][0]["ownerComponent"]["selectableIdentityReady"],
        false
    );
    assert_eq!(
        report["models"][0]["ownerComponent"]["identityIssues"][0],
        "ambiguous_component_reference"
    );
    assert_eq!(report["modelReferencesWithUniqueOwnerIdentity"], 0);
    assert_eq!(report["modelReferencesWithoutUniqueOwnerIdentity"], 1);
    assert_eq!(report["componentSelectionReady"], false);

    fs::remove_dir_all(directory).unwrap();
}
