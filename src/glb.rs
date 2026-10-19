//! Pure-Rust GLB export for a validated board shell and a bounded local VRML subset.
//! Source: KiCad board files use mm and a saved Y-down frame. Output is metres,
//! right-handed, Y-up, with the board plane in XZ and its slab centered at Y=0.
//! GLB container alignment follows Khronos glTF 2.0:
//! https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/Specification.adoc
//! General VRML, STEP, glTF and GLB component models remain unsupported; neither
//! export scope claims a complete digital twin or a verified phone import.

use super::document::Point2;
use super::model_assembly::{
    ModelAssembly, ModelAssemblyFilter, assemble_supported_vrml_models_for_components,
};
use super::model_resolver::ModelResolver;
use super::topology::{ContourRole, OutlineContour, OutlineShape};
use super::{EngineError, PhysicalBoardDocument};
use earcut::Earcut;
use gltf::binary::{Glb, Header};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};

const MM_TO_M: f64 = 0.001;
const GLB_REPORT_VERSION: &str = "1.2.0";
const MAX_THICKNESS_MM: f64 = 1_000.0;
const MAX_POLYGON_POINTS: usize = 50_000;
const MAX_GLTF_VERTICES: usize = 1_000_000;
const MAX_GLB_BYTES: usize = 128 * 1024 * 1024;
const MAX_COVERAGE_COMPONENTS: usize = 500;
const MAX_COVERAGE_MODELS: usize = 2_048;
const MAX_MODEL_PATHS_PER_COMPONENT: usize = 16;
const MAX_MODEL_PATH_CHARS: usize = 256;
const MAX_FLOAT_POSITION_ERROR_M: f64 = 0.000_001;
const MAX_CHORD_ERROR_MM: f64 = 0.01;
const AREA_REL_TOLERANCE: f64 = 1e-10;
const AREA_ABS_TOLERANCE_MM2: f64 = 1e-8;

#[derive(Clone, Debug)]
pub struct GlbExport {
    pub bytes: Vec<u8>,
    pub report: GlbCoverageReport,
}

/// Source-model filter evidence on the additive model-assembly API.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DnpFilterCoverage {
    pub applied: bool,
    pub components_excluded: usize,
    pub model_references_excluded: usize,
}

/// Intentional native footprint-type exclusions. These counts may overlap DNP
/// exclusions; the ordinary selected/filtered counters describe their union.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FootprintTypeFilterCoverage {
    pub components_excluded: usize,
    pub model_references_excluded: usize,
    pub overlap_with_dnp_components: usize,
}

impl FootprintTypeFilterCoverage {
    fn as_json(&self) -> Value {
        json!({
            "enabled": true,
            "componentsExcluded": self.components_excluded,
            "modelReferencesExcluded": self.model_references_excluded,
            "overlapWithDnpComponents": self.overlap_with_dnp_components
        })
    }
}

/// Model assembly output with variant-specific exclusion metadata.
#[derive(Clone, Debug)]
pub struct GlbModelExport {
    pub bytes: Vec<u8>,
    pub report: GlbCoverageReport,
    pub dnp_filter: DnpFilterCoverage,
}

impl GlbModelExport {
    /// Stable JSON receipt with intentional DNP exclusions when requested.
    pub fn report_json(&self) -> String {
        let mut value: Value = serde_json::from_str(&self.report.report_json())
            .expect("serialized GLB coverage report is valid JSON");
        if self.dnp_filter.applied {
            value["coverage"]["componentsIncluded"] = json!(self.report.selected_component_count);
            value["coverage"]["dnpFilter"] = json!({
                "enabled": true,
                "componentsExcluded": self.dnp_filter.components_excluded,
                "modelReferencesExcluded": self.dnp_filter.model_references_excluded
            });
        }
        format!(
            "{}\n",
            serde_json::to_string_pretty(&value).expect("finite GLB coverage report")
        )
    }
}

