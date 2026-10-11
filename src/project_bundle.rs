//! Bounded project export bundles assembled from one imported PCB document.
//!
//! Bundles preserve the selected PCB bytes and carry hashes for every derived
//! payload. They are useful offline handoff artifacts; this schema is not the
//! Tokito App import contract and makes no physical-fit or AR claims.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use gltf::binary::Glb;
use serde_json::{Value, json};

use crate::model_resolver::ModelResolver;
use crate::{
    EngineError, GlbExportOptions, PhysicalUnits, SourceCoordinateFrame, SvgSide,
    copy_bounded_file, load_kicad_board, project_intake, read_bounded_file, sha256_hex,
    write_new_file,
};

const SCHEMA: &str = "tokito.project-export-bundle.v1";
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_BUNDLE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_BUNDLE_ENTRIES: usize = 32;
const MAX_GLTF_NODES: usize = 1_000_000;
const MAX_GLTF_MESHES: usize = 1_000_000;

const GENERATED_FILES: [(&str, &str, usize); 8] = [
    (
        "derived/inspection.json",
        "application/json",
        64 * 1024 * 1024,
    ),
    (
        "derived/placements.csv",
        "text/csv; charset=utf-8",
        32 * 1024 * 1024,
    ),
    ("derived/outline.json", "application/json", 64 * 1024 * 1024),
    ("derived/board.top.svg", "image/svg+xml", 64 * 1024 * 1024),
    (
        "derived/board.bottom.svg",
        "image/svg+xml",
        64 * 1024 * 1024,
    ),
    ("derived/board.glb", "model/gltf-binary", 128 * 1024 * 1024),
    (
        "derived/glb-coverage.json",
        "application/json",
        4 * 1024 * 1024,
    ),
    (
        "derived/component-map.json",
        "application/json",
        64 * 1024 * 1024,
    ),
];

/// Inputs for a new, immutable project export directory.
pub struct ProjectBundleBuildOptions<'a> {
    pub project_root: &'a Path,
    pub board_filename: Option<&'a str>,
    pub expected_sha256: Option<&'a str>,
    pub output: &'a Path,
    pub allow_partial: bool,
    pub model_resolver: Option<&'a ModelResolver>,
}

struct Payload {
    path: &'static str,
    media_type: &'static str,
    bytes: Vec<u8>,
}

type GlbSelectorIndex = (BTreeMap<String, Vec<usize>>, BTreeSet<usize>);

