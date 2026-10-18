//! Bounded SMD, plated-hole, and non-plated round-hole footprint writer.
//!
//! This mirrors the observable *separation* used by KiCad Library Tools:
//! typed geometry nodes are validated and serialized in a stable order. It does
//! not implement the upstream IPC-7351 dimension calculator or KLC rule set.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

const MAX_SPEC_BYTES: usize = 64 * 1024;
const MAX_PADS: usize = 512;
const MAX_GRAPHICS: usize = 2048;
const MAX_POLYGON_POINTS: usize = 256;
const MAX_EVIDENCE: usize = 16;
const MAX_EXTENT_MM: f64 = 100.0;
const UM_PER_MM: f64 = 1_000_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct FootprintGeneration {
    pub text: String,
    pub report: GenerationReport,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationReport {
    pub schema: &'static str,
    pub status: &'static str,
    pub footprint_name: String,
    pub side: &'static str,
    pub pad_type: &'static str,
    pub pad_count: usize,
    pub graphic_count: usize,
    pub source_evidence: Vec<SourceEvidence>,
    pub source_evidence_content_verified: bool,
    pub manufacturer_land_pattern_qualified: bool,
    pub component_body_generated: bool,
    pub component_model_qualified: bool,
    pub board_placement_verified: bool,
    pub input_sha256: String,
    pub output_sha256: String,
    pub limitations: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SourceEvidence {
    pub label: String,
    pub uri: String,
    pub sha256: String,
    pub claim: EvidenceClaim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClaim {
    PackageDimensions,
    RecommendedLandPattern,
    LibraryGeometry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Side {
    Front,
    Back,
}

impl Side {
    fn copper(self) -> &'static str {
        match self {
            Self::Front => "F.Cu",
            Self::Back => "B.Cu",
        }
    }

    fn mask(self) -> &'static str {
        match self {
            Self::Front => "F.Mask",
            Self::Back => "B.Mask",
        }
    }

    fn paste(self) -> &'static str {
        match self {
            Self::Front => "F.Paste",
            Self::Back => "B.Paste",
        }
    }

    fn silk(self) -> &'static str {
        match self {
            Self::Front => "F.SilkS",
            Self::Back => "B.SilkS",
        }
    }

    fn fab(self) -> &'static str {
        match self {
            Self::Front => "F.Fab",
            Self::Back => "B.Fab",
        }
    }

    fn courtyard(self) -> &'static str {
        match self {
            Self::Front => "F.CrtYd",
            Self::Back => "B.CrtYd",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Back => "back",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FootprintSpec {
    schema: String,
    name: String,
    description: String,
    tags: Vec<String>,
    side: Side,
    reference: TextProperty,
    value: TextProperty,
    pads: Vec<PadSpec>,
    graphics: Vec<GraphicSpec>,
    source_evidence: Vec<SourceEvidence>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TextProperty {
    text: String,
    at_mm: [f64; 3],
    size_mm: [f64; 2],
    thickness_mm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PadShape {
    Circle,
    Rect,
    Oval,
    #[serde(rename = "roundrect")]
    RoundRect,
}

impl PadShape {
    fn as_str(self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::Rect => "rect",
            Self::Oval => "oval",
            Self::RoundRect => "roundrect",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PadType {
    Smd,
    ThruHole,
    NpThruHole,
}

impl PadType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Smd => "smd",
            Self::ThruHole => "thru_hole",
            Self::NpThruHole => "np_thru_hole",
        }
    }

    fn candidate_status(self) -> &'static str {
        match self {
            Self::Smd => "explicit-smd-geometry-candidate",
            Self::ThruHole => "explicit-through-hole-geometry-candidate",
            Self::NpThruHole => "explicit-non-plated-hole-geometry-candidate",
        }
    }

    fn footprint_attribute(self) -> &'static str {
        match self {
            Self::Smd => "smd",
            Self::ThruHole => "through_hole",
            Self::NpThruHole => "through_hole",
        }
    }
}

fn footprint_attribute(pads: &[PadSpec]) -> &'static str {
    // KiCad's footprint-level type is binary. Its native GetLikelyAttribute
    // prefers through-hole when a footprint contains any plated-hole pad,
    // even when it also contains SMD pads. NPTH-only records do not count as
    // plated pads, so preserve the existing through-hole classification for
    // a mechanical-only candidate while allowing SMD + NPTH to remain SMD.
    if pads.iter().any(|pad| pad.pad_type == PadType::ThruHole) {
        PadType::ThruHole.footprint_attribute()
    } else if pads.iter().any(|pad| pad.pad_type == PadType::Smd) {
        PadType::Smd.footprint_attribute()
    } else {
        PadType::NpThruHole.footprint_attribute()
    }
}

fn pad_type_summary(pads: &[PadSpec]) -> (&'static str, &'static str) {
    let first = pads[0].pad_type;
    if pads.iter().all(|pad| pad.pad_type == first) {
        (first.as_str(), first.candidate_status())
    } else {
        ("mixed", "explicit-mixed-pad-geometry-candidate")
    }
}