/// Explicit assigned-body export; assignments do not certify engineering fit.
#[derive(Clone, Debug)]
pub struct AssignedGlbExport {
    pub bytes: Vec<u8>,
    pub report: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlbCoverageReport {
    pub scope: &'static str,
    pub shell_geometry_complete: bool,
    pub whole_board_digital_twin_complete: bool,
    pub partial_export: bool,
    /// Total source-board component population; retained for compatibility.
    pub component_count: usize,
    /// Components admitted by the active model filters.
    pub selected_component_count: usize,
    pub source_component_count: usize,
    pub filtered_component_count: usize,
    pub component_filter: Option<Vec<String>>,
    pub no_dnp: bool,
    pub excluded_dnp_component_count: usize,
    /// Present only when the caller explicitly enabled native type filtering.
    pub unspecified_filter: Option<FootprintTypeFilterCoverage>,
    pub component_models_emitted: usize,
    pub component_models_unresolved: usize,
    pub model_references: usize,
    /// Model references remaining after component/DNP selection.
    pub selected_model_references: usize,
    pub source_model_references: usize,
    pub filtered_model_references: usize,
    pub model_references_emitted: usize,
    pub model_references_unresolved: usize,
    pub polygonized_vertices: usize,
    pub triangles: usize,
    pub exact_outline_area_mm2: f64,
    pub polygonized_outline_area_mm2: f64,
    pub triangulated_outline_area_mm2: f64,
    pub thickness_mm: f64,
    pub max_curve_chord_error_mm: f64,
    pub outline_chaining_tolerance_mm: f64,
    pub unresolved_components: Vec<UnresolvedComponentCoverage>,
    pub unresolved_components_total: usize,
    pub unresolved_components_truncated: bool,
    pub unresolved_models: Vec<UnresolvedModelCoverage>,
    pub unresolved_models_total: usize,
    pub unresolved_models_truncated: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnresolvedComponentCoverage {
    pub reference: Option<String>,
    pub uuid: Option<String>,
    pub source_kind: String,
    pub source_ordinal: usize,
    pub source_identity: String,
    pub model_paths: Vec<String>,
    pub model_reference_count: usize,
    pub model_references_without_path: usize,
    pub model_paths_truncated: bool,
    pub model_paths_redacted: bool,
    pub reason: &'static str,
    pub no_model_reference_reason: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnresolvedModelCoverage {
    pub component_reference: Option<String>,
    pub component_uuid: Option<String>,
    pub source_kind: String,
    pub source_ordinal: usize,
    pub source_identity: String,
    pub status: String,
}

impl GlbCoverageReport {
    /// Stable machine-readable receipt. It omits the local PCB file path but preserves bounded model paths read from the board.
    pub fn report_json(&self) -> String {
        let mut value = json!({
            "reportVersion": GLB_REPORT_VERSION,
            "scope": self.scope,
            "shellGeometryComplete": self.shell_geometry_complete,
            "wholeBoardDigitalTwinComplete": self.whole_board_digital_twin_complete,
            "partialExport": self.partial_export,
            "coverage": {
                "componentCount": self.component_count,
                "selectedComponentCount": self.selected_component_count,
                "sourceComponentCount": self.source_component_count,
                "filteredComponentCount": self.filtered_component_count,
                "componentFilter": &self.component_filter,
                "noDnp": self.no_dnp,
                "excludedDnpComponentCount": self.excluded_dnp_component_count,
                "componentModelsEmitted": self.component_models_emitted,
                "componentModelsUnresolved": self.component_models_unresolved,
                "modelReferences": self.model_references,
                "selectedModelReferences": self.selected_model_references,
                "sourceModelReferences": self.source_model_references,
                "filteredModelReferences": self.filtered_model_references,
                "modelReferencesEmitted": self.model_references_emitted,
                "modelReferencesUnresolved": self.model_references_unresolved,
                "unresolvedComponentsTotal": self.unresolved_components_total,
                "unresolvedComponentsTruncated": self.unresolved_components_truncated,
                "unresolvedComponents": self.unresolved_components.iter().map(|item| json!({
                    "reference": item.reference, "uuid": item.uuid, "sourceKind": item.source_kind,
                    "sourceOrdinal": item.source_ordinal, "sourceIdentity": item.source_identity,
                    "modelPaths": item.model_paths, "modelReferenceCount": item.model_reference_count,
                    "modelReferencesWithoutPath": item.model_references_without_path,
                    "modelPathsTruncated": item.model_paths_truncated,
                    "modelPathsRedacted": item.model_paths_redacted,
                    "reason": item.reason, "noModelReferenceReason": item.no_model_reference_reason
                })).collect::<Vec<_>>(),
                "unresolvedModelsTotal": self.unresolved_models_total,
                "unresolvedModelsTruncated": self.unresolved_models_truncated,
                "unresolvedModels": self.unresolved_models.iter().map(|item| json!({
                    "componentReference": item.component_reference,
                    "componentUuid": item.component_uuid,
                    "sourceKind": item.source_kind,
                    "sourceOrdinal": item.source_ordinal,
                    "sourceIdentity": item.source_identity,
                    "status": item.status
                })).collect::<Vec<_>>()
            },
            "mesh": {
                "polygonizedVertices": self.polygonized_vertices,
                "triangles": self.triangles,
                "exactOutlineAreaMm2": self.exact_outline_area_mm2,
                "polygonizedOutlineAreaMm2": self.polygonized_outline_area_mm2,
                "triangulatedOutlineAreaMm2": self.triangulated_outline_area_mm2,
                "thicknessMm": self.thickness_mm,
                "shellRepresentation": "nominal_thickness_envelope",
                "maxCurveChordErrorMm": self.max_curve_chord_error_mm,
                "outlineChainingToleranceMm": self.outline_chaining_tolerance_mm,
                "maxJoinDisplacementMm":self.outline_chaining_tolerance_mm/2.,
                "sourceGeometryEdited":false
            }
        });
        if let Some(filter) = &self.unspecified_filter {
            value["coverage"]["unspecifiedFilter"] = filter.as_json();
        }
        format!(
            "{}\n",
            serde_json::to_string_pretty(&value).expect("finite GLB coverage report")
        )
    }
}

/// Emits only the physical board shell. Missing component geometry is refused
/// unless the caller explicitly allows a partial export.
pub(crate) fn export_board_shell(
    board: &PhysicalBoardDocument,
    allow_partial: bool,
) -> Result<GlbExport, EngineError> {
    if !board.outline.is_valid() {
        return Err(EngineError::new(
            "GLB shell export requires a valid, closed physical board outline",
        ));
    }
    let thickness = board
        .thickness_mm()
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= MAX_THICKNESS_MM)
        .ok_or_else(|| EngineError::new(
            "GLB shell export requires a finite positive board thickness no greater than 1000 mm",
        ))?;
    let component_count = board.engineering().components.len();
    let model_references = board.engineering().models.len();
    let (unresolved_components, unresolved_components_truncated) =
        unresolved_component_coverage_for(
            board,
            None,
            "component_geometry_not_emitted_by_board_shell_only_export",
        );
    let unresolved_models = unresolved_models_for_shell(board);
    let partial = true;
    if !allow_partial {
        return Err(EngineError::new(format!(
            "board-shell-only GLB is always a partial digital-twin representation and omits {component_count} component model(s) and {model_references} model reference(s); pass allow_partial=true to permit this explicit partial export"
        )));
    }

    let rings = polygonize_outline(board)?;
    let triangulation = triangulate(&rings, board)?;
    let mesh = extrude_shell(
        &rings,
        &triangulation.points,
        &triangulation.triangles,
        thickness,
        board,
    )?;
    let bounds = board
        .outline
        .bounds
        .ok_or_else(|| EngineError::new("validated outline has no bounds"))?;
    let origin = (
        (bounds.min_x + bounds.max_x) / 2.0,
        (bounds.min_y + bounds.max_y) / 2.0,
    );
    let report = GlbCoverageReport {
        scope: "board_shell_only",
        shell_geometry_complete: true,
        whole_board_digital_twin_complete: false,
        partial_export: partial,
        component_count,
        selected_component_count: component_count,
        source_component_count: component_count,
        filtered_component_count: 0,
        component_filter: None,
        no_dnp: false,
        excluded_dnp_component_count: 0,
        unspecified_filter: None,
        component_models_emitted: 0,
        component_models_unresolved: component_count,
        model_references,
        selected_model_references: model_references,
        source_model_references: model_references,
        filtered_model_references: 0,
        model_references_emitted: 0,
        model_references_unresolved: model_references,
        unresolved_components,
        unresolved_components_total: component_count,
        unresolved_components_truncated,
        unresolved_models,
        unresolved_models_total: model_references,
        unresolved_models_truncated: model_references > MAX_COVERAGE_MODELS,
        polygonized_vertices: triangulation.points.len(),
        triangles: mesh.positions.len() / 9,
        exact_outline_area_mm2: triangulation.exact_area_mm2,
        polygonized_outline_area_mm2: triangulation.polygonized_area_mm2,
        triangulated_outline_area_mm2: triangulation.triangle_area_mm2,
        thickness_mm: thickness,
        max_curve_chord_error_mm: MAX_CHORD_ERROR_MM,
        outline_chaining_tolerance_mm: board.outline.join_tolerance_mm,
    };
    let document = gltf_document(&mesh, &report, board, origin)?;
    let json_bytes = serde_json::to_vec(&document)
        .map_err(|error| EngineError::new(format!("cannot serialize GLB JSON: {error}")))?;
    let total_len = container_len(json_bytes.len(), mesh.bin.len())?;
    if total_len > MAX_GLB_BYTES {
        return Err(EngineError::new(format!(
            "GLB exceeds the {MAX_GLB_BYTES}-byte export budget"
        )));
    }
    let glb = Glb {
        header: Header {
            magic: *b"glTF",
            version: 2,
            length: total_len as u32,
        },
        json: Cow::Owned(json_bytes),
        bin: Some(Cow::Owned(mesh.bin)),
    };
    let bytes = glb
        .to_vec()
        .map_err(|error| EngineError::new(format!("cannot encode GLB container: {error}")))?;
    if bytes.len() != total_len {
        return Err(EngineError::new(
            "GLB encoder returned an unexpected container length",
        ));
    }
    gltf::Gltf::from_slice(&bytes).map_err(|error| {
        EngineError::new(format!("generated GLB failed glTF validation: {error}"))
    })?;
    Ok(GlbExport { bytes, report })
}

/// Emit a board shell together with the supported, resolved VRML component
/// meshes. Every output component is a separate glTF node carrying its
/// canonical source identity, so a consumer can select it without parsing a
/// display name. Unhandled source models remain in the coverage report.
pub(crate) fn export_board_assembly(
    board: &PhysicalBoardDocument,
    resolver: &ModelResolver,
    allow_partial: bool,
    filter: &ModelAssemblyFilter,
) -> Result<GlbModelExport, EngineError> {
    if !board.outline.is_valid() {
        return Err(EngineError::new(
            "GLB assembly export requires a valid, closed physical board outline",
        ));
    }
    let thickness = board
        .thickness_mm()
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= MAX_THICKNESS_MM)
        .ok_or_else(|| EngineError::new(
            "GLB assembly export requires a finite positive board thickness no greater than 1000 mm",
        ))?;
    let rings = polygonize_outline(board)?;
    let triangulation = triangulate(&rings, board)?;
    let shell = extrude_shell(
        &rings,
        &triangulation.points,
        &triangulation.triangles,
        thickness,
        board,
    )?;
    // `selection` validates source DNP markers against canonical UUIDs and
    // returns those source ordinals as the authoritative exclusion set.
    let selection = filter.selection(board)?;
    let selected_ordinals = &selection.selected_component_ordinals;
    let assembly =
        assemble_supported_vrml_models_for_components(board, resolver, selected_ordinals);
    let (component_meshes, emitted_components) = component_meshes(&assembly)?;
    let incomplete =
        incomplete_components(board, &assembly, &emitted_components, selected_ordinals);
    let (unresolved_components, unresolved_components_truncated) =
        unresolved_component_coverage_for(
            board,
            Some(&incomplete),
            "component_model_geometry_not_emitted_by_assembly_export",
        );
    let unresolved_models = unresolved_models_for_assembly(board, &assembly);
    let source_model_references = board.engineering().models.len();
    let filtered_model_references = board
        .engineering()
        .models
        .iter()
        .filter(|model| {
            model.source.component_ordinal.is_some_and(|ordinal| {
                board
                    .engineering()
                    .components
                    .iter()
                    .find(|component| component.source.ordinal == ordinal)
                    .is_some_and(|_| !selected_ordinals.contains(&ordinal))
            })
        })
        .count();
    // Unowned/invalid-owner model records stay in selected-scope coverage as
    // unresolved. They are never treated as intentionally filtered away.
    let selected_model_references = source_model_references - filtered_model_references;
    let model_references_emitted = assembly.emitted.len();
    let source_component_count = board.engineering().components.len();
    let component_count = source_component_count;
    let selected_component_count = selected_ordinals.len();
    let filtered_component_count = source_component_count.saturating_sub(selected_component_count);
    let model_references_unresolved =
        selected_model_references.saturating_sub(model_references_emitted);
    let component_models_emitted = emitted_components.len();
    let component_models_unresolved = incomplete.len();
    let partial = component_models_unresolved != 0 || model_references_unresolved != 0;
    if partial && !allow_partial {
        return Err(EngineError::new(format!(
            "assembled GLB has {component_models_unresolved} component(s) and {model_references_unresolved} model reference(s) without supported emitted geometry; pass allow_partial=true to permit this explicit partial export"
        )));
    }
    let bounds = board
        .outline
        .bounds
        .ok_or_else(|| EngineError::new("validated outline has no bounds"))?;
    let origin = (
        (bounds.min_x + bounds.max_x) / 2.0,
        (bounds.min_y + bounds.max_y) / 2.0,
    );
    let report = GlbCoverageReport {
        scope: if filter.is_active() {
            "board_shell_and_selected_supported_vrml_models"
        } else {
            "board_shell_and_supported_vrml_models"
        },
        shell_geometry_complete: true,
        // A complete source-model coverage set does not certify artwork,
        // copper, manufacturing, physical fit, or optical registration.
        whole_board_digital_twin_complete: false,
        partial_export: partial,
        component_count,
        selected_component_count,
        source_component_count,
        filtered_component_count,
        component_filter: filter.requested_patterns(),
        no_dnp: filter.excludes_dnp(),
        excluded_dnp_component_count: selection.excluded_dnp_component_count,
        unspecified_filter: filter
            .excludes_unspecified()
            .then(|| FootprintTypeFilterCoverage {
                components_excluded: selection.excluded_unspecified_component_ordinals.len(),
                model_references_excluded: board
                    .engineering()
                    .models
                    .iter()
                    .filter(|model| {
                        model.source.component_ordinal.is_some_and(|ordinal| {
                            selection
                                .excluded_unspecified_component_ordinals
                                .contains(&ordinal)
                        })
                    })
                    .count(),
                overlap_with_dnp_components: selection
                    .excluded_unspecified_component_ordinals
                    .intersection(&selection.excluded_dnp_component_ordinals)
                    .count(),
            }),
        component_models_emitted,
        component_models_unresolved,
        model_references: source_model_references,
        selected_model_references,
        source_model_references,
        filtered_model_references,
        model_references_emitted,
        model_references_unresolved,
        unresolved_components,
        unresolved_components_total: component_models_unresolved,
        unresolved_components_truncated,
        unresolved_models,
        unresolved_models_total: model_references_unresolved,
        unresolved_models_truncated: assembly.diagnostics_truncated
            || model_references_unresolved > MAX_COVERAGE_MODELS,
        polygonized_vertices: triangulation.points.len(),
        triangles: shell.positions.len() / 9,
        exact_outline_area_mm2: triangulation.exact_area_mm2,
        polygonized_outline_area_mm2: triangulation.polygonized_area_mm2,
        triangulated_outline_area_mm2: triangulation.triangle_area_mm2,
        thickness_mm: thickness,
        max_curve_chord_error_mm: MAX_CHORD_ERROR_MM,
        outline_chaining_tolerance_mm: board.outline.join_tolerance_mm,
    };
    let dnp_filter = DnpFilterCoverage {
        applied: filter.excludes_dnp(),
        components_excluded: selection.excluded_dnp_component_count,
        model_references_excluded: board
            .engineering()
            .models
            .iter()
            .filter(|model| {
                model.source.component_ordinal.is_some_and(|ordinal| {
                    selection.excluded_dnp_component_ordinals.contains(&ordinal)
                })
            })
            .count(),
    };
    let document = gltf_assembly_document(
        &shell,
        &component_meshes,
        &report,
        &dnp_filter,
        board,
        origin,
    )?;
    let json_bytes = serde_json::to_vec(&document)
        .map_err(|error| EngineError::new(format!("cannot serialize GLB JSON: {error}")))?;
    let bin = combined_bin(&shell, &component_meshes)?;
    let total_len = container_len(json_bytes.len(), bin.len())?;
    if total_len > MAX_GLB_BYTES {
        return Err(EngineError::new(format!(
            "GLB exceeds the {MAX_GLB_BYTES}-byte export budget"
        )));
    }
    let glb = Glb {
        header: Header {
            magic: *b"glTF",
            version: 2,
            length: total_len as u32,
        },
        json: Cow::Owned(json_bytes),
        bin: Some(Cow::Owned(bin)),
    };
    let bytes = glb
        .to_vec()
        .map_err(|error| EngineError::new(format!("cannot encode GLB container: {error}")))?;
    gltf::Gltf::from_slice(&bytes).map_err(|error| {
        EngineError::new(format!("generated GLB failed glTF validation: {error}"))
    })?;
    Ok(GlbModelExport {
        bytes,
        report,
        dnp_filter,
    })
}

pub(crate) fn export_assigned_board_assembly(
    board: &PhysicalBoardDocument,
    assignments: &crate::model_assignments::ValidatedModelAssignments,
    allow_partial: bool,
) -> Result<AssignedGlbExport, EngineError> {
    if !allow_partial {
        return Err(EngineError::new(
            "assigned-model export is unqualified and requires explicit allow_partial=true",
        ));
    }
    // Reuse the validated shell metrics, never silently repair invalid contours.
    let mut report = export_board_shell(board, true)?.report;
    let assembly = crate::assigned_assembly::assemble_assigned_models(board, assignments)?;
    let mut components = Vec::new();
    let mut emitted = std::collections::BTreeSet::new();
    for item in &assembly.emitted {
        for part in &item.mesh.parts {
            components.push(ComponentMesh {
                mesh: mesh_from_component_part(part)?,
                material: part.material.clone(),
                solid: part.solid,
                reference: item.component_reference.clone(),
                uuid: item.component_uuid.clone(),
                source_ordinal: item.component_source.ordinal,
                source_asset_sha256: item.source_asset_sha256.clone(),
                model_source_ordinal: item.manifest_entry_index,
                assignment_origin: Some((
                    assignments.manifest_sha256().to_owned(),
                    item.manifest_entry_index,
                )),
                part_ordinal: part.source_shape_ordinal,
            });
        }
        emitted.insert(item.component_source.ordinal);
    }
    components.sort_by_key(|item| {
        (
            item.source_ordinal,
            item.model_source_ordinal,
            item.part_ordinal,
        )
    });
    let incomplete = board
        .engineering()
        .components
        .iter()
        .filter(|c| !emitted.contains(&c.source.ordinal))
        .map(|c| c.source.ordinal)
        .collect();
    let (unresolved, truncated) = unresolved_component_coverage_for(
        board,
        Some(&incomplete),
        "component_body_not_emitted_by_assignment_export",
    );
    report.scope = "board_shell_and_explicitly_assigned_wrl_models";
    report.component_models_emitted = emitted.len();
    report.component_models_unresolved = report.selected_component_count - emitted.len();
    report.unresolved_components = unresolved;
    report.unresolved_components_total = report.component_models_unresolved;
    report.unresolved_components_truncated = truncated;
    // Authored source references were deliberately not attempted in this mode.
    for item in &mut report.unresolved_models {
        item.status = "not_attempted_assignment_only_mode".into();
    }
    let rings = polygonize_outline(board)?;
    let triangles = triangulate(&rings, board)?;
    let shell = extrude_shell(
        &rings,
        &triangles.points,
        &triangles.triangles,
        report.thickness_mm,
        board,
    )?;
    let bounds = board
        .outline
        .bounds
        .ok_or_else(|| EngineError::new("missing outline bounds"))?;
    let origin = (
        (bounds.min_x + bounds.max_x) / 2.0,
        (bounds.min_y + bounds.max_y) / 2.0,
    );
    let mut document = gltf_assembly_document(
        &shell,
        &components,
        &report,
        &DnpFilterCoverage::default(),
        board,
        origin,
    )?;
    document["asset"]["extras"]["tokito"]["format"] =
        json!("tokito.assigned-board-assembly.glb.v1");
    document["asset"]["extras"]["tokito"]["coverage"]["componentModelCoverageComplete"] =
        json!(false);
    document["asset"]["extras"]["tokito"]["assignments"] = assembly.report.clone();
    let json_bytes = serde_json::to_vec(&document)
        .map_err(|error| EngineError::new(format!("cannot encode assignment GLB JSON: {error}")))?;
    let bin = combined_bin(&shell, &components)?;
    let total_len = container_len(json_bytes.len(), bin.len())?;
    if total_len > MAX_GLB_BYTES {
        return Err(EngineError::new("assigned GLB exceeds export budget"));
    }
    let bytes = Glb {
        header: Header {
            magic: *b"glTF",
            version: 2,
            length: total_len as u32,
        },
        json: Cow::Owned(json_bytes),
        bin: Some(Cow::Owned(bin)),
    }
    .to_vec()
    .map_err(|error| EngineError::new(format!("cannot encode assigned GLB: {error}")))?;
    gltf::Gltf::from_slice(&bytes)
        .map_err(|error| EngineError::new(format!("assigned GLB failed validation: {error}")))?;
    let mut report_json: Value = serde_json::from_str(&report.report_json())
        .map_err(|_| EngineError::new("cannot encode assignment receipt"))?;
    report_json["assignments"] = assembly.report;
    report_json["geometryVerified"] = json!(false);
    report_json["placementVerified"] = json!(false);
    report_json["authoredReferencesAttempted"] = json!(false);
    Ok(AssignedGlbExport {
        bytes,
        report: report_json,
    })
}

#[derive(Clone, Debug)]
struct ComponentMesh {
    mesh: Mesh,
    material: Option<crate::model_mesh::MeshMaterial>,
    solid: bool,
    reference: Option<String>,
    uuid: Option<String>,
    source_ordinal: usize,
    source_asset_sha256: String,
    model_source_ordinal: usize,
    assignment_origin: Option<(String, usize)>,
    part_ordinal: usize,
}

fn component_meshes(
    assembly: &ModelAssembly,
) -> Result<(Vec<ComponentMesh>, std::collections::BTreeSet<usize>), EngineError> {
    let mut result = Vec::new();
    let mut components = std::collections::BTreeSet::new();
    for emitted in &assembly.emitted {
        for part in &emitted.mesh.parts {
            result.push(ComponentMesh {
                mesh: mesh_from_component_part(part)?,
                material: part.material.clone(),
                solid: part.solid,
                reference: emitted.component_reference.clone(),
                uuid: emitted.component_uuid.clone(),
                source_ordinal: emitted.component_source.ordinal,
                source_asset_sha256: emitted.source_asset_sha256.clone(),
                model_source_ordinal: emitted.model_source.ordinal,
                assignment_origin: None,
                part_ordinal: part.source_shape_ordinal,
            });
        }
        components.insert(emitted.component_source.ordinal);
    }
    // The GLB accessors and binary payload must use exactly the same order.
    // Serialization groups by component ordinal; normalize the flat payload
    // list here so combined_bin follows that identical order.
    result.sort_by_key(|item| {
        (
            item.source_ordinal,
            item.model_source_ordinal,
            item.part_ordinal,
        )
    });
    Ok((result, components))
}

fn mesh_from_component_part(
    part: &crate::component_pose::PlacedModelMeshPart,
) -> Result<Mesh, EngineError> {
    if part.corner_normals.len() != part.triangles.len() {
        return Err(EngineError::new(
            "placed component mesh has inconsistent corner normals",
        ));
    }
    let mut positions = Vec::with_capacity(part.triangles.len() * 9);
    let mut normals = Vec::with_capacity(part.triangles.len() * 9);
    for (triangle_index, triangle) in part.triangles.iter().enumerate() {
        let face_normal = part
            .flat_normals
            .get(triangle_index)
            .copied()
            .ok_or_else(|| EngineError::new("placed component mesh has inconsistent normals"))?;
        let point = |index: u32| {
            part.positions_m
                .get(index as usize)
                .copied()
                .ok_or_else(|| EngineError::new("placed component triangle index is out of range"))
        };
        push_face_with_corner_normals(
            &mut positions,
            &mut normals,
            [
                point(triangle[0])?,
                point(triangle[1])?,
                point(triangle[2])?,
            ],
            part.corner_normals[triangle_index],
            face_normal,
        )?;
    }
    mesh_from_vectors(positions, normals)
}

fn push_face_with_corner_normals(
    positions: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    points: [[f64; 3]; 3],
    mut corner_normals: [[f64; 3]; 3],
    expected_face_normal: [f64; 3],
) -> Result<(), EngineError> {
    if positions.len() / 3 + 3 > MAX_GLTF_VERTICES {
        return Err(EngineError::new("mesh exceeds vertex budget"));
    }
    let cross = cross3(sub3(points[1], points[0]), sub3(points[2], points[0]));
    let face_dot = cross
        .iter()
        .zip(expected_face_normal)
        .map(|(left, right)| left * right)
        .sum::<f64>();
    if !face_dot.is_finite() || face_dot <= 0.0 {
        return Err(EngineError::new(
            "mesh contains a degenerate or misoriented face",
        ));
    }
    let mut quantized = [[0.0f64; 3]; 3];
    for (corner, point) in points.iter().enumerate() {
        for axis in 0..3 {
            let value = point[axis] as f32;
            if !value.is_finite() || (value as f64 - point[axis]).abs() > MAX_FLOAT_POSITION_ERROR_M
            {
                return Err(EngineError::new(
                    "position cannot meet the GLB f32 quantization error budget",
                ));
            }
            quantized[corner][axis] = value as f64;
        }
    }
    let emitted_cross = cross3(
        sub3(quantized[1], quantized[0]),
        sub3(quantized[2], quantized[0]),
    );
    let emitted_dot = emitted_cross
        .iter()
        .zip(expected_face_normal)
        .map(|(left, right)| left * right)
        .sum::<f64>();
    if !emitted_dot.is_finite() || emitted_dot <= 0.0 {
        return Err(EngineError::new(
            "f32 position quantization collapsed or inverted a mesh face",
        ));
    }
    for normal in &mut corner_normals {
        let length = normal[0].hypot(normal[1]).hypot(normal[2]);
        if !length.is_finite() || length <= 0.0 {
            return Err(EngineError::new("mesh contains an invalid corner normal"));
        }
        *normal = normal.map(|value| value / length);
        if normal
            .iter()
            .any(|value| !value.is_finite() || (*value as f32).is_infinite())
        {
            return Err(EngineError::new(
                "corner normal cannot be represented as f32",
            ));
        }
    }
    for corner_position in &quantized {
        positions.extend(corner_position.iter().map(|value| *value as f32));
    }
    for corner_normal in corner_normals {
        normals.extend(corner_normal.map(|value| value as f32));
    }
    Ok(())
}

fn mesh_from_vectors(positions: Vec<f32>, normals: Vec<f32>) -> Result<Mesh, EngineError> {
    if positions.is_empty()
        || positions.len() != normals.len()
        || !positions.len().is_multiple_of(9)
    {
        return Err(EngineError::new(
            "component mesh has invalid flattened geometry",
        ));
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for point in positions.chunks_exact(3) {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    if !min.iter().chain(max.iter()).all(|value| value.is_finite()) {
        return Err(EngineError::new("component mesh bounds are non-finite"));
    }
    let mut bin = Vec::with_capacity(positions.len() * 8);
    for value in positions.iter().chain(&normals) {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    Ok(Mesh {
        positions,
        normals,
        bin,
        min,
        max,
    })
}

fn incomplete_components(
    board: &PhysicalBoardDocument,
    assembly: &ModelAssembly,
    emitted: &std::collections::BTreeSet<usize>,
    selected_ordinals: &std::collections::BTreeSet<usize>,
) -> std::collections::BTreeSet<usize> {
    board
        .engineering()
        .components
        .iter()
        .filter(|component| {
            selected_ordinals.contains(&component.source.ordinal)
                && (!emitted.contains(&component.source.ordinal)
                    || assembly
                        .incomplete_component_ordinals
                        .contains(&component.source.ordinal))
        })
        .map(|component| component.source.ordinal)
        .collect()
}

fn unresolved_component_coverage_for(
    board: &PhysicalBoardDocument,
    only_ordinals: Option<&std::collections::BTreeSet<usize>>,
    reason: &'static str,
) -> (Vec<UnresolvedComponentCoverage>, bool) {
    let engineering = board.engineering();
    let mut by_owner = BTreeMap::<usize, Vec<&super::document::ModelReference>>::new();
    for model in &engineering.models {
        if let Some(owner) = model.source.component_ordinal {
            by_owner.entry(owner).or_default().push(model);
        }
    }
    let mut items = Vec::with_capacity(engineering.components.len().min(MAX_COVERAGE_COMPONENTS));
    for component in engineering
        .components
        .iter()
        .filter(|component| {
            only_ordinals.is_none_or(|ordinals| ordinals.contains(&component.source.ordinal))
        })
        .take(MAX_COVERAGE_COMPONENTS)
    {
        let models = by_owner
            .get(&component.source.ordinal)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut model_paths = Vec::new();
        let mut model_references_without_path = 0usize;
        let mut model_paths_truncated = false;
        let mut model_paths_redacted = false;
        for model in models {
            let Some(path) = model.path.as_deref() else {
                model_references_without_path += 1;
                continue;
            };
            if model_paths.len() >= MAX_MODEL_PATHS_PER_COMPONENT {
                model_paths_truncated = true;
                continue;
            }
            let (display_path, redacted) = report_model_path(path);
            model_paths_redacted |= redacted;
            let chars: Vec<char> = display_path
                .chars()
                .take(MAX_MODEL_PATH_CHARS + 1)
                .collect();
            if chars.len() > MAX_MODEL_PATH_CHARS {
                model_paths_truncated = true;
            }
            model_paths.push(chars.into_iter().take(MAX_MODEL_PATH_CHARS).collect());
        }
        let model_reference_count = models.len();
        items.push(UnresolvedComponentCoverage {
            reference: component.reference.clone(),
            uuid: component.source.source_uuid.as_deref().map(str::to_owned),
            source_kind: component.source.kind.clone(),
            source_ordinal: component.source.ordinal,
            source_identity: format!("{}#{}", component.source.kind, component.source.ordinal),
            model_paths,
            model_reference_count,
            model_references_without_path,
            model_paths_truncated,
            model_paths_redacted,
            reason,
            no_model_reference_reason: (model_reference_count == 0)
                .then_some("no_source_model_reference_in_imported_data"),
        });
    }
    (
        items,
        only_ordinals.map_or(engineering.components.len(), |ordinals| ordinals.len())
            > MAX_COVERAGE_COMPONENTS,
    )
}

fn unresolved_models_for_shell(board: &PhysicalBoardDocument) -> Vec<UnresolvedModelCoverage> {
    board
        .engineering()
        .models
        .iter()
        .take(MAX_COVERAGE_MODELS)
        .map(|model| {
            unresolved_model_coverage(board, &model.source, None, "not_attempted_board_shell_only")
        })
        .collect()
}

fn unresolved_models_for_assembly(
    board: &PhysicalBoardDocument,
    assembly: &ModelAssembly,
) -> Vec<UnresolvedModelCoverage> {
    assembly
        .diagnostics
        .iter()
        .take(MAX_COVERAGE_MODELS)
        .map(|diagnostic| {
            unresolved_model_coverage(
                board,
                &diagnostic.model_source,
                diagnostic.component_source.as_ref(),
                diagnostic.status.as_str(),
            )
        })
        .collect()
}

fn unresolved_model_coverage(
    board: &PhysicalBoardDocument,
    source: &super::document::SourceLocator,
    component_source: Option<&super::document::SourceLocator>,
    status: &str,
) -> UnresolvedModelCoverage {
    let component = component_source.and_then(|source| {
        board
            .engineering()
            .components
            .iter()
            .find(|component| component.source.ordinal == source.ordinal)
    });
    UnresolvedModelCoverage {
        component_reference: component.and_then(|item| item.reference.clone()),
        component_uuid: component
            .and_then(|item| item.source.source_uuid.as_deref().map(str::to_owned)),
        source_kind: source.kind.clone(),
        source_ordinal: source.ordinal,
        source_identity: format!("{}#{}", source.kind, source.ordinal),
        status: status.to_owned(),
    }
}

fn report_model_path(path: &str) -> (String, bool) {
    let path_object = std::path::Path::new(path);
    let looks_like_absolute_root = path.starts_with('/') || path.starts_with('\\');
    let looks_like_windows_absolute = path.starts_with("\\\\")
        || path.starts_with("//")
        || path.as_bytes().get(1) == Some(&b':')
            && path
                .as_bytes()
                .get(2)
                .is_some_and(|separator| *separator == b'\\' || *separator == b'/');
    if !path_object.is_absolute() && !looks_like_absolute_root && !looks_like_windows_absolute {
        return (path.to_owned(), false);
    }
    let basename = path
        .rsplit(['\\', '/'])
        .find(|part| !part.is_empty())
        .unwrap_or("<unknown>");
    (format!("<absolute path redacted; file: {basename}>"), true)
}

#[derive(Clone, Debug)]
struct Ring {
    role: ContourRole,
    points: Vec<Point2>,
    signed_area: f64,
}

#[derive(Clone, Debug)]
struct Triangulation {
    points: Vec<[f64; 2]>,
    triangles: Vec<u32>,
    exact_area_mm2: f64,
    polygonized_area_mm2: f64,
    triangle_area_mm2: f64,
}

fn polygonize_outline(board: &PhysicalBoardDocument) -> Result<Vec<Ring>, EngineError> {
    let contours = &board.outline.contours;
    let outers: Vec<_> = contours
        .iter()
        .filter(|c| c.role == ContourRole::Outer)
        .collect();
    if outers.len() != 1 || contours.iter().any(|c| c.edges.is_empty()) {
        return Err(EngineError::new(
            "outline must have one outer contour and non-empty cutouts",
        ));
    }
    let mut ordered = vec![outers[0]];
    ordered.extend(contours.iter().filter(|c| c.role == ContourRole::Cutout));
    let mut rings = Vec::with_capacity(ordered.len());
    let mut total_points = 0usize;
    for contour in ordered {
        let mut points = Vec::new();
        for (edge_index, edge) in contour.edges.iter().enumerate() {
            let shape = &board
                .outline
                .primitives
                .get(edge.primitive_index)
                .ok_or_else(|| EngineError::new("contour references a missing primitive"))?
                .shape;
            match shape {
                OutlineShape::Line { start, end } => {
                    let (start, _) = orient(*start, *end, edge.reversed);
                    points.push(canonical_join(contour, edge_index, board, start)?);
                }
                OutlineShape::Arc { start, mid, end } => {
                    let canonical = canonical_join(
                        contour,
                        edge_index,
                        board,
                        orient(*start, *end, edge.reversed).0,
                    )?;
                    let (center, radius, a0, am, a1) =
                        super::topology::arc_parameters(*start, *mid, *end).ok_or_else(|| {
                            EngineError::new("validated arc has no circle parameters")
                        })?;
                    let original = sweep_through_mid(a0, am, a1)?;
                    let (angle0, sweep) = if edge.reversed {
                        (a0 + original, -original)
                    } else {
                        (a0, original)
                    };
                    let count = curve_segments(radius, sweep, board.outline.join_tolerance_mm)?;
                    for index in 0..count {
                        if index == 0 {
                            points.push(canonical);
                        } else {
                            let angle = angle0 + sweep * index as f64 / count as f64;
                            points.push(Point2 {
                                x: center.x + radius * angle.cos(),
                                y: center.y + radius * angle.sin(),
                            });
                        }
                    }
                }
                OutlineShape::Circle { center, radius_mm } => {
                    let count = curve_segments(*radius_mm, TAU, board.outline.join_tolerance_mm)?;
                    for index in 0..count {
                        let angle = TAU * index as f64 / count as f64;
                        points.push(Point2 {
                            x: center.x + *radius_mm * angle.cos(),
                            y: center.y + *radius_mm * angle.sin(),
                        });
                    }
                }
            }
            if points.len() > MAX_POLYGON_POINTS {
                return Err(EngineError::new(
                    "outline exceeds the per-contour polygonization budget",
                ));
            }
        }
        if points.len() < 3 || points.iter().any(|p| !finite_point(*p)) {
            return Err(EngineError::new(
                "contour did not produce three finite vertices",
            ));
        }
        total_points = total_points
            .checked_add(points.len())
            .ok_or_else(|| EngineError::new("outline point count overflow"))?;
        if total_points > MAX_POLYGON_POINTS {
            return Err(EngineError::new(format!(
                "outline exceeds the {MAX_POLYGON_POINTS}-point budget"
            )));
        }
        let signed_area = polygon_area(&points)?;
        if signed_area.abs() <= f64::EPSILON
            || signed_area.signum() != contour.signed_area_mm2.signum()
        {
            return Err(EngineError::new(
                "polygonization changed contour orientation or produced zero area",
            ));
        }
        let error_bound = contour_area_error_bound(contour, board)?;
        let tolerance = error_bound
            + AREA_ABS_TOLERANCE_MM2
            + contour.signed_area_mm2.abs() * AREA_REL_TOLERANCE;
        if (signed_area - contour.signed_area_mm2).abs() > tolerance {
            return Err(EngineError::new(
                "polygonized contour exceeds its declared chord/join area bound",
            ));
        }
        rings.push(Ring {
            role: contour.role,
            points,
            signed_area,
        });
    }
    Ok(rings)
}

fn orient(start: Point2, end: Point2, reversed: bool) -> (Point2, Point2) {
    if reversed { (end, start) } else { (start, end) }
}

fn canonical_join(
    contour: &OutlineContour,
    index: usize,
    board: &PhysicalBoardDocument,
    start: Point2,
) -> Result<Point2, EngineError> {
    let previous = &contour.edges[(index + contour.edges.len() - 1) % contour.edges.len()];
    let current = &contour.edges[index];
    let previous_shape = &board
        .outline
        .primitives
        .get(previous.primitive_index)
        .ok_or_else(|| EngineError::new("contour references a missing previous primitive"))?
        .shape;
    let current_shape = &board
        .outline
        .primitives
        .get(current.primitive_index)
        .ok_or_else(|| EngineError::new("contour references a missing current primitive"))?
        .shape;
    let prev_end = match previous_shape {
        OutlineShape::Line { start, end } | OutlineShape::Arc { start, end, .. } => {
            orient(*start, *end, previous.reversed).1
        }
        OutlineShape::Circle { .. } => {
            return Err(EngineError::new(
                "a full-circle contour cannot be joined to another edge",
            ));
        }
    };
    let current_start = match current_shape {
        OutlineShape::Line { start, end } | OutlineShape::Arc { start, end, .. } => {
            orient(*start, *end, current.reversed).0
        }
        OutlineShape::Circle { .. } => {
            return Err(EngineError::new(
                "a full-circle contour cannot be joined to another edge",
            ));
        }
    };
    if distance(prev_end, current_start) > board.outline.join_tolerance_mm
        || distance(start, current_start) > 1e-12
    {
        return Err(EngineError::new(
            "contour join is inconsistent with validated endpoints",
        ));
    }
    Ok(Point2 {
        x: (prev_end.x + current_start.x) * 0.5,
        y: (prev_end.y + current_start.y) * 0.5,
    })
}

fn sweep_through_mid(start: f64, mid: f64, end: f64) -> Result<f64, EngineError> {
    if ![start, mid, end].iter().all(|v| v.is_finite()) {
        return Err(EngineError::new("arc angular parameter is non-finite"));
    }
    let to_end = (end - start).rem_euclid(TAU);
    let to_mid = (mid - start).rem_euclid(TAU);
    let sweep = if to_mid < to_end {
        to_end
    } else {
        to_end - TAU
    };
    if !sweep.is_finite() || sweep.abs() <= 1e-12 || sweep.abs() >= TAU {
        return Err(EngineError::new("arc sweep is degenerate"));
    }
    Ok(sweep)
}

fn curve_segments(radius: f64, sweep: f64, join_tolerance_mm: f64) -> Result<usize, EngineError> {
    // Reserve midpoint displacement from the combined chord/join error budget.
    let join_error = if join_tolerance_mm == super::topology::JOIN_TOLERANCE_MM {
        join_tolerance_mm
    } else {
        join_tolerance_mm / 2.
    };
    let error = MAX_CHORD_ERROR_MM - join_error;
    if !error.is_finite() || error <= 0. {
        return Err(EngineError::new(
            "join tolerance exhausts curve tessellation budget",
        ));
    }
    if !radius.is_finite() || radius <= 0.0 || !sweep.is_finite() || sweep.abs() <= 0.0 {
        return Err(EngineError::new("curve has invalid radius or sweep"));
    }
    let max_angle = if error >= radius {
        PI
    } else {
        4.0 * (error / (2.0 * radius)).sqrt().asin()
    };
    let raw = (sweep.abs() / max_angle).ceil();
    if !raw.is_finite() || raw > MAX_POLYGON_POINTS as f64 {
        return Err(EngineError::new(
            "curve exceeds the polygonization sample budget",
        ));
    }
    let count = (raw as usize).max(if sweep.abs() >= TAU - 1e-12 { 3 } else { 1 });
    if count > MAX_POLYGON_POINTS {
        return Err(EngineError::new(
            "curve exceeds the polygonization sample budget",
        ));
    }
    Ok(count)
}

fn contour_area_error_bound(
    contour: &OutlineContour,
    board: &PhysicalBoardDocument,
) -> Result<f64, EngineError> {
    let mut error = 0.0;
    let mut perimeter = 0.0;
    for edge in &contour.edges {
        let shape = &board
            .outline
            .primitives
            .get(edge.primitive_index)
            .ok_or_else(|| EngineError::new("contour references a missing primitive"))?
            .shape;
        match shape {
            OutlineShape::Line { start, end } => perimeter += distance(*start, *end),
            OutlineShape::Arc { start, mid, end } => {
                let (_, radius, a0, am, a1) = super::topology::arc_parameters(*start, *mid, *end)
                    .ok_or_else(|| {
                    EngineError::new("validated arc has no circle parameters")
                })?;
                let sweep = sweep_through_mid(a0, am, a1)?;
                let segments = curve_segments(radius, sweep, board.outline.join_tolerance_mm)?;
                error += curve_chord_area_loss(radius, sweep, segments);
                perimeter += radius * sweep.abs();
            }
            OutlineShape::Circle { radius_mm, .. } => {
                let segments = curve_segments(*radius_mm, TAU, board.outline.join_tolerance_mm)?;
                error += curve_chord_area_loss(*radius_mm, TAU, segments);
                perimeter += TAU * *radius_mm;
            }
        }
    }
    error += perimeter * board.outline.join_tolerance_mm;
    if error.is_finite() {
        Ok(error)
    } else {
        Err(EngineError::new("area error bound overflowed"))
    }
}

fn curve_chord_area_loss(radius: f64, sweep: f64, segments: usize) -> f64 {
    0.5 * radius * radius * segments as f64 * stable_sweep_minus_sin(sweep / segments as f64).abs()
}

fn stable_sweep_minus_sin(x: f64) -> f64 {
    if x.abs() > 0.05 {
        x - x.sin()
    } else {
        let x2 = x * x;
        x * x2 * (1.0 / 6.0 + x2 * (-1.0 / 120.0 + x2 * (1.0 / 5040.0 - x2 / 362_880.0)))
    }
}

fn polygon_area(points: &[Point2]) -> Result<f64, EngineError> {
    if points.len() < 3 {
        return Err(EngineError::new("polygon has fewer than three points"));
    }
    let origin = points[0];
    let twice = (0..points.len())
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            (a.x - origin.x) * (b.y - origin.y) - (b.x - origin.x) * (a.y - origin.y)
        })
        .sum::<f64>();
    let area = twice * 0.5;
    if area.is_finite() {
        Ok(area)
    } else {
        Err(EngineError::new("polygon area overflowed"))
    }
}

fn triangulate(
    rings: &[Ring],
    board: &PhysicalBoardDocument,
) -> Result<Triangulation, EngineError> {
    if rings.is_empty() || rings[0].role != ContourRole::Outer {
        return Err(EngineError::new(
            "triangulation requires an outer ring first",
        ));
    }
    let count = rings
        .iter()
        .try_fold(0usize, |n, ring| n.checked_add(ring.points.len()))
        .ok_or_else(|| EngineError::new("outline point count overflow"))?;
    if count > MAX_POLYGON_POINTS {
        return Err(EngineError::new(
            "outline exceeds triangulator point budget",
        ));
    }
    let mut points = Vec::with_capacity(count);
    let mut holes = Vec::<u32>::with_capacity(rings.len().saturating_sub(1));
    let mut polygonized_area = 0.0;
    let mut exact_area = 0.0;
    let mut exact_cutouts = board
        .outline
        .contours
        .iter()
        .filter(|c| c.role == ContourRole::Cutout);
    for (i, ring) in rings.iter().enumerate() {
        if ring.points.len() < 3 {
            return Err(EngineError::new("triangulation ring is too short"));
        }
        if i > 0 {
            if ring.role != ContourRole::Cutout {
                return Err(EngineError::new("unexpected non-cutout ring"));
            }
            holes.push(
                u32::try_from(points.len()).map_err(|_| EngineError::new("hole index overflow"))?,
            );
        }
        polygonized_area += if i == 0 {
            ring.signed_area.abs()
        } else {
            -ring.signed_area.abs()
        };
        let contour = if i == 0 {
            board
                .outline
                .contours
                .iter()
                .find(|c| c.role == ContourRole::Outer)
        } else {
            exact_cutouts.next()
        }
        .ok_or_else(|| EngineError::new("analytic contour area unavailable"))?;
        exact_area += if i == 0 {
            contour.signed_area_mm2.abs()
        } else {
            -contour.signed_area_mm2.abs()
        };
        points.extend(ring.points.iter().map(|p| [p.x, p.y]));
    }
    if !polygonized_area.is_finite()
        || !exact_area.is_finite()
        || polygonized_area <= 0.0
        || exact_area <= 0.0
    {
        return Err(EngineError::new(
            "outline has no positive material area after cutouts",
        ));
    }

    let mut indices = Vec::<u32>::new();
    Earcut::<f64>::new().earcut(points.iter().copied(), &holes, &mut indices);
    if indices.is_empty() || !indices.len().is_multiple_of(3) {
        return Err(EngineError::new(
            "Earcut returned an empty or malformed triangulation",
        ));
    }
    let mut triangle_area = 0.0;
    for tri in indices.chunks_exact(3) {
        let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if a >= count || b >= count || c >= count || a == b || b == c || c == a {
            return Err(EngineError::new("Earcut returned an invalid index"));
        }
        let signed_twice = cross2(points[a], points[b], points[c]);
        if !signed_twice.is_finite() || signed_twice == 0.0 {
            return Err(EngineError::new("Earcut returned a degenerate triangle"));
        }
        triangle_area += signed_twice.abs() * 0.5;
        let center = Point2 {
            x: (points[a][0] + points[b][0] + points[c][0]) / 3.0,
            y: (points[a][1] + points[b][1] + points[c][1]) / 3.0,
        };
        if !inside_material(center, rings) {
            return Err(EngineError::new(
                "triangulator returned a face outside board material",
            ));
        }
    }
    let polygon_tol = AREA_ABS_TOLERANCE_MM2 + polygonized_area.abs() * AREA_REL_TOLERANCE;
    if !triangle_area.is_finite() || (triangle_area - polygonized_area).abs() > polygon_tol {
        return Err(EngineError::new(
            "Earcut triangle area does not match polygonized material area",
        ));
    }
    let bound = board
        .outline
        .contours
        .iter()
        .try_fold(0.0, |sum, c| {
            let next = contour_area_error_bound(c, board).ok()?;
            let total = sum + next;
            total.is_finite().then_some(total)
        })
        .ok_or_else(|| EngineError::new("analytic outline area bound overflowed"))?;
    let exact_tol = bound + AREA_ABS_TOLERANCE_MM2 + exact_area.abs() * AREA_REL_TOLERANCE;
    if (triangle_area - exact_area).abs() > exact_tol {
        return Err(EngineError::new(
            "triangulated area exceeds analytic outline/chord error bound",
        ));
    }
    Ok(Triangulation {
        points,
        triangles: indices,
        exact_area_mm2: exact_area,
        polygonized_area_mm2: polygonized_area,
        triangle_area_mm2: triangle_area,
    })
}

fn inside_material(point: Point2, rings: &[Ring]) -> bool {
    inside_ring(point, &rings[0].points)
        && !rings[1..].iter().any(|r| inside_ring(point, &r.points))
}
fn inside_ring(p: Point2, points: &[Point2]) -> bool {
    let mut inside = false;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        if (a.y > p.y) != (b.y > p.y) {
            let x = (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x;
            if p.x < x {
                inside = !inside;
            }
        }
    }
    inside
}
fn cross2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

#[derive(Clone, Debug)]
struct Mesh {
    positions: Vec<f32>,
    normals: Vec<f32>,
    bin: Vec<u8>,
    min: [f32; 3],
    max: [f32; 3],
}

fn extrude_shell(
    rings: &[Ring],
    points: &[[f64; 2]],
    triangles: &[u32],
    thickness: f64,
    board: &PhysicalBoardDocument,
) -> Result<Mesh, EngineError> {
    let bounds = board
        .outline
        .bounds
        .ok_or_else(|| EngineError::new("outline bounds unavailable"))?;
    let (origin_x, origin_y) = (
        (bounds.min_x + bounds.max_x) * 0.5,
        (bounds.min_y + bounds.max_y) * 0.5,
    );
    let half_y = thickness * MM_TO_M * 0.5;
    let face_count = triangles
        .len()
        .checked_mul(2)
        .map(|n| n / 3)
        .and_then(|n| {
            rings.iter().try_fold(n, |sum, ring| {
                sum.checked_add(ring.points.len().checked_mul(2)?)
            })
        })
        .ok_or_else(|| EngineError::new("mesh face count overflow"))?;
    let vertex_capacity = face_count
        .checked_mul(9)
        .ok_or_else(|| EngineError::new("mesh vertex count overflow"))?;
    if vertex_capacity > MAX_GLTF_VERTICES * 3 {
        return Err(EngineError::new("mesh exceeds vertex budget"));
    }
    let mut positions = Vec::with_capacity(vertex_capacity);
    let mut normals = Vec::with_capacity(vertex_capacity);
    let world = |p: [f64; 2], y: f64| -> Result<[f64; 3], EngineError> {
        let q = [(p[0] - origin_x) * MM_TO_M, y, -(p[1] - origin_y) * MM_TO_M];
        if q.iter().all(|n| {
            n.is_finite()
                && (*n as f32).is_finite()
                && ((*n as f32) as f64 - *n).abs() <= MAX_FLOAT_POSITION_ERROR_M
        }) {
            Ok(q)
        } else {
            Err(EngineError::new(format!(
                "shell coordinate is non-finite or exceeds the {:.3} micrometre f32 position-error budget",
                MAX_FLOAT_POSITION_ERROR_M * 1_000_000.0
            )))
        }
    };
    for tri in triangles.chunks_exact(3) {
        let a = points
            .get(tri[0] as usize)
            .ok_or_else(|| EngineError::new("triangle index out of range"))?;
        let b = points
            .get(tri[1] as usize)
            .ok_or_else(|| EngineError::new("triangle index out of range"))?;
        let c = points
            .get(tri[2] as usize)
            .ok_or_else(|| EngineError::new("triangle index out of range"))?;
        push_face(
            &mut positions,
            &mut normals,
            [world(*a, half_y)?, world(*b, half_y)?, world(*c, half_y)?],
            [0.0, 1.0, 0.0],
        )?;
        push_face(
            &mut positions,
            &mut normals,
            [
                world(*a, -half_y)?,
                world(*b, -half_y)?,
                world(*c, -half_y)?,
            ],
            [0.0, -1.0, 0.0],
        )?;
    }
    for ring in rings {
        for i in 0..ring.points.len() {
            let a = ring.points[i];
            let b = ring.points[(i + 1) % ring.points.len()];
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let length = dx.hypot(dy);
            if !length.is_finite() || length == 0.0 {
                return Err(EngineError::new("degenerate wall segment"));
            }
            let left = [-dy / length, dx / length];
            let right = [dy / length, -dx / length];
            let outward = match (ring.role, ring.signed_area > 0.0) {
                (ContourRole::Outer, true) | (ContourRole::Cutout, false) => right,
                _ => left,
            };
            let normal = [outward[0], 0.0, -outward[1]];
            let a0 = world([a.x, a.y], -half_y)?;
            let b0 = world([b.x, b.y], -half_y)?;
            let b1 = world([b.x, b.y], half_y)?;
            let a1 = world([a.x, a.y], half_y)?;
            push_face(&mut positions, &mut normals, [a0, b0, b1], normal)?;
            push_face(&mut positions, &mut normals, [a0, b1, a1], normal)?;
        }
    }
    if positions.is_empty()
        || !positions.len().is_multiple_of(9)
        || positions.len() / 3 > MAX_GLTF_VERTICES
    {
        return Err(EngineError::new(
            "extruded mesh is empty or exceeds vertex budget",
        ));
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in positions.chunks_exact(3) {
        for axis in 0..3 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    if !min.iter().chain(max.iter()).all(|v| v.is_finite()) || positions.len() != normals.len() {
        return Err(EngineError::new("mesh positions/normals are invalid"));
    }
    let mut bin = Vec::with_capacity(
        positions
            .len()
            .checked_mul(8)
            .ok_or_else(|| EngineError::new("buffer length overflow"))?,
    );
    for value in positions.iter().chain(&normals) {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    Ok(Mesh {
        positions,
        normals,
        bin,
        min,
        max,
    })
}

/// Check the actual GLB conversion before assembly retains a component. A failed
/// model is withheld as a whole; no face is dropped or repaired silently.
pub(crate) fn validate_component_mesh_for_glb(
    mesh: &crate::component_pose::PlacedModelMesh,
) -> Result<(), EngineError> {
    for part in &mesh.parts {
        mesh_from_component_part(part)?;
    }
    Ok(())
}

fn push_face(
    positions: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    mut p: [[f64; 3]; 3],
    expected: [f64; 3],
) -> Result<(), EngineError> {
    if positions.len() / 3 + 3 > MAX_GLTF_VERTICES {
        return Err(EngineError::new("mesh exceeds vertex budget"));
    }
    let cross = cross3(sub3(p[1], p[0]), sub3(p[2], p[0]));
    let dot = cross.iter().zip(expected).map(|(a, b)| a * b).sum::<f64>();
    if !dot.is_finite() || dot == 0.0 {
        return Err(EngineError::new("mesh contains a degenerate face"));
    }
    if dot < 0.0 {
        p.swap(1, 2);
    }
    let norm = expected.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        return Err(EngineError::new("invalid mesh normal"));
    }
    let n = [
        (expected[0] / norm) as f32,
        (expected[1] / norm) as f32,
        (expected[2] / norm) as f32,
    ];
    if !n.iter().all(|v| v.is_finite()) {
        return Err(EngineError::new("normal cannot be represented as f32"));
    }
    let mut quantized = [[0.0f64; 3]; 3];
    for (vertex_index, vertex) in p.into_iter().enumerate() {
        for (axis, coordinate) in vertex.into_iter().enumerate() {
            let value = coordinate as f32;
            if !value.is_finite() || (value as f64 - coordinate).abs() > MAX_FLOAT_POSITION_ERROR_M
            {
                return Err(EngineError::new(
                    "position cannot meet the GLB f32 quantization error budget",
                ));
            }
            quantized[vertex_index][axis] = value as f64;
            positions.push(value);
        }
        normals.extend_from_slice(&n);
    }
    let emitted_cross = cross3(
        sub3(quantized[1], quantized[0]),
        sub3(quantized[2], quantized[0]),
    );
    let emitted_dot = emitted_cross
        .iter()
        .zip(expected)
        .map(|(a, b)| a * b)
        .sum::<f64>();
    if !emitted_dot.is_finite() || emitted_dot <= 0.0 {
        return Err(EngineError::new(
            "f32 position quantization collapsed or inverted a mesh face",
        ));
    }
    Ok(())
}
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn gltf_document(
    mesh: &Mesh,
    report: &GlbCoverageReport,
    board: &PhysicalBoardDocument,
    origin: (f64, f64),
) -> Result<Value, EngineError> {
    let count = mesh.positions.len() / 3;
    debug_assert_eq!(mesh.normals.len(), mesh.positions.len());
    let position_bytes = count
        .checked_mul(12)
        .ok_or_else(|| EngineError::new("position buffer length overflow"))?;
    let normal_bytes = count
        .checked_mul(12)
        .ok_or_else(|| EngineError::new("normal buffer length overflow"))?;
    let source = board.source();
    let document = json!({
        "asset": {
            "version": "2.0", "generator": "Tokito CLI",
            "extras": { "tokito": {
                "format": "tokito.board-shell.glb.v0",
                "scope": report.scope,
                "shellGeometryComplete": report.shell_geometry_complete,
                "wholeBoardDigitalTwinComplete": report.whole_board_digital_twin_complete,
                "partialExport": report.partial_export,
                "source": { "format": "kicad_pcb", "sha256": source.sha256, "formatVersion": source.format_version },
                "coordinateFrame": {
                    "units": "m", "handedness": "right-handed", "upAxis": "Y", "boardPlane": "XZ",
                    "sourceUnits": "mm", "sourceCoordinateFrame": "KiCad-board-Y-down",
                    "sourceOriginMm": [origin.0, origin.1],
                    "boardTopYMetres": report.thickness_mm * MM_TO_M / 2.0,
                    "boardBottomYMetres": -report.thickness_mm * MM_TO_M / 2.0
                },
                "shellRepresentation":"nominal_thickness_envelope",
                "physicalLayerGeometryComplete":false,
                "tessellation": { "maxCurveChordErrorMm": report.max_curve_chord_error_mm, "outlineJoinToleranceMm": board.outline.join_tolerance_mm,
                    "maxJoinDisplacementMm":board.outline.join_tolerance_mm/2.,"sourceGeometryEdited":false },
                "coverage": {
                    "componentCount": report.component_count,
                    "selectedComponentCount": report.selected_component_count,
                    "sourceComponentCount": report.source_component_count,
                    "filteredComponentCount": report.filtered_component_count,
                    "componentFilter": &report.component_filter,
                    "noDnp": report.no_dnp,
                    "excludedDnpComponentCount": report.excluded_dnp_component_count,
                    "componentModelsEmitted": report.component_models_emitted,
                    "componentModelsUnresolved": report.component_models_unresolved,
                    "modelReferences": report.model_references,
                    "selectedModelReferences": report.selected_model_references,
                    "sourceModelReferences": report.source_model_references,
                    "filteredModelReferences": report.filtered_model_references,
                    "modelReferencesEmitted": report.model_references_emitted,
                    "modelReferencesUnresolved": report.model_references_unresolved,
                    "unresolvedComponentsTotal": report.unresolved_components_total,
                    "unresolvedComponentsTruncated": report.unresolved_components_truncated,
                    "unresolvedComponents": report.unresolved_components.iter().map(|item| json!({
                        "reference": item.reference, "uuid": item.uuid, "sourceKind": item.source_kind,
                        "sourceOrdinal": item.source_ordinal, "sourceIdentity": item.source_identity,
                        "modelPaths": item.model_paths, "modelReferenceCount": item.model_reference_count,
                        "modelReferencesWithoutPath": item.model_references_without_path,
                        "modelPathsTruncated": item.model_paths_truncated,
                    "modelPathsRedacted": item.model_paths_redacted,
                        "reason": item.reason, "noModelReferenceReason": item.no_model_reference_reason
                    })).collect::<Vec<_>>(),
                    "unresolvedModelsTotal": report.unresolved_models_total,
                    "unresolvedModelsTruncated": report.unresolved_models_truncated,
                    "unresolvedModels": report.unresolved_models.iter().map(|item| json!({
                        "componentReference": item.component_reference,
                        "componentUuid": item.component_uuid,
                        "sourceKind": item.source_kind,
                        "sourceOrdinal": item.source_ordinal,
                        "sourceIdentity": item.source_identity,
                        "status": item.status
                    })).collect::<Vec<_>>(),
                    "componentModelCoverageComplete": report.component_models_unresolved == 0 && report.model_references_unresolved == 0
                },
                "mesh": {
                    "polygonizedVertices": report.polygonized_vertices, "triangles": report.triangles,
                    "exactOutlineAreaMm2": report.exact_outline_area_mm2,
                    "polygonizedOutlineAreaMm2": report.polygonized_outline_area_mm2,
                    "triangulatedOutlineAreaMm2": report.triangulated_outline_area_mm2,
                    "thicknessMm": report.thickness_mm, "boundsMetres": { "min": mesh.min, "max": mesh.max }
                }
            }}
        },
        "scene": 0,
        "scenes": [{ "name": "Tokito board shell", "nodes": [0] }],
        "nodes": [{ "name": "Board shell", "mesh": 0 }],
        "meshes": [{ "name": "Board shell", "primitives": [{
            "attributes": { "POSITION": 0, "NORMAL": 1 }, "material": 0, "mode": 4
        }]}],
        "materials": [{ "name": "Board shell", "pbrMetallicRoughness": {
            "baseColorFactor": [0.36, 0.39, 0.43, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.82
        }, "doubleSided": false }],
        "buffers": [{ "byteLength": mesh.bin.len() }],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": position_bytes, "target": 34962 },
            { "buffer": 0, "byteOffset": position_bytes, "byteLength": normal_bytes, "target": 34962 }
        ],
        "accessors": [
            { "bufferView": 0, "byteOffset": 0, "componentType": 5126, "count": count, "type": "VEC3", "min": mesh.min, "max": mesh.max },
            { "bufferView": 1, "byteOffset": 0, "componentType": 5126, "count": count, "type": "VEC3" }
        ]
    });
    Ok(document)
}

/// Approximate the exponent of the VRML97 Phong highlight with glTF perceptual
/// roughness. VRML defines its exponent as `shininess * 128`; the mapping below
/// is a display-oriented lobe-width approximation, not a physical material fit.
fn vrml_shininess_to_roughness(shininess: f64) -> f64 {
    let phong_exponent = shininess.clamp(0.0, 1.0) * 128.0;
    (2.0 / (phong_exponent + 2.0)).sqrt()
}

fn projected_vrml_material(source: Option<&crate::model_mesh::MeshMaterial>, solid: bool) -> Value {
    let fallback = crate::model_mesh::MeshMaterial::default();
    let m = source.unwrap_or(&fallback);
    json!({"name":"VRML Phong display projection",
        "pbrMetallicRoughness":{"baseColorFactor":[m.diffuse_color[0],m.diffuse_color[1],m.diffuse_color[2],1.0-m.transparency],
            "metallicFactor":0.0,"roughnessFactor":vrml_shininess_to_roughness(m.shininess)},
        "extensions":{"KHR_materials_specular":{"specularFactor":1.0,"specularColorFactor":m.specular_color}},
        "emissiveFactor":m.emissive_color,
        "alphaMode":if m.transparency > 0.0 {"BLEND"} else {"OPAQUE"},
        "doubleSided":!solid,
        "extras":{"tokito":{"materialProjection":"diffuse-alpha-emissive plus optional specular color; approximate Phong-exponent-to-perceptual-roughness; not lighting or physical-material equivalence",
            "roughnessProjection":{"source":"VRML97 Material.shininess","sourcePhongExponent":m.shininess*128.0,
                "formula":"sqrt(2 / (sourcePhongExponent + 2))","approximation":"display lobe-width mapping; not a recovered physical roughness"},
            "authoredMaterialPresent":source.is_some(),"sourceVrmlMaterial":{"diffuseColor":m.diffuse_color,
                "specularColor":m.specular_color,"emissiveColor":m.emissive_color,"ambientIntensity":m.ambient_intensity,
                "shininess":m.shininess,"transparency":m.transparency},"sourceSolid":solid}}})
}

fn gltf_assembly_document(
    shell: &Mesh,
    components: &[ComponentMesh],
    report: &GlbCoverageReport,
    dnp_filter: &DnpFilterCoverage,
    board: &PhysicalBoardDocument,
    origin: (f64, f64),
) -> Result<Value, EngineError> {
    let source = board.source();
    let mut buffer_views = Vec::with_capacity((components.len() + 1) * 2);
    let mut accessors = Vec::with_capacity((components.len() + 1) * 2);
    let mut meshes = Vec::with_capacity(components.len() + 1);
    let mut nodes = Vec::with_capacity(components.len() + 1);
    let mut byte_offset = 0usize;
    let mut materials = vec![
        json!({"name":"Board nominal envelope","pbrMetallicRoughness":{"baseColorFactor":[0.36,0.39,0.43,1.0],"metallicFactor":0.0,"roughnessFactor":0.82},"doubleSided":false}),
        projected_vrml_material(None, true),
    ];
    let mut material_indices = BTreeMap::new();
    material_indices.insert(serde_json::to_string(&materials[1]).unwrap(), 1usize);
    append_gltf_shell_mesh(
        shell,
        &mut buffer_views,
        &mut accessors,
        &mut meshes,
        &mut nodes,
        &mut byte_offset,
    )?;
    let mut by_component = BTreeMap::<usize, Vec<&ComponentMesh>>::new();
    for component in components {
        by_component
            .entry(component.source_ordinal)
            .or_default()
            .push(component);
    }
    let mut scene_nodes = vec![0usize];
    for (source_ordinal, parts) in by_component {
        let first = parts
            .first()
            .ok_or_else(|| EngineError::new("component group unexpectedly has no mesh parts"))?;
        let reference = first
            .reference
            .as_deref()
            .filter(|reference| !reference.is_empty())
            .ok_or_else(|| EngineError::new("assembled component has no unique reference"))?;
        let uuid = first
            .uuid
            .as_deref()
            .filter(|uuid| !uuid.is_empty())
            .ok_or_else(|| EngineError::new("assembled component has no unique UUID"))?;
        if parts.iter().any(|part| {
            part.reference.as_deref() != Some(reference)
                || part.uuid.as_deref() != Some(uuid)
                || part.source_ordinal != source_ordinal
        }) {
            return Err(EngineError::new(
                "component mesh group contains conflicting source identities",
            ));
        }
        let mut primitives = Vec::with_capacity(parts.len());
        for component in parts.iter().copied() {
            let material = projected_vrml_material(component.material.as_ref(), component.solid);
            let key =
                serde_json::to_string(&material).map_err(|e| EngineError::new(e.to_string()))?;
            let material_index = if let Some(&index) = material_indices.get(&key) {
                index
            } else {
                if materials.len() >= 4096 {
                    return Err(EngineError::new("assembly material budget exceeded"));
                }
                let index = materials.len();
                materials.push(material);
                material_indices.insert(key, index);
                index
            };
            primitives.push(append_gltf_primitive(
                &component.mesh,
                material_index,
                Some(component_primitive_provenance(component)),
                &mut buffer_views,
                &mut accessors,
                &mut byte_offset,
            )?);
        }
        let mesh_index = meshes.len();
        meshes.push(json!({
            "name": format!("Component {reference}"),
            "primitives": primitives
        }));
        let selector_node = nodes.len();
        nodes.push(json!({
            "name": format!("ref:{reference}"),
            "mesh": mesh_index,
            "extras": { "tokito": {
                "selectorContract": "componentNodePrefix+refdes",
                "componentReference": reference,
                "componentUuid": uuid,
                "componentSourceOrdinal": source_ordinal,
                "selectable": true
            }}
        }));
        scene_nodes.push(selector_node);
    }
    let bin_len = byte_offset;
    let mut document = json!({
        "asset": {
            "version": "2.0", "generator": "Tokito CLI",
            "extras": { "tokito": {
                "format": "tokito.board-assembly.glb.v0",
                "scope": report.scope,
                "shellGeometryComplete": report.shell_geometry_complete,
                "wholeBoardDigitalTwinComplete": report.whole_board_digital_twin_complete,
                "partialExport": report.partial_export,
                "source": { "format": "kicad_pcb", "sha256": source.sha256, "formatVersion": source.format_version },
                "coordinateFrame": {
                    "units": "m", "handedness": "right-handed", "upAxis": "Y", "boardPlane": "XZ",
                    "sourceUnits": "mm", "sourceCoordinateFrame": "KiCad-board-Y-down",
                    "sourceOriginMm": [origin.0, origin.1],
                    "boardTopYMetres": report.thickness_mm * MM_TO_M / 2.0,
                    "boardBottomYMetres": -report.thickness_mm * MM_TO_M / 2.0
                },
                "shellRepresentation":"nominal_thickness_envelope",
                "physicalLayerGeometryComplete":false,
                "tessellation": { "maxCurveChordErrorMm": report.max_curve_chord_error_mm, "outlineJoinToleranceMm":board.outline.join_tolerance_mm, "maxJoinDisplacementMm":board.outline.join_tolerance_mm/2., "sourceGeometryEdited":false },
                "coverage": {
                    "componentCount": report.component_count,
                    "selectedComponentCount": report.selected_component_count,
                    "sourceComponentCount": report.source_component_count,
                    "filteredComponentCount": report.filtered_component_count,
                    "componentFilter": &report.component_filter,
                    "noDnp": report.no_dnp,
                    "excludedDnpComponentCount": report.excluded_dnp_component_count,
                    "componentModelsEmitted": report.component_models_emitted,
                    "componentModelsUnresolved": report.component_models_unresolved,
                    "modelReferences": report.model_references,
                    "selectedModelReferences": report.selected_model_references,
                    "sourceModelReferences": report.source_model_references,
                    "filteredModelReferences": report.filtered_model_references,
                    "modelReferencesEmitted": report.model_references_emitted,
                    "modelReferencesUnresolved": report.model_references_unresolved,
                    "unresolvedComponentsTotal": report.unresolved_components_total,
                    "unresolvedComponentsTruncated": report.unresolved_components_truncated,
                    "unresolvedModelsTotal": report.unresolved_models_total,
                    "unresolvedModelsTruncated": report.unresolved_models_truncated,
                    "unresolvedModels": report.unresolved_models.iter().map(|item| json!({
                        "componentReference": item.component_reference,
                        "componentUuid": item.component_uuid,
                        "sourceKind": item.source_kind,
                        "sourceOrdinal": item.source_ordinal,
                        "sourceIdentity": item.source_identity,
                        "status": item.status
                    })).collect::<Vec<_>>(),
                    "componentModelCoverageComplete": report.component_models_unresolved == 0 && report.model_references_unresolved == 0
                }
            }}
        },
        "scene": 0,
        "extensionsUsed": ["KHR_materials_specular"],
        "scenes": [{ "name": "Tokito board assembly", "nodes": scene_nodes }],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "buffers": [{ "byteLength": bin_len }],
        "bufferViews": buffer_views,
        "accessors": accessors
    });
    if dnp_filter.applied {
        document["asset"]["extras"]["tokito"]["coverage"]["componentsIncluded"] =
            json!(report.selected_component_count);
        document["asset"]["extras"]["tokito"]["coverage"]["dnpFilter"] = json!({
            "enabled": true,
            "componentsExcluded": dnp_filter.components_excluded,
            "modelReferencesExcluded": dnp_filter.model_references_excluded
        });
    }
    if let Some(filter) = &report.unspecified_filter {
        document["asset"]["extras"]["tokito"]["coverage"]["unspecifiedFilter"] = filter.as_json();
    }
    Ok(document)
}

fn component_primitive_provenance(component: &ComponentMesh) -> Value {
    let mut value = json!({ "tokito": {
        "componentReference": component.reference,
        "componentUuid": component.uuid,
        "componentSourceOrdinal": component.source_ordinal,
        "sourceAssetSha256": component.source_asset_sha256,
        "partOrdinal": component.part_ordinal,
        "selectable": false
    }});
    if let Some((manifest, index)) = &component.assignment_origin {
        value["tokito"]["manifestSha256"] = json!(manifest);
        value["tokito"]["manifestEntryIndex"] = json!(index);
        value["tokito"]["modelOrigin"] = json!("explicit_assignment");
    } else {
        value["tokito"]["modelSourceOrdinal"] = json!(component.model_source_ordinal);
    }
    value
}

fn append_gltf_shell_mesh(
    mesh: &Mesh,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    meshes: &mut Vec<Value>,
    nodes: &mut Vec<Value>,
    byte_offset: &mut usize,
) -> Result<(), EngineError> {
    let primitive = append_gltf_primitive(mesh, 0, None, buffer_views, accessors, byte_offset)?;
    let mesh_index = meshes.len();
    meshes.push(json!({
        "name": "Board shell",
        "primitives": [primitive]
    }));
    nodes.push(json!({ "name": "Board shell", "mesh": mesh_index }));
    Ok(())
}

fn append_gltf_primitive(
    mesh: &Mesh,
    material: usize,
    extras: Option<Value>,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    byte_offset: &mut usize,
) -> Result<Value, EngineError> {
    if mesh.positions.is_empty()
        || mesh.positions.len() != mesh.normals.len()
        || !mesh.positions.len().is_multiple_of(9)
    {
        return Err(EngineError::new(
            "GLB primitive has invalid triangle geometry",
        ));
    }
    let count = mesh.positions.len() / 3;
    let position_bytes = count
        .checked_mul(12)
        .ok_or_else(|| EngineError::new("position buffer length overflow"))?;
    let normal_bytes = count
        .checked_mul(12)
        .ok_or_else(|| EngineError::new("normal buffer length overflow"))?;
    let position_view = buffer_views.len();
    buffer_views.push(json!({
        "buffer": 0, "byteOffset": *byte_offset, "byteLength": position_bytes, "target": 34962
    }));
    *byte_offset = byte_offset
        .checked_add(position_bytes)
        .ok_or_else(|| EngineError::new("GLB buffer offset overflow"))?;
    let normal_view = buffer_views.len();
    buffer_views.push(json!({
        "buffer": 0, "byteOffset": *byte_offset, "byteLength": normal_bytes, "target": 34962
    }));
    *byte_offset = byte_offset
        .checked_add(normal_bytes)
        .ok_or_else(|| EngineError::new("GLB buffer offset overflow"))?;
    let position_accessor = accessors.len();
    accessors.push(json!({
        "bufferView": position_view, "byteOffset": 0, "componentType": 5126,
        "count": count, "type": "VEC3", "min": mesh.min, "max": mesh.max
    }));
    let normal_accessor = accessors.len();
    accessors.push(json!({
        "bufferView": normal_view, "byteOffset": 0, "componentType": 5126,
        "count": count, "type": "VEC3"
    }));
    let mut primitive = json!({
        "attributes": { "POSITION": position_accessor, "NORMAL": normal_accessor },
        "material": material,
        "mode": 4
    });
    if let Some(extras) = extras {
        primitive["extras"] = extras;
    }
    Ok(primitive)
}

fn combined_bin(shell: &Mesh, components: &[ComponentMesh]) -> Result<Vec<u8>, EngineError> {
    let capacity = components
        .iter()
        .try_fold(shell.bin.len(), |total, component| {
            total
                .checked_add(component.mesh.bin.len())
                .ok_or_else(|| EngineError::new("GLB buffer length overflow"))
        })?;
    let mut bin = Vec::with_capacity(capacity);
    bin.extend_from_slice(&shell.bin);
    for component in components {
        bin.extend_from_slice(&component.mesh.bin);
    }
    Ok(bin)
}

fn container_len(json_len: usize, bin_len: usize) -> Result<usize, EngineError> {
    let pad = |n: usize| n.checked_add(3).map(|value| value & !3);
    let json = pad(json_len).ok_or_else(|| EngineError::new("JSON chunk length overflow"))?;
    let bin = pad(bin_len).ok_or_else(|| EngineError::new("BIN chunk length overflow"))?;
    28usize
        .checked_add(json)
        .and_then(|n| n.checked_add(bin))
        .ok_or_else(|| EngineError::new("GLB total length overflow"))
}
fn finite_point(p: Point2) -> bool {
    p.x.is_finite() && p.y.is_finite()
}
fn distance(a: Point2, b: Point2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GlbExportOptions;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn material_projection_preserves_opacity_emission_and_raw_phong_metadata() {
        let source = crate::model_mesh::MeshMaterial {
            diffuse_color: [0.1, 0.2, 0.3],
            specular_color: [0.7, 0.6, 0.5],
            emissive_color: [0.02, 0.03, 0.04],
            ambient_intensity: 0.4,
            shininess: 0.8,
            transparency: 0.25,
        };
        let projected = projected_vrml_material(Some(&source), false);
        assert_eq!(
            projected["pbrMetallicRoughness"]["baseColorFactor"],
            json!([0.1, 0.2, 0.3, 0.75])
        );
        assert_eq!(projected["emissiveFactor"], json!([0.02, 0.03, 0.04]));
        assert_eq!(projected["alphaMode"], "BLEND");
        assert_eq!(projected["doubleSided"], true);
        let exponent = 0.8_f64 * 128.0;
        let expected_roughness = (2.0 / (exponent + 2.0)).sqrt();
        assert!(
            (projected["pbrMetallicRoughness"]["roughnessFactor"]
                .as_f64()
                .unwrap()
                - expected_roughness)
                .abs()
                < 1e-15
        );
        assert_eq!(
            projected["extensions"]["KHR_materials_specular"]["specularColorFactor"],
            json!([0.7, 0.6, 0.5])
        );
        assert_eq!(
            projected["extras"]["tokito"]["roughnessProjection"]["sourcePhongExponent"],
            json!(102.4)
        );
        assert_eq!(
            projected["extras"]["tokito"]["sourceVrmlMaterial"]["shininess"],
            0.8
        );
        assert_eq!(
            projected["extras"]["tokito"]["sourceVrmlMaterial"]["specularColor"],
            json!([0.7, 0.6, 0.5])
        );
        let fallback = projected_vrml_material(None, true);
        assert_eq!(
            fallback["pbrMetallicRoughness"]["baseColorFactor"],
            json!([0.8, 0.8, 0.8, 1.0])
        );
        assert_eq!(
            fallback["extras"]["tokito"]["authoredMaterialPresent"],
            false
        );
        assert_eq!(fallback["doubleSided"], false);
        assert_eq!(
            fallback["extensions"]["KHR_materials_specular"]["specularColorFactor"],
            json!([0.0, 0.0, 0.0])
        );
        assert_eq!(
            fallback["pbrMetallicRoughness"]["roughnessFactor"],
            json!(vrml_shininess_to_roughness(
                crate::model_mesh::MeshMaterial::default().shininess
            ))
        );
    }

    #[test]
    fn vrml_shininess_maps_to_bounded_monotonic_display_roughness() {
        let levels = [0.0, 0.25, 0.7, 1.0].map(vrml_shininess_to_roughness);
        assert_eq!(levels[0], 1.0);
        assert!(levels.iter().all(|value| (0.0..=1.0).contains(value)));
        assert!(levels.windows(2).all(|pair| pair[0] > pair[1]));
        assert!((levels[3] - (2.0_f64 / 130.0).sqrt()).abs() < 1e-15);
    }

    #[test]
    fn chord_area_bound_matches_independent_regular_polygon_and_shrinks() {
        for sweep in [PI, -PI, TAU, -TAU] {
            let radius = 3.0;
            let mut previous = f64::INFINITY;
            for segments in [8, 16, 64, 256] {
                // Sector area minus the sum of triangular wedges, independently
                // evaluated without the stable-series helper used by the bound.
                let analytic = 0.5 * radius * radius * sweep.abs();
                let polygon =
                    0.5 * radius * radius * segments as f64 * (sweep.abs() / segments as f64).sin();
                let bound = curve_chord_area_loss(radius, sweep, segments);
                assert!((bound - (analytic - polygon)).abs() < 1e-12);
                assert!(bound > 0.0 && bound < previous);
                previous = bound;
            }
        }
        assert!((curve_chord_area_loss(1.0, PI, 16) - 0.0100737506658704).abs() < 1e-14);
    }

    #[test]
    fn circular_contour_bound_covers_only_chord_loss_and_join_budget() {
        let b = board(
            r#"(gr_circle (center 0 0) (end 3 0) (layer Edge.Cuts) (uuid circle))"#,
            "",
        );
        let contour = &b.outline().contours[0];
        let n = curve_segments(3.0, TAU, b.outline().join_tolerance_mm).unwrap();
        let actual_loss = PI * 9.0 - 0.5 * n as f64 * 9.0 * (TAU / n as f64).sin();
        let bound = contour_area_error_bound(contour, &b).unwrap();
        let join_budget = TAU * 3.0 * b.outline().join_tolerance_mm;
        assert!((bound - actual_loss - join_budget).abs() < 1e-12);
        assert!(bound < 0.2);
        export_board_shell(&b, true).unwrap();
    }

    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn board(outline: &str, footprint: &str) -> PhysicalBoardDocument {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let file = Temp(std::env::temp_dir().join(format!(
            "tokito-shell-{}-{time}-{n}.kicad_pcb",
            std::process::id()
        )));
        let text = format!(
            "(kicad_pcb (version 20240108) (general (thickness 1.6)) (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal) (44 \"Edge.Cuts\" user)) {outline} {footprint})"
        );
        fs::write(&file.0, text).expect("write test board");
        super::super::load_board(&file.0).expect("parse test board")
    }

    #[test]
    fn explicit_chaining_preserves_source_and_accounts_for_join_displacement() {
        let outline = r#"(gr_line (start 0 0) (end 10 0) (layer Edge.Cuts) (uuid a))
            (gr_line (start 10.005 0) (end 10 10) (layer Edge.Cuts) (uuid b))
            (gr_line (start 10 10) (end 0 10) (layer Edge.Cuts) (uuid c))
            (gr_line (start 0 10) (end 0 0) (layer Edge.Cuts) (uuid d))"#;
        let original = board(outline, "");
        let hash = original.source().sha256.to_owned();
        let primitives = original.outline().primitives.clone();
        assert!(export_board_shell(&original, true).is_err());
        let tight = board(outline, "")
            .with_outline_chaining_tolerance(0.001)
            .unwrap();
        assert!(!tight.outline().is_valid());
        let chained = original.with_outline_chaining_tolerance(0.01).unwrap();
        assert!(
            chained.outline().is_valid(),
            "{:?}",
            chained.outline().diagnostics
        );
        assert_eq!(chained.source().sha256, hash);
        assert_eq!(chained.outline().primitives, primitives);
        let export = export_board_shell(&chained, true).unwrap();
        assert_eq!(export.report.outline_chaining_tolerance_mm, 0.01);
        assert!(!export.report.whole_board_digital_twin_complete);
        let tess = extras(&export.bytes)["asset"]["extras"]["tokito"]["tessellation"].clone();
        assert_eq!(tess["maxJoinDisplacementMm"], 0.005);
        assert_eq!(tess["sourceGeometryEdited"], false);
        assert!(
            board(outline, "")
                .with_outline_chaining_tolerance(0.1)
                .is_err()
        );
    }

    #[test]
    fn chaining_does_not_bypass_crossings_and_reserves_curve_error_budget() {
        let crossing = r#"(gr_line (start 0 0) (end 10 10) (layer Edge.Cuts) (uuid a))
            (gr_line (start 10 10) (end 0 10) (layer Edge.Cuts) (uuid b))
            (gr_line (start 0 10) (end 10 0) (layer Edge.Cuts) (uuid c))
            (gr_line (start 10 0) (end 0 0) (layer Edge.Cuts) (uuid d))"#;
        assert!(
            board(crossing, "")
                .with_outline_chaining_tolerance(0.01)
                .is_err()
        );
        assert!(
            curve_segments(1.55, PI / 2., 0.01).unwrap()
                > curve_segments(1.55, PI / 2., 0.000001).unwrap()
        );
        assert!(curve_segments(1.55, PI / 2., 0.1).is_err());
    }

    fn rect(a: f64, b: f64, c: f64, d: f64, id: &str) -> String {
        format!(
            "(gr_line (start {a} {b}) (end {c} {b}) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\") (uuid \"{id}-a\")) \
        (gr_line (start {c} {b}) (end {c} {d}) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\") (uuid \"{id}-b\")) \
        (gr_line (start {c} {d}) (end {a} {d}) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\") (uuid \"{id}-c\")) \
        (gr_line (start {a} {d}) (end {a} {b}) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\") (uuid \"{id}-d\"))"
        )
    }
    fn footprint() -> &'static str {
        "(footprint \"R:R_0603\" (layer \"F.Cu\") (at 5 5) (uuid \"component-1\") \
        (property \"Reference\" \"R1\" (at 5 5 0) (layer \"F.SilkS\")) \
        (property \"Value\" \"1k\" (at 5 5 0) (layer \"F.Fab\")) \
        (model \"R_0603.step\" (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 0))))"
    }
    fn extras(bytes: &[u8]) -> Value {
        let glb = Glb::from_slice(bytes).expect("GLB framing");
        serde_json::from_slice(&glb.json).expect("GLB JSON")
    }

    #[test]
    fn component_glb_serialization_preserves_three_distinct_corner_normals() {
        let part = crate::component_pose::PlacedModelMeshPart {
            source_shape_ordinal: 0,
            positions_m: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            triangles: vec![[0, 1, 2]],
            flat_normals: vec![[0.0, 0.0, 1.0]],
            corner_normals: vec![[
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0],
                [0.0, 2.0f64.sqrt().recip(), 2.0f64.sqrt().recip()],
            ]],
            material: None,
            source_crease_angle_radians: 0.5,
            solid: true,
        };
        let mesh = mesh_from_component_part(&part).expect("smoothed triangle mesh");
        assert_eq!(
            mesh.positions,
            vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        );
        assert_eq!(
            mesh.normals,
            vec![
                0.0,
                0.0,
                1.0,
                0.0,
                1.0,
                0.0,
                0.0,
                2.0f32.sqrt().recip(),
                2.0f32.sqrt().recip(),
            ]
        );
    }

    #[test]
    fn rectangle_shell_is_self_contained_metre_y_up_glb() {
        let b = board(&rect(0.0, 0.0, 10.0, 20.0, "outer"), "");
        assert!(
            export_board_shell(&b, false).is_err(),
            "partial shell requires explicit opt-in"
        );
        let out = export_board_shell(&b, true).expect("explicit shell-only export");
        assert_eq!(out.report.triangles, 12);
        assert!(out.report.partial_export);
        assert!((out.report.triangulated_outline_area_mm2 - 200.0).abs() < 1e-8);
        let parsed = gltf::Gltf::from_slice(&out.bytes).expect("glTF validation");
        assert_eq!(parsed.document.meshes().count(), 1);
        let receipt = out.report.report_json();
        let parsed_receipt: Value = serde_json::from_str(&receipt).expect("coverage receipt JSON");
        assert_eq!(parsed_receipt["partialExport"], true);
        let extra = extras(&out.bytes)["asset"]["extras"]["tokito"].clone();
        assert_eq!(extra["coordinateFrame"]["units"], "m");
        assert_eq!(extra["coordinateFrame"]["upAxis"], "Y");
        assert_eq!(extra["coordinateFrame"]["boardPlane"], "XZ");
        assert_eq!(extra["coordinateFrame"]["boardTopYMetres"], 0.0008);
        assert_eq!(extra["coordinateFrame"]["boardBottomYMetres"], -0.0008);
        assert_eq!(
            extra["coordinateFrame"]["sourceOriginMm"],
            json!([5.0, 10.0])
        );
        let document = extras(&out.bytes);
        let assert_vec3_close = |value: &Value, expected: [f64; 3]| {
            let actual = value.as_array().expect("VEC3 accessor bounds");
            for (actual, expected) in actual.iter().zip(expected) {
                assert!((actual.as_f64().expect("numeric bound") - expected).abs() < 1e-9);
            }
        };
        assert_vec3_close(&document["accessors"][0]["min"], [-0.005, -0.0008, -0.01]);
        assert_vec3_close(&document["accessors"][0]["max"], [0.005, 0.0008, 0.01]);
    }

    #[test]
    fn hole_is_triangulated_and_extruded_as_a_cutout() {
        let outline = format!(
            "{} {}",
            rect(0.0, 0.0, 10.0, 10.0, "outer"),
            rect(4.0, 4.0, 6.0, 6.0, "hole")
        );
        let b = board(&outline, "");
        let out = export_board_shell(&b, true).expect("explicit board-with-hole export");
        assert_eq!(out.report.triangles, 32);
        assert!((out.report.triangulated_outline_area_mm2 - 96.0).abs() < 1e-8);
        gltf::Gltf::from_slice(&out.bytes).expect("cutout GLB validates");
    }

    #[test]
    fn full_circle_outline_obeys_chord_error_budget() {
        let b = board(
            "(gr_circle (center 0 0) (end 10 0) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\") (uuid \"circle\"))",
            "",
        );
        let out = export_board_shell(&b, true).expect("explicit circular shell export");
        assert!(out.report.polygonized_vertices >= 64);
        assert_eq!(out.report.max_curve_chord_error_mm, MAX_CHORD_ERROR_MM);
        assert!((out.report.exact_outline_area_mm2 - PI * 100.0).abs() < 1e-8);
        gltf::Gltf::from_slice(&out.bytes).expect("curved GLB validates");
    }

    #[test]
    fn rejects_mesh_positions_that_exceed_f32_precision_budget() {
        let b = board(&rect(-1_000_000.0, -10.0, 999_999.0, 10.0, "large"), "");
        let error =
            export_board_shell(&b, true).expect_err("large coordinates lose mesh precision");
        assert!(error.to_string().contains("f32 position-error budget"));
    }

    #[test]
    fn absolute_model_paths_are_redacted_from_coverage_metadata() {
        for path in [
            "C:/Users/Private/R_0603.step",
            "\\\\workstation\\private\\R_0603.step",
            "/home/private/R_0603.step",
            "${KIPRJMOD}/Models/R_0603.step",
        ] {
            let (reported, redacted) = report_model_path(path);
            if path.starts_with("${") {
                assert!(!redacted);
                assert_eq!(reported, path);
            } else {
                assert!(redacted);
                assert!(reported.contains("R_0603.step"));
                assert!(!reported.contains("Private"));
            }
        }
    }

    #[test]
    fn reversed_arc_contour_is_tessellated_and_extruded() {
        let outline = "(gr_line (start -10 5) (end -10 -5) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\") (uuid \"a\")) \
        (gr_line (start -10 -5) (end 0 -5) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\") (uuid \"b\")) \
        (gr_arc (start 0 5) (mid 5 0) (end 0 -5) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\") (uuid \"c\")) \
        (gr_line (start 0 5) (end -10 5) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\") (uuid \"d\"))";
        let b = board(outline, "");
        assert!(
            b.outline().is_valid(),
            "synthetic reversed-arc contour validates"
        );
        let out = export_board_shell(&b, true).expect("explicit curved shell export");
        let expected = 100.0 + 25.0 * PI / 2.0;
        assert!((out.report.exact_outline_area_mm2 - expected).abs() < 1e-7);
        assert!(out.report.polygonized_vertices > 4);
        gltf::Gltf::from_slice(&out.bytes).expect("reversed-arc GLB validates");
    }

    #[test]
    fn component_models_require_explicit_partial_export_flag() {
        let b = board(&rect(0.0, 0.0, 10.0, 10.0, "outer"), footprint());
        assert!(
            export_board_shell(&b, false)
                .expect_err("default must refuse")
                .to_string()
                .contains("allow_partial=true")
        );
        let out = export_board_shell(&b, true).expect("explicitly allowed shell");
        assert!(out.report.partial_export);
        assert_eq!(out.report.component_count, 1);
        assert_eq!(out.report.component_models_emitted, 0);
        assert_eq!(out.report.component_models_unresolved, 1);
        assert_eq!(out.report.model_references, 1);
        assert_eq!(out.report.model_references_unresolved, 1);
        assert_eq!(out.report.unresolved_components_total, 1);
        assert!(!out.report.unresolved_components_truncated);
        let item = &out.report.unresolved_components[0];
        assert_eq!(item.reference.as_deref(), Some("R1"));
        assert_eq!(item.uuid.as_deref(), Some("component-1"));
        assert_eq!(item.model_paths, vec!["R_0603.step"]);
        assert_eq!(
            item.reason,
            "component_geometry_not_emitted_by_board_shell_only_export"
        );
        assert_eq!(item.no_model_reference_reason, None);
        let glb_coverage = extras(&out.bytes)["asset"]["extras"]["tokito"]["coverage"].clone();
        assert_eq!(glb_coverage["unresolvedComponentsTotal"], 1);
        assert_eq!(
            glb_coverage["unresolvedComponents"][0]["modelPaths"][0],
            "R_0603.step"
        );
        let receipt: Value =
            serde_json::from_str(&out.report.report_json()).expect("coverage report JSON");
        assert_eq!(receipt["coverage"]["unresolvedComponentsTotal"], 1);
        assert_eq!(receipt["coverage"]["unresolvedComponentsTruncated"], false);
        assert_eq!(
            receipt["coverage"]["unresolvedComponents"][0]["reference"],
            "R1"
        );
        assert_eq!(
            receipt["coverage"]["unresolvedComponents"][0]["uuid"],
            "component-1"
        );
        assert_eq!(
            receipt["coverage"]["unresolvedComponents"][0]["sourceOrdinal"],
            item.source_ordinal
        );
        assert_eq!(
            receipt["coverage"]["unresolvedComponents"][0]["modelPaths"][0],
            "R_0603.step"
        );
        assert_eq!(
            receipt["coverage"]["unresolvedComponents"][0]["reason"],
            "component_geometry_not_emitted_by_board_shell_only_export"
        );
        assert_eq!(
            extras(&out.bytes)["asset"]["extras"]["tokito"]["coverage"]["componentModelCoverageComplete"],
            false
        );
        gltf::Gltf::from_slice(&out.bytes).expect("partial GLB validates");
    }

    #[test]
    fn selected_export_keeps_unknown_owner_model_in_unresolved_coverage() {
        let mut b = board(&rect(0.0, 0.0, 10.0, 10.0, "outer"), footprint());
        b.engineering.models[0].source.component_ordinal = Some(usize::MAX);
        let filter = ModelAssemblyFilter::exact_component_references(&b, &["R1".to_owned()])
            .expect("known unique reference");
        let resolver = crate::model_resolver::ModelResolver::new(
            crate::model_resolver::ModelResolverConfig::new(std::env::temp_dir()),
        )
        .expect("resolver config");

        let refused = b
            .export_glb_with_models_filtered(&resolver, GlbExportOptions::default(), &filter)
            .expect_err("unknown model ownership must make filtered export partial");
        assert!(refused.to_string().contains("1 model reference(s)"));

        let export = b
            .export_glb_with_models_filtered(
                &resolver,
                GlbExportOptions {
                    allow_partial: true,
                },
                &filter,
            )
            .expect("explicit partial export");
        assert!(export.report.partial_export);
        assert_eq!(export.report.source_model_references, 1);
        assert_eq!(export.report.model_references, 1);
        assert_eq!(export.report.filtered_model_references, 0);
        assert_eq!(export.report.model_references_unresolved, 1);
        let coverage = extras(&export.bytes)["asset"]["extras"]["tokito"]["coverage"].clone();
        assert_eq!(coverage["unresolvedModelsTotal"], 1);
        assert_eq!(
            coverage["unresolvedModels"][0]["status"],
            "missing_owning_component"
        );
    }

    #[test]
    fn component_without_source_model_is_reported_without_disk_claims() {
        let without_model = footprint().replace(
            "(model \"R_0603.step\" (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 0)))",
            "",
        );
        let b = board(&rect(0.0, 0.0, 10.0, 10.0, "outer"), &without_model);
        let out = export_board_shell(&b, true).expect("explicit shell-only export");
        let item = &out.report.unresolved_components[0];
        assert_eq!(item.model_reference_count, 0);
        assert!(item.model_paths.is_empty());
        assert_eq!(
            item.reason,
            "component_geometry_not_emitted_by_board_shell_only_export"
        );
        assert_eq!(
            item.no_model_reference_reason,
            Some("no_source_model_reference_in_imported_data")
        );
        let report = out.report.report_json();
        assert!(report.contains("no_source_model_reference_in_imported_data"));
        assert!(!report.contains("file_missing"));
        assert!(!report.contains("sourcePath"));
    }

    #[test]
    fn absolute_source_model_paths_are_redacted_in_report_and_glb() {
        let absolute_footprint = footprint().replace("R_0603.step", "C:/Users/Private/R_0603.step");
        let b = board(&rect(0.0, 0.0, 10.0, 10.0, "outer"), &absolute_footprint);
        let out = export_board_shell(&b, true).expect("explicit shell-only export");
        let report = out.report.report_json();
        let glb_json = extras(&out.bytes).to_string();
        for output in [report, glb_json] {
            assert!(!output.contains("C:/Users/Private"));
            assert!(output.contains("absolute path redacted; file: R_0603.step"));
            assert!(output.contains("modelPathsRedacted"));
        }
    }

    #[test]
    fn unresolved_component_list_is_bounded_and_reports_truncation() {
        let footprints = (0..=MAX_COVERAGE_COMPONENTS)
            .map(|index| {
                footprint()
                    .replace("R1", &format!("R{index}"))
                    .replace("component-1", &format!("component-{index}"))
            })
            .collect::<Vec<_>>()
            .join(" ");
        let b = board(&rect(0.0, 0.0, 10.0, 10.0, "outer"), &footprints);
        let out = export_board_shell(&b, true).expect("explicit shell-only export");
        assert_eq!(out.report.component_count, MAX_COVERAGE_COMPONENTS + 1);
        assert_eq!(
            out.report.unresolved_components.len(),
            MAX_COVERAGE_COMPONENTS
        );
        assert!(out.report.unresolved_components_truncated);
        let receipt: Value =
            serde_json::from_str(&out.report.report_json()).expect("coverage JSON");
        assert_eq!(
            receipt["coverage"]["unresolvedComponentsTotal"],
            MAX_COVERAGE_COMPONENTS + 1
        );
        assert_eq!(receipt["coverage"]["unresolvedComponentsTruncated"], true);
        assert_eq!(
            receipt["coverage"]["unresolvedComponents"]
                .as_array()
                .unwrap()
                .len(),
            MAX_COVERAGE_COMPONENTS
        );
    }
}
