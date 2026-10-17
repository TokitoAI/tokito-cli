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
use std::collections::BTreeSet;

/// Assembly-wide cap applied before any additional placed mesh is retained.
/// The flattened GLB positions/normals use 72 bytes per triangle, before JSON
/// and the board shell, so this also keeps the serialized geometry well below
/// the independent 128 MiB container ceiling.
const MAX_AGGREGATE_ASSEMBLY_TRIANGLES: usize = 250_000;
const MAX_RETAINED_MODEL_DIAGNOSTICS: usize = 2_048;
const MAX_COMPONENT_FILTER_MATCH_WORK: usize = 1_000_000;
const MAX_COMPONENT_REFERENCE_CHARS: usize = 4_096;

/// Component filters for local source-model to GLB assembly. Matching is
/// case-sensitive; `*` matches any sequence and `?` one Unicode scalar in
/// Tokito. KiCad comparisons currently verify ASCII references only. A bounded
/// aggregate work budget prevents pathological pattern/reference sets from
/// consuming unbounded time. This selects model references only; it is not a
/// STEP/board exporter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelAssemblyFilter {
    component_patterns: Option<Vec<String>>,
    exclude_dnp: bool,
    exclude_unspecified: bool,
}

/// Selection result retaining native DNP and footprint-type exclusions.
/// A component can belong to both exclusion sets; selected ordinals exclude
/// their union. Counts attributed to individual filters must not be summed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelAssemblySelection {
    pub selected_component_ordinals: BTreeSet<usize>,
    pub excluded_dnp_component_count: usize,
    pub excluded_dnp_component_ordinals: BTreeSet<usize>,
    pub excluded_unspecified_component_ordinals: BTreeSet<usize>,
}

impl ModelAssemblyFilter {
    pub fn exact_component_references(
        board: &PhysicalBoardDocument,
        references: &[String],
    ) -> Result<Self, crate::EngineError> {
        if references.is_empty() {
            return Err(crate::EngineError::new(
                "component filter requires at least one exact reference",
            ));
        }
        let mut selected = BTreeSet::new();
        for reference in references {
            if reference.is_empty() || reference.trim() != reference {
                return Err(crate::EngineError::new(
                    "component filter references must be nonempty and have no surrounding whitespace",
                ));
            }
            if reference
                .chars()
                .any(|character| matches!(character, '*' | '?'))
            {
                return Err(crate::EngineError::new(
                    "component-filter wildcards are not supported; provide exact references",
                ));
            }
            if !selected.insert(reference.clone()) {
                return Err(crate::EngineError::new(format!(
                    "duplicate component-filter reference: {reference}"
                )));
            }
        }
        for reference in &selected {
            let matches = board
                .engineering()
                .components
                .iter()
                .filter(|component| component.reference.as_deref() == Some(reference.as_str()))
                .count();
            match matches {
                0 => {
                    return Err(crate::EngineError::new(format!(
                        "unknown component-filter reference: {reference}"
                    )));
                }
                1 => {}
                _ => {
                    return Err(crate::EngineError::new(format!(
                        "ambiguous component-filter reference: {reference}"
                    )));
                }
            }
        }
        Self::component_patterns(&selected.into_iter().collect::<Vec<_>>())
    }

    /// Build a KiCad-style component-reference pattern list. An empty list
    /// means no filter, matching the observed empty 10.0.5 CLI behavior.
    /// Unknown patterns are retained and simply select no component.
    pub fn component_patterns(patterns: &[String]) -> Result<Self, crate::EngineError> {
        if patterns.is_empty() {
            return Ok(Self::default());
        }
        if patterns.len() > 1024 {
            return Err(crate::EngineError::new(
                "component filter exceeds 1024 patterns",
            ));
        }
        for pattern in patterns {
            if pattern.is_empty() || pattern.trim() != pattern {
                return Err(crate::EngineError::new(
                    "component filter patterns must be nonempty and have no surrounding whitespace",
                ));
            }
            if pattern.chars().count() > 1024 {
                return Err(crate::EngineError::new(
                    "component filter pattern exceeds 1024 characters",
                ));
            }
        }
        Ok(Self {
            component_patterns: Some(patterns.to_vec()),
            exclude_dnp: false,
            exclude_unspecified: false,
        })
    }

