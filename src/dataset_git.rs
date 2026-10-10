//! Read-only immutable Git-object inventory and LFS payload comparison.
//! No Git subprocess, checkout, filter, credential helper or network operation.
use crate::{EngineError, dataset, sha256_hex};
use gix::objs::{Kind, tree::EntryKind};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::Path;

const MAX_ENTRIES: usize = 10_000;
const MAX_TREE_DEPTH: usize = 64;
const MAX_METADATA: usize = 4 * 1024 * 1024;
const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;

fn err(message: impl Into<String>) -> EngineError {
    EngineError::new(message)
}

/// Only a standalone repository with local, unlinked storage is supported.
fn validate_storage(git_dir: &Path) -> Result<(), EngineError> {
    dataset::no_link(git_dir)?;
    if !git_dir.is_dir() {
        return Err(err(
            "dataset inventory requires a standalone .git directory",
        ));
    }
    let mut pending = vec![git_dir.to_path_buf()];
    let mut count = 0;
    let mut stored_bytes = 0u64;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(|_| err("cannot inspect Git storage"))? {
            let path = entry
                .map_err(|_| err("cannot inspect Git storage entry"))?
                .path();
            dataset::no_link(&path)?;
            count += 1;
            if count > 20_000 {
                return Err(err("Git storage entry budget exceeded"));
            }
            let metadata =
                fs::metadata(&path).map_err(|_| err("cannot inspect Git storage metadata"))?;
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                if (path == git_dir.join("config") || path == git_dir.join("config.worktree"))
                    && metadata.len() > 64 * 1024
                {
                    return Err(err("repository-local Git configuration exceeds 64 KiB"));
                }
                stored_bytes = stored_bytes
                    .checked_add(metadata.len())
                    .ok_or_else(|| err("Git storage size overflow"))?;
                if metadata.len() > dataset::MAX_ASSET as u64 || stored_bytes > MAX_TOTAL {
                    return Err(err("Git storage size budget exceeded"));
                }
            } else {
                return Err(err("unsupported Git storage file type"));
            }
        }
    }
    // Every existing parent was checked above before any nested path is probed.
    for relative in [
        "commondir",
        "objects/info/alternates",
        "objects/info/http-alternates",
    ] {
        if fs::symlink_metadata(git_dir.join(relative)).is_ok() {
            return Err(err("external/shared Git object stores are unsupported"));
        }
    }
    Ok(())
}

fn object<'a>(
    repo: &'a gix::Repository,
    id: gix::ObjectId,
    kind: Kind,
    budget: &mut u64,
) -> Result<gix::Object<'a>, EngineError> {
    let header = repo
        .find_header(id)
        .map_err(|_| err(format!("pinned Git object {id} is missing or unreadable")))?;
    let limit = if kind == Kind::Blob {
        dataset::MAX_ASSET
    } else {
        MAX_METADATA
    };
    if header.kind() != kind || header.size() > limit as u64 {
        return Err(err("unsupported Git object kind or size"));
    }
    *budget = budget
        .checked_add(header.size())
        .ok_or_else(|| err("Git object size overflow"))?;
    if *budget > MAX_TOTAL {
        return Err(err("Git decoded-object budget exceeded"));
    }
    let object = repo
        .find_object(id)
        .map_err(|_| err("cannot decode pinned Git object"))?;
    if object.kind != kind || object.data.len() as u64 != header.size() {
        return Err(err("Git object header/content mismatch"));
    }
    let computed = gix::objs::compute_hash(repo.object_hash(), kind, &object.data)
        .map_err(|_| err("Git object hash computation refused"))?;
    if computed != id {
        return Err(err("Git object hash mismatch"));
    }
    Ok(object)
}

