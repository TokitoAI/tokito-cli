//! Pure-Rust, board-shell-only GLB export from validated Edge.Cuts topology.
//! Source: KiCad board files use mm and a saved Y-down frame. Output is metres,
//! right-handed, Y-up, with the board plane in XZ and its slab centered at Y=0.
//! GLB container alignment follows Khronos glTF 2.0:
//! https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/Specification.adoc
//! This exporter does not resolve component models or claim a complete digital twin.

use super::document::Point2;
use super::topology::{ContourRole, OutlineContour, OutlineShape};
use super::{EngineError, PhysicalBoardDocument};
use earcut::Earcut;
use gltf::binary::{Glb, Header};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};

const MM_TO_M: f64 = 0.001;
const MAX_THICKNESS_MM: f64 = 1_000.0;
const MAX_POLYGON_POINTS: usize = 50_000;
const MAX_GLTF_VERTICES: usize = 1_000_000;
const MAX_GLB_BYTES: usize = 128 * 1024 * 1024;
const MAX_COVERAGE_COMPONENTS: usize = 500;
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

#[derive(Clone, Debug, PartialEq)]
pub struct GlbCoverageReport {
    pub scope: &'static str,
    pub shell_geometry_complete: bool,
    pub whole_board_digital_twin_complete: bool,
    pub partial_export: bool,
    pub component_count: usize,
    pub component_models_emitted: usize,
    pub component_models_unresolved: usize,
    pub model_references: usize,
    pub model_references_emitted: usize,
    pub model_references_unresolved: usize,
    pub polygonized_vertices: usize,
    pub triangles: usize,
    pub exact_outline_area_mm2: f64,
    pub polygonized_outline_area_mm2: f64,
    pub triangulated_outline_area_mm2: f64,
    pub thickness_mm: f64,
    pub max_curve_chord_error_mm: f64,
    pub unresolved_components: Vec<UnresolvedComponentCoverage>,
    pub unresolved_components_total: usize,
    pub unresolved_components_truncated: bool,
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

impl GlbCoverageReport {
    /// Stable machine-readable receipt. It omits the local PCB file path but preserves bounded model paths read from the board.
    pub fn report_json(&self) -> String {
        let value = json!({
            "reportVersion": "1.0.0",
            "scope": self.scope,
            "shellGeometryComplete": self.shell_geometry_complete,
            "wholeBoardDigitalTwinComplete": self.whole_board_digital_twin_complete,
            "partialExport": self.partial_export,
            "coverage": {
                "componentCount": self.component_count,
                "componentModelsEmitted": self.component_models_emitted,
                "componentModelsUnresolved": self.component_models_unresolved,
                "modelReferences": self.model_references,
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
                })).collect::<Vec<_>>()
            },
            "mesh": {
                "polygonizedVertices": self.polygonized_vertices,
                "triangles": self.triangles,
                "exactOutlineAreaMm2": self.exact_outline_area_mm2,
                "polygonizedOutlineAreaMm2": self.polygonized_outline_area_mm2,
                "triangulatedOutlineAreaMm2": self.triangulated_outline_area_mm2,
                "thicknessMm": self.thickness_mm,
                "maxCurveChordErrorMm": self.max_curve_chord_error_mm
            }
        });
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
        unresolved_component_coverage(board);
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
        component_models_emitted: 0,
        component_models_unresolved: component_count,
        model_references,
        model_references_emitted: 0,
        model_references_unresolved: model_references,
        unresolved_components,
        unresolved_components_total: component_count,
        unresolved_components_truncated,
        polygonized_vertices: triangulation.points.len(),
        triangles: mesh.positions.len() / 9,
        exact_outline_area_mm2: triangulation.exact_area_mm2,
        polygonized_outline_area_mm2: triangulation.polygonized_area_mm2,
        triangulated_outline_area_mm2: triangulation.triangle_area_mm2,
        thickness_mm: thickness,
        max_curve_chord_error_mm: MAX_CHORD_ERROR_MM,
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

fn unresolved_component_coverage(
    board: &PhysicalBoardDocument,
) -> (Vec<UnresolvedComponentCoverage>, bool) {
    let engineering = board.engineering();
    let mut by_owner = BTreeMap::<usize, Vec<&super::document::ModelReference>>::new();
    for model in &engineering.models {
        if let Some(owner) = model.source.component_ordinal {
            by_owner.entry(owner).or_default().push(model);
        }
    }
    let mut items = Vec::with_capacity(engineering.components.len().min(MAX_COVERAGE_COMPONENTS));
    for component in engineering.components.iter().take(MAX_COVERAGE_COMPONENTS) {
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
            reason: "component_geometry_not_emitted_by_board_shell_only_export",
            no_model_reference_reason: (model_reference_count == 0)
                .then_some("no_source_model_reference_in_imported_data"),
        });
    }
    (
        items,
        engineering.components.len() > MAX_COVERAGE_COMPONENTS,
    )
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
                    let count = curve_segments(radius, sweep)?;
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
                    let count = curve_segments(*radius_mm, TAU)?;
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

fn curve_segments(radius: f64, sweep: f64) -> Result<usize, EngineError> {
    let error = MAX_CHORD_ERROR_MM - super::topology::JOIN_TOLERANCE_MM;
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
                let segments = curve_segments(radius, sweep)?;
                error += 0.5
                    * radius
                    * radius
                    * (sweep.abs()
                        - segments as f64 * stable_sweep_minus_sin(sweep / segments as f64))
                    .abs();
                perimeter += radius * sweep.abs();
            }
            OutlineShape::Circle { radius_mm, .. } => {
                let segments = curve_segments(*radius_mm, TAU)?;
                error += 0.5
                    * *radius_mm
                    * *radius_mm
                    * (TAU - segments as f64 * stable_sweep_minus_sin(TAU / segments as f64)).abs();
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
    if indices.is_empty() || indices.len() % 3 != 0 {
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
    if positions.is_empty() || positions.len() % 9 != 0 || positions.len() / 3 > MAX_GLTF_VERTICES {
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
    Ok(json!({
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
                "tessellation": { "maxCurveChordErrorMm": report.max_curve_chord_error_mm, "outlineJoinToleranceMm": board.outline.join_tolerance_mm },
                "coverage": {
                    "componentCount": report.component_count, "componentModelsEmitted": report.component_models_emitted,
                    "componentModelsUnresolved": report.component_models_unresolved,
                    "modelReferences": report.model_references, "modelReferencesEmitted": report.model_references_emitted,
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
    }))
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
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static NEXT: AtomicU64 = AtomicU64::new(0);

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