    /// Exclude components explicitly marked DNP in their source footprint.
    pub fn excluding_dnp(mut self) -> Self {
        self.exclude_dnp = true;
        self
    }

    pub fn excludes_dnp(&self) -> bool {
        self.exclude_dnp
    }

    /// Exclude footprints without an explicit native SMD or through-hole type.
    /// This does not infer type from pads, names, or model availability.
    pub fn excluding_unspecified(mut self) -> Self {
        self.exclude_unspecified = true;
        self
    }

    pub fn excludes_unspecified(&self) -> bool {
        self.exclude_unspecified
    }

    pub fn is_active(&self) -> bool {
        self.component_patterns.is_some() || self.exclude_dnp || self.exclude_unspecified
    }

    pub fn requested_patterns(&self) -> Option<Vec<String>> {
        self.component_patterns.clone()
    }

    /// Resolve the selected component ordinals once, applying one aggregate
    /// matching-work budget to the complete board instead of restarting it for
    /// every component/model query.
    pub fn selected_component_ordinals(
        &self,
        board: &PhysicalBoardDocument,
    ) -> Result<BTreeSet<usize>, crate::EngineError> {
        Ok(self.selection(board)?.selected_component_ordinals)
    }

    pub fn selection(
        &self,
        board: &PhysicalBoardDocument,
    ) -> Result<ModelAssemblySelection, crate::EngineError> {
        // The source document is authoritative for native DNP attributes.
        // Component extraction intentionally keeps a compact first-attribute
        // projection, so it cannot safely decide this filter on its own when
        // malformed input contains repeated `(attr ...)` nodes.
        let excluded_dnp_ordinals = if self.exclude_dnp {
            Some(board.dnp_component_ordinals()?)
        } else {
            None
        };
        let excluded_unspecified_ordinals = if self.exclude_unspecified {
            Some(board.unspecified_component_ordinals()?)
        } else {
            None
        };
        let components = &board.engineering().components;
        let compiled_patterns = self.component_patterns.as_ref().map(|patterns| {
            patterns
                .iter()
                .map(|pattern| pattern.chars().collect::<Vec<_>>())
                .collect::<Vec<_>>()
        });
        let mut remaining_work = MAX_COMPONENT_FILTER_MATCH_WORK;
        let mut result = ModelAssemblySelection::default();
        for component in components {
            let matched = if let Some(patterns) = &compiled_patterns {
                let Some(reference) = component.reference.as_deref() else {
                    continue;
                };
                let reference_char_count = reference
                    .chars()
                    .take(MAX_COMPONENT_REFERENCE_CHARS + 1)
                    .count();
                if reference_char_count > MAX_COMPONENT_REFERENCE_CHARS {
                    return Err(crate::EngineError::new(
                        "component reference exceeds the filter matching limit",
                    ));
                }
                consume_filter_work(&mut remaining_work, reference_char_count)?;
                let reference = reference.chars().collect::<Vec<_>>();
                let mut matched = false;
                for pattern in patterns {
                    if component_reference_matches(pattern, &reference, &mut remaining_work)? {
                        matched = true;
                        break;
                    }
                }
                matched
            } else {
                true
            };
            if !matched {
                continue;
            }
            let excluded_dnp = excluded_dnp_ordinals
                .as_ref()
                .is_some_and(|excluded| excluded.contains(&component.source.ordinal));
            let excluded_unspecified = excluded_unspecified_ordinals
                .as_ref()
                .is_some_and(|excluded| excluded.contains(&component.source.ordinal));
            if excluded_dnp {
                result.excluded_dnp_component_count += 1;
                result
                    .excluded_dnp_component_ordinals
                    .insert(component.source.ordinal);
            }
            if excluded_unspecified {
                result
                    .excluded_unspecified_component_ordinals
                    .insert(component.source.ordinal);
            }
            if !excluded_dnp && !excluded_unspecified {
                result
                    .selected_component_ordinals
                    .insert(component.source.ordinal);
            }
        }
        Ok(result)
    }
}

