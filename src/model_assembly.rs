//! Bounded assembly preparation for the explicitly supported KiCad VRML subset.
//!
//! This is deliberately separate from GLB serialization. It resolves one model
//! reference at a time from caller-approved roots, parses only the bounded VRML
//! profile, and transforms it into the Tokito board frame. STEP, glTF and GLB
//! assets remain explicit non-emissions; a successful lookup is not treated as
//! a successful mesh import.

use crate::PhysicalBoardDocument;
use crate::component_pose::{
    BoardSide, KicadStepPlacement, ModelTransformMm, PlacedModelMesh, place_model_mesh_in_glb_m,
};
use crate::document::{Component, ModelReference, SourceLocator};
use crate::model_mesh::{MeshLimits, ModelUnits, parse_vrml_mesh};
use crate::model_resolver::{ModelAssetFormat, ModelResolutionStatus, ModelResolver};

/// Assembly-wide cap applied before any additional placed mesh is retained.
/// The flattened GLB positions/normals use 72 bytes per triangle, before JSON
/// and the board shell, so this also keeps the serialized geometry well below
/// the independent 128 MiB container ceiling.
const MAX_AGGREGATE_ASSEMBLY_TRIANGLES: usize = 250_000;
const MAX_RETAINED_MODEL_DIAGNOSTICS: usize = 2_048;

/// An individual model successfully converted into the Tokito GLB coordinate frame.
#[derive(Clone, Debug)]
pub struct AssembledComponentMesh {
    pub component_source: SourceLocator,
    pub component_reference: Option<String>,
    pub component_uuid: Option<String>,
    pub model_source: SourceLocator,
    pub source_asset_sha256: String,
    pub mesh: PlacedModelMesh,
}

/// Why a source model reference was deliberately not emitted into an assembly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelAssemblyStatus {
    MissingOwningComponent,
    MissingComponentPose,
    UnsupportedComponentSide,
    MissingStackupPlanes,
    StackupThicknessMismatch,
    MissingUniqueIdentity,
    MissingUniqueReference,
    LegacyAtUnsupported,
    Resolver(ModelResolutionStatus),
    UnsupportedAssetFormat,
    MeshRejected,
    PlacementRejected,
    AggregateGeometryBudgetExceeded,
}

impl ModelAssemblyStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingOwningComponent => "missing_owning_component",
            Self::MissingComponentPose => "missing_component_pose",
            Self::UnsupportedComponentSide => "unsupported_component_side",
            Self::MissingStackupPlanes => "missing_stackup_planes",
            Self::StackupThicknessMismatch => "stackup_thickness_mismatch",
            Self::MissingUniqueIdentity => "missing_unique_component_uuid",
            Self::MissingUniqueReference => "missing_unique_component_reference",
            Self::LegacyAtUnsupported => "legacy_at_unsupported",
            Self::Resolver(status) => status.as_str(),
            Self::UnsupportedAssetFormat => "unsupported_asset_format",
            Self::MeshRejected => "mesh_rejected",
            Self::PlacementRejected => "placement_rejected",
            Self::AggregateGeometryBudgetExceeded => "aggregate_geometry_budget_exceeded",
        }
    }
}

/// A skipped source model remains addressable and carries no local filesystem path.
#[derive(Clone, Debug)]
pub struct ModelAssemblyDiagnostic {
    pub model_source: SourceLocator,
    pub component_source: Option<SourceLocator>,
    pub status: ModelAssemblyStatus,
}

/// A partial assembly result. `emitted` has only parsed and placed VRML meshes;
/// every input model that could not meet that standard appears in `diagnostics`.
#[derive(Clone, Debug, Default)]
pub struct ModelAssembly {
    pub emitted: Vec<AssembledComponentMesh>,
    pub diagnostics: Vec<ModelAssemblyDiagnostic>,
    pub diagnostics_total: usize,
    pub diagnostics_truncated: bool,
    /// Component ordinals with at least one unresolved model reference, even
    /// when the bounded diagnostic list no longer retains each detail record.
    pub incomplete_component_ordinals: std::collections::BTreeSet<usize>,
}

impl ModelAssembly {
    pub fn is_complete_for_source_models(&self) -> bool {
        self.diagnostics_total == 0 && !self.diagnostics_truncated
    }
}

