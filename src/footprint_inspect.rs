//! Bounded structural inspection of one native KiCad footprint asset.
//!
//! This reports selected direct pad fields and source-declared 3D model paths
//! and transforms from the S-expression shape. It does not resolve model paths
//! or determine whether KiCad accepts the footprint or whether its geometry is
//! electrically, mechanically, or fabrication correct.

use serde::Serialize;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

const MAX_FOOTPRINT_BYTES: usize = 4 * 1024 * 1024;
const MAX_PADS: usize = 4096;
const MAX_MODEL_REFERENCES: usize = 4096;
const MAX_LAYER_NAMES: usize = 64;
const MAX_FIELD_BYTES: usize = 128;
const MAX_MODEL_PATH_BYTES: usize = 2048;
const MAX_UNINTERPRETED_FIELDS: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintInspection {
    pub schema: &'static str,
    /// Complete only for the selected fields in this report; this does not
    /// mean the complete KiCad pad record was interpreted.
    pub status: &'static str,
    pub source_file: String,
    pub source_bytes: usize,
    pub source_sha256: String,
    pub footprint_name: String,
    pub pad_count: usize,
    pub pads: Vec<FootprintPadInspection>,
    pub model_reference_count: usize,
    pub model_references: Vec<FootprintModelReferenceInspection>,
    pub model_transform_coverage: &'static str,
    pub pad_record_coverage: &'static str,
    pub structural_inspection_only: bool,
    pub semantic_validation_performed: bool,
    pub geometry_validation_performed: bool,
    pub manufacturing_qualified: bool,
    pub limitations: Vec<&'static str>,
}

/// Source-declared 3D model path and explicit transform fields. Paths are not
/// resolved and transform defaults, units, model-local axes, and board poses
/// are intentionally not inferred here.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintModelReferenceInspection {
    pub ordinal: usize,
    pub path: Option<String>,
    pub offset_xyz: Option<[f64; 3]>,
    pub scale_xyz: Option<[f64; 3]>,
    pub rotate_xyz: Option<[f64; 3]>,
    pub status: &'static str,
    pub diagnostics: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintPadInspection {
    pub ordinal: usize,
    /// Preserved exactly, including an empty number when the source has one.
    pub number: Option<String>,
    pub pad_type: Option<String>,
    pub shape: Option<String>,
    pub status: &'static str,
    pub at_mm: Option<PadPosition>,
    pub size_mm: Option<[f64; 2]>,
    pub roundrect_ratio: Option<f64>,
    pub layers: Option<Vec<String>>,
    /// Source drill dimensions are reported without deriving a manufacturing
    /// interpretation. Absent drills are omitted to preserve SMD report shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drill: Option<PadDrillInspection>,
    /// Names of other direct pad clauses are surfaced, but their values are
    /// not decoded. Direct atoms are represented as `uninterpreted_atom`.
    pub uninterpreted_fields: Vec<String>,
    pub uninterpreted_field_count: usize,
    pub uninterpreted_fields_truncated: bool,
    /// Stable diagnostic codes; source text is not copied into diagnostics.
    pub diagnostics: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PadDrillInspection {
    /// `round` for a single diameter, or `oval` when the source has that token.
    pub shape: &'static str,
    pub diameter_mm: f64,
    /// The optional second source dimension for an oval slot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_mm: Option<f64>,
    /// Optional drill-center offset from the pad center, in source millimetres.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset_mm: Option<[f64; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PadPosition {
    pub x: f64,
    pub y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_deg: Option<f64>,
}

/// Inspect a regular `.kicad_mod` file without following a symbolic link.
/// The path is rechecked for byte-count changes after reading; concurrent
/// hostile replacement is outside this local read-only operation's guarantee.
pub fn inspect_path(path: &Path) -> Result<FootprintInspection, String> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("kicad_mod"))
    {
        return Err("footprint inspection input must end in .kicad_mod".into());
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect footprint file: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("footprint inspection input must be a regular non-symlink file".into());
    }
    if metadata.len() > MAX_FOOTPRINT_BYTES as u64 {
        return Err(format!(
            "footprint file exceeds the size limit ({MAX_FOOTPRINT_BYTES} bytes)"
        ));
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| safe_text(name, MAX_FIELD_BYTES, false))
        .ok_or_else(|| "footprint file name is invalid or too long".to_string())?;
    let file = File::open(path).map_err(|error| format!("cannot read footprint file: {error}"))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_FOOTPRINT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read footprint file: {error}"))?;
    if bytes.len() > MAX_FOOTPRINT_BYTES {
        return Err(format!(
            "footprint file exceeds the size limit ({MAX_FOOTPRINT_BYTES} bytes)"
        ));
    }
    if bytes.len() as u64 != metadata.len() {
        return Err("footprint file changed while being inspected".into());
    }
    inspect_bytes(file_name, &bytes)
}

