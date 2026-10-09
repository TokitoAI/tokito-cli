use super::{CliError, Footprint, Sexp};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

const MAX_SVG_BYTES: usize = 64 * 1024 * 1024;
const MAX_SVG_FIELD_BYTES: usize = 8192;
const MAX_DIAGNOSTICS: usize = 500;
const MAX_POLYGON_POINTS: usize = 100_000;
const MAX_GEOMETRY_VALUE_MM: f64 = 1_000_000.0;
const OUTLINE_MARGIN_MM: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Drill {
    pub width_mm: f64,
    pub height_mm: f64,
    pub offset: Point,
}

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    fn from_point(point: Point) -> Self {
        Self {
            min_x: point.x,
            min_y: point.y,
            max_x: point.x,
            max_y: point.y,
        }
    }

    fn include(&mut self, point: Point) {
        self.min_x = self.min_x.min(point.x);
        self.min_y = self.min_y.min(point.y);
        self.max_x = self.max_x.max(point.x);
        self.max_y = self.max_y.max(point.y);
    }

    fn width(self) -> f64 {
        self.max_x - self.min_x
    }

    fn height(self) -> f64 {
        self.max_y - self.min_y
    }
}

#[derive(Clone, Debug)]
pub enum Shape {
    Line {
        start: Point,
        end: Point,
        width: f64,
    },
    Arc {
        start: Point,
        mid: Point,
        end: Point,
        width: f64,
    },
    Circle {
        center: Point,
        radius: f64,
        width: f64,
    },
    Polygon {
        points: Vec<Point>,
        width: f64,
        fill: bool,
    },
    Text {
        text: String,
        at: Point,
        rotation_deg: f64,
        size_x_mm: f64,
        size_y_mm: f64,
        width: f64,
    },
    Pad {
        number: String,
        at: Point,
        size_x_mm: f64,
        size_y_mm: f64,
        rotation_deg: f64,
        shape: String,
        roundrect_ratio: f64,
        drill: Option<Drill>,
        net_name: Option<String>,
    },
    Via {
        at: Point,
        size_mm: f64,
        drill_mm: f64,
        net_name: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct Item {
    pub layers: Vec<String>,
    pub id: Option<String>,
    pub component_id: Option<String>,
    pub shape: Shape,
}

#[derive(Debug, Default)]
pub struct Geometry {
    items: Vec<Item>,
    diagnostics: Vec<String>,
    outline: Option<Bounds>,
    outline_closed: bool,
}

impl Geometry {
    /// Supported board-space primitives; engineering metadata is in the document.
    pub fn items(&self) -> &[Item] {
        &self.items
    }
    pub fn outline_bounds(&self) -> Option<Bounds> {
        self.outline
    }
    /// Endpoint closure only; not a full contour/intersection validation result.
    pub fn outline_closed(&self) -> bool {
        self.outline_closed
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
}

pub struct SvgOutput {
    pub contents: String,
    pub rendered_items: usize,
    pub warnings: Vec<String>,
    pub width_mm: f64,
    pub height_mm: f64,
}

#[derive(Clone, Copy)]
struct Pose {
    at: Point,
    rotation_deg: f64,
}

fn value<'a>(node: &'a Sexp, key: &str, index: usize) -> Option<&'a str> {
    node.child(key)?.value_at(index)
}

fn number(node: &Sexp, key: &str, index: usize) -> Option<f64> {
    super::parse_number(value(node, key, index))
}

fn point(node: &Sexp, key: &str) -> Option<Point> {
    let coordinates = node.child(key)?;
    let point = Point {
        x: super::parse_number(coordinates.value_at(1))?,
        y: super::parse_number(coordinates.value_at(2))?,
    };
    point_is_supported(point).then_some(point)
}

fn point_list(node: &Sexp) -> Option<Vec<Point>> {
    let points = node.child("pts")?;
    let parsed = points
        .children("xy")
        .map(|xy| {
            let point = Point {
                x: super::parse_number(xy.value_at(1))?,
                y: super::parse_number(xy.value_at(2))?,
            };
            point_is_supported(point).then_some(point)
        })
        .collect::<Option<Vec<_>>>()?;
    (!parsed.is_empty() && parsed.len() <= MAX_POLYGON_POINTS).then_some(parsed)
}

fn layer(node: &Sexp) -> Option<String> {
    node.child("layer")?.value_at(1).map(str::to_owned)
}

fn layers(node: &Sexp) -> Vec<String> {
    if let Some(list) = node.child("layers") {
        list.items()
            .into_iter()
            .flat_map(|items| items.iter().skip(1))
            .filter_map(Sexp::atom)
            .map(str::to_owned)
            .collect()
    } else {
        layer(node).into_iter().collect()
    }
}

fn stroke_width(node: &Sexp, default: f64) -> f64 {
    node.child("stroke")
        .and_then(|stroke| number(stroke, "width", 1))
        .or_else(|| number(node, "width", 1))
        .filter(|width| *width >= 0.0)
        .unwrap_or(default)
}

fn source_id(node: &Sexp) -> Option<String> {
    node.child("uuid")
        .or_else(|| node.child("tstamp"))
        .and_then(|value| value.value_at(1))
        .map(str::to_owned)
}

fn value_is_supported(value: f64) -> bool {
    value.is_finite() && value.abs() <= MAX_GEOMETRY_VALUE_MM
}

fn point_is_supported(point: Point) -> bool {
    value_is_supported(point.x) && value_is_supported(point.y)
}

fn pose_from_footprint(footprint: &Footprint) -> Option<Pose> {
    Some(Pose {
        at: Point {
            x: footprint.x_mm?,
            y: footprint.y_mm?,
        },
        rotation_deg: footprint.rotation_deg?,
    })
}

fn transform(point: Point, pose: Option<Pose>) -> Point {
    let Some(pose) = pose else { return point };
    let (x, y) = super::document::rotate_translate_xy(
        point.x,
        point.y,
        pose.at.x,
        pose.at.y,
        pose.rotation_deg,
    );
    Point { x, y }
}

fn at_angle(node: &Sexp, key: &str) -> f64 {
    node.child(key)
        .and_then(|at| at.value_at(3))
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
}

fn font_size(node: &Sexp) -> (f64, f64, f64) {
    let font = node
        .child("effects")
        .and_then(|effects| effects.child("font"));
    let size = font.and_then(|font| font.child("size"));
    let x = size
        .and_then(|size| size.value_at(1))
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(1.0);
    let y = size
        .and_then(|size| size.value_at(2))
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(x);
    let width = font
        .and_then(|font| font.child("thickness"))
        .and_then(|width| width.value_at(1))
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.15);
    (x, y, width)
}

fn is_hidden(node: &Sexp) -> bool {
    node.child("hide")
        .is_some_and(|hide| hide.value_at(1) == Some("yes"))
        || node
            .items()
            .is_some_and(|items| items.iter().skip(3).any(|item| item.atom() == Some("hide")))
        || node.child("effects").is_some_and(|effects| {
            effects
                .items()
                .is_some_and(|items| items.iter().skip(1).any(|item| item.atom() == Some("hide")))
                || effects
                    .child("hide")
                    .is_some_and(|hide| hide.value_at(1) == Some("yes"))
        })
}

fn net_name(node: &Sexp) -> Option<String> {
    node.child("net")?.value_at(2).map(str::to_owned)
}

