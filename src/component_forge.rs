//! Strict, PCB-identity-bound component envelope specifications.
//!
//! This module emits a simple cuboid envelope from explicitly supplied nominal
//! dimensions. It does not infer package geometry, terminals, pad layouts, or
//! placement from a photograph, BOM value, or manufacturer name. Dataset and
//! BOM fields are unverified source claims; only the board hash and unique
//! reference/UUID identity are checked against the loaded PCB.

use crate::{EngineError, PhysicalBoardDocument, strict_json};
use serde_json::{Map, Value, json};

/// Maximum encoded specification size accepted by the parser.
pub const MAX_COMPONENT_ENVELOPE_SPEC_BYTES: usize = 64 * 1024;
const MAX_IDENTITY_TEXT_BYTES: usize = 256;
const MAX_MANUFACTURER_URL_BYTES: usize = 2048;
const MAX_DIMENSION_MM: f64 = 100.0;
const COMPONENT_ENVELOPE_SCHEMA: &str = "tokito.component-envelope.v1";

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentEnvelopeSpec {
    board_sha256: String,
    dataset_commit: String,
    component_reference: String,
    component_uuid: String,
    bom_source_sha256: String,
    manufacturer: String,
    manufacturer_part_number: String,
    package: String,
    manufacturer_url: String,
    length_mm: f64,
    width_mm: f64,
    height_mm: f64,
}

impl ComponentEnvelopeSpec {
    pub fn board_sha256(&self) -> &str {
        &self.board_sha256
    }

    pub fn dataset_commit(&self) -> &str {
        &self.dataset_commit
    }

    pub fn component_reference(&self) -> &str {
        &self.component_reference
    }

    pub fn component_uuid(&self) -> &str {
        &self.component_uuid
    }

    pub fn bom_source_sha256(&self) -> &str {
        &self.bom_source_sha256
    }

    pub fn manufacturer(&self) -> &str {
        &self.manufacturer
    }

    pub fn manufacturer_part_number(&self) -> &str {
        &self.manufacturer_part_number
    }

    pub fn package(&self) -> &str {
        &self.package
    }

    pub fn manufacturer_url(&self) -> &str {
        &self.manufacturer_url
    }

    pub fn length_mm(&self) -> f64 {
        self.length_mm
    }

    pub fn width_mm(&self) -> f64 {
        self.width_mm
    }

    pub fn height_mm(&self) -> f64 {
        self.height_mm
    }
}

struct ComponentEnvelopeWire {
    schema: String,
    board: BoardIdentityWire,
    component: ComponentIdentityWire,
    evidence: EnvelopeEvidenceWire,
    body_mm: EnvelopeDimensionsWire,
}

struct BoardIdentityWire {
    sha256: String,
}

struct ComponentIdentityWire {
    reference: String,
    uuid: String,
    manufacturer: String,
    mpn: String,
    package: String,
}

struct EnvelopeEvidenceWire {
    dataset_commit: String,
    bom_source_sha256: String,
    manufacturer_url: String,
}

struct EnvelopeDimensionsWire {
    length: f64,
    width: f64,
    height: f64,
}

impl ComponentEnvelopeWire {
    fn from_value(value: Value) -> Result<Self, EngineError> {
        let mut fields = strict_object(
            value,
            &["schema", "board", "component", "evidence", "bodyMm"],
            "root",
        )?;
        let schema = take_string(&mut fields, "schema", "root")?;
        let board = parse_board_identity(take_field(&mut fields, "board")?)?;
        let component = parse_component_identity(take_field(&mut fields, "component")?)?;
        let evidence = parse_envelope_evidence(take_field(&mut fields, "evidence")?)?;
        let body_mm = parse_envelope_dimensions(take_field(&mut fields, "bodyMm")?)?;
        Ok(Self {
            schema,
            board,
            component,
            evidence,
            body_mm,
        })
    }
}

