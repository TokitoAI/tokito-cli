//! Read-only, bounded inventory for native KiCad `.pretty` footprint libraries.
//!
//! Inspection records file identity/content hashes and parses only enough of
//! each S-expression to identify a footprint root and count direct pads/models.
//! It does not validate KiCad semantics or geometry, register the library, or
//! qualify assets.

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 4096;
const MAX_FOOTPRINT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_LOCK_BYTES: u64 = 1024 * 1024;

const LOCK_SCHEMA: &str = "tokito.footprint-library-lock.v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintLibraryInspection {
    pub schema: &'static str,
    pub status: &'static str,
    pub library_directory: String,
    pub footprint_file_count: usize,
    pub ignored_entry_count: usize,
    pub total_bytes: u64,
    pub inventory_sha256: String,
    pub assets: Vec<FootprintLibraryAsset>,
    pub ignored_entries: Vec<String>,
    pub footprint_syntax_validated: bool,
    pub geometry_validated: bool,
    pub library_registration_changed: bool,
    pub limitations: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintLibraryAsset {
    pub file_name: String,
    pub bytes: u64,
    pub sha256: String,
    pub parse_status: &'static str,
    pub footprint_name: Option<String>,
    pub pad_count: usize,
    pub model_reference_count: usize,
}

/// Verification result for a content-pinned `.pretty` library snapshot.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintLibraryLockVerification {
    pub schema: &'static str,
    pub status: &'static str,
    pub matches: bool,
    pub expected_inventory_sha256: String,
    pub actual_inventory_sha256: String,
    pub expected_file_count: usize,
    pub actual_file_count: usize,
    pub missing_files: Vec<String>,
    pub added_files: Vec<String>,
    pub changed_files: Vec<String>,
}