fn add_diagnostic(geometry: &mut Geometry, message: impl Into<String>) {
    match geometry.diagnostics.len().cmp(&MAX_DIAGNOSTICS) {
        std::cmp::Ordering::Less => geometry.diagnostics.push(message.into()),
        std::cmp::Ordering::Equal => geometry
            .diagnostics
            .push("additional geometry diagnostics omitted at the per-board limit".to_owned()),
        std::cmp::Ordering::Greater => {}
    }
}

fn add_item(geometry: &mut Geometry, item: Item) {
    if !item_geometry_is_supported(&item.shape) {
        add_diagnostic(
            geometry,
            "geometry exceeds the supported finite coordinate or dimension range and was omitted",
        );
        return;
    }
    if geometry.items.len() < 1_000_000 {
        geometry.items.push(item);
    } else if geometry.diagnostics.is_empty()
        || !geometry
            .diagnostics
            .iter()
            .any(|message| message.contains("geometry item limit"))
    {
        add_diagnostic(
            geometry,
            "geometry item limit reached; remaining items were not imported",
        );
    }
}

fn add_footprint_item(geometry: &mut Geometry, mut item: Item, footprint: &Footprint) {
    if !footprint.uuid.is_empty() {
        item.component_id = Some(footprint.uuid.clone());
    }
    add_item(geometry, item);
}

fn item_geometry_is_supported(shape: &Shape) -> bool {
    let valid_width = |width: f64| width >= 0.0 && value_is_supported(width);
    match shape {
        Shape::Line { start, end, width } => {
            point_is_supported(*start) && point_is_supported(*end) && valid_width(*width)
        }
        Shape::Arc {
            start,
            mid,
            end,
            width,
        } => {
            point_is_supported(*start)
                && point_is_supported(*mid)
                && point_is_supported(*end)
                && valid_width(*width)
                && arc_circle(*start, *mid, *end).is_some()
        }
        Shape::Circle {
            center,
            radius,
            width,
        } => {
            point_is_supported(*center)
                && *radius > 0.0
                && value_is_supported(*radius)
                && valid_width(*width)
        }
        Shape::Polygon { points, width, .. } => {
            points.len() <= MAX_POLYGON_POINTS
                && points.iter().all(|point| point_is_supported(*point))
                && valid_width(*width)
        }
        Shape::Text {
            at,
            rotation_deg,
            size_x_mm,
            size_y_mm,
            width,
            ..
        } => {
            point_is_supported(*at)
                && value_is_supported(*rotation_deg)
                && *size_x_mm > 0.0
                && value_is_supported(*size_x_mm)
                && *size_y_mm > 0.0
                && value_is_supported(*size_y_mm)
                && valid_width(*width)
        }
        Shape::Pad {
            at,
            size_x_mm,
            size_y_mm,
            rotation_deg,
            roundrect_ratio,
            drill,
            ..
        } => {
            point_is_supported(*at)
                && *size_x_mm > 0.0
                && value_is_supported(*size_x_mm)
                && *size_y_mm > 0.0
                && value_is_supported(*size_y_mm)
                && value_is_supported(*rotation_deg)
                && value_is_supported(*roundrect_ratio)
                && drill.is_none_or(|drill| {
                    drill.width_mm > 0.0
                        && value_is_supported(drill.width_mm)
                        && drill.height_mm > 0.0
                        && value_is_supported(drill.height_mm)
                        && point_is_supported(drill.offset)
                })
        }
        Shape::Via {
            at,
            size_mm,
            drill_mm,
            ..
        } => {
            point_is_supported(*at)
                && *size_mm > 0.0
                && value_is_supported(*size_mm)
                && *drill_mm > 0.0
                && value_is_supported(*drill_mm)
        }
    }
}

fn transformed_points(points: Vec<Point>, pose: Option<Pose>) -> Vec<Point> {
    points
        .into_iter()
        .map(|point| transform(point, pose))
        .collect()
}

fn graphic_item(
    node: &Sexp,
    pose: Option<Pose>,
    text_index: Option<usize>,
    geometry: &mut Geometry,
) -> Option<Item> {
    let head = node.head()?;
    let layer = layer(node)?;
    let default_width = if layer == "Edge.Cuts" { 0.05 } else { 0.15 };
    let id = source_id(node);
    let shape = match head {
        "gr_line" | "fp_line" => Shape::Line {
            start: transform(point(node, "start")?, pose),
            end: transform(point(node, "end")?, pose),
            width: stroke_width(node, default_width),
        },
        "gr_arc" | "fp_arc" => Shape::Arc {
            start: transform(point(node, "start")?, pose),
            mid: transform(point(node, "mid")?, pose),
            end: transform(point(node, "end")?, pose),
            width: stroke_width(node, default_width),
        },
        "gr_rect" | "fp_rect" => {
            let start = point(node, "start")?;
            let end = point(node, "end")?;
            Shape::Polygon {
                points: transformed_points(
                    vec![
                        start,
                        Point {
                            x: end.x,
                            y: start.y,
                        },
                        end,
                        Point {
                            x: start.x,
                            y: end.y,
                        },
                    ],
                    pose,
                ),
                width: stroke_width(node, default_width),
                fill: false,
            }
        }
        "gr_circle" | "fp_circle" => {
            let center = point(node, "center")?;
            let edge = point(node, "end")?;
            let radius = ((edge.x - center.x).powi(2) + (edge.y - center.y).powi(2)).sqrt();
            if !radius.is_finite() || radius <= 0.0 {
                add_diagnostic(geometry, format!("{head} has an invalid circle radius"));
                return None;
            }
            Shape::Circle {
                center: transform(center, pose),
                radius,
                width: stroke_width(node, default_width),
            }
        }
        "gr_poly" | "fp_poly" => {
            let Some(points) = point_list(node) else {
                add_diagnostic(
                    geometry,
                    format!("{head} has an invalid or oversized point list"),
                );
                return None;
            };
            Shape::Polygon {
                points: transformed_points(points, pose),
                width: stroke_width(node, default_width),
                fill: node
                    .child("fill")
                    .and_then(|fill| fill.value_at(1))
                    .is_some_and(|value| matches!(value, "yes" | "solid")),
            }
        }
        "gr_text" | "fp_text" | "property" => {
            if is_hidden(node) {
                return None;
            }
            let text = node.value_at(text_index?)?.to_owned();
            let at = point(node, "at")?;
            // Saved board text angles are absolute; SVG has the opposite rotation sign.
            let rotation_deg = -at_angle(node, "at");
            let (size_x_mm, size_y_mm, width) = font_size(node);
            Shape::Text {
                text,
                at: transform(at, pose),
                rotation_deg,
                size_x_mm,
                size_y_mm,
                width,
            }
        }
        _ => return None,
    };
    Some(Item {
        layers: vec![layer],
        id,
        component_id: None,
        shape,
    })
}

fn drill_geometry(node: &Sexp) -> Option<Drill> {
    let drill = node.child("drill")?;
    let numbers = drill
        .items()?
        .iter()
        .skip(1)
        .filter_map(Sexp::atom)
        .filter_map(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= MAX_GEOMETRY_VALUE_MM)
        .collect::<Vec<_>>();
    let (width_mm, height_mm) = match numbers.as_slice() {
        [diameter] => (*diameter, *diameter),
        [width, height] => (*width, *height),
        _ => return None,
    };
    let offset = if drill.child("offset").is_some() {
        point(drill, "offset")?
    } else {
        Point { x: 0.0, y: 0.0 }
    };
    Some(Drill {
        width_mm,
        height_mm,
        offset,
    })
}