fn parse_board_identity(value: Value) -> Result<BoardIdentityWire, EngineError> {
    let mut fields = strict_object(value, &["sha256"], "board")?;
    Ok(BoardIdentityWire {
        sha256: take_string(&mut fields, "sha256", "board")?,
    })
}

fn parse_component_identity(value: Value) -> Result<ComponentIdentityWire, EngineError> {
    let mut fields = strict_object(
        value,
        &["reference", "uuid", "manufacturer", "mpn", "package"],
        "component",
    )?;
    Ok(ComponentIdentityWire {
        reference: take_string(&mut fields, "reference", "component")?,
        uuid: take_string(&mut fields, "uuid", "component")?,
        manufacturer: take_string(&mut fields, "manufacturer", "component")?,
        mpn: take_string(&mut fields, "mpn", "component")?,
        package: take_string(&mut fields, "package", "component")?,
    })
}

fn parse_envelope_evidence(value: Value) -> Result<EnvelopeEvidenceWire, EngineError> {
    let mut fields = strict_object(
        value,
        &["datasetCommit", "bomSourceSha256", "manufacturerUrl"],
        "evidence",
    )?;
    Ok(EnvelopeEvidenceWire {
        dataset_commit: take_string(&mut fields, "datasetCommit", "evidence")?,
        bom_source_sha256: take_string(&mut fields, "bomSourceSha256", "evidence")?,
        manufacturer_url: take_string(&mut fields, "manufacturerUrl", "evidence")?,
    })
}

fn parse_envelope_dimensions(value: Value) -> Result<EnvelopeDimensionsWire, EngineError> {
    let mut fields = strict_object(value, &["length", "width", "height"], "bodyMm")?;
    Ok(EnvelopeDimensionsWire {
        length: take_number(&mut fields, "length", "bodyMm")?,
        width: take_number(&mut fields, "width", "bodyMm")?,
        height: take_number(&mut fields, "height", "bodyMm")?,
    })
}

fn strict_object(
    value: Value,
    expected_keys: &[&str],
    context: &str,
) -> Result<Map<String, Value>, EngineError> {
    let Value::Object(fields) = value else {
        return Err(EngineError::new(format!("{context} must be a JSON object")));
    };
    if let Some(key) = fields
        .keys()
        .find(|key| !expected_keys.contains(&key.as_str()))
    {
        return Err(EngineError::new(format!(
            "{context} contains unknown field {key:?}"
        )));
    }
    if let Some(key) = expected_keys.iter().find(|key| !fields.contains_key(**key)) {
        return Err(EngineError::new(format!(
            "{context} is missing required field {key:?}"
        )));
    }
    Ok(fields)
}

fn take_field(fields: &mut Map<String, Value>, name: &str) -> Result<Value, EngineError> {
    fields
        .remove(name)
        .ok_or_else(|| EngineError::new(format!("missing required field {name:?}")))
}

fn take_string(
    fields: &mut Map<String, Value>,
    name: &str,
    context: &str,
) -> Result<String, EngineError> {
    take_field(fields, name)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| EngineError::new(format!("{context}.{name} must be a string")))
}

fn take_number(
    fields: &mut Map<String, Value>,
    name: &str,
    context: &str,
) -> Result<f64, EngineError> {
    take_field(fields, name)?
        .as_f64()
        .ok_or_else(|| EngineError::new(format!("{context}.{name} must be a finite number")))
}