fn default_pad_type() -> PadType {
    PadType::Smd
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PadSpec {
    number: String,
    shape: PadShape,
    #[serde(default = "default_pad_type")]
    pad_type: PadType,
    at_mm: [f64; 3],
    size_mm: [f64; 2],
    layers: Vec<String>,
    #[serde(default)]
    drill_mm: Option<f64>,
    #[serde(default)]
    drill_oval_mm: Option<[f64; 2]>,
    #[serde(default)]
    drill_offset_mm: Option<[f64; 2]>,
    #[serde(default)]
    roundrect_ratio: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum GraphicSpec {
    Line {
        start_mm: [f64; 2],
        end_mm: [f64; 2],
        width_mm: f64,
        layer: String,
    },
    Rectangle {
        start_mm: [f64; 2],
        end_mm: [f64; 2],
        width_mm: f64,
        layer: String,
    },
    Circle {
        center_mm: [f64; 2],
        radius_mm: f64,
        width_mm: f64,
        layer: String,
    },
    Polygon {
        points_mm: Vec<[f64; 2]>,
        width_mm: f64,
        layer: String,
        filled: bool,
    },
}

/// Parse strict JSON, validate the explicit geometry and return a deterministic
/// single-footprint KiCad file. This function performs no path or network IO.
pub fn generate(input: &[u8]) -> Result<FootprintGeneration, String> {
    if input.is_empty() || input.len() > MAX_SPEC_BYTES {
        return Err(format!(
            "specification must be between 1 and {MAX_SPEC_BYTES} bytes"
        ));
    }
    let text = std::str::from_utf8(input).map_err(|_| "specification must be UTF-8".to_owned())?;
    let value =
        crate::strict_json::parse(text).map_err(|error| format!("invalid strict JSON: {error}"))?;
    let spec: FootprintSpec = serde_json::from_value(value)
        .map_err(|error| format!("invalid footprint specification: {error}"))?;
    validate(&spec)?;
    let serialized = serialize(&spec)?;
    let (pad_type, status) = pad_type_summary(&spec.pads);
    let report = GenerationReport {
        schema: "tokito.footprint-generation-report.v1",
        status,
        footprint_name: spec.name,
        side: spec.side.as_str(),
        pad_type,
        pad_count: spec.pads.len(),
        graphic_count: spec.graphics.len(),
        source_evidence: spec.source_evidence,
        source_evidence_content_verified: false,
        manufacturer_land_pattern_qualified: false,
        component_body_generated: false,
        component_model_qualified: false,
        board_placement_verified: false,
        input_sha256: crate::sha256_hex(input),
        output_sha256: crate::sha256_hex(serialized.as_bytes()),
        limitations: vec![
            "source evidence labels and digest strings are preserved, but external files are not fetched or independently verified",
            "only explicit SMD, plated through-hole (round or oval drill, including offsets), and centered round non-plated-hole pads are supported; pad technologies may be mixed, but custom pads are not implemented",
            "graphics support lines, rectangles, circles, and simple filled or unfilled polygons with bounded, non-self-intersecting outlines",
            "IPC-7351 pad calculation, zones, groups, 3D model references, and KLC rules are not implemented",
            "manufacturer land-pattern qualification, component body generation, board placement, and physical fit are not asserted",
        ],
    };
    Ok(FootprintGeneration {
        text: serialized,
        report,
    })
}

fn validate(spec: &FootprintSpec) -> Result<(), String> {
    if spec.schema != "tokito.footprint-definition.v1" {
        return Err("schema must be tokito.footprint-definition.v1".into());
    }
    if spec.side != Side::Front {
        return Err(
            "library footprint definitions must use front-side layers; board-side flipping belongs to placement".into(),
        );
    }
    if spec.pads.is_empty() {
        return Err("at least one supported pad is required".into());
    }
    if spec.name.is_empty()
        || spec.name.len() > 96
        || !spec
            .name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err("name must contain 1-96 ASCII letters, digits, '_', '-' or '.'".into());
    }
    validate_text(&spec.description, "description", 512)?;
    if spec.tags.len() > 32 {
        return Err("at most 32 tags are supported".into());
    }
    for tag in &spec.tags {
        validate_text(tag, "tag", 96)?;
        if tag.trim().is_empty() {
            return Err("tags must not be empty".into());
        }
    }
    validate_property(&spec.reference, "reference")?;
    validate_property(&spec.value, "value")?;
    if spec.pads.is_empty() || spec.pads.len() > MAX_PADS {
        return Err(format!("pad count must be between 1 and {MAX_PADS}"));
    }
    if spec.graphics.len() > MAX_GRAPHICS {
        return Err(format!("graphic count must not exceed {MAX_GRAPHICS}"));
    }
    for pad in &spec.pads {
        if !(pad.number.is_empty() && pad.pad_type == PadType::NpThruHole)
            && (pad.number.is_empty()
                || pad.number.len() > 16
                || !pad.number.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'-' | b'.')
                }))
        {
            return Err(
                "pad number must contain 1-16 ASCII letters, digits, '_', '+', '-' or '.'".into(),
            );
        }
        validate_position(pad.at_mm, "pad position")?;
        validate_positive(pad.size_mm[0], "pad width")?;
        validate_positive(pad.size_mm[1], "pad height")?;
        // KiCad rotates a pad about its center. Bound the full axis-aligned
        // envelope after that rotation so long, narrow pads cannot extend
        // beyond the specification's geometry limit while their unrotated
        // dimensions still appear in range.
        let angle = pad.at_mm[2].rem_euclid(360.0).to_radians();
        let (sin, cos) = angle.sin_cos();
        let half_width = pad.size_mm[0] / 2.0;
        let half_height = pad.size_mm[1] / 2.0;
        let rotated_half_x = half_width * cos.abs() + half_height * sin.abs();
        let rotated_half_y = half_width * sin.abs() + half_height * cos.abs();
        validate_extent(pad.at_mm[0].abs() + rotated_half_x, "pad X extent")?;
        validate_extent(pad.at_mm[1].abs() + rotated_half_y, "pad Y extent")?;
        // Validate the corner ratio before any drill containment check can
        // consume it. In particular, malformed round-rectangle THT input must
        // return a validation error instead of reaching an internal expect.
        match (pad.shape, pad.roundrect_ratio) {
            (PadShape::RoundRect, Some(ratio))
                if ratio.is_finite() && (0.0..=0.5).contains(&ratio) =>
            {
                quantized(ratio, "round-rectangle ratio")?;
            }
            (PadShape::RoundRect, _) => {
                return Err("roundrect pads require a roundrectRatio in [0, 0.5]".into());
            }
            (_, None) => (),
            (_, Some(_)) => return Err("roundrectRatio is only valid for roundrect pads".into()),
        }
        match pad.pad_type {
            PadType::Smd => {
                match pad.layers.as_slice() {
                    [copper, mask] if copper == spec.side.copper() && mask == spec.side.mask() => {}
                    [copper, mask, paste]
                        if copper == spec.side.copper()
                            && mask == spec.side.mask()
                            && paste == spec.side.paste() => {}
                    _ => {
                        return Err(
                            "SMD layers must be ordered Cu, Mask[, Paste] for the selected side"
                                .into(),
                        );
                    }
                }
                if pad.drill_mm.is_some()
                    || pad.drill_oval_mm.is_some()
                    || pad.drill_offset_mm.is_some()
                {
                    return Err(
                        "SMD pads must not declare drillMm, drillOvalMm, or drillOffsetMm".into(),
                    );
                }
            }
            PadType::ThruHole => {
                if pad.layers.as_slice() != ["*.Cu", "*.Mask"] {
                    return Err("through-hole layers must be ordered *.Cu, *.Mask".into());
                }
                if pad.drill_mm.is_some() && pad.drill_oval_mm.is_some() {
                    return Err(
                        "through-hole pads must declare only one of drillMm or drillOvalMm".into(),
                    );
                }
                let drill_size = if let Some(drill) = pad.drill_mm {
                    validate_positive(drill, "drill diameter")?;
                    [drill, drill]
                } else if let Some(drill) = pad.drill_oval_mm {
                    validate_positive(drill[0], "oval drill diameter")?;
                    validate_positive(drill[1], "oval drill width")?;
                    drill
                } else {
                    return Err("through-hole pads require drillMm or drillOvalMm".into());
                };
                let offset = pad.drill_offset_mm.unwrap_or([0.0, 0.0]);
                validate_point2(offset, "drill offset")?;
                validate_drill_fit(
                    pad.shape,
                    pad.size_mm,
                    pad.roundrect_ratio,
                    drill_size,
                    offset,
                )?;
                for (axis, value) in [
                    ("drill X extent", offset[0]),
                    ("drill Y extent", offset[1]),
                    ("drill diameter", drill_size[0]),
                    ("drill width", drill_size[1]),
                ] {
                    quantized(value, axis)?;
                }
            }
            PadType::NpThruHole => {
                if pad.layers.as_slice() != ["*.Cu", "*.Mask"] {
                    return Err("non-plated hole layers must be ordered *.Cu, *.Mask".into());
                }
                if pad.shape != PadShape::Circle {
                    return Err(
                        "non-plated hole candidates require a circular pad and round drill".into(),
                    );
                }
                let drill = pad
                    .drill_mm
                    .ok_or("non-plated holes require a round drillMm")?;
                if pad.drill_oval_mm.is_some() || pad.drill_offset_mm.is_some() {
                    return Err(
                        "non-plated hole candidates support only a centered round drill".into(),
                    );
                }
                validate_positive(drill, "drill diameter")?;
                let drill = quantized(drill, "drill diameter")?;
                if drill != quantized(pad.size_mm[0], "non-plated hole size")?
                    || drill != quantized(pad.size_mm[1], "non-plated hole size")?
                {
                    return Err(
                        "non-plated circular hole size must equal its drill diameter".into(),
                    );
                }
            }
        }
        for (label, value) in [
            ("pad X", pad.at_mm[0]),
            ("pad Y", pad.at_mm[1]),
            ("pad angle", pad.at_mm[2]),
            ("pad width", pad.size_mm[0]),
            ("pad height", pad.size_mm[1]),
        ] {
            quantized(value, label)?;
        }
        if pad.shape == PadShape::Circle
            && quantized(pad.size_mm[0], "circular pad width")?
                != quantized(pad.size_mm[1], "circular pad height")?
        {
            return Err("circular pads must have equal width and height".into());
        }
    }
    for graphic in &spec.graphics {
        validate_graphic(graphic, spec.side)?;
    }
    if spec.source_evidence.len() > MAX_EVIDENCE {
        return Err(format!(
            "at most {MAX_EVIDENCE} source evidence entries are supported"
        ));
    }
    for evidence in &spec.source_evidence {
        validate_text(&evidence.label, "source evidence label", 160)?;
        validate_text(&evidence.uri, "source evidence URI", 512)?;
        if evidence.label.trim().is_empty() || evidence.uri.trim().is_empty() {
            return Err("source evidence label and URI must not be empty".into());
        }
        if evidence.sha256.len() != 64 || !evidence.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("source evidence SHA-256 must contain 64 hexadecimal characters".into());
        }
    }
    Ok(())
}

fn validate_property(property: &TextProperty, name: &str) -> Result<(), String> {
    validate_text(&property.text, name, 128)?;
    if property.text.trim().is_empty() {
        return Err(format!("{name} text must not be empty"));
    }
    validate_position(property.at_mm, name)?;
    validate_positive(property.size_mm[0], "property font width")?;
    validate_positive(property.size_mm[1], "property font height")?;
    validate_positive(property.thickness_mm, "property font thickness")?;
    for value in property
        .at_mm
        .into_iter()
        .chain(property.size_mm)
        .chain([property.thickness_mm])
    {
        quantized(value, name)?;
    }
    Ok(())
}

