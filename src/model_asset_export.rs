//! Source-bound export of model payloads referenced by a PCB.
//!
//! This copies verified model bytes into a new offline directory. It does not
//! decode geometry, apply board placement, or qualify a component for rendering.

use crate::{EngineError, sha256_hex};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const MAX_MODEL_REFERENCES: usize = 1024;
const MAX_EXPORTED_BYTES: usize = 256 * 1024 * 1024;
// Three bundle-prefix segments (`assets/gltf/<digest>`) precede up to 64
// validated URI segments in the emitted manifest path.
const MAX_GLTF_PATH_SEGMENTS: usize = 67;

/// One resolved, single-file source asset. `model_index` indexes the matching
/// row in the JSON model report and keeps duplicate model references distinct.
#[derive(Debug, Clone)]
pub(crate) struct ModelAssetPayload {
    pub(crate) model_index: usize,
    pub(crate) format: String,
    pub(crate) sha256: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) dependencies: Vec<ModelAssetDependencyPayload>,
}

/// One approved local resource referenced by a JSON glTF buffer/image URI.
#[derive(Debug, Clone)]
pub(crate) struct ModelAssetDependencyPayload {
    pub(crate) relative_path: String,
    pub(crate) sha256: String,
    pub(crate) bytes: Vec<u8>,
}

