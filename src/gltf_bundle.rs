//! Bounded offline dependency capture for JSON glTF model documents.
//!
//! This closes only the core source-file dependency graph (`buffers` and `images`).
//! It does not parse meshes, apply PCB placement, or qualify geometry.

use crate::model_asset_export::ModelAssetDependencyPayload;
use crate::model_resolver::{ModelResolver, ResolvedModelAsset};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

const MAX_RESOURCES_PER_DOCUMENT: usize = 4096;
const MAX_URI_BYTES: usize = 4096;
const MAX_URI_SEGMENTS: usize = 64;

#[derive(Debug)]
pub(crate) struct GltfBundleResolution {
    pub(crate) dependencies: Vec<ModelAssetDependencyPayload>,
    pub(crate) report: Value,
    pub(crate) complete: bool,
}

/// Resolve every external buffer/image URI from a glTF document under the
/// source file's approved local root. Inline `data:` URIs remain embedded in
/// the unchanged document. Any malformed, remote, escaping, missing, linked,
/// oversized, or unreadable dependency keeps the bundle incomplete.
pub(crate) fn resolve_gltf_bundle(
    asset: &ResolvedModelAsset,
    resolver: &ModelResolver,
    max_dependency_bytes: usize,
) -> GltfBundleResolution {
    let (resource_paths, inline_count, reference_count) = match resource_paths(asset.bytes()) {
        Ok(resources) => resources,
        Err(status) => {
            return GltfBundleResolution {
                dependencies: Vec::new(),
                report: json!({
                    "status":"incomplete",
                        "resourceReferences":0,
                    "uniqueExternalResources":0,
                    "resolvedExternalResources":0,
                    "inlineDataResources":0,
                    "diagnostics":[{"resourceOrdinal":null,"status":status}],
                    "diagnosticsTruncated":false
                }),
                complete: false,
            };
        }
    };

    let mut dependencies = Vec::with_capacity(resource_paths.len());
    let mut diagnostics = Vec::new();
    let mut captured_dependency_bytes = 0usize;
    for (ordinal, relative_path) in resource_paths.iter().enumerate() {
        let remaining_bytes = max_dependency_bytes.saturating_sub(captured_dependency_bytes);
        let resolved =
            resolver.resolve_gltf_dependency(asset, &PathBuf::from(relative_path), remaining_bytes);
        match resolved {
            Ok(dependency) => {
                // The resolver rejects a file larger than this remaining
                // budget before retaining its bytes, so this sum stays within
                // `max_dependency_bytes`.
                captured_dependency_bytes += dependency.bytes.len();
                dependencies.push(ModelAssetDependencyPayload {
                    relative_path: relative_path.clone(),
                    sha256: dependency.sha256,
                    bytes: dependency.bytes,
                });
            }
            Err(status) => diagnostics.push(json!({
                "resourceOrdinal":ordinal,
                "relativePath":relative_path,
                "status":status.as_str()
            })),
        }
    }
    let complete = diagnostics.is_empty();
    let status = if complete { "complete" } else { "incomplete" };
    GltfBundleResolution {
        dependencies,
        report: json!({
            "status":status,
            "resourceReferences":reference_count,
            "uniqueExternalResources":resource_paths.len(),
            "resolvedExternalResources":resource_paths.len().saturating_sub(diagnostics.len()),
            "inlineDataResources":inline_count,
            "diagnostics":diagnostics,
            "diagnosticsTruncated":false
        }),
        complete,
    }
}