/// Build a bounded bundle. The output directory must not exist and must be
/// outside the selected source project.
pub fn build_project_bundle(options: ProjectBundleBuildOptions<'_>) -> Result<Value, EngineError> {
    let intake = project_intake::inspect_project_with_sha256(
        options.project_root,
        options.board_filename,
        options.expected_sha256,
    )?;
    if !intake.accepted() {
        return Err(EngineError::new(format!(
            "project intake refused bundle build: {}",
            intake.diagnostics.join("; ")
        )));
    }
    let selected = intake
        .selected_board
        .as_ref()
        .ok_or_else(|| EngineError::new("accepted project intake has no selected PCB"))?;
    let board_path = options.project_root.join(&selected.relative_path);
    reject_link_or_special_file(&board_path, "selected PCB")?;
    let board = load_kicad_board(&board_path)?;
    let source = board.source();
    if source.bytes != selected.bytes
        || source.sha256 != selected.sha256
        || source.format_version != selected.format_version
    {
        return Err(EngineError::new(
            "selected PCB changed between project inspection and import; retry against a stable source",
        ));
    }
    if source.units != PhysicalUnits::Millimetres
        || source.coordinate_frame != SourceCoordinateFrame::KiCadBoardYDown
    {
        return Err(EngineError::new(
            "selected PCB importer returned an unsupported units or coordinate frame",
        ));
    }

    let (output_parent, output_path, output_name) =
        resolve_new_output(options.project_root, options.output)?;
    let inspection: Value = serde_json::from_str(&board.inspection_json())
        .map_err(|error| EngineError::new(format!("cannot parse canonical inspection: {error}")))?;
    let placements = board.placements_csv()?;
    let outline = board.outline_json();
    let top = board.svg(SvgSide::Top, None)?;
    let bottom = board.svg(SvgSide::Bottom, None)?;
    let glb = match options.model_resolver {
        Some(resolver) => board.export_glb_with_models(
            resolver,
            GlbExportOptions {
                allow_partial: options.allow_partial,
            },
        )?,
        None => board.export_glb(GlbExportOptions {
            allow_partial: options.allow_partial,
        })?,
    };
    let coverage_text = glb.report.report_json();
    let coverage: Value = serde_json::from_str(&coverage_text)
        .map_err(|error| EngineError::new(format!("cannot parse GLB coverage report: {error}")))?;
    let component_map = component_map(&inspection, &glb.bytes)?;

    let payloads = vec![
        payload(0, inspection_bytes(&inspection)?)?,
        payload(1, placements.into_bytes())?,
        payload(2, outline.into_bytes())?,
        payload(3, top.contents.into_bytes())?,
        payload(4, bottom.contents.into_bytes())?,
        payload(5, glb.bytes)?,
        payload(6, coverage_text.into_bytes())?,
        payload(7, json_bytes(&component_map)?)?,
    ];
    let mut total_bytes = source.bytes as u64;
    for item in &payloads {
        if item.bytes.len()
            > GENERATED_FILES
                .iter()
                .find(|(path, _, _)| *path == item.path)
                .expect("payload path is fixed")
                .2
        {
            return Err(EngineError::new(format!(
                "generated payload {} exceeds its size limit",
                item.path
            )));
        }
        total_bytes = total_bytes
            .checked_add(item.bytes.len() as u64)
            .ok_or_else(|| EngineError::new("bundle size overflow"))?;
    }
    if total_bytes > MAX_BUNDLE_BYTES {
        return Err(EngineError::new(format!(
            "bundle payloads exceed the {MAX_BUNDLE_BYTES}-byte total budget"
        )));
    }

    let artifact_records = payloads
        .iter()
        .map(|item| {
            json!({
                "path": item.path,
                "sha256": sha256_hex(&item.bytes),
                "bytes": item.bytes.len(),
                "mediaType": item.media_type
            })
        })
        .collect::<Vec<_>>();
    let manifest = json!({
        "schema": SCHEMA,
        "source": {
            "path": "source/board.kicad_pcb",
            "relativePath": selected.relative_path,
            "format": "kicad_pcb",
            "formatVersion": source.format_version,
            "bytes": source.bytes,
            "sha256": source.sha256,
            "units": "mm",
            "coordinateFrame": "KiCadBoardYDown"
        },
        "artifacts": artifact_records,
        "exportOptions": {
            "allowPartial": options.allow_partial,
            "modelResolverProvided": options.model_resolver.is_some(),
            "modelResolution": if options.model_resolver.is_some() { "local-supported-vrml-subset" } else { "board-shell-only" }
        },
        "svgWarnings": {
            "top": top.warnings,
            "bottom": bottom.warnings
        },
        "glbCoverage": coverage_summary(&coverage)?,
        "partialExport": coverage["partialExport"],
        "geometryComplete": false,
        "mobileImportReady": false,
        "appDigitalTwinManifestIncluded": false,
        "arRegistrationVerified": false,
        "limitations": [
            "The importer supports a reviewed subset of KiCad PCB data; this is not full KiCad validation or command parity.",
            "SVG and GLB contain only geometry supported by the current Rust importer and exporters.",
            "An emitted GLB selector means mesh geometry is present; it does not establish accurate physical fit or geometry qualification.",
            "This bundle is not the Tokito App digital-twin import format and has not passed mobile import acceptance.",
            "AR registration, camera alignment, and physical board correspondence are not verified.",
            "Source-adjacent licensing and provenance files are not copied automatically."
        ]
    });
    let manifest_bytes = json_bytes(&manifest)?;
    if manifest_bytes.len() > MAX_MANIFEST_BYTES {
        return Err(EngineError::new("bundle manifest exceeds its size limit"));
    }
    total_bytes = total_bytes
        .checked_add(manifest_bytes.len() as u64)
        .ok_or_else(|| EngineError::new("bundle size overflow"))?;
    if total_bytes > MAX_BUNDLE_BYTES {
        return Err(EngineError::new(format!(
            "bundle exceeds the {MAX_BUNDLE_BYTES}-byte total budget"
        )));
    }

    let (staging_path, mut staging_owned) = create_staging_directory(&output_parent, &output_name)?;
    let mut output_reservation_owned = false;
    let mut published_files = Vec::new();
    let build_result = (|| {
        fs::create_dir(staging_path.join("source")).map_err(|error| {
            EngineError::new(format!("cannot create staged source directory: {error}"))
        })?;
        fs::create_dir(staging_path.join("derived")).map_err(|error| {
            EngineError::new(format!("cannot create staged derived directory: {error}"))
        })?;
        let staged_source = staging_path.join("source/board.kicad_pcb");
        let (copied_bytes, copied_sha) =
            copy_bounded_file(source.path, &staged_source, crate::MAX_INPUT_BYTES)?;
        #[cfg(feature = "e2e-test-hooks")]
        if std::env::var_os("TOKITO_BUNDLE_TEST_FAIL_AFTER_SOURCE_COPY").is_some() {
            fs::write(&staged_source, b"injected staged-source corruption").map_err(|error| {
                EngineError::new(format!("cannot inject bundle cleanup test fault: {error}"))
            })?;
            return Err(EngineError::new(
                "injected failure after source copy for cleanup E2E",
            ));
        }
        if copied_bytes != source.bytes as u64 || copied_sha != source.sha256 {
            return Err(EngineError::new(
                "source PCB changed while it was copied into the bundle",
            ));
        }
        for item in &payloads {
            let relative = safe_bundle_relative_path(item.path)?;
            let target = staging_path.join(relative);
            write_new_file(&target, &item.bytes)?;
        }
        let current_source = read_bounded_file(source.path, crate::MAX_INPUT_BYTES)?;
        if current_source.len() != source.bytes || sha256_hex(&current_source) != source.sha256 {
            return Err(EngineError::new(
                "source PCB changed before bundle publication; staged bundle was discarded",
            ));
        }

        // Keep the destination reservation for the whole publication. Files
        // are linked into it without replacement, and the manifest is written
        // last as the commit marker. This avoids releasing the name before
        // publication, which would let a concurrent build replace it.
        fs::create_dir(&output_path).map_err(|error| {
            EngineError::new(format!(
                "cannot reserve bundle output without overwrite: {error}"
            ))
        })?;
        output_reservation_owned = true;
        let reservation = match fs::symlink_metadata(&output_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                output_reservation_owned = false;
                return Err(EngineError::new(format!(
                    "cannot inspect output reservation: {error}"
                )));
            }
        };
        if !reservation.is_dir() || is_reparse_point(&reservation) {
            output_reservation_owned = false;
            return Err(EngineError::new("bundle output reservation was replaced"));
        }

        fs::create_dir(output_path.join("source")).map_err(|error| {
            EngineError::new(format!(
                "cannot create reserved bundle source directory: {error}"
            ))
        })?;
        fs::create_dir(output_path.join("derived")).map_err(|error| {
            EngineError::new(format!(
                "cannot create reserved bundle derived directory: {error}"
            ))
        })?;

        publish_staged_file(
            &staged_source,
            &output_path.join("source/board.kicad_pcb"),
            &mut published_files,
            source.sha256,
            source.bytes,
        )?;
        for item in &payloads {
            let relative = safe_bundle_relative_path(item.path)?;
            publish_staged_file(
                &staging_path.join(&relative),
                &output_path.join(relative),
                &mut published_files,
                &sha256_hex(&item.bytes),
                item.bytes.len(),
            )?;
        }

        remove_staging_tree(&staging_path);
        if fs::symlink_metadata(&staging_path).is_ok() {
            return Err(EngineError::new(
                "could not remove staged bundle before publication",
            ));
        }
        staging_owned = false;

        // The App-independent bundle is considered complete only when this
        // exclusive manifest write succeeds. write_new_file uses create_new
        // plus a same-directory hard link, so it cannot replace a raced file.
        write_new_file(&output_path.join("manifest.json"), &manifest_bytes)?;
        published_files.push((
            output_path.join("manifest.json"),
            sha256_hex(&manifest_bytes),
            manifest_bytes.len(),
        ));
        output_reservation_owned = false;
        Ok(())
    })();
    if build_result.is_err() && staging_owned {
        remove_staging_tree(&staging_path);
    }
    if build_result.is_err() && output_reservation_owned {
        cleanup_published_output(&output_path, &published_files);
    }
    build_result?;

    Ok(manifest)
}