/// Write a hash-addressed asset directory and a source-bound manifest.
///
/// Existing destinations are never replaced. Assets are staged beside the
/// destination and linked into an exclusively-created directory; the manifest
/// is published last as the completeness marker. JSON glTF dependencies are
/// copied under an isolated content-addressed bundle directory, retaining the
/// document's safe relative URI paths.
pub(crate) fn export_model_assets(
    output: impl AsRef<Path>,
    source_board_sha256: &str,
    mut models: Vec<Value>,
    assets: &[ModelAssetPayload],
) -> Result<Value, EngineError> {
    if !valid_sha256(source_board_sha256) || models.len() > MAX_MODEL_REFERENCES {
        return Err(error("invalid source identity or model-reference count"));
    }
    if models.iter().any(|row| !row.is_object()) {
        return Err(error("model report rows must be JSON objects"));
    }

    let mut assets_by_model = BTreeMap::new();
    let mut unique_assets = BTreeMap::<String, (String, &[u8])>::new();
    let mut total_bytes = 0usize;
    let mut dependency_payload_bytes = 0usize;
    let mut dependency_file_count = 0usize;
    for asset in assets {
        let Some(model) = models.get(asset.model_index) else {
            return Err(error("invalid or duplicate resolved model payload"));
        };
        let format_matches = matches!(
            (model["format"].as_str(), asset.format.as_str()),
            (Some("step"), "step" | "stp")
                | (Some("wrl"), "wrl" | "vrml")
                | (Some("glb"), "glb")
                | (Some("gltf"), "gltf")
        );
        if asset.model_index >= models.len()
            || assets_by_model.insert(asset.model_index, ()).is_some()
            || !matches!(
                asset.format.as_str(),
                "step" | "stp" | "wrl" | "vrml" | "glb" | "gltf"
            )
            || model["status"] != "resolved"
            || !format_matches
            || model["sha256"].as_str() != Some(asset.sha256.as_str())
            || !valid_sha256(&asset.sha256)
            || sha256_hex(&asset.bytes) != asset.sha256
            || asset.bytes.is_empty()
        {
            return Err(error("invalid or duplicate resolved model payload"));
        }
        total_bytes = total_bytes
            .checked_add(asset.bytes.len())
            .filter(|total| *total <= MAX_EXPORTED_BYTES)
            .ok_or_else(|| error("model asset export exceeds its total byte budget"))?;
        let gltf_bundle_id = if asset.format == "gltf" {
            Some(gltf_bundle_sha256(asset)?)
        } else {
            if !asset.dependencies.is_empty() {
                return Err(error(
                    "external dependency payloads are only valid for glTF documents",
                ));
            }
            None
        };
        let path = if let Some(bundle_id) = &gltf_bundle_id {
            format!("assets/gltf/{bundle_id}/scene.gltf")
        } else {
            asset_path(&asset.sha256, &asset.format)?
        };
        insert_asset(
            &mut unique_assets,
            path.clone(),
            &asset.sha256,
            &asset.bytes,
            "content-addressed model asset collision",
        )?;
        let mut dependency_rows = Vec::with_capacity(asset.dependencies.len());
        for dependency in &asset.dependencies {
            validate_relative_resource_path(&dependency.relative_path)?;
            if gltf_path_collides_with_document(&dependency.relative_path)
                || !valid_sha256(&dependency.sha256)
                || sha256_hex(&dependency.bytes) != dependency.sha256
                || dependency.bytes.is_empty()
            {
                return Err(error("invalid glTF dependency payload"));
            }
            let bundle_id = gltf_bundle_id
                .as_deref()
                .ok_or_else(|| error("glTF dependency bundle identity is missing"))?;
            let relative_path = format!("assets/gltf/{bundle_id}/{}", dependency.relative_path);
            insert_asset(
                &mut unique_assets,
                relative_path.clone(),
                &dependency.sha256,
                &dependency.bytes,
                "glTF bundle path collision",
            )?;
            total_bytes = total_bytes
                .checked_add(dependency.bytes.len())
                .filter(|total| *total <= MAX_EXPORTED_BYTES)
                .ok_or_else(|| error("model asset export exceeds its total byte budget"))?;
            dependency_payload_bytes = dependency_payload_bytes
                .checked_add(dependency.bytes.len())
                .filter(|total| *total <= MAX_EXPORTED_BYTES)
                .ok_or_else(|| error("glTF dependency payloads exceed their byte budget"))?;
            dependency_file_count = dependency_file_count
                .checked_add(1)
                .ok_or_else(|| error("glTF dependency count overflow"))?;
            dependency_rows.push(json!({
                "relativeUriPath":dependency.relative_path,
                "assetPath":relative_path,
                "sha256":dependency.sha256,
                "bytes":dependency.bytes.len()
            }));
        }
        models[asset.model_index]["extraction"] = json!({
            "status":if asset.format == "gltf" { "exported_gltf_bundle" } else { "exported_source_payload" },
            "assetPath":path,
            "sha256":asset.sha256,
            "bytes":asset.bytes.len(),
            "gltfBundleSha256":gltf_bundle_id,
            "dependencies":dependency_rows,
            "geometryDecoded":false,
            "placementApplied":false,
            "componentGeometryQualified":false
        });
    }

    let mut extracted_rows = 0usize;
    let mut unexported_rows = 0usize;
    for (index, row) in models.iter_mut().enumerate() {
        if assets_by_model.contains_key(&index) {
            extracted_rows += 1;
        } else {
            unexported_rows += 1;
            let status = match row["status"].as_str() {
                Some("resolved") if row["format"] == "gltf" => {
                    if row["gltfBundle"]["status"] == "incomplete" {
                        "gltf_bundle_incomplete"
                    } else {
                        "external_gltf_dependencies_not_collected"
                    }
                }
                Some("resolved") => "unsupported_or_missing_payload",
                _ => "source_model_unresolved",
            };
            row["extraction"] = json!({
                "status":status,
                "assetPath":null,
                "geometryDecoded":false,
                "placementApplied":false,
                "componentGeometryQualified":false
            });
        }
    }

    let unique_asset_bytes = unique_assets
        .values()
        .try_fold(0usize, |total, (_, bytes)| {
            total
                .checked_add(bytes.len())
                .filter(|value| *value <= MAX_EXPORTED_BYTES)
                .ok_or_else(|| error("unique model asset files exceed their byte budget"))
        })?;
    let partial = unexported_rows != 0;
    let manifest = json!({
        "schema":"tokito.model-assets.v1",
        "schemaVersion":"1.0.0",
        "sourceBoardSha256":source_board_sha256,
        "modelReferences":models.len(),
        "resolvedModelPayloads":extracted_rows,
        "unexportedModelReferences":unexported_rows,
        "uniqueAssetFiles":unique_assets.len(),
        "uniqueAssetBytes":unique_asset_bytes,
        "payloadBytes":total_bytes,
        "gltfDependencyFiles":dependency_file_count,
        "gltfDependencyPayloadBytes":dependency_payload_bytes,
        "partialExport":partial,
        "geometryDecoded":false,
        "geometryValidated":false,
        "placementApplied":false,
        "componentSelectionReady":false,
        "mobileImportReady":false,
        "models":models,
        "limitations":[
            "This directory contains original resolved model payload bytes, not generated geometry.",
            "STEP units, model-local transforms, board placement, surface completeness and physical fit are not validated.",
            "Each model row preserves the PCB owner identity when the source provides it; this is not component-geometry selection readiness.",
            "JSON glTF bundles include only local relative core buffer/image resources and unchanged data URIs; documents using glTF extensions remain incomplete until those dependencies are inspected."
        ]
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|_| error("cannot serialize model asset manifest"))?;
    if manifest_bytes.len() > 8 * 1024 * 1024 {
        return Err(error("model asset manifest exceeds its byte limit"));
    }

    let output = output.as_ref();
    let (parent, output_path, name) = resolve_new_directory(output)?;
    let staging_path = create_staging_directory(&parent, &name)?;
    let mut output_reserved = false;
    let mut published_files = Vec::<(PathBuf, String, usize)>::new();
    let write_result = (|| {
        fs::create_dir(staging_path.join("assets"))
            .map_err(|e| error(format!("cannot create staged asset directory: {e}")))?;
        for (relative, (digest, bytes)) in &unique_assets {
            let staged = staging_path.join(relative);
            create_parent_directories(&staging_path, relative)?;
            write_new_file(&staged, bytes)?;
            if sha256_hex(bytes) != *digest {
                return Err(error("model asset changed during staging"));
            }
        }
        write_new_file(&staging_path.join("manifest.json"), &manifest_bytes)?;

        fs::create_dir(&output_path).map_err(|e| {
            error(format!(
                "cannot reserve model export directory without overwrite: {e}"
            ))
        })?;
        output_reserved = true;
        let metadata = fs::symlink_metadata(&output_path)
            .map_err(|e| error(format!("cannot inspect model export reservation: {e}")))?;
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            output_reserved = false;
            return Err(error("model export reservation was replaced"));
        }
        fs::create_dir(output_path.join("assets"))
            .map_err(|e| error(format!("cannot create exported asset directory: {e}")))?;
        for (relative, (digest, bytes)) in &unique_assets {
            create_parent_directories(&output_path, relative)?;
            let source = staging_path.join(relative);
            let destination = output_path.join(relative);
            fs::hard_link(&source, &destination)
                .map_err(|e| error(format!("cannot publish asset without replacement: {e}")))?;
            published_files.push((destination, digest.clone(), bytes.len()));
        }
        let staged_manifest = staging_path.join("manifest.json");
        let final_manifest = output_path.join("manifest.json");
        fs::hard_link(&staged_manifest, &final_manifest)
            .map_err(|e| error(format!("cannot publish manifest without replacement: {e}")))?;
        published_files.push((
            final_manifest,
            sha256_hex(&manifest_bytes),
            manifest_bytes.len(),
        ));
        output_reserved = false;
        Ok(())
    })();

    if write_result.is_err() && output_reserved {
        cleanup_output(&output_path, &published_files);
    }
    remove_staging_tree(&staging_path);
    write_result?;
    Ok(json!({
        "schema":manifest["schema"].clone(),
        "schemaVersion":manifest["schemaVersion"].clone(),
        "sourceBoardSha256":manifest["sourceBoardSha256"].clone(),
        "outputDirectory":output_path.to_string_lossy(),
        "manifest":"manifest.json",
        "models":manifest["models"].clone(),
        "modelReferences":models.len(),
        "resolvedModelPayloads":extracted_rows,
        "unexportedModelReferences":unexported_rows,
        "uniqueAssetFiles":unique_assets.len(),
        "uniqueAssetBytes":unique_asset_bytes,
        "payloadBytes":total_bytes,
        "gltfDependencyFiles":dependency_file_count,
        "gltfDependencyPayloadBytes":dependency_payload_bytes,
        "partialExport":partial,
        "manifestSha256":sha256_hex(&manifest_bytes),
        "geometryDecoded":false,
        "componentSelectionReady":false,
        "mobileImportReady":false
    }))
}