/// Parse a duplicate-key-free, strict versioned JSON envelope specification.
pub fn parse_component_envelope_spec(input: &[u8]) -> Result<ComponentEnvelopeSpec, EngineError> {
    if input.len() > MAX_COMPONENT_ENVELOPE_SPEC_BYTES {
        return Err(EngineError::new(
            "component envelope specification exceeds the 64 KiB limit",
        ));
    }
    let text = std::str::from_utf8(input)
        .map_err(|_| EngineError::new("component envelope specification is not UTF-8"))?;
    let value = strict_json::parse(text)
        .map_err(|error| EngineError::new(format!("invalid component envelope JSON: {error}")))?;
    let wire = ComponentEnvelopeWire::from_value(value)?;

    if wire.schema != COMPONENT_ENVELOPE_SCHEMA {
        return Err(EngineError::new(format!(
            "unsupported component envelope schema; expected {COMPONENT_ENVELOPE_SCHEMA}"
        )));
    }
    validate_hash(&wire.board.sha256, "board.sha256")?;
    validate_commit(&wire.evidence.dataset_commit)?;
    validate_hash(&wire.evidence.bom_source_sha256, "evidence.bomSourceSha256")?;
    validate_text(
        &wire.component.reference,
        "component.reference",
        MAX_IDENTITY_TEXT_BYTES,
    )?;
    validate_text(
        &wire.component.uuid,
        "component.uuid",
        MAX_IDENTITY_TEXT_BYTES,
    )?;
    validate_text(
        &wire.component.manufacturer,
        "component.manufacturer",
        MAX_IDENTITY_TEXT_BYTES,
    )?;
    validate_text(
        &wire.component.mpn,
        "component.mpn",
        MAX_IDENTITY_TEXT_BYTES,
    )?;
    validate_text(
        &wire.component.package,
        "component.package",
        MAX_IDENTITY_TEXT_BYTES,
    )?;
    validate_manufacturer_url(&wire.evidence.manufacturer_url)?;
    validate_dimension(wire.body_mm.length, "bodyMm.length")?;
    validate_dimension(wire.body_mm.width, "bodyMm.width")?;
    validate_dimension(wire.body_mm.height, "bodyMm.height")?;

    Ok(ComponentEnvelopeSpec {
        board_sha256: wire.board.sha256,
        dataset_commit: wire.evidence.dataset_commit,
        component_reference: wire.component.reference,
        component_uuid: wire.component.uuid,
        bom_source_sha256: wire.evidence.bom_source_sha256,
        manufacturer: wire.component.manufacturer,
        manufacturer_part_number: wire.component.mpn,
        package: wire.component.package,
        manufacturer_url: wire.evidence.manufacturer_url,
        length_mm: wire.body_mm.length,
        width_mm: wire.body_mm.width,
        height_mm: wire.body_mm.height,
    })
}

/// Require the claimed source digest and the reference/UUID pair to identify
/// exactly one component in this imported physical board.
pub fn validate_component_envelope(
    spec: &ComponentEnvelopeSpec,
    board: &PhysicalBoardDocument,
) -> Result<(), EngineError> {
    validate_spec_values(spec)?;
    if spec.board_sha256 != board.source().sha256 {
        return Err(EngineError::new(
            "component envelope board SHA-256 does not match the loaded board",
        ));
    }

    let components = &board.engineering().components;
    let by_reference: Vec<_> = components
        .iter()
        .filter(|component| {
            component.reference.as_deref() == Some(spec.component_reference.as_str())
        })
        .collect();
    let by_uuid: Vec<_> = components
        .iter()
        .filter(|component| {
            component
                .source
                .source_uuid
                .as_deref()
                .is_some_and(|uuid| uuid == spec.component_uuid.as_str())
        })
        .collect();
    if by_reference.len() != 1 || by_uuid.len() != 1 || !std::ptr::eq(by_reference[0], by_uuid[0]) {
        return Err(EngineError::new(
            "component envelope reference and UUID must identify the same unique board component",
        ));
    }
    Ok(())
}

