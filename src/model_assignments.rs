//! Explicit offline model assignments bound to exact PCB source identities.
//! Supplied assets, units, transforms and population remain user claims.
use crate::component_pose::ModelTransformMm;
use crate::document::Component;
use crate::model_mesh::{MeshLimits, ModelUnits, parse_vrml_mesh};
use crate::model_resolver::{ModelAssetFormat, ModelResolver, ResolvedModelAsset};
use crate::{EngineError, PhysicalBoardDocument, read_bounded_file, sha256_hex, strict_json};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

pub const MAX_ASSIGNMENT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_ASSIGNMENTS: usize = 10_000;
pub const MAX_FIELD_BYTES: usize = 4096;
const MAX_AGGREGATE_ASSET_BYTES: usize = 128 * 1024 * 1024;
const SCHEMA: &str = "tokito.model-assignments.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignedModelUnits {
    Millimetres,
    KiCadLegacyWrlDeciinch,
    StepDeclared,
}
impl AssignedModelUnits {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Millimetres => "millimetres",
            Self::KiCadLegacyWrlDeciinch => "kicad-wrl",
            Self::StepDeclared => "step-declared",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AssignedModel {
    path: String,
    asset_sha256: String,
    units: AssignedModelUnits,
    transform: ModelTransformMm,
}
impl AssignedModel {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn asset_sha256(&self) -> &str {
        &self.asset_sha256
    }
    pub fn units(&self) -> AssignedModelUnits {
        self.units
    }
    pub fn transform(&self) -> ModelTransformMm {
        self.transform
    }
}
#[derive(Clone, Debug)]
pub struct ModelAssignment {
    component_ordinal: usize,
    component_uuid: Option<String>,
    reference: Option<String>,
    population: String,
    evidence: Option<String>,
    model: Option<AssignedModel>,
}
impl ModelAssignment {
    pub fn component_ordinal(&self) -> usize {
        self.component_ordinal
    }
    pub fn component_uuid(&self) -> Option<&str> {
        self.component_uuid.as_deref()
    }
    pub fn reference(&self) -> Option<&str> {
        self.reference.as_deref()
    }
    pub fn population(&self) -> &str {
        &self.population
    }
    pub fn evidence(&self) -> Option<&str> {
        self.evidence.as_deref()
    }
    pub fn model(&self) -> Option<&AssignedModel> {
        self.model.as_ref()
    }
}
/// Ingestion limits cannot be bypassed by external record construction.
/// ```compile_fail
/// fn bypass(document:&mut tokito_cli::model_assignments::ModelAssignmentsDocument) {
///     document.assignments.clear();
/// }
/// ```
#[derive(Clone, Debug)]
pub struct ModelAssignmentsDocument {
    source_board_sha256: String,
    source_sha256: String,
    source_bytes: usize,
    assignments: Vec<ModelAssignment>,
}
impl ModelAssignmentsDocument {
    pub fn source_board_sha256(&self) -> &str {
        &self.source_board_sha256
    }
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    pub fn source_bytes(&self) -> usize {
        self.source_bytes
    }
    pub fn assignments(&self) -> &[ModelAssignment] {
        &self.assignments
    }
}
#[derive(Clone, Debug)]
pub struct ValidatedAssignment {
    assignment: ModelAssignment,
    asset: Option<Arc<ResolvedModelAsset>>,
    status: String,
    supported_geometry: bool,
    source_unit_metres: Option<f64>,
    geometry_diagnostics: Vec<String>,
}
impl ValidatedAssignment {
    pub fn assignment(&self) -> &ModelAssignment {
        &self.assignment
    }
    pub fn asset(&self) -> Option<&ResolvedModelAsset> {
        self.asset.as_deref()
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    /// Means the format-specific bounded reader produced candidate geometry,
    /// never physical equivalence or engineering qualification.
    pub fn supported_geometry(&self) -> bool {
        self.supported_geometry
    }
    pub fn source_unit_metres(&self) -> Option<f64> {
        self.source_unit_metres
    }
    pub fn geometry_diagnostics(&self) -> &[String] {
        &self.geometry_diagnostics
    }
}
#[derive(Clone, Debug)]
pub struct ValidatedModelAssignments {
    source_board_sha256: String,
    manifest_sha256: String,
    assignments: Vec<ValidatedAssignment>,
}
impl ValidatedModelAssignments {
    pub fn assignments(&self) -> &[ValidatedAssignment] {
        &self.assignments
    }
    pub fn source_board_sha256(&self) -> &str {
        &self.source_board_sha256
    }
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
    pub fn report(&self) -> Value {
        let rows:Vec<_>=self.assignments.iter().map(|row|{
            let entry=&row.assignment;let model=entry.model();let asset=row.asset();
            let mut result = json!({"sourceComponentOrdinal":entry.component_ordinal(),"componentUuid":entry.component_uuid(),"reference":entry.reference(),"populationClaim":entry.population(),"populationVerified":false,"evidenceClaim":entry.evidence(),"status":row.status(),"assetResolved":asset.is_some(),"assetSha256":asset.map(ResolvedModelAsset::sha256),"assetFormat":asset.map(|a|a.format().as_str()),"supportedGeometry":row.supported_geometry(),"modelUnitsClaim":model.map(|m|m.units().as_str()),"localTransformClaim":model.map(|m|json!({"scale":m.transform().scale,"rotationDeg":m.transform().rotation_deg,"offsetMm":m.transform().offset_mm})),"modelPhysicalMatchVerified":false,"placementVerified":false,"boardRevisionMatchVerified":false});
            // Keep the established WRL verification schema stable. These
            // diagnostics describe the new STEP reader only and are additive
            // exclusively for STEP assets.
            if asset.is_some_and(|asset| asset.format() == ModelAssetFormat::Step) {
                let object = result.as_object_mut().expect("json! creates an object");
                object.insert("sourceUnitMetres".into(), json!(row.source_unit_metres()));
                object.insert("geometryDiagnostics".into(), json!(row.geometry_diagnostics()));
            }
            result
        }).collect();
        json!({"schema":"tokito.model-assignment-validation.v1","sourceBoardSha256":self.source_board_sha256,"manifestSha256":self.manifest_sha256,"entryCount":rows.len(),"assignedModelCount":self.assignments.iter().filter(|a|a.assignment.model.is_some()).count(),"resolvedAssetCount":self.assignments.iter().filter(|a|a.asset.is_some()).count(),"supportedGeometryCount":self.assignments.iter().filter(|a|a.supported_geometry).count(),"assignments":rows,"sourceIdentityBound":true,"physicalQualificationVerified":false,"modelPhysicalMatchVerified":false,"placementVerified":false,"boardRevisionMatchVerified":false,"maxAggregateAssetBytes":MAX_AGGREGATE_ASSET_BYTES})
    }
}
fn fail(message: impl Into<String>) -> EngineError {
    EngineError::new(message)
}
fn keys(value: &Value, expected: &[&str]) -> Result<(), EngineError> {
    let object = value
        .as_object()
        .ok_or_else(|| fail("model assignment must be an object"))?;
    if object.len() != expected.len() || expected.iter().any(|k| !object.contains_key(*k)) {
        return Err(fail("model assignment keys differ from reviewed schema"));
    }
    Ok(())
}
fn text(value: &Value) -> Result<String, EngineError> {
    let s = value
        .as_str()
        .ok_or_else(|| fail("model assignment field must be a string"))?;
    if s.len() > MAX_FIELD_BYTES || s.chars().any(char::is_control) {
        return Err(fail("model assignment text field invalid or exceeds cap"));
    }
    Ok(s.to_owned())
}
fn optional_text(value: &Value) -> Result<Option<String>, EngineError> {
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(text(value)?))
    }
}
fn hash(value: &Value) -> Result<String, EngineError> {
    let h = text(value)?;
    if h.len() != 64
        || !h
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return Err(fail(
            "model assignment SHA256 must be 64 lowercase hexadecimal characters",
        ));
    }
    Ok(h)
}
fn vector(value: &Value, nonzero: bool) -> Result<[f64; 3], EngineError> {
    let a = value
        .as_array()
        .ok_or_else(|| fail("model assignment vector must have three numbers"))?;
    if a.len() != 3 {
        return Err(fail("model assignment vector must have three numbers"));
    }
    let mut out = [0.; 3];
    for (dest, src) in out.iter_mut().zip(a) {
        *dest = src
            .as_f64()
            .filter(|v| v.is_finite() && (!nonzero || *v != 0.))
            .ok_or_else(|| fail("model assignment transform must be finite with nonzero scale"))?;
    }
    Ok(out)
}

