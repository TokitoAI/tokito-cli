//! Explicit assignment assembly, separate from canonical design and qualification.
use crate::component_pose::{
    BoardSide, KicadStepPlacement, PlacedModelMesh, place_model_mesh_in_glb_m,
};
use crate::document::SourceLocator;
use crate::model_assignments::{MAX_ASSIGNMENTS, ValidatedModelAssignments};
use crate::model_mesh::{MeshLimits, parse_vrml_mesh};
use crate::{EngineError, PhysicalBoardDocument};
use serde_json::{Value, json};
use std::collections::BTreeMap;
const MAX_TRIANGLES: usize = 250_000;
const MAX_DETAILS: usize = 500;
#[derive(Clone, Debug)]
pub struct AssignedComponentMesh {
    pub component_source: SourceLocator,
    pub component_reference: Option<String>,
    pub component_uuid: Option<String>,
    pub manifest_entry_index: usize,
    pub source_asset_sha256: String,
    pub mesh: PlacedModelMesh,
}
#[derive(Clone, Debug)]
pub struct AssignedAssembly {
    pub emitted: Vec<AssignedComponentMesh>,
    pub report: Value,
}
/// Rechecks source binding; no canonical mutation, fallback boxes or inferred assets.
pub fn assemble_assigned_models(
    board: &PhysicalBoardDocument,
    assignments: &ValidatedModelAssignments,
) -> Result<AssignedAssembly, EngineError> {
    if board.source().sha256 != assignments.source_board_sha256() {
        return Err(EngineError::new("assignment assembly board hash mismatch"));
    }
    let components = &board.engineering().components;
    if components.len() > MAX_ASSIGNMENTS {
        return Err(EngineError::new(
            "assignment assembly component budget exceeded",
        ));
    }
    let rows: BTreeMap<_, _> = assignments
        .assignments()
        .iter()
        .enumerate()
        .map(|(index, row)| (row.assignment().component_ordinal(), (index, row)))
        .collect();
    if rows.len() != assignments.assignments().len() {
        return Err(EngineError::new("assignment assembly duplicate ordinals"));
    }
    for (_, row) in rows.values() {
        let a = row.assignment();
        let component = components
            .iter()
            .find(|c| c.source.ordinal == a.component_ordinal())
            .ok_or_else(|| EngineError::new("assignment assembly missing component"))?;
        if component.reference.as_deref() != a.reference()
            || component.source.source_uuid.as_deref() != a.component_uuid()
        {
            return Err(EngineError::new("assignment assembly identity mismatch"));
        }
    }
    let planes = board
        .engineering()
        .stackup
        .as_ref()
        .and_then(|s| s.outer_copper_planes_mm().ok());
    let stackup_matches = planes.is_some_and(|p| {
        board.thickness_mm().is_some_and(|t| {
            (p.front_copper_surface_z_mm - p.back_copper_surface_z_mm - t).abs() <= 0.000_001
        })
    });
    let origin = board
        .outline()
        .bounds
        .map(|b| [(b.min_x + b.max_x) * 0.5, (b.min_y + b.max_y) * 0.5]);
    let mut emitted = Vec::new();
    let mut details = Vec::new();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut triangle_count = 0usize;
    for component in components {
        let selected = rows.get(&component.source.ordinal).copied();
        let mut placement_snapshot = Value::Null;
        let status = if let Some((index, validated)) = selected {
            let a = validated.assignment();
            if matches!(a.population(), "not-fitted" | "no-part-to-order") {
                "population_claim_body_skipped".to_owned()
            } else if let Some(model) = a.model() {
                if !validated.supported_geometry() {
                    validated.status().to_owned()
                } else if let Some(asset) = validated.asset() {
                    let pose = component.source_board_pose;
                    match (
                        pose.position_mm,
                        pose.effective_rotation_deg(),
                        component.layer.as_deref(),
                        planes,
                        origin,
                    ) {
                        (Some(position), Some(rotation), Some(layer), Some(p), Some(origin))
                            if matches!(layer, "F.Cu" | "B.Cu") =>
                        {
                            if !stackup_matches {
                                "stackup_thickness_mismatch".to_owned()
                            } else {
                                let placement = KicadStepPlacement {
                                    board_position_mm: [position.x, position.y],
                                    board_origin_mm: origin,
                                    footprint_rotation_deg: rotation,
                                    side: if layer == "F.Cu" {
                                        BoardSide::Front
                                    } else {
                                        BoardSide::Back
                                    },
                                    front_copper_surface_z_mm: p.front_copper_surface_z_mm,
                                    back_copper_surface_z_mm: p.back_copper_surface_z_mm,
                                    model: model.transform(),
                                };
                                placement_snapshot = json!({"boardPositionMm":placement.board_position_mm,"boardOriginMm":placement.board_origin_mm,"footprintRotationDeg":rotation,"side":layer,"frontCopperSurfaceZMm":p.front_copper_surface_z_mm,"backCopperSurfaceZMm":p.back_copper_surface_z_mm,"scale":model.transform().scale,"rotationDeg":model.transform().rotation_deg,"offsetMm":model.transform().offset_mm});
                                match parse_vrml_mesh(
                                    asset.bytes(),
                                    model.units(),
                                    MeshLimits::default(),
                                ) {
                                    Ok(parsed) => {
                                        let triangles =
                                            parsed.parts.iter().try_fold(0usize, |n, p| {
                                                n.checked_add(p.triangles.len())
                                            });
                                        if let Some(total) = triangles
                                            .and_then(|n| triangle_count.checked_add(n))
                                            .filter(|n| *n <= MAX_TRIANGLES)
                                        {
                                            match place_model_mesh_in_glb_m(&parsed, placement) {
                                                Ok(mesh) => {
                                                    triangle_count = total;
                                                    emitted.push(AssignedComponentMesh {
                                                        component_source: component.source.clone(),
                                                        component_reference: component
                                                            .reference
                                                            .clone(),
                                                        component_uuid: component
                                                            .source
                                                            .source_uuid
                                                            .as_deref()
                                                            .map(str::to_owned),
                                                        manifest_entry_index: index,
                                                        source_asset_sha256: asset
                                                            .sha256()
                                                            .to_owned(),
                                                        mesh,
                                                    });
                                                    "emitted_claimed_candidate".to_owned()
                                                }
                                                Err(_) => "placement_rejected".to_owned(),
                                            }
                                        } else {
                                            "aggregate_geometry_budget_exceeded".to_owned()
                                        }
                                    }
                                    Err(_) => "geometry_rejected".to_owned(),
                                }
                            }
                        }
                        (None, _, _, _, _) | (_, None, _, _, _) => {
                            "missing_component_pose".to_owned()
                        }
                        (_, _, Some(layer), _, _) if !matches!(layer, "F.Cu" | "B.Cu") => {
                            "unsupported_component_side".to_owned()
                        }
                        (_, _, None, _, _) => "unsupported_component_side".to_owned(),
                        _ => "missing_stackup_or_outline".to_owned(),
                    }
                } else {
                    "asset_unresolved".to_owned()
                }
            } else {
                "model_unassigned".to_owned()
            }
        } else {
            "assignment_absent".to_owned()
        };
        *counts.entry(status.clone()).or_default() += 1;
        if details.len() < MAX_DETAILS {
            details.push(json!({"sourceComponentOrdinal":component.source.ordinal,"componentUuid":component.source.source_uuid.as_deref(),"reference":component.reference,"manifestEntryIndex":selected.map(|(i,_)|i),"populationClaim":selected.map(|(_,v)|v.assignment().population()),"evidenceClaim":selected.and_then(|(_,v)|v.assignment().evidence()),"status":status,"placementSnapshot":placement_snapshot,"populationVerified":false,"placementVerified":false,"physicalQualificationVerified":false}));
        }
    }
    let report = json!({"schema":"tokito.assigned-assembly.v1","sourceBoardSha256":board.source().sha256,"manifestSha256":assignments.manifest_sha256(),"componentCount":components.len(),"emittedComponentCount":emitted.len(),"aggregateTriangleCount":triangle_count,"classificationCounts":counts,"components":details,"componentDetailsTotal":components.len(),"componentDetailsTruncated":components.len()>MAX_DETAILS,"maxRetainedComponentDetails":MAX_DETAILS,"maxAggregateTriangles":MAX_TRIANGLES,"physicalQualificationVerified":false,"modelPhysicalMatchVerified":false,"placementVerified":false,"populationVerified":false,"boardRevisionMatchVerified":false,"wholeBoardDigitalTwinComplete":false});
    Ok(AssignedAssembly { emitted, report })
}