fn pad_item(
    node: &Sexp,
    pose: Pose,
    footprint: &Footprint,
    geometry: &mut Geometry,
) -> Option<Item> {
    let pad_number = node.value_at(1)?.to_owned();
    let shape = node.value_at(3)?.to_owned();
    if !matches!(shape.as_str(), "circle" | "oval" | "rect" | "roundrect") {
        add_diagnostic(
            geometry,
            format!(
                "pad {pad_number} on {} uses unsupported {shape} geometry",
                footprint.reference
            ),
        );
        return None;
    }
    let at = point(node, "at")?;
    let size_x_mm = number(node, "size", 1)?;
    let size_y_mm = number(node, "size", 2)?;
    if size_x_mm <= 0.0 || size_y_mm <= 0.0 {
        add_diagnostic(
            geometry,
            format!("pad {pad_number} has a non-positive size"),
        );
        return None;
    }
    let drill = if node.child("drill").is_some() {
        match drill_geometry(node) {
            Some(drill) => Some(drill),
            None => {
                add_diagnostic(
                    geometry,
                    format!("pad {pad_number} has unsupported or invalid drill geometry"),
                );
                return None;
            }
        }
    } else {
        None
    };
    // Pad XY is footprint-relative, but the saved pad angle is board-absolute.
    let rotation_deg = -at_angle(node, "at");
    let roundrect_ratio = number(node, "roundrect_rratio", 1)
        .filter(|ratio| *ratio > 0.0 && *ratio <= 0.5)
        .unwrap_or(0.25);
    let layers = layers(node);
    if layers.is_empty() {
        add_diagnostic(
            geometry,
            format!(
                "pad {pad_number} on {} has no recognized layers",
                footprint.reference
            ),
        );
        return None;
    }
    let id = source_id(node).or_else(|| {
        (!footprint.uuid.is_empty()).then(|| format!("{}:pad:{pad_number}", footprint.uuid))
    });
    Some(Item {
        layers,
        id,
        component_id: None,
        shape: Shape::Pad {
            number: pad_number,
            at: transform(at, Some(pose)),
            size_x_mm,
            size_y_mm,
            rotation_deg,
            shape,
            roundrect_ratio,
            drill,
            net_name: net_name(node),
        },
    })
}

fn via_item(node: &Sexp, geometry: &mut Geometry) -> Option<Item> {
    let at = point(node, "at")?;
    let size_mm = number(node, "size", 1)?;
    let drill_mm = number(node, "drill", 1)?;
    let layers = layers(node);
    if size_mm <= 0.0 || drill_mm <= 0.0 || drill_mm > size_mm || layers.is_empty() {
        add_diagnostic(geometry, "via has invalid size, drill, or layer data");
        return None;
    }
    Some(Item {
        layers,
        id: source_id(node),
        component_id: None,
        shape: Shape::Via {
            at,
            size_mm,
            drill_mm,
            net_name: net_name(node),
        },
    })
}

fn track_item(node: &Sexp, geometry: &mut Geometry) -> Option<Item> {
    let head = node.head()?;
    let layer = layer(node)?;
    let width = number(node, "width", 1)?;
    if width <= 0.0 {
        add_diagnostic(geometry, format!("{head} has a non-positive track width"));
        return None;
    }
    let shape = if head == "segment" {
        Shape::Line {
            start: point(node, "start")?,
            end: point(node, "end")?,
            width,
        }
    } else {
        Shape::Arc {
            start: point(node, "start")?,
            mid: point(node, "mid")?,
            end: point(node, "end")?,
            width,
        }
    };
    Some(Item {
        layers: vec![layer],
        id: source_id(node),
        component_id: None,
        shape,
    })
}

fn zone_items(node: &Sexp, geometry: &mut Geometry) -> Vec<Item> {
    let mut result = Vec::new();
    for filled in node.children("filled_polygon") {
        let Some(layer) = layer(filled) else {
            add_diagnostic(geometry, "zone filled polygon has no layer and was omitted");
            continue;
        };
        let Some(points) = point_list(filled) else {
            add_diagnostic(
                geometry,
                "zone filled polygon has invalid or oversized points",
            );
            continue;
        };
        result.push(Item {
            layers: vec![layer],
            id: source_id(filled),
            component_id: None,
            shape: Shape::Polygon {
                points,
                width: 0.0,
                fill: true,
            },
        });
    }
    if result.is_empty() {
        add_diagnostic(
            geometry,
            "unfilled or legacy-segment zone geometry is not rendered",
        );
    }
    result
}

/// Resolve canonical source locators once, preserving saved source order for
/// the frozen SVG projection. Unsupported render syntax stays in the typed
/// inventory and raw source instead of becoming invented geometry.
type SourceMap<'a> = BTreeMap<usize, &'a Sexp>;
type ComponentSources<'a> = BTreeMap<usize, SourceMap<'a>>;

fn render_sources<'a>(
    document: &'a Sexp,
    data: &super::document::EngineeringData,
) -> (SourceMap<'a>, ComponentSources<'a>) {
    let mut root = BTreeMap::new();
    let mut children: BTreeMap<usize, BTreeMap<usize, &Sexp>> = BTreeMap::new();
    let sources = data
        .pads
        .iter()
        .map(|record| &record.source)
        .chain(data.graphics.iter().map(|record| &record.source))
        .chain(
            data.components
                .iter()
                .flat_map(|component| component.properties.iter().map(|record| &record.source)),
        )
        .chain(data.track_segments.iter().map(|record| &record.source))
        .chain(data.track_arcs.iter().map(|record| &record.source))
        .chain(data.vias.iter().map(|record| &record.source))
        .chain(data.zones.iter().map(|record| &record.source));
    for source in sources {
        let Some(node) = super::source_at_path(document, &source.source_path) else {
            continue;
        };
        match source.source_path.as_slice() {
            [ordinal] => {
                root.insert(*ordinal, node);
            }
            [parent, ordinal] if source.component_ordinal == Some(*parent) => {
                children.entry(*parent).or_default().insert(*ordinal, node);
            }
            _ => {}
        }
    }
    (root, children)
}