/// Read-only convention/collision observations over one bounded library
/// inventory. Findings are not KiCad semantic errors or KLC violations.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintLibraryAudit {
    pub schema: &'static str,
    pub status: &'static str,
    pub footprint_file_count: usize,
    pub ignored_entry_count: usize,
    pub ignored_entries: Vec<String>,
    pub inventory_sha256: String,
    pub parsed_footprint_count: usize,
    pub findings: Vec<FootprintLibraryAuditFinding>,
    pub semantic_validation_performed: bool,
    pub geometry_validation_performed: bool,
    pub library_modified: bool,
    pub limitations: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintLibraryAuditFinding {
    /// `duplicate_internal_name`, `filename_name_mismatch`, or `asset_not_parsed`.
    pub code: &'static str,
    pub severity: &'static str,
    pub file_names: Vec<String>,
    pub footprint_name: Option<String>,
    pub expected_name_from_file: Option<String>,
    pub parse_status: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FootprintLibraryLock {
    schema: String,
    inventory_sha256: String,
    assets: Vec<LockedAsset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LockedAsset {
    file_name: String,
    bytes: u64,
    sha256: String,
}

/// Serialize a portable, deterministic content lock from a prior inventory.
/// This pins bytes and names only; it does not qualify footprint semantics.
pub fn create_lock_json(inspection: &FootprintLibraryInspection) -> Result<Vec<u8>, String> {
    preflight_lock_creation(inspection)?;
    let lock = FootprintLibraryLock {
        schema: LOCK_SCHEMA.to_owned(),
        inventory_sha256: inspection.inventory_sha256.clone(),
        assets: inspection
            .assets
            .iter()
            .map(|asset| LockedAsset {
                file_name: asset.file_name.clone(),
                bytes: asset.bytes,
                sha256: asset.sha256.clone(),
            })
            .collect(),
    };
    validate_lock(&lock)?;
    let mut output = serde_json::to_vec_pretty(&lock)
        .map_err(|error| format!("cannot serialize footprint library lock: {error}"))?;
    output.push(b'\n');
    if output.len() > MAX_LOCK_BYTES as usize {
        return Err(format!(
            "serialized footprint library lock exceeds {MAX_LOCK_BYTES} bytes"
        ));
    }
    Ok(output)
}

/// Bound public API inputs before cloning their owned strings into the lock.
/// CLI inspections already obey these limits, but this public function must
/// also be safe for caller-constructed inspection values.
fn preflight_lock_creation(inspection: &FootprintLibraryInspection) -> Result<(), String> {
    if inspection.assets.len() > MAX_ENTRIES {
        return Err(format!(
            "footprint library lock exceeds {MAX_ENTRIES} assets"
        ));
    }
    if !is_sha256(&inspection.inventory_sha256) {
        return Err("footprint library lock has an invalid inventory SHA-256".into());
    }

    let mut input_string_bytes = inspection.inventory_sha256.len();
    let mut total_bytes = 0_u64;
    for asset in &inspection.assets {
        // A real filesystem entry cannot exceed this portable single-name
        // bound. Check before serializing or cloning caller-owned strings.
        if asset.file_name.len() > 255 || asset.sha256.len() > 64 {
            return Err("footprint library lock input field exceeds its size limit".into());
        }
        let encoded_name_bytes = serde_json::to_string(&asset.file_name)
            .map_err(|error| format!("cannot size footprint library lock name: {error}"))?
            .len();
        input_string_bytes = input_string_bytes
            .checked_add(encoded_name_bytes)
            .and_then(|total| total.checked_add(asset.sha256.len()))
            .filter(|total| *total <= MAX_LOCK_BYTES as usize)
            .ok_or_else(|| {
                format!("footprint library lock input strings exceed {MAX_LOCK_BYTES} bytes")
            })?;
        if !is_sha256(&asset.sha256) {
            return Err(format!(
                "footprint library lock has an invalid SHA-256 for {}",
                asset.file_name
            ));
        }
        total_bytes = total_bytes
            .checked_add(asset.bytes)
            .filter(|total| *total <= MAX_TOTAL_BYTES)
            .ok_or_else(|| {
                format!("footprint library lock exceeds {MAX_TOTAL_BYTES} total bytes")
            })?;
    }
    Ok(())
}

/// Compare an existing `.pretty` inventory with a strict lock file.
/// A mismatch is returned as a report, not an I/O error, so callers can show
/// missing/added/changed names while still exiting unsuccessfully.
pub fn verify_lock(
    library_path: &Path,
    lock_path: &Path,
) -> Result<FootprintLibraryLockVerification, String> {
    let lock = read_lock(lock_path)?;
    validate_lock(&lock)?;
    let current = inspect(library_path)?;
    let expected: std::collections::BTreeMap<_, _> = lock
        .assets
        .iter()
        .map(|asset| {
            (
                asset.file_name.as_str(),
                (asset.bytes, asset.sha256.as_str()),
            )
        })
        .collect();
    let actual: std::collections::BTreeMap<_, _> = current
        .assets
        .iter()
        .map(|asset| {
            (
                asset.file_name.as_str(),
                (asset.bytes, asset.sha256.as_str()),
            )
        })
        .collect();
    let missing_files = expected
        .keys()
        .filter(|name| !actual.contains_key(**name))
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    let added_files = actual
        .keys()
        .filter(|name| !expected.contains_key(**name))
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    let changed_files = expected
        .iter()
        .filter_map(|(name, record)| {
            actual
                .get(name)
                .filter(|actual_record| *actual_record != record)
                .map(|_| (*name).to_owned())
        })
        .collect::<Vec<_>>();
    let matches = missing_files.is_empty()
        && added_files.is_empty()
        && changed_files.is_empty()
        && current.inventory_sha256 == lock.inventory_sha256;
    Ok(FootprintLibraryLockVerification {
        schema: "tokito.footprint-library-lock-verification.v1",
        status: if matches { "verified" } else { "drifted" },
        matches,
        expected_inventory_sha256: lock.inventory_sha256,
        actual_inventory_sha256: current.inventory_sha256,
        expected_file_count: expected.len(),
        actual_file_count: actual.len(),
        missing_files,
        added_files,
        changed_files,
    })
}

fn read_lock(path: &Path) -> Result<FootprintLibraryLock, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect footprint library lock: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("footprint library lock must be a regular non-symlink file".into());
    }
    if metadata.len() > MAX_LOCK_BYTES {
        return Err(format!(
            "footprint library lock exceeds {MAX_LOCK_BYTES} bytes"
        ));
    }
    let file =
        File::open(path).map_err(|error| format!("cannot read footprint library lock: {error}"))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_LOCK_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read footprint library lock: {error}"))?;
    if bytes.len() as u64 > MAX_LOCK_BYTES || bytes.len() as u64 != metadata.len() {
        return Err("footprint library lock changed while being read or exceeds size limit".into());
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid footprint library lock JSON: {error}"))
}

fn validate_lock(lock: &FootprintLibraryLock) -> Result<(), String> {
    if lock.schema != LOCK_SCHEMA {
        return Err("unsupported footprint library lock schema".into());
    }
    if !is_sha256(&lock.inventory_sha256) {
        return Err("footprint library lock has an invalid inventory SHA-256".into());
    }
    if lock.assets.len() > MAX_ENTRIES {
        return Err(format!(
            "footprint library lock exceeds {MAX_ENTRIES} assets"
        ));
    }
    let mut previous: Option<&str> = None;
    let mut total_bytes = 0_u64;
    for asset in &lock.assets {
        let path = Path::new(&asset.file_name);
        if asset.file_name.is_empty()
            || asset.file_name.contains(':')
            || asset.file_name.contains('/')
            || asset.file_name.contains('\\')
            || asset.file_name.chars().any(char::is_control)
            || path.file_name().and_then(|name| name.to_str()) != Some(asset.file_name.as_str())
            || !path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("kicad_mod"))
        {
            return Err("footprint library lock contains an invalid footprint file name".into());
        }
        if !is_sha256(&asset.sha256) {
            return Err(format!(
                "footprint library lock has an invalid SHA-256 for {}",
                asset.file_name
            ));
        }
        if asset.bytes > MAX_FOOTPRINT_BYTES {
            return Err(format!(
                "footprint library lock asset exceeds {MAX_FOOTPRINT_BYTES} bytes: {}",
                asset.file_name
            ));
        }
        total_bytes = total_bytes
            .checked_add(asset.bytes)
            .filter(|total| *total <= MAX_TOTAL_BYTES)
            .ok_or_else(|| {
                format!("footprint library lock exceeds {MAX_TOTAL_BYTES} total bytes")
            })?;
        if previous.is_some_and(|name| name >= asset.file_name.as_str()) {
            return Err(
                "footprint library lock assets must be uniquely sorted by file name".into(),
            );
        }
        previous = Some(&asset.file_name);
    }
    if inventory_digest(&lock.assets) != lock.inventory_sha256 {
        return Err("footprint library lock inventory digest does not match its assets".into());
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn inventory_digest(assets: &[LockedAsset]) -> String {
    let mut material = Vec::new();
    for asset in assets {
        material.extend_from_slice(asset.file_name.len().to_string().as_bytes());
        material.push(b':');
        material.extend_from_slice(asset.file_name.as_bytes());
        material.push(b':');
        material.extend_from_slice(asset.bytes.to_string().as_bytes());
        material.push(b':');
        material.extend_from_slice(asset.sha256.as_bytes());
        material.push(b'\n');
    }
    crate::sha256_hex(&material)
}

/// Inventory a single `.pretty` directory without following nested directories.
pub fn inspect(path: &Path) -> Result<FootprintLibraryInspection, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "footprint library path must end in .pretty".to_owned())?;
    if !extension.eq_ignore_ascii_case("pretty") {
        return Err("footprint library path must end in .pretty".into());
    }

    let root_metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect footprint library directory: {error}"))?;
    if root_metadata.file_type().is_symlink() {
        return Err("footprint library directory must not be a symbolic link".into());
    }
    if !root_metadata.is_dir() {
        return Err("footprint library path is not a directory".into());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(path)
        .map_err(|error| format!("cannot read footprint library directory: {error}"))?
    {
        if entries.len() >= MAX_ENTRIES {
            return Err(format!("footprint library exceeds {MAX_ENTRIES} entries"));
        }
        let entry = entry.map_err(|error| format!("cannot read library entry: {error}"))?;
        let file_name = entry
            .file_name()
            .into_string()
            .map_err(|_| "library entry name must be valid UTF-8".to_owned())?;
        if file_name.is_empty() || file_name.chars().any(char::is_control) {
            return Err("library entry name is empty or contains control characters".into());
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("cannot inspect library entry {file_name}: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "symbolic links are not accepted in a footprint library: {file_name}"
            ));
        }
        if metadata.is_dir() {
            entries.push(Entry::Ignored(file_name));
            continue;
        }
        if !metadata.is_file() {
            return Err(format!(
                "unsupported filesystem entry in footprint library: {file_name}"
            ));
        }
        let is_footprint = Path::new(&file_name)
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("kicad_mod"));
        if !is_footprint {
            entries.push(Entry::Ignored(file_name));
            continue;
        }
        if metadata.len() > MAX_FOOTPRINT_BYTES {
            return Err(format!(
                "footprint asset exceeds {MAX_FOOTPRINT_BYTES} bytes: {file_name}"
            ));
        }
        entries.push(Entry::Footprint {
            file_name,
            path: entry.path(),
            metadata_bytes: metadata.len(),
        });
    }

    entries.sort_by(|left, right| left.name().cmp(right.name()));
    let mut assets = Vec::new();
    let mut ignored_entries = Vec::new();
    let mut total_bytes = 0_u64;
    let mut inventory_material = Vec::new();
    for entry in entries {
        match entry {
            Entry::Ignored(file_name) => ignored_entries.push(file_name),
            Entry::Footprint {
                file_name,
                path,
                metadata_bytes,
            } => {
                let captured = read_footprint(&path, &file_name, metadata_bytes)?;
                let bytes = captured.bytes.len() as u64;
                total_bytes = total_bytes
                    .checked_add(bytes)
                    .filter(|total| *total <= MAX_TOTAL_BYTES)
                    .ok_or_else(|| format!("footprint assets exceed {MAX_TOTAL_BYTES} bytes"))?;
                inventory_material.extend_from_slice(file_name.len().to_string().as_bytes());
                inventory_material.push(b':');
                inventory_material.extend_from_slice(file_name.as_bytes());
                inventory_material.push(b':');
                inventory_material.extend_from_slice(bytes.to_string().as_bytes());
                inventory_material.push(b':');
                inventory_material.extend_from_slice(captured.sha256.as_bytes());
                inventory_material.push(b'\n');
                assets.push(FootprintLibraryAsset {
                    file_name,
                    bytes,
                    sha256: captured.sha256,
                    parse_status: captured.summary.parse_status,
                    footprint_name: captured.summary.footprint_name,
                    pad_count: captured.summary.pad_count,
                    model_reference_count: captured.summary.model_reference_count,
                });
            }
        }
    }
    let inventory_sha256 = crate::sha256_hex(&inventory_material);
    Ok(FootprintLibraryInspection {
        schema: "tokito.footprint-library-inventory.v1",
        status: if assets.is_empty() {
            "empty-or-unsupported-library"
        } else {
            "inventory-only"
        },
        library_directory: path.to_string_lossy().into_owned(),
        footprint_file_count: assets.len(),
        ignored_entry_count: ignored_entries.len(),
        total_bytes,
        inventory_sha256,
        assets,
        ignored_entries,
        footprint_syntax_validated: false,
        geometry_validated: false,
        library_registration_changed: false,
        limitations: vec![
            "the bounded shared S-expression parser identifies footprint roots and counts direct pads/model references but does not validate KiCad footprint semantics or geometry",
            "unsupported files and nested directories are reported by name but are not traversed",
            "manufacturer evidence, package-family qualification, KLC checks, board fit and 3D model qualification are not performed",
            "the selected local directory is expected to remain stable while it is read; inspection is not race-proof against a concurrent hostile writer",
        ],
    })
}