/// Verify exact bundle layout, payload hashes, source reimport identity, and
/// component selectors against the embedded GLB scene.
pub fn verify_project_bundle(path: impl AsRef<Path>) -> Result<Value, EngineError> {
    let root = reject_linked_path_components(path.as_ref())?;
    inventory_bundle(&root)?;
    let manifest_path = root.join("manifest.json");
    let manifest_bytes = read_regular_bounded(&manifest_path, MAX_MANIFEST_BYTES)?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| EngineError::new(format!("invalid bundle manifest JSON: {error}")))?;
    if manifest["schema"].as_str() != Some(SCHEMA) {
        return Err(EngineError::new("unsupported project bundle schema"));
    }
    for claim in [
        "geometryComplete",
        "mobileImportReady",
        "appDigitalTwinManifestIncluded",
        "arRegistrationVerified",
    ] {
        if manifest[claim].as_bool() != Some(false) {
            return Err(EngineError::new(format!(
                "bundle contains an invalid qualification claim: {claim}"
            )));
        }
    }
    let source = &manifest["source"];
    if source["path"].as_str() != Some("source/board.kicad_pcb")
        || source["format"].as_str() != Some("kicad_pcb")
        || source["units"].as_str() != Some("mm")
        || source["coordinateFrame"].as_str() != Some("KiCadBoardYDown")
    {
        return Err(EngineError::new("bundle source metadata is invalid"));
    }
    let source_bytes =
        read_regular_bounded(&root.join("source/board.kicad_pcb"), crate::MAX_INPUT_BYTES)?;
    let source_sha = sha256_hex(&source_bytes);
    if source["bytes"].as_u64() != Some(source_bytes.len() as u64)
        || source["sha256"].as_str() != Some(source_sha.as_str())
    {
        return Err(EngineError::new(
            "bundled source size or SHA-256 does not match manifest",
        ));
    }
    let source_board = load_kicad_board(root.join("source/board.kicad_pcb"))?;
    let source_meta = source_board.source();
    if source_meta.bytes != source_bytes.len()
        || source_meta.sha256 != source_sha
        || source["formatVersion"].as_u64() != Some(source_meta.format_version)
    {
        return Err(EngineError::new(
            "bundled PCB reimport does not match the declared source size, digest, or format version",
        ));
    }

    let artifact_array = manifest["artifacts"]
        .as_array()
        .ok_or_else(|| EngineError::new("bundle manifest has no artifact list"))?;
    if artifact_array.len() != GENERATED_FILES.len() {
        return Err(EngineError::new(
            "bundle manifest artifact count is invalid",
        ));
    }
    let expected: BTreeMap<&str, (&str, usize)> = GENERATED_FILES
        .iter()
        .map(|(path, media, limit)| (*path, (*media, *limit)))
        .collect();
    let mut seen = BTreeSet::new();
    let mut total_bytes = source_bytes.len() as u64 + manifest_bytes.len() as u64;
    for record in artifact_array {
        let relative = record["path"]
            .as_str()
            .ok_or_else(|| EngineError::new("bundle artifact path is missing"))?;
        let (media, limit) = expected.get(relative).copied().ok_or_else(|| {
            EngineError::new(format!("unexpected bundle artifact path: {relative}"))
        })?;
        if !seen.insert(relative.to_owned()) || record["mediaType"].as_str() != Some(media) {
            return Err(EngineError::new(format!(
                "duplicate bundle artifact or invalid media type: {relative}"
            )));
        }
        let file = root.join(safe_bundle_relative_path(relative)?);
        let bytes = read_regular_bounded(&file, limit)?;
        let digest = sha256_hex(&bytes);
        if record["bytes"].as_u64() != Some(bytes.len() as u64)
            || record["sha256"].as_str() != Some(digest.as_str())
        {
            return Err(EngineError::new(format!(
                "bundle artifact size or SHA-256 mismatch: {relative}"
            )));
        }
        total_bytes = total_bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| EngineError::new("bundle size overflow"))?;
        if total_bytes > MAX_BUNDLE_BYTES {
            return Err(EngineError::new(
                "bundle exceeds the total verification budget",
            ));
        }
    }
    if seen.len() != GENERATED_FILES.len() {
        return Err(EngineError::new("bundle is missing a required artifact"));
    }
    let inspection: Value = serde_json::from_slice(&read_regular_bounded(
        &root.join("derived/inspection.json"),
        GENERATED_FILES[0].2,
    )?)
    .map_err(|error| EngineError::new(format!("invalid embedded inspection JSON: {error}")))?;
    let glb_bytes = read_regular_bounded(&root.join("derived/board.glb"), GENERATED_FILES[5].2)?;
    let expected_map = component_map(&inspection, &glb_bytes)?;
    let stored_map: Value = serde_json::from_slice(&read_regular_bounded(
        &root.join("derived/component-map.json"),
        GENERATED_FILES[7].2,
    )?)
    .map_err(|error| EngineError::new(format!("invalid component map JSON: {error}")))?;
    if stored_map != expected_map {
        return Err(EngineError::new(
            "component map does not match embedded inspection identities and reachable GLB selectors",
        ));
    }
    let coverage: Value = serde_json::from_slice(&read_regular_bounded(
        &root.join("derived/glb-coverage.json"),
        GENERATED_FILES[6].2,
    )?)
    .map_err(|error| EngineError::new(format!("invalid GLB coverage JSON: {error}")))?;
    let summary = coverage_summary(&coverage)?;
    if manifest["glbCoverage"] != summary || manifest["partialExport"] != summary["partialExport"] {
        return Err(EngineError::new(
            "manifest GLB coverage summary does not match the coverage artifact",
        ));
    }
    for side in ["top", "bottom"] {
        if manifest["svgWarnings"][side].as_array().is_none() {
            return Err(EngineError::new(format!(
                "missing SVG warning list for {side} view"
            )));
        }
    }
    let limitations = manifest["limitations"]
        .as_array()
        .ok_or_else(|| EngineError::new("bundle limitations are missing"))?;
    if limitations.is_empty() || limitations.iter().any(|item| item.as_str().is_none()) {
        return Err(EngineError::new("bundle limitations must be explicit text"));
    }

    Ok(json!({
        "schema": SCHEMA,
        "status": "verified",
        "sourceSha256": source_sha,
        "sourceBytes": source_bytes.len(),
        "formatVersion": source_meta.format_version,
        "artifactCount": seen.len(),
        "componentCount": expected_map["component_count"],
        "renderableCount": expected_map["renderable_count"],
        "geometryComplete": false,
        "mobileImportReady": false,
        "arRegistrationVerified": false,
        "totalBytes": total_bytes
    }))
}