fn validate_graphic(graphic: &GraphicSpec, side: Side) -> Result<(), String> {
    let (layer, width) = match graphic {
        GraphicSpec::Line {
            start_mm,
            end_mm,
            width_mm,
            layer,
        }
        | GraphicSpec::Rectangle {
            start_mm,
            end_mm,
            width_mm,
            layer,
        } => {
            validate_point2(*start_mm, "graphic start")?;
            validate_point2(*end_mm, "graphic end")?;
            if start_mm == end_mm {
                return Err("line or rectangle endpoints must differ".into());
            }
            if matches!(graphic, GraphicSpec::Rectangle { .. })
                && (start_mm[0] == end_mm[0] || start_mm[1] == end_mm[1])
            {
                return Err("rectangle must have nonzero width and height".into());
            }
            let start_quantized = [
                quantized(start_mm[0], "graphic coordinate")?,
                quantized(start_mm[1], "graphic coordinate")?,
            ];
            let end_quantized = [
                quantized(end_mm[0], "graphic coordinate")?,
                quantized(end_mm[1], "graphic coordinate")?,
            ];
            if start_quantized == end_quantized {
                return Err(
                    "line or rectangle endpoints must differ after coordinate quantization".into(),
                );
            }
            if matches!(graphic, GraphicSpec::Rectangle { .. })
                && (start_quantized[0] == end_quantized[0]
                    || start_quantized[1] == end_quantized[1])
            {
                return Err(
                    "rectangle must have nonzero width and height after coordinate quantization"
                        .into(),
                );
            }
            (layer.as_str(), *width_mm)
        }
        GraphicSpec::Circle {
            center_mm,
            radius_mm,
            width_mm,
            layer,
        } => {
            validate_point2(*center_mm, "circle center")?;
            validate_positive(*radius_mm, "circle radius")?;
            quantized(center_mm[0], "circle coordinate")?;
            quantized(center_mm[1], "circle coordinate")?;
            quantized(*radius_mm, "circle radius")?;
            (layer.as_str(), *width_mm)
        }
        GraphicSpec::Polygon {
            points_mm,
            width_mm,
            layer,
            ..
        } => {
            validate_polygon(points_mm)?;
            (layer.as_str(), *width_mm)
        }
    };
    if ![side.silk(), side.fab(), side.courtyard()].contains(&layer) {
        return Err(format!(
            "unsupported graphic layer for selected side: {layer}"
        ));
    }
    validate_positive(width, "graphic stroke width")?;
    quantized(width, "graphic stroke width")?;
    // KiCad serializes strokes around path centerlines, so visible geometry
    // extends half the stroke width beyond a graphic's centerline. Keep the
    // emitted geometry within the same bound used for pad envelopes.
    let half_stroke = width * 0.5;
    match graphic {
        GraphicSpec::Line {
            start_mm, end_mm, ..
        }
        | GraphicSpec::Rectangle {
            start_mm, end_mm, ..
        } => {
            for point in [start_mm, end_mm] {
                validate_extent(point[0].abs() + half_stroke, "graphic X extent")?;
                validate_extent(point[1].abs() + half_stroke, "graphic Y extent")?;
            }
        }
        GraphicSpec::Circle {
            center_mm,
            radius_mm,
            ..
        } => {
            let visible_radius = radius_mm + half_stroke;
            validate_extent(center_mm[0].abs() + visible_radius, "circle X extent")?;
            validate_extent(center_mm[1].abs() + visible_radius, "circle Y extent")?;
        }
        GraphicSpec::Polygon { points_mm, .. } => {
            for point in points_mm {
                validate_extent(point[0].abs() + half_stroke, "polygon X extent")?;
                validate_extent(point[1].abs() + half_stroke, "polygon Y extent")?;
            }
        }
    }
    Ok(())
}

fn validate_polygon(points_mm: &[[f64; 2]]) -> Result<(), String> {
    if !(3..=MAX_POLYGON_POINTS).contains(&points_mm.len()) {
        return Err(format!(
            "polygon must contain 3..={MAX_POLYGON_POINTS} points"
        ));
    }

    // Validate the exact micrometre grid emitted to KiCad. This prevents
    // source-space points that collapse during serialization from creating
    // zero-length edges or a misleading nonzero area.
    let mut points = Vec::with_capacity(points_mm.len());
    for point in points_mm {
        validate_point2(*point, "polygon point")?;
        let quantized_point = [
            quantized(point[0], "polygon X")?,
            quantized(point[1], "polygon Y")?,
        ];
        if points.contains(&quantized_point) {
            return Err("polygon points must be unique after coordinate quantization".into());
        }
        points.push(quantized_point);
    }

    let twice_area: i128 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| i128::from(a[0]) * i128::from(b[1]) - i128::from(b[0]) * i128::from(a[1]))
        .sum();
    if twice_area == 0 {
        return Err("polygon must have nonzero area after coordinate quantization".into());
    }

    // Concave, simple polygons are allowed. Reject adjacent backtracking and
    // non-adjacent crossings/touches that would make their fill ambiguous.
    for index in 0..points.len() {
        let previous = points[(index + points.len() - 1) % points.len()];
        let current = points[index];
        let next = points[(index + 1) % points.len()];
        if orientation(previous, current, next) == 0 {
            let incoming = [current[0] - previous[0], current[1] - previous[1]];
            let outgoing = [next[0] - current[0], next[1] - current[1]];
            let dot = i128::from(incoming[0]) * i128::from(outgoing[0])
                + i128::from(incoming[1]) * i128::from(outgoing[1]);
            if dot <= 0 {
                return Err("polygon adjacent edges must not overlap".into());
            }
        }
    }
    for first in 0..points.len() {
        let first_next = (first + 1) % points.len();
        for second in first + 1..points.len() {
            let second_next = (second + 1) % points.len();
            if first_next == second || second_next == first {
                continue;
            }
            if segments_intersect(
                points[first],
                points[first_next],
                points[second],
                points[second_next],
            ) {
                return Err("polygon must not self-intersect or self-touch".into());
            }
        }
    }
    Ok(())
}

fn orientation(a: [i64; 2], b: [i64; 2], c: [i64; 2]) -> i128 {
    let ab_x = i128::from(b[0]) - i128::from(a[0]);
    let ab_y = i128::from(b[1]) - i128::from(a[1]);
    let ac_x = i128::from(c[0]) - i128::from(a[0]);
    let ac_y = i128::from(c[1]) - i128::from(a[1]);
    ab_x * ac_y - ab_y * ac_x
}

fn on_segment(a: [i64; 2], b: [i64; 2], point: [i64; 2]) -> bool {
    point[0] >= a[0].min(b[0])
        && point[0] <= a[0].max(b[0])
        && point[1] >= a[1].min(b[1])
        && point[1] <= a[1].max(b[1])
}

fn segments_intersect(a: [i64; 2], b: [i64; 2], c: [i64; 2], d: [i64; 2]) -> bool {
    let (ab_c, ab_d, cd_a, cd_b) = (
        orientation(a, b, c),
        orientation(a, b, d),
        orientation(c, d, a),
        orientation(c, d, b),
    );
    (ab_c == 0 && on_segment(a, b, c))
        || (ab_d == 0 && on_segment(a, b, d))
        || (cd_a == 0 && on_segment(c, d, a))
        || (cd_b == 0 && on_segment(c, d, b))
        || (((ab_c > 0 && ab_d < 0) || (ab_c < 0 && ab_d > 0))
            && ((cd_a > 0 && cd_b < 0) || (cd_a < 0 && cd_b > 0)))
}

fn validate_text(value: &str, label: &str, maximum: usize) -> Result<(), String> {
    if value.len() > maximum || value.chars().any(char::is_control) {
        return Err(format!(
            "{label} exceeds its length limit or contains control characters"
        ));
    }
    Ok(())
}

fn validate_point2(value: [f64; 2], label: &str) -> Result<(), String> {
    for coordinate in value {
        validate_extent(coordinate.abs(), label)?;
        quantized(coordinate, label)?;
    }
    Ok(())
}

