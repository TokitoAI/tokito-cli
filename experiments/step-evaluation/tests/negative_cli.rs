//! Executable negative-input journeys; these are not target-board fixtures.
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

struct NegativeFile(PathBuf);

impl NegativeFile {
    fn new(contents: &[u8]) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("negative-integration");
        fs::create_dir_all(&root).expect("create ignored fixture directory");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after Unix epoch")
            .as_nanos();
        let ordinal = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!(
            "negative-{}-{timestamp}-{ordinal}.step",
            std::process::id()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .expect("create unique negative input without overwriting user work");
        file.write_all(contents).expect("write negative input");
        Self(path)
    }
}

impl Drop for NegativeFile {
    fn drop(&mut self) {
        // Delete only this fixture, never the shared target directory.
        let _ = fs::remove_file(&self.0);
    }
}

fn run_negative(contents: &[u8]) -> Value {
    let input = NegativeFile::new(contents);
    let output = Command::new(env!("CARGO_BIN_EXE_tokito-step-evaluation"))
        .arg(&input.0)
        .output()
        .expect("run actual STEP evaluation executable");
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected refusal exit 2, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "unexpected panic/error stream: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value =
        serde_json::from_slice(&output.stdout).expect("executable must emit one valid JSON report");
    assert_eq!(report["probeSchema"], "tokito.step-evaluation.v1");
    assert_eq!(report["admitted"], false);
    assert_eq!(report["exportsProduced"], serde_json::json!([]));
    report
}

#[test]
fn executable_refuses_materialization_pointer() {
    let report = run_negative(
        b"version https://git-lfs.github.com/spec/v1\noid sha256:0000000000000000000000000000000000000000000000000000000000000000\nsize 123\n",
    );
    assert_eq!(report["error"], "git_lfs_pointer_not_materialized");
}

#[test]
fn executable_reports_malformed_step_without_admission_or_export() {
    let report = run_negative(b"ISO-10303-21; MALFORMED NEGATIVE INPUT\n");
    let error = report["error"].as_str().expect("error string");
    assert!(
        error.starts_with("reader_error: STEP parse error:"),
        "{error}"
    );
    assert!(error.contains("expected `HEADER`"), "{error}");
    assert!(error.contains("MALFORMED"), "{error}");
}

#[test]
fn physical_research_mode_refuses_invalid_parameters_and_unpinned_source() {
    let input = NegativeFile::new(b"synthetic negative input, not a qualified STEP source");
    for (arguments, expected) in [
        (vec!["--research-physical-source-face"], "usage:"),
        (
            vec![
                "--research-physical-source-face",
                "2875",
                "--chord-tolerance",
                "nan",
                "--source-representation",
                "5957",
            ],
            "invalid_physical_chord_tolerance",
        ),
        (
            vec![
                "--research-physical-source-face",
                "2875",
                "--chord-tolerance",
                "0.00001",
                "--source-representation",
                "5957",
            ],
            "physical_research_requires_pinned_model16_face_and_representation",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_tokito-step-evaluation"))
            .args(&arguments)
            .arg(&input.0)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stderr.is_empty());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(report["error"].as_str().unwrap().starts_with(expected));
        assert_eq!(report["admitted"], false);
        assert_eq!(report["exportsProduced"], serde_json::json!([]));
        assert!(report.get("physicalRefinementCandidate").is_none());
    }
}