/// Returns normalized local dependency paths, inline-URI count, and total URI
/// reference count. Repeated external paths are resolved and copied once.
fn resource_paths(bytes: &[u8]) -> Result<(Vec<String>, usize, usize), &'static str> {
    let text = std::str::from_utf8(bytes).map_err(|_| "invalid_gltf_json")?;
    // Do not accept serde_json's default last-key-wins behavior here. A
    // downstream glTF reader may choose another duplicate `uri`, which could
    // make the copied offline dependency set differ from the document's
    // effective resource graph.
    let document: Value = crate::strict_json::parse(text).map_err(|_| "invalid_gltf_json")?;
    let object = document.as_object().ok_or("invalid_gltf_root")?;
    let asset = object
        .get("asset")
        .and_then(Value::as_object)
        .ok_or("invalid_gltf_asset")?;
    if asset.get("version").and_then(Value::as_str) != Some("2.0") {
        return Err("unsupported_gltf_asset_version");
    }
    if has_extension_payload(&document) {
        return Err("gltf_extensions_not_inspected");
    }
    let mut paths = BTreeMap::<String, ()>::new();
    let mut portable_paths = BTreeMap::<String, String>::new();
    let mut inline_count = 0usize;
    let mut reference_count = 0usize;

    if let Some(buffers) = object.get("buffers") {
        let buffers = buffers.as_array().ok_or("invalid_gltf_buffers")?;
        for buffer in buffers {
            let buffer = buffer.as_object().ok_or("invalid_gltf_buffer")?;
            let uri = buffer
                .get("uri")
                .and_then(Value::as_str)
                .ok_or("gltf_buffer_uri_missing_or_invalid")?;
            classify_uri(
                uri,
                &mut paths,
                &mut portable_paths,
                &mut inline_count,
                &mut reference_count,
            )?;
        }
    }
    if let Some(images) = object.get("images") {
        let images = images.as_array().ok_or("invalid_gltf_images")?;
        for image in images {
            let image = image.as_object().ok_or("invalid_gltf_image")?;
            match (image.get("uri"), image.contains_key("bufferView")) {
                (Some(_), true) => return Err("gltf_image_source_ambiguous"),
                (Some(uri), false) => {
                    let uri = uri.as_str().ok_or("gltf_image_uri_invalid")?;
                    classify_uri(
                        uri,
                        &mut paths,
                        &mut portable_paths,
                        &mut inline_count,
                        &mut reference_count,
                    )?;
                }
                (None, true) => {}
                (None, false) => return Err("gltf_image_source_missing"),
            }
        }
    }
    if paths.len() > MAX_RESOURCES_PER_DOCUMENT {
        return Err("gltf_resource_count_exceeded");
    }
    if paths.keys().any(|path| {
        path.split('/')
            .next()
            .is_some_and(|first| first.eq_ignore_ascii_case("scene.gltf"))
    }) {
        return Err("gltf_resource_path_collides_with_exported_document");
    }
    Ok((paths.into_keys().collect(), inline_count, reference_count))
}

fn has_extension_payload(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            (key == "extensions"
                && value
                    .as_object()
                    .is_none_or(|extensions| !extensions.is_empty()))
                || (matches!(key.as_str(), "extensionsUsed" | "extensionsRequired")
                    && value
                        .as_array()
                        .is_none_or(|extensions| !extensions.is_empty()))
                || has_extension_payload(value)
        }),
        Value::Array(values) => values.iter().any(has_extension_payload),
        _ => false,
    }
}

fn classify_uri(
    uri: &str,
    paths: &mut BTreeMap<String, ()>,
    portable_paths: &mut BTreeMap<String, String>,
    inline_count: &mut usize,
    reference_count: &mut usize,
) -> Result<(), &'static str> {
    *reference_count = reference_count
        .checked_add(1)
        .ok_or("gltf_resource_count_exceeded")?;
    if *reference_count > MAX_RESOURCES_PER_DOCUMENT {
        return Err("gltf_resource_count_exceeded");
    }
    if uri.len() > MAX_URI_BYTES {
        return Err("gltf_resource_uri_too_long");
    }
    if uri
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("data:"))
    {
        *inline_count = inline_count
            .checked_add(1)
            .ok_or("gltf_resource_count_exceeded")?;
        return Ok(());
    }
    let relative = normalize_relative_uri(uri)?;
    let folded = relative.to_lowercase();
    if let Some(existing) = portable_paths.get(&folded) {
        if existing != &relative {
            return Err("gltf_external_uri_paths_collide_on_case_insensitive_filesystem");
        }
    } else {
        portable_paths.insert(folded, relative.clone());
    }
    paths.insert(relative, ());
    if paths.len() > MAX_RESOURCES_PER_DOCUMENT {
        return Err("gltf_resource_count_exceeded");
    }
    Ok(())
}

fn normalize_relative_uri(uri: &str) -> Result<String, &'static str> {
    if uri.is_empty()
        || uri.starts_with('/')
        || uri.starts_with("\\")
        || uri.contains(['\\', '?', '#', ':', '<', '>', '"', '|', '*', ' '])
    {
        return Err("gltf_external_uri_not_safe_relative_path");
    }
    let decoded = percent_decode(uri)?;
    if decoded.is_empty()
        || decoded.starts_with('/')
        || decoded.contains(['\\', '?', '#', ':', '<', '>', '"', '|', '*'])
        || decoded.chars().any(char::is_control)
    {
        return Err("gltf_external_uri_not_safe_relative_path");
    }
    let mut segments = Vec::new();
    for segment in decoded.split('/') {
        match segment {
            "" => return Err("gltf_external_uri_not_safe_relative_path"),
            "." => {}
            ".." => return Err("gltf_external_uri_traversal_rejected"),
            _ => {
                if segment.ends_with([' ', '.']) || windows_device_name(segment) {
                    return Err("gltf_external_uri_not_safe_relative_path");
                }
                segments.push(segment);
                if segments.len() > MAX_URI_SEGMENTS {
                    return Err("gltf_external_uri_path_depth_exceeded");
                }
            }
        }
    }
    if segments.is_empty() {
        return Err("gltf_external_uri_not_safe_relative_path");
    }
    Ok(segments.join("/"))
}