/// Inventory a library and report bounded identity/convention observations.
/// File names and internal names are compared byte-for-byte after removing the
/// case-insensitive `.kicad_mod` suffix. These observations do not determine
/// whether KiCad will load the library or whether a footprint is valid.
pub fn audit(path: &Path) -> Result<FootprintLibraryAudit, String> {
    let inventory = inspect(path)?;
    let mut findings = Vec::new();
    let mut names = std::collections::BTreeMap::<&str, Vec<&str>>::new();
    let mut parsed_footprint_count = 0;

    for asset in &inventory.assets {
        if asset.parse_status != "parsed_footprint_root" {
            findings.push(FootprintLibraryAuditFinding {
                code: "asset_not_parsed",
                severity: "information",
                file_names: vec![asset.file_name.clone()],
                footprint_name: asset.footprint_name.clone(),
                expected_name_from_file: None,
                parse_status: Some(asset.parse_status),
            });
            continue;
        }
        parsed_footprint_count += 1;
        let Some(name) = asset.footprint_name.as_deref() else {
            continue;
        };
        names.entry(name).or_default().push(&asset.file_name);
        let suffix_start = asset.file_name.len().saturating_sub(".kicad_mod".len());
        let stem = asset
            .file_name
            .get(..suffix_start)
            .filter(|_| asset.file_name[suffix_start..].eq_ignore_ascii_case(".kicad_mod"));
        let Some(stem) = stem else { continue };
        if stem != name {
            findings.push(FootprintLibraryAuditFinding {
                code: "filename_name_mismatch",
                severity: "information",
                file_names: vec![asset.file_name.clone()],
                footprint_name: Some(name.to_owned()),
                expected_name_from_file: Some(stem.to_owned()),
                parse_status: None,
            });
        }
    }

    for (name, mut file_names) in names {
        if file_names.len() > 1 {
            file_names.sort_unstable();
            findings.push(FootprintLibraryAuditFinding {
                code: "duplicate_internal_name",
                severity: "information",
                file_names: file_names.into_iter().map(str::to_owned).collect(),
                footprint_name: Some(name.to_owned()),
                expected_name_from_file: None,
                parse_status: None,
            });
        }
    }
    findings.sort_by(|left, right| {
        left.code
            .cmp(right.code)
            .then_with(|| left.footprint_name.cmp(&right.footprint_name))
            .then_with(|| left.file_names.cmp(&right.file_names))
    });

    Ok(FootprintLibraryAudit {
        schema: "tokito.footprint-library-audit.v1",
        status: if findings.is_empty() {
            "no-observed-convention-findings"
        } else {
            "observations-present"
        },
        footprint_file_count: inventory.footprint_file_count,
        ignored_entry_count: inventory.ignored_entry_count,
        ignored_entries: inventory.ignored_entries,
        inventory_sha256: inventory.inventory_sha256,
        parsed_footprint_count,
        findings,
        semantic_validation_performed: false,
        geometry_validation_performed: false,
        library_modified: false,
        limitations: vec![
            "duplicate internal names and filename mismatches are deterministic inventory observations, not confirmed KiCad load failures or KLC violations",
            "malformed or unsupported assets are listed by parse status and excluded from internal-name collision checks",
            "footprint semantics, pad/layer geometry, library registration, manufacturer evidence, package qualification and KLC compliance are not validated",
        ],
    })
}