/// Inspect a bounded UTF-8 footprint document supplied by a Rust consumer.
pub fn inspect_bytes(source_file: &str, bytes: &[u8]) -> Result<FootprintInspection, String> {
    if !safe_text(source_file, MAX_FIELD_BYTES, false)
        || source_file.contains('/')
        || source_file.contains('\\')
    {
        return Err("footprint source name must be a bounded file name".into());
    }
    if bytes.len() > MAX_FOOTPRINT_BYTES {
        return Err(format!(
            "footprint file exceeds the size limit ({MAX_FOOTPRINT_BYTES} bytes)"
        ));
    }
    let root = crate::parse_document(bytes).map_err(|error| error.to_string())?;
    if root.head() != Some("footprint") {
        return Err("footprint file has an unsupported S-expression root".into());
    }
    let footprint_name = root
        .value_at(1)
        .filter(|name| safe_text(name, MAX_FIELD_BYTES, false))
        .ok_or_else(|| "footprint name is missing or invalid".to_string())?
        .to_owned();
    let pad_count = root.children("pad").count();
    if pad_count > MAX_PADS {
        return Err(format!("footprint exceeds the pad limit ({MAX_PADS})"));
    }
    let pad_nodes: Vec<_> = root.children("pad").collect();
    let pads: Vec<_> = pad_nodes
        .iter()
        .enumerate()
        .map(|(ordinal, node)| inspect_pad(ordinal, node))
        .collect();
    let model_nodes: Vec<_> = root.children("model").collect();
    if model_nodes.len() > MAX_MODEL_REFERENCES {
        return Err(format!(
            "footprint exceeds the model-reference limit ({MAX_MODEL_REFERENCES})"
        ));
    }
    let model_references: Vec<_> = model_nodes
        .iter()
        .enumerate()
        .map(|(ordinal, node)| inspect_model_reference(ordinal, node))
        .collect();
    let has_partial = pads
        .iter()
        .any(|pad| pad.status != "reported_fields_complete")
        || model_references
            .iter()
            .any(|model| model.status != "reported_explicit_fields");
    Ok(FootprintInspection {
        schema: "tokito.footprint-geometry-inspection.v1",
        status: if has_partial {
            "partial_reported_fields"
        } else {
            "reported_fields_complete"
        },
        source_file: source_file.to_owned(),
        source_bytes: bytes.len(),
        source_sha256: crate::sha256_hex(bytes),
        footprint_name,
        pad_count: pads.len(),
        pads,
        model_reference_count: model_references.len(),
        model_references,
        model_transform_coverage: "explicit-source-fields-only",
        pad_record_coverage: "selected-fields-only",
        structural_inspection_only: true,
        semantic_validation_performed: false,
        geometry_validation_performed: false,
        manufacturing_qualified: false,
        limitations: vec![
            "pad positions, sizes, types, shapes, drill dimensions/offsets and layer names are reported as source structure in millimetres; electrical and fabrication semantics are not validated",
            "custom-pad primitives, per-layer overrides, clearance, mask/paste behavior and library rules are not interpreted",
            "3D model paths are not resolved; omitted transforms are not defaulted, and source units, model axes, board placement and physical fit are not validated",
            "recognized shape names do not mean their rendered boundary or physical component has been generated",
            "inspection does not establish a manufacturer-qualified land pattern, board fit, KiCad load success or fabrication readiness",
        ],
    })
}