/// Bounded-space glob matching for KiCad's observed `*`/`?` syntax.
/// Pattern and reference matching is deliberately case-sensitive.
fn component_reference_matches(
    pattern: &[char],
    reference: &[char],
    remaining_work: &mut usize,
) -> Result<bool, crate::EngineError> {
    let (mut pattern_index, mut reference_index) = (0, 0);
    let mut last_star = None;
    let mut star_reference_index = 0;

    while reference_index < reference.len() {
        consume_filter_work(remaining_work, 1)?;
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == '?'
                || pattern[pattern_index] == reference[reference_index])
        {
            pattern_index += 1;
            reference_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            last_star = Some(pattern_index);
            pattern_index += 1;
            star_reference_index = reference_index;
        } else if let Some(star_index) = last_star {
            star_reference_index += 1;
            reference_index = star_reference_index;
            pattern_index = star_index + 1;
        } else {
            return Ok(false);
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
        consume_filter_work(remaining_work, 1)?;
        pattern_index += 1;
    }
    Ok(pattern_index == pattern.len())
}

fn consume_filter_work(
    remaining_work: &mut usize,
    amount: usize,
) -> Result<(), crate::EngineError> {
    *remaining_work = remaining_work
        .checked_sub(amount)
        .ok_or_else(|| crate::EngineError::new("component filter matching work budget exceeded"))?;
    Ok(())
}

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
    GlbPrecisionRejected,
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
            Self::GlbPrecisionRejected => "glb_precision_rejected",
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
    let selected = board
        .engineering()
        .components
        .iter()
        .map(|component| component.source.ordinal)
        .collect();
    assemble_supported_vrml_models_for_components(board, resolver, &selected)
}

/// Resolve, parse and place only components admitted by a reference-pattern filter.
/// Filtered model paths are never resolved or read from disk.
pub fn assemble_supported_vrml_models_with_filter(
    board: &PhysicalBoardDocument,
    resolver: &ModelResolver,
    filter: &ModelAssemblyFilter,
) -> Result<ModelAssembly, crate::EngineError> {
    let selected = filter.selected_component_ordinals(board)?;
    Ok(assemble_supported_vrml_models_for_components(
        board, resolver, &selected,
    ))
}