fn payload(index: usize, bytes: Vec<u8>) -> Result<Payload, EngineError> {
    let (path, media_type, limit) = GENERATED_FILES
        .get(index)
        .copied()
        .ok_or_else(|| EngineError::new("invalid internal bundle payload index"))?;
    if bytes.len() > limit {
        return Err(EngineError::new(format!(
            "generated payload {path} exceeds its {limit}-byte limit"
        )));
    }
    Ok(Payload {
        path,
        media_type,
        bytes,
    })
}

fn inspection_bytes(value: &Value) -> Result<Vec<u8>, EngineError> {
    json_bytes(value)
}

fn json_bytes(value: &Value) -> Result<Vec<u8>, EngineError> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| EngineError::new(format!("cannot serialize bundle JSON: {error}")))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn component_map(inspection: &Value, glb_bytes: &[u8]) -> Result<Value, EngineError> {
    let footprints = inspection["footprints"]
        .as_array()
        .ok_or_else(|| EngineError::new("board inspection has no footprint list"))?;
    if footprints.len() > 100_000 {
        return Err(EngineError::new(
            "footprint count exceeds component-map limit",
        ));
    }
    let (selectors, mesh_nodes) = glb_selectors(glb_bytes)?;
    let mut reference_counts = BTreeMap::<String, usize>::new();
    let mut uuid_counts = BTreeMap::<String, usize>::new();
    for footprint in footprints {
        let reference = required_text(footprint, "reference")?;
        let uuid = required_text(footprint, "uuid")?;
        *reference_counts.entry(reference.to_owned()).or_default() += 1;
        *uuid_counts.entry(uuid.to_owned()).or_default() += 1;
    }
    let mut assigned = BTreeSet::new();
    let mut components = Vec::with_capacity(footprints.len());
    let mut renderable_count = 0usize;
    for footprint in footprints {
        let reference = required_text(footprint, "reference")?;
        let uuid = required_text(footprint, "uuid")?;
        let placeholder = footprint["reference_is_placeholder"]
            .as_bool()
            .ok_or_else(|| EngineError::new("inspection placeholder state is invalid"))?;
        let mut selector = None;
        let mut renderable = false;
        let reason = if placeholder {
            Some("placeholder_reference")
        } else if reference_counts.get(reference) != Some(&1) {
            Some("duplicate_reference")
        } else if uuid.is_empty() || uuid_counts.get(uuid) != Some(&1) {
            Some("missing_or_duplicate_uuid")
        } else if let Some(nodes) = selectors.get(reference) {
            if nodes.iter().any(|index| !assigned.insert(*index)) {
                return Err(EngineError::new(
                    "GLB component selector was assigned more than once",
                ));
            }
            if nodes.iter().any(|index| mesh_nodes.contains(index)) {
                renderable = true;
                renderable_count += 1;
                selector = Some(format!("ref:{reference}"));
                None
            } else {
                Some("selector_has_no_mesh_geometry")
            }
        } else {
            Some("component_model_not_emitted")
        };
        components.push(json!({
            "reference": reference,
            "uuid": uuid,
            "reference_is_placeholder": placeholder,
            "selector": selector.clone(),
            "glb_selector": selector.clone(),
            "renderable": renderable,
            "reason": reason,
            "physical_fit_verified": false,
            "geometry_qualified": false,
            "ar_registration_verified": false
        }));
    }
    Ok(json!({
        "schema": "tokito.project-export-component-map.v1",
        "component_count": components.len(),
        "renderable_count": renderable_count,
        "components": components,
        "qualification": {
            "physicalFitVerified": false,
            "geometryQualified": false,
            "arRegistrationVerified": false
        }
    }))
}