fn validate_position(value: [f64; 3], label: &str) -> Result<(), String> {
    validate_point2([value[0], value[1]], label)?;
    if !value[2].is_finite() || value[2].abs() > 360_000.0 {
        return Err(format!(
            "{label} angle must be finite and within ±360000 degrees"
        ));
    }
    quantized(value[2], label)?;
    Ok(())
}

fn validate_positive(value: f64, label: &str) -> Result<(), String> {
    if !value.is_finite() || !(0.000001..=MAX_EXTENT_MM).contains(&value) {
        return Err(format!(
            "{label} must be finite and within [0.000001, {MAX_EXTENT_MM}] mm"
        ));
    }
    quantized(value, label)?;
    Ok(())
}

fn validate_extent(value: f64, label: &str) -> Result<(), String> {
    if !value.is_finite() || value > MAX_EXTENT_MM {
        return Err(format!(
            "{label} must be finite and within ±{MAX_EXTENT_MM} mm"
        ));
    }
    Ok(())
}

fn quantized(value: f64, label: &str) -> Result<i64, String> {
    if !value.is_finite() {
        return Err(format!("{label} must be finite"));
    }
    let scaled = value * UM_PER_MM;
    let rounded = scaled.round();
    if !rounded.is_finite()
        || (scaled - rounded).abs() > 0.000001
        || rounded < i64::MIN as f64
        || rounded > i64::MAX as f64
    {
        return Err(format!(
            "{label} must be representable at six decimal places"
        ));
    }
    Ok(rounded as i64)
}

fn mm(value: f64, label: &str) -> Result<String, String> {
    let units = quantized(value, label)?;
    let sign = if units < 0 { "-" } else { "" };
    let magnitude = units.unsigned_abs();
    let whole = magnitude / UM_PER_MM as u64;
    let fraction = magnitude % UM_PER_MM as u64;
    if fraction == 0 {
        return Ok(format!("{sign}{whole}"));
    }
    let digits = format!("{fraction:06}");
    Ok(format!("{sign}{whole}.{}", digits.trim_end_matches('0')))
}

fn quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn serialize(spec: &FootprintSpec) -> Result<String, String> {
    let mut out = String::with_capacity(4096);
    let side = spec.side;
    writeln!(out, "(footprint {}", quote(&spec.name)).unwrap();
    out.push_str("  (version 20260206)\n  (generator \"tokito-cli\")\n");
    writeln!(out, "  (layer {})", quote(side.copper())).unwrap();
    writeln!(out, "  (descr {})", quote(&spec.description)).unwrap();
    let tags = spec.tags.join(" ");
    writeln!(out, "  (tags {})", quote(&tags)).unwrap();
    writeln!(out, "  (attr {})", footprint_attribute(&spec.pads)).unwrap();
    write_property(
        &mut out,
        "Reference",
        &spec.reference.text,
        &spec.reference,
        side.silk(),
    )?;
    write_property(&mut out, "Value", &spec.value.text, &spec.value, side.fab())?;
    for graphic in &spec.graphics {
        write_graphic(&mut out, graphic)?;
    }
    for pad in &spec.pads {
        write!(
            out,
            "  (pad {} {} {} (at {} {} {}) (size {} {})",
            quote(&pad.number),
            pad.pad_type.as_str(),
            pad.shape.as_str(),
            mm(pad.at_mm[0], "pad X")?,
            mm(pad.at_mm[1], "pad Y")?,
            mm(pad.at_mm[2], "pad angle")?,
            mm(pad.size_mm[0], "pad width")?,
            mm(pad.size_mm[1], "pad height")?
        )
        .unwrap();
        if let Some(drill) = pad.drill_mm {
            write!(out, " (drill {}", mm(drill, "drill diameter")?).unwrap();
            write_drill_offset(&mut out, pad.drill_offset_mm)?;
            out.push(')');
        } else if let Some([diameter, width]) = pad.drill_oval_mm {
            write!(
                out,
                " (drill oval {} {}",
                mm(diameter, "oval drill diameter")?,
                mm(width, "oval drill width")?
            )
            .unwrap();
            write_drill_offset(&mut out, pad.drill_offset_mm)?;
            out.push(')');
        }
        out.push_str(" (layers");
        for layer in &pad.layers {
            write!(out, " {}", quote(layer)).unwrap();
        }
        out.push(')');
        if let Some(ratio) = pad.roundrect_ratio {
            write!(out, " (roundrect_rratio {})", mm(ratio, "roundrect ratio")?).unwrap();
        }
        out.push_str(")\n");
    }
    out.push_str(")\n");
    Ok(out)
}

fn write_drill_offset(out: &mut String, offset: Option<[f64; 2]>) -> Result<(), String> {
    if let Some(offset) = offset {
        write!(
            out,
            " (offset {} {})",
            mm(offset[0], "drill offset X")?,
            mm(offset[1], "drill offset Y")?
        )
        .unwrap();
    }
    Ok(())
}

/// Require the drill's bounding geometry to remain inside the copper pad.
/// Circle pads use exact capsule-in-circle containment; round-rectangle pads
/// use the exact inward offset of their rounded boundary; oval pads use
/// capsule containment; rectangular pads use their exact axis-aligned
/// envelope. This is a geometric sanity check, not an annular-ring or
/// manufacturing-rule qualification.
fn validate_drill_fit(
    shape: PadShape,
    pad_size: [f64; 2],
    roundrect_ratio: Option<f64>,
    drill_size: [f64; 2],
    offset: [f64; 2],
) -> Result<(), String> {
    let inside = match shape {
        PadShape::Rect => {
            offset[0].abs() + drill_size[0] / 2.0 < pad_size[0] / 2.0
                && offset[1].abs() + drill_size[1] / 2.0 < pad_size[1] / 2.0
        }
        PadShape::Circle => {
            let (drill_segment, drill_radius) = capsule_axis(drill_size);
            let start = [offset[0] - drill_segment[0], offset[1] - drill_segment[1]];
            let end = [offset[0] + drill_segment[0], offset[1] + drill_segment[1]];
            start[0].hypot(start[1]) + drill_radius < pad_size[0] / 2.0
                && end[0].hypot(end[1]) + drill_radius < pad_size[0] / 2.0
        }
        PadShape::RoundRect => {
            let (drill_segment, drill_radius) = capsule_axis(drill_size);
            let inner_size = [
                pad_size[0] - 2.0 * drill_radius,
                pad_size[1] - 2.0 * drill_radius,
            ];
            let corner_radius =
                pad_size[0].min(pad_size[1]) * roundrect_ratio.expect("validated roundrect ratio");
            let inner_corner_radius = (corner_radius - drill_radius).max(0.0);
            let start = [offset[0] - drill_segment[0], offset[1] - drill_segment[1]];
            let end = [offset[0] + drill_segment[0], offset[1] + drill_segment[1]];
            inner_size[0] > 0.0
                && inner_size[1] > 0.0
                && point_in_roundrect(start, inner_size, inner_corner_radius)
                && point_in_roundrect(end, inner_size, inner_corner_radius)
        }
        PadShape::Oval => {
            let (pad_segment, pad_radius) = capsule_axis(pad_size);
            let (drill_segment, drill_radius) = capsule_axis(drill_size);
            let start = [offset[0] - drill_segment[0], offset[1] - drill_segment[1]];
            let end = [offset[0] + drill_segment[0], offset[1] + drill_segment[1]];
            point_segment_distance(start, [-pad_segment[0], -pad_segment[1]], pad_segment)
                + drill_radius
                < pad_radius
                && point_segment_distance(end, [-pad_segment[0], -pad_segment[1]], pad_segment)
                    + drill_radius
                    < pad_radius
        }
    };
    if inside {
        Ok(())
    } else {
        Err("drill geometry and offset must remain strictly inside the pad bounds".into())
    }
}

fn point_in_roundrect(point: [f64; 2], size: [f64; 2], corner_radius: f64) -> bool {
    let half_width = size[0] / 2.0;
    let half_height = size[1] / 2.0;
    if point[0].abs() > half_width || point[1].abs() > half_height {
        return false;
    }
    let core_half_width = half_width - corner_radius;
    let core_half_height = half_height - corner_radius;
    let outside_core_x = (point[0].abs() - core_half_width).max(0.0);
    let outside_core_y = (point[1].abs() - core_half_height).max(0.0);
    if corner_radius == 0.0 {
        point[0].abs() < half_width && point[1].abs() < half_height
    } else {
        outside_core_x.hypot(outside_core_y) < corner_radius
    }
}

