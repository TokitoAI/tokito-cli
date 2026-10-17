//! Intake for the existing v0.1 Tokito source-and-inventory directory format.
//! Manifest paths are never used to select files; this version has a fixed layout.
use super::{
    Board, CliError, MAX_INPUT_BYTES, load_board, package_manifest, placements_csv,
    read_bounded_file,
};
use serde_json::Value;
use std::fs;
use std::path::Path;

const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_DERIVED_BYTES: usize = 64 * 1024 * 1024;

fn reject_link(path: &Path) -> Result<(), CliError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        CliError::new(format!(
            "cannot inspect package member {}: {error}",
            path.display()
        ))
    })?;
    let is_link = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let is_link = {
        use std::os::windows::fs::MetadataExt;
        is_link || metadata.file_attributes() & 0x400 != 0
    };
    if is_link {
        return Err(CliError::new(
            "Tokito package members may not be symbolic links or reparse points",
        ));
    }
    Ok(())
}

fn read_text(path: &Path, limit: usize) -> Result<String, CliError> {
    String::from_utf8(read_bounded_file(path, limit)?)
        .map_err(|_| CliError::new("Tokito package metadata must contain valid UTF-8"))
}

pub(super) fn load_input(path: &Path) -> Result<Board, CliError> {
    if path.is_dir() {
        load_package(path)
    } else {
        load_board(path)
    }
}

pub(super) fn load_package(root: &Path) -> Result<Board, CliError> {
    reject_link(root)?;
    for relative in [
        "manifest.json",
        "source",
        "source/board.kicad_pcb",
        "derived",
        "derived/inspection.json",
        "derived/placements.csv",
    ] {
        reject_link(&root.join(relative))?;
    }
    let manifest_text = read_text(&root.join("manifest.json"), MAX_MANIFEST_BYTES)?;
    let manifest: Value = super::strict_json::parse(&manifest_text)
        .map_err(|error| CliError::new(format!("invalid Tokito package manifest JSON: {error}")))?;
    if manifest["schemaVersion"] != "0.1.0" {
        return Err(CliError::new(
            "unsupported Tokito board-package schema version; expected 0.1.0",
        ));
    }
    // Check fixed paths before loading source; never resolve paths from an untrusted manifest.
    if manifest["source"]["path"] != "source/board.kicad_pcb"
        || manifest["artifacts"]["inspection"]["path"] != "derived/inspection.json"
        || manifest["artifacts"]["placements"]["path"] != "derived/placements.csv"
    {
        return Err(CliError::new(
            "Tokito v0.1 package paths must match the fixed package layout",
        ));
    }
    let board = load_board(&root.join("source/board.kicad_pcb"))?;
    if board.source_bytes > MAX_INPUT_BYTES {
        return Err(CliError::new("package source exceeds its byte limit"));
    }
    let inspection = read_text(&root.join("derived/inspection.json"), MAX_DERIVED_BYTES)?;
    let inspection_value: Value = super::strict_json::parse(&inspection)
        .map_err(|error| CliError::new(format!("invalid package inspection JSON: {error}")))?;
    if inspection_value["schema"] != "tokito.board-inspection.v0" {
        return Err(CliError::new("unsupported package inspection schema"));
    }
    let expected_inspection: Value =
        serde_json::from_str(&super::inspect_json_for_source(&board, "board.kicad_pcb")).map_err(
            |error| CliError::new(format!("cannot construct expected inspection: {error}")),
        )?;
    if inspection_value != expected_inspection {
        return Err(CliError::new(
            "package inspection does not match the preserved source under this engine version; rebuild stale development packages",
        ));
    }
    let placements = read_text(&root.join("derived/placements.csv"), MAX_DERIVED_BYTES)?;
    if placements != placements_csv(&board)? {
        return Err(CliError::new(
            "package placement inventory does not match the preserved source",
        ));
    }
    let mut expected: Value =
        serde_json::from_str(&package_manifest(&board, &inspection, &placements)).map_err(
            |error| CliError::new(format!("cannot construct expected manifest: {error}")),
        )?;
    // The schema permits caller-authored nonempty limitation strings.
    if !manifest["limitations"].as_array().is_some_and(|values| {
        !values.is_empty()
            && values
                .iter()
                .all(|value| value.as_str().is_some_and(|text| !text.is_empty()))
    }) {
        return Err(CliError::new(
            "package limitations must be a nonempty array of nonempty strings",
        ));
    }
    expected["limitations"] = manifest["limitations"].clone();
    // Producer identity is metadata, not the package schema or a publisher
    // signature. A new executable must not invalidate an intact older package
    // solely because its producer version differs.
    let producer_version = manifest["producer"]["version"]
        .as_str()
        .filter(|version| {
            !version.is_empty()
                && version.len() <= 64
                && version
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b".-+".contains(&byte))
        })
        .ok_or_else(|| {
            CliError::new("package producer version must be bounded ASCII version text")
        })?;
    expected["producer"]["version"] = Value::String(producer_version.to_owned());
    if manifest != expected {
        return Err(CliError::new(
            "package manifest contract or source/artifact integrity checks failed",
        ));
    }
    // Stored inspection is integrity-checked above; callers receive a freshly parsed Board.
    Ok(board)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_package_round_trip_rejects_tampering_and_path_escape() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/open-source/sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb");
        let board = load_board(&fixture).expect("fixture");
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let owned_dir = std::env::temp_dir().join(format!(
            "tokito-native-intake-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&owned_dir).expect("reserve unique test root");
        let root = owned_dir.join("package");
        super::super::package_build(&board, &root).expect("package");
        let imported = load_input(&root).expect("native package intake");
        assert_eq!(imported.source_sha256, board.source_sha256);
        assert_eq!(imported.footprints.len(), board.footprints.len());
        let manifest_path = root.join("manifest.json");
        let original = fs::read_to_string(&manifest_path).expect("manifest");
        let mut older: Value = serde_json::from_str(&original).unwrap();
        older["producer"]["version"] = Value::String("0.0.1".into());
        fs::write(&manifest_path, serde_json::to_vec(&older).unwrap()).unwrap();
        assert!(load_package(&root).is_ok());
        older["producer"]["name"] = Value::String("unknown-producer".into());
        fs::write(&manifest_path, serde_json::to_vec(&older).unwrap()).unwrap();
        assert!(load_package(&root).is_err());
        fs::write(&manifest_path, &original).unwrap();
        fs::write(
            &manifest_path,
            original.replace("source/board.kicad_pcb", "../external.kicad_pcb"),
        )
        .expect("tamper path");
        assert!(load_package(&root).is_err());
        fs::write(&manifest_path, &original).expect("restore");
        fs::write(
            root.join("derived/inspection.json"),
            "{\"schema\":\"tokito.board-inspection.v0\"}",
        )
        .expect("tamper artifact");
        assert!(load_package(&root).is_err());
        let changed_inspection =
            fs::read_to_string(root.join("derived/inspection.json")).expect("changed inspection");
        let original_inspection_digest = serde_json::from_str::<Value>(&original)
            .expect("original manifest")["artifacts"]["inspection"]["sha256"]
            .as_str()
            .expect("digest")
            .to_owned();
        fs::write(
            &manifest_path,
            original.replace(
                &original_inspection_digest,
                &format!(
                    "sha256:{}",
                    super::super::sha256_hex(changed_inspection.as_bytes())
                ),
            ),
        )
        .expect("update tampered hash");
        assert!(load_package(&root).is_err());
        fs::remove_dir_all(&owned_dir).expect("cleanup own reserved test output");
    }
}