fn glb_selectors(glb_bytes: &[u8]) -> Result<GlbSelectorIndex, EngineError> {
    let container = Glb::from_slice(glb_bytes)
        .map_err(|error| EngineError::new(format!("invalid generated GLB container: {error}")))?;
    let document: Value = serde_json::from_slice(&container.json)
        .map_err(|error| EngineError::new(format!("invalid generated GLB JSON: {error}")))?;
    let nodes = document["nodes"]
        .as_array()
        .filter(|nodes| nodes.len() <= MAX_GLTF_NODES)
        .ok_or_else(|| EngineError::new("GLB node array is missing or exceeds bounds"))?;
    let meshes = document["meshes"]
        .as_array()
        .filter(|meshes| meshes.len() <= MAX_GLTF_MESHES)
        .ok_or_else(|| EngineError::new("GLB mesh array is missing or exceeds bounds"))?;
    let scenes = document["scenes"]
        .as_array()
        .ok_or_else(|| EngineError::new("GLB scene array is missing"))?;
    let active = document["scene"]
        .as_u64()
        .and_then(|index| usize::try_from(index).ok())
        .filter(|index| *index < scenes.len())
        .or_else(|| (document["scene"].is_null() && scenes.len() == 1).then_some(0))
        .ok_or_else(|| EngineError::new("GLB active scene is invalid or ambiguous"))?;
    let roots = scenes[active]["nodes"]
        .as_array()
        .ok_or_else(|| EngineError::new("GLB active scene has no root nodes"))?;
    let mut stack = roots.clone();
    let mut reachable = BTreeSet::new();
    let mut mesh_nodes = BTreeSet::new();
    while let Some(index_value) = stack.pop() {
        let index = index_value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|index| *index < nodes.len())
            .ok_or_else(|| EngineError::new("GLB scene references an invalid node"))?;
        if !reachable.insert(index) {
            return Err(EngineError::new(
                "GLB active scene has a cycle or shared node",
            ));
        }
        let node = &nodes[index];
        if let Some(mesh) = node["mesh"].as_u64() {
            if usize::try_from(mesh)
                .ok()
                .filter(|i| *i < meshes.len())
                .is_none()
            {
                return Err(EngineError::new("GLB node references an invalid mesh"));
            }
            mesh_nodes.insert(index);
        }
        if let Some(children) = node["children"].as_array() {
            stack.extend(children.iter().cloned());
        } else if !node["children"].is_null() {
            return Err(EngineError::new("GLB node children field is invalid"));
        }
    }
    let mut selectors = BTreeMap::<String, Vec<usize>>::new();
    for (index, node) in nodes.iter().enumerate() {
        let Some(name) = node["name"]
            .as_str()
            .filter(|name| name.starts_with("ref:"))
        else {
            continue;
        };
        if !reachable.contains(&index) {
            return Err(EngineError::new(
                "GLB contains an unreachable component selector",
            ));
        }
        let reference = name.strip_prefix("ref:").unwrap_or_default();
        if reference.is_empty() {
            return Err(EngineError::new("GLB contains an empty component selector"));
        }
        if selectors
            .insert(reference.to_owned(), vec![index])
            .is_some()
        {
            return Err(EngineError::new(
                "GLB contains duplicate component selectors",
            ));
        }
    }
    // A renderable selector may own mesh-bearing descendants. Resolve each
    // selector subtree only through nodes in the active, validated scene.
    for (reference, roots) in &mut selectors {
        let mut todo = roots.clone();
        let mut subtree_meshes = BTreeSet::new();
        while let Some(index) = todo.pop() {
            if mesh_nodes.contains(&index) {
                subtree_meshes.insert(index);
            }
            if let Some(children) = nodes[index]["children"].as_array() {
                for child in children {
                    let child_index = child
                        .as_u64()
                        .and_then(|v| usize::try_from(v).ok())
                        .ok_or_else(|| EngineError::new("GLB selector subtree index is invalid"))?;
                    todo.push(child_index);
                }
            }
        }
        if !reachable.contains(&roots[0]) || reference.is_empty() {
            return Err(EngineError::new("GLB selector is invalid"));
        }
        *roots = subtree_meshes.into_iter().collect();
    }
    Ok((selectors, mesh_nodes))
}