fn percent_decode(uri: &str) -> Result<String, &'static str> {
    let input = uri.as_bytes();
    let mut decoded = Vec::with_capacity(input.len());
    let mut cursor = 0usize;
    while cursor < input.len() {
        if input[cursor] == b'%' {
            let high = input.get(cursor + 1).and_then(|byte| hex(*byte));
            let low = input.get(cursor + 2).and_then(|byte| hex(*byte));
            let (Some(high), Some(low)) = (high, low) else {
                return Err("gltf_external_uri_percent_encoding_invalid");
            };
            decoded.push((high << 4) | low);
            cursor += 3;
        } else {
            decoded.push(input[cursor]);
            cursor += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| "gltf_external_uri_not_utf8")
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    use super::{normalize_relative_uri, resource_paths};

    #[test]
    fn normalizes_safe_uri_paths_and_deduplicates_references() {
        let bytes = br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"./meshes/a%20b.bin"},{"uri":"meshes/a%20b.bin"}],"images":[{"uri":"textures/x.png"},{"uri":"data:image/png;base64,AAAA"},{"bufferView":0}]}"#;
        let (paths, inline, refs) = resource_paths(bytes).unwrap();
        assert_eq!(paths, ["meshes/a b.bin", "textures/x.png"]);
        assert_eq!(inline, 1);
        assert_eq!(refs, 4);
    }

    #[test]
    fn rejects_traversal_schemes_query_and_windows_special_names() {
        for (uri, expected) in [
            ("../secret.bin", "gltf_external_uri_traversal_rejected"),
            ("%2e%2e/secret.bin", "gltf_external_uri_traversal_rejected"),
            (
                "https://example.test/a.bin",
                "gltf_external_uri_not_safe_relative_path",
            ),
            ("a.bin?token=x", "gltf_external_uri_not_safe_relative_path"),
            ("CON.png", "gltf_external_uri_not_safe_relative_path"),
            ("COM¹.bin", "gltf_external_uri_not_safe_relative_path"),
            ("LPT³.bin", "gltf_external_uri_not_safe_relative_path"),
            (
                "bad%2f%2fpath.bin",
                "gltf_external_uri_not_safe_relative_path",
            ),
            ("bad%00.bin", "gltf_external_uri_not_safe_relative_path"),
            ("bad name.bin", "gltf_external_uri_not_safe_relative_path"),
        ] {
            assert_eq!(normalize_relative_uri(uri), Err(expected), "{uri}");
        }
    }

    #[test]
    fn malformed_document_and_unbound_buffer_are_not_treated_as_closed() {
        assert_eq!(
            resource_paths(b"not json").unwrap_err(),
            "invalid_gltf_json"
        );
        assert_eq!(
            resource_paths(br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":4}]}"#)
                .unwrap_err(),
            "gltf_buffer_uri_missing_or_invalid"
        );
    }

    #[test]
    fn rejects_paths_that_collide_with_document_or_on_windows_case_aliases() {
        assert_eq!(
            resource_paths(br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"Scene.gltf"}]}"#)
                .unwrap_err(),
            "gltf_resource_path_collides_with_exported_document"
        );
        assert_eq!(
            resource_paths(
                br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"scene.gltf/mesh.bin"}]}"#
            )
            .unwrap_err(),
            "gltf_resource_path_collides_with_exported_document"
        );
        assert_eq!(
            resource_paths(br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"textures/A.bin"},{"uri":"textures/a.bin"}]}"#)
                .unwrap_err(),
            "gltf_external_uri_paths_collide_on_case_insensitive_filesystem"
        );
    }

    #[test]
    fn rejects_extension_resources_and_ambiguous_image_sources() {
        assert_eq!(
            resource_paths(
                br#"{"asset":{"version":"2.0"},"extensionsUsed":["EXT_meshopt_compression"]}"#
            )
            .unwrap_err(),
            "gltf_extensions_not_inspected"
        );
        assert_eq!(
            resource_paths(
                br#"{"asset":{"version":"2.0"},"images":[{"uri":"image.png","bufferView":0}]}"#
            )
            .unwrap_err(),
            "gltf_image_source_ambiguous"
        );
    }
}