fn capsule_axis(size: [f64; 2]) -> ([f64; 2], f64) {
    if size[0] >= size[1] {
        ([(size[0] - size[1]) / 2.0, 0.0], size[1] / 2.0)
    } else {
        ([0.0, (size[1] - size[0]) / 2.0], size[0] / 2.0)
    }
}

fn point_segment_distance(point: [f64; 2], start: [f64; 2], half_segment: [f64; 2]) -> f64 {
    let end = [
        start[0] + 2.0 * half_segment[0],
        start[1] + 2.0 * half_segment[1],
    ];
    let segment = [end[0] - start[0], end[1] - start[1]];
    let length_squared = segment[0] * segment[0] + segment[1] * segment[1];
    if length_squared == 0.0 {
        return (point[0] - start[0]).hypot(point[1] - start[1]);
    }
    let projection = (((point[0] - start[0]) * segment[0] + (point[1] - start[1]) * segment[1])
        / length_squared)
        .clamp(0.0, 1.0);
    let closest = [
        start[0] + projection * segment[0],
        start[1] + projection * segment[1],
    ];
    (point[0] - closest[0]).hypot(point[1] - closest[1])
}

fn write_property(
    out: &mut String,
    name: &str,
    text: &str,
    property: &TextProperty,
    layer: &str,
) -> Result<(), String> {
    writeln!(
        out,
        "  (property {} {} (at {} {} {}) (layer {}) (effects (font (size {} {}) (thickness {}))))",
        quote(name),
        quote(text),
        mm(property.at_mm[0], name)?,
        mm(property.at_mm[1], name)?,
        mm(property.at_mm[2], name)?,
        quote(layer),
        mm(property.size_mm[0], name)?,
        mm(property.size_mm[1], name)?,
        mm(property.thickness_mm, name)?
    )
    .unwrap();
    Ok(())
}

