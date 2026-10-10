//! Bounded offline source BOM ingestion and exact component-identity reconciliation.
//! A joined identity does not certify part selection, physical revision or geometry.
use crate::{EngineError, PhysicalBoardDocument, read_bounded_file, sha256_hex, strict_json};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub const MAX_BOM_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_BOM_ROWS: usize = 10_000;
pub const MAX_BOM_REFERENCES: usize = 10_000;
pub const MAX_BOM_FIELD_BYTES: usize = 4096;
const NXP_COLUMNS: [&str; 11] = [
    "Item",
    "Quantity",
    "AgilePN",
    "Description",
    "MFG",
    "MFG_PN",
    "VALUE",
    "ASSY_OPT",
    "FIND#",
    "PRE_APN",
    "Reference",
];
const CSV_COLUMNS: [&str; 4] = ["footprint_index", "value", "footprint", "reference"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BomFormat {
    NxpJson,
    Esp32Csv,
}
impl BomFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NxpJson => "nxp-json",
            Self::Esp32Csv => "esp32-csv",
        }
    }
}
#[derive(Clone, Debug)]
pub struct BomEntry {
    pub source_row_index: usize,
    pub source_row_id: String,
    pub source_row_number: Option<u64>,
    pub source_footprint_index: Option<u64>,
    pub references: Vec<String>,
    pub source_quantity: Option<u64>,
    pub manufacturer_candidates: Vec<String>,
    pub manufacturer_part_number_candidates: Vec<String>,
    pub dnp_explicit: Option<bool>,
    pub no_part_to_order_evidence: Option<String>,
    pub assembly_option: Option<String>,
    pub source_fields: Value,
}
#[derive(Clone, Debug)]
/// Parsed documents can be inspected but not externally mutated past ingestion caps.
/// ```compile_fail
/// fn bypass(document: &mut tokito_cli::bom::BomDocument) {
///     document.entries.clear();
/// }
/// ```
pub struct BomDocument {
    format: BomFormat,
    source_sha256: String,
    source_bytes: usize,
    source_provenance: Option<Value>,
    entries: Vec<BomEntry>,
}
impl BomDocument {
    pub fn format(&self) -> BomFormat {
        self.format
    }
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    pub fn source_bytes(&self) -> usize {
        self.source_bytes
    }
    pub fn source_provenance(&self) -> Option<&Value> {
        self.source_provenance.as_ref()
    }
    pub fn entries(&self) -> &[BomEntry] {
        &self.entries
    }
}
fn error(message: impl Into<String>) -> EngineError {
    EngineError::new(message)
}
fn bounded_text(value: &Value, label: &str) -> Result<String, EngineError> {
    let text = value
        .as_str()
        .ok_or_else(|| error(format!("BOM {label} must be a string")))?;
    if text.len() > MAX_BOM_FIELD_BYTES {
        return Err(error(format!("BOM {label} field size cap exceeded")));
    }
    Ok(text.to_owned())
}
fn optional_cell(value: &Value, label: &str) -> Result<Option<String>, EngineError> {
    match value {
        Value::Null => Ok(None),
        Value::String(_) => Ok(Some(bounded_text(value, label)?)),
        Value::Number(number) => Ok(Some(number.to_string())),
        _ => Err(error(format!(
            "BOM {label} must be a scalar text/number/null cell"
        ))),
    }
}
fn candidate(text: Option<String>) -> Vec<String> {
    text.filter(|v| !v.trim().is_empty() && v.trim() != "-")
        .into_iter()
        .collect()
}
fn validate_json_fields(value: &Value) -> Result<(), EngineError> {
    match value {
        Value::String(s) if s.len() > MAX_BOM_FIELD_BYTES => {
            return Err(error("BOM string field size cap exceeded"));
        }
        Value::Object(object) => {
            for (key, value) in object {
                if key.len() > MAX_BOM_FIELD_BYTES {
                    return Err(error("BOM key size cap exceeded"));
                }
                validate_json_fields(value)?;
            }
        }
        Value::Array(values) => {
            for item in values {
                validate_json_fields(item)?;
            }
        }
        _ => (),
    }
    Ok(())
}
fn object_keys(value: &Value, expected: &[&str], label: &str) -> Result<(), EngineError> {
    let object = value
        .as_object()
        .ok_or_else(|| error(format!("BOM {label} must be an object")))?;
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        return Err(error(format!(
            "BOM {label} keys do not match reviewed schema"
        )));
    }
    Ok(())
}
fn reference(text: String) -> Result<String, EngineError> {
    if text.is_empty()
        || text.trim() != text
        || text == "-"
        || text
            .chars()
            .any(|c| c.is_whitespace() || c == ',' || c.is_control())
    {
        return Err(error(
            "BOM reference must be an explicit single nonblank identity",
        ));
    }
    Ok(text)
}