fn coverage_summary(coverage: &Value) -> Result<Value, EngineError> {
    let fields = [
        "scope",
        "partialExport",
        "shellGeometryComplete",
        "wholeBoardDigitalTwinComplete",
    ];
    let coverage_body = &coverage["coverage"];
    let mut summary = serde_json::Map::new();
    for field in fields {
        if coverage[field].is_null() {
            return Err(EngineError::new(format!("GLB coverage is missing {field}")));
        }
        summary.insert(field.to_owned(), coverage[field].clone());
    }
    for field in [
        "componentCount",
        "componentModelsEmitted",
        "componentModelsUnresolved",
        "modelReferences",
        "modelReferencesEmitted",
        "modelReferencesUnresolved",
    ] {
        if coverage_body[field].as_u64().is_none() {
            return Err(EngineError::new(format!("GLB coverage is missing {field}")));
        }
        summary.insert(field.to_owned(), coverage_body[field].clone());
    }
    Ok(Value::Object(summary))
}

fn required_text<'a>(object: &'a Value, field: &str) -> Result<&'a str, EngineError> {
    object[field]
        .as_str()
        .ok_or_else(|| EngineError::new(format!("inspection footprint has invalid {field}")))
}

fn resolve_new_output(
    project_root: &Path,
    output: &Path,
) -> Result<(PathBuf, PathBuf, String), EngineError> {
    let root = project_root
        .canonicalize()
        .map_err(|error| EngineError::new(format!("cannot resolve project root: {error}")))?;
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|error| EngineError::new(format!("bundle output parent must exist: {error}")))?;
    if !parent.is_dir() {
        return Err(EngineError::new("bundle output parent is not a directory"));
    }
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .ok_or_else(|| EngineError::new("bundle output needs a valid directory name"))?
        .to_owned();
    let final_path = parent.join(&name);
    if final_path.starts_with(&root) || root.starts_with(&final_path) {
        return Err(EngineError::new(
            "bundle output must be outside and non-overlapping with the source project",
        ));
    }
    match fs::symlink_metadata(&final_path) {
        Ok(_) => {
            return Err(EngineError::new(
                "bundle output already exists; refusing overwrite",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(EngineError::new(format!(
                "cannot inspect bundle output: {error}"
            )));
        }
    }
    Ok((parent, final_path, name))
}

fn create_staging_directory(parent: &Path, name: &str) -> Result<(PathBuf, bool), EngineError> {
    for attempt in 0..100u32 {
        let path = parent.join(format!(
            ".{name}.tokito-{}-{attempt}.stage",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok((path, true)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(EngineError::new(format!(
                    "cannot create bundle staging directory: {error}"
                )));
            }
        }
    }
    Err(EngineError::new(
        "cannot reserve a unique bundle staging directory",
    ))
}

fn publish_staged_file(
    staged: &Path,
    destination: &Path,
    published: &mut Vec<(PathBuf, String, usize)>,
    digest: &str,
    size: usize,
) -> Result<(), EngineError> {
    fs::hard_link(staged, destination).map_err(|error| {
        EngineError::new(format!(
            "cannot publish bundle file without replacing an existing path: {error}"
        ))
    })?;
    published.push((destination.to_path_buf(), digest.to_owned(), size));
    Ok(())
}

fn remove_staging_tree(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() && !is_reparse_point(&metadata) {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            remove_staging_tree(&entry.path());
        }
        remove_empty_regular_directory(path);
    } else if metadata.is_file()
        && !metadata.file_type().is_symlink()
        && !is_reparse_point(&metadata)
    {
        let _ = fs::remove_file(path);
    }
}

fn cleanup_published_output(path: &Path, published: &[(PathBuf, String, usize)]) {
    for (file, digest, size) in published {
        remove_owned_file(file, digest, *size);
    }
    remove_empty_regular_directory(&path.join("source"));
    remove_empty_regular_directory(&path.join("derived"));
    remove_empty_regular_directory(path);
}

fn remove_owned_file(path: &Path, expected_sha: &str, limit: usize) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return;
    }
    let Ok(bytes) = read_bounded_file(path, limit) else {
        return;
    };
    if sha256_hex(&bytes) == expected_sha {
        let _ = fs::remove_file(path);
    }
}