fn inspect_model_reference(
    ordinal: usize,
    node: &crate::SourceNode,
) -> FootprintModelReferenceInspection {
    let mut diagnostics = Vec::new();
    let path = node
        .value_at(1)
        .filter(|path| safe_text(path, MAX_MODEL_PATH_BYTES, false))
        .map(ToOwned::to_owned);
    if path.is_none() {
        diagnostics.push("missing_or_invalid_model_path");
    }
    let offset_xyz =
        parse_model_transform(node, "offset", "invalid_model_offset", &mut diagnostics);
    let scale_xyz = parse_model_transform(node, "scale", "invalid_model_scale", &mut diagnostics);
    let rotate_xyz =
        parse_model_transform(node, "rotate", "invalid_model_rotation", &mut diagnostics);
    let has_uninterpreted_content = node.items().is_some_and(|items| {
        items.iter().skip(2).any(|item| match item {
            crate::SourceNode::Atom(_) => true,
            crate::SourceNode::List(_) => {
                !matches!(item.head(), Some("offset" | "scale" | "rotate"))
            }
        })
    });
    if has_uninterpreted_content {
        diagnostics.push("uninterpreted_model_content");
    }
    let status = if diagnostics.is_empty() {
        "reported_explicit_fields"
    } else {
        "partial_reported_fields"
    };
    FootprintModelReferenceInspection {
        ordinal,
        path,
        offset_xyz,
        scale_xyz,
        rotate_xyz,
        status,
        diagnostics,
    }
}

fn parse_model_transform(
    model: &crate::SourceNode,
    field: &'static str,
    invalid_code: &'static str,
    diagnostics: &mut Vec<&'static str>,
) -> Option<[f64; 3]> {
    let records: Vec<_> = model.children(field).collect();
    if records.is_empty() {
        return None;
    }
    if records.len() != 1 {
        diagnostics.push(invalid_code);
        return None;
    }
    let xyz: Vec<_> = records[0].children("xyz").collect();
    if xyz.len() != 1 {
        diagnostics.push(invalid_code);
        return None;
    }
    let Some(values) = scalar_children(xyz[0]) else {
        diagnostics.push(invalid_code);
        return None;
    };
    if values.len() != 3 {
        diagnostics.push(invalid_code);
        return None;
    }
    let parsed = values
        .iter()
        .map(|value| finite_number(value))
        .collect::<Option<Vec<_>>>();
    match parsed {
        Some(values) => Some([values[0], values[1], values[2]]),
        None => {
            diagnostics.push(invalid_code);
            None
        }
    }
}