fn asset_path(sha256: &str, format: &str) -> Result<String, EngineError> {
    if !valid_sha256(sha256) {
        return Err(error("model asset has an invalid SHA-256"));
    }
    let extension = match format.to_ascii_lowercase().as_str() {
        "step" | "stp" => "step",
        "wrl" | "vrml" => "wrl",
        "glb" => "glb",
        _ => {
            return Err(error(
                "model format cannot be exported as a single-file asset",
            ));
        }
    };
    Ok(format!("assets/{sha256}.{extension}"))
}

fn gltf_bundle_sha256(asset: &ModelAssetPayload) -> Result<String, EngineError> {
    let mut dependencies: Vec<_> = asset.dependencies.iter().collect();
    dependencies.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let mut identity = Vec::new();
    let mut portable_paths = BTreeMap::<String, String>::new();
    append_identity_field(&mut identity, asset.sha256.as_bytes())?;
    for dependency in dependencies {
        validate_relative_resource_path(&dependency.relative_path)?;
        if gltf_path_collides_with_document(&dependency.relative_path) {
            return Err(error("glTF dependency path collides with its document"));
        }
        let folded_path = dependency.relative_path.to_lowercase();
        if portable_paths
            .insert(folded_path, dependency.relative_path.clone())
            .is_some_and(|existing| existing != dependency.relative_path)
        {
            return Err(error(
                "glTF dependency paths collide on a case-insensitive filesystem",
            ));
        }
        if !valid_sha256(&dependency.sha256) || sha256_hex(&dependency.bytes) != dependency.sha256 {
            return Err(error("glTF bundle dependency hash mismatch"));
        }
        append_identity_field(&mut identity, dependency.relative_path.as_bytes())?;
        append_identity_field(&mut identity, dependency.sha256.as_bytes())?;
    }
    Ok(sha256_hex(&identity))
}