fn remove_empty_regular_directory(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() && !is_reparse_point(&metadata) {
        let _ = fs::remove_dir(path);
    }
}

fn safe_bundle_relative_path(path: &str) -> Result<PathBuf, EngineError> {
    let parsed = Path::new(path);
    if parsed.is_absolute()
        || parsed
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || parsed.to_string_lossy().replace('\\', "/") != path
    {
        return Err(EngineError::new("bundle contains an unsafe relative path"));
    }
    Ok(parsed.to_path_buf())
}

fn inventory_bundle(root: &Path) -> Result<BTreeSet<String>, EngineError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| EngineError::new(format!("cannot inspect bundle directory: {error}")))?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Err(EngineError::new("bundle root is not a regular directory"));
    }
    let mut paths = BTreeSet::new();
    let mut total_entries = 0usize;
    let mut stack = vec![(root.to_path_buf(), String::new(), 0usize)];
    while let Some((directory, relative_parent, depth)) = stack.pop() {
        if depth > 2 {
            return Err(EngineError::new(
                "bundle directory depth exceeds the schema limit",
            ));
        }
        for entry in fs::read_dir(&directory)
            .map_err(|error| EngineError::new(format!("cannot list bundle directory: {error}")))?
        {
            total_entries += 1;
            if total_entries > MAX_BUNDLE_ENTRIES {
                return Err(EngineError::new("bundle has too many files or directories"));
            }
            let entry = entry
                .map_err(|error| EngineError::new(format!("cannot read bundle entry: {error}")))?;
            let file_name = entry
                .file_name()
                .into_string()
                .map_err(|_| EngineError::new("bundle path is not UTF-8"))?;
            let relative = if relative_parent.is_empty() {
                file_name.clone()
            } else {
                format!("{relative_parent}/{file_name}")
            };
            let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
                EngineError::new(format!("cannot inspect bundle entry: {error}"))
            })?;
            if is_reparse_point(&metadata) || metadata.file_type().is_symlink() {
                return Err(EngineError::new(format!(
                    "bundle contains a symlink or reparse point: {relative}"
                )));
            }
            if metadata.is_dir() {
                if !matches!(relative.as_str(), "source" | "derived") {
                    return Err(EngineError::new(format!(
                        "unexpected bundle directory: {relative}"
                    )));
                }
                paths.insert(format!("{relative}/"));
                stack.push((entry.path(), relative, depth + 1));
            } else if metadata.is_file() {
                paths.insert(relative);
            } else {
                return Err(EngineError::new(
                    "bundle contains a non-regular filesystem entry",
                ));
            }
        }
    }
    let expected = [
        "manifest.json",
        "source/",
        "source/board.kicad_pcb",
        "derived/",
        "derived/inspection.json",
        "derived/placements.csv",
        "derived/outline.json",
        "derived/board.top.svg",
        "derived/board.bottom.svg",
        "derived/board.glb",
        "derived/glb-coverage.json",
        "derived/component-map.json",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();
    if paths != expected {
        let unexpected = paths.difference(&expected).next();
        let missing = expected.difference(&paths).next();
        return Err(EngineError::new(format!(
            "bundle layout is not exact (unexpected: {}, missing: {})",
            unexpected.map_or("none", String::as_str),
            missing.map_or("none", String::as_str)
        )));
    }
    Ok(paths)
}