fn inspect_pad(ordinal: usize, node: &crate::SourceNode) -> FootprintPadInspection {
    let mut diagnostics = Vec::new();
    let number = bounded_atom(node.value_at(1), true);
    let pad_type = bounded_atom(node.value_at(2), false);
    let shape = bounded_atom(node.value_at(3), false);
    if number.is_none() {
        diagnostics.push("invalid_pad_number");
    }
    if pad_type.is_none() {
        diagnostics.push("missing_or_invalid_pad_type");
    } else if !matches!(
        pad_type.as_deref(),
        Some("thru_hole" | "smd" | "connect" | "np_thru_hole")
    ) {
        diagnostics.push("unsupported_pad_type");
    }
    if shape.is_none() {
        diagnostics.push("missing_or_invalid_pad_shape");
    } else if !matches!(
        shape.as_deref(),
        Some("circle" | "rect" | "oval" | "trapezoid" | "roundrect" | "custom")
    ) {
        diagnostics.push("unsupported_pad_shape");
    } else if shape.as_deref() == Some("custom") {
        diagnostics.push("custom_pad_primitives_not_interpreted");
    }

    let at_nodes: Vec<_> = node.children("at").collect();
    let at_mm = match at_nodes.as_slice() {
        [at] => parse_position(at, &mut diagnostics),
        [] => {
            diagnostics.push("missing_pad_position");
            None
        }
        _ => {
            diagnostics.push("duplicate_pad_position");
            None
        }
    };
    let size_nodes: Vec<_> = node.children("size").collect();
    let size_mm = match size_nodes.as_slice() {
        [size] => parse_size(size, &mut diagnostics),
        [] => {
            diagnostics.push("missing_pad_size");
            None
        }
        _ => {
            diagnostics.push("duplicate_pad_size");
            None
        }
    };
    let layer_nodes: Vec<_> = node.children("layers").collect();
    let layers = match layer_nodes.as_slice() {
        [layers] => parse_layers(layers, &mut diagnostics),
        [] => {
            diagnostics.push("missing_pad_layers");
            None
        }
        _ => {
            diagnostics.push("duplicate_pad_layers");
            None
        }
    };
    let ratio_nodes: Vec<_> = node.children("roundrect_rratio").collect();
    let roundrect_ratio = match ratio_nodes.as_slice() {
        [ratio] => parse_roundrect_ratio(ratio, &mut diagnostics),
        [] => None,
        _ => {
            diagnostics.push("duplicate_roundrect_ratio");
            None
        }
    };
    let drill_nodes: Vec<_> = node.children("drill").collect();
    let drill = match drill_nodes.as_slice() {
        [] => None,
        [drill] => parse_drill(drill, &mut diagnostics),
        _ => {
            diagnostics.push("duplicate_pad_drill");
            None
        }
    };
    let (uninterpreted_fields, uninterpreted_field_count, uninterpreted_fields_truncated) =
        collect_uninterpreted_fields(node);

    let status = if diagnostics.is_empty() {
        "reported_fields_complete"
    } else {
        "partial_reported_fields"
    };
    FootprintPadInspection {
        ordinal,
        number,
        pad_type,
        shape,
        status,
        at_mm,
        size_mm,
        roundrect_ratio,
        layers,
        drill,
        uninterpreted_fields,
        uninterpreted_field_count,
        uninterpreted_fields_truncated,
        diagnostics,
    }
}

fn parse_roundrect_ratio(
    node: &crate::SourceNode,
    diagnostics: &mut Vec<&'static str>,
) -> Option<f64> {
    let Some(values) = scalar_children(node) else {
        diagnostics.push("invalid_roundrect_ratio_structure");
        return None;
    };
    if values.len() != 1 {
        diagnostics.push("invalid_roundrect_ratio_arity");
        return None;
    }
    let Some(ratio) = finite_number(values[0]) else {
        diagnostics.push("invalid_roundrect_ratio_value");
        return None;
    };
    if !(0.0..=1.0).contains(&ratio) {
        diagnostics.push("roundrect_ratio_out_of_range");
        return None;
    }
    Some(ratio)
}

fn collect_uninterpreted_fields(node: &crate::SourceNode) -> (Vec<String>, usize, bool) {
    let mut names = Vec::new();
    let mut count = 0;
    if let Some(items) = node.items() {
        for item in items.iter().skip(4) {
            let name = match item {
                crate::SourceNode::Atom(_) => Some("uninterpreted_atom"),
                crate::SourceNode::List(_) => match item.head() {
                    None => Some("uninterpreted_empty_clause"),
                    Some(name)
                        if !matches!(
                            name,
                            "at" | "size" | "layers" | "roundrect_rratio" | "drill"
                        ) =>
                    {
                        Some(name)
                    }
                    Some(_) => None,
                },
            };
            let Some(name) = name else { continue };
            count += 1;
            if names.len() < MAX_UNINTERPRETED_FIELDS {
                names.push(if safe_text(name, MAX_FIELD_BYTES, false) {
                    name.to_owned()
                } else {
                    "uninterpreted_child".to_owned()
                });
            }
        }
    }
    let truncated = count > names.len();
    (names, count, truncated)
}