fn append_identity_field(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EngineError> {
    let length = u64::try_from(bytes.len()).map_err(|_| error("glTF bundle field is too large"))?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn gltf_path_collides_with_document(path: &str) -> bool {
    path.split('/')
        .next()
        .is_some_and(|first| first.eq_ignore_ascii_case("scene.gltf"))
}

fn validate_relative_resource_path(path: &str) -> Result<(), EngineError> {
    if path.is_empty()
        || path.len() > 16 * 1024
        || path.starts_with('/')
        || path.contains(['\\', ':', '?', '#'])
        || path.split('/').any(|segment| {
            segment.is_empty()
                || segment == "."
                || segment == ".."
                || segment.ends_with([' ', '.'])
                || windows_device_name(segment)
        })
        || path.split('/').count() > MAX_GLTF_PATH_SEGMENTS
    {
        return Err(error("invalid relative glTF resource path"));
    }
    Ok(())
}

fn windows_device_name(segment: &str) -> bool {
    let stem = segment
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "CLOCK$"
            | "CONIN$"
            | "CONOUT$"
            | "COM¹"
            | "COM²"
            | "COM³"
            | "LPT¹"
            | "LPT²"
            | "LPT³"
    ) || (stem.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.as_bytes()[3].is_ascii_digit())
}

fn insert_asset<'a>(
    assets: &mut BTreeMap<String, (String, &'a [u8])>,
    path: String,
    digest: &str,
    bytes: &'a [u8],
    collision_message: &'static str,
) -> Result<(), EngineError> {
    match assets.get(&path) {
        Some((existing_digest, existing_bytes))
            if existing_digest == digest && *existing_bytes == bytes =>
        {
            Ok(())
        }
        Some(_) => Err(error(collision_message)),
        None => {
            assets.insert(path, (digest.to_owned(), bytes));
            Ok(())
        }
    }
}

fn create_parent_directories(root: &Path, relative: &str) -> Result<(), EngineError> {
    validate_relative_resource_path(relative)?;
    let parent = Path::new(relative)
        .parent()
        .ok_or_else(|| error("model asset path has no parent"))?;
    let mut current = root.to_path_buf();
    for component in parent.components() {
        match component {
            Component::Normal(value) => current.push(value),
            _ => return Err(error("model asset path is not relative")),
        }
        match fs::create_dir(&current) {
            Ok(()) => {}
            Err(error_value) if error_value.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = fs::symlink_metadata(&current)
                    .map_err(|e| error(format!("cannot inspect model asset directory: {e}")))?;
                if !metadata.is_dir()
                    || metadata.file_type().is_symlink()
                    || is_reparse_point(&metadata)
                {
                    return Err(error("model asset parent is not a plain directory"));
                }
            }
            Err(error_value) => {
                return Err(error(format!(
                    "cannot create model asset parent directory: {error_value}"
                )));
            }
        }
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn resolve_new_directory(output: &Path) -> Result<(PathBuf, PathBuf, String), EngineError> {
    let parent_input = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let absolute_parent = if parent_input.is_absolute() {
        parent_input.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| error(format!("cannot resolve model export parent: {e}")))?
            .join(parent_input)
    };
    reject_linked_path_components(&absolute_parent)?;
    let parent = absolute_parent
        .canonicalize()
        .map_err(|e| error(format!("model export parent must exist: {e}")))?;
    if !parent.is_dir() {
        return Err(error("model export parent is not a directory"));
    }
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| {
            !value.is_empty()
                && *value != "."
                && *value != ".."
                && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| error("model export needs a valid directory name"))?
        .to_owned();
    let output_path = parent.join(&name);
    match fs::symlink_metadata(&output_path) {
        Ok(_) => return Err(error("model export destination exists; refusing overwrite")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(error(format!(
                "cannot inspect model export destination: {e}"
            )));
        }
    }
    Ok((parent, output_path, name))
}

fn reject_linked_path_components(path: &Path) -> Result<(), EngineError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => current.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => current.push(component.as_os_str()),
            Component::Normal(value) => current.push(value),
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || is_reparse_point(&metadata) => {
                return Err(error(
                    "model export path contains a symbolic link or reparse point",
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(error(format!("cannot inspect model export path: {e}"))),
        }
    }
    Ok(())
}

fn create_staging_directory(parent: &Path, name: &str) -> Result<PathBuf, EngineError> {
    for attempt in 0..100u32 {
        let path = parent.join(format!(
            ".{name}.tokito-{}-{attempt}.stage",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(error(format!("cannot create model staging directory: {e}"))),
        }
    }
    Err(error("cannot reserve a unique model staging directory"))
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), EngineError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            error(format!(
                "cannot create model export file without replacement: {e}"
            ))
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| error(format!("cannot write model export file: {e}")))
}