pub(super) fn parse_geometry(
    document: &Sexp,
    footprints: &[Footprint],
    data: &super::document::EngineeringData,
) -> Geometry {
    let mut geometry = Geometry::default();
    if document.items().is_none() {
        add_diagnostic(&mut geometry, "board document is not a list");
        return geometry;
    }
    let (root_sources, child_sources) = render_sources(document, data);
    for item in root_sources.values().copied() {
        match item.head() {
            Some("segment" | "arc") => {
                if let Some(parsed) = track_item(item, &mut geometry) {
                    add_item(&mut geometry, parsed);
                } else {
                    add_diagnostic(
                        &mut geometry,
                        format!(
                            "{} has invalid track geometry",
                            item.head().unwrap_or("track")
                        ),
                    );
                }
            }
            Some("via") => {
                if let Some(parsed) = via_item(item, &mut geometry) {
                    add_item(&mut geometry, parsed);
                } else {
                    add_diagnostic(&mut geometry, "via has incomplete geometry and was omitted");
                }
            }
            Some("zone") => {
                for parsed in zone_items(item, &mut geometry) {
                    add_item(&mut geometry, parsed);
                }
            }
            Some("gr_line" | "gr_arc" | "gr_rect" | "gr_circle" | "gr_poly" | "gr_text") => {
                if let Some(parsed) = graphic_item(item, None, Some(1), &mut geometry) {
                    add_item(&mut geometry, parsed);
                } else {
                    add_diagnostic(
                        &mut geometry,
                        format!(
                            "{} has unsupported or invalid geometry",
                            item.head().unwrap_or("graphic")
                        ),
                    );
                }
            }
            Some("dimension" | "image" | "gr_curve" | "gr_text_box") => {
                add_diagnostic(
                    &mut geometry,
                    format!("{} geometry is not rendered", item.head().unwrap_or("item")),
                );
            }
            _ => {}
        }
    }

    for (component, footprint) in data
        .components
        .iter()
        .filter(|component| component.source.kind == "footprint")
        .zip(footprints.iter())
    {
        let Some(pose) = pose_from_footprint(footprint) else {
            add_diagnostic(
                &mut geometry,
                format!(
                    "footprint {} has no usable pose; its SVG geometry was omitted",
                    footprint.reference
                ),
            );
            continue;
        };
        let Some(children) = child_sources.get(&component.source.ordinal) else {
            continue;
        };
        for child in children.values().copied() {
            match child.head() {
                Some("pad") => {
                    if let Some(parsed) = pad_item(child, pose, footprint, &mut geometry) {
                        add_footprint_item(&mut geometry, parsed, footprint);
                    } else {
                        add_diagnostic(
                            &mut geometry,
                            format!(
                                "pad on {} has incomplete geometry and was omitted",
                                footprint.reference
                            ),
                        );
                    }
                }
                Some("property") => {
                    if child.child("at").is_some() && child.child("layer").is_some() {
                        if let Some(parsed) =
                            graphic_item(child, Some(pose), Some(2), &mut geometry)
                        {
                            add_footprint_item(&mut geometry, parsed, footprint);
                        } else if !is_hidden(child) {
                            add_diagnostic(
                                &mut geometry,
                                format!(
                                    "{} text on {} is incomplete",
                                    child.value_at(1).unwrap_or("property"),
                                    footprint.reference
                                ),
                            );
                        }
                    }
                }
                Some("fp_text") => {
                    if let Some(parsed) = graphic_item(child, Some(pose), Some(2), &mut geometry) {
                        add_footprint_item(&mut geometry, parsed, footprint);
                    } else if !is_hidden(child) {
                        add_diagnostic(
                            &mut geometry,
                            format!("footprint text on {} is incomplete", footprint.reference),
                        );
                    }
                }
                Some("fp_line" | "fp_arc" | "fp_rect" | "fp_circle" | "fp_poly") => {
                    if let Some(parsed) = graphic_item(child, Some(pose), None, &mut geometry) {
                        add_footprint_item(&mut geometry, parsed, footprint);
                    } else {
                        add_diagnostic(
                            &mut geometry,
                            format!(
                                "{} on {} is unsupported or incomplete",
                                child.head().unwrap_or("graphic"),
                                footprint.reference
                            ),
                        );
                    }
                }
                Some("fp_curve" | "fp_text_box") => {
                    add_diagnostic(
                        &mut geometry,
                        format!(
                            "{} on {} is not rendered",
                            child.head().unwrap_or("item"),
                            footprint.reference
                        ),
                    );
                }
                _ => {}
            }
        }
    }

    let (bounds, outline_closed) = analyze_outline(&geometry.items);
    geometry.outline = bounds;
    geometry.outline_closed = outline_closed;
    if geometry.outline.is_none() {
        add_diagnostic(
            &mut geometry,
            "no supported Edge.Cuts geometry was imported",
        );
    } else if !geometry.outline_closed {
        add_diagnostic(
            &mut geometry,
            "Edge.Cuts segments do not form closed contours; SVG export is blocked",
        );
    }
    geometry
}

fn point_key(point: Point) -> (i64, i64) {
    (
        (point.x * 1_000_000.0).round() as i64,
        (point.y * 1_000_000.0).round() as i64,
    )
}

fn analyze_outline(items: &[Item]) -> (Option<Bounds>, bool) {
    let mut bounds = None;
    let mut endpoint_degrees = BTreeMap::<(i64, i64), usize>::new();
    let mut edges = BTreeSet::<((i64, i64), (i64, i64))>::new();
    let mut closed_primitives = 0usize;
    let mut unsupported = false;
    for item in items
        .iter()
        .filter(|item| item.layers.iter().any(|layer| layer == "Edge.Cuts"))
    {
        include_shape_bounds(&mut bounds, &item.shape, false);
        let endpoints = match &item.shape {
            Shape::Line { start, end, .. } | Shape::Arc { start, end, .. } => Some((*start, *end)),
            Shape::Circle { .. } => {
                closed_primitives += 1;
                None
            }
            Shape::Polygon { points, .. } if points.len() >= 3 => {
                closed_primitives += 1;
                None
            }
            Shape::Text { .. } | Shape::Pad { .. } | Shape::Via { .. } | Shape::Polygon { .. } => {
                unsupported = true;
                None
            }
        };
        if let Some((start, end)) = endpoints {
            let a = point_key(start);
            let b = point_key(end);
            if a == b {
                unsupported = true;
                continue;
            }
            let edge = if a < b { (a, b) } else { (b, a) };
            if !edges.insert(edge) {
                unsupported = true;
            }
            *endpoint_degrees.entry(a).or_default() += 1;
            *endpoint_degrees.entry(b).or_default() += 1;
        }
    }
    let has_outline = !edges.is_empty() || closed_primitives > 0;
    let closed_segments = endpoint_degrees.values().all(|degree| *degree == 2);
    (bounds, has_outline && closed_segments && !unsupported)
}

fn include_shape_bounds(bounds: &mut Option<Bounds>, shape: &Shape, physical: bool) {
    let mut points = Vec::new();
    let stroke = match shape {
        Shape::Line { start, end, width } => {
            points.extend([*start, *end]);
            *width
        }
        Shape::Arc {
            start,
            mid,
            end,
            width,
        } => {
            points.extend(arc_bounds_points(*start, *mid, *end, physical));
            *width
        }
        Shape::Circle {
            center,
            radius,
            width,
        } => {
            points.extend([
                Point {
                    x: center.x - radius,
                    y: center.y - radius,
                },
                Point {
                    x: center.x + radius,
                    y: center.y + radius,
                },
            ]);
            *width
        }
        Shape::Polygon {
            points: polygon,
            width,
            ..
        } => {
            points.extend(polygon.iter().copied());
            *width
        }
        Shape::Text { .. } | Shape::Pad { .. } | Shape::Via { .. } => return,
    };
    for point in points {
        let half = stroke / 2.0;
        let corners = [
            Point {
                x: point.x - half,
                y: point.y - half,
            },
            Point {
                x: point.x + half,
                y: point.y + half,
            },
        ];
        for corner in corners {
            if let Some(current) = bounds {
                current.include(corner);
            } else {
                *bounds = Some(Bounds::from_point(corner));
            }
        }
    }
}