fn parse_drill(
    node: &crate::SourceNode,
    diagnostics: &mut Vec<&'static str>,
) -> Option<PadDrillInspection> {
    let Some(items) = node.items() else {
        diagnostics.push("invalid_pad_drill_structure");
        return None;
    };
    let fields = &items[1..];
    let is_oval = fields.first().and_then(crate::SourceNode::atom) == Some("oval");
    let shape = if is_oval { "oval" } else { "round" };
    let mut dimensions = Vec::new();
    let mut offsets = Vec::new();
    for field in fields.iter().skip(usize::from(is_oval)) {
        if field.head() == Some("offset") {
            offsets.push(field);
        } else if let Some(value) = field.atom() {
            dimensions.push(value);
        } else {
            diagnostics.push("unsupported_pad_drill_clause");
        }
    }

    let diameter_mm = match dimensions.first().and_then(|value| finite_number(value)) {
        Some(value) if value > 0.0 => value,
        Some(_) => {
            diagnostics.push("non_positive_pad_drill_diameter");
            return None;
        }
        None => {
            diagnostics.push("missing_or_invalid_pad_drill_diameter");
            return None;
        }
    };
    let width_mm = if is_oval {
        match dimensions.get(1) {
            Some(value) => match finite_number(value) {
                Some(value) if value > 0.0 => Some(value),
                Some(_) => {
                    diagnostics.push("non_positive_pad_drill_width");
                    None
                }
                None => {
                    diagnostics.push("invalid_pad_drill_width");
                    None
                }
            },
            None => {
                diagnostics.push("missing_pad_drill_width");
                None
            }
        }
    } else {
        if dimensions.len() > 1 {
            diagnostics.push("unexpected_round_pad_drill_width");
        }
        None
    };
    if dimensions.len() > 2 {
        diagnostics.push("invalid_pad_drill_arity");
    }

    let offset_mm = match offsets.as_slice() {
        [] => None,
        [offset] => {
            let Some(values) = scalar_children(offset) else {
                diagnostics.push("invalid_pad_drill_offset_structure");
                return Some(PadDrillInspection {
                    shape,
                    diameter_mm,
                    width_mm,
                    offset_mm: None,
                });
            };
            if values.len() != 2 {
                diagnostics.push("invalid_pad_drill_offset_arity");
                None
            } else {
                match (finite_number(values[0]), finite_number(values[1])) {
                    (Some(x), Some(y)) => Some([x, y]),
                    _ => {
                        diagnostics.push("invalid_pad_drill_offset_value");
                        None
                    }
                }
            }
        }
        _ => {
            diagnostics.push("duplicate_pad_drill_offset");
            None
        }
    };
    Some(PadDrillInspection {
        shape,
        diameter_mm,
        width_mm,
        offset_mm,
    })
}

fn parse_position(
    node: &crate::SourceNode,
    diagnostics: &mut Vec<&'static str>,
) -> Option<PadPosition> {
    let Some(values) = scalar_children(node) else {
        diagnostics.push("invalid_pad_position_structure");
        return None;
    };
    if !(2..=3).contains(&values.len()) {
        diagnostics.push("invalid_pad_position_arity");
        return None;
    }
    let Some(x) = finite_number(values[0]) else {
        diagnostics.push("invalid_pad_position_x");
        return None;
    };
    let Some(y) = finite_number(values[1]) else {
        diagnostics.push("invalid_pad_position_y");
        return None;
    };
    let rotation_deg = if values.len() == 3 {
        let Some(rotation) = finite_number(values[2]) else {
            diagnostics.push("invalid_pad_rotation");
            return None;
        };
        Some(rotation)
    } else {
        None
    };
    Some(PadPosition { x, y, rotation_deg })
}

fn parse_size(node: &crate::SourceNode, diagnostics: &mut Vec<&'static str>) -> Option<[f64; 2]> {
    let Some(values) = scalar_children(node) else {
        diagnostics.push("invalid_pad_size_structure");
        return None;
    };
    if values.len() != 2 {
        diagnostics.push("invalid_pad_size_arity");
        return None;
    }
    let (Some(width), Some(height)) = (finite_number(values[0]), finite_number(values[1])) else {
        diagnostics.push("invalid_pad_size_value");
        return None;
    };
    if width <= 0.0 || height <= 0.0 {
        diagnostics.push("non_positive_pad_size");
        return None;
    }
    Some([width, height])
}

