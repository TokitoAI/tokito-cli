//! Offline external payload identity against immutable, source-bound provenance.
//! A source-authored hash reference is not publisher or engineering approval.
use crate::{EngineError, dataset, dataset_git, sha256_hex, strict_json};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    repository: String,
    revision: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Asset {
    board_id: String,
    role: String,
    path: String,
    provenance_path: String,
    claim_pointer: String,
}

fn err(message: impl Into<String>) -> EngineError {
    EngineError::new(message)
}

fn valid_text(text: &str) -> bool {
    !text.trim().is_empty() && text.len() <= 2048 && !text.chars().any(char::is_control)
}

fn valid_pointer(pointer: &str) -> bool {
    if !valid_text(pointer) || !pointer.starts_with('/') {
        return false;
    }
    let mut chars = pointer.chars();
    while let Some(c) = chars.next() {
        if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return false;
        }
    }
    true
}

/// Resolve only an explicit `schema_version: 1.0` provenance hash-reference
/// profile: a JSON-pointer-selected object with sha256, integer bytes and URL.
/// All payloads and provenance stay under the approved assets root.
pub fn bind_external(
    git_root: &Path,
    assets_root: &Path,
    request: &Value,
) -> Result<Value, EngineError> {
    let request: Request = serde_json::from_value(request.clone())
        .map_err(|e| err(format!("invalid external binding request: {e}")))?;
    if request.schema != "tokito.dataset-external-request.v1"
        || request.repository != "https://github.com/TokitoAI/tokito-app-dataset"
        || !dataset::hex(&request.revision, 40)
        || request.assets.is_empty()
        || request.assets.len() > 10_000
    {
        return Err(err(
            "unsupported external binding request identity or count",
        ));
    }
    let root = dataset::root_path(assets_root)?;
    let mut paths = BTreeSet::new();
    let mut provenance_paths = BTreeSet::new();
    for asset in &request.assets {
        dataset::relative_path(&asset.path)?;
        dataset::relative_path(&asset.provenance_path)?;
        if asset.role != "model"
            || !valid_text(&asset.board_id)
            || asset.path.len() > 2048
            || asset.provenance_path.len() > 2048
            || !valid_pointer(&asset.claim_pointer)
            || asset
                .path
                .split('/')
                .any(|part| part.eq_ignore_ascii_case(".git"))
            || !paths.insert(asset.path.to_lowercase())
        {
            return Err(err("invalid or duplicate external asset reference"));
        }
        provenance_paths.insert(asset.provenance_path.clone());
    }
    let source_request = json!({"schema":"tokito.dataset-source-request.v1","repository":request.repository,"revision":request.revision,"assets":provenance_paths.iter().map(|path| json!({"boardId":"source-provenance","role":"provenance","path":path,"provenance":"Explicit source-authored hash reference; engineering identity unverified"})).collect::<Vec<_>>()});
    let source = dataset_git::bind_source_lock(git_root, &root, &source_request)?;
    let source_records: BTreeMap<_, _> = source["contentLock"]["assets"]
        .as_array()
        .ok_or_else(|| err("source binding assets missing"))?
        .iter()
        .filter_map(|asset| asset["path"].as_str().map(|path| (path, asset)))
        .collect();
    let mut provenance = BTreeMap::new();
    let mut provenance_bytes = 0usize;
    for path in provenance_paths {
        let bytes = dataset::asset_bytes(&root, &path)?;
        provenance_bytes = provenance_bytes
            .checked_add(bytes.len())
            .ok_or_else(|| err("provenance budget overflow"))?;
        let record = source_records
            .get(path.as_str())
            .ok_or_else(|| err("source-bound provenance missing"))?;
        if bytes.len() > 4 * 1024 * 1024
            || provenance_bytes > 16 * 1024 * 1024
            || record["sha256"] != sha256_hex(&bytes)
            || record["bytes"] != bytes.len()
        {
            return Err(err("provenance changed or exceeds JSON budget"));
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| err("provenance is not UTF-8"))?;
        let document = strict_json::parse(text).map_err(|_| err("invalid provenance JSON"))?;
        if document["schema_version"] != "1.0" {
            return Err(err("unsupported provenance hash-reference profile"));
        }
        provenance.insert(path, document);
    }
    let mut total = 0u64;
    let mut assets = Vec::new();
    for asset in request.assets {
        let claim = provenance[&asset.provenance_path]
            .pointer(&asset.claim_pointer)
            .filter(|value| value.is_object())
            .ok_or_else(|| err("external hash claim pointer does not select an object"))?;
        let digest = claim["sha256"]
            .as_str()
            .filter(|value| dataset::hex(value, 64))
            .ok_or_else(|| err("external claim requires lowercase SHA-256"))?;
        let size = claim["bytes"]
            .as_u64()
            .filter(|size| *size > 0 && *size <= dataset::MAX_ASSET as u64)
            .ok_or_else(|| err("external claim requires bounded positive integer bytes"))?;
        let url = claim["url"]
            .as_str()
            .filter(|url| valid_text(url) && url.starts_with("https://"))
            .ok_or_else(|| err("external claim requires passive HTTPS URL"))?;
        total = total
            .checked_add(size)
            .ok_or_else(|| err("external byte budget overflow"))?;
        if total > 2 * 1024 * 1024 * 1024 {
            return Err(err("external aggregate byte budget exceeded"));
        }
        let payload = dataset::asset_bytes(&root, &asset.path)?;
        if payload.len() as u64 != size || sha256_hex(&payload) != digest {
            return Err(err(format!(
                "external payload differs from source claim: {}",
                asset.path
            )));
        }
        assets.push(json!({"path":asset.path,"boardIdClaim":asset.board_id,"roleClaim":asset.role,"provenancePath":asset.provenance_path,"claimPointer":asset.claim_pointer,"declaredUrl":url,"sha256":digest,"bytes":size,"payloadMatchesSourceClaim":true}));
    }
    assets.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let result = json!({"schema":"tokito.dataset-external-binding.v1","status":"bound","scope":"listed_external_payloads_only","profile":"provenance-v1-hash-reference","sourceBinding":source,"externalAssets":assets,"verifiedBytes":total,"publisherAuthenticated":false,"urlOriginVerified":false,"externalUrlFetched":false,"roleAndBoardIdentityQualified":false,"boardRevisionQualified":false,"componentCrosswalkVerified":false,"geometryQualified":false,"physicalFitVerified":false,"appAdmissionAuthorized":false});
    if serde_json::to_vec_pretty(&result)
        .map_err(|_| err("cannot serialize external binding"))?
        .len()
        > 4 * 1024 * 1024
    {
        return Err(err("external binding JSON budget exceeded"));
    }
    Ok(result)
}