pub fn load_bom(path: impl AsRef<Path>, format: BomFormat) -> Result<BomDocument, EngineError> {
    parse_bom(&read_bounded_file(path.as_ref(), MAX_BOM_BYTES)?, format)
}
pub fn parse_bom(bytes: &[u8], format: BomFormat) -> Result<BomDocument, EngineError> {
    if bytes.len() > MAX_BOM_BYTES {
        return Err(error("BOM input byte cap exceeded"));
    }
    if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1") {
        return Err(error("BOM Git LFS pointer is not materialized"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| error("BOM input must be UTF-8"))?;
    let (entries, source_provenance) = match format {
        BomFormat::NxpJson => parse_nxp(text)?,
        BomFormat::Esp32Csv => {
            // A single leading UTF-8 signature is encoding metadata; source
            // hashing retains the original bytes and embedded markers are data.
            (
                parse_csv(text.strip_prefix('\u{feff}').unwrap_or(text))?,
                None,
            )
        }
    };
    if entries.len() > MAX_BOM_ROWS {
        return Err(error("BOM row cap exceeded"));
    }
    if entries.iter().map(|e| e.references.len()).sum::<usize>() > MAX_BOM_REFERENCES {
        return Err(error("BOM expanded reference cap exceeded"));
    }
    Ok(BomDocument {
        format,
        source_sha256: sha256_hex(bytes),
        source_bytes: bytes.len(),
        source_provenance,
        entries,
    })
}
fn parse_nxp(text: &str) -> Result<(Vec<BomEntry>, Option<Value>), EngineError> {
    let root = strict_json::parse(text).map_err(|_| error("invalid strict BOM JSON"))?;
    validate_json_fields(&root)?;
    object_keys(
        &root,
        &["schema_version", "source", "columns", "items"],
        "root",
    )?;
    if root["schema_version"] != "1.0" {
        return Err(error("unsupported BOM schema_version"));
    }
    if root["columns"] != json!(NXP_COLUMNS) {
        return Err(error("BOM columns do not match reviewed NXP schema"));
    }
    object_keys(&root["source"], &["path", "sha256", "bytes"], "source")?;
    bounded_text(&root["source"]["path"], "source.path")?;
    let digest = bounded_text(&root["source"]["sha256"], "source.sha256")?;
    if digest.len() != 64
        || !digest.bytes().all(|v| v.is_ascii_hexdigit())
        || root["source"]["bytes"].as_u64().is_none()
    {
        return Err(error("invalid claimed BOM source provenance"));
    }
    let items = root["items"]
        .as_array()
        .ok_or_else(|| error("BOM items must be an array"))?;
    if items.len() > MAX_BOM_ROWS {
        return Err(error("BOM row cap exceeded"));
    }
    let mut entries = Vec::new();
    let mut refs_total = 0usize;
    for (index, item) in items.iter().enumerate() {
        object_keys(
            item,
            &[
                "id",
                "sheet",
                "row",
                "range",
                "source_columns",
                "reference_designators",
                "dnp_explicit",
                "manufacturer_part_number_text",
                "manufacturer_part_number_cell_type",
            ],
            "item",
        )?;
        let id = bounded_text(&item["id"], "item.id")?;
        if id.trim().is_empty() {
            return Err(error("missing BOM row identity"));
        }
        bounded_text(&item["sheet"], "item.sheet")?;
        bounded_text(&item["range"], "item.range")?;
        let row = item["row"]
            .as_u64()
            .ok_or_else(|| error("BOM source row must be an unsigned integer"))?;
        let columns = &item["source_columns"];
        object_keys(columns, &NXP_COLUMNS, "source_columns")?;
        for name in NXP_COLUMNS {
            optional_cell(&columns[name], name)?;
        }
        let quantity = columns["Quantity"]
            .as_u64()
            .ok_or_else(|| error("BOM quantity must be an unsigned integer"))?;
        let declared_refs = item["reference_designators"]
            .as_array()
            .ok_or_else(|| error("BOM reference_designators must be an array"))?;
        refs_total = refs_total
            .checked_add(declared_refs.len())
            .ok_or_else(|| error("BOM reference cap exceeded"))?;
        if refs_total > MAX_BOM_REFERENCES {
            return Err(error("BOM expanded reference cap exceeded"));
        }
        let references = declared_refs
            .iter()
            .map(|v| reference(bounded_text(v, "reference")?))
            .collect::<Result<Vec<_>, EngineError>>()?;
        let raw_refs = bounded_text(&columns["Reference"], "source Reference")?;
        let raw_expanded: Vec<String> = if raw_refs.is_empty() {
            Vec::new()
        } else {
            raw_refs.split(',').map(|r| r.trim().to_owned()).collect()
        };
        if raw_expanded != references {
            return Err(error(
                "BOM expanded references conflict with source Reference cell",
            ));
        }
        let dnp = item["dnp_explicit"]
            .as_bool()
            .ok_or_else(|| error("BOM dnp_explicit must be boolean"))?;
        let description = optional_cell(&columns["Description"], "Description")?;
        let manufacturer_candidates = candidate(optional_cell(&columns["MFG"], "MFG")?);
        let mut mpn = candidate(optional_cell(&columns["MFG_PN"], "MFG_PN")?);
        let text_candidate = match &item["manufacturer_part_number_text"] {
            Value::Null => None,
            Value::String(_) => Some(bounded_text(
                &item["manufacturer_part_number_text"],
                "manufacturer_part_number_text",
            )?),
            _ => return Err(error("manufacturer_part_number_text must be string/null")),
        };
        for text in candidate(text_candidate) {
            if !mpn.contains(&text) {
                mpn.push(text);
            }
        }
        bounded_text(
            &item["manufacturer_part_number_cell_type"],
            "manufacturer_part_number_cell_type",
        )?;
        let no_part_to_order_evidence = description.filter(|d| d.contains("NO PART TO ORDER"));
        let assembly_option = optional_cell(&columns["ASSY_OPT"], "ASSY_OPT")?;
        entries.push(BomEntry {
            source_row_index: index,
            source_row_id: id,
            source_row_number: Some(row),
            source_footprint_index: None,
            references,
            source_quantity: Some(quantity),
            manufacturer_candidates,
            manufacturer_part_number_candidates: mpn,
            dnp_explicit: Some(dnp),
            no_part_to_order_evidence,
            assembly_option,
            source_fields: item.clone(),
        });
    }
    Ok((entries, Some(root["source"].clone())))
}

fn csv_records(text: &str) -> Result<Vec<Vec<String>>, EngineError> {
    // RFC-style comma records: quotes only at field start, doubled quotes escape.
    let mut chars = text.chars().peekable();
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut closed = false;
    let mut active = false;
    while let Some(ch) = chars.next() {
        active = true;
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(ch);
            }
        } else {
            match ch {
                '"' if field.is_empty() && !closed => quoted = true,
                '"' => return Err(error("malformed BOM CSV quote")),
                ',' => {
                    row.push(std::mem::take(&mut field));
                    closed = false;
                }
                '\n' | '\r' => {
                    if ch == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    closed = false;
                    active = false;
                    if rows.len() > MAX_BOM_ROWS + 1 {
                        return Err(error("BOM row cap exceeded"));
                    }
                }
                _ if closed => {
                    return Err(error("unexpected character after quoted BOM CSV field"));
                }
                _ => field.push(ch),
            }
        }
        if field.len() > MAX_BOM_FIELD_BYTES {
            return Err(error("BOM CSV field size cap exceeded"));
        }
        if row.len() > CSV_COLUMNS.len() {
            return Err(error("BOM CSV column cap exceeded"));
        }
    }
    if quoted {
        return Err(error("unterminated BOM CSV quoted field"));
    }
    if active || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    if rows.len() > MAX_BOM_ROWS + 1 {
        return Err(error("BOM row cap exceeded"));
    }
    Ok(rows)
}
fn parse_csv(text: &str) -> Result<Vec<BomEntry>, EngineError> {
    let rows = csv_records(text)?;
    if rows
        .first()
        .map(|r| r.iter().map(String::as_str).collect::<Vec<_>>())
        != Some(CSV_COLUMNS.to_vec())
    {
        return Err(error(
            "BOM CSV header must match reviewed ESP32 four-column schema",
        ));
    }
    let mut entries = Vec::new();
    for (index, row) in rows.into_iter().skip(1).enumerate() {
        if row.len() != 4 {
            return Err(error("BOM CSV row must have exactly four fields"));
        }
        let footprint_index = row[0]
            .parse::<u64>()
            .map_err(|_| error("BOM CSV footprint_index must be unsigned integer"))?;
        let references = vec![reference(row[3].clone())?];
        entries.push(BomEntry{source_row_index:index,source_row_id:format!("csv-row-{}",index+2),source_row_number:Some((index+2)as u64),source_footprint_index:Some(footprint_index),references,source_quantity:None,manufacturer_candidates:Vec::new(),manufacturer_part_number_candidates:Vec::new(),dnp_explicit:None,no_part_to_order_evidence:None,assembly_option:None,source_fields:json!({"footprint_index":row[0],"value":row[1],"footprint":row[2],"reference":row[3]})});
    }
    Ok(entries)
}