fn lfs_pointer(bytes: &[u8]) -> Result<Option<(String, u64)>, EngineError> {
    if !bytes.starts_with(b"version https://git-lfs.github.com/spec/v1") {
        return Ok(None);
    }
    if bytes.len() >= 1024 || !bytes.ends_with(b"\n") || bytes.contains(&b'\r') {
        return Err(err("oversized LFS pointer"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| err("invalid LFS pointer encoding"))?;
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != 3 || lines[0] != "version https://git-lfs.github.com/spec/v1" {
        return Err(err("unsupported LFS pointer fields"));
    }
    let digest = lines[1]
        .strip_prefix("oid sha256:")
        .filter(|s| dataset::hex(s, 64))
        .ok_or_else(|| err("invalid LFS object digest"))?;
    let size_text = lines[2]
        .strip_prefix("size ")
        .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
        .ok_or_else(|| err("invalid LFS payload size"))?;
    let size: u64 = size_text.parse().map_err(|_| err("LFS size overflow"))?;
    if size.to_string() != size_text {
        return Err(err("noncanonical LFS size"));
    }
    Ok(Some((digest.to_owned(), size)))
}

fn role(path: &str) -> &'static str {
    let path = path.to_lowercase();
    let name = path.rsplit('/').next().unwrap_or("");
    let extension = name.rsplit('.').next().unwrap_or("");
    match extension {
        "kicad_pcb" | "pcbdoc" | "brd" => "pcb",
        "kicad_sch" | "schdoc" | "sch" => "schematic",
        "gbr" | "ger" | "gtl" | "gbl" | "gko" | "gm1" => "gerber",
        "step" | "stp" | "glb" | "gltf" | "wrl" | "stl" | "obj" => "model",
        "jpg" | "jpeg" | "png" | "webp" | "bmp" => "image",
        "mp4" | "mov" => "video",
        _ if name.contains("bom") => "bom",
        _ if name.contains("component-map") || name.contains("component-mapping") => {
            "component_map"
        }
        _ if name.contains("ar-registration") || name.contains("board-reference") => "registration",
        _ if name.contains("provenance") => "provenance",
        _ => "metadata",
    }
}

/// Inspect exactly a caller-pinned commit, independent of HEAD, sparse checkout,
/// untracked output folders, Git environment overrides and user configuration.
pub fn inventory(root: &Path, revision: &str) -> Result<Value, EngineError> {
    inventory_with_assets(root, revision, root)
}

/// Inspect immutable source objects against a separately materialized asset tree.
/// Asset paths retain their source-relative names; no checkout or copying occurs.
pub fn inventory_with_assets(
    root: &Path,
    revision: &str,
    assets_root: &Path,
) -> Result<Value, EngineError> {
    if !dataset::hex(revision, 40) {
        return Err(err("inventory requires a full lowercase Git SHA-1"));
    }
    let root = dataset::root_path(root)?;
    let assets_root = dataset::root_path(assets_root)?;
    let git_dir = root.join(".git");
    validate_storage(&git_dir)?;
    let options = gix::open::Options::isolated()
        .open_path_as_is(true)
        .config_overrides([
            "gitoxide.objects.noReplace=true",
            "gitoxide.objects.allocLimit=268435456",
        ]);
    let repo =
        gix::open_opts(git_dir, options).map_err(|_| err("cannot open isolated Git storage"))?;
    if repo.object_hash() != gix::hash::Kind::Sha1 {
        return Err(err("only SHA-1 Git repositories are supported"));
    }
    let commit_id =
        gix::ObjectId::from_hex(revision.as_bytes()).map_err(|_| err("invalid commit ID"))?;
    let mut decoded_bytes = 0;
    let commit = object(&repo, commit_id, Kind::Commit, &mut decoded_bytes)?
        .try_into_commit()
        .map_err(|_| err("requested object is not a commit"))?;
    let tree_id = commit
        .tree_id()
        .map_err(|_| err("commit has no valid tree"))?
        .detach();
    let mut pending = VecDeque::from([(tree_id, String::new(), 0)]);
    let mut paths = BTreeSet::new();
    let mut assets = Vec::new();
    let mut file_count = 0usize;
    let mut tree_count = 0usize;
    let mut payload_bytes = 0u64;
    let mut totals: BTreeMap<String, usize> = BTreeMap::new();
    let mut board_sources: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    while let Some((id, prefix, depth)) = pending.pop_front() {
        if depth > MAX_TREE_DEPTH {
            return Err(err("Git tree depth budget exceeded"));
        }
        tree_count += 1;
        if tree_count > MAX_ENTRIES {
            return Err(err("Git tree count budget exceeded"));
        }
        let tree = object(&repo, id, Kind::Tree, &mut decoded_bytes)?
            .try_into_tree()
            .map_err(|_| err("invalid Git tree"))?;
        for entry in tree.iter() {
            let entry = entry.map_err(|_| err("invalid Git tree entry"))?;
            let name = std::str::from_utf8(entry.filename().as_ref())
                .map_err(|_| err("non-UTF8 Git asset name unsupported"))?;
            if name.contains('/') || name.eq_ignore_ascii_case(".git") {
                return Err(err("unsafe Git tree entry name"));
            }
            let path = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}/{name}")
            };
            dataset::relative_path(&path)?;
            if path.len() > 2048 || !paths.insert(path.to_lowercase()) {
                return Err(err("ambiguous or oversized Git asset path"));
            }
            let id = entry.object_id();
            if entry.kind() == EntryKind::Tree {
                pending.push_back((id, path, depth + 1));
                continue;
            }
            file_count += 1;
            if file_count > MAX_ENTRIES {
                return Err(err("Git asset count budget exceeded"));
            }
            let source_parts: Vec<_> = path.split('/').collect();
            let source_id = if source_parts.len() >= 4
                && source_parts[0] == "data"
                && source_parts[1] == "board-sources"
            {
                Some(source_parts[2].to_owned())
            } else {
                None
            };
            let claimed_role = role(&path);
            let mut record = json!({"path":path,"gitObjectId":id.to_string(),"boardSourceId":source_id,"roleClaim":claimed_role,"gitBlobHashVerified":false,"contentVerified":false,"materialization":"unsupported_mode","lfs":null});
            if matches!(entry.kind(), EntryKind::Blob | EntryKind::BlobExecutable) {
                let blob = object(&repo, id, Kind::Blob, &mut decoded_bytes)?;
                record["gitBlobHashVerified"] = json!(true);
                record["sourceBlobBytes"] = json!(blob.data.len());
                match lfs_pointer(&blob.data) {
                    Err(error) => {
                        record["materialization"] =
                            json!(if error.to_string().starts_with("unsupported") {
                                "unsupported_source_pointer"
                            } else {
                                "invalid_source_pointer"
                            });
                        record["reason"] = json!(error.to_string());
                    }
                    Ok(pointer) => {
                        if let Some((hash, size)) = &pointer {
                            record["lfs"] = json!({"sha256":hash,"bytes":size});
                        }
                        if pointer
                            .as_ref()
                            .is_some_and(|(_, size)| *size > dataset::MAX_ASSET as u64)
                        {
                            record["materialization"] = json!("unsupported_payload_size");
                        } else {
                            // Linked paths are refused even when their payload matches.
                            let payload = match dataset::asset_present(&assets_root, &path) {
                                Ok(false) => {
                                    record["materialization"] = json!("missing");
                                    None
                                }
                                Ok(true) => Some(dataset::asset_bytes(&assets_root, &path)),
                                Err(error) => Some(Err(error)),
                            };
                            match payload {
                                None => {}
                                Some(Err(error)) => {
                                    record["materialization"] =
                                        json!(if error.to_string().contains("Git-LFS pointer") {
                                            "pointer_only"
                                        } else {
                                            "unavailable"
                                        });
                                    record["reason"] = json!(error.to_string());
                                }
                                Some(Ok(bytes)) => {
                                    payload_bytes += bytes.len() as u64;
                                    if payload_bytes > MAX_TOTAL {
                                        return Err(err("working payload budget exceeded"));
                                    }
                                    let digest = sha256_hex(&bytes);
                                    let matches = match pointer {
                                        Some((hash, size)) => {
                                            digest == hash && bytes.len() as u64 == size
                                        }
                                        None => bytes == blob.data,
                                    };
                                    record["materialization"] = json!(if matches {
                                        "materialized"
                                    } else {
                                        "content_mismatch"
                                    });
                                    record["contentVerified"] = json!(matches);
                                    record["actualSha256"] = json!(digest);
                                    record["actualBytes"] = json!(bytes.len());
                                }
                            }
                        }
                    }
                }
            }
            let state = record["materialization"].as_str().unwrap().to_owned();
            *totals.entry(state.clone()).or_default() += 1;
            if let Some(id) = source_id {
                *board_sources
                    .entry(id)
                    .or_default()
                    .entry(state)
                    .or_default() += 1;
            }
            assets.push(record);
        }
    }
    assets.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let complete = assets.iter().all(|a| a["contentVerified"] == true);
    let result = json!({"schema":"tokito.dataset-git-inventory.v1","status":"inspected","commit":revision,"tree":tree_id.to_string(),"commitAndTreeHashesVerified":true,"publisherAuthenticated":false,"roleAndBoardIdentityQualified":false,"workingTreeCoverageComplete":complete,"trackedAssetCount":file_count,"treeCount":tree_count,"workingPayloadBytesRead":payload_bytes,"sourceObjectBytesRead":decoded_bytes,"materializationCounts":totals,"boardSourceCounts":board_sources,"assets":assets,"physicalFitVerified":false,"appAdmissionAuthorized":false});
    if serde_json::to_vec_pretty(&result)
        .map_err(|_| err("cannot serialize dataset inventory"))?
        .len()
        > 8 * 1024 * 1024
    {
        return Err(err("inventory report exceeds 8 MiB"));
    }
    Ok(result)
}

