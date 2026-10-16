//! Read-only source and outline checks, not a model or manufacturing admission gate.
use crate::{EngineError, cli_presentation::Style, load_kicad_board, project_intake};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(crate) fn run(args: &[String], style: Style) -> Result<(), EngineError> {
    let mut input = None;
    let mut selected = None;
    let mut machine = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--json" if !machine => machine = true,
            "--board" if selected.is_none() => {
                index += 1;
                selected = Some(
                    args.get(index)
                        .filter(|value| !value.starts_with('-'))
                        .ok_or_else(|| {
                            EngineError::new("doctor --board requires an exact PCB filename")
                        })?
                        .as_str(),
                );
            }
            value if !value.starts_with('-') && input.is_none() => {
                input = Some(PathBuf::from(value))
            }
            _ => {
                return Err(EngineError::new(
                    "usage: tokito-cli doctor [PROJECT|BOARD.kicad_pcb] [--board EXACT_FILENAME] [--json]; unknown or duplicate option",
                ));
            }
        }
        index += 1;
    }
    if selected.is_some() && !input.as_ref().is_some_and(|path| path.is_dir()) {
        return Err(EngineError::new(
            "doctor --board requires an existing project directory",
        ));
    }
    let mut checks = vec![
        json!({"code":"runtime", "status":"passed", "message":format!("Tokito CLI {} ({}/{})", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH)}),
    ];
    let mut source = Value::Null;
    if let Some(input) = input.as_ref() {
        match inspect_source(input, selected) {
            Ok((path, discovery)) => {
                if let Some(discovery) = discovery {
                    checks.push(discovery);
                }
                match load_kicad_board(&path) {
                    Ok(board) => {
                        source = json!({"path":path,"sha256":board.source().sha256,"formatVersion":board.source().format_version,
                            "components":board.engineering().components.len(),"modelReferences":board.engineering().models.len()});
                        checks.push(json!({"code":"pcb_import","status":"passed","message":"Imported supported PCB source fields."}));
                        let valid = board.outline().is_valid();
                        checks.push(json!({"code":"outline","status":if valid {"passed"} else {"failed"},
                            "message":if valid {"Physical outline is valid within the supported geometry scope."} else {"Outline is invalid or unsupported. Run validate outline BOARD --json for diagnostics."}}));
                        checks.push(json!({"code":"component_models","status":"not_checked", "message":"Model references were inventoried only. Use resolve models with explicit approved roots, then inspect export coverage."}));
                        if !board.diagnostics.is_empty() || !board.geometry.diagnostics().is_empty()
                        {
                            checks.push(json!({"code":"partial_import","status":"warning","message":"Import/render diagnostics remain; inspect BOARD --json reports the supported scope."}));
                        }
                    }
                    Err(error) => checks.push(
                        json!({"code":"pcb_import","status":"failed","message":error.to_string()}),
                    ),
                }
            }
            Err(error) => checks.push(
                json!({"code":"source_selection","status":"failed","message":error.to_string()}),
            ),
        }
    } else {
        checks.push(json!({"code":"project","status":"not_checked","message":"No project supplied. Run tokito-cli doctor PROJECT to check source selection and outline."}));
    }
    let failed = checks.iter().any(|check| check["status"] == "failed");
    let report = json!({"schema":"tokito.cli-doctor.v1","scope":if input.is_some() {"source_and_outline_only"} else {"runtime_information_only"},
        "status":if failed {"failed"} else {"passed"},"checks":checks,"source":source,
        "geometryQualified":false,"appAdmissionAuthorized":false,"arRegistrationVerified":false,
        "limitations":["Read-only and offline: no downloads, dependency installation, source changes, or KiCad runtime invocation.",
            "Does not qualify model availability, geometry, manufacturing, physical fit, App import or AR."]});
    if machine {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| EngineError::new(e.to_string()))?
        );
    } else {
        println!("{}", style.help("Tokito CLI doctor\nCHECKS:"));
        for check in report["checks"].as_array().expect("authored checks array") {
            println!(
                "  [{}] {}: {}",
                style.status(check["status"].as_str().unwrap_or("unknown")),
                check["code"].as_str().unwrap_or("check"),
                check["message"].as_str().unwrap_or("")
            );
        }
        println!(
            "Scope: {}. Model qualification and AR admission are not checked.",
            report["scope"].as_str().unwrap_or("unknown")
        );
    }
    if failed {
        Err(EngineError::new(
            "doctor found failed source/outline checks; see the report above",
        ))
    } else {
        Ok(())
    }
}

fn inspect_source(
    input: &Path,
    selected: Option<&str>,
) -> Result<(PathBuf, Option<Value>), EngineError> {
    if !input.exists() {
        return Err(EngineError::new(format!(
            "doctor input does not exist or is inaccessible: {}",
            input.display()
        )));
    }
    if !input.is_dir() {
        return Ok((input.to_path_buf(), None));
    }
    let inspection = project_intake::inspect_project(input, selected)?;
    if !inspection.accepted() {
        return Err(EngineError::new(format!(
            "project selection refused: {}. Use project inspect PROJECT --json and --board EXACT_FILENAME when needed.",
            inspection.diagnostics.join("; ")
        )));
    }
    let board = inspection
        .selected_board
        .as_ref()
        .ok_or_else(|| EngineError::new("project has no selected board"))?;
    Ok((
        input.join(&board.relative_path),
        Some(
            json!({"code":"source_selection","status":"passed","message":format!("Selected {}. {} schematic source(s) inventoried only.",board.relative_path,inspection.schematics.len())}),
        ),
    ))
}