/// Emit a deterministic VRML97 cuboid using only the dimensions in the spec.
///
/// Callers must run validate_component_envelope against the intended board
/// before treating the result as associated with a board component.
pub fn generate_envelope_wrl(spec: &ComponentEnvelopeSpec) -> String {
    let half_x = spec.length_mm / 2.0;
    let half_y = spec.width_mm / 2.0;
    let x0 = number(-half_x);
    let x1 = number(half_x);
    let y0 = number(-half_y);
    let y1 = number(half_y);
    let z1 = number(spec.height_mm);

    format!(
        "#VRML V2.0 utf8\n\
# Tokito candidate component envelope; coordinates are millimetres.\n\
Shape {{\n\
  geometry IndexedFaceSet {{\n\
    solid TRUE\n\
    ccw TRUE\n\
    coord Coordinate {{\n\
      point [\n\
        {x0} {y0} 0,\n\
        {x1} {y0} 0,\n\
        {x1} {y1} 0,\n\
        {x0} {y1} 0,\n\
        {x0} {y0} {z1},\n\
        {x1} {y0} {z1},\n\
        {x1} {y1} {z1},\n\
        {x0} {y1} {z1}\n\
      ]\n\
    }}\n\
    coordIndex [\n\
      0 2 1 -1, 0 3 2 -1,\n\
      4 5 6 -1, 4 6 7 -1,\n\
      0 1 5 -1, 0 5 4 -1,\n\
      1 2 6 -1, 1 6 5 -1,\n\
      2 3 7 -1, 2 7 6 -1,\n\
      3 0 4 -1, 3 4 7 -1\n\
    ]\n\
  }}\n\
}}\n"
    )
}

/// Report only source claims and the deliberately limited envelope output.
/// Board revision, package fit, footprint geometry, and electrical terminals
/// are not qualified by this generated cuboid.
pub fn envelope_report_json(spec: &ComponentEnvelopeSpec) -> Value {
    json!({
        "schema": "tokito.component-envelope-report.v1",
        "status": "candidate-envelope-only",
        "sourceClaims": {
            "boardSha256": spec.board_sha256,
            "datasetCommit": spec.dataset_commit,
            "component": {
                "reference": spec.component_reference,
                "uuid": spec.component_uuid,
                "manufacturer": spec.manufacturer,
                "mpn": spec.manufacturer_part_number,
                "package": spec.package
            },
            "evidence": {
                "bomSourceSha256": spec.bom_source_sha256,
                "manufacturerUrl": spec.manufacturer_url
            }
        },
        "bodyMm": {
            "units": "mm",
            "length": spec.length_mm,
            "width": spec.width_mm,
            "height": spec.height_mm
        },
        "validation": {
            "sourceClaimsRequireBoardValidation": true,
            "bomManufacturerPartMatchVerified": false,
            "physicalBoardRevisionVerified": false,
            "packageFitVerified": false
        },
        "output": {
            "format": "VRML97",
            "geometry": "rectangular-body-envelope",
            "vertexCount": 8,
            "triangleCount": 12
        },
        "limitations": {
            "candidateEnvelopeOnly": true,
            "terminalsIncluded": false,
            "pinOneDetailIncluded": false,
            "footprintGenerated": false,
            "stepSolidGenerated": false,
            "sourcePlacementVerified": false,
            "photoEvidenceUsed": false
        }
    })
}

fn validate_spec_values(spec: &ComponentEnvelopeSpec) -> Result<(), EngineError> {
    validate_hash(&spec.board_sha256, "board.sha256")?;
    validate_commit(&spec.dataset_commit)?;
    validate_hash(&spec.bom_source_sha256, "bom.sourceSha256")?;
    validate_text(
        &spec.component_reference,
        "component.reference",
        MAX_IDENTITY_TEXT_BYTES,
    )?;
    validate_text(
        &spec.component_uuid,
        "component.uuid",
        MAX_IDENTITY_TEXT_BYTES,
    )?;
    validate_text(
        &spec.manufacturer,
        "bom.manufacturer",
        MAX_IDENTITY_TEXT_BYTES,
    )?;
    validate_text(
        &spec.manufacturer_part_number,
        "bom.manufacturerPartNumber",
        MAX_IDENTITY_TEXT_BYTES,
    )?;
    validate_text(&spec.package, "bom.package", MAX_IDENTITY_TEXT_BYTES)?;
    validate_manufacturer_url(&spec.manufacturer_url)?;
    validate_dimension(spec.length_mm, "dimensions.lengthMm")?;
    validate_dimension(spec.width_mm, "dimensions.widthMm")?;
    validate_dimension(spec.height_mm, "dimensions.heightMm")?;
    Ok(())
}

