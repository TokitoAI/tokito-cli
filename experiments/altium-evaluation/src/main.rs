#![forbid(unsafe_code)]
//! Bounded source evidence and candidate joins; no qualified geometry or pose.
mod assembly;
mod association;
mod board_inputs;
mod embedded;
mod pose_inputs;
mod records;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read},
    path::Path,
};

const MAX_FILE: usize = 32 * 1024 * 1024;
const MAX_STREAM: usize = 4 * 1024 * 1024;
const MAX_SPANS: usize = 2048;

fn property_spans(bytes: &[u8]) -> Result<Vec<Value>, String> {
    let mut spans = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        if bytes[start] != b'|' {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_graphic() || bytes[end] == b' ') {
            end += 1;
        }
        let text = std::str::from_utf8(&bytes[start..end]).map_err(|e| e.to_string())?;
        if text.contains('=') {
            if spans.len() == MAX_SPANS {
                return Err("property_span_limit_exceeded".into());
            }
            spans.push(
                json!({"byteOffset":start,"byteLength":end-start,"text":text,
                "recordFramingVerified":false,"componentAssociationVerified":false}),
            );
        }
        start = end.max(start + 1);
    }
    Ok(spans)
}

fn inspect(path: &Path) -> Result<Value, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE {
        return Err("input_byte_limit_exceeded".into());
    }
    if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1") {
        return Err("git_lfs_pointer_not_materialized".into());
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let mut compound = cfb::OpenOptions::new()
        .max_buffer_size(64 * 1024)
        .open_with(Cursor::new(bytes))
        .map_err(|e| format!("CFB_read_error:{e}"))?;
    let mut inventory = Vec::new();
    for entry in compound.walk() {
        if inventory.len() >= 4096 {
            return Err("entry_count_limit_exceeded".into());
        }
        if entry.is_stream() {
            inventory.push((entry.path().to_path_buf(), entry.len()));
        }
    }
    let mut streams = Vec::new();
    let mut consumed = 0usize;
    let mut decompressed_total = 0usize;
    for (path, declared_len) in inventory {
        let name = path.to_string_lossy().replace('\\', "/");
        let relevant = matches!(
            name.as_str(),
            "/Board6/Data"
                | "/Models/Data"
                | "/ComponentBodies6/Data"
                | "/ShapeBasedBodies6/Data"
                | "/Components6/Data"
        );
        let mut report = json!({"path":name,"declaredBytes":declared_len});
        let embedded_ordinal = name
            .strip_prefix("/Models/")
            .and_then(|s| s.parse::<u32>().ok());
        if relevant || embedded_ordinal.is_some() {
            if declared_len > MAX_STREAM as u64 {
                return Err("selected_stream_byte_limit_exceeded".into());
            }
            let mut data = Vec::new();
            compound
                .open_stream(&path)
                .map_err(|e| e.to_string())?
                .take(MAX_STREAM as u64 + 1)
                .read_to_end(&mut data)
                .map_err(|e| e.to_string())?;
            consumed += data.len();
            if data.len() > MAX_STREAM || consumed > MAX_FILE {
                return Err("selected_stream_budget_exceeded".into());
            }
            report["sha256"] = json!(format!("{:x}", Sha256::digest(&data)));
            report["prefixHex"] = json!(data
                .iter()
                .take(64)
                .map(|b| format!("{b:02x}"))
                .collect::<String>());
            if let Some(ordinal) = embedded_ordinal {
                let decoded = embedded::decode_with_limit(&data, embedded::MAX_MODEL)?;
                decompressed_total += decoded.len();
                if decompressed_total > 128 * 1024 * 1024 {
                    return Err("aggregate_embedded_output_limit_exceeded".into());
                }
                report["embeddedStep"] = json!({"streamOrdinal":ordinal,"decompressedBytes":decoded.len(),
                    "decompressedSha256":format!("{:x}",Sha256::digest(&decoded)),"completeZlibStream":true,
                    "geometryQualified":false});
            } else {
                report["propertySpans"] = json!(property_spans(&data)?);
                let framed = if name == "/Board6/Data" {
                    records::inspect_ascii(&data)
                } else {
                    records::inspect(&data, name == "/ComponentBodies6/Data")
                };
                report["framedRecordInspection"] = match framed {
                    Ok(records) => {
                        json!({"records":records,"profileFramingResolved":true,"geometryAndPoseVerified":false})
                    }
                    Err(error) => json!({"profileFramingResolved":false,"error":error}),
                };
            }
        }
        streams.push(report);
    }
    let mut associations = association::inspect(&streams).unwrap_or_else(|error|
        json!({"schema":"tokito.altium-candidate-associations.v1","error":error,"assemblyAdmitted":false}));
    associations["sourceSha256"] = json!(hash);
    let mut board_inputs = board_inputs::inspect(&streams).unwrap_or_else(|error|
        json!({"schema":"tokito.altium-board-source-inputs.v1","error":error,"numericInputsResolved":false,"geometryQualified":false}));
    board_inputs["sourceSha256"] = json!(hash);
    Ok(
        json!({"schema":"tokito.altium-source-evidence.v1","sourceSha256":hash,
        "reader":"cfb=0.14.0","streams":streams,"candidateAssociations":associations,"boardGeometryInputs":board_inputs,"admitted":false,"exportsProduced":[],
        "componentModelLinksVerified":false,"bodyRecordLayoutDecoded":false,
        "scope":"CFB streams, observed bounded record profile and candidate identity joins; geometry and pose unqualified"}),
    )
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "export-prototype") {
        let result = if args.len() == 5 && args[4] == "--allow-partial" {
            inspect(Path::new(&args[1])).and_then(|source| {
                assembly::export(&source, Path::new(&args[2]), Path::new(&args[3]))
            })
        } else {
            Err("usage: tokito-altium-evaluation export-prototype <materialized.PcbDoc> <positioned-mesh-report-directory> <new-output-directory> --allow-partial; qualified assembly unavailable".into())
        };
        match result {
            Ok(report) => {
                println!("{report}");
                return;
            }
            Err(error) => {
                println!("{}", json!({"error":error,"admitted":false}));
                std::process::exit(2);
            }
        }
    }
    let result = if args.len() == 1 {
        inspect(Path::new(&args[0]))
    } else {
        Err("usage: tokito-altium-evaluation <materialized.PcbDoc>".into())
    };
    let report = result.unwrap_or_else(|e| json!({"schema":"tokito.altium-source-evidence.v1","error":e,"admitted":false,"exportsProduced":[]}));
    println!("{report}");
    std::process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offsets_preserve_binary_framing_and_do_not_claim_links() {
        let bytes = b"\x02\x03|MODELID=A|NAME=x\0\xff|MODEL.2D.X=10mil\0";
        let spans = property_spans(bytes).unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0]["byteOffset"], 2);
        assert_eq!(spans[0]["recordFramingVerified"], false);
        assert_eq!(spans[1]["text"], "|MODEL.2D.X=10mil");
    }
    #[test]
    fn unrelated_ascii_is_not_a_property_record() {
        assert!(property_spans(b"MODELID=unused\0|noassignment\0")
            .unwrap()
            .is_empty());
    }
    #[test]
    fn too_many_spans_refuse_instead_of_silent_truncation() {
        let bytes = b"|A=B\0".repeat(MAX_SPANS + 1);
        assert_eq!(
            property_spans(&bytes).unwrap_err(),
            "property_span_limit_exceeded"
        );
    }
}