fn parse_layers(
    node: &crate::SourceNode,
    diagnostics: &mut Vec<&'static str>,
) -> Option<Vec<String>> {
    let Some(items) = node.items() else {
        diagnostics.push("invalid_pad_layers");
        return None;
    };
    let fields = &items[1..];
    if fields.is_empty() || fields.len() > MAX_LAYER_NAMES {
        diagnostics.push("invalid_pad_layer_count");
        return None;
    }
    let mut layers = Vec::with_capacity(fields.len());
    for field in fields {
        let Some(layer) = field
            .atom()
            .filter(|layer| safe_text(layer, MAX_FIELD_BYTES, false))
        else {
            diagnostics.push("invalid_pad_layer_name");
            return None;
        };
        layers.push(layer.to_owned());
    }
    Some(layers)
}

fn scalar_children(node: &crate::SourceNode) -> Option<Vec<&str>> {
    node.items()?
        .iter()
        .skip(1)
        .map(crate::SourceNode::atom)
        .collect()
}

fn finite_number(value: &str) -> Option<f64> {
    let number = value.parse::<f64>().ok()?;
    number.is_finite().then_some(number)
}

fn bounded_atom(value: Option<&str>, allow_empty: bool) -> Option<String> {
    value
        .filter(|value| safe_text(value, MAX_FIELD_BYTES, allow_empty))
        .map(ToOwned::to_owned)
}