/// Resolve, parse and place the supported local VRML source-model subset.
///
/// The resolver's approved roots and variables are owned by the caller. This
/// function performs no model search, no network access, no STEP conversion and
/// no fallback geometry synthesis.
pub fn assemble_supported_vrml_models(
    board: &PhysicalBoardDocument,
    resolver: &ModelResolver,
) -> ModelAssembly {
    let planes = board
        .engineering()
        .stackup
        .as_ref()
        .and_then(|stackup| stackup.outer_copper_planes_mm().ok());
    let stackup_matches_shell = planes.is_some_and(|planes| {
        board.thickness_mm().is_some_and(|thickness| {
            (planes.front_copper_surface_z_mm - planes.back_copper_surface_z_mm - thickness).abs()
                <= 0.000_001
        })
    });
    let origin = board.outline().bounds.map(|bounds| {
        [
            (bounds.min_x + bounds.max_x) * 0.5,
            (bounds.min_y + bounds.max_y) * 0.5,
        ]
    });
    let mut result = ModelAssembly::default();
    let mut aggregate_triangles = 0usize;
    let mut uuid_counts = std::collections::BTreeMap::<&str, usize>::new();
    let mut reference_counts = std::collections::BTreeMap::<&str, usize>::new();
    for component in &board.engineering().components {
        if let Some(uuid) = component
            .source
            .source_uuid
            .as_deref()
            .filter(|uuid| !uuid.is_empty())
        {
            *uuid_counts.entry(uuid).or_default() += 1;
        }
        if let Some(reference) = component
            .reference
            .as_deref()
            .filter(|reference| !reference.is_empty())
        {
            *reference_counts.entry(reference).or_default() += 1;
        }
    }

    for model in &board.engineering().models {
        let component = model.source.component_ordinal.and_then(|ordinal| {
            board
                .engineering()
                .components
                .iter()
                .find(|item| item.source.ordinal == ordinal)
        });
        let Some(component) = component else {
            record_diagnostic(
                &mut result,
                diagnostic(model, None, ModelAssemblyStatus::MissingOwningComponent),
            );
            continue;
        };
        let unique_uuid = component
            .source
            .source_uuid
            .as_deref()
            .filter(|uuid| !uuid.is_empty())
            .is_some_and(|uuid| uuid_counts.get(uuid) == Some(&1));
        if !unique_uuid {
            record_diagnostic(
                &mut result,
                diagnostic(
                    model,
                    Some(component),
                    ModelAssemblyStatus::MissingUniqueIdentity,
                ),
            );
            continue;
        }
        let unique_reference = component
            .reference
            .as_deref()
            .filter(|reference| !reference.is_empty())
            .is_some_and(|reference| reference_counts.get(reference) == Some(&1));
        if !unique_reference {
            record_diagnostic(
                &mut result,
                diagnostic(
                    model,
                    Some(component),
                    ModelAssemblyStatus::MissingUniqueReference,
                ),
            );
            continue;
        }
        let (Some(planes), Some(origin)) = (planes, origin) else {
            record_diagnostic(
                &mut result,
                diagnostic(
                    model,
                    Some(component),
                    ModelAssemblyStatus::MissingStackupPlanes,
                ),
            );
            continue;
        };
        if !stackup_matches_shell {
            record_diagnostic(
                &mut result,
                diagnostic(
                    model,
                    Some(component),
                    ModelAssemblyStatus::StackupThicknessMismatch,
                ),
            );
            continue;
        }
        if model.legacy_at.is_some() {
            record_diagnostic(
                &mut result,
                diagnostic(
                    model,
                    Some(component),
                    ModelAssemblyStatus::LegacyAtUnsupported,
                ),
            );
            continue;
        }
        let placement = match placement(
            model,
            component,
            origin,
            planes.front_copper_surface_z_mm,
            planes.back_copper_surface_z_mm,
        ) {
            Ok(placement) => placement,
            Err(status) => {
                record_diagnostic(&mut result, diagnostic(model, Some(component), status));
                continue;
            }
        };
        let resolution = resolver.resolve(model);
        let Some(asset) = resolution.asset else {
            record_diagnostic(
                &mut result,
                diagnostic(
                    model,
                    Some(component),
                    ModelAssemblyStatus::Resolver(resolution.status),
                ),
            );
            continue;
        };
        if asset.format() != ModelAssetFormat::Wrl {
            record_diagnostic(
                &mut result,
                diagnostic(
                    model,
                    Some(component),
                    ModelAssemblyStatus::UnsupportedAssetFormat,
                ),
            );
            continue;
        }
        let Ok(parsed) = parse_vrml_mesh(
            asset.bytes(),
            ModelUnits::KiCadLegacyWrlDeciinch,
            MeshLimits::default(),
        ) else {
            record_diagnostic(
                &mut result,
                diagnostic(model, Some(component), ModelAssemblyStatus::MeshRejected),
            );
            continue;
        };
        let model_triangles = parsed.parts.iter().try_fold(0usize, |total, part| {
            total.checked_add(part.triangles.len())
        });
        let Some(model_triangles) = model_triangles else {
            record_diagnostic(
                &mut result,
                diagnostic(model, Some(component), ModelAssemblyStatus::MeshRejected),
            );
            continue;
        };
        let Ok(next_aggregate_triangles) =
            next_aggregate_geometry_triangles(aggregate_triangles, model_triangles)
        else {
            record_diagnostic(
                &mut result,
                diagnostic(
                    model,
                    Some(component),
                    ModelAssemblyStatus::AggregateGeometryBudgetExceeded,
                ),
            );
            continue;
        };
        let Ok(mesh) = place_model_mesh_in_glb_m(&parsed, placement) else {
            record_diagnostic(
                &mut result,
                diagnostic(
                    model,
                    Some(component),
                    ModelAssemblyStatus::PlacementRejected,
                ),
            );
            continue;
        };
        aggregate_triangles = next_aggregate_triangles;
        result.emitted.push(AssembledComponentMesh {
            component_source: component.source.clone(),
            component_reference: component.reference.clone(),
            component_uuid: component.source.source_uuid.as_deref().map(str::to_owned),
            model_source: model.source.clone(),
            source_asset_sha256: asset.sha256().to_owned(),
            mesh,
        });
    }
    result
}