fn arc_bounds_points(start: Point, mid: Point, end: Point, physical: bool) -> Vec<Point> {
    let Some((center, radius, start_angle, mid_angle, end_angle)) =
        export_arc_circle(start, mid, end, physical)
    else {
        return vec![start, mid, end];
    };
    let ccw_end = positive_angle(end_angle - start_angle);
    let ccw_mid = positive_angle(mid_angle - start_angle);
    let sweep = if ccw_mid <= ccw_end {
        ccw_end
    } else {
        ccw_end - 2.0 * std::f64::consts::PI
    };
    let candidates = [
        0.0,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
        3.0 * std::f64::consts::FRAC_PI_2,
    ];
    let mut points = vec![start, mid, end];
    for angle in candidates {
        let distance = if sweep >= 0.0 {
            positive_angle(angle - start_angle)
        } else {
            -positive_angle(start_angle - angle)
        };
        if (sweep >= 0.0 && distance <= sweep) || (sweep < 0.0 && distance >= sweep) {
            points.push(Point {
                x: center.x + radius * angle.cos(),
                y: center.y + radius * angle.sin(),
            });
        }
    }
    points
}

fn arc_circle(start: Point, mid: Point, end: Point) -> Option<(Point, f64, f64, f64, f64)> {
    let determinant =
        2.0 * (start.x * (mid.y - end.y) + mid.x * (end.y - start.y) + end.x * (start.y - mid.y));
    if determinant.abs() < 1e-12 {
        return None;
    }
    let start_sq = start.x * start.x + start.y * start.y;
    let mid_sq = mid.x * mid.x + mid.y * mid.y;
    let end_sq = end.x * end.x + end.y * end.y;
    let center = Point {
        x: (start_sq * (mid.y - end.y) + mid_sq * (end.y - start.y) + end_sq * (start.y - mid.y))
            / determinant,
        y: (start_sq * (end.x - mid.x) + mid_sq * (start.x - end.x) + end_sq * (mid.x - start.x))
            / determinant,
    };
    let radius = ((start.x - center.x).powi(2) + (start.y - center.y).powi(2)).sqrt();
    if !point_is_supported(center) || !value_is_supported(radius) || radius <= 0.0 {
        return None;
    }
    Some((
        center,
        radius,
        (start.y - center.y).atan2(start.x - center.x),
        (mid.y - center.y).atan2(mid.x - center.x),
        (end.y - center.y).atan2(end.x - center.x),
    ))
}

fn positive_angle(value: f64) -> f64 {
    value.rem_euclid(2.0 * std::f64::consts::PI)
}

fn export_arc_circle(
    start: Point,
    mid: Point,
    end: Point,
    physical: bool,
) -> Option<(Point, f64, f64, f64, f64)> {
    if !physical {
        return arc_circle(start, mid, end);
    }
    let p = |q: Point| super::document::Point2 { x: q.x, y: q.y };
    let (center, radius, first, middle, last) =
        super::topology::arc_parameters(p(start), p(mid), p(end))?;
    Some((
        Point {
            x: center.x,
            y: center.y,
        },
        radius,
        first,
        middle,
        last,
    ))
}