pub fn reconcile(board: &PhysicalBoardDocument, bom: &BomDocument) -> Value {
    let components = &board.engineering().components;
    if components.len() > MAX_BOM_REFERENCES
        || bom.entries.len() > MAX_BOM_ROWS
        || bom
            .entries
            .iter()
            .map(|e| e.references.len())
            .sum::<usize>()
            > MAX_BOM_REFERENCES
    {
        return json!({"schema":"tokito.bom-reconciliation.v1","status":"refused","reason":"identity_reconciliation_cap_exceeded","boardRevisionMatchVerified":false,"manufacturerPartMatchVerified":false,"claimedBomSourceProvenanceVerified":false,"modelVerified":false,"poseVerified":false,"geometryVerified":false});
    }
    let mut board_refs: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut uuids: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut bom_refs: BTreeMap<&str, usize> = BTreeMap::new();
    let mut row_ids: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, component) in components.iter().enumerate() {
        if let Some(reference) = component.reference.as_deref() {
            board_refs.entry(reference).or_default().push(index);
        }
        if let Some(uuid) = component.source.source_uuid.as_deref() {
            uuids.entry(uuid).or_default().push(index);
        }
    }
    for entry in &bom.entries {
        *row_ids.entry(&entry.source_row_id).or_default() += 1;
        for reference in &entry.references {
            *bom_refs.entry(reference).or_default() += 1;
        }
    }
    let mut rows = Vec::new();
    let mut matched = 0usize;
    let mut unmatched = 0usize;
    let mut ambiguous = 0usize;
    let mut refs = 0usize;
    for entry in &bom.entries {
        let mut joins = Vec::new();
        for reference in &entry.references {
            refs += 1;
            let candidates = board_refs.get(reference.as_str());
            let mut uuid = None;
            let mut ordinal = None;
            let status = if bom_refs[reference.as_str()] != 1 {
                "ambiguous_bom_reference"
            } else {
                match candidates {
                    None => "unmatched_bom_reference",
                    Some(indices) if indices.len() != 1 => "ambiguous_board_reference",
                    Some(indices) => {
                        let component = &components[indices[0]];
                        match component.source.source_uuid.as_deref() {
                            None | Some("") => "missing_board_uuid",
                            Some(id) if uuids[id].len() != 1 => "ambiguous_board_uuid",
                            Some(id) => {
                                uuid = Some(id);
                                ordinal = Some(component.source.ordinal);
                                "matched_identity"
                            }
                        }
                    }
                }
            };
            if status == "matched_identity" {
                matched += 1;
            } else if status == "unmatched_bom_reference" {
                unmatched += 1;
            } else {
                ambiguous += 1;
            }
            joins.push(json!({"reference":reference,"status":status,"boardUuid":uuid,"boardComponentOrdinal":ordinal,"boardCandidateOrdinalsTotal":candidates.map_or(0,Vec::len),"boardCandidateOrdinalsTruncated":candidates.is_some_and(|v|v.len()>32),"boardCandidateOrdinals":candidates.map(|indices|indices.iter().take(32).map(|index|components[*index].source.ordinal).collect::<Vec<_>>()).unwrap_or_default()}));
        }
        rows.push(json!({"sourceRowIndex":entry.source_row_index,"sourceRowId":entry.source_row_id,"sourceRowNumber":entry.source_row_number,"sourceFootprintIndex":entry.source_footprint_index,"references":entry.references,"sourceQuantity":entry.source_quantity,"quantityMismatch":entry.source_quantity.map(|q|q!=entry.references.len()as u64),"missingReferences":entry.references.is_empty(),"duplicateRowId":row_ids[entry.source_row_id.as_str()]>1,"manufacturerCandidates":entry.manufacturer_candidates,"manufacturerPartNumberCandidates":entry.manufacturer_part_number_candidates,"manufacturerPartNumberConflict":entry.manufacturer_part_number_candidates.len()>1,"populationStatus":"unverified","physicalBodyPresence":"unknown","dnpExplicit":entry.dnp_explicit,"dnpStatus":if entry.dnp_explicit.is_some(){"explicit_source_boolean"}else{"unknown"},"noPartToOrderEvidence":entry.no_part_to_order_evidence,"assemblyOption":entry.assembly_option,"assemblyOptionInterpretation":"unknown","sourceFields":entry.source_fields,"referenceJoins":joins}));
    }
    let board_missing:Vec<_>=components.iter().filter(|c|c.reference.as_deref().is_some_and(|r|!bom_refs.contains_key(r))).map(|c|json!({"reference":c.reference,"uuid":c.source.source_uuid.as_deref(),"componentOrdinal":c.source.ordinal})).collect();
    let board_identity_issues: Vec<_> = components.iter().filter_map(|component| {
        let reference = component.reference.as_deref();
        let uuid = component.source.source_uuid.as_deref();
        let mut issues = Vec::new();
        match reference {
            None | Some("") => issues.push("missing_reference"),
            Some(value) if board_refs[value].len() != 1 => issues.push("duplicate_reference"),
            _ => (),
        }
        match uuid {
            None | Some("") => issues.push("missing_uuid"),
            Some(value) if uuids[value].len() != 1 => issues.push("duplicate_uuid"),
            _ => (),
        }
        (!issues.is_empty()).then(|| json!({"componentOrdinal":component.source.ordinal,"reference":reference,"uuid":uuid,"issues":issues}))
    }).collect();
    json!({"schema":"tokito.bom-reconciliation.v1","status":"reconciled","bomSourceSha256":bom.source_sha256,"bomSourceBytes":bom.source_bytes,"claimedBomSourceProvenance":bom.source_provenance,"boardSourceSha256":board.source().sha256,"format":bom.format.as_str(),"rowCount":rows.len(),"bomReferenceCount":refs,"matchedIdentityCount":matched,"unmatchedBomReferenceCount":unmatched,"ambiguousReferenceCount":ambiguous,"boardMissingFromBomCount":board_missing.len(),"boardMissingFromBom":board_missing,"boardIdentityIssues":board_identity_issues,"rows":rows,"identityMatchPolicy":"exact_case_sensitive_reference_and_unique_nonempty_source_uuid","boardRevisionMatchVerified":false,"claimedBomSourceProvenanceVerified":false,"manufacturerPartMatchVerified":false,"modelVerified":false,"poseVerified":false,"geometryVerified":false})
}