fn write_graphic(out: &mut String, graphic: &GraphicSpec) -> Result<(), String> {
    match graphic {
        GraphicSpec::Line {
            start_mm,
            end_mm,
            width_mm,
            layer,
        } => {
            writeln!(
                out,
                "  (fp_line (start {} {}) (end {} {}) (stroke (width {}) (type solid)) (layer {}))",
                mm(start_mm[0], "line start X")?,
                mm(start_mm[1], "line start Y")?,
                mm(end_mm[0], "line end X")?,
                mm(end_mm[1], "line end Y")?,
                mm(*width_mm, "line width")?,
                quote(layer)
            )
            .unwrap();
        }
        GraphicSpec::Rectangle {
            start_mm,
            end_mm,
            width_mm,
            layer,
        } => {
            writeln!(out, "  (fp_rect (start {} {}) (end {} {}) (stroke (width {}) (type solid)) (fill no) (layer {}))",
                mm(start_mm[0], "rectangle start X")?, mm(start_mm[1], "rectangle start Y")?, mm(end_mm[0], "rectangle end X")?, mm(end_mm[1], "rectangle end Y")?, mm(*width_mm, "rectangle width")?, quote(layer)).unwrap();
        }
        GraphicSpec::Circle {
            center_mm,
            radius_mm,
            width_mm,
            layer,
        } => {
            writeln!(out, "  (fp_circle (center {} {}) (end {} {}) (stroke (width {}) (type solid)) (fill none) (layer {}))",
                mm(center_mm[0], "circle center X")?, mm(center_mm[1], "circle center Y")?, mm(center_mm[0] + radius_mm, "circle end X")?, mm(center_mm[1], "circle end Y")?, mm(*width_mm, "circle width")?, quote(layer)).unwrap();
        }
        GraphicSpec::Polygon {
            points_mm,
            width_mm,
            layer,
            filled,
        } => {
            write!(out, "  (fp_poly (pts").unwrap();
            for point in points_mm {
                write!(
                    out,
                    " (xy {} {})",
                    mm(point[0], "polygon X")?,
                    mm(point[1], "polygon Y")?
                )
                .unwrap();
            }
            writeln!(
                out,
                ") (stroke (width {}) (type solid)) (fill {}) (layer {}))",
                mm(*width_mm, "polygon stroke width")?,
                if *filled { "yes" } else { "no" },
                quote(layer)
            )
            .unwrap();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_spec() -> Vec<u8> {
        br#"{
          "schema":"tokito.footprint-definition.v1",
          "name":"Demo_SMD",
          "description":"Explicit geometry candidate",
          "tags":["demo","smd"],
          "side":"front",
          "reference":{"text":"REF**","atMm":[0,-2,0],"sizeMm":[1,1],"thicknessMm":0.15},
          "value":{"text":"Demo_SMD","atMm":[0,2,0],"sizeMm":[1,1],"thicknessMm":0.15},
          "pads":[
            {"number":"1","shape":"roundrect","atMm":[-0.5,0,0],"sizeMm":[0.6,0.8],"layers":["F.Cu","F.Mask","F.Paste"],"roundrectRatio":0.25},
            {"number":"2","shape":"rect","atMm":[0.5,0,0],"sizeMm":[0.6,0.8],"layers":["F.Cu","F.Mask"]}
          ],
          "graphics":[
            {"kind":"line","startMm":[-0.3,-0.5],"endMm":[0.3,-0.5],"widthMm":0.12,"layer":"F.SilkS"},
            {"kind":"rectangle","startMm":[-0.8,-0.45],"endMm":[0.8,0.45],"widthMm":0.05,"layer":"F.CrtYd"},
            {"kind":"circle","centerMm":[0,0],"radiusMm":0.1,"widthMm":0.05,"layer":"F.Fab"}
          ],
          "sourceEvidence":[]
        }"#.to_vec()
    }

    fn polygon_spec(points: &str, filled: bool) -> Vec<u8> {
        let source = String::from_utf8(valid_spec()).unwrap();
        let circle =
            r#"{"kind":"circle","centerMm":[0,0],"radiusMm":0.1,"widthMm":0.05,"layer":"F.Fab"}"#;
        let polygon = format!(
            r#"{{"kind":"polygon","pointsMm":{points},"widthMm":0.05,"layer":"F.Fab","filled":{filled}}}"#
        );
        source.replace(circle, &polygon).into_bytes()
    }

    fn through_hole_spec() -> Vec<u8> {
        br#"{
          "schema":"tokito.footprint-definition.v1",
          "name":"Header_1x02_P2.54mm",
          "description":"Explicit round-drill through-hole candidate",
          "tags":["header","through-hole"],
          "side":"front",
          "reference":{"text":"REF**","atMm":[0,-2.5,0],"sizeMm":[1,1],"thicknessMm":0.15},
          "value":{"text":"Header_1x02_P2.54mm","atMm":[0,2.5,0],"sizeMm":[1,1],"thicknessMm":0.15},
          "pads":[
            {"number":"1","padType":"thru_hole","shape":"circle","atMm":[0,0,0],"sizeMm":[1.7,1.7],"drillMm":0.8,"layers":["*.Cu","*.Mask"]},
            {"number":"2","padType":"thru_hole","shape":"circle","atMm":[2.54,0,0],"sizeMm":[1.7,1.7],"drillMm":0.8,"layers":["*.Cu","*.Mask"]}
          ],
          "graphics":[
            {"kind":"rectangle","startMm":[-0.8,-1.1],"endMm":[3.34,1.1],"widthMm":0.1,"layer":"F.Fab"},
            {"kind":"rectangle","startMm":[-1.0,-1.5],"endMm":[3.54,1.5],"widthMm":0.05,"layer":"F.CrtYd"}
          ],
          "sourceEvidence":[]
        }"#.to_vec()
    }

    fn oval_offset_through_hole_spec() -> Vec<u8> {
        String::from_utf8(through_hole_spec())
            .unwrap()
            .replace("\"shape\":\"circle\"", "\"shape\":\"oval\"")
            .replace("\"sizeMm\":[1.7,1.7]", "\"sizeMm\":[2.0,1.7]")
            .replace(
                "\"drillMm\":0.8",
                "\"drillOvalMm\":[0.8,1.2],\"drillOffsetMm\":[0.1,0.0]",
            )
            .into_bytes()
    }

    fn non_plated_hole_spec() -> Vec<u8> {
        br#"{
          "schema":"tokito.footprint-definition.v1",
          "name":"MountingHole_3mm",
          "description":"Explicit round non-plated hole candidate",
          "tags":["mechanical","non-plated-hole"],
          "side":"front",
          "reference":{"text":"REF**","atMm":[0,-2.5,0],"sizeMm":[1,1],"thicknessMm":0.15},
          "value":{"text":"MountingHole_3mm","atMm":[0,2.5,0],"sizeMm":[1,1],"thicknessMm":0.15},
          "pads":[
            {"number":"","padType":"np_thru_hole","shape":"circle","atMm":[0,0,0],"sizeMm":[3,3],"drillMm":3,"layers":["*.Cu","*.Mask"]}
          ],
          "graphics":[
            {"kind":"circle","centerMm":[0,0],"radiusMm":1.75,"widthMm":0.1,"layer":"F.Fab"},
            {"kind":"circle","centerMm":[0,0],"radiusMm":2.0,"widthMm":0.05,"layer":"F.CrtYd"}
          ],
          "sourceEvidence":[]
        }"#.to_vec()
    }

    fn mixed_pad_spec() -> Vec<u8> {
        br#"{
          "schema":"tokito.footprint-definition.v1",
          "name":"Mixed_Header_Test",
          "description":"Synthetic mixed pad geometry acceptance fixture",
          "tags":["mixed","test"],
          "side":"front",
          "reference":{"text":"REF**","atMm":[0,-2,0],"sizeMm":[1,1],"thicknessMm":0.15},
          "value":{"text":"Mixed_Header_Test","atMm":[0,2,0],"sizeMm":[1,1],"thicknessMm":0.15},
          "pads":[
            {"number":"1","padType":"smd","shape":"rect","atMm":[-2.54,0,0],"sizeMm":[1.5,1.5],"layers":["F.Cu","F.Mask","F.Paste"]},
            {"number":"2","padType":"thru_hole","shape":"circle","atMm":[0,0,0],"sizeMm":[2.5,2.5],"drillMm":0.8,"layers":["*.Cu","*.Mask"]},
            {"number":"","padType":"np_thru_hole","shape":"circle","atMm":[2.54,0,0],"sizeMm":[2,2],"drillMm":2,"layers":["*.Cu","*.Mask"]}
          ],
          "graphics":[],
          "sourceEvidence":[]
        }"#.to_vec()
    }

    #[test]
    fn generates_stable_typed_smd_geometry_and_digest() {
        let input = valid_spec();
        let first = generate(&input).expect("valid spec");
        let second = generate(&input).expect("same valid spec");
        assert_eq!(first.text, second.text);
        assert_eq!(first.report, second.report);
        assert_eq!(first.report.pad_count, 2);
        assert_eq!(first.report.graphic_count, 3);
        assert_eq!(first.report.status, "explicit-smd-geometry-candidate");
        assert!(!first.report.manufacturer_land_pattern_qualified);
        assert!(first.text.contains("(pad \"1\" smd roundrect"));
        assert!(first.text.contains("(roundrect_rratio 0.25)"));
        assert!(first.text.contains("(fp_circle"));
        assert!(first.text.contains("(property \"Reference\" \"REF**\""));
        assert_eq!(
            first.report.output_sha256,
            crate::sha256_hex(first.text.as_bytes())
        );
    }

    #[test]
    fn generates_stable_explicit_round_drill_through_hole_pads() {
        let input = through_hole_spec();
        let first = generate(&input).expect("valid through-hole spec");
        let second = generate(&input).expect("same through-hole spec");
        assert_eq!(first.text, second.text);
        assert_eq!(first.report, second.report);
        assert_eq!(
            first.report.status,
            "explicit-through-hole-geometry-candidate"
        );
        assert_eq!(first.report.pad_type, "thru_hole");
        assert_eq!(first.report.pad_count, 2);
        assert!(first.text.contains("(attr through_hole)"));
        assert!(first.text.contains(
            "(pad \"1\" thru_hole circle (at 0 0 0) (size 1.7 1.7) (drill 0.8) (layers \"*.Cu\" \"*.Mask\"))"
        ));
        assert!(first.text.contains(
            "(pad \"2\" thru_hole circle (at 2.54 0 0) (size 1.7 1.7) (drill 0.8) (layers \"*.Cu\" \"*.Mask\"))"
        ));
        assert_eq!(
            first.report.output_sha256,
            crate::sha256_hex(first.text.as_bytes())
        );
    }

    #[test]
    fn through_hole_spec_requires_round_drill_and_global_copper_mask_layers() {
        let no_drill = String::from_utf8(through_hole_spec())
            .unwrap()
            .replace("\"drillMm\":0.8,", "");
        assert!(
            generate(no_drill.as_bytes())
                .unwrap_err()
                .contains("require drillMm or drillOvalMm")
        );

        let missing_roundrect_ratio = String::from_utf8(through_hole_spec())
            .unwrap()
            .replace("\"shape\":\"circle\"", "\"shape\":\"roundrect\"");
        assert!(
            generate(missing_roundrect_ratio.as_bytes())
                .unwrap_err()
                .contains("roundrect pads require a roundrectRatio")
        );

        let invalid_roundrect_ratio = String::from_utf8(through_hole_spec()).unwrap().replace(
            "\"shape\":\"circle\"",
            "\"shape\":\"roundrect\",\"roundrectRatio\":0.8",
        );
        assert!(
            generate(invalid_roundrect_ratio.as_bytes())
                .unwrap_err()
                .contains("roundrect pads require a roundrectRatio")
        );

        let drill_on_smd = String::from_utf8(valid_spec()).unwrap().replace(
            "\"shape\":\"roundrect\"",
            "\"shape\":\"roundrect\",\"drillMm\":0.2",
        );
        assert!(
            generate(drill_on_smd.as_bytes())
                .unwrap_err()
                .contains("SMD pads must not declare")
        );

        let oversized_drill = String::from_utf8(through_hole_spec())
            .unwrap()
            .replace("\"drillMm\":0.8", "\"drillMm\":1.7");
        assert!(
            generate(oversized_drill.as_bytes())
                .unwrap_err()
                .contains("strictly inside the pad bounds")
        );

        let wrong_layers = String::from_utf8(through_hole_spec())
            .unwrap()
            .replace("\"*.Cu\",\"*.Mask\"", "\"F.Cu\",\"F.Mask\"");
        assert!(
            generate(wrong_layers.as_bytes())
                .unwrap_err()
                .contains("through-hole layers")
        );
    }

    #[test]
    fn generates_oval_offset_plated_holes_and_checks_pad_containment() {
        let input = oval_offset_through_hole_spec();
        let generated = generate(&input).expect("valid oval offset through-hole spec");
        assert!(generated.text.contains(
            "(pad \"1\" thru_hole oval (at 0 0 0) (size 2 1.7) (drill oval 0.8 1.2 (offset 0.1 0)) (layers \"*.Cu\" \"*.Mask\"))"
        ));

        let round_offset = String::from_utf8(through_hole_spec()).unwrap().replace(
            "\"drillMm\":0.8",
            "\"drillMm\":0.8,\"drillOffsetMm\":[0.1,0.0]",
        );
        let generated_round = generate(round_offset.as_bytes()).expect("valid round offset drill");
        assert!(generated_round.text.contains("(drill 0.8 (offset 0.1 0))"));

        let both_shapes = String::from_utf8(input.clone()).unwrap().replace(
            "\"drillOvalMm\":[0.8,1.2]",
            "\"drillMm\":0.8,\"drillOvalMm\":[0.8,1.2]",
        );
        assert!(
            generate(both_shapes.as_bytes())
                .unwrap_err()
                .contains("only one of drillMm or drillOvalMm")
        );

        let outside = String::from_utf8(input)
            .unwrap()
            .replace("\"drillOffsetMm\":[0.1,0.0]", "\"drillOffsetMm\":[1.0,0.0]");
        assert!(
            generate(outside.as_bytes())
                .unwrap_err()
                .contains("strictly inside the pad bounds")
        );
    }

    #[test]
    fn oval_drills_fit_circle_and_roundrect_pads_without_accepting_corner_escape() {
        let circle_fit = String::from_utf8(through_hole_spec())
            .unwrap()
            .replace("\"sizeMm\":[1.7,1.7]", "\"sizeMm\":[2,2]")
            .replace(
                "\"drillMm\":0.8",
                "\"drillOvalMm\":[1.0,0.4],\"drillOffsetMm\":[0.0,0.5]",
            );
        generate(circle_fit.as_bytes()).expect("offset oval capsule fits the circular pad");

        let circle_escape = circle_fit.replace(
            "\"drillOffsetMm\":[0.0,0.5]",
            "\"drillOffsetMm\":[0.0,0.85]",
        );
        assert!(
            generate(circle_escape.as_bytes())
                .unwrap_err()
                .contains("strictly inside the pad bounds")
        );

        let circle_tangent = String::from_utf8(through_hole_spec())
            .unwrap()
            .replace("\"sizeMm\":[1.7,1.7]", "\"sizeMm\":[2,2]")
            .replace(
                "\"drillMm\":0.8",
                "\"drillMm\":0.4,\"drillOffsetMm\":[0,0.8]",
            );
        assert!(
            generate(circle_tangent.as_bytes())
                .unwrap_err()
                .contains("strictly inside the pad bounds")
        );

        let roundrect_fit = String::from_utf8(through_hole_spec())
            .unwrap()
            .replace("\"shape\":\"circle\"", "\"shape\":\"roundrect\"")
            .replace("\"sizeMm\":[1.7,1.7]", "\"sizeMm\":[4,2]")
            .replace(
                "\"shape\":\"roundrect\"",
                "\"shape\":\"roundrect\",\"roundrectRatio\":0.25",
            )
            .replace("\"drillMm\":0.8", "\"drillOvalMm\":[2.5,0.6]");
        generate(roundrect_fit.as_bytes())
            .expect("centered slot fits inside the roundrect's central region");

        let roundrect_tangent = String::from_utf8(through_hole_spec())
            .unwrap()
            .replace("\"shape\":\"circle\"", "\"shape\":\"roundrect\"")
            .replace("\"sizeMm\":[1.7,1.7]", "\"sizeMm\":[4,2]")
            .replace(
                "\"shape\":\"roundrect\"",
                "\"shape\":\"roundrect\",\"roundrectRatio\":0.25",
            )
            .replace(
                "\"drillMm\":0.8",
                "\"drillMm\":0.4,\"drillOffsetMm\":[1.5,0.8]",
            );
        assert!(
            generate(roundrect_tangent.as_bytes())
                .unwrap_err()
                .contains("strictly inside the pad bounds")
        );

        let zero_inner_corner = String::from_utf8(through_hole_spec())
            .unwrap()
            .replace("\"shape\":\"circle\"", "\"shape\":\"roundrect\"")
            .replace("\"sizeMm\":[1.7,1.7]", "\"sizeMm\":[4,2]")
            .replace(
                "\"shape\":\"roundrect\"",
                "\"shape\":\"roundrect\",\"roundrectRatio\":0.25",
            )
            .replace("\"drillMm\":0.8", "\"drillMm\":1.0");
        generate(zero_inner_corner.as_bytes())
            .expect("centered drill fits when erosion leaves a strict rectangle");

        let roundrect_corner_escape = String::from_utf8(through_hole_spec())
            .unwrap()
            .replace("\"shape\":\"circle\"", "\"shape\":\"roundrect\"")
            .replace("\"sizeMm\":[1.7,1.7]", "\"sizeMm\":[4,2]")
            .replace(
                "\"shape\":\"roundrect\"",
                "\"shape\":\"roundrect\",\"roundrectRatio\":0.25",
            )
            .replace(
                "\"drillMm\":0.8",
                "\"drillOvalMm\":[1.0,0.4],\"drillOffsetMm\":[1.45,0.7]",
            );
        assert!(
            generate(roundrect_corner_escape.as_bytes())
                .unwrap_err()
                .contains("strictly inside the pad bounds")
        );
    }

    #[test]
    fn generates_explicit_round_non_plated_hole_and_requires_hole_only_geometry() {
        let input = non_plated_hole_spec();
        let generated = generate(&input).expect("valid non-plated hole spec");
        assert_eq!(
            generated.report.status,
            "explicit-non-plated-hole-geometry-candidate"
        );
        assert_eq!(generated.report.pad_type, "np_thru_hole");
        assert_eq!(generated.report.pad_count, 1);
        assert!(generated.text.contains("(attr through_hole)"));
        assert!(generated.text.contains(
            "(pad \"\" np_thru_hole circle (at 0 0 0) (size 3 3) (drill 3) (layers \"*.Cu\" \"*.Mask\"))"
        ));

        let no_drill = String::from_utf8(input.clone())
            .unwrap()
            .replace("\"drillMm\":3,", "");
        assert!(
            generate(no_drill.as_bytes())
                .unwrap_err()
                .contains("require a round drillMm")
        );

        let size_mismatch = String::from_utf8(input.clone())
            .unwrap()
            .replace("\"sizeMm\":[3,3]", "\"sizeMm\":[3.2,3.2]");
        assert!(
            generate(size_mismatch.as_bytes())
                .unwrap_err()
                .contains("size must equal its drill diameter")
        );

        let wrong_layers = String::from_utf8(input.clone())
            .unwrap()
            .replace("\"*.Cu\",\"*.Mask\"", "\"F.Cu\",\"F.Mask\"");
        assert!(
            generate(wrong_layers.as_bytes())
                .unwrap_err()
                .contains("non-plated hole layers")
        );

        let non_circular = String::from_utf8(input)
            .unwrap()
            .replace("\"shape\":\"circle\"", "\"shape\":\"oval\"");
        assert!(
            generate(non_circular.as_bytes())
                .unwrap_err()
                .contains("require a circular pad and round drill")
        );
    }

    #[test]
    fn generates_mixed_smd_plated_and_non_plated_pads_with_through_hole_attribute() {
        let generated = generate(&mixed_pad_spec())
            .expect("a footprint can contain independently typed pad geometries");
        assert_eq!(
            generated.report.status,
            "explicit-mixed-pad-geometry-candidate"
        );
        assert_eq!(generated.report.pad_type, "mixed");
        assert_eq!(generated.report.pad_count, 3);
        assert!(generated.text.contains("(attr through_hole)"));
        assert!(generated.text.contains(
            "(pad \"1\" smd rect (at -2.54 0 0) (size 1.5 1.5) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\"))"
        ));
        assert!(generated.text.contains(
            "(pad \"2\" thru_hole circle (at 0 0 0) (size 2.5 2.5) (drill 0.8) (layers \"*.Cu\" \"*.Mask\"))"
        ));
        assert!(generated.text.contains(
            "(pad \"\" np_thru_hole circle (at 2.54 0 0) (size 2 2) (drill 2) (layers \"*.Cu\" \"*.Mask\"))"
        ));
    }

    #[test]
    fn mixed_smd_and_npth_footprint_uses_smd_attribute() {
        let mixed = String::from_utf8(mixed_pad_spec())
            .unwrap()
            .replace(",\n            {\"number\":\"2\",\"padType\":\"thru_hole\",\"shape\":\"circle\",\"atMm\":[0,0,0],\"sizeMm\":[2.5,2.5],\"drillMm\":0.8,\"layers\":[\"*.Cu\",\"*.Mask\"]}", "");
        let generated = generate(mixed.as_bytes()).expect("SMD and NPTH pads are valid together");
        assert_eq!(generated.report.pad_type, "mixed");
        assert!(generated.text.contains("(attr smd)"));
        assert!(!generated.text.contains("(pad \"2\" thru_hole"));
    }

    #[test]
    fn accepts_rotation_beyond_legacy_test_angles_but_bounds_extremes() {
        let rotated = String::from_utf8(valid_spec())
            .unwrap()
            .replace("\"atMm\":[-0.5,0,0]", "\"atMm\":[-0.5,0,125.5]");
        generate(rotated.as_bytes()).expect("valid arbitrary rotation");

        let excessive = rotated.replace("125.5]", "360001]");
        assert!(
            generate(excessive.as_bytes())
                .unwrap_err()
                .contains("within ±360000 degrees")
        );
    }

    #[test]
    fn rotated_pad_bounds_use_the_actual_axis_aligned_envelope() {
        let escaping = String::from_utf8(valid_spec())
            .unwrap()
            .replace("\"atMm\":[-0.5,0,0]", "\"atMm\":[99,0,90]")
            .replace("\"sizeMm\":[0.6,0.8]", "\"sizeMm\":[0.001,100]");
        assert!(
            generate(escaping.as_bytes())
                .unwrap_err()
                .contains("pad X extent")
        );

        let contained = escaping.replace("\"atMm\":[99,0,90]", "\"atMm\":[49,0,90]");
        assert!(generate(contained.as_bytes()).is_ok());
    }

    #[test]
    fn graphic_bounds_include_stroke_and_serialized_endpoints_must_remain_distinct() {
        let line_outside = String::from_utf8(valid_spec()).unwrap().replace(
            "\"startMm\":[-0.3,-0.5],\"endMm\":[0.3,-0.5],\"widthMm\":0.12",
            "\"startMm\":[99.5,0],\"endMm\":[100,0],\"widthMm\":1",
        );
        assert!(
            generate(line_outside.as_bytes())
                .unwrap_err()
                .contains("graphic X extent")
        );

        let circle_outside = String::from_utf8(valid_spec()).unwrap().replace(
            "\"centerMm\":[0,0],\"radiusMm\":0.1,\"widthMm\":0.05",
            "\"centerMm\":[0,0],\"radiusMm\":100,\"widthMm\":1",
        );
        assert!(
            generate(circle_outside.as_bytes())
                .unwrap_err()
                .contains("circle X extent")
        );

        let collapsed_line = String::from_utf8(valid_spec()).unwrap().replace(
            "\"startMm\":[-0.3,-0.5],\"endMm\":[0.3,-0.5]",
            "\"startMm\":[0,0],\"endMm\":[1e-13,0]",
        );
        assert!(
            generate(collapsed_line.as_bytes())
                .unwrap_err()
                .contains("after coordinate quantization")
        );

        let collapsed_rectangle = String::from_utf8(valid_spec()).unwrap().replace(
            "\"startMm\":[-0.8,-0.45],\"endMm\":[0.8,0.45]",
            "\"startMm\":[0,0],\"endMm\":[1e-13,1]",
        );
        assert!(
            generate(collapsed_rectangle.as_bytes())
                .unwrap_err()
                .contains("nonzero width and height")
        );
    }

    #[test]
    fn preserves_custom_reference_text_and_rejects_degenerate_rectangles() {
        let custom_reference = String::from_utf8(valid_spec())
            .unwrap()
            .replace("\"REF**\"", "\"REF_CUSTOM\"");
        let generated = generate(custom_reference.as_bytes()).expect("custom reference");
        assert!(
            generated
                .text
                .contains("(property \"Reference\" \"REF_CUSTOM\"")
        );

        let degenerate = String::from_utf8(valid_spec()).unwrap().replace(
            "\"startMm\":[-0.8,-0.45],\"endMm\":[0.8,0.45]",
            "\"startMm\":[0,-0.45],\"endMm\":[0,0.45]",
        );
        assert!(
            generate(degenerate.as_bytes())
                .unwrap_err()
                .contains("rectangle must have nonzero width and height")
        );
    }

    #[test]
    fn emits_filled_and_unfilled_polygon_geometry() {
        let filled = generate(&polygon_spec("[[-0.1,-0.1],[0.1,-0.1],[0,0.1]]", true))
            .expect("simple filled polygon");
        assert!(filled.text.contains(
            "(fp_poly (pts (xy -0.1 -0.1) (xy 0.1 -0.1) (xy 0 0.1)) (stroke (width 0.05) (type solid)) (fill yes) (layer \"F.Fab\"))"
        ));

        let outline = generate(&polygon_spec("[[0,0],[0.2,0],[0.2,0.2],[0,0.2]]", false))
            .expect("simple unfilled polygon");
        assert!(outline.text.contains("(fill no) (layer \"F.Fab\")"));

        // Concave polygons are valid when their edges remain simple.
        generate(&polygon_spec("[[0,0],[2,0],[2,2],[1,1],[0,2]]", true))
            .expect("simple concave polygon");
    }

    #[test]
    fn rejects_invalid_or_ambiguous_polygon_geometry() {
        assert!(
            generate(&polygon_spec("[[0,0],[1,0]]", true))
                .unwrap_err()
                .contains("3..=256 points")
        );
        assert!(
            generate(&polygon_spec("[[0,0],[1,0],[2,0]]", true))
                .unwrap_err()
                .contains("nonzero area")
        );
        assert!(
            generate(&polygon_spec("[[0,0],[1e-13,0],[0,1]]", true))
                .unwrap_err()
                .contains("unique after coordinate quantization")
        );
        assert!(
            generate(&polygon_spec("[[0,0],[2,2],[0,2],[2,0],[3,1]]", true))
                .unwrap_err()
                .contains("self-intersect")
        );
        assert!(
            generate(&polygon_spec("[[0,0],[2,0],[1,0],[1,1]]", true))
                .unwrap_err()
                .contains("adjacent edges must not overlap")
        );
        assert!(
            generate(&polygon_spec(
                "[[0,0],[0,4],[4,4],[4,1],[2,0],[3,-1],[4,0]]",
                true
            ))
            .unwrap_err()
            .contains("self-intersect or self-touch")
        );
    }

    #[test]
    fn rejects_duplicate_json_keys_unknown_fields_and_unsupported_geometry() {
        let duplicate = br#"{"schema":"tokito.footprint-definition.v1","schema":"bad"}"#;
        assert!(
            generate(duplicate)
                .unwrap_err()
                .contains("duplicate JSON object key")
        );
        let unknown = String::from_utf8(valid_spec()).unwrap().replace(
            "\"sourceEvidence\":[]",
            "\"unknown\":true,\"sourceEvidence\":[]",
        );
        assert!(
            generate(unknown.as_bytes())
                .unwrap_err()
                .contains("unknown field")
        );
        let custom_shape = String::from_utf8(valid_spec())
            .unwrap()
            .replace("\"shape\":\"rect\"", "\"shape\":\"custom\"");
        assert!(
            generate(custom_shape.as_bytes())
                .unwrap_err()
                .contains("unknown variant")
        );
    }

    #[test]
    fn rejects_duplicate_pads_bad_layers_non_nm_values_and_invalid_evidence() {
        let duplicate_pad = String::from_utf8(valid_spec())
            .unwrap()
            .replace("\"number\":\"2\"", "\"number\":\"1\"");
        let repeated_pin_number = generate(duplicate_pad.as_bytes())
            .expect("multiple physical pads may share one electrical pin number");
        assert_eq!(repeated_pin_number.text.matches("(pad \"1\"").count(), 2);
        let wrong_side = String::from_utf8(valid_spec()).unwrap().replace(
            "\"F.Cu\",\"F.Mask\",\"F.Paste\"",
            "\"B.Cu\",\"B.Mask\",\"B.Paste\"",
        );
        assert!(
            generate(wrong_side.as_bytes())
                .unwrap_err()
                .contains("SMD layers")
        );
        let too_precise = String::from_utf8(valid_spec())
            .unwrap()
            .replace("-0.5", "-0.5000001");
        assert!(
            generate(too_precise.as_bytes())
                .unwrap_err()
                .contains("representable at six decimal places")
        );
        let bad_digest = String::from_utf8(valid_spec()).unwrap().replace("\"sourceEvidence\":[]", "\"sourceEvidence\":[{\"label\":\"x\",\"uri\":\"https://example.invalid\",\"sha256\":\"bad\",\"claim\":\"library_geometry\"}]");
        assert!(
            generate(bad_digest.as_bytes())
                .unwrap_err()
                .contains("SHA-256")
        );
    }

    #[test]
    fn circles_require_equal_serialized_dimensions_and_library_side_is_front_only() {
        let non_circular = String::from_utf8(valid_spec())
            .unwrap()
            .replace("\"shape\":\"roundrect\"", "\"shape\":\"circle\"")
            .replace(",\"roundrectRatio\":0.25", "");
        assert!(
            generate(non_circular.as_bytes())
                .unwrap_err()
                .contains("circular pads must have equal width and height")
        );

        let circular = non_circular.replace("\"sizeMm\":[0.6,0.8]", "\"sizeMm\":[0.7,0.7]");
        assert!(generate(circular.as_bytes()).is_ok());

        let back_side = String::from_utf8(valid_spec())
            .unwrap()
            .replace("\"side\":\"front\"", "\"side\":\"back\"");
        assert!(
            generate(back_side.as_bytes())
                .unwrap_err()
                .contains("board-side flipping belongs to placement")
        );
    }
}