pub fn load_model_assignments(
    path: impl AsRef<Path>,
) -> Result<ModelAssignmentsDocument, EngineError> {
    parse_model_assignments(&read_bounded_file(path.as_ref(), MAX_ASSIGNMENT_BYTES)?)
}
pub fn parse_model_assignments(bytes: &[u8]) -> Result<ModelAssignmentsDocument, EngineError> {
    if bytes.len() > MAX_ASSIGNMENT_BYTES {
        return Err(fail("model assignment input byte cap exceeded"));
    }
    if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1") {
        return Err(fail(
            "model assignment input is an unmaterialized LFS pointer",
        ));
    }
    let source = std::str::from_utf8(bytes).map_err(|_| fail("model assignments must be UTF8"))?;
    let value =
        strict_json::parse(source).map_err(|_| fail("invalid strict model assignment JSON"))?;
    keys(&value, &["schema", "sourceBoardSha256", "assignments"])?;
    if value["schema"] != SCHEMA {
        return Err(fail("unsupported model assignment schema"));
    }
    let source_board_sha256 = hash(&value["sourceBoardSha256"])?;
    let rows = value["assignments"]
        .as_array()
        .ok_or_else(|| fail("model assignments must be an array"))?;
    if rows.len() > MAX_ASSIGNMENTS {
        return Err(fail("model assignment entry cap exceeded"));
    }
    let mut assignments = Vec::new();
    let mut ordinals = BTreeSet::new();
    let mut assigned_uuids = BTreeSet::new();
    let mut assigned_refs = BTreeSet::new();
    for row in rows {
        keys(
            row,
            &[
                "sourceComponentOrdinal",
                "componentUuid",
                "reference",
                "identityIssues",
                "population",
                "evidence",
                "model",
            ],
        )?;
        let ordinal = row["sourceComponentOrdinal"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| fail("component ordinal must be unsigned"))?;
        if !ordinals.insert(ordinal) {
            return Err(fail("duplicate component assignment ordinal"));
        }
        let uuid = optional_text(&row["componentUuid"])?;
        let reference = optional_text(&row["reference"])?;
        let issues = row["identityIssues"]
            .as_array()
            .ok_or_else(|| fail("identityIssues must be an array"))?;
        if issues.len() > 16 {
            return Err(fail("identity issue list cap exceeded"));
        }
        for issue in issues {
            text(issue)?;
        }
        let population = text(&row["population"])?;
        if !["unknown", "fitted", "not-fitted", "no-part-to-order"].contains(&population.as_str()) {
            return Err(fail("unsupported population claim"));
        }
        let evidence = optional_text(&row["evidence"])?;
        let model = if row["model"].is_null() {
            None
        } else {
            keys(
                &row["model"],
                &[
                    "path",
                    "sha256",
                    "units",
                    "scale",
                    "rotationDeg",
                    "offsetMm",
                ],
            )?;
            let path = text(&row["model"]["path"])?;
            if path.trim().is_empty() {
                return Err(fail("assigned model path is blank"));
            }
            let asset_sha256 = hash(&row["model"]["sha256"])?;
            let units = match row["model"]["units"].as_str() {
                Some("millimetres") => AssignedModelUnits::Millimetres,
                Some("kicad-wrl") => AssignedModelUnits::KiCadLegacyWrlDeciinch,
                Some("step-declared") => AssignedModelUnits::StepDeclared,
                _ => return Err(fail("assigned model units must be explicitly supported")),
            };
            let transform = ModelTransformMm {
                scale: vector(&row["model"]["scale"], true)?,
                rotation_deg: vector(&row["model"]["rotationDeg"], false)?,
                offset_mm: vector(&row["model"]["offsetMm"], false)?,
            };
            if uuid.as_deref().is_none_or(str::is_empty)
                || reference.as_deref().is_none_or(str::is_empty)
            {
                return Err(fail(
                    "assigned model requires explicit component UUID and reference",
                ));
            }
            if evidence.as_deref().is_none_or(|v| v.trim().is_empty()) {
                return Err(fail(
                    "assigned model requires an explicit evidence description claim",
                ));
            }
            if !assigned_uuids.insert(uuid.clone()) || !assigned_refs.insert(reference.clone()) {
                return Err(fail("duplicate assigned UUID or reference"));
            }
            Some(AssignedModel {
                path,
                asset_sha256,
                units,
                transform,
            })
        };
        assignments.push(ModelAssignment {
            component_ordinal: ordinal,
            component_uuid: uuid,
            reference,
            population,
            evidence,
            model,
        });
    }
    Ok(ModelAssignmentsDocument {
        source_board_sha256,
        source_sha256: sha256_hex(bytes),
        source_bytes: bytes.len(),
        assignments,
    })
}
fn identity_issues(
    component: &Component,
    uuid_counts: &BTreeMap<&str, usize>,
    ref_counts: &BTreeMap<&str, usize>,
) -> Vec<&'static str> {
    let mut issues = Vec::new();
    match component.source.source_uuid.as_deref() {
        None | Some("") => issues.push("missing_uuid"),
        Some(uuid) if uuid_counts.get(uuid).copied().unwrap_or(0) != 1 => {
            issues.push("duplicate_uuid")
        }
        _ => (),
    }
    match component.reference.as_deref() {
        None | Some("") => issues.push("missing_reference"),
        Some(reference) if reference.contains('*') || reference == "?" => {
            issues.push("placeholder_reference")
        }
        Some(reference) if ref_counts.get(reference).copied().unwrap_or(0) != 1 => {
            issues.push("duplicate_reference")
        }
        _ => (),
    }
    issues
}
fn identity_counts(
    board: &PhysicalBoardDocument,
) -> (BTreeMap<&str, usize>, BTreeMap<&str, usize>) {
    let mut uuids = BTreeMap::new();
    let mut refs = BTreeMap::new();
    for c in &board.engineering().components {
        if let Some(u) = c.source.source_uuid.as_deref() {
            *uuids.entry(u).or_default() += 1;
        }
        if let Some(r) = c.reference.as_deref() {
            *refs.entry(r).or_default() += 1;
        }
    }
    (uuids, refs)
}
pub fn assignment_template(board: &PhysicalBoardDocument) -> Result<Value, EngineError> {
    if board.engineering().components.len() > MAX_ASSIGNMENTS {
        return Err(fail("board component cap exceeded for assignments"));
    }
    let (uuids, refs) = identity_counts(board);
    let rows:Vec<_>=board.engineering().components.iter().map(|c|json!({"sourceComponentOrdinal":c.source.ordinal,"componentUuid":c.source.source_uuid.as_deref(),"reference":c.reference,"identityIssues":identity_issues(c,&uuids,&refs),"population":"unknown","evidence":null,"model":null})).collect();
    let value =
        json!({"schema":SCHEMA,"sourceBoardSha256":board.source().sha256,"assignments":rows});
    let bytes =
        serde_json::to_vec(&value).map_err(|_| fail("cannot encode assignment template"))?;
    // Round-trip through the same boundary: never emit a template exceeding caps.
    parse_model_assignments(&bytes)?;
    Ok(value)
}

