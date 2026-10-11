//! Conservative physical Edge.Cuts validation, independent of drawing stroke.
//!
//! Units are millimetres. Joins use Euclidean distance <= 1e-6 mm, without
//! quantization or transitive welding. Positive signed area is clockwise in the
//! saved Y-down board frame. Curved area, bounds and containment are analytic.
//! This is an outline gate, not a CAD kernel, copper solver or triangulator.

use crate::document::{CoordinateFrame, EngineeringData, Graphic, Point2, SourceLocator};
use std::f64::consts::{PI, TAU};

pub const JOIN_TOLERANCE_MM: f64 = 1e-6;
const MAX_PRIMITIVES: usize = 4096;
const MAX_PAIR_WORK: usize = 2_000_000;
const MAX_DIAGNOSTICS: usize = 200;
const MAX_COORDINATE_MM: f64 = 1e6;

#[derive(Clone, Debug, PartialEq)]
pub enum OutlineShape {
    Line {
        start: Point2,
        end: Point2,
    },
    Arc {
        start: Point2,
        mid: Point2,
        end: Point2,
    },
    Circle {
        center: Point2,
        radius_mm: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutlinePrimitive {
    pub source: SourceLocator,
    pub shape: OutlineShape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContourRole {
    Outer,
    Cutout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContourEdge {
    pub primitive_index: usize,
    pub reversed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutlineContour {
    pub edges: Vec<ContourEdge>,
    pub role: ContourRole,
    /// Positive is clockwise in the source Y-down frame; no winding is forced.
    pub signed_area_mm2: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutlineBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutlineDiagnosticCode {
    MissingOutline,
    InvalidSource,
    UnsupportedPrimitive,
    DegeneratePrimitive,
    AmbiguousJoin,
    OpenContour,
    Branch,
    Intersection,
    Overlap,
    DisconnectedOuter,
    NestedIsland,
    AmbiguousContainment,
    WorkBudgetExceeded,
    DiagnosticsTruncated,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutlineDiagnostic {
    pub code: OutlineDiagnosticCode,
    pub source: Option<SourceLocator>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutlineReport {
    pub primitives: Vec<OutlinePrimitive>,
    pub contours: Vec<OutlineContour>,
    pub bounds: Option<OutlineBounds>,
    pub diagnostics: Vec<OutlineDiagnostic>,
    pub join_tolerance_mm: f64,
}

impl OutlineReport {
    /// Assess endpoint chaining only; never repairs or certifies an outline.
    pub fn compare_chaining(
        &self,
        tolerance_mm: f64,
    ) -> Result<serde_json::Value, crate::EngineError> {
        if ![0.001, 0.01, 0.1].contains(&tolerance_mm) {
            return Err(crate::EngineError::new(
                "comparison tolerance must be 0.001, 0.01 or 0.1 mm",
            ));
        }
        let mut points = Vec::new();
        for (primitive, item) in self.primitives.iter().enumerate() {
            if let Some((start, end)) = endpoints(&item.shape) {
                points.push((primitive, false, start));
                points.push((primitive, true, end));
            }
        }
        let mut work = 0;
        let mut peers = vec![Vec::new(); points.len()];
        for i in 0..points.len() {
            for j in i + 1..points.len() {
                if !work_tick(&mut work) {
                    return Err(crate::EngineError::new(
                        "chaining comparison work budget exceeded",
                    ));
                }
                if points[i].0 != points[j].0 && distance(points[i].2, points[j].2) <= tolerance_mm
                {
                    peers[i].push(j);
                    peers[j].push(i);
                }
            }
        }
        let mut joins = Vec::new();
        let mut missing = Vec::new();
        let mut ambiguous = Vec::new();
        for (i, candidates) in peers.iter().enumerate() {
            let endpoint = serde_json::json!({"primitiveIndex":points[i].0,"endpoint":if points[i].1 {"end"} else {"start"}});
            if candidates.is_empty() {
                missing.push(endpoint);
            } else if candidates.len() != 1 {
                ambiguous.push(endpoint);
            } else {
                let j = candidates[0];
                if i < j && peers[j].len() == 1 {
                    joins.push(serde_json::json!({"a":endpoint,"b":{"primitiveIndex":points[j].0,"endpoint":if points[j].1 {"end"} else {"start"}},"gapMm":distance(points[i].2,points[j].2)}));
                }
            }
        }
        Ok(
            serde_json::json!({"schema":"tokito.outline-chaining-comparison.v1","toleranceMm":tolerance_mm,
            "strictToleranceMm":self.join_tolerance_mm,"endpointCount":points.len(),"uniqueJoinCandidates":joins,
            "unmatchedEndpoints":missing,"ambiguousEndpoints":ambiguous,
            "allEndpointsHaveUniqueMatch":!points.is_empty()&&missing.is_empty()&&ambiguous.is_empty(),
            "strictOutlineValid":self.is_valid(),"physicalOutlineValidated":false,"sourceCoordinatesChanged":false,
            "workSteps":work,"scope":"endpoint-distance compatibility only; not contour, intersection, area or mesh validation"}),
        )
    }
    pub fn is_valid(&self) -> bool {
        self.diagnostics.is_empty() && !self.contours.is_empty()
    }
    /// Unsupported source records may invalidate a previously built report.
    /// Keep primitives/diagnostics for inspection, but never publish certified
    /// derived geometry when the complete outline did not pass validation.
    pub(crate) fn discard_invalid_geometry(&mut self) {
        if !self.is_valid() {
            self.bounds = None;
            self.contours.clear();
        }
    }
    fn fail(&mut self, code: OutlineDiagnosticCode, source: Option<&SourceLocator>, message: &str) {
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            self.diagnostics.push(OutlineDiagnostic {
                code,
                source: source.cloned(),
                message: message.into(),
            });
        } else if self
            .diagnostics
            .last()
            .is_some_and(|d| d.code != OutlineDiagnosticCode::DiagnosticsTruncated)
        {
            self.diagnostics.push(OutlineDiagnostic {
                code: OutlineDiagnosticCode::DiagnosticsTruncated,
                source: None,
                message: "Further outline diagnostics omitted; validation failed.".into(),
            });
        }
    }
}

fn p(x: f64, y: f64) -> Point2 {
    Point2 { x, y }
}
fn sub(a: Point2, b: Point2) -> Point2 {
    p(a.x - b.x, a.y - b.y)
}
fn add(a: Point2, b: Point2) -> Point2 {
    p(a.x + b.x, a.y + b.y)
}
fn mul(a: Point2, b: f64) -> Point2 {
    p(a.x * b, a.y * b)
}
fn dot(a: Point2, b: Point2) -> f64 {
    a.x * b.x + a.y * b.y
}
fn cross(a: Point2, b: Point2) -> f64 {
    a.x * b.y - a.y * b.x
}
fn length(a: Point2) -> f64 {
    a.x.hypot(a.y)
}
fn distance(a: Point2, b: Point2) -> f64 {
    length(sub(a, b))
}
fn valid_point(a: Point2) -> bool {
    a.x.is_finite()
        && a.y.is_finite()
        && a.x.abs() <= MAX_COORDINATE_MM
        && a.y.abs() <= MAX_COORDINATE_MM
}
fn near(a: Point2, b: Point2) -> bool {
    distance(a, b) <= JOIN_TOLERANCE_MM
}

#[derive(Clone, Copy, Debug)]
struct ArcData {
    center: Point2,
    radius: f64,
    start: f64,
    sweep: f64,
}
impl ArcData {
    fn point(self, t: f64) -> Point2 {
        add(self.center, p(self.radius * t.cos(), self.radius * t.sin()))
    }
    fn contains_angle(self, angle: f64, tolerance: f64) -> bool {
        let d = if self.sweep > 0.0 {
            (angle - self.start).rem_euclid(TAU)
        } else {
            (self.start - angle).rem_euclid(TAU)
        };
        d <= self.sweep.abs() + tolerance || TAU - d <= tolerance
    }
    fn contains_point(self, point: Point2) -> bool {
        self.contains_angle(
            (point.y - self.center.y).atan2(point.x - self.center.x),
            JOIN_TOLERANCE_MM / self.radius,
        )
    }
    fn reversed(self) -> Self {
        Self {
            start: self.start + self.sweep,
            sweep: -self.sweep,
            ..self
        }
    }
}

fn arc_data(start: Point2, mid: Point2, end: Point2) -> Option<ArcData> {
    // Translate before squaring: global PCB coordinates must not destroy the
    // small circumcircle through cancellation of large squared values.
    let a = sub(mid, start);
    let b = sub(end, start);
    let scale = length(a).max(length(b));
    let det = 2.0 * cross(a, b);
    if scale <= JOIN_TOLERANCE_MM
        || distance(mid, end) <= JOIN_TOLERANCE_MM
        || distance(start, end) <= JOIN_TOLERANCE_MM
        || det.abs() <= scale * scale * 1e-12
    {
        return None;
    }
    let local = p(
        (dot(a, a) * b.y - dot(b, b) * a.y) / det,
        (a.x * dot(b, b) - b.x * dot(a, a)) / det,
    );
    let center = add(start, local);
    let radius = length(local);
    if !valid_point(center)
        || !radius.is_finite()
        || !(JOIN_TOLERANCE_MM..=MAX_COORDINATE_MM).contains(&radius)
    {
        return None;
    }
    let angle = |q: Point2| (q.y - center.y).atan2(q.x - center.x);
    let first = angle(start);
    let middle = (angle(mid) - first).rem_euclid(TAU);
    let finish = (angle(end) - first).rem_euclid(TAU);
    let sweep = if middle < finish {
        finish
    } else {
        finish - TAU
    };
    if sweep.abs() <= 1e-12 || (distance(center, mid) - radius).abs() > JOIN_TOLERANCE_MM {
        return None;
    }
    Some(ArcData {
        center,
        radius,
        start: first,
        sweep,
    })
}

/// Shared stable circle calculation for physical SVG, kept separate from the
/// frozen v0.1 drawing-inventory projection.
pub(crate) fn arc_parameters(
    start: Point2,
    mid: Point2,
    end: Point2,
) -> Option<(Point2, f64, f64, f64, f64)> {
    let arc = arc_data(start, mid, end)?;
    let angle = |q: Point2| (q.y - arc.center.y).atan2(q.x - arc.center.x);
    Some((arc.center, arc.radius, angle(start), angle(mid), angle(end)))
}

fn transform(g: &Graphic, e: &EngineeringData, q: Option<Point2>) -> Option<Point2> {
    let q = q?;
    if !valid_point(q) {
        return None;
    }
    let q = match g.frame {
        CoordinateFrame::Board => q,
        CoordinateFrame::ComponentLocal { component_ordinal } => e
            .components
            .iter()
            .find(|c| c.source.ordinal == component_ordinal)?
            .source_board_pose
            .local_to_board(q)?,
    };
    valid_point(q).then_some(q)
}

pub(crate) fn from_engineering(engineering: &EngineeringData) -> OutlineReport {
    from_engineering_with_tolerance(engineering, JOIN_TOLERANCE_MM)
}

pub(crate) fn from_engineering_with_tolerance(
    engineering: &EngineeringData,
    tolerance_mm: f64,
) -> OutlineReport {
    let mut report = OutlineReport {
        primitives: Vec::new(),
        contours: Vec::new(),
        bounds: None,
        diagnostics: Vec::new(),
        join_tolerance_mm: tolerance_mm,
    };
    for pad in &engineering.pads {
        if pad.layers.iter().any(|layer| layer == "Edge.Cuts") {
            report.fail(
                OutlineDiagnosticCode::UnsupportedPrimitive,
                Some(&pad.source),
                "A pad on Edge.Cuts cannot be silently omitted from physical outline validation.",
            );
        }
    }
    for component in &engineering.components {
        for property in &component.properties {
            if property.layer.as_deref() == Some("Edge.Cuts") {
                report.fail(
                    OutlineDiagnosticCode::UnsupportedPrimitive,
                    Some(&property.source),
                    "A property on Edge.Cuts is not a supported physical boundary.",
                );
            }
        }
    }
    for zone in &engineering.zones {
        if zone.layers.iter().any(|layer| layer == "Edge.Cuts")
            || zone
                .boundary_polygons
                .iter()
                .chain(&zone.cached_fill_polygons)
                .any(|poly| poly.layer.as_deref() == Some("Edge.Cuts"))
        {
            report.fail(
                OutlineDiagnosticCode::UnsupportedPrimitive,
                Some(&zone.source),
                "A zone on Edge.Cuts is not a supported physical boundary.",
            );
        }
    }
    for (source_index, graphic) in engineering
        .graphics
        .iter()
        .filter(|g| g.layer.as_deref() == Some("Edge.Cuts"))
        .enumerate()
    {
        if source_index >= MAX_PRIMITIVES || report.primitives.len() >= MAX_PRIMITIVES {
            report.fail(
                OutlineDiagnosticCode::WorkBudgetExceeded,
                Some(&graphic.source),
                "Outline primitive budget exceeded.",
            );
            break;
        }
        let start = transform(graphic, engineering, graphic.start_mm);
        let end = transform(graphic, engineering, graphic.end_mm);
        let shape = match graphic.kind.as_str() {
            "gr_line" | "fp_line" => start
                .zip(end)
                .map(|(start, end)| OutlineShape::Line { start, end }),
            "gr_arc" | "fp_arc" => start
                .zip(transform(graphic, engineering, graphic.mid_mm))
                .zip(end)
                .map(|((start, mid), end)| OutlineShape::Arc { start, mid, end }),
            "gr_circle" | "fp_circle" => transform(graphic, engineering, graphic.center_mm)
                .zip(end)
                .map(|(center, end)| OutlineShape::Circle {
                    center,
                    radius_mm: distance(center, end),
                }),
            "gr_rect" | "fp_rect" => {
                // Construct corners in the saved local frame, then transform
                // each corner. Rotated rectangles are not axis-aligned boxes.
                if let Some((a, b)) = graphic.start_mm.zip(graphic.end_mm) {
                    let corners = [a, p(b.x, a.y), b, p(a.x, b.y)];
                    let points = corners
                        .into_iter()
                        .map(|q| transform(graphic, engineering, Some(q)))
                        .collect::<Option<Vec<_>>>();
                    if let Some(points) = points {
                        if report.primitives.len() + 4 > MAX_PRIMITIVES {
                            report.fail(
                                OutlineDiagnosticCode::WorkBudgetExceeded,
                                Some(&graphic.source),
                                "Rectangle exceeds outline primitive budget.",
                            );
                            break;
                        }
                        for i in 0..4 {
                            report.primitives.push(OutlinePrimitive {
                                source: graphic.source.clone(),
                                shape: OutlineShape::Line {
                                    start: points[i],
                                    end: points[(i + 1) % 4],
                                },
                            });
                        }
                        continue;
                    }
                }
                None
            }
            "gr_poly" | "fp_poly" => {
                if let Some(poly) = &graphic.polygon {
                    if poly.is_complete && poly.points_mm.len() >= 3 {
                        if poly.points_mm.len() > MAX_PRIMITIVES + 1 {
                            report.fail(
                                OutlineDiagnosticCode::WorkBudgetExceeded,
                                Some(&graphic.source),
                                "Polygon exceeds outline primitive budget.",
                            );
                            break;
                        }
                        let points = poly
                            .points_mm
                            .iter()
                            .map(|q| transform(graphic, engineering, *q))
                            .collect::<Option<Vec<_>>>();
                        if let Some(mut points) = points {
                            if points.len() > 3 && points.first() == points.last() {
                                points.pop();
                            }
                            if points.len() + report.primitives.len() > MAX_PRIMITIVES {
                                report.fail(
                                    OutlineDiagnosticCode::WorkBudgetExceeded,
                                    Some(&graphic.source),
                                    "Polygon exceeds outline primitive budget.",
                                );
                                break;
                            }
                            for i in 0..points.len() {
                                report.primitives.push(OutlinePrimitive {
                                    source: graphic.source.clone(),
                                    shape: OutlineShape::Line {
                                        start: points[i],
                                        end: points[(i + 1) % points.len()],
                                    },
                                });
                            }
                            continue;
                        }
                    }
                }
                None
            }
            _ => {
                report.fail(OutlineDiagnosticCode::UnsupportedPrimitive,Some(&graphic.source),"Unsupported Edge.Cuts object remains in source inventory; outline cannot be certified.");
                continue;
            }
        };
        if let Some(shape) = shape {
            report.primitives.push(OutlinePrimitive {
                source: graphic.source.clone(),
                shape,
            });
        } else {
            report.fail(
                OutlineDiagnosticCode::InvalidSource,
                Some(&graphic.source),
                "Missing, malformed, unbounded or unsupported legacy outline coordinates/pose.",
            );
        }
    }
    let mut report = validate(report);
    report.discard_invalid_geometry();
    report
}

fn endpoints(shape: &OutlineShape) -> Option<(Point2, Point2)> {
    match *shape {
        OutlineShape::Line { start, end } | OutlineShape::Arc { start, end, .. } => {
            Some((start, end))
        }
        OutlineShape::Circle { .. } => None,
    }
}
fn circular(shape: &OutlineShape) -> Option<ArcData> {
    match *shape {
        OutlineShape::Arc { start, mid, end } => arc_data(start, mid, end),
        OutlineShape::Circle { center, radius_mm } => Some(ArcData {
            center,
            radius: radius_mm,
            start: 0.0,
            sweep: TAU,
        }),
        _ => None,
    }
}

#[derive(Default)]
struct Contacts {
    points: Vec<Point2>,
    overlap: bool,
    ambiguous: bool,
}
fn same_circle(a: ArcData, b: ArcData) -> bool {
    near(a.center, b.center) && (a.radius - b.radius).abs() <= JOIN_TOLERANCE_MM
}
fn angular_intervals(a: ArcData) -> Vec<(f64, f64)> {
    if a.sweep.abs() >= TAU - 1e-12 {
        return vec![(0.0, TAU)];
    }
    let start = if a.sweep > 0.0 {
        a.start
    } else {
        a.start + a.sweep
    };
    let start = start.rem_euclid(TAU);
    let end = start + a.sweep.abs();
    if end <= TAU {
        vec![(start, end)]
    } else {
        vec![(start, TAU), (0.0, end - TAU)]
    }
}
fn contacts(a: &OutlineShape, b: &OutlineShape) -> Contacts {
    match (a, b) {
        (OutlineShape::Line { start: a0, end: a1 }, OutlineShape::Line { start: b0, end: b1 }) => {
            line_line(*a0, *a1, *b0, *b1)
        }
        (OutlineShape::Line { start, end }, _) => {
            line_circle(*start, *end, circular(b).expect("validated curve"))
        }
        (_, OutlineShape::Line { start, end }) => {
            line_circle(*start, *end, circular(a).expect("validated curve"))
        }
        _ => circle_circle(
            circular(a).expect("validated curve"),
            circular(b).expect("validated curve"),
        ),
    }
}
fn line_line(a: Point2, b: Point2, c: Point2, d: Point2) -> Contacts {
    let mut out = Contacts::default();
    let u = sub(b, a);
    let v = sub(d, c);
    let w = sub(c, a);
    let denominator = cross(u, v);
    let lu = length(u);
    let lv = length(v);
    if denominator.abs() <= 1e-12 * lu * lv {
        let t0 = dot(w, u) / dot(u, u);
        let t1 = dot(sub(d, a), u) / dot(u, u);
        let low = t0.min(t1).max(0.0);
        let high = t0.max(t1).min(1.0);
        if cross(w, u).abs() / lu <= JOIN_TOLERANCE_MM {
            if high > low {
                out.overlap = true;
            } else if high >= low - JOIN_TOLERANCE_MM / lu {
                out.points.push(add(a, mul(u, (low + high) / 2.0)));
            }
        } else if cross(w, u).abs() / lu <= JOIN_TOLERANCE_MM * 2.0
            && high >= low - JOIN_TOLERANCE_MM * 2.0 / lu
        {
            // Close infinite support lines do not imply contact between
            // finite segments whose projected intervals are distant.
            out.ambiguous = true;
        }
        return out;
    }
    let t = cross(w, v) / denominator;
    let s = cross(w, u) / denominator;
    if t >= -JOIN_TOLERANCE_MM / lu
        && t <= 1.0 + JOIN_TOLERANCE_MM / lu
        && s >= -JOIN_TOLERANCE_MM / lv
        && s <= 1.0 + JOIN_TOLERANCE_MM / lv
    {
        out.points.push(add(a, mul(u, t)));
    }
    out
}
fn line_circle(start: Point2, end: Point2, arc: ArcData) -> Contacts {
    let mut out = Contacts::default();
    let u = sub(end, start);
    let len = length(u);
    let unit = mul(u, 1.0 / len);
    let q = sub(arc.center, start);
    let projection = dot(q, unit);
    let perpendicular = cross(q, unit).abs();
    let delta = arc.radius - perpendicular;
    if delta < -JOIN_TOLERANCE_MM {
        return out;
    }
    if delta.abs() <= JOIN_TOLERANCE_MM {
        let point = add(start, mul(unit, projection));
        if projection >= -JOIN_TOLERANCE_MM
            && projection <= len + JOIN_TOLERANCE_MM
            && arc.contains_point(point)
        {
            out.points.push(point);
            out.ambiguous = true;
        }
        return out;
    }
    let offset = ((arc.radius - perpendicular) * (arc.radius + perpendicular)).sqrt();
    for t in [projection - offset, projection + offset] {
        let point = add(start, mul(unit, t));
        if t >= -JOIN_TOLERANCE_MM && t <= len + JOIN_TOLERANCE_MM && arc.contains_point(point) {
            out.points.push(point);
        }
    }
    out
}
fn circle_circle(a: ArcData, b: ArcData) -> Contacts {
    let mut out = Contacts::default();
    if same_circle(a, b) {
        let angular_tolerance = JOIN_TOLERANCE_MM / a.radius.min(b.radius);
        for (a0, a1) in angular_intervals(a) {
            for (b0, b1) in angular_intervals(b) {
                let low = a0.max(b0);
                let high = a1.min(b1);
                if high > low {
                    out.overlap = true;
                }
            }
        }
        for angle in [a.start, a.start + a.sweep, b.start, b.start + b.sweep] {
            if a.contains_angle(angle, angular_tolerance)
                && b.contains_angle(angle, angular_tolerance)
            {
                let point = a.point(angle);
                if !out.points.iter().any(|other| near(*other, point)) {
                    out.points.push(point);
                }
            }
        }
        return out;
    }
    let delta = sub(b.center, a.center);
    let d = length(delta);
    if d <= JOIN_TOLERANCE_MM {
        return out;
    }
    let outer = d - a.radius - b.radius;
    let inner = d - (a.radius - b.radius).abs();
    if outer > JOIN_TOLERANCE_MM || inner < -JOIN_TOLERANCE_MM {
        return out;
    }
    let along = (d * d + (a.radius - b.radius) * (a.radius + b.radius)) / (2.0 * d);
    let height2 = (a.radius - along) * (a.radius + along);
    if height2 < -JOIN_TOLERANCE_MM * a.radius {
        return out;
    }
    let unit = mul(delta, 1.0 / d);
    let base = add(a.center, mul(unit, along));
    let h = height2.max(0.0).sqrt();
    for sign in [-1.0, 1.0] {
        let point = add(base, mul(p(-unit.y, unit.x), h * sign));
        if a.contains_point(point)
            && b.contains_point(point)
            && !out.points.iter().any(|other| near(*other, point))
        {
            out.points.push(point);
        }
    }
    if (outer.abs() <= JOIN_TOLERANCE_MM || inner.abs() <= JOIN_TOLERANCE_MM)
        && !out.points.is_empty()
    {
        out.ambiguous = true;
    }
    out
}

fn work_tick(work: &mut usize) -> bool {
    *work += 1;
    *work <= MAX_PAIR_WORK
}

fn validate(mut report: OutlineReport) -> OutlineReport {
    if report.primitives.len() > MAX_PRIMITIVES {
        report.fail(
            OutlineDiagnosticCode::WorkBudgetExceeded,
            None,
            "Outline primitive budget exceeded.",
        );
        return report;
    }
    if report.primitives.is_empty() {
        report.fail(
            OutlineDiagnosticCode::MissingOutline,
            None,
            "No physical Edge.Cuts primitives found.",
        );
        return report;
    }
    // Source errors fail closed, never validate a subset of the board boundary.
    if !report.diagnostics.is_empty() {
        return report;
    }
    for i in 0..report.primitives.len() {
        let primitive = &report.primitives[i];
        let valid = match primitive.shape {
            OutlineShape::Line { start, end } => {
                valid_point(start) && valid_point(end) && distance(start, end) > JOIN_TOLERANCE_MM
            }
            OutlineShape::Arc { start, mid, end } => {
                valid_point(start)
                    && valid_point(mid)
                    && valid_point(end)
                    && arc_data(start, mid, end).is_some()
            }
            OutlineShape::Circle { center, radius_mm } => {
                valid_point(center)
                    && radius_mm.is_finite()
                    && radius_mm > JOIN_TOLERANCE_MM
                    && radius_mm <= MAX_COORDINATE_MM
            }
        };
        if !valid {
            let source = primitive.source.clone();
            report.fail(
                OutlineDiagnosticCode::DegeneratePrimitive,
                Some(&source),
                "Zero-length, non-finite, unbounded or unstable outline primitive.",
            );
        }
    }
    if !report.diagnostics.is_empty() {
        return report;
    }
    let mut endpoints_list = Vec::new();
    for (index, primitive) in report.primitives.iter().enumerate() {
        if let Some((a, b)) = endpoints(&primitive.shape) {
            endpoints_list.push((index, false, a));
            endpoints_list.push((index, true, b));
        }
    }
    let mut work = 0;
    let mut neighbours = vec![Vec::new(); endpoints_list.len()];
    let mut nearest = vec![None::<(f64, usize)>; endpoints_list.len()];
    for i in 0..endpoints_list.len() {
        for j in i + 1..endpoints_list.len() {
            if !work_tick(&mut work) {
                report.fail(
                    OutlineDiagnosticCode::WorkBudgetExceeded,
                    None,
                    "Endpoint comparison budget exceeded.",
                );
                return report;
            }
            let gap = distance(endpoints_list[i].2, endpoints_list[j].2);
            // Reuse the bounded comparison pass; diagnostics never weld or
            // mutate endpoints. Exclude the other end of the same primitive.
            if endpoints_list[i].0 != endpoints_list[j].0 {
                if nearest[i].is_none_or(|(best, _)| gap < best) {
                    nearest[i] = Some((gap, j));
                }
                if nearest[j].is_none_or(|(best, _)| gap < best) {
                    nearest[j] = Some((gap, i));
                }
            }
            if gap <= report.join_tolerance_mm {
                neighbours[i].push(j);
                neighbours[j].push(i);
            }
        }
    }
    for (i, peers) in neighbours.iter().enumerate() {
        let source = report.primitives[endpoints_list[i].0].source.clone();
        if peers.is_empty() {
            let point = endpoints_list[i].2;
            let nearest_detail = nearest[i]
                .map(|(gap, other)| {
                    format!(
                        " Nearest other primitive {} {} endpoint is {:.9} mm away.",
                        endpoints_list[other].0,
                        if endpoints_list[other].1 {
                            "end"
                        } else {
                            "start"
                        },
                        gap
                    )
                })
                .unwrap_or_else(|| " No other primitive endpoint exists.".to_owned());
            let message = format!(
                "Outline {} endpoint at ({:.9}, {:.9}) mm has no match within {:.9} mm.{} Source coordinates were not changed.",
                if endpoints_list[i].1 { "end" } else { "start" },
                point.x,
                point.y,
                report.join_tolerance_mm,
                nearest_detail
            );
            report.fail(OutlineDiagnosticCode::OpenContour, Some(&source), &message);
        } else if peers.len() > 1 {
            report.fail(
                OutlineDiagnosticCode::Branch,
                Some(&source),
                "An outline endpoint has multiple possible joins; no transitive welding performed.",
            );
        }
    }
    if !report.diagnostics.is_empty() {
        return report;
    }
    let mut joins = vec![None; report.primitives.len() * 2];
    for (i, (edge, end, _)) in endpoints_list.iter().enumerate() {
        let (other, other_end, _) = endpoints_list[neighbours[i][0]];
        joins[edge * 2 + usize::from(*end)] = Some((other, other_end));
    }
    for i in 0..report.primitives.len() {
        for j in i + 1..report.primitives.len() {
            if !work_tick(&mut work) {
                report.fail(
                    OutlineDiagnosticCode::WorkBudgetExceeded,
                    None,
                    "Primitive intersection comparison budget exceeded.",
                );
                return report;
            }
            let contact = contacts(&report.primitives[i].shape, &report.primitives[j].shape);
            let allowed = |point: Point2| {
                endpoints(&report.primitives[i].shape)
                    .zip(endpoints(&report.primitives[j].shape))
                    .is_some_and(|((a, b), (c, d))| {
                        [(a, false), (b, true)].iter().any(|(pa, ea)| {
                            [(c, false), (d, true)].iter().any(|(pb, eb)| {
                                distance(*pa, point) <= report.join_tolerance_mm
                                    && distance(*pb, point) <= report.join_tolerance_mm
                                    && joins[i * 2 + usize::from(*ea)] == Some((j, *eb))
                            })
                        })
                    })
            };
            let source = report.primitives[i].source.clone();
            if contact.overlap {
                report.fail(
                    OutlineDiagnosticCode::Overlap,
                    Some(&source),
                    "Outline primitives overlap or duplicate a positive-length boundary.",
                );
            } else if contact.ambiguous
                && (contact.points.is_empty() || contact.points.iter().any(|q| !allowed(*q)))
            {
                report.fail(
                    OutlineDiagnosticCode::AmbiguousJoin,
                    Some(&source),
                    "Near-tangent outline contact is ambiguous.",
                );
            } else if contact.points.iter().any(|q| !allowed(*q)) {
                report.fail(
                    OutlineDiagnosticCode::Intersection,
                    Some(&source),
                    "Outline primitives cross or touch outside a unique shared endpoint.",
                );
            }
        }
    }
    if !report.diagnostics.is_empty() {
        return report;
    }
    let mut used = vec![false; report.primitives.len()];
    for index in 0..report.primitives.len() {
        if used[index] {
            continue;
        }
        let mut edges = Vec::new();
        if endpoints(&report.primitives[index].shape).is_none() {
            used[index] = true;
            edges.push(ContourEdge {
                primitive_index: index,
                reversed: false,
            });
        } else {
            let mut current = index;
            let mut reversed = false;
            loop {
                if used[current] {
                    break;
                }
                used[current] = true;
                edges.push(ContourEdge {
                    primitive_index: current,
                    reversed,
                });
                let (next, enter_end) =
                    joins[current * 2 + usize::from(!reversed)].expect("validated endpoint");
                current = next;
                reversed = enter_end;
            }
            if current != index || reversed {
                report.fail(
                    OutlineDiagnosticCode::AmbiguousJoin,
                    None,
                    "Contour traversal did not close with consistent orientation.",
                );
                return report;
            }
        }
        let area_origin = sample_point(&report.primitives[edges[0].primitive_index].shape);
        let mut area = edges
            .iter()
            .map(|edge| {
                signed_area(
                    &report.primitives[edge.primitive_index].shape,
                    edge.reversed,
                    area_origin,
                )
            })
            .sum::<f64>();
        // A tolerance-connected contour includes short straight connections
        // between saved endpoints. Include their line integrals rather than
        // describing an open primitive sum as the exact enclosed area.
        if report.join_tolerance_mm > JOIN_TOLERANCE_MM {
            for k in 0..edges.len() {
                let a = &edges[k];
                let b = &edges[(k + 1) % edges.len()];
                if let Some(((a0, a1), (b0, b1))) =
                    endpoints(&report.primitives[a.primitive_index].shape)
                        .zip(endpoints(&report.primitives[b.primitive_index].shape))
                {
                    let end = if a.reversed { a0 } else { a1 };
                    let start = if b.reversed { b1 } else { b0 };
                    area += cross(sub(end, area_origin), sub(start, area_origin)) / 2.;
                }
            }
        }
        if !area.is_finite() || area.abs() <= JOIN_TOLERANCE_MM * JOIN_TOLERANCE_MM {
            report.fail(
                OutlineDiagnosticCode::DegeneratePrimitive,
                None,
                "Contour has zero or unstable enclosed area.",
            );
            return report;
        }
        report.contours.push(OutlineContour {
            edges,
            role: ContourRole::Outer,
            signed_area_mm2: area,
        });
    }
    let mut depths = Vec::new();
    for i in 0..report.contours.len() {
        let edge = report.contours[i].edges[0];
        let q = sample_point(&report.primitives[edge.primitive_index].shape);
        let mut depth = 0;
        for j in 0..report.contours.len() {
            if i != j {
                work = work.saturating_add(report.contours[j].edges.len());
                if work > MAX_PAIR_WORK {
                    report.fail(
                        OutlineDiagnosticCode::WorkBudgetExceeded,
                        None,
                        "Contour containment comparison budget exceeded.",
                    );
                    return report;
                }
                if contour_contains(q, &report.contours[j], &report.primitives) {
                    depth += 1;
                }
            }
        }
        depths.push(depth);
    }
    if depths.iter().filter(|depth| **depth == 0).count() != 1 {
        report.fail(
            OutlineDiagnosticCode::DisconnectedOuter,
            None,
            "A physical board requires exactly one outer contour.",
        );
    }
    if depths.iter().any(|depth| *depth > 1) {
        report.fail(
            OutlineDiagnosticCode::NestedIsland,
            None,
            "Nested cutout islands are unsupported; outline fails closed.",
        );
    }
    if !report.diagnostics.is_empty() {
        return report;
    }
    for (contour, depth) in report.contours.iter_mut().zip(depths) {
        contour.role = if depth == 0 {
            ContourRole::Outer
        } else {
            ContourRole::Cutout
        };
    }
    let mut bounds = None;
    for primitive in &report.primitives {
        for q in extrema(&primitive.shape) {
            let b = bounds.get_or_insert(OutlineBounds {
                min_x: q.x,
                min_y: q.y,
                max_x: q.x,
                max_y: q.y,
            });
            b.min_x = b.min_x.min(q.x);
            b.min_y = b.min_y.min(q.y);
            b.max_x = b.max_x.max(q.x);
            b.max_y = b.max_y.max(q.y);
        }
    }
    report.bounds = bounds;
    report
}

fn signed_area(shape: &OutlineShape, reversed: bool, origin: Point2) -> f64 {
    let area = match *shape {
        OutlineShape::Line { start, end } => cross(sub(start, origin), sub(end, origin)) / 2.0,
        OutlineShape::Arc { start, mid, end } => {
            let a = arc_data(start, mid, end).expect("validated arc");
            cross(sub(start, origin), sub(end, origin)) / 2.0
                + a.radius * a.radius * sweep_minus_sin(a.sweep) / 2.0
        }
        OutlineShape::Circle { radius_mm, .. } => PI * radius_mm * radius_mm,
    };
    if reversed { -area } else { area }
}

fn sweep_minus_sin(angle: f64) -> f64 {
    if angle.abs() > 0.1 {
        return angle - angle.sin();
    }
    // The direct subtraction loses the area of shallow, large-radius arcs.
    // This convergent small-angle expression retains the circular segment.
    let square = angle * angle;
    angle
        * square
        * (1.0 / 6.0
            - square
                * (1.0 / 120.0
                    - square * (1.0 / 5040.0 - square * (1.0 / 362880.0 - square / 39916800.0))))
}
fn sample_point(shape: &OutlineShape) -> Point2 {
    match *shape {
        OutlineShape::Line { start, .. } | OutlineShape::Arc { start, .. } => start,
        OutlineShape::Circle { center, radius_mm } => p(center.x + radius_mm, center.y),
    }
}
fn extrema(shape: &OutlineShape) -> Vec<Point2> {
    match *shape {
        OutlineShape::Line { start, end } => vec![start, end],
        _ => {
            let a = circular(shape).expect("validated curve");
            let mut points = vec![a.point(a.start), a.point(a.start + a.sweep)];
            for angle in [0.0, PI / 2.0, PI, 3.0 * PI / 2.0] {
                if a.contains_angle(angle, 0.0) {
                    points.push(a.point(angle));
                }
            }
            points
        }
    }
}

fn contour_contains(q: Point2, contour: &OutlineContour, primitives: &[OutlinePrimitive]) -> bool {
    let mut winding = 0;
    for edge in &contour.edges {
        match primitives[edge.primitive_index].shape {
            OutlineShape::Line { mut start, mut end } => {
                if edge.reversed {
                    std::mem::swap(&mut start, &mut end);
                }
                if (start.y <= q.y && end.y > q.y) || (end.y <= q.y && start.y > q.y) {
                    let x = start.x + (q.y - start.y) * (end.x - start.x) / (end.y - start.y);
                    if x > q.x {
                        winding += if end.y > start.y { 1 } else { -1 };
                    }
                }
            }
            _ => {
                let mut a =
                    circular(&primitives[edge.primitive_index].shape).expect("validated curve");
                if edge.reversed {
                    a = a.reversed();
                }
                // Split at exact vertical extrema into y-monotone circular
                // intervals. Half-open endpoint rule matches line crossings.
                let mut cuts = vec![0.0, 1.0];
                for angle in [PI / 2.0, 3.0 * PI / 2.0] {
                    let distance = if a.sweep > 0.0 {
                        (angle - a.start).rem_euclid(TAU)
                    } else {
                        (a.start - angle).rem_euclid(TAU)
                    };
                    let t = distance / a.sweep.abs();
                    if t > 0.0 && t < 1.0 {
                        cuts.push(t);
                    }
                }
                cuts.sort_by(f64::total_cmp);
                for interval in cuts.windows(2) {
                    let start = a.point(a.start + a.sweep * interval[0]);
                    let end = a.point(a.start + a.sweep * interval[1]);
                    if (start.y <= q.y && end.y > q.y) || (end.y <= q.y && start.y > q.y) {
                        let dy = q.y - a.center.y;
                        let dx = ((a.radius - dy) * (a.radius + dy)).max(0.0).sqrt();
                        let middle = a.start + a.sweep * (interval[0] + interval[1]) / 2.0;
                        let x = a.center.x + if middle.cos() >= 0.0 { dx } else { -dx };
                        if x > q.x {
                            winding += if end.y > start.y { 1 } else { -1 };
                        }
                    }
                }
            }
        }
    }
    winding != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(index: usize) -> SourceLocator {
        SourceLocator {
            kind: "test".into(),
            ordinal: index,
            component_ordinal: None,
            source_uuid: None,
            source_path: vec![index],
        }
    }
    fn report(shapes: Vec<OutlineShape>) -> OutlineReport {
        validate(OutlineReport {
            primitives: shapes
                .into_iter()
                .enumerate()
                .map(|(i, shape)| OutlinePrimitive {
                    source: source(i),
                    shape,
                })
                .collect(),
            contours: vec![],
            bounds: None,
            diagnostics: vec![],
            join_tolerance_mm: JOIN_TOLERANCE_MM,
        })
    }
    fn polygon(points: &[(f64, f64)]) -> Vec<OutlineShape> {
        (0..points.len())
            .map(|i| OutlineShape::Line {
                start: p(points[i].0, points[i].1),
                end: p(
                    points[(i + 1) % points.len()].0,
                    points[(i + 1) % points.len()].1,
                ),
            })
            .collect()
    }
    #[test]
    fn bow_tie_is_not_a_physical_board() {
        let r = report(polygon(&[(0., 0.), (10., 10.), (0., 10.), (10., 0.)]));
        assert!(!r.is_valid());
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == OutlineDiagnosticCode::Intersection)
        );
    }
    #[test]
    fn rectangle_and_hole_have_physical_bounds() {
        let mut shapes = polygon(&[(0., 0.), (10., 0.), (10., 8.), (0., 8.)]);
        shapes.push(OutlineShape::Circle {
            center: p(5., 4.),
            radius_mm: 1.,
        });
        let r = report(shapes);
        assert!(r.is_valid(), "{:?}", r.diagnostics);
        assert_eq!(r.contours.len(), 2);
        assert_eq!(r.contours[1].role, ContourRole::Cutout);
        assert_eq!(
            r.bounds,
            Some(OutlineBounds {
                min_x: 0.,
                min_y: 0.,
                max_x: 10.,
                max_y: 8.
            })
        );
        assert_eq!(r.contours[0].signed_area_mm2, 80.);
    }
    #[test]
    fn chaining_comparison_keeps_strict_refusal_and_reports_ambiguity() {
        let r = report(vec![
            OutlineShape::Line {
                start: p(0., 0.),
                end: p(1., 0.),
            },
            OutlineShape::Line {
                start: p(1.005, 0.),
                end: p(0.005, 0.),
            },
        ]);
        assert!(!r.is_valid());
        let tight = r.compare_chaining(0.001).unwrap();
        assert_eq!(tight["allEndpointsHaveUniqueMatch"], false);
        let standard = r.compare_chaining(0.01).unwrap();
        assert_eq!(standard["allEndpointsHaveUniqueMatch"], true);
        assert_eq!(standard["strictOutlineValid"], false);
        assert_eq!(standard["physicalOutlineValidated"], false);
        assert_eq!(standard["sourceCoordinatesChanged"], false);
        assert!(r.compare_chaining(f64::NAN).is_err());
        assert!(r.compare_chaining(1.).is_err());
        let ambiguous = report(vec![
            OutlineShape::Line {
                start: p(0., 0.),
                end: p(1., 0.),
            },
            OutlineShape::Line {
                start: p(1.005, 0.),
                end: p(0.005, 0.),
            },
            OutlineShape::Line {
                start: p(1.006, 0.),
                end: p(0.006, 0.),
            },
        ]);
        let c = ambiguous.compare_chaining(0.01).unwrap();
        assert_eq!(c["allEndpointsHaveUniqueMatch"], false);
        assert!(!c["ambiguousEndpoints"].as_array().unwrap().is_empty());
    }
    #[test]
    fn open_endpoint_diagnostic_measures_gap_without_repairing_source() {
        let shapes = vec![
            OutlineShape::Line {
                start: p(0., 0.),
                end: p(10., 0.),
            },
            OutlineShape::Line {
                start: p(10.0002, 0.),
                end: p(10., 8.),
            },
            OutlineShape::Line {
                start: p(10., 8.),
                end: p(0., 8.),
            },
            OutlineShape::Line {
                start: p(0., 8.),
                end: p(0., 0.),
            },
        ];
        let r = report(shapes.clone());
        assert!(!r.is_valid());
        let gaps: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.code == OutlineDiagnosticCode::OpenContour)
            .collect();
        assert_eq!(gaps.len(), 2);
        assert!(
            gaps.iter()
                .all(|d| d.message.contains("0.000200000 mm away"))
        );
        assert!(
            gaps.iter()
                .all(|d| d.message.contains("Source coordinates were not changed"))
        );
        assert_eq!(
            r.primitives
                .iter()
                .map(|p| p.shape.clone())
                .collect::<Vec<_>>(),
            shapes
        );
    }
    #[test]
    fn complementary_arcs_form_a_circle() {
        let r = report(vec![
            OutlineShape::Arc {
                start: p(-2., 0.),
                mid: p(0., -2.),
                end: p(2., 0.),
            },
            OutlineShape::Arc {
                start: p(2., 0.),
                mid: p(0., 2.),
                end: p(-2., 0.),
            },
        ]);
        assert!(r.is_valid(), "{:?}", r.diagnostics);
        assert!((r.contours[0].signed_area_mm2 - 4. * PI).abs() < 1e-10);
    }
    #[test]
    fn duplicate_arc_and_line_overlap_fail() {
        let a = OutlineShape::Arc {
            start: p(-2., 0.),
            mid: p(0., -2.),
            end: p(2., 0.),
        };
        assert!(!report(vec![a.clone(), a]).is_valid());
        let r = line_line(p(0., 0.), p(10., 0.), p(2., 0.), p(8., 0.));
        assert!(r.overlap);
    }
    #[test]
    fn translated_arc_keeps_small_radius() {
        let a = arc_data(p(100000., 100002.), p(100002., 100000.), p(100000., 99998.)).unwrap();
        assert!((a.radius - 2.).abs() < 1e-10);
        assert_eq!(a.center, p(100000., 100000.));
    }
    #[test]
    fn disconnected_and_nested_contours_fail() {
        let c = |center, radius_mm| OutlineShape::Circle { center, radius_mm };
        assert!(!report(vec![c(p(0., 0.), 2.), c(p(10., 0.), 2.)]).is_valid());
        let r = report(vec![c(p(0., 0.), 8.), c(p(0., 0.), 4.), c(p(0., 0.), 1.)]);
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == OutlineDiagnosticCode::NestedIsland)
        );
    }
    #[test]
    fn tangent_hole_and_crossing_curves_fail() {
        assert!(
            !report(vec![
                OutlineShape::Circle {
                    center: p(0., 0.),
                    radius_mm: 5.
                },
                OutlineShape::Circle {
                    center: p(4., 0.),
                    radius_mm: 1.
                }
            ])
            .is_valid()
        );
        assert!(
            !report(vec![
                OutlineShape::Circle {
                    center: p(0., 0.),
                    radius_mm: 3.
                },
                OutlineShape::Circle {
                    center: p(3., 0.),
                    radius_mm: 3.
                }
            ])
            .is_valid()
        );
    }
    #[test]
    fn arc_and_line_contacts_are_analytic() {
        let a = arc_data(p(-2., 0.), p(0., 2.), p(2., 0.)).unwrap();
        let hits = line_circle(p(-3., 1.), p(3., 1.), a);
        assert_eq!(hits.points.len(), 2);
        for q in hits.points {
            assert!((q.x.abs() - 3f64.sqrt()).abs() < 1e-12);
        }
    }
    #[test]
    fn zero_and_unbounded_primitives_fail() {
        assert!(
            !report(vec![OutlineShape::Line {
                start: p(0., 0.),
                end: p(0., 0.)
            }])
            .is_valid()
        );
        assert!(
            !report(vec![OutlineShape::Circle {
                center: p(1e7, 0.),
                radius_mm: 1.
            }])
            .is_valid()
        );
    }

    #[test]
    fn tiny_translated_contour_keeps_physical_area() {
        let origin = 100000.0;
        let side = 0.001;
        let r = report(polygon(&[
            (origin, origin),
            (origin + side, origin),
            (origin + side, origin + side),
            (origin, origin + side),
        ]));
        assert!(r.is_valid(), "{:?}", r.diagnostics);
        let actual_side = (origin + side) - origin;
        assert!(
            (r.contours[0].signed_area_mm2 - actual_side * actual_side).abs() < 1e-14,
            "{}",
            r.contours[0].signed_area_mm2
        );
    }

    #[test]
    fn sub_join_tolerance_overlap_is_still_overlap() {
        let contact = line_line(
            p(0., 0.),
            p(1., 0.),
            p(1. - JOIN_TOLERANCE_MM / 2., 0.),
            p(2., 0.),
        );
        assert!(
            contact.overlap,
            "A join tolerance cannot authorize positive length overlap"
        );
        let a = ArcData {
            center: p(0., 0.),
            radius: 1.,
            start: 0.,
            sweep: PI,
        };
        let b = ArcData {
            center: p(0., 0.),
            radius: 1.,
            start: PI - JOIN_TOLERANCE_MM / 2.,
            sweep: PI,
        };
        assert!(
            circle_circle(a, b).overlap,
            "Angular overlap cannot be hidden by join tolerance"
        );
    }

    #[test]
    fn curved_contour_ray_through_join_vertex_keeps_winding() {
        let r = report(vec![
            OutlineShape::Arc {
                start: p(-2., 0.),
                mid: p(0., -2.),
                end: p(2., 0.),
            },
            OutlineShape::Arc {
                start: p(2., 0.),
                mid: p(0., 2.),
                end: p(-2., 0.),
            },
        ]);
        assert!(r.is_valid());
        assert!(contour_contains(p(0., 0.), &r.contours[0], &r.primitives));
        assert!(!contour_contains(p(3., 0.), &r.contours[0], &r.primitives));
    }

    fn engineering(text: &str) -> EngineeringData {
        EngineeringData::from_source(&crate::parse_document(text.as_bytes()).unwrap())
    }
    #[test]
    fn saved_component_rectangle_uses_canonical_pose_without_second_flip() {
        let data = engineering(
            "(kicad_pcb (footprint x (layer B.Cu) (at 10 20 90) (fp_rect (start 0 0) (end 2 3) (stroke (width 9)) (layer Edge.Cuts))))",
        );
        let r = from_engineering(&data);
        assert!(r.is_valid(), "{:?}", r.diagnostics);
        assert_eq!(r.primitives.len(), 4);
        let b = r.bounds.unwrap();
        assert!(
            (b.min_x - 10.).abs() < 1e-12
                && (b.max_x - 13.).abs() < 1e-12
                && (b.min_y - 18.).abs() < 1e-12
                && (b.max_y - 20.).abs() < 1e-12
        );
        assert!(
            r.primitives
                .iter()
                .all(|primitive| primitive.source.component_ordinal.is_some())
        );
        assert!((r.contours[0].signed_area_mm2 - 6.).abs() < 1e-12);
    }
    #[test]
    fn unsupported_and_invalid_source_outline_never_validate_a_subset() {
        for fragment in [
            "(gr_curve (layer Edge.Cuts))",
            "(gr_arc (start 0 0) (end 1 0) (angle 90) (layer Edge.Cuts))",
            "(footprint x (at 0 0) (pad 1 smd circle (at 1 1) (size 1 1) (layers Edge.Cuts)))",
            "(footprint x (at 0 0) (property Reference R1 (layer Edge.Cuts)))",
            "(zone (layer Edge.Cuts))",
        ] {
            let data = engineering(&format!(
                "(kicad_pcb (gr_rect (start 0 0) (end 10 10) (layer Edge.Cuts)) {fragment})"
            ));
            let r = from_engineering(&data);
            assert!(!r.is_valid(), "{fragment}");
            assert!(r.diagnostics.iter().any(|d| d.source.is_some()));
        }
    }
    #[test]
    fn endpoint_pair_work_budget_fails_closed() {
        let shapes = (0..1100)
            .map(|i| OutlineShape::Line {
                start: p(f64::from(i) * 2., 0.),
                end: p(f64::from(i) * 2. + 1., 0.),
            })
            .collect();
        let r = report(shapes);
        assert!(!r.is_valid());
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == OutlineDiagnosticCode::WorkBudgetExceeded)
        );
        assert!(r.diagnostics.len() <= MAX_DIAGNOSTICS + 1);
    }

    #[test]
    fn shallow_arc_area_does_not_cancel_sine() {
        let shape = OutlineShape::Arc {
            start: p(0., 0.),
            mid: p(0.5, 1e-6),
            end: p(1., 0.),
        };
        let a = circular(&shape).unwrap();
        let expected = a.radius * a.radius * a.sweep.powi(3) / 12.;
        let area = signed_area(&shape, false, p(0., 0.));
        assert!((area - expected).abs() < 1e-16, "{area} != {expected}");
    }
}
