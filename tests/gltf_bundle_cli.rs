use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "tokito-gltf-bundle-cli-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self { root }
    }

    fn write_board(&self, model_path: &str) -> PathBuf {
        let board = self.root.join("board.kicad_pcb");
        let source = format!(
            "(kicad_pcb (version 20240108) (general (thickness 1.6)) (layers (0 F.Cu signal) (31 B.Cu signal) (44 Edge.Cuts user)) (footprint \"Test:U1\" (layer F.Cu) (at 1 2) (uuid \"component-u1\") (property \"Reference\" \"U1\") (model \"{model_path}\")))"
        );
        fs::write(&board, source).unwrap();
        board
    }

    fn run(&self, board: &Path, output: &Path, allow_partial: bool) -> Output {
        self.run_with_batch_budget(board, output, allow_partial, None)
    }

    fn run_with_batch_budget(
        &self,
        board: &Path,
        output: &Path,
        allow_partial: bool,
        max_batch_bytes: Option<usize>,
    ) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tokito-cli"));
        command.args([
            "resolve",
            "models",
            board.to_str().unwrap(),
            "--project-root",
            self.root.to_str().unwrap(),
            "--extract-dir",
            output.to_str().unwrap(),
        ]);
        if allow_partial {
            command.arg("--allow-partial");
        }
        if let Some(max_batch_bytes) = max_batch_bytes {
            command
                .arg("--max-batch-bytes")
                .arg(max_batch_bytes.to_string());
        }
        command.arg("--json").output().unwrap()
    }
}

#[test]
fn cli_enforces_remaining_batch_budget_before_capturing_gltf_dependencies() {
    let fixture = Fixture::new();
    let models = fixture.root.join("models");
    fs::create_dir_all(&models).unwrap();
    let document = br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"mesh.bin","byteLength":4}]}"#;
    fs::write(models.join("scene.gltf"), document).unwrap();
    fs::write(
        models.join("mesh.bin"),
        b"mesh payload larger than the remaining budget",
    )
    .unwrap();
    let board = fixture.write_board("models/scene.gltf");
    let output_dir = fixture.root.join("partial-budget-output");
    let batch_budget = document.len() + 4;

    let output = fixture.run_with_batch_budget(&board, &output_dir, true, Some(batch_budget));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["capturedBytes"], document.len());
    assert_eq!(report["models"][0]["gltfBundle"]["status"], "incomplete");
    assert_eq!(
        report["models"][0]["gltfBundle"]["diagnostics"][0]["status"],
        "batch_budget_exceeded"
    );
    assert_eq!(report["assetExport"]["gltfDependencyFiles"], 0);
    assert_eq!(report["assetExport"]["partialExport"], true);
    assert!(!output_dir.join("assets/gltf").exists());
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn cli_exports_gltf_with_local_buffers_and_percent_encoded_images() {
    let fixture = Fixture::new();
    let models = fixture.root.join("models");
    fs::create_dir_all(models.join("textures")).unwrap();
    let document = br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"mesh.bin","byteLength":4}],"images":[{"uri":"textures/board%20photo.png"}]}"#;
    let binary = b"mesh";
    let image = b"image-bytes";
    fs::write(models.join("scene.gltf"), document).unwrap();
    fs::write(models.join("mesh.bin"), binary).unwrap();
    fs::write(models.join("textures/board photo.png"), image).unwrap();
    let board = fixture.write_board("models/scene.gltf");
    let output_dir = fixture.root.join("offline-models");

    let output = fixture.run(&board, &output_dir, false);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["models"][0]["gltfBundle"]["status"], "complete");
    assert_eq!(
        report["models"][0]["gltfBundle"]["uniqueExternalResources"],
        2
    );
    assert_eq!(report["assetExport"]["gltfDependencyFiles"], 2);
    assert_eq!(
        report["assetExport"]["gltfDependencyPayloadBytes"],
        binary.len() + image.len()
    );
    assert_eq!(report["assetExport"]["partialExport"], false);

    let manifest: Value =
        serde_json::from_slice(&fs::read(output_dir.join("manifest.json")).unwrap()).unwrap();
    let extraction = &manifest["models"][0]["extraction"];
    assert_eq!(extraction["status"], "exported_gltf_bundle");
    let scene = output_dir.join(extraction["assetPath"].as_str().unwrap());
    assert_eq!(fs::read(&scene).unwrap(), document);
    let scene_dir = scene.parent().unwrap();
    assert_eq!(fs::read(scene_dir.join("mesh.bin")).unwrap(), binary);
    assert_eq!(
        fs::read(scene_dir.join("textures/board photo.png")).unwrap(),
        image
    );
    assert_eq!(extraction["dependencies"].as_array().unwrap().len(), 2);
}

