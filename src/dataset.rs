//! Offline, bounded content locking. A declared Git revision is not authenticated
//! by this module, and byte integrity never implies component or physical fit.
use crate::{EngineError, read_bounded_file, sha256_hex, strict_json};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_JSON: usize = 4 * 1024 * 1024;
const MAX_ASSET: usize = 256 * 1024 * 1024;
const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;
const ROLES: &[&str] = &[
    "schematic",
    "pcb",
    "gerber",
    "bom",
    "photo",
    "model",
    "component_map",
    "registration",
    "provenance",
    "metadata",
];

fn err(message: impl Into<String>) -> EngineError {
    EngineError::new(message)
}

pub fn load_json(path: &Path) -> Result<Value, EngineError> {
    let bytes = read_bounded_file(path, MAX_JSON)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| err("dataset JSON must be UTF-8"))?;
    strict_json::parse(text).map_err(|e| err(format!("invalid dataset JSON: {e}")))
}

fn fields(value: &Value, allowed: &[&str]) -> Result<(), EngineError> {
    let object = value
        .as_object()
        .ok_or_else(|| err("dataset entry must be an object"))?;
    if object.len() != allowed.len() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(err("dataset fields are missing or unknown"));
    }
    Ok(())
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, EngineError> {
    value[key]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 2048 && !s.chars().any(char::is_control))
        .ok_or_else(|| err(format!("invalid dataset string {key}")))
}

fn hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn relative_path(value: &str) -> Result<(), EngineError> {
    if value.contains(['\\', ':'])
        || value.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part.ends_with(['.', ' '])
                || part.contains(['<', '>', '"', '|', '?', '*'])
        })
    {
        return Err(err("dataset asset path must be a portable relative path"));
    }
    for part in value.split('/') {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.ends_with(|c: char| ('1'..='9').contains(&c)))
        {
            return Err(err("dataset asset path contains a reserved device name"));
        }
    }
    Ok(())
}

fn no_link(path: &Path) -> Result<(), EngineError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| err(format!("dataset path unavailable: {e}")))?;
    let linked = metadata.file_type().is_symlink();
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(err("linked dataset paths are not accepted"));
        }
    }
    if linked {
        return Err(err("linked dataset paths are not accepted"));
    }
    Ok(())
}

fn root_path(root: &Path) -> Result<PathBuf, EngineError> {
    // Check lexical ancestors before canonicalization, which would erase links.
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| err(e.to_string()))?
            .join(root)
    };
    for ancestor in absolute.ancestors() {
        no_link(ancestor)?;
    }
    let result = fs::canonicalize(absolute).map_err(|e| err(e.to_string()))?;
    if !result.is_dir() {
        return Err(err("dataset root must be a directory"));
    }
    Ok(result)
}

fn asset_bytes(root: &Path, relative: &str) -> Result<Vec<u8>, EngineError> {
    relative_path(relative)?;
    let mut target = root.to_path_buf();
    for segment in relative.split('/') {
        target.push(segment);
        no_link(&target)?;
    }
    if !fs::metadata(&target)
        .map_err(|e| err(e.to_string()))?
        .is_file()
    {
        return Err(err("dataset asset must be a regular file"));
    }
    let resolved = fs::canonicalize(&target).map_err(|e| err(e.to_string()))?;
    if !resolved.starts_with(root) {
        return Err(err("dataset asset escapes root"));
    }
    let bytes = read_bounded_file(&target, MAX_ASSET)?;
    if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1") {
        return Err(err("dataset asset is an unmaterialized Git-LFS pointer"));
    }
    Ok(bytes)
}