pub(crate) fn assemble_supported_vrml_models_for_components(
    board: &PhysicalBoardDocument,
    resolver: &ModelResolver,
    selected_component_ordinals: &BTreeSet<usize>,
) -> ModelAssembly {
    let planes = board
        .engineering()
        .stackup
        .as_ref()
        .and_then(|stackup| stackup.outer_copper_planes_mm().ok());
    let stackup_matches_shell = board.engineering().stackup.as_ref().is_some_and(|stackup| {
        board
            .thickness_mm()
            .is_some_and(|t| stackup.matches_nominal_thickness_mm(t))
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
        if !selected_component_ordinals.contains(&component.source.ordinal) {
            continue;
        }
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
        let resolution = resolver.resolve_board_model(board, model);
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
        if crate::glb::validate_component_mesh_for_glb(&mesh).is_err() {
            record_diagnostic(
                &mut result,
                diagnostic(
                    model,
                    Some(component),
                    ModelAssemblyStatus::GlbPrecisionRejected,
                ),
            );
            continue;
        }
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
    fn reference_globs_are_case_sensitive_and_support_star_and_single_character() {
        for (pattern, reference, expected) in [
            ("U*", "U3", true),
            ("U?", "U3", true),
            ("U??", "U3", false),
            ("*3", "U3", true),
            ("U*3", "U3", true),
            ("u*", "U3", false),
            ("R?", "R", false),
            ("R*", "R", true),
        ] {
            let pattern_chars = pattern.chars().collect::<Vec<_>>();
            let reference_chars = reference.chars().collect::<Vec<_>>();
            let mut remaining_work = MAX_COMPONENT_FILTER_MATCH_WORK;
            assert_eq!(
                component_reference_matches(&pattern_chars, &reference_chars, &mut remaining_work,)
                    .unwrap(),
                expected,
                "pattern {pattern:?} and reference {reference:?}"
            );
        }
    }

    #[test]
    fn adversarial_reference_glob_stops_at_the_aggregate_work_budget() {
        let pattern = format!("*{}b", "a".repeat(600));
        let pattern = pattern.chars().collect::<Vec<_>>();
        let reference = "a".repeat(MAX_COMPONENT_REFERENCE_CHARS);
        let reference = reference.chars().collect::<Vec<_>>();
        let mut remaining_work = MAX_COMPONENT_FILTER_MATCH_WORK;
        let error = component_reference_matches(&pattern, &reference, &mut remaining_work)
            .unwrap_err()
            .to_string();
        assert!(error.contains("matching work budget exceeded"));
    }

    #[test]
    fn reference_filter_does_not_claim_native_unicode_parity() {
        // Tokito currently treats `?` as one Unicode scalar. KiCad 10.0.5
        // comparison fixtures cover ASCII reference designators only.
        let pattern = "??".chars().collect::<Vec<_>>();
        let reference = "é1".chars().collect::<Vec<_>>();
        let mut remaining_work = MAX_COMPONENT_FILTER_MATCH_WORK;
        assert!(component_reference_matches(&pattern, &reference, &mut remaining_work).unwrap());
    }

    #[test]
    fn empty_filter_means_no_filter_and_unknown_patterns_remain_valid() {
        assert!(
            ModelAssemblyFilter::component_patterns(&[])
                .unwrap()
                .requested_patterns()
                .is_none()
        );
        assert_eq!(
            ModelAssemblyFilter::component_patterns(&["R404".to_owned()])
                .unwrap()
                .requested_patterns(),
            Some(vec!["R404".to_owned()])
        );
    }

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
        let masked_text = fs::read_to_string(&board_path).unwrap()
            .replace("(stackup (layer", "(stackup (layer \"F.Mask\" (type \"Top Solder Mask\") (thickness 0.01)) (layer")
            .replace("(thickness 1.53)", "(thickness 1.51)")
            .replace("(thickness 0.035))))", "(thickness 0.035)) (layer \"B.Mask\" (type \"Bottom Solder Mask\") (thickness 0.01))))");
        fs::write(&board_path, &masked_text).unwrap();
        let masked = load_kicad_board(&board_path).unwrap();
        let mask_assembly = assemble_supported_vrml_models(&masked, &resolver);
        assert!(mask_assembly.diagnostics.is_empty());
        assert_eq!(mask_assembly.emitted.len(), 1);
        let minimum_y = mask_assembly.emitted[0]
            .mesh
            .parts
            .iter()
            .flat_map(|p| &p.positions_m)
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min);
        // Independent two-layer arithmetic: 1.51/2 + .035 copper + .05 clearance.
        assert!((minimum_y - 0.000_840).abs() < 1e-12);
        fs::write(
            &board_path,
            masked_text.replace("(general (thickness 1.6))", "(general (thickness 1.58))"),
        )
        .unwrap();
        let wrong_total = load_kicad_board(&board_path).unwrap();
        let refused = assemble_supported_vrml_models(&wrong_total, &resolver);
        assert!(refused.emitted.is_empty());
        assert_eq!(
            refused.diagnostics[0].status,
            ModelAssemblyStatus::StackupThicknessMismatch
        );
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
        let primitives = document["meshes"][1]["primitives"].as_array().unwrap();
        let mut colours = std::collections::BTreeSet::new();
        let mut material_ids = std::collections::BTreeSet::new();
        for primitive in primitives {
            let material_id = primitive["material"].as_u64().unwrap() as usize;
            material_ids.insert(material_id);
            colours.insert(
                document["materials"][material_id]["pbrMetallicRoughness"]["baseColorFactor"]
                    .to_string(),
            );
        }
        // Literal values independently read from the pinned resistor asset's
        // PIN-01 and RES-SMD-01 definitions; do not accept the old generic gold.
        assert!(colours.contains("[0.824,0.82,0.781,1.0]"));
        assert!(colours.contains("[0.082,0.086,0.094,1.0]"));
        assert_eq!(material_ids.len(), 3);
        assert_eq!(document["materials"].as_array().unwrap().len(), 5);
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
