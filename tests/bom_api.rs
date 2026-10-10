//! Synthetic ingestion/reconciliation tests, not physical population evidence.
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tokito_cli::bom::{
    BomFormat, MAX_BOM_BYTES, MAX_BOM_FIELD_BYTES, MAX_BOM_REFERENCES, MAX_BOM_ROWS, parse_bom,
    reconcile,
};

fn item(id: &str, refs: &[&str], quantity: u64) -> Value {
    json!({"id":id,"sheet":"test","row":15,"range":"A15:K15","source_columns":{"Item":1,"Quantity":quantity,"AgilePN":"candidate","Description":"test component","MFG":"Vendor","MFG_PN":"part","VALUE":"1k","ASSY_OPT":null,"FIND#":null,"PRE_APN":null,"Reference":refs.join(",")},"reference_designators":refs,"dnp_explicit":false,"manufacturer_part_number_text":"part","manufacturer_part_number_cell_type":"s"})
}
fn root(items: Vec<Value>) -> Value {
    json!({"schema_version":"1.0","source":{"path":"test.xlsx","sha256":"0000000000000000000000000000000000000000000000000000000000000000","bytes":1},"columns":["Item","Quantity","AgilePN","Description","MFG","MFG_PN","VALUE","ASSY_OPT","FIND#","PRE_APN","Reference"],"items":items})
}
fn nxp(value: Value) -> tokito_cli::bom::BomDocument {
    parse_bom(&serde_json::to_vec(&value).unwrap(), BomFormat::NxpJson).unwrap()
}
struct BoardInput(PathBuf);
static NEXT_BOARD_INPUT: AtomicU64 = AtomicU64::new(0);
impl BoardInput {
    fn new(footprints: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/bom-api-tests");
        fs::create_dir_all(&dir).unwrap();
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_BOARD_INPUT.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(
            "bom-{}-{time}-{sequence}.kicad_pcb",
            std::process::id()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        write!(file,"(kicad_pcb (version 20240108) (generator pcbnew) (general (thickness 1.6)) (layers (0 F.Cu signal) (31 B.Cu signal) (44 Edge.Cuts user)) {footprints})").unwrap();
        Self(path)
    }
    fn load(&self) -> tokito_cli::PhysicalBoardDocument {
        tokito_cli::load_board_input(&self.0).unwrap()
    }
}
impl Drop for BoardInput {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn footprint(reference: &str, uuid: &str) -> String {
    format!(
        "(footprint test (layer F.Cu) (uuid {uuid}) (at 1 2) (property Reference {reference}) (property Value 1k))"
    )
}

#[test]
fn grouped_rows_join_exact_uuid_and_quantity_diagnostics_remain_separate() {
    let input = BoardInput::new(
        &(footprint("R1", "uuid-r1") + &footprint("R2", "uuid-r2") + &footprint("R3", "uuid-r3")),
    );
    let bom = nxp(root(vec![item("row1", &["R1", "R2"], 3)]));
    let report = reconcile(&input.load(), &bom);
    assert_eq!(report["matchedIdentityCount"], 2);
    assert_eq!(report["boardMissingFromBomCount"], 1);
    assert_eq!(report["rows"][0]["quantityMismatch"], true);
    assert_eq!(
        report["rows"][0]["referenceJoins"][1]["boardUuid"],
        "uuid-r2"
    );
    for flag in [
        "boardRevisionMatchVerified",
        "manufacturerPartMatchVerified",
        "modelVerified",
        "poseVerified",
        "geometryVerified",
    ] {
        assert_eq!(report[flag], false);
    }
    assert_eq!(bom.source_sha256().len(), 64);
    assert_eq!(bom.entries()[0].source_row_id, "row1");
}
#[test]
fn duplicate_bom_and_pcb_identities_are_not_joined() {
    let input = BoardInput::new(
        &(footprint("R1", "same-uuid")
            + &footprint("R2", "same-uuid")
            + &footprint("R3", "u3")
            + &footprint("R3", "u4")),
    );
    let bom = nxp(root(vec![item(
        "repeat",
        &["R1", "R1", "R2", "R3", "missing"],
        5,
    )]));
    let report = reconcile(&input.load(), &bom);
    assert_eq!(report["matchedIdentityCount"], 0);
    assert_eq!(report["ambiguousReferenceCount"], 4);
    assert_eq!(report["unmatchedBomReferenceCount"], 1);
    let joins = report["rows"][0]["referenceJoins"].as_array().unwrap();
    assert_eq!(joins[0]["status"], "ambiguous_bom_reference");
    assert_eq!(joins[2]["status"], "ambiguous_board_uuid");
    assert_eq!(joins[3]["status"], "ambiguous_board_reference");
    assert!(joins.iter().all(|j| j["boardUuid"].is_null()));
}
#[test]
fn source_order_missing_uuid_case_and_mpn_conflicts_are_explicit() {
    let input = BoardInput::new("(footprint test (layer F.Cu) (at 1 2) (property Reference R1))");
    let mut source = item("row1", &["R1", "r1"], 2);
    source["manufacturer_part_number_text"] = json!("different");
    let bom = nxp(root(vec![source]));
    let report = reconcile(&input.load(), &bom);
    assert_eq!(
        report["rows"][0]["referenceJoins"][0]["status"],
        "missing_board_uuid"
    );
    assert_eq!(
        report["rows"][0]["referenceJoins"][1]["status"],
        "unmatched_bom_reference"
    );
    assert_eq!(report["rows"][0]["manufacturerPartNumberConflict"], true);
}
#[test]
fn csv_quotes_commas_escaped_quotes_and_unknown_dnp_are_preserved() {
    let bom = parse_bom(
        b"footprint_index,value,footprint,reference\r\n0,\"a,b\"\"c\nvalue\",\"test,fp\",R1\r\n",
        BomFormat::Esp32Csv,
    )
    .unwrap();
    assert_eq!(bom.entries()[0].source_fields["value"], "a,b\"c\nvalue");
    assert_eq!(bom.entries()[0].source_fields["footprint"], "test,fp");
    assert_eq!(bom.entries()[0].dnp_explicit, None);
    assert_eq!(bom.entries()[0].source_quantity, None);
    let input = BoardInput::new(&footprint("R1", "u1"));
    let report = reconcile(&input.load(), &bom);
    assert_eq!(report["rows"][0]["dnpStatus"], "unknown");
    assert!(report["rows"][0]["quantityMismatch"].is_null());
}
#[test]
fn no_part_to_order_and_placeholders_do_not_claim_a_body_or_population() {
    let mut row = item("row1", &["BH1"], 1);
    row["source_columns"]["MFG"] = json!("-");
    row["source_columns"]["MFG_PN"] = json!("-");
    row["manufacturer_part_number_text"] = json!("-");
    row["source_columns"]["Description"] = json!("MOUNTING HOLE NO PART TO ORDER");
    row["source_columns"]["ASSY_OPT"] = json!("DNP?");
    let bom = nxp(root(vec![row]));
    assert!(bom.entries()[0].manufacturer_candidates.is_empty());
    assert!(
        bom.entries()[0]
            .manufacturer_part_number_candidates
            .is_empty()
    );
    assert_eq!(bom.entries()[0].dnp_explicit, Some(false));
    assert!(bom.entries()[0].no_part_to_order_evidence.is_some());
    let input = BoardInput::new(&footprint("BH1", "u1"));
    let report = reconcile(&input.load(), &bom);
    assert_eq!(report["rows"][0]["assemblyOptionInterpretation"], "unknown");
}
#[test]
fn malformed_fields_duplicate_json_keys_and_csv_records_refuse() {
    assert!(
        parse_bom(
            br#"{"schema_version":"1.0","schema_version":"2.0"}"#,
            BomFormat::NxpJson
        )
        .is_err()
    );
    for change in ["refs", "quantity", "dnp", "column", "rootkey"] {
        let mut value = root(vec![item("r", &["R1"], 1)]);
        match change {
            "refs" => value["items"][0]["reference_designators"] = json!(["R2"]),
            "quantity" => value["items"][0]["source_columns"]["Quantity"] = json!(-1),
            "dnp" => value["items"][0]["dnp_explicit"] = json!("yes"),
            "column" => value["columns"][0] = json!("Unknown"),
            _ => value["extra"] = json!("unknown"),
        }
        assert!(
            parse_bom(&serde_json::to_vec(&value).unwrap(), BomFormat::NxpJson).is_err(),
            "{change}"
        );
    }
    for row in [
        "0,\"unclosed,test,R1",
        "0,ab\"cd,test,R1",
        "0,\"value\"x,test,R1",
        "0,value,test,R1,extra",
        "0,value,test,\"R1,R2\"",
    ] {
        assert!(
            parse_bom(
                format!("footprint_index,value,footprint,reference\n{row}").as_bytes(),
                BomFormat::Esp32Csv
            )
            .is_err()
        );
    }
}
#[test]
fn byte_field_row_and_expanded_reference_caps_refuse() {
    assert!(parse_bom(&vec![b' '; MAX_BOM_BYTES + 1], BomFormat::Esp32Csv).is_err());
    assert!(
        parse_bom(
            format!(
                "footprint_index,value,footprint,reference\n0,{},test,R1",
                "x".repeat(MAX_BOM_FIELD_BYTES + 1)
            )
            .as_bytes(),
            BomFormat::Esp32Csv
        )
        .is_err()
    );
    let rows = format!(
        "footprint_index,value,footprint,reference\n{}",
        "0,value,test,R1\n".repeat(MAX_BOM_ROWS + 1)
    );
    assert!(parse_bom(rows.as_bytes(), BomFormat::Esp32Csv).is_err());
    let mut value = root(vec![item("r", &["R1"], 1)]);
    value["items"][0]["reference_designators"] = json!(vec!["R1"; MAX_BOM_REFERENCES + 1]);
    assert!(parse_bom(&serde_json::to_vec(&value).unwrap(), BomFormat::NxpJson).is_err());
}

#[test]
fn explicit_dnp_and_numeric_mpn_text_remain_source_evidence() {
    let mut row = item("numeric", &["Y1"], 1);
    row["source_columns"]["MFG_PN"] = json!(830108212309u64);
    row["manufacturer_part_number_text"] = json!("830108212309");
    row["manufacturer_part_number_cell_type"] = json!("n");
    row["dnp_explicit"] = json!(true);
    let bom = nxp(root(vec![row]));
    assert_eq!(
        bom.entries()[0].manufacturer_part_number_candidates,
        vec!["830108212309"]
    );
    assert_eq!(bom.entries()[0].dnp_explicit, Some(true));
    assert_eq!(
        bom.entries()[0].source_fields["source_columns"]["MFG_PN"],
        json!(830108212309u64)
    );
}
#[test]
fn board_candidate_details_are_bounded_without_losing_total() {
    let footprints = (0..40)
        .map(|i| footprint("R1", &format!("uuid-{i}")))
        .collect::<String>();
    let input = BoardInput::new(&footprints);
    let bom = nxp(root(vec![item("row1", &["R1"], 1)]));
    let report = reconcile(&input.load(), &bom);
    let join = &report["rows"][0]["referenceJoins"][0];
    assert_eq!(join["status"], "ambiguous_board_reference");
    assert_eq!(join["boardCandidateOrdinalsTotal"], 40);
    assert_eq!(join["boardCandidateOrdinalsTruncated"], true);
    assert_eq!(join["boardCandidateOrdinals"].as_array().unwrap().len(), 32);
    assert_eq!(report["matchedIdentityCount"], 0);
}

#[test]
fn csv_accepts_single_encoding_signature_and_hashes_original_bytes() {
    let text = "footprint_index,value,footprint,reference\r\n0,1k,test,R1\r\n";
    let plain = parse_bom(text.as_bytes(), BomFormat::Esp32Csv).unwrap();
    let prefixed = format!("\u{feff}{text}");
    let signed = parse_bom(prefixed.as_bytes(), BomFormat::Esp32Csv).unwrap();
    assert_eq!(signed.entries()[0].references, vec!["R1"]);
    assert_eq!(
        signed.entries()[0].source_fields,
        plain.entries()[0].source_fields
    );
    assert_eq!(signed.source_bytes(), plain.source_bytes() + 3);
    assert_ne!(signed.source_sha256(), plain.source_sha256());
    assert_eq!(
        signed.source_sha256(),
        "af409cc1f5ce2b4bb0526831998c9dc5895b2bac69cbbadfa4768a928d56389c"
    );
}
#[test]
fn csv_does_not_trim_repeated_or_embedded_encoding_signatures() {
    for invalid in [
        "\u{feff}\u{feff}footprint_index,value,footprint,reference\n0,1k,test,R1",
        "footprint_\u{feff}index,value,footprint,reference\n0,1k,test,R1",
        " footprint_index,value,footprint,reference\n0,1k,test,R1",
    ] {
        assert!(parse_bom(invalid.as_bytes(), BomFormat::Esp32Csv).is_err());
    }
    let source = "footprint_index,value,footprint,reference\n0,va\u{feff}lue,test,R1";
    let bom = parse_bom(source.as_bytes(), BomFormat::Esp32Csv).unwrap();
    assert_eq!(bom.entries()[0].source_fields["value"], "va\u{feff}lue");
}
