//! Bounded, original SMD-footprint definition tree and KiCad S-expression writer.
//!
//! This mirrors the observable *separation* used by KiCad Library Tools:
//! typed geometry nodes are validated and serialized in a stable order. It does
//! not implement the upstream IPC-7351 dimension calculator or KLC rule set.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

const MAX_SPEC_BYTES: usize = 64 * 1024;
const MAX_PADS: usize = 512;
const MAX_GRAPHICS: usize = 2048;
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PadSpec {
    number: String,
    shape: PadShape,
    at_mm: [f64; 3],
    size_mm: [f64; 2],
    layers: Vec<String>,
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
    let report = GenerationReport {
        schema: "tokito.footprint-generation-report.v1",
        status: "explicit-smd-geometry-candidate",
        footprint_name: spec.name,
        side: spec.side.as_str(),
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
            "only explicit surface-mount pads and line, rectangle, and circle graphics are supported",
            "IPC-7351 pad calculation, through-hole/custom pads, zones, groups, 3D model references, and KLC rules are not implemented",
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
        return Err("at least one SMD pad is required".into());
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
        if pad.number.is_empty()
            || pad.number.len() > 16
            || !pad.number.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'-' | b'.')
            })
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
        let expected = match pad.layers.as_slice() {
            [copper, mask] if copper == spec.side.copper() && mask == spec.side.mask() => 2,
            [copper, mask, paste]
                if copper == spec.side.copper()
                    && mask == spec.side.mask()
                    && paste == spec.side.paste() =>
            {
                3
            }
            _ => {
                return Err(
                    "SMD layers must be ordered Cu, Mask[, Paste] for the selected side".into(),
                );
            }
        };
        debug_assert!(expected == 2 || expected == 3);
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
    };
    if ![side.silk(), side.fab(), side.courtyard()].contains(&layer) {
        return Err(format!(
            "unsupported graphic layer for selected side: {layer}"
        ));
    }
    validate_positive(width, "graphic stroke width")?;
    quantized(width, "graphic stroke width")?;
    // KiCad serializes the stroke around the path centerline, so visible
    // geometry extends half the stroke width beyond line, rectangle, and
    // circle centerlines. Keep the emitted raster/CAD extents within the same
    // bound used for pad envelopes.
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
    }
    Ok(())
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
    out.push_str("  (attr smd)\n");
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
            "  (pad {} smd {} (at {} {} {}) (size {} {}) (layers",
            quote(&pad.number),
            pad.shape.as_str(),
            mm(pad.at_mm[0], "pad X")?,
            mm(pad.at_mm[1], "pad Y")?,
            mm(pad.at_mm[2], "pad angle")?,
            mm(pad.size_mm[0], "pad width")?,
            mm(pad.size_mm[1], "pad height")?
        )
        .unwrap();
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