/// Bind a listed-asset content lock to verified immutable local source objects.
/// This proves selected payload identity, not publisher or engineering acceptance.
pub fn bind_source_lock(
    git_root: &Path,
    assets_root: &Path,
    request: &Value,
) -> Result<Value, EngineError> {
    let lock = dataset::create_source_lock(assets_root, request)?;
    let revision = lock["revision"]
        .as_str()
        .ok_or_else(|| err("source lock revision missing"))?;
    let inventory = inventory_with_assets(git_root, revision, assets_root)?;
    let records: BTreeMap<_, _> = inventory["assets"]
        .as_array()
        .ok_or_else(|| err("source inventory missing"))?
        .iter()
        .filter_map(|record| record["path"].as_str().map(|path| (path, record)))
        .collect();
    let mut source_assets = Vec::new();
    for asset in lock["assets"]
        .as_array()
        .ok_or_else(|| err("source lock assets missing"))?
    {
        let path = asset["path"]
            .as_str()
            .ok_or_else(|| err("source lock path missing"))?;
        let record = records.get(path).ok_or_else(|| {
            err(format!(
                "requested asset is not tracked at source pin: {path}"
            ))
        })?;
        if record["contentVerified"] != true
            || record["actualSha256"] != asset["sha256"]
            || record["actualBytes"] != asset["bytes"]
        {
            return Err(err(format!(
                "selected source payload does not match pinned object: {path}"
            )));
        }
        source_assets.push(json!({"path":path,"gitObjectId":record["gitObjectId"],"lfs":record["lfs"],"sha256":asset["sha256"],"bytes":asset["bytes"]}));
    }
    let result = json!({"schema":"tokito.dataset-source-binding.v1","status":"bound","scope":"listed_assets_only","commit":revision,"tree":inventory["tree"],"commitAndTreeHashesVerified":true,"selectedPayloadsMatchSourceObjects":true,"publisherAuthenticated":false,"roleAndBoardIdentityQualified":false,"physicalFitVerified":false,"geometryQualified":false,"appAdmissionAuthorized":false,"contentLock":lock,"sourceAssets":source_assets});
    if serde_json::to_vec_pretty(&result)
        .map_err(|_| err("cannot serialize source binding"))?
        .len()
        > MAX_METADATA
    {
        return Err(err("source binding JSON budget exceeded"));
    }
    Ok(result)
}
