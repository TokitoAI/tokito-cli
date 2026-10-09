//! Source-semantic PCB records. Coordinates are millimetres in KiCad's saved
//! board frame; this module does not certify topology or synthesize 3D geometry.
//! Missing/malformed coordinates remain absent, including individual polygon
//! points. Unsupported objects remain addressable by their source locator.

use super::Sexp;
use std::sync::Arc;

const MAX_DIAGNOSTICS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Saved source pose. An omitted angle means zero in the KiCad format; an
/// explicitly malformed angle is absent with `rotation_was_explicit == true`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourcePose {
    pub position_mm: Option<Point2>,
    pub rotation_deg: Option<f64>,
    pub rotation_was_explicit: bool,
}

impl SourcePose {
    pub fn effective_rotation_deg(self) -> Option<f64> {
        self.rotation_deg
            .or((!self.rotation_was_explicit).then_some(0.0))
    }

    pub fn local_to_board(self, point: Point2) -> Option<Point2> {
        source_local_to_board(point, self.position_mm?, self.effective_rotation_deg()?)
    }
}

/// Ordinals are zero-based positions in the containing source list, excluding
/// its head atom. A UUID is preserved as text, never manufactured from an index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocator {
    pub kind: String,
    pub ordinal: usize,
    pub component_ordinal: Option<usize>,
    /// Shared identity text prevents repeated diagnostic locators from copying
    /// an arbitrarily long source identity.
    pub source_uuid: Option<Arc<str>>,
    /// Nested source-list ordinals distinguish anonymous polygons and points.
    pub source_path: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticCode {
    MissingField,
    InvalidNumber,
    InvalidInteger,
    InvalidPoint,
    InvalidPolygon,
    InvalidDimension,
    UnsupportedShape,
    UnsupportedGraphic,
    UnsupportedPrimitive,
    MissingIdentity,
    UnresolvedNet,
    ConflictingNet,
    DuplicateIdentity,
    DuplicateProperty,
    ConflictingProperty,
    DiagnosticsTruncated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub source: SourceLocator,
    pub field: String,
    pub message: String,
    pub raw_value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub source: SourceLocator,
    pub id: Option<i64>,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub display_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Net {
    pub source: SourceLocator,
    pub id: Option<i64>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetReference {
    pub id: Option<i64>,
    /// Optional name saved on this specific source entity.
    pub name: Option<String>,
    /// Name resolved from a unique source net-table ID. Conflicts remain absent.
    pub resolved_name: Option<Arc<str>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub source: SourceLocator,
    pub name: Option<String>,
    pub value: Option<String>,
    pub source_pose: SourcePose,
    pub layer: Option<String>,
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    pub source: SourceLocator,
    pub reference: Option<String>,
    pub value: Option<String>,
    pub library_id: Option<String>,
    pub layer: Option<String>,
    pub source_board_pose: SourcePose,
    pub properties: Vec<Property>,
    pub attributes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Drill {
    /// `circle`, `oval`, or an unsupported source atom.
    pub shape: String,
    pub diameter_mm: Option<f64>,
    pub size_mm: Option<Point2>,
    pub offset_mm: Option<Point2>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pad {
    pub source: SourceLocator,
    pub number: Option<String>,
    pub pad_type: Option<String>,
    pub shape: Option<String>,
    pub source_local_pose: SourcePose,
    pub board_position_mm: Option<Point2>,
    /// Pad angles in saved KiCad files are absolute, not relative to the parent.
    pub source_absolute_angle_deg: Option<f64>,
    pub size_mm: Option<Point2>,
    pub drill: Option<Drill>,
    pub layers: Vec<String>,
    pub net: Option<NetReference>,
    pub roundrect_ratio: Option<f64>,
    pub chamfer_ratio: Option<f64>,
    pub custom_primitive_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackSegment {
    pub source: SourceLocator,
    pub start_mm: Option<Point2>,
    pub end_mm: Option<Point2>,
    pub width_mm: Option<f64>,
    pub layer: Option<String>,
    pub net: Option<NetReference>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackArc {
    pub source: SourceLocator,
    pub start_mm: Option<Point2>,
    pub mid_mm: Option<Point2>,
    pub end_mm: Option<Point2>,
    pub width_mm: Option<f64>,
    pub layer: Option<String>,
    pub net: Option<NetReference>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Via {
    pub source: SourceLocator,
    pub source_pose: SourcePose,
    pub size_mm: Option<f64>,
    pub drill_mm: Option<f64>,
    pub via_type: Option<String>,
    pub layers: Vec<String>,
    pub net: Option<NetReference>,
}

/// Invalid entries remain `None`; consumers must not connect across them.
#[derive(Clone, Debug, PartialEq)]
pub struct Polygon {
    pub source: SourceLocator,
    pub points_mm: Vec<Option<Point2>>,
    pub layer: Option<String>,
    pub is_complete: bool,
    pub is_island: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub source: SourceLocator,
    pub frame: CoordinateFrame,
    pub layers: Vec<String>,
    pub net: Option<NetReference>,
    pub name: Option<String>,
    pub boundary_polygons: Vec<Polygon>,
    /// Stored filled polygons are a source cache, not newly solved copper.
    pub cached_fill_polygons: Vec<Polygon>,
    pub has_fill_configuration: bool,
    pub is_keepout: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CoordinateFrame {
    Board,
    ComponentLocal { component_ordinal: usize },
}

/// Source graphic fields remain in their saved frame. Text angles are absolute.
/// Legacy arc angle fields are retained without fabricating a midpoint.
#[derive(Clone, Debug, PartialEq)]
pub struct Graphic {
    pub source: SourceLocator,
    pub frame: CoordinateFrame,
    pub kind: String,
    pub layer: Option<String>,
    pub hidden: bool,
    pub start_mm: Option<Point2>,
    pub mid_mm: Option<Point2>,
    pub end_mm: Option<Point2>,
    pub center_mm: Option<Point2>,
    pub source_pose: SourcePose,
    pub source_angle_deg: Option<f64>,
    pub stroke_width_mm: Option<f64>,
    pub fill: Option<String>,
    pub text: Option<String>,
    pub text_role: Option<String>,
    pub font_size_mm: Option<Point2>,
    pub polygon: Option<Polygon>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelReference {
    pub source: SourceLocator,
    pub path: Option<String>,
    pub offset: Option<Point3>,
    /// Legacy `(at (xyz ...))` is retained separately; its units may differ
    /// from modern `(offset (xyz ...))`. No conversion or resolution is done.
    pub legacy_at: Option<Point3>,
    pub rotation_deg: Option<Point3>,
    pub scale: Option<Point3>,
    pub hidden: bool,
}

/// Typed source inventory shared by Rust consumers. Geometry remains explicitly
/// partial: this record is not a DRC, a contour solver or a model resolver.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EngineeringData {
    pub layers: Vec<Layer>,
    pub nets: Vec<Net>,
    pub components: Vec<Component>,
    pub pads: Vec<Pad>,
    pub track_segments: Vec<TrackSegment>,
    pub track_arcs: Vec<TrackArc>,
    pub vias: Vec<Via>,
    pub zones: Vec<Zone>,
    pub graphics: Vec<Graphic>,
    pub models: Vec<ModelReference>,
    pub diagnostics: Vec<Diagnostic>,
    pub diagnostics_truncated: bool,
}

/// KiCad's serialized footprint transform; back-side children are already
/// flipped in the saved file. There is deliberately no second back-side flip.
pub(crate) fn rotate_translate_xy(
    x: f64,
    y: f64,
    origin_x: f64,
    origin_y: f64,
    angle_deg: f64,
) -> (f64, f64) {
    let angle = angle_deg.to_radians();
    (
        origin_x + x * angle.cos() + y * angle.sin(),
        origin_y - x * angle.sin() + y * angle.cos(),
    )
}

/// Transform a valid source-local point into board millimetres. Overflow and
/// non-finite input produce `None`; no replacement coordinates are invented.
pub fn source_local_to_board(point: Point2, origin: Point2, angle_deg: f64) -> Option<Point2> {
    if ![point.x, point.y, origin.x, origin.y, angle_deg]
        .iter()
        .all(|value| value.is_finite())
    {
        return None;
    }
    let (x, y) = rotate_translate_xy(point.x, point.y, origin.x, origin.y, angle_deg);
    (x.is_finite() && y.is_finite()).then_some(Point2 { x, y })
}

fn text(node: &Sexp, key: &str, index: usize) -> Option<String> {
    node.child(key)?.value_at(index).map(str::to_owned)
}

fn source_locator(node: &Sexp, ordinal: usize, component: Option<usize>) -> SourceLocator {
    SourceLocator {
        kind: node.head().unwrap_or("invalid-list").to_owned(),
        ordinal,
        component_ordinal: component,
        source_uuid: node
            .child("uuid")
            .or_else(|| node.child("tstamp"))
            .and_then(|identity| identity.value_at(1))
            .map(Arc::from),
        source_path: component
            .into_iter()
            .chain(std::iter::once(ordinal))
            .collect(),
    }
}

fn child_locator(node: &Sexp, ordinal: usize, parent: &SourceLocator) -> SourceLocator {
    let mut locator = source_locator(node, ordinal, parent.component_ordinal);
    locator.source_path = parent
        .source_path
        .iter()
        .copied()
        .chain(std::iter::once(ordinal))
        .collect();
    locator
}

fn children(node: &Sexp) -> impl Iterator<Item = (usize, &Sexp)> {
    node.items()
        .into_iter()
        .flat_map(|items| items.iter().skip(1).enumerate())
}

fn layer_names(node: &Sexp) -> Vec<String> {
    if let Some(layer_list) = node.child("layers") {
        children(layer_list)
            .filter_map(|(_, value)| value.atom().map(str::to_owned))
            .collect()
    } else {
        text(node, "layer", 1).into_iter().collect()
    }
}

fn hidden(node: &Sexp) -> bool {
    node.child("hide")
        .is_some_and(|value| value.value_at(1) == Some("yes") || value.value_at(1).is_none())
        || children(node)
            .skip(if matches!(node.head(), Some("fp_text" | "property")) {
                2
            } else {
                1
            })
            .any(|(_, value)| value.atom() == Some("hide"))
        || node.child("effects").is_some_and(|effects| {
            children(effects).any(|(_, value)| value.atom() == Some("hide"))
                || effects.child("hide").is_some_and(|value| {
                    value.value_at(1) == Some("yes") || value.value_at(1).is_none()
                })
        })
}

struct Importer {
    data: EngineeringData,
}

impl Importer {
    fn diagnostic(
        &mut self,
        source: &SourceLocator,
        code: DiagnosticCode,
        field: &str,
        message: &str,
        raw: Option<&str>,
    ) {
        if self.data.diagnostics.len() < MAX_DIAGNOSTICS - 1 {
            self.data.diagnostics.push(Diagnostic {
                code,
                source: source.clone(),
                field: field.to_owned(),
                message: message.to_owned(),
                raw_value: raw.map(str::to_owned),
            });
        } else if !self.data.diagnostics_truncated {
            self.data.diagnostics_truncated = true;
            self.data.diagnostics.push(Diagnostic {
                code: DiagnosticCode::DiagnosticsTruncated,
                source: source.clone(),
                field: "diagnostics".to_owned(),
                message: "Further canonical source diagnostics omitted at the 500-record limit"
                    .to_owned(),
                raw_value: None,
            });
        }
    }

    fn number(
        &mut self,
        node: Option<&Sexp>,
        index: usize,
        source: &SourceLocator,
        field: &str,
        required: bool,
    ) -> Option<f64> {
        let raw = node.and_then(|value| value.value_at(index));
        if raw.is_none() && !required {
            return None;
        }
        let result = raw
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite());
        if result.is_none() {
            self.diagnostic(
                source,
                if raw.is_some() {
                    DiagnosticCode::InvalidNumber
                } else {
                    DiagnosticCode::MissingField
                },
                field,
                "Expected a finite source number; value retained as absent",
                raw,
            );
        }
        result
    }

    fn dimension(
        &mut self,
        node: &Sexp,
        key: &str,
        source: &SourceLocator,
        required: bool,
    ) -> Option<f64> {
        let value = self.number(node.child(key), 1, source, key, required)?;
        if value <= 0.0 {
            self.diagnostic(
                source,
                DiagnosticCode::InvalidDimension,
                key,
                "Source dimension must be positive; raw finite value retained",
                node.child(key).and_then(|value| value.value_at(1)),
            );
        }
        Some(value)
    }

    fn point_node(
        &mut self,
        node: Option<&Sexp>,
        source: &SourceLocator,
        field: &str,
        required: bool,
    ) -> Option<Point2> {
        if node.is_none() && !required {
            return None;
        }
        let x = self.number(node, 1, source, field, true);
        let y = self.number(node, 2, source, field, true);
        match (x, y) {
            (Some(x), Some(y)) => Some(Point2 { x, y }),
            _ => None,
        }
    }

    fn point(
        &mut self,
        node: &Sexp,
        key: &str,
        source: &SourceLocator,
        required: bool,
    ) -> Option<Point2> {
        self.point_node(node.child(key), source, key, required)
    }

    fn pose(&mut self, node: &Sexp, source: &SourceLocator, required: bool) -> SourcePose {
        let at = node.child("at");
        let rotation_was_explicit =
            at.is_some_and(|value| value.items().is_some_and(|items| items.len() > 3));
        SourcePose {
            position_mm: self.point_node(at, source, "at", required),
            rotation_deg: self.number(at, 3, source, "at.rotation", rotation_was_explicit),
            rotation_was_explicit,
        }
    }

    fn integer(
        &mut self,
        raw: Option<&str>,
        source: &SourceLocator,
        field: &str,
        required: bool,
    ) -> Option<i64> {
        if raw.is_none() && !required {
            return None;
        }
        let parsed = raw.and_then(|value| value.parse::<i64>().ok());
        if parsed.is_none() {
            self.diagnostic(
                source,
                DiagnosticCode::InvalidInteger,
                field,
                "Expected an integer source identifier",
                raw,
            );
        }
        parsed
    }

    fn net_reference(&mut self, node: &Sexp, source: &SourceLocator) -> Option<NetReference> {
        let net = node.child("net")?;
        Some(NetReference {
            id: self.integer(net.value_at(1), source, "net.id", true),
            name: net
                .value_at(2)
                .map(str::to_owned)
                .or_else(|| text(node, "net_name", 1)),
            resolved_name: None,
        })
    }

    fn polygon(&mut self, node: &Sexp, source: SourceLocator) -> Polygon {
        let mut points_mm = Vec::new();
        if let Some((points_ordinal, points)) =
            children(node).find(|(_, child)| child.head() == Some("pts"))
        {
            for (ordinal, point) in children(points) {
                let point_source = SourceLocator {
                    kind: format!("{}.pts", source.kind),
                    ordinal,
                    component_ordinal: source.component_ordinal,
                    source_uuid: source.source_uuid.clone(),
                    source_path: source
                        .source_path
                        .iter()
                        .copied()
                        .chain([points_ordinal, ordinal])
                        .collect(),
                };
                if point.head() == Some("xy") {
                    points_mm.push(self.point_node(Some(point), &point_source, "xy", true));
                } else {
                    points_mm.push(None);
                    self.diagnostic(
                        &point_source,
                        DiagnosticCode::InvalidPoint,
                        "pts",
                        "Unsupported polygon point syntax retained as an absent point slot",
                        point.head().or_else(|| point.atom()),
                    );
                }
            }
        }
        let is_complete = points_mm.len() >= 3 && points_mm.iter().all(Option::is_some);
        if !is_complete {
            self.diagnostic(
                &source,
                DiagnosticCode::InvalidPolygon,
                "pts",
                "Polygon needs at least three finite points; consumers must not infer missing edges",
                None,
            );
        }
        Polygon {
            source,
            points_mm,
            layer: text(node, "layer", 1),
            is_complete,
            is_island: children(node).any(|(_, child)| child.atom() == Some("island")),
        }
    }

    fn drill(&mut self, node: &Sexp, source: &SourceLocator) -> Option<Drill> {
        let drill = node.child("drill")?;
        let raw_shape = drill.value_at(1);
        let oval = raw_shape == Some("oval");
        let numeric_round = raw_shape.is_some_and(|value| value.parse::<f64>().is_ok());
        let shape = if oval {
            "oval".to_owned()
        } else if numeric_round {
            "circle".to_owned()
        } else {
            raw_shape.unwrap_or("missing").to_owned()
        };
        let diameter_mm = if numeric_round {
            self.number(Some(drill), 1, source, "drill", true)
        } else {
            None
        };
        let size_mm = if oval {
            let x = self.number(Some(drill), 2, source, "drill.oval.x", true);
            let y = self.number(Some(drill), 3, source, "drill.oval.y", true);
            x.zip(y).map(|(x, y)| Point2 { x, y })
        } else {
            None
        };
        if !oval && !numeric_round {
            self.diagnostic(
                source,
                DiagnosticCode::UnsupportedShape,
                "drill",
                "Unsupported drill shape retained without synthesizing geometry",
                raw_shape,
            );
        }
        if diameter_mm.is_some_and(|value| value <= 0.0)
            || size_mm.is_some_and(|value| value.x <= 0.0 || value.y <= 0.0)
        {
            self.diagnostic(
                source,
                DiagnosticCode::InvalidDimension,
                "drill",
                "Nonpositive drill size retained as source metadata",
                raw_shape,
            );
        }
        Some(Drill {
            shape,
            diameter_mm,
            size_mm,
            offset_mm: self.point(drill, "offset", source, false),
        })
    }

    fn pad(&mut self, node: &Sexp, source: SourceLocator, parent: SourcePose) {
        let source_local_pose = self.pose(node, &source, true);
        let board_position_mm = source_local_pose
            .position_mm
            .and_then(|point| parent.local_to_board(point));
        let shape = node.value_at(3).map(str::to_owned);
        if !matches!(
            shape.as_deref(),
            Some("circle" | "oval" | "rect" | "roundrect")
        ) {
            self.diagnostic(
                &source,
                DiagnosticCode::UnsupportedShape,
                "pad.shape",
                "Pad shape retained; current SVG projection does not fully support this shape",
                node.value_at(3),
            );
        }
        let size_mm = self.point(node, "size", &source, true);
        if size_mm.is_some_and(|size| size.x <= 0.0 || size.y <= 0.0) {
            self.diagnostic(
                &source,
                DiagnosticCode::InvalidDimension,
                "size",
                "Nonpositive pad dimensions retained as source metadata",
                None,
            );
        }
        let custom_primitive_count = node
            .child("primitives")
            .map(|primitives| children(primitives).count())
            .unwrap_or(0);
        if custom_primitive_count > 0 {
            self.diagnostic(
                &source,
                DiagnosticCode::UnsupportedPrimitive,
                "primitives",
                "Custom pad primitives remain in raw source; count retained without evaluating shape",
                None,
            );
        }
        let pad = Pad {
            number: node.value_at(1).map(str::to_owned),
            pad_type: node.value_at(2).map(str::to_owned),
            shape,
            source_local_pose,
            board_position_mm,
            source_absolute_angle_deg: source_local_pose.rotation_deg,
            size_mm,
            drill: self.drill(node, &source),
            layers: layer_names(node),
            net: self.net_reference(node, &source),
            roundrect_ratio: self.number(
                node.child("roundrect_rratio"),
                1,
                &source,
                "roundrect_rratio",
                false,
            ),
            chamfer_ratio: self.number(
                node.child("chamfer_ratio"),
                1,
                &source,
                "chamfer_ratio",
                false,
            ),
            custom_primitive_count,
            source,
        };
        self.data.pads.push(pad);
    }

    fn graphic(&mut self, node: &Sexp, source: SourceLocator, frame: CoordinateFrame) {
        let kind = node.head().unwrap_or("unknown");
        let suffix = kind
            .strip_prefix("gr_")
            .or_else(|| kind.strip_prefix("fp_"))
            .unwrap_or(kind);
        if !matches!(
            suffix,
            "line" | "arc" | "circle" | "rect" | "poly" | "text" | "text_box"
        ) {
            self.diagnostic(
                &source,
                DiagnosticCode::UnsupportedGraphic,
                "graphic",
                "Unsupported graphic retained as source metadata",
                Some(kind),
            );
        }
        if suffix == "text_box" {
            self.diagnostic(
                &source,
                DiagnosticCode::UnsupportedGraphic,
                "text_box",
                "Text-box layout is not evaluated; source fields retained",
                None,
            );
        }
        let text_index = if kind == "fp_text" { 2 } else { 1 };
        let is_text = matches!(suffix, "text" | "text_box");
        let stroke_width_mm = if let Some(stroke) = node.child("stroke") {
            self.number(stroke.child("width"), 1, &source, "stroke.width", false)
        } else {
            self.number(node.child("width"), 1, &source, "width", false)
        };
        let font = node
            .child("effects")
            .and_then(|effects| effects.child("font"));
        let requires_start = matches!(suffix, "line" | "arc" | "rect" | "text_box");
        let requires_end = matches!(suffix, "line" | "arc" | "rect" | "circle" | "text_box");
        let legacy_arc = suffix == "arc" && node.child("angle").is_some();
        if legacy_arc {
            self.diagnostic(
                &source,
                DiagnosticCode::UnsupportedGraphic,
                "angle",
                "Legacy start/end/angle arc preserved without fabricating a midpoint",
                None,
            );
        }
        let graphic = Graphic {
            kind: kind.to_owned(),
            frame,
            layer: text(node, "layer", 1),
            hidden: hidden(node),
            start_mm: self.point(node, "start", &source, requires_start),
            mid_mm: self.point(node, "mid", &source, suffix == "arc" && !legacy_arc),
            end_mm: self.point(node, "end", &source, requires_end),
            center_mm: self.point(node, "center", &source, suffix == "circle"),
            source_pose: self.pose(node, &source, suffix == "text"),
            source_angle_deg: self.number(node.child("angle"), 1, &source, "angle", false),
            stroke_width_mm,
            fill: text(node, "fill", 1),
            text: if is_text {
                node.value_at(text_index).map(str::to_owned)
            } else {
                None
            },
            text_role: if kind == "fp_text" {
                node.value_at(1).map(str::to_owned)
            } else {
                None
            },
            font_size_mm: font.and_then(|font| self.point(font, "size", &source, false)),
            polygon: if suffix == "poly" || node.child("pts").is_some() {
                Some(self.polygon(node, source.clone()))
            } else {
                None
            },
            source,
        };
        self.data.graphics.push(graphic);
    }

    fn xyz(&mut self, node: &Sexp, key: &str, source: &SourceLocator) -> Option<Point3> {
        let vector = node.child(key)?;
        let xyz = vector.child("xyz");
        let x = self.number(xyz, 1, source, key, true);
        let y = self.number(xyz, 2, source, key, true);
        let z = self.number(xyz, 3, source, key, true);
        match (x, y, z) {
            (Some(x), Some(y), Some(z)) => Some(Point3 { x, y, z }),
            _ => None,
        }
    }

    fn model(&mut self, node: &Sexp, source: SourceLocator) {
        let model = ModelReference {
            path: node.value_at(1).map(str::to_owned),
            offset: self.xyz(node, "offset", &source),
            legacy_at: self.xyz(node, "at", &source),
            rotation_deg: self.xyz(node, "rotate", &source),
            scale: self.xyz(node, "scale", &source),
            hidden: hidden(node),
            source,
        };
        self.data.models.push(model);
    }

    fn component(&mut self, node: &Sexp, source: SourceLocator) {
        if source.source_uuid.as_deref().is_none_or(str::is_empty) {
            self.diagnostic(
                &source,
                DiagnosticCode::MissingIdentity,
                "uuid",
                "Component has no nonempty source UUID/tstamp; ordinal is a source locator only",
                None,
            );
        }
        let pose = self.pose(node, &source, true);
        let mut component = Component {
            reference: None,
            value: None,
            library_id: node.value_at(1).map(str::to_owned),
            layer: text(node, "layer", 1),
            source_board_pose: pose,
            properties: Vec::new(),
            attributes: node
                .child("attr")
                .map(|attr| {
                    children(attr)
                        .filter_map(|(_, item)| item.atom().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            source: source.clone(),
        };
        // Presence and value are separate: a malformed first property still
        // blocks later properties and the legacy fp_text fallback.
        let mut property_values: std::collections::BTreeMap<&str, Option<&str>> =
            std::collections::BTreeMap::new();
        let mut legacy_values: std::collections::BTreeMap<&str, Option<&str>> =
            std::collections::BTreeMap::new();
        for (ordinal, child) in children(node) {
            let child_source = source_locator(child, ordinal, Some(source.ordinal));
            match child.head() {
                Some("property") => {
                    if let Some(name) = child.value_at(1) {
                        let value = child.value_at(2);
                        match property_values.entry(name) {
                            std::collections::btree_map::Entry::Vacant(entry) => {
                                entry.insert(value);
                            }
                            std::collections::btree_map::Entry::Occupied(entry) => {
                                self.diagnostic(&child_source, if *entry.get() == value { DiagnosticCode::DuplicateProperty } else { DiagnosticCode::ConflictingProperty }, "property", "Repeated property preserved; the first exact-name property remains authoritative", Some(name));
                            }
                        }
                        if value.is_none() {
                            self.diagnostic(&child_source, DiagnosticCode::MissingField, "property.value", "Property has no string value; absence retained without later-value substitution", Some(name));
                        }
                    }
                    let property = Property {
                        name: child.value_at(1).map(str::to_owned),
                        value: child.value_at(2).map(str::to_owned),
                        source_pose: self.pose(child, &child_source, false),
                        layer: text(child, "layer", 1),
                        hidden: hidden(child),
                        source: child_source,
                    };
                    component.properties.push(property);
                }
                Some("fp_text") => {
                    if let Some(role) = child.value_at(1) {
                        let canonical_name = if role.eq_ignore_ascii_case("reference") {
                            Some("Reference")
                        } else if role.eq_ignore_ascii_case("value") {
                            Some("Value")
                        } else {
                            None
                        };
                        if let Some(name) = canonical_name {
                            legacy_values.entry(name).or_insert(child.value_at(2));
                        }
                    }
                    self.graphic(
                        child,
                        child_source,
                        CoordinateFrame::ComponentLocal {
                            component_ordinal: source.ordinal,
                        },
                    );
                }
                Some("pad") => self.pad(child, child_source, pose),
                Some("model") => self.model(child, child_source),
                Some("zone") => self.zone(
                    child,
                    child_source,
                    CoordinateFrame::ComponentLocal {
                        component_ordinal: source.ordinal,
                    },
                ),
                Some(kind) if kind.starts_with("fp_") => self.graphic(
                    child,
                    child_source,
                    CoordinateFrame::ComponentLocal {
                        component_ordinal: source.ordinal,
                    },
                ),
                _ => {}
            }
        }
        component.reference = property_values
            .get("Reference")
            .or_else(|| legacy_values.get("Reference"))
            .and_then(|value| value.map(str::to_owned));
        component.value = property_values
            .get("Value")
            .or_else(|| legacy_values.get("Value"))
            .and_then(|value| value.map(str::to_owned));
        self.data.components.push(component);
    }

    fn zone(&mut self, node: &Sexp, source: SourceLocator, frame: CoordinateFrame) {
        let mut boundary_polygons = Vec::new();
        let mut cached_fill_polygons = Vec::new();
        for (ordinal, child) in children(node) {
            match child.head() {
                Some("polygon") => boundary_polygons.push(self.polygon(child, child_locator(child, ordinal, &source))),
                Some("filled_polygon") => cached_fill_polygons.push(self.polygon(child, child_locator(child, ordinal, &source))),
                Some("fill_segments") => self.diagnostic(&source, DiagnosticCode::UnsupportedPrimitive, "fill_segments", "Legacy zone fill segments remain in raw source and are not solved into polygons", None),
                _ => {}
            }
        }
        let zone = Zone {
            frame,
            layers: layer_names(node),
            net: self.net_reference(node, &source),
            name: text(node, "name", 1),
            boundary_polygons,
            cached_fill_polygons,
            has_fill_configuration: node.child("fill").is_some(),
            is_keepout: node.child("keepout").is_some(),
            source,
        };
        self.data.zones.push(zone);
    }

    fn track_segment(&mut self, node: &Sexp, source: SourceLocator) {
        let segment = TrackSegment {
            start_mm: self.point(node, "start", &source, true),
            end_mm: self.point(node, "end", &source, true),
            width_mm: self.dimension(node, "width", &source, true),
            layer: text(node, "layer", 1),
            net: self.net_reference(node, &source),
            source,
        };
        self.data.track_segments.push(segment);
    }

    fn track_arc(&mut self, node: &Sexp, source: SourceLocator) {
        let arc = TrackArc {
            start_mm: self.point(node, "start", &source, true),
            mid_mm: self.point(node, "mid", &source, true),
            end_mm: self.point(node, "end", &source, true),
            width_mm: self.dimension(node, "width", &source, true),
            layer: text(node, "layer", 1),
            net: self.net_reference(node, &source),
            source,
        };
        self.data.track_arcs.push(arc);
    }

    fn via(&mut self, node: &Sexp, source: SourceLocator) {
        let via = Via {
            source_pose: self.pose(node, &source, true),
            size_mm: self.dimension(node, "size", &source, true),
            drill_mm: self.dimension(node, "drill", &source, true),
            via_type: children(node)
                .filter_map(|(_, child)| child.atom())
                .find(|atom| matches!(*atom, "blind" | "micro"))
                .map(str::to_owned),
            layers: layer_names(node),
            net: self.net_reference(node, &source),
            source,
        };
        self.data.vias.push(via);
    }

    fn resolve_net_names(&mut self) {
        use std::collections::BTreeMap;
        let mut nets: BTreeMap<i64, Option<Arc<str>>> = BTreeMap::new();
        for net in &self.data.nets {
            if let Some(id) = net.id {
                match nets.entry(id) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(net.name.as_deref().map(Arc::from));
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        entry.insert(None);
                    }
                }
            }
        }
        let mut unresolved = Vec::new();
        let mut resolve = |source: &SourceLocator, reference: &mut Option<NetReference>| {
            if let Some(reference) = reference {
                if let Some(id) = reference.id {
                    if let Some(Some(name)) = nets.get(&id) {
                        if reference
                            .name
                            .as_ref()
                            .is_none_or(|source_name| source_name == name.as_ref())
                        {
                            reference.resolved_name = Some(name.clone());
                        } else if unresolved.len() < MAX_DIAGNOSTICS {
                            unresolved.push((
                                source.clone(),
                                id.to_string(),
                                DiagnosticCode::ConflictingNet,
                            ));
                        }
                    } else {
                        if unresolved.len() < MAX_DIAGNOSTICS {
                            unresolved.push((
                                source.clone(),
                                id.to_string(),
                                DiagnosticCode::UnresolvedNet,
                            ));
                        }
                    }
                }
            }
        };
        for pad in &mut self.data.pads {
            resolve(&pad.source, &mut pad.net);
        }
        for track in &mut self.data.track_segments {
            resolve(&track.source, &mut track.net);
        }
        for arc in &mut self.data.track_arcs {
            resolve(&arc.source, &mut arc.net);
        }
        for via in &mut self.data.vias {
            resolve(&via.source, &mut via.net);
        }
        for zone in &mut self.data.zones {
            resolve(&zone.source, &mut zone.net);
        }
        for (source, id, code) in unresolved {
            self.diagnostic(
                &source,
                code,
                "net.id",
                "Net name conflicts with its table entry, or its identifier has no unique named table entry",
                Some(&id),
            );
        }
    }

    fn check_component_identities(&mut self) {
        use std::collections::BTreeSet;
        let mut identities = BTreeSet::new();
        let duplicates: Vec<_> = self
            .data
            .components
            .iter()
            .filter_map(|component| {
                let uuid = component.source.source_uuid.as_ref()?;
                (!identities.insert(uuid.clone())).then(|| (component.source.clone(), uuid.clone()))
            })
            .take(MAX_DIAGNOSTICS)
            .collect();
        for (source, _uuid) in duplicates {
            self.diagnostic(&source, DiagnosticCode::DuplicateIdentity, "uuid", "Duplicate component UUID retained in source locator; consumers cannot treat it as a unique selection identity", None);
        }
    }
}

impl EngineeringData {
    pub(crate) fn from_source(root: &Sexp) -> Self {
        let mut importer = Importer {
            data: Self::default(),
        };
        if let Some((layers_ordinal, layers)) =
            children(root).find(|(_, node)| node.head() == Some("layers"))
        {
            let layers_source = source_locator(layers, layers_ordinal, None);
            for (ordinal, node) in children(layers) {
                let source = SourceLocator {
                    kind: "layer".to_owned(),
                    ..child_locator(node, ordinal, &layers_source)
                };
                let layer = Layer {
                    id: importer.integer(node.value_at(0), &source, "layer.id", true),
                    name: node.value_at(1).map(str::to_owned),
                    kind: node.value_at(2).map(str::to_owned),
                    display_label: node.value_at(3).map(str::to_owned),
                    source,
                };
                importer.data.layers.push(layer);
            }
        }
        for (ordinal, node) in children(root) {
            let source = source_locator(node, ordinal, None);
            match node.head() {
                Some("net") => {
                    let net = Net {
                        id: importer.integer(node.value_at(1), &source, "net.id", true),
                        name: node.value_at(2).map(str::to_owned),
                        source,
                    };
                    importer.data.nets.push(net);
                }
                Some("footprint" | "module") => importer.component(node, source),
                Some("segment") => importer.track_segment(node, source),
                Some("arc") => importer.track_arc(node, source),
                Some("via") => importer.via(node, source),
                Some("zone") => importer.zone(node, source, CoordinateFrame::Board),
                Some(kind)
                    if kind.starts_with("gr_")
                        || matches!(kind, "dimension" | "image" | "target") =>
                {
                    importer.graphic(node, source, CoordinateFrame::Board)
                }
                _ => {}
            }
        }
        importer.resolve_net_names();
        importer.check_component_identities();
        importer.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(input: &str) -> EngineeringData {
        let root = crate::parse_document(input.as_bytes()).expect("fixture parsing");
        EngineeringData::from_source(&root)
    }

    #[test]
    fn component_transform_preserves_saved_back_side_and_absolute_pad_angle() {
        for side in ["F.Cu", "B.Cu"] {
            let data = source(&format!(
                "(kicad_pcb (layers (0 F.Cu signal)) (net 1 VCC) (footprint lib:id (layer {side}) (uuid component-1) (at 10 20 90) (property Reference D1) (pad 1 smd rect (uuid pad-1) (at 2 3 30) (size 1 2) (layers {side}) (net 1 VCC))))"
            ));
            assert_eq!(data.components[0].reference.as_deref(), Some("D1"));
            assert_eq!(data.pads[0].source.source_uuid.as_deref(), Some("pad-1"));
            assert_eq!(
                data.pads[0].source.component_ordinal,
                Some(data.components[0].source.ordinal)
            );
            assert_eq!(
                data.pads[0].board_position_mm,
                Some(Point2 { x: 13.0, y: 18.0 })
            );
            assert_eq!(data.pads[0].source_absolute_angle_deg, Some(30.0));
            assert_eq!(
                data.pads[0].source_local_pose.position_mm,
                Some(Point2 { x: 2.0, y: 3.0 })
            );
        }
    }

    #[test]
    fn malformed_geometry_stays_absent_and_unsupported_pads_are_retained() {
        let data = source(
            "(kicad_pcb (footprint X (uuid x) (pad 7 smd custom (at 2 NaN) (size 1 1) (layers F.Cu) (primitives (gr_circle (center 0 0) (end 1 0))))) (zone (polygon (pts (xy 0 0) (xy NaN 1) (xy 2 2)))))",
        );
        assert_eq!(data.pads.len(), 1);
        assert_eq!(data.pads[0].shape.as_deref(), Some("custom"));
        assert_eq!(data.pads[0].source_local_pose.position_mm, None);
        assert_eq!(data.pads[0].board_position_mm, None);
        assert_eq!(data.zones[0].boundary_polygons[0].points_mm.len(), 3);
        assert_eq!(data.zones[0].boundary_polygons[0].points_mm[1], None);
        assert!(!data.zones[0].boundary_polygons[0].is_complete);
        assert!(
            data.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::UnsupportedShape)
        );
        assert!(
            data.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidNumber)
        );
    }

    #[test]
    fn models_and_zone_boundary_are_not_replaced_with_guessed_geometry() {
        let data = source(
            "(kicad_pcb (net 4 GND) (footprint L (uuid c) (at 0 0) (model ${KIPRJMOD}/part.step (offset (xyz 1 2 3)) (rotate (xyz 0 0 90)) (scale (xyz 1 1 1)) hide)) (zone (net 4) (layer F.Cu) (polygon (pts (xy 0 0) (xy 4 0) (xy 4 4))) (filled_polygon (layer F.Cu) (pts (xy 1 1) (xy 2 1) (xy 2 2)))))",
        );
        assert_eq!(
            data.models[0].path.as_deref(),
            Some("${KIPRJMOD}/part.step")
        );
        assert_eq!(
            data.models[0].offset,
            Some(Point3 {
                x: 1.0,
                y: 2.0,
                z: 3.0
            })
        );
        assert!(data.models[0].hidden);
        assert_eq!(data.zones[0].boundary_polygons.len(), 1);
        assert_eq!(data.zones[0].cached_fill_polygons.len(), 1);
        assert_eq!(
            data.zones[0]
                .net
                .as_ref()
                .and_then(|net| net.resolved_name.as_deref()),
            Some("GND")
        );
    }

    #[test]
    fn diagnostics_are_capped_with_an_explicit_truncation_record() {
        let mut text = String::from("(kicad_pcb");
        for _ in 0..600 {
            text.push_str(" (segment (start NaN 0))");
        }
        text.push(')');
        let data = source(&text);
        assert_eq!(data.track_segments.len(), 600);
        assert_eq!(data.diagnostics.len(), MAX_DIAGNOSTICS);
        assert!(data.diagnostics_truncated);
        assert_eq!(
            data.diagnostics.last().unwrap().code,
            DiagnosticCode::DiagnosticsTruncated
        );
    }

    #[test]
    fn invalid_angle_does_not_default_to_zero_and_coordinate_overflow_is_rejected() {
        let data = source(
            "(kicad_pcb (footprint X (uuid c) (at 1 2 NaN) (pad 1 smd rect (at 0 0) (size 1 1))))",
        );
        assert_eq!(data.pads[0].board_position_mm, None);
        assert_eq!(
            data.components[0]
                .source_board_pose
                .effective_rotation_deg(),
            None
        );
        assert_eq!(
            source_local_to_board(
                Point2 {
                    x: f64::MAX,
                    y: 0.0
                },
                Point2 {
                    x: f64::MAX,
                    y: 0.0
                },
                0.0
            ),
            None
        );
    }

    #[test]
    fn source_labels_do_not_trigger_hide_and_conflicting_nets_do_not_resolve() {
        let data = source(
            "(kicad_pcb (net 1 GND) (footprint X (uuid c) (at 0 0) (property Comment hide (at 1 1)) (fp_text user hide (at 2 2)) (pad 1 smd rect (at 0 0) (size 1 1) (net 1 VCC))) (gr_text hide (at 3 3)))",
        );
        assert!(!data.components[0].properties[0].hidden);
        assert!(data.graphics.iter().all(|graphic| !graphic.hidden));
        assert_eq!(
            data.pads[0].net.as_ref().unwrap().name.as_deref(),
            Some("VCC")
        );
        assert_eq!(data.pads[0].net.as_ref().unwrap().resolved_name, None);
        assert!(
            data.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::ConflictingNet)
        );
    }

    #[test]
    fn first_property_wins_over_later_properties_and_earlier_legacy_text() {
        let data = source(
            "(kicad_pcb (footprint X (uuid c1) (at 0 0) (fp_text ReFeReNcE OLD (at 0 0)) (property Reference D1) (property Reference D2) (property Value \"\") (property Value replacement)) (footprint Y (uuid c2) (at 0 0) (fp_text reference FALLBACK (at 0 0)) (property Reference) (property Reference later)) (footprint Z (uuid c3) (at 0 0) (fp_text REFERENCE legacy-first (at 0 0)) (fp_text reference legacy-second (at 0 0)) (property Value same) (property Value same)))",
        );
        assert_eq!(data.components[0].reference.as_deref(), Some("D1"));
        assert_eq!(data.components[0].value.as_deref(), Some(""));
        assert_eq!(data.components[0].properties.len(), 4);
        assert_eq!(data.components[1].reference, None);
        assert_eq!(data.components[1].properties.len(), 2);
        assert_eq!(
            data.components[2].reference.as_deref(),
            Some("legacy-first")
        );
        assert_eq!(data.components[2].value.as_deref(), Some("same"));
        assert_eq!(
            data.diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == DiagnosticCode::ConflictingProperty)
                .count(),
            3
        );
        assert_eq!(
            data.diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == DiagnosticCode::DuplicateProperty)
                .count(),
            1
        );
    }

    #[test]
    fn locator_paths_traverse_layers_and_nested_polygon_points_to_original_nodes() {
        fn walk<'a>(root: &'a Sexp, path: &[usize]) -> &'a Sexp {
            path.iter()
                .fold(root, |node, ordinal| &node.items().unwrap()[ordinal + 1])
        }
        let input = "(kicad_pcb (version 20240108) (general (thickness 1.6)) (layers (0 F.Cu signal) (31 B.Cu signal)) (zone (layer F.Cu) (polygon (pts (xy 1 2) (xy NaN 4) (xy 5 6)))))";
        let root = crate::parse_document(input.as_bytes()).unwrap();
        let data = EngineeringData::from_source(&root);
        for layer in &data.layers {
            let node = walk(&root, &layer.source.source_path);
            assert_eq!(node.value_at(1), layer.name.as_deref());
            assert_eq!(layer.source.source_path.len(), 2);
        }
        let polygon = &data.zones[0].boundary_polygons[0];
        assert_eq!(
            walk(&root, &polygon.source.source_path).head(),
            Some("polygon")
        );
        let malformed_point = data
            .diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code == DiagnosticCode::InvalidNumber && diagnostic.field == "xy"
            })
            .unwrap();
        let raw_point = walk(&root, &malformed_point.source.source_path);
        assert_eq!(raw_point.head(), Some("xy"));
        assert_eq!(raw_point.value_at(1), Some("NaN"));
        assert_eq!(malformed_point.source.source_path.len(), 4);
    }

    #[test]
    fn resolved_net_names_and_diagnostic_identities_share_exact_source_text() {
        let long_name = "N".repeat(32_768);
        let long_uuid = "U".repeat(32_768);
        let input = format!(
            "(kicad_pcb (net 1 {long_name}) (footprint X (uuid {long_uuid}) (at 0 0) (pad 1 smd rect (at 0 0) (size 1 1) (net 1)) (pad 2 smd rect (at 1 0) (size 1 1) (net 1))) (footprint Y (uuid {long_uuid}) (at 0 0)))"
        );
        let data = source(&input);
        let first_name = data.pads[0]
            .net
            .as_ref()
            .unwrap()
            .resolved_name
            .as_ref()
            .unwrap();
        let second_name = data.pads[1]
            .net
            .as_ref()
            .unwrap()
            .resolved_name
            .as_ref()
            .unwrap();
        assert!(Arc::ptr_eq(first_name, second_name));
        assert_eq!(first_name.as_ref(), long_name);
        let duplicate = data
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::DuplicateIdentity)
            .unwrap();
        let component_id = data.components[1].source.source_uuid.as_ref().unwrap();
        assert!(Arc::ptr_eq(
            component_id,
            duplicate.source.source_uuid.as_ref().unwrap()
        ));
        assert_eq!(component_id.as_ref(), long_uuid);
        assert_eq!(duplicate.raw_value, None);
    }
}