fn safe_text(value: &str, max_bytes: usize, allow_empty: bool) -> bool {
    (allow_empty || !value.is_empty())
        && value.len() <= max_bytes
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pad_fields_and_preserves_optional_rotation() {
        let source = br#"(footprint "Test"
          (pad "1" smd roundrect (at -0.825 0) (size 0.8 0.95)
            (layers "F.Cu" "F.Mask" "F.Paste") (roundrect_rratio 0.25)))"#;
        let report = inspect_bytes("Test.kicad_mod", source).unwrap();
        assert_eq!(report.status, "reported_fields_complete");
        assert_eq!(report.pad_count, 1);
        assert_eq!(report.pads[0].number.as_deref(), Some("1"));
        assert_eq!(report.pads[0].pad_type.as_deref(), Some("smd"));
        assert_eq!(report.pads[0].shape.as_deref(), Some("roundrect"));
        assert_eq!(report.pads[0].status, "reported_fields_complete");
        assert_eq!(report.pads[0].at_mm.as_ref().unwrap().x, -0.825);
        assert_eq!(report.pads[0].at_mm.as_ref().unwrap().rotation_deg, None);
        assert_eq!(report.pads[0].size_mm, Some([0.8, 0.95]));
        assert_eq!(report.pads[0].roundrect_ratio, Some(0.25));
        assert_eq!(
            report.pads[0].layers.as_ref().unwrap(),
            &["F.Cu", "F.Mask", "F.Paste"]
        );
        assert!(report.structural_inspection_only);
        assert_eq!(report.pad_record_coverage, "selected-fields-only");
        assert!(!report.geometry_validation_performed);
    }

    #[test]
    fn reports_source_declared_3d_model_path_and_explicit_transform() {
        let source = br#"(footprint "WithModel"
          (model "${KICAD10_3DMODEL_DIR}/Package_TO_SOT_SMD.3dshapes/SOT-23.step"
            (offset (xyz 0.1 -0.2 0.3))
            (scale (xyz 1 1 1))
            (rotate (xyz 0 0 90))))"#;
        let report = inspect_bytes("WithModel.kicad_mod", source).unwrap();
        assert_eq!(report.status, "reported_fields_complete");
        assert_eq!(report.model_reference_count, 1);
        assert_eq!(
            report.model_transform_coverage,
            "explicit-source-fields-only"
        );
        assert_eq!(
            report.model_references[0].path.as_deref(),
            Some("${KICAD10_3DMODEL_DIR}/Package_TO_SOT_SMD.3dshapes/SOT-23.step")
        );
        assert_eq!(
            report.model_references[0].offset_xyz,
            Some([0.1, -0.2, 0.3])
        );
        assert_eq!(report.model_references[0].scale_xyz, Some([1.0, 1.0, 1.0]));
        assert_eq!(
            report.model_references[0].rotate_xyz,
            Some([0.0, 0.0, 90.0])
        );
        assert_eq!(
            report.model_references[0].status,
            "reported_explicit_fields"
        );
        assert!(!report.semantic_validation_performed);
        assert!(!report.geometry_validation_performed);
    }

    #[test]
    fn reports_malformed_model_transform_without_defaulting_or_resolving() {
        let source = br#"(footprint "BadModel"
          (model "model.step"
            (offset (xyz 0 0))
            (offset (xyz 1 2 3))
            (scale (xyz 1 NaN 1))))"#;
        let report = inspect_bytes("BadModel.kicad_mod", source).unwrap();
        assert_eq!(report.status, "partial_reported_fields");
        let model = &report.model_references[0];
        assert_eq!(model.path.as_deref(), Some("model.step"));
        assert_eq!(model.offset_xyz, None);
        assert_eq!(model.scale_xyz, None);
        assert_eq!(model.rotate_xyz, None);
        assert_eq!(model.diagnostics.len(), 2);
        assert!(model.diagnostics.contains(&"invalid_model_offset"));
        assert!(model.diagnostics.contains(&"invalid_model_scale"));
    }

    #[test]
    fn marks_uninterpreted_model_content_partial() {
        let source = br#"(footprint "ModelContent"
          (model "model.step" (offset (xyz 0 0 0)) (vendor_field yes) extra))"#;
        let report = inspect_bytes("ModelContent.kicad_mod", source).unwrap();
        let model = &report.model_references[0];
        assert_eq!(report.status, "partial_reported_fields");
        assert_eq!(model.status, "partial_reported_fields");
        assert!(model.diagnostics.contains(&"uninterpreted_model_content"));
    }

    #[test]
    fn does_not_infer_omitted_model_transforms() {
        let report = inspect_bytes(
            "NoTransform.kicad_mod",
            br#"(footprint "NoTransform" (model "${KICAD9_3DMODEL_DIR}/part.wrl"))"#,
        )
        .unwrap();
        let model = &report.model_references[0];
        assert_eq!(model.status, "reported_explicit_fields");
        assert_eq!(model.offset_xyz, None);
        assert_eq!(model.scale_xyz, None);
        assert_eq!(model.rotate_xyz, None);
        assert_eq!(
            report.model_transform_coverage,
            "explicit-source-fields-only"
        );
    }

    #[test]
    fn reports_round_oval_and_offset_drill_source_dimensions() {
        let source = br#"(footprint "Drills"
          (pad "1" thru_hole circle (at 0 0) (size 1.2 1.2)
            (drill 0.7 (offset 0.1 -0.2)) (layers "*.Cu" "*.Mask"))
          (pad "2" thru_hole oval (at 2 0 90) (size 1.4 2.0)
            (drill oval 0.6 1.0 (offset -0.15 0)) (layers "*.Cu" "*.Mask"))
          (pad "" np_thru_hole circle (at 4 0) (size 3 3)
            (drill 3) (layers "*.Cu" "*.Mask")))"#;
        let report = inspect_bytes("Drills.kicad_mod", source).unwrap();
        assert_eq!(report.status, "reported_fields_complete");
        assert_eq!(
            report.pads[0].drill,
            Some(PadDrillInspection {
                shape: "round",
                diameter_mm: 0.7,
                width_mm: None,
                offset_mm: Some([0.1, -0.2]),
            })
        );
        assert_eq!(
            report.pads[1].drill,
            Some(PadDrillInspection {
                shape: "oval",
                diameter_mm: 0.6,
                width_mm: Some(1.0),
                offset_mm: Some([-0.15, 0.0]),
            })
        );
        assert_eq!(report.pads[2].number.as_deref(), Some(""));
        assert_eq!(report.pads[2].drill.as_ref().unwrap().diameter_mm, 3.0);
    }

    #[test]
    fn reports_malformed_drill_records_as_partial_without_guessing() {
        let source = br#"(footprint "BadDrills"
          (pad "1" thru_hole circle (at 0 0) (size 1 1) (drill -0.5) (layers "*.Cu" "*.Mask"))
          (pad "2" thru_hole circle (at 2 0) (size 1 1)
            (drill 0.6 (offset 0)) (layers "*.Cu" "*.Mask"))
          (pad "3" thru_hole circle (at 4 0) (size 1 1)
            (drill 0.6) (drill 0.5) (layers "*.Cu" "*.Mask")))"#;
        let report = inspect_bytes("BadDrills.kicad_mod", source).unwrap();
        assert_eq!(report.status, "partial_reported_fields");
        assert!(report.pads[0].drill.is_none());
        assert!(
            report.pads[0]
                .diagnostics
                .contains(&"non_positive_pad_drill_diameter")
        );
        assert!(report.pads[1].drill.as_ref().unwrap().offset_mm.is_none());
        assert!(
            report.pads[1]
                .diagnostics
                .contains(&"invalid_pad_drill_offset_arity")
        );
        assert!(report.pads[2].drill.is_none());
        assert!(report.pads[2].diagnostics.contains(&"duplicate_pad_drill"));
    }

    #[test]
    fn missing_oval_drill_width_is_reported_as_partial() {
        let source = br#"(footprint "MissingOvalWidth"
          (pad "1" thru_hole oval (at 0 0) (size 1.6 2.0)
            (drill oval 0.6) (layers "*.Cu" "*.Mask")))"#;
        let report = inspect_bytes("MissingOvalWidth.kicad_mod", source).unwrap();
        assert_eq!(report.status, "partial_reported_fields");
        assert_eq!(report.pads[0].status, "partial_reported_fields");
        assert_eq!(
            report.pads[0].drill,
            Some(PadDrillInspection {
                shape: "oval",
                diameter_mm: 0.6,
                width_mm: None,
                offset_mm: None,
            })
        );
        assert!(
            report.pads[0]
                .diagnostics
                .contains(&"missing_pad_drill_width")
        );
    }

    #[test]
    fn marks_unknown_shape_custom_geometry_and_malformed_fields_partial() {
        let source = br#"(footprint "Test"
          (pad "A" smd polygonal (at 1 2 90) (size 1 2) (layers "F.Cu"))
          (pad "B" smd custom (at nope 2) (size 0 1) (layers "F.Cu")))"#;
        let report = inspect_bytes("Test.kicad_mod", source).unwrap();
        assert_eq!(report.status, "partial_reported_fields");
        assert_eq!(report.pads[0].diagnostics, ["unsupported_pad_shape"]);
        assert_eq!(
            report.pads[0].at_mm.as_ref().unwrap().rotation_deg,
            Some(90.0)
        );
        assert!(
            report.pads[1]
                .diagnostics
                .contains(&"custom_pad_primitives_not_interpreted")
        );
        assert!(
            report.pads[1]
                .diagnostics
                .contains(&"invalid_pad_position_x")
        );
        assert!(
            report.pads[1]
                .diagnostics
                .contains(&"non_positive_pad_size")
        );
    }

    #[test]
    fn exposes_uninterpreted_direct_clauses_and_atoms() {
        let source = br#"(footprint "Test"
          (pad "1" smd rect locked (at 0 0) (size 1 1) (layers "F.Cu")
            (net 2 "GND") (custom_pad_options (clearance 0.2)) ()))"#;
        let report = inspect_bytes("Test.kicad_mod", source).unwrap();
        let pad = &report.pads[0];
        assert_eq!(pad.status, "reported_fields_complete");
        assert_eq!(
            pad.uninterpreted_fields,
            [
                "uninterpreted_atom",
                "net",
                "custom_pad_options",
                "uninterpreted_empty_clause"
            ]
        );
        assert_eq!(pad.uninterpreted_field_count, 4);
        assert!(!pad.uninterpreted_fields_truncated);
    }

    #[test]
    fn rejects_bad_roots_oversized_documents_and_excessive_pad_counts() {
        assert!(inspect_bytes("Test.kicad_mod", b"(module \"old\")").is_err());
        assert!(inspect_bytes("Test.kicad_mod", &vec![b' '; MAX_FOOTPRINT_BYTES + 1]).is_err());
        let mut source = String::from("(footprint \"TooMany\"");
        for _ in 0..=MAX_PADS {
            source.push_str(" (pad \"1\" smd rect (at 0 0) (size 1 1) (layers \"F.Cu\"))");
        }
        source.push(')');
        assert!(
            inspect_bytes("Test.kicad_mod", source.as_bytes())
                .unwrap_err()
                .contains("pad limit")
        );
    }
}
