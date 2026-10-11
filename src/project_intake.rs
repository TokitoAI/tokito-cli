//! Bounded, read-only intake for an existing physical PCB project directory.
//! This stage discovers sources; it does not turn schematics into PCB geometry.

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::{EngineError, load_kicad_board};

const MAX_DIRECTORY_ENTRIES: usize = 4096;
const MAX_CANDIDATES: usize = 256;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCandidate {
    pub relative_path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedBoard {
    pub relative_path: String,
    pub bytes: usize,
    pub sha256: String,
    pub format_version: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInspection {
    pub schema_version: &'static str,
    pub status: &'static str,
    pub boards: Vec<ProjectCandidate>,
    pub schematics: Vec<ProjectCandidate>,
    pub selected_board: Option<SelectedBoard>,
    pub diagnostics: Vec<String>,
    pub limitations: Vec<&'static str>,
}

impl ProjectInspection {
    pub fn accepted(&self) -> bool {
        self.status == "accepted"
    }
}

/// Inspect only immediate regular files in `root`. A board is selected implicitly
/// only when exactly one is present; multiple boards require an exact filename.
/// Symlink entries observed during discovery are skipped. Concurrent mutation of
/// the selected directory is not guarded against by this read-only intake API.
/// The selected board is validated and hashed by
/// the canonical KiCad importer; schematic files are inventoried, not parsed.
pub fn inspect_project(
    root: impl AsRef<Path>,
    selected_filename: Option<&str>,
) -> Result<ProjectInspection, EngineError> {
    inspect_project_with_sha256(root, selected_filename, None)
}

/// Optionally requires the selected board to match a caller-pinned SHA-256.
/// The digest is compared after the existing importer hashes the bytes it parsed.
pub fn inspect_project_with_sha256(
    root: impl AsRef<Path>,
    selected_filename: Option<&str>,
    expected_sha256: Option<&str>,
) -> Result<ProjectInspection, EngineError> {
    let root = root.as_ref();
    if !root.is_dir() {
        return Err(EngineError::new(format!(
            "project root is not a directory: {}",
            root.display()
        )));
    }
    let mut report = ProjectInspection {
        schema_version: "0.1.0",
        status: "refused",
        boards: Vec::new(),
        schematics: Vec::new(),
        selected_board: None,
        diagnostics: Vec::new(),
        limitations: vec![
            "Only immediate regular .kicad_pcb and .kicad_sch files are discovered.",
            "Schematic sources are inventory only; no PCB placement, routing, or 3D geometry is inferred from them.",
            "Selected PCB import covers the reviewed KiCad format subset; this is not full KiCad project validation.",
            "Do not run intake against a concurrently modified or untrusted project directory; source selection is path-based.",
        ],
    };
    if let Some(digest) = expected_sha256 {
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            report
                .diagnostics
                .push("--expect-sha256 requires exactly 64 hexadecimal characters".into());
            return Ok(report);
        }
    }

    let entries = fs::read_dir(root)
        .map_err(|error| EngineError::new(format!("cannot list project directory: {error}")))?;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_DIRECTORY_ENTRIES {
            report.boards.clear();
            report.schematics.clear();
            report.diagnostics.push(format!(
                "project directory exceeds {MAX_DIRECTORY_ENTRIES} entries; discovery refused"
            ));
            return Ok(report);
        }
        let entry = entry.map_err(|error| {
            EngineError::new(format!("cannot read project directory entry: {error}"))
        })?;
        let file_type = entry.file_type().map_err(|error| {
            EngineError::new(format!("cannot inspect project directory entry: {error}"))
        })?;
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        let candidate_count = report.boards.len() + report.schematics.len();
        let is_board = extension.eq_ignore_ascii_case("kicad_pcb");
        if !is_board && !extension.eq_ignore_ascii_case("kicad_sch") {
            continue;
        }
        if candidate_count >= MAX_CANDIDATES {
            report.boards.clear();
            report.schematics.clear();
            report.diagnostics.push(format!(
                "project contains more than {MAX_CANDIDATES} source candidates; discovery refused"
            ));
            return Ok(report);
        }
        let Some(filename) = entry.file_name().into_string().ok() else {
            report.boards.clear();
            report.schematics.clear();
            report.diagnostics.push(
                "source candidate has a filename that is not UTF-8; discovery refused".into(),
            );
            return Ok(report);
        };
        let metadata = entry.metadata().map_err(|error| {
            EngineError::new(format!(
                "cannot inspect source candidate {filename}: {error}"
            ))
        })?;
        let target = if is_board {
            &mut report.boards
        } else {
            &mut report.schematics
        };
        target.push(ProjectCandidate {
            relative_path: filename,
            bytes: metadata.len(),
        });
    }
    report
        .boards
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    report
        .schematics
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let selected = match selected_filename {
        Some(filename) => {
            if filename.is_empty()
                || Path::new(filename).components().count() != 1
                || Path::new(filename)
                    .file_name()
                    .and_then(|name| name.to_str())
                    != Some(filename)
            {
                report
                    .diagnostics
                    .push("--board must be one exact top-level candidate filename".into());
                return Ok(report);
            }
            match report
                .boards
                .iter()
                .find(|item| item.relative_path == filename)
            {
                Some(candidate) => candidate,
                None => {
                    report.diagnostics.push(format!(
                        "selected board {filename:?} is not a discovered top-level .kicad_pcb file"
                    ));
                    return Ok(report);
                }
            }
        }
        None if report.boards.len() == 1 => &report.boards[0],
        None if report.boards.is_empty() => {
            report
                .diagnostics
                .push("no top-level .kicad_pcb file was found".into());
            return Ok(report);
        }
        None => {
            report.diagnostics.push(
                "multiple .kicad_pcb files found; select one by exact filename with --board".into(),
            );
            return Ok(report);
        }
    };

    let filename = selected.relative_path.clone();
    match load_kicad_board(root.join(&filename)) {
        Ok(board) => {
            let source = board.source();
            let digest_matches =
                expected_sha256.is_none_or(|expected| expected.eq_ignore_ascii_case(source.sha256));
            report.selected_board = Some(SelectedBoard {
                relative_path: filename,
                bytes: source.bytes,
                sha256: source.sha256.to_owned(),
                format_version: source.format_version,
            });
            if digest_matches {
                report.status = "accepted";
            } else {
                report.diagnostics.push(
                    "selected PCB SHA-256 does not match --expect-sha256; source may be a different revision".into(),
                );
            }
        }
        Err(error) => report.diagnostics.push(format!(
            "selected PCB could not be imported by the supported engine: {error}"
        )),
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_top_level_selection_rejects_traversal() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/open-source/sparkfun-default-kicad");
        let accepted = inspect_project(&root, None).unwrap();
        assert!(accepted.accepted());
        assert_eq!(accepted.boards.len(), 1);
        assert_eq!(accepted.selected_board.as_ref().unwrap().sha256.len(), 64);
        let refused =
            inspect_project(&root, Some("../SparkFun_Basic_Board_Example.kicad_pcb")).unwrap();
        assert!(!refused.accepted());
        assert!(refused.selected_board.is_none());
    }
}
