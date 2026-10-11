//! Unit audit must remain a non-exporting experiment, even for resolved inputs.
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, process::Command};
static FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn run(data: &[u8], flags: &[&str]) -> Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/unit-integration");
    fs::create_dir_all(&root).unwrap();
    let path = root.join(format!(
        "{}-{}-{}.step",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.write_all(data).unwrap();
    drop(file);
    let output = Command::new(env!("CARGO_BIN_EXE_tokito-step-evaluation"))
        .args(flags)
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["probeSchema"],
        "tokito.step-length-unit-preflight.v1"
    );
    assert_eq!(report["admitted"], false);
    assert_eq!(report["exportsProduced"], json!([]));
    report
}

fn step(units: &str) -> Vec<u8> {
    format!("ISO-10303-21;HEADER;FILE_DESCRIPTION((''),'2;1');FILE_NAME('','',(''),(''),'','','');FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));ENDSEC;DATA;#1=SI_UNIT(*,.MILLI.,.METRE.);#2=SHAPE_REPRESENTATION('',(),#3);#3=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT(({units})) REPRESENTATION_CONTEXT('',''));ENDSEC;END-ISO-10303-21;").into_bytes()
}

#[test]
fn resolved_unit_audit_still_refuses_geometry_admission() {
    let report = run(&step("#1"), &["--units-only"]);
    let audit = &report["declaredRepresentationLengthUnits"];
    assert_eq!(audit["lengthUnitsResolved"], true);
    assert_eq!(audit["analysisComplete"], true);
    assert_eq!(audit["geometryQualified"], false);
    assert_eq!(audit["assemblyPoseVerified"], false);
    assert_eq!(audit["records"][0]["metresPerUnit"], 0.001);
    assert_eq!(audit["resolvedGeometricRepresentations"], 1);
}

#[test]
fn ambiguous_units_and_invalid_cli_modes_cannot_succeed() {
    let report = run(&step("#1,#1"), &["--units-only"]);
    assert_eq!(
        report["declaredRepresentationLengthUnits"]["lengthUnitsResolved"],
        false
    );
    for flags in [
        vec!["--units-only", "--units-only"],
        vec!["--units-only", "--negative-torus-compat"],
        vec!["--units-only", "--unknown"],
        vec!["--units-only", "--candidate-mesh-data"],
    ] {
        let report = run(&step("#1"), &flags);
        assert!(report["error"].as_str().unwrap().starts_with("usage:"));
        assert!(report.get("declaredRepresentationLengthUnits").is_none());
    }
}

#[test]
fn malformed_and_unmaterialized_inputs_return_structured_refusals() {
    let malformed = run(b"ISO-10303-21; BROKEN", &["--units-only"]);
    assert!(malformed["error"]
        .as_str()
        .unwrap()
        .starts_with("reader_error: STEP parse error:"));
    let pointer = run(
        b"version https://git-lfs.github.com/spec/v1\noid sha256:x",
        &["--units-only"],
    );
    assert_eq!(pointer["error"], "git_lfs_pointer_not_materialized");
}