fn reject_linked_path_components(path: &Path) -> Result<PathBuf, EngineError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                EngineError::new(format!("cannot resolve current directory: {error}"))
            })?
            .join(path)
    };
    let mut current = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(EngineError::new(
                    "bundle path must not contain parent traversal",
                ));
            }
            Component::Normal(name) => current.push(name),
        }
        let metadata = fs::symlink_metadata(&current).map_err(|error| {
            EngineError::new(format!("cannot inspect bundle path component: {error}"))
        })?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(EngineError::new(
                "bundle path contains a symlink or reparse point",
            ));
        }
        if current != absolute && !metadata.is_dir() {
            return Err(EngineError::new("bundle path parent is not a directory"));
        }
    }
    Ok(absolute)
}

fn reject_link_or_special_file(path: &Path, label: &str) -> Result<(), EngineError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| EngineError::new(format!("cannot inspect {label}: {error}")))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return Err(EngineError::new(format!("{label} is not a regular file")));
    }
    Ok(())
}

fn read_regular_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, EngineError> {
    reject_link_or_special_file(path, "bundle file")?;
    let bytes = read_bounded_file(path, limit)?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| EngineError::new(format!("cannot re-inspect bundle file: {error}")))?;
    if !metadata.is_file() || is_reparse_point(&metadata) || metadata.len() != bytes.len() as u64 {
        return Err(EngineError::new("bundle file changed during verification"));
    }
    Ok(bytes)
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