fn fmt_num(value: f64) -> String {
    let rounded = if value.abs() < 0.0000005 { 0.0 } else { value };
    format!("{rounded:.6}")
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            character if !is_xml10_character(character) => escaped.push('\u{fffd}'),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn is_xml10_character(character: char) -> bool {
    matches!(character, '\u{9}' | '\u{a}' | '\u{d}')
        || matches!(character as u32, 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff)
}

fn has_invalid_xml10_character(value: &str) -> bool {
    value
        .chars()
        .any(|character| !is_xml10_character(character))
}

fn validate_svg_field(name: &str, value: &str) -> Result<(), CliError> {
    if value.len() > MAX_SVG_FIELD_BYTES {
        return Err(CliError::new(format!(
            "SVG {name} exceeds the {MAX_SVG_FIELD_BYTES}-byte field limit"
        )));
    }
    Ok(())
}

fn validate_svg_item(item: &Item, layer: &str) -> Result<(), CliError> {
    validate_svg_field("layer name", layer)?;
    if let Some(id) = item.id.as_deref() {
        validate_svg_field("item identifier", id)?;
    }
    if let Some(id) = item.component_id.as_deref() {
        validate_svg_field("component identifier", id)?;
    }
    match &item.shape {
        Shape::Text { text, .. } => validate_svg_field("text element", text),
        Shape::Pad {
            number, net_name, ..
        } => {
            validate_svg_field("pad number", number)?;
            if let Some(net_name) = net_name.as_deref() {
                validate_svg_field("net name", net_name)?;
            }
            Ok(())
        }
        Shape::Via { net_name, .. } => {
            if let Some(net_name) = net_name.as_deref() {
                validate_svg_field("net name", net_name)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn item_has_invalid_xml10_character(item: &Item, layer: &str) -> bool {
    layer
        .chars()
        .any(|character| !is_xml10_character(character))
        || item.id.as_deref().is_some_and(has_invalid_xml10_character)
        || item
            .component_id
            .as_deref()
            .is_some_and(has_invalid_xml10_character)
        || match &item.shape {
            Shape::Text { text, .. } => has_invalid_xml10_character(text),
            Shape::Pad {
                number, net_name, ..
            } => {
                has_invalid_xml10_character(number)
                    || net_name.as_deref().is_some_and(has_invalid_xml10_character)
            }
            Shape::Via { net_name, .. } => {
                net_name.as_deref().is_some_and(has_invalid_xml10_character)
            }
            _ => false,
        }
}

fn layer_color(layer: &str) -> &'static str {
    match layer {
        "Edge.Cuts" => "#202020",
        "F.Cu" => "#c04b28",
        "B.Cu" => "#3159a6",
        "F.SilkS" | "B.SilkS" => "#303030",
        "F.Mask" | "B.Mask" => "#369b68",
        "F.Paste" | "B.Paste" => "#747474",
        "F.Fab" | "B.Fab" => "#777777",
        _ => "#666666",
    }
}

fn layer_matches(pattern: &str, selected: &str) -> bool {
    if pattern == "*.Cu" {
        selected.ends_with(".Cu")
    } else if pattern == "*.Mask" {
        selected.ends_with(".Mask")
    } else if pattern == "*.Paste" {
        selected.ends_with(".Paste")
    } else if pattern == "*.SilkS" {
        selected.ends_with(".SilkS")
    } else {
        pattern == selected
    }
}

fn visible_layer<'a>(item: &Item, selected: &'a [String]) -> Option<&'a str> {
    selected
        .iter()
        .find(|selected_layer| {
            item.layers
                .iter()
                .any(|pattern| layer_matches(pattern, selected_layer))
        })
        .map(String::as_str)
}

fn arc_svg_path(start: Point, mid: Point, end: Point, physical: bool) -> Result<String, CliError> {
    let Some((_, radius, start_angle, mid_angle, end_angle)) =
        export_arc_circle(start, mid, end, physical)
    else {
        if physical {
            return Err(CliError::new(
                "SVG arc exceeds stable physical arc support; export refused",
            ));
        }
        return Ok(format!(
            "M {} {} L {} {} L {} {}",
            fmt_num(start.x),
            fmt_num(start.y),
            fmt_num(mid.x),
            fmt_num(mid.y),
            fmt_num(end.x),
            fmt_num(end.y)
        ));
    };
    let ccw_end = positive_angle(end_angle - start_angle);
    let ccw_mid = positive_angle(mid_angle - start_angle);
    let sweep = if ccw_mid <= ccw_end {
        ccw_end
    } else {
        2.0 * std::f64::consts::PI - ccw_end
    };
    let sweep_flag = if ccw_mid <= ccw_end { 1 } else { 0 };
    let large_arc = u8::from(sweep > std::f64::consts::PI);
    Ok(format!(
        "M {} {} A {} {} 0 {large_arc} {sweep_flag} {} {}",
        fmt_num(start.x),
        fmt_num(start.y),
        fmt_num(radius),
        fmt_num(radius),
        fmt_num(end.x),
        fmt_num(end.y)
    ))
}

fn render_item(item: &Item, layer: &str, bottom: bool, physical: bool) -> Result<String, CliError> {
    let color = layer_color(layer);
    let layer_attr = escape_xml(layer);
    let mut id_attr = item
        .id
        .as_deref()
        .map(|id| format!(" data-id=\"{}\"", escape_xml(id)))
        .unwrap_or_default();
    if let Some(id) = item.component_id.as_deref() {
        write!(id_attr, " data-component-id=\"{}\"", escape_xml(id)).unwrap();
    }
    let mut svg = String::new();
    match &item.shape {
        Shape::Line { start, end, width } => {
            writeln!(
                svg,
                "<path data-layer=\"{layer_attr}\"{id_attr} d=\"M {} {} L {} {}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
                fmt_num(start.x), fmt_num(start.y), fmt_num(end.x), fmt_num(end.y), fmt_num(*width)
            )
            .unwrap();
        }
        Shape::Arc {
            start,
            mid,
            end,
            width,
        } => {
            writeln!(
                svg,
                "<path data-layer=\"{layer_attr}\"{id_attr} d=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
                arc_svg_path(*start, *mid, *end, physical && layer == "Edge.Cuts")?, fmt_num(*width)
            )
            .unwrap();
        }
        Shape::Circle {
            center,
            radius,
            width,
        } => {
            writeln!(
                svg,
                "<circle data-layer=\"{layer_attr}\"{id_attr} cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\"/>",
                fmt_num(center.x), fmt_num(center.y), fmt_num(*radius), fmt_num(*width)
            )
            .unwrap();
        }
        Shape::Polygon {
            points,
            width,
            fill,
        } => {
            let mut path = String::new();
            for (index, point) in points.iter().enumerate() {
                write!(
                    path,
                    "{} {} {}",
                    if index == 0 { "M" } else { "L" },
                    fmt_num(point.x),
                    fmt_num(point.y)
                )
                .unwrap();
                path.push(' ');
            }
            if *fill || layer == "Edge.Cuts" {
                path.push('Z');
            }
            writeln!(
                svg,
                "<path data-layer=\"{layer_attr}\"{id_attr} d=\"{}\" fill=\"{}\" stroke=\"{color}\" stroke-width=\"{}\" fill-rule=\"evenodd\"/>",
                path,
                if *fill { color } else { "none" },
                fmt_num(*width)
            )
            .unwrap();
        }
        Shape::Text {
            text,
            at,
            rotation_deg,
            size_x_mm,
            size_y_mm,
            width,
        } => {
            let text = escape_xml(text);
            let transform = if *rotation_deg == 0.0 {
                String::new()
            } else {
                format!(
                    " transform=\"rotate({} {} {})\"",
                    fmt_num(*rotation_deg),
                    fmt_num(at.x),
                    fmt_num(at.y)
                )
            };
            let label = format!(
                "<text data-layer=\"{layer_attr}\"{id_attr} x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"{}\" text-anchor=\"middle\" dominant-baseline=\"central\" fill=\"{color}\" stroke=\"none\" data-source-font-height-mm=\"{}\" data-source-stroke-mm=\"{}\"{transform}>{text}</text>\n",
                fmt_num(at.x),
                fmt_num(at.y),
                fmt_num(*size_y_mm),
                fmt_num(*size_x_mm),
                fmt_num(*width)
            );
            if bottom {
                writeln!(
                    svg,
                    "<g transform=\"translate({} 0) scale(-1 1)\">{label}</g>",
                    fmt_num(2.0 * at.x)
                )
                .unwrap();
            } else {
                svg.push_str(&label);
            }
        }
        Shape::Pad {
            number,
            at,
            size_x_mm,
            size_y_mm,
            rotation_deg,
            shape,
            roundrect_ratio,
            drill,
            net_name,
        } => {
            let transform = if *rotation_deg == 0.0 {
                String::new()
            } else {
                format!(
                    " transform=\"rotate({} {} {})\"",
                    fmt_num(*rotation_deg),
                    fmt_num(at.x),
                    fmt_num(at.y)
                )
            };
            writeln!(
                svg,
                "<g data-layer=\"{layer_attr}\"{id_attr} data-pad=\"{}\" data-net=\"{}\"{transform}>",
                escape_xml(number),
                escape_xml(net_name.as_deref().unwrap_or(""))
            )
            .unwrap();
            let x = at.x - size_x_mm / 2.0;
            let y = at.y - size_y_mm / 2.0;
            match shape.as_str() {
                "circle" => writeln!(svg, "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{color}\"/>", fmt_num(at.x), fmt_num(at.y), fmt_num(size_x_mm / 2.0), fmt_num(size_y_mm / 2.0)).unwrap(),
                "oval" => writeln!(svg, "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{color}\"/>", fmt_num(x), fmt_num(y), fmt_num(*size_x_mm), fmt_num(*size_y_mm), fmt_num(size_x_mm.min(*size_y_mm) / 2.0)).unwrap(),
                "roundrect" => writeln!(svg, "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{color}\"/>", fmt_num(x), fmt_num(y), fmt_num(*size_x_mm), fmt_num(*size_y_mm), fmt_num(size_x_mm.min(*size_y_mm) * roundrect_ratio)).unwrap(),
                _ => writeln!(svg, "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{color}\"/>", fmt_num(x), fmt_num(y), fmt_num(*size_x_mm), fmt_num(*size_y_mm)).unwrap(),
            }
            if let Some(drill) = drill {
                writeln!(svg, "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"white\" data-drill-mm=\"{} {}\" data-drill-offset-mm=\"{} {}\"/>", fmt_num(at.x + drill.offset.x - drill.width_mm / 2.0), fmt_num(at.y + drill.offset.y - drill.height_mm / 2.0), fmt_num(drill.width_mm), fmt_num(drill.height_mm), fmt_num(drill.width_mm.min(drill.height_mm) / 2.0), fmt_num(drill.width_mm), fmt_num(drill.height_mm), fmt_num(drill.offset.x), fmt_num(drill.offset.y)).unwrap();
            }
            svg.push_str("</g>\n");
        }
        Shape::Via {
            at,
            size_mm,
            drill_mm,
            net_name,
        } => {
            writeln!(
                svg,
                "<g data-layer=\"{layer_attr}\"{id_attr} data-net=\"{}\"><circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{color}\"/><circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"white\" data-drill-mm=\"{}\"/></g>",
                escape_xml(net_name.as_deref().unwrap_or("")),
                fmt_num(at.x), fmt_num(at.y), fmt_num(size_mm / 2.0),
                fmt_num(at.x), fmt_num(at.y), fmt_num(drill_mm / 2.0), fmt_num(*drill_mm)
            )
            .unwrap();
        }
    }
    Ok(svg)
}

fn append_bounded(output: &mut String, contents: &str) -> Result<(), CliError> {
    if output.len().saturating_add(contents.len()) > MAX_SVG_BYTES {
        return Err(CliError::new(format!(
            "SVG output exceeds the {}-byte size limit",
            MAX_SVG_BYTES
        )));
    }
    output.push_str(contents);
    Ok(())
}

#[cfg(test)]
pub(super) fn render_svg(
    geometry: &Geometry,
    board_layers: &[String],
    side: &str,
    requested_layers: Option<&[String]>,
) -> Result<SvgOutput, CliError> {
    render_svg_projection(geometry, board_layers, side, requested_layers, true)
}

/// The caller has certified all saved physical boundary objects with the
/// canonical topology gate. Preserve legacy drawing bounds/bytes, but do not
/// reapply its endpoint-pair heuristic (which rejects complementary arcs).
pub(super) fn render_validated_svg(
    geometry: &Geometry,
    board_layers: &[String],
    side: &str,
    requested_layers: Option<&[String]>,
) -> Result<SvgOutput, CliError> {
    render_svg_projection(geometry, board_layers, side, requested_layers, false)
}

fn render_svg_projection(
    geometry: &Geometry,
    board_layers: &[String],
    side: &str,
    requested_layers: Option<&[String]>,
    require_legacy_closure: bool,
) -> Result<SvgOutput, CliError> {
    if !matches!(side, "top" | "bottom") {
        return Err(CliError::new("SVG side must be 'top' or 'bottom'"));
    }
    let bounds = if require_legacy_closure {
        geometry.outline
    } else {
        let mut bounds = None;
        for item in geometry
            .items
            .iter()
            .filter(|i| i.layers.iter().any(|l| l == "Edge.Cuts"))
        {
            include_shape_bounds(&mut bounds, &item.shape, true);
        }
        bounds
    }
    .ok_or_else(|| CliError::new("SVG export requires supported Edge.Cuts outline geometry"))?;
    if require_legacy_closure && !geometry.outline_closed {
        return Err(CliError::new(
            "SVG export requires Edge.Cuts line and arc segments to form closed contours",
        ));
    }
    if !bounds.width().is_finite()
        || !bounds.height().is_finite()
        || bounds.width() <= 0.0
        || bounds.height() <= 0.0
    {
        return Err(CliError::new(
            "Edge.Cuts outline has invalid or degenerate physical bounds",
        ));
    }
    let selected = if let Some(requested) = requested_layers {
        if requested.is_empty() {
            return Err(CliError::new("at least one SVG layer must be selected"));
        }
        for layer in requested {
            if !board_layers.iter().any(|candidate| candidate == layer) {
                return Err(CliError::new(format!(
                    "SVG layer is not declared by the board: {layer}"
                )));
            }
        }
        requested.to_vec()
    } else if side == "top" {
        ["F.Cu", "F.SilkS", "F.Fab", "Edge.Cuts"]
            .into_iter()
            .filter(|layer| board_layers.iter().any(|candidate| candidate == layer))
            .map(str::to_owned)
            .collect()
    } else {
        ["B.Cu", "B.SilkS", "B.Fab", "Edge.Cuts"]
            .into_iter()
            .filter(|layer| board_layers.iter().any(|candidate| candidate == layer))
            .map(str::to_owned)
            .collect()
    };
    let selected_metadata = selected.join(",");
    validate_svg_field("selected layer metadata", &selected_metadata)?;
    let view_box_x = bounds.min_x - OUTLINE_MARGIN_MM;
    let view_box_y = bounds.min_y - OUTLINE_MARGIN_MM;
    let view_box_width = bounds.width() + 2.0 * OUTLINE_MARGIN_MM;
    let view_box_height = bounds.height() + 2.0 * OUTLINE_MARGIN_MM;
    let mirror_sum = bounds.min_x + bounds.max_x;
    let mut output = String::new();
    append_bounded(
        &mut output,
        &format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"{}mm\" height=\"{}mm\" viewBox=\"{} {} {} {}\" data-units=\"mm\" data-coordinate-frame=\"KiCad-board\" data-side=\"{side}\" data-completeness=\"partial\">\n<title>Tokito board view ({side})</title>\n<metadata>Rust-native subset export; unsupported geometry is reported by Tokito CLI. Source Y increases downward. Selected layers: {}.</metadata>\n<g id=\"board-view\"{}>\n",
            fmt_num(view_box_width),
            fmt_num(view_box_height),
            fmt_num(view_box_x),
            fmt_num(view_box_y),
            fmt_num(view_box_width),
            fmt_num(view_box_height),
            escape_xml(&selected_metadata),
            if side == "bottom" {
                format!(
                    " transform=\"translate({} 0) scale(-1 1)\" data-view-transform=\"horizontal-mirror\"",
                    fmt_num(mirror_sum)
                )
            } else {
                String::new()
            }
        ),
    )?;
    let mut rendered_items = 0;
    let mut invalid_xml_characters_replaced = has_invalid_xml10_character(&selected_metadata);
    for item in &geometry.items {
        let Some(layer) = visible_layer(item, &selected) else {
            continue;
        };
        validate_svg_item(item, layer)?;
        invalid_xml_characters_replaced |= item_has_invalid_xml10_character(item, layer);
        let rendered = render_item(item, layer, side == "bottom", !require_legacy_closure)?;
        append_bounded(&mut output, &rendered)?;
        rendered_items += 1;
    }
    append_bounded(&mut output, "</g>\n</svg>\n")?;
    if rendered_items == 0 {
        return Err(CliError::new(
            "the selected SVG layers contain no supported geometry",
        ));
    }
    let mut warnings = geometry.diagnostics.clone();
    if !require_legacy_closure && !geometry.outline_closed {
        warnings.retain(|warning| {
            warning != "Edge.Cuts segments do not form closed contours; SVG export is blocked"
        });
    }
    if invalid_xml_characters_replaced {
        warnings.push(
            "invalid XML 1.0 control characters were replaced with U+FFFD in SVG text or metadata"
                .to_owned(),
        );
    }
    Ok(SvgOutput {
        contents: output,
        rendered_items,
        warnings,
        width_mm: view_box_width,
        height_mm: view_box_height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str, footprints: &[Footprint]) -> Geometry {
        let document =
            super::super::parse_document(source.as_bytes()).expect("valid test S-expression");
        parse_geometry(
            &document,
            footprints,
            &super::super::document::EngineeringData::from_source(&document),
        )
    }

    fn parse_with_footprints(source: &str) -> Geometry {
        let document = super::super::parse_document(source.as_bytes()).expect("test document");
        let footprints = document
            .children("footprint")
            .map(super::super::read_footprint)
            .collect::<Vec<_>>();
        parse_geometry(
            &document,
            &footprints,
            &super::super::document::EngineeringData::from_source(&document),
        )
    }

    #[test]
    fn saved_footprint_xy_is_relative_and_pad_angles_are_absolute_on_both_sides() {
        for layer in ["F.Cu", "B.Cu"] {
            let source = format!(
                r#"(kicad_pcb
                (gr_rect (start 0 0) (end 30 30) (layer "Edge.Cuts"))
                (footprint "Test:Part" (layer "{layer}") (at 10 20 90) (uuid "part")
                    (pad "1" smd rect (at 2 3 30) (size 2 1) (layers "{layer}"))))"#
            );
            let geometry = parse_with_footprints(&source);
            let item = geometry
                .items
                .iter()
                .find(|item| matches!(item.shape, Shape::Pad { .. }))
                .expect("pad");
            let Shape::Pad {
                at, rotation_deg, ..
            } = item.shape
            else {
                panic!("pad")
            };
            assert!((at.x - 13.0).abs() < 1e-9);
            assert!((at.y - 18.0).abs() < 1e-9);
            assert_eq!(rotation_deg, -30.0);
            let svg = render_svg(
                &geometry,
                &[layer.to_owned(), "Edge.Cuts".to_owned()],
                "top",
                Some(&[layer.to_owned()]),
            )
            .expect("SVG");
            assert!(
                svg.contents
                    .contains("rotate(-30.000000 13.000000 18.000000)")
            );
        }
    }

    #[test]
    fn honors_hidden_text_and_offset_slot_drills() {
        let source = r#"(kicad_pcb
            (gr_rect (start 0 0) (end 20 20) (layer "Edge.Cuts"))
            (footprint "Test:Part" (layer "F.Cu") (at 5 5) (uuid "part")
                (property "Reference" "HIDDEN" (at 0 0) (layer "F.SilkS") (hide yes))
                (property "Value" "hide" (at 1 1) (layer "F.SilkS"))
                (fp_text user "LEGACY_HIDDEN" (at 0 0) (layer "F.SilkS") (effects (font (size 1 1)) hide))
                (pad "1" thru_hole oval (at 0 0) (size 4 3) (drill oval 2 1 (offset 0.5 -0.25)) (layers "*.Cu"))))"#;
        let geometry = parse_with_footprints(source);
        let layers = vec![
            "F.Cu".to_owned(),
            "F.SilkS".to_owned(),
            "Edge.Cuts".to_owned(),
        ];
        let svg = render_svg(&geometry, &layers, "top", None).expect("SVG");
        assert!(!svg.contents.contains("HIDDEN"));
        assert!(svg.contents.contains(">hide</text>"));
        assert!(svg.contents.contains(
            "x=\"4.500000\" y=\"4.250000\" width=\"2.000000\" height=\"1.000000\" rx=\"0.500000\""
        ));
        assert!(
            svg.contents
                .contains("data-drill-offset-mm=\"0.500000 -0.250000\"")
        );
    }

    #[test]
    fn replaces_xml_control_characters_and_bounds_metadata_fields() {
        let source = r#"(kicad_pcb
            (gr_rect (start 0 0) (end 20 20) (layer "Edge.Cuts"))
            (gr_text "A\x01B<&" (at 5 5) (layer "F.SilkS")))"#;
        let mut geometry = parse(source, &[]);
        let layers = vec!["F.SilkS".to_owned(), "Edge.Cuts".to_owned()];
        let svg = render_svg(&geometry, &layers, "top", None).expect("SVG");
        assert!(svg.contents.contains("A\u{fffd}B&lt;&amp;"));
        assert!(!svg.contents.contains('\u{1}'));
        assert!(
            svg.warnings
                .iter()
                .any(|message| message.contains("XML 1.0"))
        );
        geometry.items[0].id = Some("x".repeat(MAX_SVG_FIELD_BYTES + 1));
        assert!(render_svg(&geometry, &layers, "top", None).is_err());
    }

    #[test]
    fn rejects_open_or_extreme_outlines_and_nonpositive_pads() {
        let layers = vec!["F.Cu".to_owned(), "Edge.Cuts".to_owned()];
        for outline in [
            "(gr_line (start 0 0) (end 20 20) (layer \"Edge.Cuts\"))",
            "(gr_rect (start 1e100 1e100) (end 2e100 2e100) (layer \"Edge.Cuts\"))",
        ] {
            let geometry = parse(&format!("(kicad_pcb {outline})"), &[]);
            assert!(render_svg(&geometry, &layers, "top", None).is_err());
        }
        let geometry = parse_with_footprints(
            r#"(kicad_pcb
            (gr_rect (start 0 0) (end 20 20) (layer "Edge.Cuts"))
            (footprint "Test:Part" (layer "F.Cu") (at 5 5) (uuid "part")
                (pad "1" smd rect (at 0 0) (size -1 1) (layers "F.Cu"))))"#,
        );
        assert!(
            geometry
                .diagnostics
                .iter()
                .any(|message| message.contains("non-positive"))
        );
        assert!(
            !geometry
                .items
                .iter()
                .any(|item| matches!(item.shape, Shape::Pad { .. }))
        );
    }

    #[test]
    fn emits_physical_layer_aware_svg_and_mirrored_bottom_view() {
        let source = r#"(kicad_pcb
            (version 20240108)
            (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (44 "Edge.Cuts" user))
            (gr_rect (start 10 20) (end 30 40) (stroke (width 0.1) (type default)) (layer "Edge.Cuts"))
            (segment (start 12 22) (end 20 22) (width 0.25) (layer "F.Cu") (net 1))
            (via (at 20 22) (size 0.8) (drill 0.4) (layers "F.Cu" "B.Cu") (net 1))
            (arc (start 12 30) (mid 15 27) (end 18 30) (width 0.25) (layer "F.Cu") (net 1))
            (zone (filled_polygon (layer "F.Cu") (pts (xy 13 23) (xy 15 23) (xy 15 25) (xy 13 25))))
        )"#;
        let geometry = parse(source, &[]);
        let layers = vec!["F.Cu".to_owned(), "B.Cu".to_owned(), "Edge.Cuts".to_owned()];
        let top = render_svg(&geometry, &layers, "top", None).expect("top SVG");
        let bottom = render_svg(&geometry, &layers, "bottom", None).expect("bottom SVG");
        assert!(top.contents.contains("width=\"21.100000mm\""));
        assert!(top.contents.contains("data-layer=\"F.Cu\""));
        assert!(top.contents.contains(" A "));
        assert!(
            bottom
                .contents
                .contains("data-view-transform=\"horizontal-mirror\"")
        );
        assert!(bottom.contents.contains("data-completeness=\"partial\""));
    }

    #[test]
    fn reports_unrendered_custom_pads_instead_of_claiming_full_coverage() {
        let source = r#"(kicad_pcb (version 20240108) (layers (0 "F.Cu" signal) (44 "Edge.Cuts" user))
            (gr_rect (start 0 0) (end 10 10) (layer "Edge.Cuts"))
            (footprint "Test:Part" (layer "F.Cu") (at 5 5) (uuid "footprint-id")
                (property "Reference" "U1" (at 0 0 0) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))))
                (pad "1" smd custom (at 0 0) (size 1 1) (layers "F.Cu") (net 1 "DATA")))
        )"#;
        let document =
            super::super::parse_document(source.as_bytes()).expect("valid test S-expression");
        let footprint_node = document.children("footprint").next().expect("footprint");
        let footprint = super::super::read_footprint(footprint_node);
        let geometry = parse_geometry(
            &document,
            &[footprint],
            &super::super::document::EngineeringData::from_source(&document),
        );
        assert!(
            geometry
                .diagnostics
                .iter()
                .any(|message| message.contains("unsupported custom geometry"))
        );
        let svg = render_svg(
            &geometry,
            &[
                "F.Cu".to_owned(),
                "F.SilkS".to_owned(),
                "Edge.Cuts".to_owned(),
            ],
            "top",
            None,
        )
        .expect("partial SVG");
        assert!(svg.contents.contains("data-completeness=\"partial\""));
        assert!(
            svg.warnings
                .iter()
                .any(|message| message.contains("unsupported custom geometry"))
        );
    }
}