fn validate_hash(value: &str, field: &str) -> Result<(), EngineError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(EngineError::new(format!(
            "{field} must be a 64-character lowercase SHA-256 hex digest"
        )));
    }
    Ok(())
}

fn validate_commit(value: &str) -> Result<(), EngineError> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(EngineError::new(
            "evidence.datasetCommit must be a 40-character lowercase Git commit",
        ));
    }
    Ok(())
}

fn validate_text(value: &str, field: &str, max_bytes: usize) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(EngineError::new(format!(
            "{field} must be nonblank, control-free, and at most {max_bytes} bytes"
        )));
    }
    Ok(())
}

fn validate_manufacturer_url(value: &str) -> Result<(), EngineError> {
    let authority_and_path = value
        .strip_prefix("https://")
        .ok_or_else(|| EngineError::new("bom.manufacturerUrl must be an absolute HTTPS URL"))?;
    let authority = authority_and_path
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if value.len() > MAX_MANUFACTURER_URL_BYTES
        || authority.is_empty()
        || authority.contains('@')
        || value.contains('\\')
        || value.chars().any(char::is_whitespace)
        || value.chars().any(char::is_control)
    {
        return Err(EngineError::new(
            "bom.manufacturerUrl must have a nonempty HTTPS host and no credentials or whitespace",
        ));
    }
    Ok(())
}

fn validate_dimension(value: f64, field: &str) -> Result<(), EngineError> {
    if !value.is_finite() || value <= 0.0 || value > MAX_DIMENSION_MM || value / 2.0 == 0.0 {
        return Err(EngineError::new(format!(
            "{field} must be finite, positive, representable as a nondegenerate extent, and no greater than {MAX_DIMENSION_MM} mm"
        )));
    }
    Ok(())
}

