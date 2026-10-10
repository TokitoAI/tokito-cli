//! Explicit native board input normalization; no topology or physical admission.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_FIELDS: usize = 16384;
const MAX_ROWS: usize = 2048;
const MAX_TEXT: usize = 128;

struct Fields {
    values: BTreeMap<String, Vec<(u64, Value)>>,
    used: BTreeMap<String, Value>,
}

impl Fields {
    fn text(&mut self, key: &str) -> Result<String, String> {
        let rows = self
            .values
            .get(key)
            .ok_or_else(|| format!("missing_board_property:{key}"))?;
        let mut value = None;
        let mut ordinals = BTreeSet::new();
        for (ordinal, raw) in rows {
            let text = raw
                .as_str()
                .ok_or_else(|| format!("unresolved_board_property:{key}"))?;
            if text.len() > MAX_TEXT || text.trim().is_empty() {
                return Err(format!("invalid_board_property_size:{key}"));
            }
            let text = text.trim();
            if value.is_some_and(|old| old != text) {
                return Err(format!("conflicting_board_property:{key}"));
            }
            value = Some(text);
            ordinals.insert(*ordinal);
        }
        let value = value.unwrap().to_string();
        self.used.insert(
            key.into(),
            json!({"value":value,"sectionOrdinals":ordinals}),
        );
        Ok(value)
    }
    fn expected(&mut self, key: &str, expected: &str) -> Result<(), String> {
        if self.text(key)? != expected {
            return Err(format!("unsupported_board_profile:{key}"));
        }
        Ok(())
    }
    fn number(&mut self, key: &str, mils: bool) -> Result<f64, String> {
        let text = self.text(key)?;
        let text = if mils {
            text.strip_suffix("mil")
                .ok_or_else(|| format!("unsupported_board_length_unit:{key}"))?
        } else {
            &text
        };
        let value = text
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("invalid_board_number:{key}"))?;
        if !value.is_finite() || value.abs() > if mils { 1_000_000.0 } else { 360_000.0 } {
            return Err(format!("nonfinite_or_out_of_range_board_number:{key}"));
        }
        Ok(if mils { value * 0.0254 } else { value })
    }
    fn positive(&mut self, key: &str) -> Result<f64, String> {
        let value = self.number(key, true)?;
        if value <= 0.0 {
            return Err(format!("nonpositive_board_thickness:{key}"));
        }
        Ok(value)
    }
    fn modern_prefix(&mut self, id: &str, name: &str) -> Result<String, String> {
        let keys: Vec<_> = self
            .values
            .keys()
            .filter(|k| k.starts_with("V9_STACK_LAYER") && k.ends_with("_LAYERID"))
            .cloned()
            .collect();
        if keys.len() > MAX_ROWS {
            return Err("board_stack_row_limit_exceeded".into());
        }
        let mut matches = Vec::new();
        for key in keys {
            if self.text(&key)? == id {
                let prefix = key.strip_suffix("LAYERID").unwrap().to_string();
                self.expected(&format!("{prefix}NAME"), name)?;
                matches.push(prefix);
            }
        }
        if matches.len() != 1 {
            return Err(format!("missing_or_ambiguous_physical_stack_layer:{id}"));
        }
        Ok(matches.remove(0))
    }
}

