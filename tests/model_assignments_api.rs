//! Synthetic identity/model inputs; never physical model qualification.
#[path = "support/test_scratch.rs"]
mod test_scratch;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tokito_cli::model_assignments::{
    MAX_ASSIGNMENT_BYTES, MAX_ASSIGNMENTS, MAX_FIELD_BYTES, assignment_template,
    parse_model_assignments, validate_model_assignments,
};
use tokito_cli::model_resolver::{ModelResolver, ModelResolverConfig};
const WRL:&[u8]=b"#VRML V2.0 utf8\nShape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0, 1 0 0, 0 1 0] } coordIndex [0, 1, 2, -1] } }\n";
const WRL_HASH: &str = "8f4b0cd95aa6f7a49371b2c0c012435729952cc2919189a14784ae30a0421343";
static FIXTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    root: PathBuf,
    parent: PathBuf,
    board: tokito_cli::PhysicalBoardDocument,
}
impl Fixture {
    fn new(footprints: &str) -> Self {
        let parent = test_scratch::directory("model-assignments-tests");
        let parent = parent.canonicalize().unwrap();
        #[cfg(windows)]
        let parent = PathBuf::from(parent.to_string_lossy().trim_start_matches(r"\\?\"));
        let root = parent.join(format!(
            "{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let boardfile = root.join("synthetic.kicad_pcb");
        fs::write(&boardfile,format!("(kicad_pcb (version 20240108) (generator pcbnew) (general (thickness 1.6)) (layers (0 F.Cu signal) (31 B.Cu signal) (44 Edge.Cuts user)) {footprints})")).unwrap();
        let board = tokito_cli::load_board_input(&boardfile).unwrap();
        Self {
            root,
            parent,
            board,
        }
    }
    fn resolver(&self) -> ModelResolver {
        ModelResolver::new(ModelResolverConfig::new(&self.root)).unwrap()
    }
    fn model(&self) {
        fs::write(self.root.join("model.wrl"), WRL).unwrap();
    }
    fn template(&self) -> Value {
        assignment_template(&self.board).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self
            .root
            .canonicalize()
            .ok()
            .is_some_and(|p| p.parent() == Some(self.parent.as_path()))
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
fn fp(reference: &str, uuid: &str) -> String {
    format!(
        "(footprint test (layer F.Cu) (uuid {uuid}) (at 1 2) (property Reference {reference}) (property Value 1k))"
    )
}
fn assigned(template: &mut Value) {
    template["assignments"][0]["population"] = json!("fitted");
    template["assignments"][0]["evidence"] =
        json!("Synthetic candidate for API test only; unverified physical match");
    template["assignments"][0]["model"] = json!({"path":"model.wrl","sha256":WRL_HASH,"units":"millimetres","scale":[1.,1.,1.],"rotationDeg":[0.,0.,0.],"offsetMm":[0.,0.,0.]});
}
fn parse(value: &Value) -> tokito_cli::model_assignments::ModelAssignmentsDocument {
    parse_model_assignments(&serde_json::to_vec(value).unwrap()).unwrap()
}
#[test]
fn template_preserves_anonymous_and_placeholder_records_without_models() {
    let f = Fixture::new(&(fp("REF**", "u1") + "(footprint test (layer F.Cu) (at 1 2))"));
    let t = f.template();
    let rows = t["assignments"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r["model"].is_null()));
    assert!(
        rows[0]["identityIssues"]
            .as_array()
            .unwrap()
            .contains(&json!("placeholder_reference"))
    );
    assert!(rows[1]["componentUuid"].is_null());
    assert!(rows[1]["reference"].is_null());
    let doc = parse(&t);
    assert_eq!(doc.assignments().len(), 2);
    let validation = validate_model_assignments(&f.board, &doc, &f.resolver()).unwrap();
    assert_eq!(validation.report()["supportedGeometryCount"], 0);
}
#[test]
fn exact_identity_hash_and_supported_wrl_remain_unqualified() {
    let f = Fixture::new(&fp("R1", "u1"));
    f.model();
    let mut template = f.template();
    assigned(&mut template);
    let before = f.board.engineering().components.clone();
    let doc = parse(&template);
    let result = validate_model_assignments(&f.board, &doc, &f.resolver()).unwrap();
    assert_eq!(f.board.engineering().components, before);
    let e = &result.assignments()[0];
    assert_eq!(e.assignment().component_uuid(), Some("u1"));
    assert_eq!(e.asset().unwrap().bytes(), WRL);
    assert!(e.supported_geometry());
    assert_eq!(e.status(), "resolved_supported_wrl");
    assert_eq!(
        e.assignment().model().unwrap().transform().scale,
        [1., 1., 1.]
    );
    let report = result.report();
    assert_eq!(report["resolvedAssetCount"], 1);
    assert_eq!(report["supportedGeometryCount"], 1);
    for flag in [
        "physicalQualificationVerified",
        "modelPhysicalMatchVerified",
        "placementVerified",
        "boardRevisionMatchVerified",
    ] {
        assert_eq!(report[flag], false);
    }
    assert_eq!(report["assignments"][0]["populationVerified"], false);
    assert!(
        !report
            .to_string()
            .contains(&f.root.to_string_lossy().to_string())
    );
    assert_eq!(doc.source_sha256().len(), 64);
}
#[test]
fn wrong_board_ref_uuid_or_hash_cannot_bind() {
    let f = Fixture::new(&fp("R1", "u1"));
    f.model();
    for field in ["board", "ref", "uuid", "hash", "ordinal"] {
        let mut t = f.template();
        assigned(&mut t);
        match field {
            "board" => {
                t["sourceBoardSha256"] =
                    json!("0000000000000000000000000000000000000000000000000000000000000000")
            }
            "ref" => t["assignments"][0]["reference"] = json!("r1"),
            "uuid" => t["assignments"][0]["componentUuid"] = json!("other"),
            "ordinal" => t["assignments"][0]["sourceComponentOrdinal"] = json!(9999),
            _ => {
                t["assignments"][0]["model"]["sha256"] =
                    json!("0000000000000000000000000000000000000000000000000000000000000000")
            }
        };
        assert!(
            validate_model_assignments(&f.board, &parse(&t), &f.resolver()).is_err(),
            "{field}"
        );
    }
}
#[test]
fn duplicate_assignments_and_ambiguous_board_identity_refuse() {
    let f = Fixture::new(&(fp("R1", "u1") + &fp("R1", "u2")));
    f.model();
    let mut t = f.template();
    assigned(&mut t);
    assert!(validate_model_assignments(&f.board, &parse(&t), &f.resolver()).is_err());
    let f = Fixture::new(&fp("R1", "u1"));
    let mut t = f.template();
    let row = t["assignments"][0].clone();
    t["assignments"].as_array_mut().unwrap().push(row);
    assert!(parse_model_assignments(&serde_json::to_vec(&t).unwrap()).is_err());
}
#[test]
fn explicit_units_and_finite_complete_transform_are_required() {
    let f = Fixture::new(&fp("R1", "u1"));
    for field in [
        "units",
        "scale",
        "rotation",
        "offset",
        "evidence",
        "blankpath",
    ] {
        let mut t = f.template();
        assigned(&mut t);
        match field {
            "units" => t["assignments"][0]["model"]["units"] = json!("auto"),
            "scale" => t["assignments"][0]["model"]["scale"] = json!([1, 0, 1]),
            "rotation" => t["assignments"][0]["model"]["rotationDeg"] = json!([0, 0]),
            "offset" => t["assignments"][0]["model"]["offsetMm"] = json!([0, null, 0]),
            "evidence" => t["assignments"][0]["evidence"] = Value::Null,
            _ => t["assignments"][0]["model"]["path"] = json!(""),
        };
        assert!(
            parse_model_assignments(&serde_json::to_vec(&t).unwrap()).is_err(),
            "{field}"
        );
    }
    let mut t = f.template();
    assigned(&mut t);
    t["assignments"][0]["model"]["scale"] = json!([-1, 1, 1]);
    assert!(parse_model_assignments(&serde_json::to_vec(&t).unwrap()).is_ok());
}
#[test]
fn strict_schema_field_entry_and_byte_limits_reject() {
    assert!(parse_model_assignments(&vec![b' '; MAX_ASSIGNMENT_BYTES + 1]).is_err());
    assert!(
        parse_model_assignments(br#"{"schema":"tokito.model-assignments.v1","schema":"bad"}"#)
            .is_err()
    );
    let f = Fixture::new(&fp("R1", "u1"));
    let mut t = f.template();
    t["assignments"][0]["evidence"] = json!("x".repeat(MAX_FIELD_BYTES + 1));
    assert!(parse_model_assignments(&serde_json::to_vec(&t).unwrap()).is_err());
    let mut t = f.template();
    t["assignments"] = json!(vec![t["assignments"][0].clone(); MAX_ASSIGNMENTS + 1]);
    assert!(parse_model_assignments(&serde_json::to_vec(&t).unwrap()).is_err());
    let mut t = f.template();
    t["schema"] = json!("tokito.model-assignments.v2");
    assert!(parse_model_assignments(&serde_json::to_vec(&t).unwrap()).is_err());
}
#[test]
fn asset_resolution_is_distinct_from_format_and_geometry_acceptance() {
    let f = Fixture::new(&fp("R1", "u1"));
    let mut t = f.template();
    assigned(&mut t);
    let result = validate_model_assignments(&f.board, &parse(&t), &f.resolver()).unwrap();
    assert!(!result.assignments()[0].supported_geometry());
    assert!(result.assignments()[0].asset().is_none());
    // STEP bytes use the same synthetic bytes/hash only to exercise unsupported-format reporting.
    fs::write(f.root.join("model.step"), WRL).unwrap();
    t["assignments"][0]["model"]["path"] = json!("model.step");
    let result = validate_model_assignments(&f.board, &parse(&t), &f.resolver()).unwrap();
    assert!(result.assignments()[0].asset().is_some());
    assert!(!result.assignments()[0].supported_geometry());
    assert_eq!(
        result.assignments()[0].status(),
        "resolved_unsupported_production_geometry"
    );
}
#[test]
fn assignment_assembly_places_only_explicit_candidates_and_keeps_real_source_identity() {
    let mut f = Fixture::new(&(fp("R1", "u1") + &fp("R2", "u2") + &fp("R3", "u3")));
    let pcb = format!(
        "(kicad_pcb (version 20240108) (general (thickness 1.6)) (layers (0 F.Cu signal) (31 B.Cu signal) (44 Edge.Cuts user)) (setup (stackup (layer F.Cu (type copper) (thickness 0.035)) (layer dielectric (type core) (thickness 1.53)) (layer B.Cu (type copper) (thickness 0.035)))) (gr_rect (start 0 0) (end 10 10) (stroke (width 0.05) (type default)) (fill none) (layer Edge.Cuts)) {}{}{})",
        fp("R1", "u1"),
        fp("R2", "u2").replace("layer F.Cu", "layer B.Cu"),
        fp("R3", "u3")
    );
    let path = f.root.join("complete.kicad_pcb");
    fs::write(&path, pcb).unwrap();
    f.board = tokito_cli::load_board_input(&path).unwrap();
    f.model();
    let mut template = f.template();
    assigned(&mut template);
    template["assignments"][1]["model"] = template["assignments"][0]["model"].clone();
    template["assignments"][1]["evidence"] = json!("Synthetic unverified candidate");
    template["assignments"][2]["population"] = json!("not-fitted");
    let validated = validate_model_assignments(&f.board, &parse(&template), &f.resolver()).unwrap();
    let output =
        tokito_cli::assigned_assembly::assemble_assigned_models(&f.board, &validated).unwrap();
    assert_eq!(output.emitted.len(), 2);
    assert_eq!(
        output.emitted[0].component_source,
        f.board.engineering().components[0].source
    );
    assert_eq!(output.emitted[1].manifest_entry_index, 1);
    assert!(output.emitted[0].mesh.parts[0].positions_m[0][1] > 0.0);
    assert!(output.emitted[1].mesh.parts[0].positions_m[0][1] < 0.0);
    assert_eq!(
        output.report["classificationCounts"]["population_claim_body_skipped"],
        1
    );
    assert_eq!(output.report["aggregateTriangleCount"], 2);
    assert_eq!(output.report["wholeBoardDigitalTwinComplete"], false);
    let other = Fixture::new(&fp("R1", "u1"));
    assert!(
        tokito_cli::assigned_assembly::assemble_assigned_models(&other.board, &validated).is_err()
    );
}
#[test]
fn assignment_assembly_reports_missing_stackup_and_absent_assignments() {
    let f = Fixture::new(&(fp("R1", "u1") + &fp("R2", "u2")));
    f.model();
    let mut t = f.template();
    assigned(&mut t);
    t["assignments"].as_array_mut().unwrap().pop();
    let v = validate_model_assignments(&f.board, &parse(&t), &f.resolver()).unwrap();
    let output = tokito_cli::assigned_assembly::assemble_assigned_models(&f.board, &v).unwrap();
    assert!(output.emitted.is_empty());
    assert_eq!(
        output.report["classificationCounts"]["missing_stackup_or_outline"],
        1
    );
    assert_eq!(
        output.report["classificationCounts"]["assignment_absent"],
        1
    );
    assert_eq!(output.report["componentDetailsTotal"], 2);
}