fn number(value: f64) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_mesh::{MeshLimits, ModelUnits, parse_vrml_mesh};

    const FIXTURE: &str = "tests/fixtures/assembly-e2e/single-resistor.kicad_pcb";

    fn board() -> PhysicalBoardDocument {
        crate::load_kicad_board(FIXTURE).expect("fixture board imports")
    }

    fn valid_spec_json(board: &PhysicalBoardDocument) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "schema": COMPONENT_ENVELOPE_SCHEMA,
            "board": {
                "sha256": board.source().sha256
            },
            "component": {
                "reference": "R1",
                "uuid": "assembly-r1",
                "manufacturer": "Example Manufacturer",
                "mpn": "EXAMPLE-0603",
                "package": "0603"
            },
            "evidence": {
                "datasetCommit": "a".repeat(40),
                "bomSourceSha256": "b".repeat(64),
                "manufacturerUrl": "https://example.com/package"
            },
            "bodyMm": {
                "length": 3.0,
                "width": 3.0,
                "height": 0.85
            }
        }))
        .expect("valid test JSON")
    }

    #[test]
    fn strict_parser_rejects_unknown_and_duplicate_keys() {
        let board = board();
        let valid = String::from_utf8(valid_spec_json(&board)).expect("UTF-8");

        let unknown = valid.replacen(
            "\"schema\":\"tokito.component-envelope.v1\"",
            "\"schema\":\"tokito.component-envelope.v1\",\"unexpected\":true",
            1,
        );
        assert!(parse_component_envelope_spec(unknown.as_bytes()).is_err());

        let duplicate = valid.replacen(
            "\"schema\":\"tokito.component-envelope.v1\"",
            "\"schema\":\"tokito.component-envelope.v1\",\"schema\":\"tokito.component-envelope.v1\"",
            1,
        );
        assert!(parse_component_envelope_spec(duplicate.as_bytes()).is_err());
        assert!(
            parse_component_envelope_spec(&vec![b' '; MAX_COMPONENT_ENVELOPE_SPEC_BYTES + 1])
                .is_err()
        );
    }

    #[test]
    fn dimensions_and_board_identity_are_checked() {
        let board = board();
        let valid = valid_spec_json(&board);
        let value: Value = serde_json::from_slice(&valid).expect("test value");
        for (field, bad_value) in [
            ("length", json!(0.0)),
            ("width", json!(-1.0)),
            ("height", json!(100.01)),
        ] {
            let mut invalid = value.clone();
            invalid["bodyMm"][field] = bad_value;
            let bytes = serde_json::to_vec(&invalid).expect("test JSON");
            assert!(parse_component_envelope_spec(&bytes).is_err());
        }

        let spec = parse_component_envelope_spec(&valid).expect("valid spec");
        validate_component_envelope(&spec, &board).expect("identity matches fixture");

        let mut wrong_board = value.clone();
        wrong_board["board"]["sha256"] = json!("c".repeat(64));
        let wrong_board = parse_component_envelope_spec(&serde_json::to_vec(&wrong_board).unwrap())
            .expect("well-formed mismatched hash");
        assert!(validate_component_envelope(&wrong_board, &board).is_err());

        let mut wrong_reference = value.clone();
        wrong_reference["component"]["reference"] = json!("R99");
        let wrong_reference =
            parse_component_envelope_spec(&serde_json::to_vec(&wrong_reference).unwrap())
                .expect("well-formed mismatched reference");
        assert!(validate_component_envelope(&wrong_reference, &board).is_err());

        let mut wrong_uuid = value;
        wrong_uuid["component"]["uuid"] = json!("different-uuid");
        let wrong_uuid = parse_component_envelope_spec(&serde_json::to_vec(&wrong_uuid).unwrap())
            .expect("well-formed mismatched UUID");
        assert!(validate_component_envelope(&wrong_uuid, &board).is_err());
    }

    #[test]
    fn envelope_3x3x0_85_is_a_bounded_8_vertex_12_triangle_mesh() {
        let board = board();
        let spec = parse_component_envelope_spec(&valid_spec_json(&board)).expect("valid spec");
        validate_component_envelope(&spec, &board).expect("identity matches fixture");

        let wrl = generate_envelope_wrl(&spec);
        let mesh = parse_vrml_mesh(
            wrl.as_bytes(),
            ModelUnits::Millimetres,
            MeshLimits::default(),
        )
        .expect("generated envelope satisfies the existing VRML profile");
        let part = &mesh.parts[0];
        assert_eq!(part.positions_mm.len(), 8);
        assert_eq!(part.triangles.len(), 12);
        let bounds = |axis: usize| {
            let mut values = part.positions_mm.iter().map(|point| point[axis]);
            let first = values.next().expect("vertex");
            values.fold((first, first), |(min, max), value| {
                (min.min(value), max.max(value))
            })
        };
        assert!((bounds(0).1 - bounds(0).0 - 3.0).abs() < 1e-12);
        assert!((bounds(1).1 - bounds(1).0 - 3.0).abs() < 1e-12);
        assert!((bounds(2).1 - bounds(2).0 - 0.85).abs() < 1e-12);
        assert_eq!(wrl, generate_envelope_wrl(&spec));

        let report = envelope_report_json(&spec);
        assert_eq!(report["status"], "candidate-envelope-only");
        assert_eq!(report["limitations"]["terminalsIncluded"], false);
        assert_eq!(report["limitations"]["pinOneDetailIncluded"], false);
        assert_eq!(report["limitations"]["footprintGenerated"], false);
        assert_eq!(report["validation"]["physicalBoardRevisionVerified"], false);
        assert_eq!(report["validation"]["packageFitVerified"], false);
    }
}