pub fn inspect(streams: &[Value]) -> Result<Value, String> {
    let selected: Vec<_> = streams
        .iter()
        .filter(|s| s["path"] == "/Board6/Data")
        .collect();
    if selected.len() != 1 {
        return Err("missing_or_ambiguous_board_stream".into());
    }
    let stream = selected[0];
    let records = stream["framedRecordInspection"]["records"]
        .as_array()
        .ok_or("missing_board_records")?;
    if records.len() != 1 {
        return Err("unsupported_board_record_count".into());
    }
    let sections = records[0]["propertySections"]
        .as_array()
        .ok_or("missing_board_sections")?;
    if sections.is_empty() || sections.len() > MAX_ROWS {
        return Err("board_section_limit_or_empty".into());
    }
    let mut fields = Fields {
        values: BTreeMap::new(),
        used: BTreeMap::new(),
    };
    let mut seen = BTreeSet::new();
    let mut count = 0;
    for section in sections {
        let ordinal = section["sectionOrdinal"]
            .as_u64()
            .ok_or("invalid_board_section_ordinal")?;
        if !seen.insert(ordinal) {
            return Err("duplicate_board_section_ordinal".into());
        }
        let properties = section["properties"]
            .as_object()
            .ok_or("missing_board_section_properties")?;
        for (key, value) in properties {
            count += 1;
            if count > MAX_FIELDS || key.len() > MAX_TEXT {
                return Err("board_field_limit_exceeded".into());
            }
            fields
                .values
                .entry(key.clone())
                .or_default()
                .push((ordinal, value.clone()));
        }
    }
    for (key, value) in [
        ("LAYER1PREV", "0"),
        ("LAYER1NEXT", "32"),
        ("LAYER32PREV", "1"),
        ("LAYER32NEXT", "0"),
        ("LAYER1NAME", "Top Layer"),
        ("LAYER32NAME", "Bottom Layer"),
    ] {
        fields.expected(key, value)?;
    }
    let top = fields.positive("LAYER1COPTHICK")?;
    let bottom = fields.positive("LAYER32COPTHICK")?;
    let core = fields.positive("LAYER1DIELHEIGHT")?;
    let material = fields.text("LAYER1DIELMATERIAL")?;
    fields.expected("LAYER1DIELTYPE", "0")?;
    let top_prefix = fields.modern_prefix("16777217", "Top Layer")?;
    let bottom_prefix = fields.modern_prefix("16842751", "Bottom Layer")?;
    let core_prefix = fields.modern_prefix("17039361", "Dielectric 1")?;
    for (key, value) in [
        (format!("{top_prefix}COPTHICK"), top),
        (format!("{bottom_prefix}COPTHICK"), bottom),
        (format!("{core_prefix}DIELHEIGHT"), core),
    ] {
        if fields.positive(&key)? != value {
            return Err(format!("legacy_physical_stack_disagreement:{key}"));
        }
    }
    fields.expected(&format!("{core_prefix}DIELMATERIAL"), &material)?;
    fields.expected(&format!("{core_prefix}DIELTYPE"), "0")?;
    fields.expected(&format!("{top_prefix}COMPONENTPLACEMENT"), "1")?;
    fields.expected(&format!("{bottom_prefix}COMPONENTPLACEMENT"), "2")?;
    let top_mask = fields.modern_prefix("16973834", "Top Solder")?;
    let bottom_mask = fields.modern_prefix("16973835", "Bottom Solder")?;
    let mut masks = Vec::new();
    for prefix in [&top_mask, &bottom_mask] {
        fields.expected(&format!("{prefix}DIELTYPE"), "3")?;
        fields.expected(&format!("{prefix}DIELMATERIAL"), "Solder Resist")?;
        masks.push(fields.positive(&format!("{prefix}DIELHEIGHT"))?);
    }
    // Refuse additional physical copper/core rows rather than silently reducing a multilayer stack.
    // Every property creates a physical row obligation; orphan rows may not
    // disappear merely because their NAME or LAYERID field is absent.
    let mut physical_prefixes = BTreeMap::new();
    for key in fields
        .values
        .keys()
        .filter(|k| k.starts_with("V9_STACK_LAYER"))
    {
        let tail = key.strip_prefix("V9_STACK_LAYER").unwrap();
        let (digits, field) = tail.split_once('_').ok_or("invalid_physical_stack_key")?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) || field.is_empty() {
            return Err("invalid_physical_stack_key".into());
        }
        let index = digits
            .parse::<usize>()
            .map_err(|_| "invalid_physical_stack_index")?;
        if digits != index.to_string() {
            return Err("noncanonical_physical_stack_index".into());
        }
        if index > MAX_ROWS {
            return Err("physical_stack_index_limit_exceeded".into());
        }
        let prefix = format!("V9_STACK_LAYER{digits}_");
        if let Some(old) = physical_prefixes.get(&index) {
            if old != &prefix {
                return Err("duplicate_physical_stack_index".into());
            }
        } else {
            physical_prefixes.insert(index, prefix);
        }
    }
    if physical_prefixes.len() > MAX_ROWS {
        return Err("board_stack_row_limit_exceeded".into());
    }
    let allowed = [
        ("Top Paste", "16973832"),
        ("Top Overlay", "16973830"),
        ("Top Solder", "16973834"),
        ("Top Layer", "16777217"),
        ("Dielectric 1", "17039361"),
        ("Bottom Layer", "16842751"),
        ("Bottom Solder", "16973835"),
        ("Bottom Overlay", "16973831"),
        ("Bottom Paste", "16973833"),
    ];
    let mut physical_order = BTreeMap::new();
    let mut physical_ids = BTreeSet::new();
    for (index, prefix) in physical_prefixes {
        let key = format!("{prefix}NAME");
        let name = fields.text(&key)?;
        let (rank, (_, id)) = allowed
            .iter()
            .enumerate()
            .find(|(_, (n, _))| *n == name)
            .ok_or_else(|| format!("unsupported_physical_stack_row:{key}"))?;
        fields.expected(&format!("{prefix}LAYERID"), id)?;
        if !physical_ids.insert(*id) {
            return Err("duplicate_physical_stack_layer".into());
        }
        if physical_order.insert(index, rank).is_some() {
            return Err("duplicate_physical_stack_index".into());
        }
    }
    if physical_order
        .values()
        .copied()
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
    {
        return Err("unsupported_physical_stack_order".into());
    }
    let origin = [
        fields.number("ORIGINX", true)?,
        fields.number("ORIGINY", true)?,
    ];
    let indices: BTreeSet<usize> = fields
        .values
        .keys()
        .filter_map(|k| k.strip_prefix("VX"))
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| "invalid_contour_index".to_string())
        })
        .collect::<Result<_, _>>()?;
    if indices.is_empty()
        || indices.len() > MAX_ROWS
        || indices.iter().copied().ne(0..indices.len())
    {
        return Err("missing_noncontiguous_or_oversized_contour".into());
    }
    let mut contour = Vec::new();
    for index in indices {
        let kind = fields.text(&format!("KIND{index}"))?;
        let kind = match kind.as_str() {
            "0" => "line",
            "1" => "arc",
            _ => return Err("unsupported_contour_kind".into()),
        };
        let point = [
            fields.number(&format!("VX{index}"), true)?,
            fields.number(&format!("VY{index}"), true)?,
        ];
        let center = [
            fields.number(&format!("CX{index}"), true)?,
            fields.number(&format!("CY{index}"), true)?,
        ];
        let radius = fields.number(&format!("R{index}"), true)?;
        if radius < 0.0 || kind == "arc" && radius == 0.0 {
            return Err("invalid_contour_radius".into());
        }
        contour.push(json!({"ordinal":index,"kind":kind,"pointMm":point,"centerMm":center,"radiusMm":radius,
            "startAngleDegrees":fields.number(&format!("SA{index}"),false)?,"endAngleDegrees":fields.number(&format!("EA{index}"),false)?}));
    }
    Ok(
        json!({"schema":"tokito.altium-board-source-inputs.v1","numericInputsResolved":true,
        "sourceStream":{"path":"/Board6/Data","sha256":stream["sha256"],"recordOrdinal":records[0]["ordinal"]},
        "sourceProperties":fields.used,"lengthUnit":"mm","angleUnit":"degree","sourceOriginMm":origin,
        "stack":{"profile":"agreed-two-copper-layer-linked-legacy-and-V9","topCopperMm":top,"coreMm":core,
            "bottomCopperMm":bottom,"coreMaterial":material,"topMaskMm":masks[0],"bottomMaskMm":masks[1],
            "copperAndCoreThicknessMm":top+core+bottom,"maskInclusiveThicknessMm":top+core+bottom+masks[0]+masks[1]},
        "contourRecords":contour,"contourTopologyVerified":false,"physicalThicknessVerified":false,
        "sourceZOrigin":Value::Null,"worldPoseVerified":false,"geometryQualified":false,"exportsProduced":[]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<Value> {
        let mut props = serde_json::Map::new();
        for (key, value) in [
            ("LAYER1PREV", "0"),
            ("LAYER1NEXT", "32"),
            ("LAYER32PREV", "1"),
            ("LAYER32NEXT", "0"),
            ("LAYER1NAME", "Top Layer"),
            ("LAYER32NAME", "Bottom Layer"),
            ("LAYER1COPTHICK", "1.4mil"),
            ("LAYER32COPTHICK", "1.4mil"),
            ("LAYER1DIELHEIGHT", "12.6mil"),
            ("LAYER1DIELMATERIAL", "FR-4"),
            ("LAYER1DIELTYPE", "0"),
            ("ORIGINX", "100mil"),
            ("ORIGINY", "-50mil"),
            ("VX0", "0mil"),
            ("VY0", "0mil"),
            ("KIND0", "1"),
            ("CX0", "10mil"),
            ("CY0", "0mil"),
            ("R0", "10mil"),
            ("SA0", "90"),
            ("EA0", "180"),
        ] {
            props.insert(key.into(), json!(value));
        }
        // Deliberately varied physical row indices to exercise ID selection.
        for (index, id, name) in [
            (9, "16777217", "Top Layer"),
            (10, "17039361", "Dielectric 1"),
            (11, "16842751", "Bottom Layer"),
            (8, "16973834", "Top Solder"),
            (12, "16973835", "Bottom Solder"),
        ] {
            let prefix = format!("V9_STACK_LAYER{index}_");
            props.insert(format!("{prefix}LAYERID"), json!(id));
            props.insert(format!("{prefix}NAME"), json!(name));
            let extra = match name {
                "Top Layer" => vec![("COPTHICK", "1.4mil"), ("COMPONENTPLACEMENT", "1")],
                "Bottom Layer" => vec![("COPTHICK", "1.4mil"), ("COMPONENTPLACEMENT", "2")],
                "Dielectric 1" => vec![
                    ("DIELHEIGHT", "12.6mil"),
                    ("DIELMATERIAL", "FR-4"),
                    ("DIELTYPE", "0"),
                ],
                _ => vec![
                    ("DIELHEIGHT", "0.4mil"),
                    ("DIELMATERIAL", "Solder Resist"),
                    ("DIELTYPE", "3"),
                ],
            };
            for (key, value) in extra {
                props.insert(format!("{prefix}{key}"), json!(value));
            }
        }
        vec![
            json!({"path":"/Board6/Data","framedRecordInspection":{"records":[{"ordinal":0,
            "propertySections":[{"sectionOrdinal":7,"properties":props}]}]}}),
        ]
    }
    fn props(s: &mut [Value]) -> &mut serde_json::Map<String, Value> {
        s[0]["framedRecordInspection"]["records"][0]["propertySections"][0]["properties"]
            .as_object_mut()
            .unwrap()
    }
    #[test]
    fn converts_supported_stack_and_contour_with_section_provenance() {
        let result = inspect(&fixture()).unwrap();
        assert!(
            (result["stack"]["maskInclusiveThicknessMm"]
                .as_f64()
                .unwrap()
                - 0.41148)
                .abs()
                < 1e-12
        );
        assert_eq!(result["sourceOriginMm"], json!([2.54, -1.27]));
        assert_eq!(result["contourRecords"][0]["radiusMm"], 0.254);
        assert_eq!(
            result["sourceProperties"]["ORIGINX"]["sectionOrdinals"],
            json!([7])
        );
        assert_eq!(result["geometryQualified"], false);
    }
    #[test]
    fn refuses_missing_disagreed_ambiguous_and_unsupported_stacks() {
        for (key, value) in [
            ("LAYER1NEXT", json!("2")),
            ("V9_STACK_LAYER9_COPTHICK", json!("2mil")),
            (
                "LAYER1DIELMATERIAL",
                json!({"ambiguousValues":["FR-4","other"]}),
            ),
            ("LAYER1COPTHICK", json!("NaNmil")),
            ("LAYER1COPTHICK", json!("1mm")),
        ] {
            let mut s = fixture();
            props(&mut s).insert(key.into(), value);
            assert!(inspect(&s).is_err());
        }
        let mut s = fixture();
        props(&mut s).remove("LAYER1DIELHEIGHT");
        assert!(inspect(&s).is_err());
        let mut s = fixture();
        s[0]["framedRecordInspection"]["records"][0]["propertySections"]
            .as_array_mut()
            .unwrap()
            .push(json!({"sectionOrdinal":8,"properties":{"LAYER1COPTHICK":"2mil"}}));
        assert!(inspect(&s)
            .unwrap_err()
            .contains("conflicting_board_property"));
    }
    #[test]
    fn refuses_invalid_contour_and_units_without_topology_claims() {
        for (key, value) in [
            ("KIND0", "2"),
            ("SA0", "inf"),
            ("R0", "-1mil"),
            ("VX0", "1000001mil"),
        ] {
            let mut s = fixture();
            props(&mut s).insert(key.into(), json!(value));
            assert!(inspect(&s).is_err());
        }
        let mut s = fixture();
        props(&mut s).insert("VX2".into(), json!("1mil"));
        assert!(inspect(&s).is_err());
        let mut s = fixture();
        props(&mut s).remove("VY0");
        assert!(inspect(&s).is_err());
    }
    #[test]
    fn refuses_extra_physical_ids_and_reversed_stack_order() {
        let mut s = fixture();
        props(&mut s).insert("V9_STACK_LAYER15_NAME".into(), json!("Dielectric 1"));
        props(&mut s).insert("V9_STACK_LAYER15_LAYERID".into(), json!("17039362"));
        assert!(inspect(&s).is_err());
        let mut s = fixture();
        let properties = props(&mut s);
        let keys: Vec<_> = properties
            .keys()
            .filter(|k| k.starts_with("V9_STACK_LAYER9_"))
            .cloned()
            .collect();
        for key in keys {
            let value = properties.remove(&key).unwrap();
            properties.insert(key.replacen("LAYER9_", "LAYER14_", 1), value);
        }
        assert!(inspect(&s)
            .unwrap_err()
            .contains("unsupported_physical_stack_order"));
    }
    #[test]
    fn refuses_orphan_rows_from_any_property_and_index_aliases() {
        for (key, value) in [
            ("V9_STACK_LAYER15_LAYERID", "17039362"),
            ("V9_STACK_LAYER15_COPTHICK", "1.4mil"),
            ("V9_STACK_LAYER15_NAME", "Top Layer"),
        ] {
            let mut s = fixture();
            props(&mut s).insert(key.into(), json!(value));
            assert!(inspect(&s).is_err(), "orphan row accepted: {key}");
        }
        for alias in ["09", "009", "+9", "9x", ""] {
            let mut s = fixture();
            props(&mut s).insert(format!("V9_STACK_LAYER{alias}_DIELHEIGHT"), json!("1mil"));
            assert!(inspect(&s).is_err(), "index alias accepted: {alias}");
        }
    }
}