fn validate(value: &Value, locked: bool) -> Result<&Vec<Value>, EngineError> {
    fields(value, &["schema", "repository", "revision", "assets"])?;
    if value["schema"]
        != if locked {
            "tokito.dataset-source-lock.v1"
        } else {
            "tokito.dataset-source-request.v1"
        }
    {
        return Err(err("unsupported dataset schema"));
    }
    if string(value, "repository")? != "https://github.com/TokitoAI/tokito-app-dataset" {
        return Err(err("unsupported dataset repository"));
    }
    if !hex(string(value, "revision")?, 40) {
        return Err(err("dataset revision must be a full lowercase Git SHA-1"));
    }
    let assets = value["assets"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 10_000)
        .ok_or_else(|| err("dataset requires 1..10000 assets"))?;
    let mut paths = BTreeSet::new();
    let mut total = 0u64;
    for asset in assets {
        fields(
            asset,
            if locked {
                &["boardId", "role", "path", "provenance", "bytes", "sha256"]
            } else {
                &["boardId", "role", "path", "provenance"]
            },
        )?;
        string(asset, "boardId")?;
        string(asset, "provenance")?;
        if !ROLES.contains(&string(asset, "role")?) {
            return Err(err("unknown dataset asset role"));
        }
        let path = string(asset, "path")?;
        relative_path(path)?;
        // Portable uniqueness also prevents inconsistent aliases on Windows.
        if !paths.insert(path.to_lowercase()) {
            return Err(err("duplicate dataset asset path"));
        }
        if locked {
            if !hex(string(asset, "sha256")?, 64) {
                return Err(err("invalid dataset SHA-256"));
            }
            let bytes = asset["bytes"]
                .as_u64()
                .filter(|n| *n <= MAX_ASSET as u64)
                .ok_or_else(|| err("invalid dataset asset byte count"))?;
            total += bytes;
            if total > MAX_TOTAL {
                return Err(err("dataset aggregate exceeds 2 GiB limit"));
            }
        }
    }
    Ok(assets)
}

/// Lock exactly the requested files, never unrelated files or generated outputs.
pub fn create_source_lock(root: &Path, request: &Value) -> Result<Value, EngineError> {
    let assets = validate(request, false)?;
    let root = root_path(root)?;
    let mut total = 0u64;
    let mut locked = Vec::new();
    for asset in assets {
        let bytes = asset_bytes(&root, string(asset, "path")?)?;
        total += bytes.len() as u64;
        if total > MAX_TOTAL {
            return Err(err("dataset aggregate exceeds 2 GiB limit"));
        }
        let mut record = asset.clone();
        record["bytes"] = json!(bytes.len());
        record["sha256"] = json!(sha256_hex(&bytes));
        locked.push(record);
    }
    locked.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let result = json!({"schema":"tokito.dataset-source-lock.v1", "repository":request["repository"], "revision":request["revision"], "assets":locked});
    // The emitted representation must fit the same parser used on re-import.
    let encoded = serde_json::to_vec_pretty(&result).map_err(|e| err(e.to_string()))?;
    if encoded.len() > MAX_JSON {
        return Err(err(
            "generated dataset lock exceeds 4 MiB; split the requested asset set",
        ));
    }
    Ok(result)
}

/// Verify content integrity, returning explicit failures without admitting CAD.
pub fn verify_source_lock(root: &Path, lock: &Value) -> Result<Value, EngineError> {
    let assets = validate(lock, true)?;
    let root = root_path(root)?;
    let mut results = Vec::new();
    let mut inventory: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut total = 0u64;
    for asset in assets {
        let path = string(asset, "path")?;
        *inventory
            .entry(string(asset, "boardId")?.to_owned())
            .or_default()
            .entry(string(asset, "role")?.to_owned())
            .or_default() += 1;
        let result = match asset_bytes(&root, path) {
            Ok(bytes) => {
                total += bytes.len() as u64;
                if total > MAX_TOTAL {
                    return Err(err("dataset aggregate exceeds 2 GiB limit"));
                }
                let hash = sha256_hex(&bytes);
                let passed = asset["bytes"] == json!(bytes.len()) && asset["sha256"] == hash;
                json!({"path":path,"status":if passed {"passed"} else {"failed"},"actualBytes":bytes.len(),"actualSha256":hash,"reason":if passed {"content_match"} else {"content_mismatch"}})
            }
            Err(error) => json!({"path":path,"status":"failed","reason":error.to_string()}),
        };
        results.push(result);
    }
    let passed = results.iter().all(|r| r["status"] == "passed");
    Ok(
        json!({"schema":"tokito.dataset-source-verification.v1","status":if passed {"passed"} else {"refused"},"repository":lock["repository"],"declaredRevision":lock["revision"],"revisionAuthenticated":false,"scope":"listed_assets_only","assetCount":assets.len(),"verifiedBytes":total,"inventory":inventory,"assets":results,"physicalFitVerified":false,"componentCrosswalkVerified":false,"geometryQualified":false,"appAdmissionAuthorized":false}),
    )
}