enum Entry {
    Footprint {
        file_name: String,
        path: PathBuf,
        metadata_bytes: u64,
    },
    Ignored(String),
}

impl Entry {
    fn name(&self) -> &str {
        match self {
            Self::Footprint { file_name, .. } | Self::Ignored(file_name) => file_name,
        }
    }
}

fn read_footprint(
    path: &Path,
    file_name: &str,
    expected_bytes: u64,
) -> Result<CapturedFootprint, String> {
    let file = File::open(path)
        .map_err(|error| format!("cannot read footprint asset {file_name}: {error}"))?;
    let mut bytes = Vec::with_capacity((expected_bytes as usize).min(MAX_FOOTPRINT_BYTES as usize));
    file.take(MAX_FOOTPRINT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read footprint asset {file_name}: {error}"))?;
    if bytes.len() as u64 > MAX_FOOTPRINT_BYTES {
        return Err(format!(
            "footprint asset exceeds {MAX_FOOTPRINT_BYTES} bytes: {file_name}"
        ));
    }
    if bytes.len() as u64 != expected_bytes {
        return Err(format!(
            "footprint asset changed while being inspected: {file_name}"
        ));
    }
    let sha256 = crate::sha256_hex(&bytes);
    let summary = summarize_footprint_sexpr(&bytes);
    Ok(CapturedFootprint {
        bytes,
        sha256,
        summary,
    })
}

struct CapturedFootprint {
    bytes: Vec<u8>,
    sha256: String,
    summary: FootprintSyntaxSummary,
}

struct FootprintSyntaxSummary {
    parse_status: &'static str,
    footprint_name: Option<String>,
    pad_count: usize,
    model_reference_count: usize,
}

fn summarize_footprint_sexpr(bytes: &[u8]) -> FootprintSyntaxSummary {
    let invalid = || FootprintSyntaxSummary {
        parse_status: "invalid_sexpr",
        footprint_name: None,
        pad_count: 0,
        model_reference_count: 0,
    };
    let Ok(root) = crate::parse_document(bytes) else {
        return invalid();
    };
    if root.head() != Some("footprint") {
        return FootprintSyntaxSummary {
            parse_status: "unsupported_root",
            footprint_name: None,
            pad_count: 0,
            model_reference_count: 0,
        };
    }
    let Some(name) = root.value_at(1).filter(|name| {
        !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
    }) else {
        return FootprintSyntaxSummary {
            parse_status: "invalid_footprint_name",
            footprint_name: None,
            pad_count: 0,
            model_reference_count: 0,
        };
    };
    FootprintSyntaxSummary {
        parse_status: "parsed_footprint_root",
        footprint_name: Some(name.to_owned()),
        pad_count: root.children("pad").count(),
        model_reference_count: root.children("model").count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!("tokito-footprint-library-{label}-{nonce}.pretty"))
    }

    #[test]
    fn inventory_is_stable_and_reports_unsupported_entries_without_following_them() {
        let root = temp_path("stable");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("B.kicad_mod"), b"footprint-b").unwrap();
        fs::write(root.join("A.kicad_mod"), b"footprint-a").unwrap();
        fs::write(root.join("README.txt"), b"not footprint data").unwrap();
        let first = inspect(&root).unwrap();
        let second = inspect(&root).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.footprint_file_count, 2);
        assert_eq!(first.assets[0].file_name, "A.kicad_mod");
        assert_eq!(first.ignored_entries, ["README.txt"]);
        assert!(!first.footprint_syntax_validated);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_wrong_suffix_and_nested_directory_without_reading_nested_assets() {
        let wrong = std::env::temp_dir().join("footprints.not-pretty");
        assert!(inspect(&wrong).unwrap_err().contains("end in .pretty"));
        let root = temp_path("nested");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested/hidden.kicad_mod"), b"not traversed").unwrap();
        let inventory = inspect(&root).unwrap();
        assert_eq!(inventory.footprint_file_count, 0);
        assert_eq!(inventory.ignored_entries, ["nested"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_only_footprint_root_and_direct_pad_and_model_counts() {
        let input = br#"(footprint "Demo_SMD" (version 20260206) (layer "F.Cu")
            (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu" "F.Mask"))
            (model "body.step"))"#;
        let summary = summarize_footprint_sexpr(input);
        assert_eq!(summary.parse_status, "parsed_footprint_root");
        assert_eq!(summary.footprint_name.as_deref(), Some("Demo_SMD"));
        assert_eq!(summary.pad_count, 1);
        assert_eq!(summary.model_reference_count, 1);

        assert_eq!(
            summarize_footprint_sexpr(b"(module \"old\")").parse_status,
            "unsupported_root"
        );
        assert_eq!(
            summarize_footprint_sexpr(b"(footprint \"broken\"").parse_status,
            "invalid_sexpr"
        );
    }

    #[test]
    fn lock_creation_refuses_output_larger_than_verification_limit() {
        let assets = (0..MAX_ENTRIES)
            .map(|index| {
                let file_name = format!("{index:04}{}.kicad_mod", "X".repeat(220));
                FootprintLibraryAsset {
                    file_name,
                    bytes: 0,
                    sha256: "0".repeat(64),
                    parse_status: "not_checked",
                    footprint_name: None,
                    pad_count: 0,
                    model_reference_count: 0,
                }
            })
            .collect::<Vec<_>>();
        let locked_assets = assets
            .iter()
            .map(|asset| LockedAsset {
                file_name: asset.file_name.clone(),
                bytes: asset.bytes,
                sha256: asset.sha256.clone(),
            })
            .collect::<Vec<_>>();
        let inspection = FootprintLibraryInspection {
            schema: "tokito.footprint-library-inspection.v1",
            status: "inspected",
            library_directory: "fixture.pretty".into(),
            footprint_file_count: assets.len(),
            ignored_entry_count: 0,
            total_bytes: 0,
            inventory_sha256: inventory_digest(&locked_assets),
            assets,
            ignored_entries: Vec::new(),
            footprint_syntax_validated: false,
            geometry_validated: false,
            library_registration_changed: false,
            limitations: Vec::new(),
        };

        let error = create_lock_json(&inspection).unwrap_err();
        assert!(error.contains("footprint library lock input strings exceed"));
    }

    #[test]
    fn lock_creation_bounds_public_strings_before_cloning() {
        let inspection = FootprintLibraryInspection {
            schema: "tokito.footprint-library-inspection.v1",
            status: "inspected",
            library_directory: "fixture.pretty".into(),
            footprint_file_count: 1,
            ignored_entry_count: 0,
            total_bytes: 0,
            inventory_sha256: inventory_digest(&[]),
            assets: vec![FootprintLibraryAsset {
                file_name: "X".repeat(MAX_LOCK_BYTES as usize + 1),
                bytes: 0,
                sha256: "0".repeat(64),
                parse_status: "not_checked",
                footprint_name: None,
                pad_count: 0,
                model_reference_count: 0,
            }],
            ignored_entries: Vec::new(),
            footprint_syntax_validated: false,
            geometry_validated: false,
            library_registration_changed: false,
            limitations: Vec::new(),
        };
        let error = create_lock_json(&inspection).unwrap_err();
        assert!(error.contains("input field exceeds its size limit"));
    }
}