pub fn validate_model_assignments(
    board: &PhysicalBoardDocument,
    document: &ModelAssignmentsDocument,
    resolver: &ModelResolver,
) -> Result<ValidatedModelAssignments, EngineError> {
    if document.source_board_sha256() != board.source().sha256 {
        return Err(fail(
            "model assignments are bound to a different board SHA256",
        ));
    }
    if board.engineering().components.len() > MAX_ASSIGNMENTS {
        return Err(fail("board component cap exceeded for assignments"));
    }
    let (uuids, refs) = identity_counts(board);
    let components: BTreeMap<_, _> = board
        .engineering()
        .components
        .iter()
        .map(|c| (c.source.ordinal, c))
        .collect();
    if components.len() != board.engineering().components.len() {
        return Err(fail("board component ordinals are ambiguous"));
    }
    let mut validated = Vec::new();
    let mut aggregate = 0usize;
    for entry in document.assignments() {
        let component = components
            .get(&entry.component_ordinal())
            .ok_or_else(|| fail("assigned component ordinal not present in board"))?;
        if entry.component_uuid() != component.source.source_uuid.as_deref()
            || entry.reference() != component.reference.as_deref()
        {
            return Err(fail(
                "model assignment UUID/reference conflicts with exact board component",
            ));
        }
        let Some(model) = entry.model() else {
            validated.push(ValidatedAssignment {
                assignment: entry.clone(),
                asset: None,
                status: "unassigned".into(),
                supported_geometry: false,
                source_unit_metres: None,
                geometry_diagnostics: Vec::new(),
            });
            continue;
        };
        if !identity_issues(component, &uuids, &refs).is_empty() {
            return Err(fail(
                "assigned component lacks unique non-placeholder source identity",
            ));
        }
        let asset = match resolver.resolve_selected_path(model.path()) {
            Ok(asset) => asset,
            Err(_) => {
                validated.push(ValidatedAssignment {
                    assignment: entry.clone(),
                    asset: None,
                    status: "asset_resolution_failed".into(),
                    supported_geometry: false,
                    source_unit_metres: None,
                    geometry_diagnostics: Vec::new(),
                });
                continue;
            }
        };
        if asset.sha256() != model.asset_sha256() {
            return Err(fail("assigned asset SHA256 conflicts with captured bytes"));
        }
        let asset_cap = match asset.format() {
            ModelAssetFormat::Step => crate::step_geometry::MAX_STEP_BYTES,
            ModelAssetFormat::Wrl | ModelAssetFormat::Glb | ModelAssetFormat::Gltf => {
                MeshLimits::default().max_bytes
            }
        };
        if asset.bytes().len() > asset_cap {
            return Err(fail("assigned asset exceeds supported bounded capture cap"));
        }
        aggregate = aggregate
            .checked_add(asset.bytes().len())
            .filter(|v| *v <= MAX_AGGREGATE_ASSET_BYTES)
            .ok_or_else(|| fail("aggregate assignment asset byte cap exceeded"))?;
        let (supported_geometry, status, source_unit_metres, geometry_diagnostics) =
            match (asset.format(), model.units()) {
                (ModelAssetFormat::Wrl, AssignedModelUnits::Millimetres) => {
                    let parsed = parse_vrml_mesh(
                        asset.bytes(),
                        ModelUnits::Millimetres,
                        MeshLimits::default(),
                    );
                    (
                        parsed.is_ok(),
                        if parsed.is_ok() {
                            "resolved_supported_wrl"
                        } else {
                            "resolved_geometry_rejected"
                        },
                        Some(0.001),
                        Vec::new(),
                    )
                }
                (ModelAssetFormat::Wrl, AssignedModelUnits::KiCadLegacyWrlDeciinch) => {
                    let parsed = parse_vrml_mesh(
                        asset.bytes(),
                        ModelUnits::KiCadLegacyWrlDeciinch,
                        MeshLimits::default(),
                    );
                    (
                        parsed.is_ok(),
                        if parsed.is_ok() {
                            "resolved_supported_wrl"
                        } else {
                            "resolved_geometry_rejected"
                        },
                        Some(0.00254),
                        Vec::new(),
                    )
                }
                (ModelAssetFormat::Step, AssignedModelUnits::StepDeclared) => {
                    match crate::step_geometry::parse_step_mesh(asset.bytes()) {
                        Ok(parsed) => (
                            true,
                            "resolved_supported_step_candidate",
                            Some(parsed.source_unit_metres),
                            parsed.diagnostics,
                        ),
                        Err(error) => (
                            false,
                            "resolved_geometry_rejected",
                            None,
                            vec![error.to_string()],
                        ),
                    }
                }
                (ModelAssetFormat::Step, _)
                | (ModelAssetFormat::Wrl, AssignedModelUnits::StepDeclared) => {
                    (false, "resolved_unit_format_mismatch", None, Vec::new())
                }
                _ => (
                    false,
                    "resolved_unsupported_production_geometry",
                    None,
                    Vec::new(),
                ),
            };
        validated.push(ValidatedAssignment {
            assignment: entry.clone(),
            asset: Some(Arc::new(asset)),
            status: status.into(),
            supported_geometry,
            source_unit_metres,
            geometry_diagnostics,
        });
    }
    Ok(ValidatedModelAssignments {
        source_board_sha256: document.source_board_sha256.clone(),
        manifest_sha256: document.source_sha256.clone(),
        assignments: validated,
    })
}