fn next_aggregate_geometry_triangles(
    current: usize,
    additional: usize,
) -> Result<usize, ModelAssemblyStatus> {
    current
        .checked_add(additional)
        .filter(|total| *total <= MAX_AGGREGATE_ASSEMBLY_TRIANGLES)
        .ok_or(ModelAssemblyStatus::AggregateGeometryBudgetExceeded)
}

fn record_diagnostic(result: &mut ModelAssembly, item: ModelAssemblyDiagnostic) {
    result.diagnostics_total = result.diagnostics_total.saturating_add(1);
    if let Some(ordinal) = item.component_source.as_ref().map(|source| source.ordinal) {
        result.incomplete_component_ordinals.insert(ordinal);
    }
    if result.diagnostics.len() < MAX_RETAINED_MODEL_DIAGNOSTICS {
        result.diagnostics.push(item);
    } else {
        result.diagnostics_truncated = true;
    }
}

fn diagnostic(
    model: &ModelReference,
    component: Option<&Component>,
    status: ModelAssemblyStatus,
) -> ModelAssemblyDiagnostic {
    ModelAssemblyDiagnostic {
        model_source: model.source.clone(),
        component_source: component.map(|item| item.source.clone()),
        status,
    }
}

fn placement(
    model: &ModelReference,
    component: &Component,
    origin: [f64; 2],
    front_copper_surface_z_mm: f64,
    back_copper_surface_z_mm: f64,
) -> Result<KicadStepPlacement, ModelAssemblyStatus> {
    let pose = component.source_board_pose;
    let position = pose
        .position_mm
        .ok_or(ModelAssemblyStatus::MissingComponentPose)?;
    let rotation = pose
        .effective_rotation_deg()
        .ok_or(ModelAssemblyStatus::MissingComponentPose)?;
    let side = match component.layer.as_deref() {
        Some("F.Cu") => BoardSide::Front,
        Some("B.Cu") => BoardSide::Back,
        _ => return Err(ModelAssemblyStatus::UnsupportedComponentSide),
    };
    let point = |value: Option<crate::document::Point3>, default: [f64; 3]| {
        value
            .map(|point| [point.x, point.y, point.z])
            .unwrap_or(default)
    };
    Ok(KicadStepPlacement {
        board_position_mm: [position.x, position.y],
        board_origin_mm: origin,
        footprint_rotation_deg: rotation,
        side,
        front_copper_surface_z_mm,
        back_copper_surface_z_mm,
        model: ModelTransformMm {
            scale: point(model.scale, [1.0, 1.0, 1.0]),
            rotation_deg: point(model.rotation_deg, [0.0, 0.0, 0.0]),
            offset_mm: point(model.offset, [0.0, 0.0, 0.0]),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_kicad_board, model_resolver::ModelResolverConfig};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn aggregate_geometry_budget_checks_total_before_retaining_next_mesh() {
        assert_eq!(
            next_aggregate_geometry_triangles(MAX_AGGREGATE_ASSEMBLY_TRIANGLES - 1, 1),
            Ok(MAX_AGGREGATE_ASSEMBLY_TRIANGLES)
        );
        assert_eq!(
            next_aggregate_geometry_triangles(MAX_AGGREGATE_ASSEMBLY_TRIANGLES, 1),
            Err(ModelAssemblyStatus::AggregateGeometryBudgetExceeded)
        );
        assert_eq!(
            next_aggregate_geometry_triangles(usize::MAX, 1),
            Err(ModelAssemblyStatus::AggregateGeometryBudgetExceeded)
        );
    }

    #[test]
    fn bounded_diagnostic_retention_preserves_every_incomplete_component_ordinal() {
        let mut assembly = ModelAssembly::default();
        for ordinal in 0..(MAX_RETAINED_MODEL_DIAGNOSTICS + 7) {
            let source = |kind: &str, ordinal| SourceLocator {
                kind: kind.to_owned(),
                ordinal,
                component_ordinal: None,
                source_uuid: None,
                source_path: vec![ordinal],
            };
            record_diagnostic(
                &mut assembly,
                ModelAssemblyDiagnostic {
                    model_source: source("model", ordinal),
                    component_source: Some(source("footprint", ordinal)),
                    status: ModelAssemblyStatus::MeshRejected,
                },
            );
        }

        assert_eq!(assembly.diagnostics.len(), MAX_RETAINED_MODEL_DIAGNOSTICS);
        assert_eq!(
            assembly.diagnostics_total,
            MAX_RETAINED_MODEL_DIAGNOSTICS + 7
        );
        assert!(assembly.diagnostics_truncated);
        assert_eq!(
            assembly.incomplete_component_ordinals.len(),
            MAX_RETAINED_MODEL_DIAGNOSTICS + 7
        );
        assert!(!assembly.is_complete_for_source_models());
    }

    #[test]
    fn assembles_a_pinned_vrml_model_with_source_identity() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/open-source/kicad-resistor-models");
        let dir =
            std::env::temp_dir().join(format!("tokito-model-assembly-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let board_path = dir.join("board.kicad_pcb");
        fs::write(&board_path, "(kicad_pcb (version 20240108) (general (thickness 1.6)) (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal) (44 \"Edge.Cuts\" user)) (setup (stackup (layer \"F.Cu\" (type \"copper\") (thickness 0.035)) (layer \"dielectric 1\" (type \"core\") (thickness 1.53)) (layer \"B.Cu\" (type \"copper\") (thickness 0.035)))) (gr_rect (start 0 0) (end 10 10) (stroke (width 0.05) (type default)) (fill none) (layer \"Edge.Cuts\") (uuid \"edge\")) (footprint \"R:R\" (layer \"F.Cu\") (at 5 5 90) (uuid \"component\") (property \"Reference\" \"R1\") (model \"Resistor_SMD.3dshapes/R_0603_1608Metric.wrl\")))").unwrap();
        let board = load_kicad_board(&board_path).unwrap();
        let resolver = ModelResolver::new(ModelResolverConfig::new(root)).unwrap();
        let assembly = assemble_supported_vrml_models(&board, &resolver);
        assert!(assembly.diagnostics.is_empty());
        assert_eq!(assembly.emitted.len(), 1);
        let emitted = &assembly.emitted[0];
        assert_eq!(emitted.component_reference.as_deref(), Some("R1"));
        assert_eq!(emitted.component_uuid.as_deref(), Some("component"));
        assert_eq!(
            emitted.source_asset_sha256,
            "eadfec9e71c67b1dd3910c72b45d811a06f0ab5397d0609354b10e9a3a4c1fdd"
        );
        assert_eq!(emitted.mesh.parts.len(), 26);
        let export = board
            .export_glb_with_models(&resolver, crate::GlbExportOptions::default())
            .expect("supported component assembly GLB");
        assert!(!export.report.partial_export);
        assert!(!export.report.whole_board_digital_twin_complete);
        assert_eq!(export.report.component_models_emitted, 1);
        assert_eq!(export.report.model_references_emitted, 1);
        gltf::Gltf::from_slice(&export.bytes).expect("assembly GLB validates");
        let raw_json = gltf::binary::Glb::from_slice(&export.bytes)
            .expect("GLB framing")
            .json;
        let document: serde_json::Value = serde_json::from_slice(&raw_json).expect("GLB JSON");
        let nodes = document["nodes"].as_array().unwrap();
        let selector = nodes
            .iter()
            .find(|node| node["name"] == "ref:R1")
            .expect("App-facing renderable selector node");
        assert_eq!(selector["extras"]["tokito"]["componentUuid"], "component");
        assert_eq!(selector["extras"]["tokito"]["selectable"], true);
        assert_eq!(selector["mesh"], 1);
        assert_eq!(
            document["meshes"][1]["primitives"]
                .as_array()
                .unwrap()
                .len(),
            26
        );
        let _ = fs::remove_dir_all(dir);
    }
}