#[test]
fn cli_requires_explicit_partial_export_for_escaping_or_missing_gltf_resources() {
    let fixture = Fixture::new();
    let models = fixture.root.join("models");
    fs::create_dir_all(&models).unwrap();
    fs::write(
        models.join("scene.gltf"),
        br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"../outside.bin","byteLength":4}]}"#,
    )
    .unwrap();
    let board = fixture.write_board("models/scene.gltf");
    let strict_output = fixture.root.join("strict-output");
    let strict = fixture.run(&board, &strict_output, false);
    assert!(!strict.status.success());
    assert!(!strict_output.exists());

    let partial_output = fixture.root.join("partial-output");
    let partial = fixture.run(&board, &partial_output, true);
    assert!(
        partial.status.success(),
        "{}",
        String::from_utf8_lossy(&partial.stderr)
    );
    let report: Value = serde_json::from_slice(&partial.stdout).unwrap();
    assert_eq!(report["assetExport"]["partialExport"], true);
    assert_eq!(report["models"][0]["gltfBundle"]["status"], "incomplete");
    assert_eq!(
        report["models"][0]["gltfBundle"]["diagnostics"][0]["status"],
        "gltf_external_uri_traversal_rejected"
    );
    let manifest: Value =
        serde_json::from_slice(&fs::read(partial_output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["models"][0]["extraction"]["status"],
        "gltf_bundle_incomplete"
    );
    assert_eq!(manifest["unexportedModelReferences"], 1);

    fs::write(
        models.join("scene.gltf"),
        br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"missing.bin","byteLength":4}]}"#,
    )
    .unwrap();
    let missing_strict = fixture.root.join("missing-strict");
    assert!(!fixture.run(&board, &missing_strict, false).status.success());
    assert!(!missing_strict.exists());
    let missing_partial = fixture.root.join("missing-partial");
    let missing = fixture.run(&board, &missing_partial, true);
    assert!(
        missing.status.success(),
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );
    let report: Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(report["models"][0]["gltfBundle"]["status"], "incomplete");
    assert_eq!(
        report["models"][0]["gltfBundle"]["diagnostics"][0]["status"],
        "missing"
    );
}

#[test]
fn cli_rejects_duplicate_gltf_uri_keys_instead_of_choosing_a_dependency() {
    let fixture = Fixture::new();
    let models = fixture.root.join("models");
    fs::create_dir_all(&models).unwrap();
    let document = br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"mesh.bin","uri":"alternate.bin","byteLength":4}]}"#;
    fs::write(models.join("scene.gltf"), document).unwrap();
    fs::write(models.join("mesh.bin"), b"first").unwrap();
    fs::write(models.join("alternate.bin"), b"second").unwrap();
    let board = fixture.write_board("models/scene.gltf");

    let strict_output = fixture.root.join("strict-duplicate-key-output");
    let strict = fixture.run(&board, &strict_output, false);
    assert!(!strict.status.success());
    assert!(!strict_output.exists());

    let partial_output = fixture.root.join("partial-duplicate-key-output");
    let partial = fixture.run(&board, &partial_output, true);
    assert!(
        partial.status.success(),
        "{}",
        String::from_utf8_lossy(&partial.stderr)
    );
    let report: Value = serde_json::from_slice(&partial.stdout).unwrap();
    let gltf_bundle = &report["models"][0]["gltfBundle"];
    assert_eq!(gltf_bundle["status"], "incomplete");
    assert_eq!(gltf_bundle["uniqueExternalResources"], 0);
    assert_eq!(gltf_bundle["resolvedExternalResources"], 0);
    assert_eq!(gltf_bundle["diagnostics"][0]["status"], "invalid_gltf_json");
    let manifest: Value =
        serde_json::from_slice(&fs::read(partial_output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["models"][0]["extraction"]["status"],
        "gltf_bundle_incomplete"
    );
    assert_eq!(manifest["gltfDependencyFiles"], 0);
    assert!(!partial_output.join("assets/gltf").exists());
}

#[test]
fn cli_rejects_gltf_dependency_path_that_collides_with_emitted_document() {
    let fixture = Fixture::new();
    let models = fixture.root.join("models");
    fs::create_dir_all(models.join("scene.gltf")).unwrap();
    fs::write(
        models.join("board.gltf"),
        br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"scene.gltf/mesh.bin","byteLength":4}]}"#,
    )
    .unwrap();
    fs::write(models.join("scene.gltf/mesh.bin"), b"mesh").unwrap();
    let board = fixture.write_board("models/board.gltf");

    let strict_output = fixture.root.join("strict-collision-output");
    let strict = fixture.run(&board, &strict_output, false);
    assert!(!strict.status.success());
    assert!(!strict_output.exists());

    let partial_output = fixture.root.join("partial-collision-output");
    let partial = fixture.run(&board, &partial_output, true);
    assert!(
        partial.status.success(),
        "{}",
        String::from_utf8_lossy(&partial.stderr)
    );
    let report: Value = serde_json::from_slice(&partial.stdout).unwrap();
    assert_eq!(report["assetExport"]["partialExport"], true);
    assert_eq!(report["models"][0]["gltfBundle"]["status"], "incomplete");
    assert_eq!(
        report["models"][0]["gltfBundle"]["diagnostics"][0]["status"],
        "gltf_resource_path_collides_with_exported_document"
    );
    let manifest: Value =
        serde_json::from_slice(&fs::read(partial_output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["models"][0]["extraction"]["status"],
        "gltf_bundle_incomplete"
    );
    assert_eq!(manifest["gltfDependencyFiles"], 0);
}