fn cleanup_output(path: &Path, files: &[(PathBuf, String, usize)]) {
    for (file, digest, size) in files.iter().rev() {
        let Ok(metadata) = fs::symlink_metadata(file) else {
            continue;
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            continue;
        }
        if metadata.len() > *size as u64 {
            continue;
        }
        if fs::read(file).is_ok_and(|bytes| sha256_hex(&bytes) == *digest) {
            let _ = fs::remove_file(file);
        }
    }
    remove_empty_directory_tree(&path.join("assets"));
    remove_empty_directory(path);
}

fn remove_empty_directory_tree(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        let Ok(child_metadata) = fs::symlink_metadata(&child) else {
            continue;
        };
        if child_metadata.is_dir()
            && !child_metadata.file_type().is_symlink()
            && !is_reparse_point(&child_metadata)
        {
            remove_empty_directory_tree(&child);
        }
    }
    remove_empty_directory(path);
}

fn remove_staging_tree(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let item = entry.path();
        let Ok(item_metadata) = fs::symlink_metadata(&item) else {
            continue;
        };
        if item_metadata.is_dir()
            && !item_metadata.file_type().is_symlink()
            && !is_reparse_point(&item_metadata)
        {
            remove_staging_tree(&item);
        } else if item_metadata.is_file()
            && !item_metadata.file_type().is_symlink()
            && !is_reparse_point(&item_metadata)
        {
            let _ = fs::remove_file(item);
        }
    }
    remove_empty_directory(path);
}

fn remove_empty_directory(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() && !is_reparse_point(&metadata) {
        let _ = fs::remove_dir(path);
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn error(message: impl Into<String>) -> EngineError {
    EngineError::new(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    fn test_root() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tokito-model-export-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    fn row(status: &str, format: &str) -> Value {
        json!({
            "source":{"kind":"model","ordinal":4,"sourcePath":[4],"componentOrdinal":2},
            "status":status,
            "format":format,
            "ownerComponent":{"componentUuid":"uuid-1","reference":"C82","selectableIdentityReady":true}
        })
    }

    fn payload(index: usize, format: &str, bytes: &[u8]) -> ModelAssetPayload {
        ModelAssetPayload {
            model_index: index,
            format: format.into(),
            sha256: sha256_hex(bytes),
            bytes: bytes.to_vec(),
            dependencies: Vec::new(),
        }
    }

    fn resolved_row(format: &str, bytes: &[u8]) -> Value {
        let mut value = row("resolved", format);
        value["sha256"] = json!(sha256_hex(bytes));
        value
    }

    #[test]
    fn exports_hash_addressed_source_with_owner_identity_and_unqualified_status() {
        let root = test_root();
        let bytes = b"ISO-10303-21;\nEND-ISO-10303-21;\n";
        let manifest = export_model_assets(
            root.join("assets-out"),
            &"a".repeat(64),
            vec![resolved_row("step", bytes)],
            &[payload(0, "step", bytes)],
        )
        .unwrap();
        let output = root.join("assets-out");
        let written: Value =
            serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
        let asset_path = written["models"][0]["extraction"]["assetPath"]
            .as_str()
            .unwrap();
        assert_eq!(fs::read(output.join(asset_path)).unwrap(), bytes);
        assert_eq!(written["sourceBoardSha256"], "a".repeat(64));
        assert_eq!(written["models"][0]["ownerComponent"]["reference"], "C82");
        assert_eq!(written["models"][0]["extraction"]["geometryDecoded"], false);
        assert_eq!(written["geometryValidated"], false);
        assert_eq!(manifest["uniqueAssetFiles"], 1);
        assert_eq!(manifest["uniqueAssetBytes"], bytes.len());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn deduplicates_identical_payloads_but_keeps_each_component_row() {
        let root = test_root();
        let bytes = b"same model payload";
        let manifest = export_model_assets(
            root.join("dedup"),
            &"b".repeat(64),
            vec![resolved_row("wrl", bytes), resolved_row("wrl", bytes)],
            &[payload(0, "wrl", bytes), payload(1, "wrl", bytes)],
        )
        .unwrap();
        assert_eq!(manifest["payloadBytes"], bytes.len() * 2);
        assert_eq!(manifest["resolvedModelPayloads"], 2);
        assert_eq!(manifest["uniqueAssetFiles"], 1);
        assert_eq!(manifest["uniqueAssetBytes"], bytes.len());
        assert_eq!(
            manifest["models"][0]["extraction"]["assetPath"],
            manifest["models"][1]["extraction"]["assetPath"]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn exports_gltf_document_and_relative_resources_as_a_closed_hashed_bundle() {
        let root = test_root();
        let document = br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"mesh.bin","byteLength":4}],"images":[{"uri":"textures/board%20photo.png"}]}"#;
        let binary = b"mesh";
        let image = b"image-bytes";
        let mut gltf = payload(0, "gltf", document);
        gltf.dependencies = vec![
            ModelAssetDependencyPayload {
                relative_path: "mesh.bin".into(),
                sha256: sha256_hex(binary),
                bytes: binary.to_vec(),
            },
            ModelAssetDependencyPayload {
                relative_path: "textures/board photo.png".into(),
                sha256: sha256_hex(image),
                bytes: image.to_vec(),
            },
        ];
        let result = export_model_assets(
            root.join("gltf-out"),
            &"e".repeat(64),
            vec![resolved_row("gltf", document)],
            &[gltf],
        )
        .unwrap();
        let output = root.join("gltf-out");
        let manifest: Value =
            serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
        let row = &manifest["models"][0];
        assert_eq!(row["extraction"]["status"], "exported_gltf_bundle");
        assert_eq!(
            row["extraction"]["dependencies"].as_array().unwrap().len(),
            2
        );
        let scene = output.join(row["extraction"]["assetPath"].as_str().unwrap());
        assert_eq!(fs::read(&scene).unwrap(), document);
        let scene_dir = scene.parent().unwrap();
        assert_eq!(fs::read(scene_dir.join("mesh.bin")).unwrap(), binary);
        assert_eq!(
            fs::read(scene_dir.join("textures/board photo.png")).unwrap(),
            image
        );
        assert_eq!(manifest["gltfDependencyFiles"], 2);
        assert_eq!(
            manifest["gltfDependencyPayloadBytes"],
            binary.len() + image.len()
        );
        assert_eq!(
            manifest["payloadBytes"],
            document.len() + binary.len() + image.len()
        );
        assert_eq!(result["partialExport"], false);
        assert_eq!(result["mobileImportReady"], false);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_gltf_dependency_subtree_that_collides_with_emitted_document() {
        let root = test_root();
        let document = br#"{"asset":{"version":"2.0"}}"#;
        let dependency = b"mesh";
        let mut gltf = payload(0, "gltf", document);
        gltf.dependencies = vec![ModelAssetDependencyPayload {
            relative_path: "scene.gltf/mesh.bin".into(),
            sha256: sha256_hex(dependency),
            bytes: dependency.to_vec(),
        }];
        let output = root.join("collision");
        assert!(
            export_model_assets(
                &output,
                &"f".repeat(64),
                vec![resolved_row("gltf", document)],
                &[gltf],
            )
            .is_err()
        );
        assert!(!output.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unresolved_and_external_gltf_rows_are_reported_without_claiming_complete_export() {
        let root = test_root();
        let manifest = export_model_assets(
            root.join("partial"),
            &"c".repeat(64),
            vec![row("resolved", "gltf"), row("missing", "step")],
            &[],
        )
        .unwrap();
        assert_eq!(manifest["partialExport"], true);
        let written: Value =
            serde_json::from_slice(&fs::read(root.join("partial").join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(
            written["models"][0]["extraction"]["status"],
            "external_gltf_dependencies_not_collected"
        );
        assert_eq!(
            written["models"][1]["extraction"]["status"],
            "source_model_unresolved"
        );
        assert_eq!(written["componentSelectionReady"], false);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_existing_destination_and_preserves_its_contents() {
        let root = test_root();
        let output = root.join("existing");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("keep.txt"), b"user data").unwrap();
        assert!(
            export_model_assets(
                output.clone(),
                &"d".repeat(64),
                vec![row("resolved", "step")],
                &[]
            )
            .is_err()
        );
        assert_eq!(fs::read(output.join("keep.txt")).unwrap(), b"user data");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_payload_hash_mismatch_before_creating_output() {
        let root = test_root();
        let mut item = payload(0, "step", b"bytes");
        item.sha256 = "0".repeat(64);
        let output = root.join("bad");
        assert!(
            export_model_assets(
                output.clone(),
                &"e".repeat(64),
                vec![resolved_row("step", b"bytes")],
                &[item]
            )
            .is_err()
        );
        assert!(!output.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_payload_that_does_not_match_reported_model_identity() {
        let root = test_root();
        let bytes = b"body";
        let mut mismatched = resolved_row("wrl", bytes);
        mismatched["format"] = json!("step");
        let output = root.join("mismatch");
        assert!(
            export_model_assets(
                output.clone(),
                &"f".repeat(64),
                vec![mismatched],
                &[payload(0, "wrl", bytes)],
            )
            .is_err()
        );
        assert!(!output.exists());
        let _ = fs::remove_dir_all(root);
    }
}
