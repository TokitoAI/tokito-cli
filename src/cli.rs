use super::*;

fn dataset_command(args: &[String]) -> Result<(), CliError> {
    let value = match args.get(1).map(String::as_str) {
        Some("bind-external")
            if args.len() == 9
                && args[3] == "--assets-root"
                && args[5] == "--request"
                && args[7] == "--output" =>
        {
            let request = dataset::load_json(Path::new(&args[6]))?;
            let binding = dataset_external::bind_external(
                Path::new(&args[2]),
                Path::new(&args[4]),
                &request,
            )?;
            let bytes =
                serde_json::to_vec_pretty(&binding).map_err(|e| CliError::new(e.to_string()))?;
            write_new_file(Path::new(&args[8]), &bytes)?;
            binding
        }
        Some("inventory") if args.len() == 5 && args[3] == "--revision" => {
            dataset_git::inventory(Path::new(&args[2]), &args[4])?
        }
        Some("inventory")
            if args.len() == 7 && args[3] == "--revision" && args[5] == "--assets-root" =>
        {
            dataset_git::inventory_with_assets(Path::new(&args[2]), &args[4], Path::new(&args[6]))?
        }
        Some("bind")
            if args.len() == 9
                && args[3] == "--assets-root"
                && args[5] == "--request"
                && args[7] == "--output" =>
        {
            let request = dataset::load_json(Path::new(&args[6]))?;
            let binding =
                dataset_git::bind_source_lock(Path::new(&args[2]), Path::new(&args[4]), &request)?;
            let bytes =
                serde_json::to_vec_pretty(&binding).map_err(|e| CliError::new(e.to_string()))?;
            write_new_file(Path::new(&args[8]), &bytes)?;
            binding
        }
        Some("lock") if args.len() == 7 && args[3] == "--request" && args[5] == "--output" => {
            let request = dataset::load_json(Path::new(&args[4]))?;
            let lock = dataset::create_source_lock(Path::new(&args[2]), &request)?;
            let bytes =
                serde_json::to_vec_pretty(&lock).map_err(|e| CliError::new(e.to_string()))?;
            write_new_file(Path::new(&args[6]), &bytes)?;
            lock
        }
        Some("verify") if args.len() == 5 && args[3] == "--lock" => {
            let lock = dataset::load_json(Path::new(&args[4]))?;
            dataset::verify_source_lock(Path::new(&args[2]), &lock)?
        }
        _ => return Err(CliError::new("use tokito-cli help dataset")),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|e| CliError::new(e.to_string()))?
    );
    if value["status"] == "refused" {
        return Err(CliError::new(
            "dataset verification refused; inspect JSON diagnostics",
        ));
    }
    Ok(())
}

fn twin_command(args: &[String]) -> Result<(), CliError> {
    if args.get(1).map(String::as_str) == Some("audit-population") {
        if args.len() != 9
            || args[3] != "--component-map"
            || args[5] != "--manifest"
            || args[7] != "--reference-bom"
        {
            return Err(CliError::new(
                "usage: twin audit-population <GLB> --component-map <JSON> --manifest <JSON> --reference-bom <esp32-csv>",
            ));
        }
        let twin = digital_twin::ValidatedTwin::load(
            Path::new(&args[2]),
            Path::new(&args[4]),
            Path::new(&args[6]),
        )?;
        let reference = bom::load_bom(Path::new(&args[8]), bom::BomFormat::Esp32Csv)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&population_audit::compare_esp32_reference(
                &twin, &reference
            )?)
            .map_err(|e| CliError::new(e.to_string()))?
        );
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("adapt-source") {
        if !matches!(args.len(), 17 | 19)
            || (args.len() == 19 && args[17] != "--board-plane-node")
            || args[3] != "--component-map"
            || args[5] != "--geometry-index"
            || args[7] != "--provenance"
            || args[9] != "--coverage"
            || args[11] != "--board-id"
            || args[13] != "--board-revision"
            || args[15] != "--output"
        {
            return Err(CliError::new("use tokito-cli help twin adapt-source"));
        }
        let candidate = source_candidate::load_with_board_plane(
            Path::new(&args[2]),
            Path::new(&args[4]),
            Path::new(&args[6]),
            Path::new(&args[8]),
            Path::new(&args[10]),
            (&args[12], &args[14]),
            args.get(18).map(String::as_str),
        )?;
        let output = Path::new(&args[16]);
        let files = vec![
            ("active.digital-twin.glb", candidate.glb.clone()),
            (
                "component-map.json",
                serde_json::to_vec_pretty(&candidate.map)
                    .map_err(|e| CliError::new(e.to_string()))?,
            ),
            (
                "adaptation-report.json",
                serde_json::to_vec_pretty(&candidate.report)
                    .map_err(|e| CliError::new(e.to_string()))?,
            ),
            (
                "active.digital-twin.json",
                serde_json::to_vec_pretty(&candidate.manifest)
                    .map_err(|e| CliError::new(e.to_string()))?,
            ),
        ];
        flat_output::publish(output, &files)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&candidate.report)
                .map_err(|e| CliError::new(e.to_string()))?
        );
        return Ok(());
    }
    if args.len() < 7 || args[3] != "--component-map" || args[5] != "--manifest" {
        return Err(CliError::new(
            "usage: twin inspect|prepare|prototype|select <GLB> --component-map <JSON> --manifest <JSON> [--output <NEW_DIR> [--allow-partial] | --reference <REF> | --class <AUTHORED_LABEL>]",
        ));
    }
    let twin = digital_twin::ValidatedTwin::load(
        Path::new(&args[2]),
        Path::new(&args[4]),
        Path::new(&args[6]),
    )?;
    let json_bytes = |value: &serde_json::Value| {
        serde_json::to_vec_pretty(value).map_err(|e| CliError::new(e.to_string()))
    };
    match args[1].as_str() {
        "inspect" if args.len() == 7 => {
            println!(
                "{}",
                String::from_utf8(json_bytes(&twin.index_json())?).unwrap()
            );
        }
        "select" if args.len() == 9 && (args[7] == "--reference" || args[7] == "--class") => {
            let result = if args[7] == "--reference" {
                twin.select(Some(&args[8]), None)?
            } else {
                twin.select(None, Some(&args[8]))?
            };
            println!("{}", String::from_utf8(json_bytes(&result)?).unwrap());
        }
        "prepare" | "prototype" if args.len() >= 9 && args[7] == "--output" => {
            let mut allow_partial = false;
            let mut reference_bom = None;
            let mut i = 9;
            while i < args.len() {
                match args[i].as_str() {
                    "--allow-partial" if !allow_partial => {
                        allow_partial = true;
                        i += 1;
                    }
                    "--reference-bom"
                        if args[1] == "prototype"
                            && reference_bom.is_none()
                            && i + 1 < args.len() =>
                    {
                        reference_bom = Some(bom::load_bom(
                            Path::new(&args[i + 1]),
                            bom::BomFormat::Esp32Csv,
                        )?);
                        i += 2;
                    }
                    _ => {
                        return Err(CliError::new(
                            "unknown, duplicate or incomplete twin output option",
                        ));
                    }
                }
            }
            let mut prepared = twin.prepare(allow_partial)?;
            let population = reference_bom
                .as_ref()
                .map(|bom| population_audit::compare_esp32_index(&prepared.index, bom))
                .transpose()?;
            if let Some(ref population) = population {
                prepared.index["referencePopulationComparison"] = population.clone();
            }
            let viewer = if args[1] == "prototype" {
                Some(twin_viewer::generate(&prepared.glb, &prepared.index).map_err(CliError::new)?)
            } else {
                None
            };
            let output = Path::new(&args[8]);
            let mut files = vec![
                ("active.digital-twin.glb", prepared.glb.clone()),
                ("component-map.json", json_bytes(&prepared.component_map)?),
                ("component-index.json", json_bytes(&prepared.index)?),
                ("prepare-report.json", json_bytes(&prepared.report)?),
            ];
            if let Some(viewer) = viewer {
                files.push(("index.html", viewer.into_bytes()));
                files.push((
                    "presentation-profile.json",
                    json_bytes(&twin_viewer::presentation_profile())?,
                ));
            }
            if let Some(population) = population {
                files.push(("reference-population-audit.json", json_bytes(&population)?));
            }
            files.push(("active.digital-twin.json", json_bytes(&prepared.manifest)?));
            flat_output::publish(output, &files)?;
            println!(
                "{}",
                String::from_utf8(json_bytes(&prepared.report)?).unwrap()
            );
        }
        _ => {
            return Err(CliError::new(
                "unsupported twin command or arguments; use tokito-cli help twin",
            ));
        }
    }
    Ok(())
}

fn reconcile_bom(args: &[String]) -> Result<(), CliError> {
    if !(args.len() == 7 || args.len() == 8 && args[7] == "--json")
        || args[3] != "--bom"
        || args[5] != "--format"
    {
        return Err(CliError::new(
            "usage: tokito-cli bom reconcile <BOARD|PACKAGE> --bom <FILE> --format nxp-json|esp32-csv [--json]",
        ));
    }
    let format = match args[6].as_str() {
        "nxp-json" => bom::BomFormat::NxpJson,
        "esp32-csv" => bom::BomFormat::Esp32Csv,
        _ => return Err(CliError::new("BOM format must be nxp-json or esp32-csv")),
    };
    let board = load_board_input(Path::new(&args[2]))?;
    let document = bom::load_bom(Path::new(&args[4]), format)?;
    let report = bom::reconcile(&board, &document);
    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|_| CliError::new("Cannot serialize BOM reconciliation"))?
    );
    if report["status"] == "refused" {
        return Err(CliError::new(
            "BOM reconciliation refused; inspect the JSON report",
        ));
    }
    Ok(())
}

fn model_assignments_command(args: &[String]) -> Result<(), CliError> {
    use crate::model_assignments::{
        assignment_template, load_model_assignments, validate_model_assignments,
    };
    use crate::model_resolver::{ModelResolver, ModelResolverConfig};
    match args.get(2).map(String::as_str) {
        Some("template") if args.len() == 6 && args[4] == "--output" => {
            let board = load_board_input(Path::new(&args[3]))?;
            let template = assignment_template(&board)?;
            let bytes = serde_json::to_vec_pretty(&template)
                .map_err(|_| CliError::new("cannot encode assignment template"))?;
            write_new_file(Path::new(&args[5]), &bytes)?;
            println!(
                "Wrote {} unassigned component records to {}",
                board.engineering().components.len(),
                args[5]
            );
            Ok(())
        }
        Some("verify")
            if (args.len() == 8 || args.len() == 9 && args[8] == "--json")
                && args[4] == "--manifest"
                && args[6] == "--project-root" =>
        {
            let board = load_board_input(Path::new(&args[3]))?;
            let manifest = load_model_assignments(Path::new(&args[5]))?;
            let resolver = ModelResolver::new(ModelResolverConfig::new(PathBuf::from(&args[7])))?;
            let validated = validate_model_assignments(&board, &manifest, &resolver)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&validated.report())
                    .map_err(|_| CliError::new("cannot encode assignment report"))?
            );
            Ok(())
        }
        _ => Err(CliError::new(
            "usage: model assignments template <BOARD> --output <JSON> | verify <BOARD> --manifest <JSON> --project-root <DIR> [--json]",
        )),
    }
}

fn model_derive_wrl_command(args: &[String]) -> Result<(), CliError> {
    let flags = args.get(7..).unwrap_or(&[]);
    let palette = flags
        .iter()
        .any(|s| s == "--remove-unused-invalid-materials");
    let zero_area = flags.iter().any(|s| s == "--remove-zero-area-faces");
    if args.len() < 8
        || args.len() > 9
        || args[3] != "--output"
        || args[5] != "--units"
        || flags
            .iter()
            .any(|s| s != "--remove-unused-invalid-materials" && s != "--remove-zero-area-faces")
        || flags.len() != usize::from(palette) + usize::from(zero_area)
    {
        return Err(CliError::new(
            "usage: model derive-wrl <INPUT.wrl> --output <NEW.wrl> --units millimetres|kicad-wrl [--remove-unused-invalid-materials] [--remove-zero-area-faces] (at least one required)",
        ));
    }
    let units = match args[6].as_str() {
        "millimetres" => model_mesh::ModelUnits::Millimetres,
        "kicad-wrl" => model_mesh::ModelUnits::KiCadLegacyWrlDeciinch,
        _ => return Err(CliError::new("unsupported explicit model units")),
    };
    let input = read_bounded_file(
        Path::new(&args[2]),
        model_mesh::MeshLimits::default().max_bytes,
    )?;
    let (derived, report) = if zero_area {
        model_mesh::derive_without_zero_area_faces(&input, units, palette)?
    } else {
        model_mesh::derive_without_unused_invalid_materials(&input, units)?
    };
    write_new_file(Path::new(&args[4]), &derived)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| CliError::new(e.to_string()))?
    );
    Ok(())
}

fn component_model_envelope_command(args: &[String]) -> Result<(), CliError> {
    use std::io::Read;

    if args.len() < 8 || args[0] != "component" || args[1] != "model" || args[2] != "envelope" {
        return Err(CliError::new(
            "usage: tokito-cli component model envelope <SPEC.json> --board <BOARD|PACKAGE> --output <FILE.wrl> [--json]",
        ));
    }
    let spec_path = Path::new(&args[3]);
    let mut board_path = None;
    let mut output_path = None;
    let mut json_output = false;
    let mut index = 4;
    while index < args.len() {
        match args[index].as_str() {
            "--board" if index + 1 < args.len() && board_path.is_none() => {
                board_path = Some(Path::new(&args[index + 1]));
                index += 2;
            }
            "--output" if index + 1 < args.len() && output_path.is_none() => {
                output_path = Some(Path::new(&args[index + 1]));
                index += 2;
            }
            "--json" if !json_output => {
                json_output = true;
                index += 1;
            }
            _ => {
                return Err(CliError::new(
                    "usage: tokito-cli component model envelope <SPEC.json> --board <BOARD|PACKAGE> --output <FILE.wrl> [--json]",
                ));
            }
        }
    }
    let board_path = board_path.ok_or_else(|| CliError::new("missing --board"))?;
    let output_path = output_path.ok_or_else(|| CliError::new("missing --output"))?;
    let file = std::fs::File::open(spec_path)
        .map_err(|error| CliError::new(format!("cannot read component specification: {error}")))?;
    let mut bytes = Vec::new();
    file.take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| CliError::new(format!("cannot read component specification: {error}")))?;
    let spec = component_forge::parse_component_envelope_spec(&bytes)?;
    let board = load_board_input(board_path)?;
    component_forge::validate_component_envelope(&spec, &board)?;
    let wrl = component_forge::generate_envelope_wrl(&spec);
    write_new_file(output_path, wrl.as_bytes())?;
    let report = component_forge::envelope_report_json(&spec);
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|_| CliError::new("cannot encode component envelope report"))?
        );
    } else {
        println!(
            "Wrote provisional body envelope for {} to {}",
            spec.component_reference(),
            output_path.display()
        );
        println!(
            "Geometry remains an envelope candidate; footprint, pose and physical fit are not certified."
        );
    }
    Ok(())
}

fn component_footprint_command(args: &[String]) -> Result<(), CliError> {
    if args.get(2).map(String::as_str) == Some("inspect") {
        return component_footprint_inspect_command(args);
    }
    if args.get(2).map(String::as_str) == Some("generate") {
        return component_footprint_generate_command(args);
    }
    if args.get(2).map(String::as_str) == Some("library") {
        return component_footprint_library_command(args);
    }
    if !matches!(args.len(), 5 | 6)
        || args[1] != "footprint"
        || args[2] != "r0603"
        || args[3] != "--output"
        || (args.len() == 6 && args[5] != "--json")
    {
        return Err(CliError::new(
            "usage: tokito-cli component footprint r0603 --output <NEW.kicad_mod> [--json]",
        ));
    }
    let spec = footprint_family::TwoTerminalChipSpec::r0603_nominal();
    let data = spec
        .to_kicad_mod()
        .map_err(|error| CliError::new(error.to_string()))?;
    write_new_file(Path::new(&args[4]), data.as_bytes())?;
    if args.len() == 6 {
        let report = serde_json::json!({
            "schema": "tokito.footprint-family-report.v1",
            "status": "library-geometry-candidate",
            "family": "two-terminal-chip",
            "footprint": spec.name,
            "padCount": 2,
            "units": "mm",
            "sourceReference": "KiCad 10.0.5 R_0603_1608Metric.kicad_mod, SHA-256 03fc7902b2661df01b4d828fdb6eab9eddf974e7130ed2b98892607387a50c4b",
            "manufacturerLandPatternQualified": false,
            "componentBodyGenerated": false,
            "componentModelQualified": false,
            "boardPlacementVerified": false
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| CliError::new(error.to_string()))?
        );
    } else {
        println!(
            "Wrote {} library-geometry footprint candidate to {}",
            spec.name, args[4]
        );
    }
    Ok(())
}

fn component_footprint_inspect_command(args: &[String]) -> Result<(), CliError> {
    let usage = "usage: tokito-cli component footprint inspect <FILE.kicad_mod> [--json]";
    if !(args.len() == 4 || args.len() == 5)
        || args[3].is_empty()
        || (args.len() == 5 && args[4] != "--json")
    {
        return Err(CliError::new(usage));
    }
    let report =
        crate::footprint_inspect::inspect_path(Path::new(&args[3])).map_err(CliError::new)?;
    if args.len() == 5 {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| CliError::new(error.to_string()))?
        );
    } else {
        let partial_pad_count = report
            .pads
            .iter()
            .filter(|pad| pad.status != "reported_fields_complete")
            .count();
        let partial_model_count = report
            .model_references
            .iter()
            .filter(|model| model.status != "reported_explicit_fields")
            .count();
        println!(
            "Inspected {} direct pad record(s) and {} source-declared 3D model reference(s) in {} ({}); {} pad record(s) and {} model reference(s) need attention. Model paths are not resolved; geometry and fabrication validity are not checked.",
            report.pad_count,
            report.model_reference_count,
            report.footprint_name,
            report.status,
            partial_pad_count,
            partial_model_count
        );
    }
    Ok(())
}

fn component_footprint_generate_command(args: &[String]) -> Result<(), CliError> {
    let usage = "usage: tokito-cli component footprint generate <SPEC.json> --output <NEW.kicad_mod> [--json]";
    if !(args.len() == 6 || args.len() == 7)
        || args[3].is_empty()
        || args[4] != "--output"
        || (args.len() == 7 && args[6] != "--json")
    {
        return Err(CliError::new(usage));
    }
    let generated = read_footprint_generation(Path::new(&args[3]))?;
    write_new_file(Path::new(&args[5]), generated.text.as_bytes())?;
    if args.len() == 7 {
        println!(
            "{}",
            serde_json::to_string_pretty(&generated.report)
                .map_err(|error| CliError::new(error.to_string()))?
        );
    } else {
        let pad_kind = match generated.report.pad_type {
            "smd" => "SMD",
            "thru_hole" => "plated through-hole",
            "np_thru_hole" => "round non-plated hole",
            "mixed" => "mixed pad-technology",
            _ => "supported",
        };
        println!(
            "Wrote explicit {pad_kind} footprint candidate {} ({} pads, {} graphics) to {}",
            generated.report.footprint_name,
            generated.report.pad_count,
            generated.report.graphic_count,
            args[5]
        );
        println!("Source geometry and manufacturer qualification remain unverified.");
    }
    Ok(())
}

fn read_footprint_generation(
    spec_path: &Path,
) -> Result<footprint_spec::FootprintGeneration, CliError> {
    let file = std::fs::File::open(spec_path)
        .map_err(|error| CliError::new(format!("cannot read footprint specification: {error}")))?;
    let mut input = Vec::new();
    file.take(64 * 1024 + 1)
        .read_to_end(&mut input)
        .map_err(|error| CliError::new(format!("cannot read footprint specification: {error}")))?;
    footprint_spec::generate(&input).map_err(CliError::new)
}

fn component_footprint_library_command(args: &[String]) -> Result<(), CliError> {
    if args.get(3).map(String::as_str) == Some("audit") {
        if !(args.len() == 5 || args.len() == 6) || (args.len() == 6 && args[5] != "--json") {
            return Err(CliError::new(
                "usage: tokito-cli component footprint library audit <LIBRARY.pretty> [--json]",
            ));
        }
        let report = crate::footprint_library::audit(Path::new(&args[4])).map_err(CliError::new)?;
        if args.len() == 6 {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|error| CliError::new(error.to_string()))?
            );
        } else {
            println!(
                "Footprint library audit: {} observed finding(s) across {} parsed asset(s); semantics and geometry are unverified.",
                report.findings.len(),
                report.parsed_footprint_count
            );
        }
        return Ok(());
    }
    if args.get(3).map(String::as_str) == Some("inspect") {
        if !(args.len() == 5 || args.len() == 6) || (args.len() == 6 && args[5] != "--json") {
            return Err(CliError::new(
                "usage: tokito-cli component footprint library inspect <LIBRARY.pretty> [--json]",
            ));
        }
        let report =
            crate::footprint_library::inspect(Path::new(&args[4])).map_err(CliError::new)?;
        if args.len() == 6 {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|error| CliError::new(error.to_string()))?
            );
        } else {
            println!(
                "Inventoried {} footprint asset(s) in {} ({} bytes); content syntax and geometry are unverified.",
                report.footprint_file_count, report.library_directory, report.total_bytes
            );
        }
        return Ok(());
    }
    if args.get(3).map(String::as_str) == Some("lock") {
        let usage = "usage: tokito-cli component footprint library lock <LIBRARY.pretty> --output <NEW.lock.json> [--json]";
        if !(args.len() == 7 || args.len() == 8)
            || args[4].is_empty()
            || args[5] != "--output"
            || args[6].is_empty()
            || (args.len() == 8 && args[7] != "--json")
        {
            return Err(CliError::new(usage));
        }
        let inventory =
            crate::footprint_library::inspect(Path::new(&args[4])).map_err(CliError::new)?;
        let lock_output = Path::new(&args[6]);
        if !lock_output
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            return Err(CliError::new(
                "footprint library lock output must end in .json",
            ));
        }
        // Keep the guard on the original caller path instead of round-tripping
        // through the inventory's JSON/display representation.
        ensure_lock_output_outside_library(Path::new(&args[4]), lock_output)?;
        let lock = crate::footprint_library::create_lock_json(&inventory).map_err(CliError::new)?;
        write_new_file(lock_output, &lock)?;
        let report = serde_json::json!({
            "schema": "tokito.footprint-library-lock-create.v1",
            "status": "content-locked",
            "lockFile": args[6],
            "inventorySha256": inventory.inventory_sha256,
            "footprintFileCount": inventory.footprint_file_count,
            "footprintSemanticsQualified": false
        });
        if args.len() == 8 {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|error| CliError::new(error.to_string()))?
            );
        } else {
            println!(
                "Wrote content lock for {} footprint asset(s); footprint semantics remain unqualified.",
                inventory.footprint_file_count
            );
        }
        return Ok(());
    }
    if args.get(3).map(String::as_str) == Some("verify") {
        let usage = "usage: tokito-cli component footprint library verify <LIBRARY.pretty> --lock <LOCK.json> [--json]";
        if !(args.len() == 7 || args.len() == 8)
            || args[4].is_empty()
            || args[5] != "--lock"
            || args[6].is_empty()
            || (args.len() == 8 && args[7] != "--json")
        {
            return Err(CliError::new(usage));
        }
        let report =
            crate::footprint_library::verify_lock(Path::new(&args[4]), Path::new(&args[6]))
                .map_err(CliError::new)?;
        if args.len() == 8 {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|error| CliError::new(error.to_string()))?
            );
        } else if report.matches {
            println!(
                "Verified {} footprint asset(s) against the inventory lock.",
                report.actual_file_count
            );
        }
        if !report.matches {
            return Err(CliError::new(
                "footprint library does not match the content lock",
            ));
        }
        return Ok(());
    }
    let usage = "usage: tokito-cli component footprint library create <SPEC.json> --output <NEW.pretty> [--json]";
    if !(args.len() == 7 || args.len() == 8)
        || args[3] != "create"
        || args[4].is_empty()
        || args[5] != "--output"
        || (args.len() == 8 && args[7] != "--json")
    {
        return Err(CliError::new(usage));
    }
    let output_dir = Path::new(&args[6]);
    if !output_dir
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pretty"))
    {
        return Err(CliError::new(
            "library output directory must end in .pretty",
        ));
    }
    let generated = read_footprint_generation(Path::new(&args[4]))?;
    std::fs::create_dir(output_dir).map_err(|error| {
        CliError::new(format!(
            "cannot create new footprint library directory: {error}"
        ))
    })?;
    let footprint_path = output_dir.join(format!("{}.kicad_mod", generated.report.footprint_name));
    if let Err(error) = write_new_file(&footprint_path, generated.text.as_bytes()) {
        // This directory was created exclusively above. Remove only the empty
        // directory; never recurse into or alter any content another process
        // may have created concurrently.
        let _ = std::fs::remove_dir(output_dir);
        return Err(error);
    }
    let library_status = match generated.report.pad_type {
        "smd" => "explicit-smd-library-candidate",
        "thru_hole" => "explicit-through-hole-library-candidate",
        "np_thru_hole" => "explicit-non-plated-hole-library-candidate",
        "mixed" => "explicit-mixed-pad-library-candidate",
        _ => return Err(CliError::new("unsupported generated footprint pad type")),
    };
    let report = serde_json::json!({
        "schema": "tokito.footprint-library-create-report.v1",
        "status": library_status,
        "libraryDirectory": output_dir.to_string_lossy(),
        "footprintFile": footprint_path.to_string_lossy(),
        "registeredInFpLibTable": false,
        "footprintGeneration": generated.report
    });
    if args.len() == 8 {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| CliError::new(error.to_string()))?
        );
    } else {
        println!(
            "Created unregistered footprint library candidate at {}",
            output_dir.display()
        );
        println!("No existing library or fp-lib-table was changed.");
    }
    Ok(())
}

fn ensure_lock_output_outside_library(
    library_path: &Path,
    lock_output: &Path,
) -> Result<(), CliError> {
    let library_root = std::fs::canonicalize(library_path).map_err(|error| {
        CliError::new(format!(
            "cannot resolve footprint library directory: {error}"
        ))
    })?;
    let output_parent = lock_output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let output_parent = std::fs::canonicalize(output_parent).map_err(|error| {
        CliError::new(format!(
            "cannot resolve footprint lock output directory: {error}"
        ))
    })?;
    if output_parent.starts_with(&library_root) {
        return Err(CliError::new(
            "footprint library lock output must be outside the inspected .pretty directory",
        ));
    }
    Ok(())
}

fn print_help(style: cli_presentation::Style) {
    println!("{}", style.help(&r#"Tokito CLI {version}
Offline-first Rust tools for existing PCB layouts.

USAGE:
  tokito-cli [--color auto|always|never] <COMMAND> [ARGUMENTS]
  tokito-cli help <COMMAND...>

START HERE:
  doctor      Check this runtime, or inspect a project/PCB without changing it
  project     Discover a PCB; build and verify a source-bound export bundle
  pcb         Inspect, validate, resolve models, and export an existing PCB

COMMAND GROUPS:
  component   Inspect/generate footprint candidates; manage candidate libraries
  model       Inspect local VRML models; template and verify model assignments
  bom         Reconcile exact source references with a supported BOM
  twin        Inspect, prepare, select and preview authored 3D candidates
  dataset     Inventory, bind, lock and verify explicitly selected offline assets
  package     Build and verify source-and-inventory packages
  version     Print the CLI version

WORKFLOW:
  1. tokito-cli doctor ./board-project
  2. tokito-cli project inspect ./board-project
  3. tokito-cli pcb resolve models board.kicad_pcb --project-root . --library-root ./models
  4. tokito-cli project build ./board-project --output ./exports --assemble-wrl --library-root ./models
  5. tokito-cli project verify ./exports

HELP AND PRESENTATION:
  -h and --help show root or command-specific help.
  --version|-V prints the CLI version.
  --color auto|always|never and --no-color are leading global options.
  NO_COLOR disables colour; redirected output is plain by default.
  Machine JSON is never decorated.

EXAMPLES:
  tokito-cli help project build
  tokito-cli pcb export --help
  tokito-cli component footprint generate --help
  tokito-cli doctor ./board-project --json

SHORTCUTS:
  inspect, validate, resolve and export remain available without the pcb prefix.

SCOPE:
  Supported 2D views are partial. Local VRML assembly and source-linked component
  selection are implemented; STEP geometry assembly, schematic-to-PCB, full KiCad
  parity, App admission and registered AR are not complete. Missing models need
  explicit --allow-partial; a successful export is not physical qualification."#.replace("{version}", env!("CARGO_PKG_VERSION"))));
}

fn print_command_help(path: &[String], style: cli_presentation::Style) -> Result<(), CliError> {
    macro_rules! println {
        ($($arg:tt)*) => { std::println!("{}", style.help(&format!($($arg)*))) };
    }

    if path.len() == 1 && path[0] == "pcb" {
        print_pcb_help(style);
        return Ok(());
    }
    let path = if path.first().is_some_and(|part| part == "pcb") {
        &path[1..]
    } else {
        path
    };
    let path = help_topic_without_arguments(path);
    match path
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => print_help(style),
        ["doctor"] => println!(
            "USAGE: tokito-cli doctor [PROJECT|BOARD.kicad_pcb] [--board EXACT_FILENAME] [--json]\nWith no input, reports runtime information only. With a project or PCB, performs bounded source selection/import and supported outline checks without modifying files. --board disambiguates a project. Model references are inventory only, not availability or geometry qualification. Failed checks exit 2; JSON remains machine-readable.\nEXAMPLE: tokito-cli doctor ./board-project --json"
        ),
        ["version"] => println!("USAGE: tokito-cli version\nPrint the Tokito CLI version."),
        ["project"] => println!(
            "USAGE: tokito-cli project inspect|build|verify ...\nInspect and select an existing physical PCB, build a hashed offline export bundle, or verify one. Schematic files are inventoried only; physical geometry is never inferred from a schematic."
        ),
        ["project", "inspect"] => println!(
            "USAGE: tokito-cli project inspect <DIR> [--board <EXACT_FILENAME.kicad_pcb>] [--expect-sha256 <HEX>] [--json]\nLists only top-level KiCad PCB and schematic files. Selects the sole PCB or requires an exact --board filename when ambiguous. Validates and hashes the selected PCB through the Rust engine; --expect-sha256 refuses a different revision. Schematic files are inventoried only; no layout or geometry is inferred."
        ),
        ["project", "build"] => println!(
            "USAGE: tokito-cli project build <DIR> --output <NEW_DIRECTORY> [--board <EXACT_FILENAME.kicad_pcb>] [--expect-sha256 <HEX>] [--allow-partial] [--assemble-wrl --library-root <DIR> [--model-var NAME=DIR]] [--json]\nWrites a new source-bound bundle with top/bottom SVG, placement and identity inventories, outline diagnostics and GLB. Missing component models refuse output unless --allow-partial is explicit. The bundle records hashes and omissions; it is not accepted by the App digital-twin importer until a matching board-context contract is verified."
        ),
        ["project", "verify"] => println!(
            "USAGE: tokito-cli project verify <BUNDLE_DIRECTORY> [--json]\nRechecks bundle paths, payload digests and the imported source board identity. Integrity verification does not qualify model geometry, App compatibility, or AR registration."
        ),
        ["inspect"] => println!(
            "USAGE: tokito-cli inspect <BOARD.kicad_pcb|TOKITO_PACKAGE_DIRECTORY> [--json]\nReads supported board metadata, inventory and import diagnostics."
        ),
        ["bom"] | ["bom", "reconcile"] => println!(
            "USAGE: tokito-cli bom reconcile <BOARD|PACKAGE> --bom <FILE> --format nxp-json|esp32-csv [--json]\nJoins exact source references and reports identity/population conflicts; model geometry, placement and board revision remain unverified."
        ),
        ["validate"] | ["validate", "outline"] => println!(
            "USAGE: tokito-cli validate outline <BOARD|TOKITO_PACKAGE_DIRECTORY> [--json]\n       tokito-cli validate outline <BOARD|PACKAGE> --compare-chaining 0.001|0.01|0.1 --json\nValidates the supported physical Edge.Cuts topology. Comparison reports endpoint-distance candidates only; it does not repair or certify an outline."
        ),
        ["resolve"] | ["resolve", "models"] => println!(
            "USAGE: tokito-cli resolve models <BOARD|PACKAGE> --project-root <DIR> [--library-root <DIR>] [--model-var NAME=DIR] [--max-model-bytes N] [--max-batch-bytes N] [--extract-dir <NEW_DIR> [--allow-partial]] [--json]\nCaptures local model assets under caller-approved roots and verifies bounded KiCad embedded payloads. Optional --extract-dir copies resolved STEP/WRL/GLB source bytes and complete JSON glTF files with their approved local relative buffers/images into a new offline folder and writes manifest.json; incomplete extraction refuses unless --allow-partial is explicit. glTF data URIs remain inline; remote, unsafe, missing or unsupported resources make that model bundle incomplete. Extraction does not decode geometry, apply board placement, or qualify component selection/mobile import."
        ),
        ["model", "assignments"]
        | ["model", "assignments", "template"]
        | ["model", "assignments", "verify"] => println!(
            "USAGE: tokito-cli model assignments template <BOARD|PACKAGE> --output <FILE.json>\n       tokito-cli model assignments verify <BOARD|PACKAGE> --manifest <FILE.json> --project-root <DIR> [--json]\nAssigned bodies: export glb <BOARD> --model-assignments <FILE.json> --project-root <DIR> --output <FILE.glb> --allow-partial --json-report\nAssignment and rendered coverage do not certify part identity, pose or geometry."
        ),
        ["component"] => println!(
            "USAGE: tokito-cli component <COMMAND>\nCOMMANDS:\n  footprint inspect|r0603|generate    Inspect native pads or generate explicit candidates\n  footprint library                 Create, inspect, audit, lock or verify a candidate library\n  model envelope                    Emit an explicitly unqualified nominal rectangular body\nEXAMPLE: tokito-cli component footprint --help"
        ),
        ["component", "model"] | ["component", "model", "envelope"] => println!(
            "USAGE: tokito-cli component model envelope <SPEC.json> --board <BOARD|PACKAGE> --output <FILE.wrl> [--json]\nGenerates only a dimensioned rectangular body envelope from explicit source dimensions. This does not generate a footprint, terminals, or a qualified physical CAD model."
        ),
        ["component", "footprint", "inspect"] => println!(
            "USAGE: tokito-cli component footprint inspect <FILE.kicad_mod> [--json]\nReads a bounded native footprint S-expression and reports direct pad fields plus source-declared 3D model paths and explicit offset/scale/rotation vectors. Unsupported or malformed reported fields are diagnosed; other direct pad clauses are listed by name but their values are not decoded. Model paths are not resolved and omitted transforms are not defaulted. This is structural inspection only; it does not validate KiCad semantics, render pad geometry, qualify a manufacturer land pattern, or certify fabrication readiness."
        ),
        ["component", "footprint"]
        | ["component", "footprint", "r0603"]
        | ["component", "footprint", "generate"] => println!(
            "USAGE: tokito-cli component footprint inspect <FILE.kicad_mod> [--json]\n       tokito-cli component footprint r0603 --output <NEW.kicad_mod> [--json]\n       tokito-cli component footprint generate <SPEC.json> --output <NEW.kicad_mod> [--json]\nInspects direct pad source structure or generates bounded explicit SMD, plated through-hole (round or oval drill, optional offset), and centered round non-plated-hole geometry from a versioned JSON spec. Pad technologies can be mixed per pad. Graphics support lines, rectangles, circles and simple filled/unfilled polygons. IPC-7351 calculations, custom pads, 3D models, KLC checks and physical qualification are not implemented."
        ),
        ["component", "footprint", "library", "create"] => {
            println!(
                "USAGE: tokito-cli component footprint library create <SPEC.json> --output <NEW.pretty> [--json]\n       tokito-cli component footprint library inspect <LIBRARY.pretty> [--json]\n       tokito-cli component footprint inspect <FILE.kicad_mod> [--json]\nCreates an unregistered `.pretty` candidate or inventories existing library assets. Use the standalone footprint inspect command for per-pad structural fields. Library inventory does not validate geometry or semantics, and no operation edits an existing library or fp-lib-table. Generation supports explicitly specified SMD, plated through-hole and centered round non-plated-hole pads, including mixed pad technologies, plus bounded simple polygon graphics; IPC-7351 calculations, custom pads, 3D models, KLC checks and physical qualification are not implemented."
            )
        }
        ["component", "footprint", "library", "inspect"] => println!(
            "USAGE: tokito-cli component footprint library inspect <LIBRARY.pretty> [--json]\nInventories immediate `.kicad_mod` assets with bounded byte counts/hashes and minimal S-expression root, pad and model-reference counts. It does not validate footprint semantics or geometry, register a library or access the network."
        ),
        ["component", "footprint", "library", "audit"] => println!(
            "USAGE: tokito-cli component footprint library audit <LIBRARY.pretty> [--json]\nReports duplicate parsed internal names, filename/name mismatches and assets that could not be parsed. These are read-only observations, not KiCad load failures, KLC findings, footprint semantic validation or geometry qualification."
        ),
        ["component", "footprint", "library", "lock"]
        | ["component", "footprint", "library", "verify"] => println!(
            "USAGE: tokito-cli component footprint library lock <LIBRARY.pretty> --output <NEW.lock.json> [--json]\n       tokito-cli component footprint library verify <LIBRARY.pretty> --lock <LOCK.json> [--json]\nLocks and verifies file names, byte lengths and SHA-256 hashes only. It does not qualify footprint semantics, geometry or library registration."
        ),
        ["component", "footprint", "library"] => println!(
            "USAGE: tokito-cli component footprint library inspect <LIBRARY.pretty> [--json]\n       tokito-cli component footprint library audit <LIBRARY.pretty> [--json]\n       tokito-cli component footprint library create <SPEC.json> --output <NEW.pretty> [--json]\n       tokito-cli component footprint library lock <LIBRARY.pretty> --output <NEW.lock.json> [--json]\n       tokito-cli component footprint library verify <LIBRARY.pretty> --lock <LOCK.json> [--json]\nInventories assets, reports naming/collision observations, creates an unregistered candidate library, or writes/verifies a deterministic byte-identity lock. Audit findings do not qualify footprint semantics, geometry, physical qualification or library registration."
        ),
        ["dataset"]
        | ["dataset", "inventory"]
        | ["dataset", "bind"]
        | ["dataset", "bind-external"]
        | ["dataset", "lock"]
        | ["dataset", "verify"] => {
            println!(
                "USAGE: tokito-cli dataset inventory <GIT_ROOT> --revision <FULL_COMMIT> [--assets-root <ROOT>]\n       tokito-cli dataset bind <GIT_ROOT> --assets-root <ROOT> --request <JSON> --output <NEW_JSON>\n       tokito-cli dataset bind-external <GIT_ROOT> --assets-root <ROOT> --request <JSON> --output <NEW_JSON>\n       tokito-cli dataset lock <ROOT> --request <JSON> --output <NEW_JSON>\n       tokito-cli dataset verify <ROOT> --lock <JSON>\nInventory/bind verify local immutable source object and payload identities. bind-external checks a local payload against source-pinned provenance hash/size claims, without fetching URLs. lock/verify retain a caller-declared revision. None authenticate publishers, qualify CAD geometry or admit an App package."
            )
        }
        ["twin", "audit-population"] => println!(
            "USAGE: tokito-cli twin audit-population <GLB> --component-map <JSON> --manifest <JSON> --reference-bom <esp32-csv>\nCompares rendered authored classes with a reference BOM. Reports coverage gaps and reused-label conflicts. Does not assert that the reference design matches the photographed board."
        ),
        ["twin"] => println!(
            "USAGE: tokito-cli twin <COMMAND>\nCOMMANDS:\n  inspect           Inspect hash-bound GLB/map/manifest identities\n  prepare           Prepare per-component renderable selectors\n  prototype         Generate an offline interactive browser viewer\n  select            Query an exact reference or authored class\n  adapt-source      Adapt a complete source-candidate profile\n  audit-population  Compare authored classes with an ESP32 reference BOM\nEXAMPLE: tokito-cli twin prototype --help\nAll outputs remain reference candidates; no AR registration or App admission is granted."
        ),
        ["twin", "inspect"] | ["twin", "prepare"] | ["twin", "prototype"] | ["twin", "select"] => {
            println!(
                "USAGE: tokito-cli twin adapt-source <GLB> --component-map <JSON> --geometry-index <JSON> --provenance <JSON> --coverage <JSON> --board-id <ID> --board-revision <REV> --output <NEW_DIR>\n       tokito-cli twin prototype <GLB> --component-map <JSON> --manifest <JSON> --output <NEW_DIR> [--allow-partial] [--reference-bom <esp32-csv>]\n       tokito-cli twin inspect <GLB> --component-map <JSON> --manifest <JSON>\n       tokito-cli twin prepare <GLB> --component-map <JSON> --manifest <JSON> --output <NEW_DIR> [--allow-partial]\n       tokito-cli twin select <GLB> --component-map <JSON> --manifest <JSON> --reference <REF>|--class <AUTHORED_LABEL>\nprototype uses the same preparation gates and writes an offline index.html with the Ultimaker reference presentation, optional material-studio lighting, orbit, zoom, per-component highlighting and a Save 4K PNG action. presentation-profile.json records shared defaults. 4K export requires graphics-device support. Moves selector names onto isolated mesh-bearing nodes without changing geometry or transforms. Outputs an unregistered reference candidate; board-reference admission, physical fit, App import and colour rendering require separate acceptance."
            )
        }
        ["twin", "adapt-source"] => println!(
            "USAGE: tokito-cli twin adapt-source <GLB> --component-map <JSON> --geometry-index <JSON> --provenance <JSON> --coverage <JSON> --board-id <ID> --board-revision <REV> --output <NEW_DIR> [--board-plane-node <EXACT_SOURCE_NODE>]\nThe optional node must name one active, nonempty source subtree. Its decoded world bounds are retained separately from whole-scene bounds; author selection does not grant physical or pose qualification."
        ),
        ["model"] => println!(
            "USAGE: tokito-cli model <COMMAND>\nCOMMANDS:\n  inspect                Inspect supported local VRML mesh geometry\n  derive-wrl             Explicitly derive a new, changed VRML candidate\n  assignments template   Create an exact-identity assignment template\n  assignments verify     Verify explicit local candidate assignments\nEXAMPLE: tokito-cli model assignments --help"
        ),
        ["model", "derive-wrl"] => println!(
            "USAGE: tokito-cli model derive-wrl INPUT.wrl --output NEW.wrl --units millimetres|kicad-wrl [--remove-unused-invalid-materials] [--remove-zero-area-faces]\nAt least one explicit derivation flag is required. Writes a new candidate with a change report; never edits the source or qualifies physical geometry. Removing faces is not proof of a watertight solid."
        ),
        ["model", "inspect"] => println!(
            "USAGE: tokito-cli model inspect <FILE.wrl|vrml> --units millimetres|kicad-wrl [--json]\nInspects the bounded VRML97 Group/Transform/scene DEF/USE and triangle-mesh profile."
        ),
        ["export"] => println!(
            "USAGE: tokito-cli [pcb] export <COMMAND>\nCOMMANDS:\n  svg          Partial physical-scale drawing of supported source geometry\n  glb          Board shell or bounded local VRML component assembly\n  pos          KiCad-reference manufacturing-position CSV/ASCII subset\n  placements   Raw source-coordinate component inventory CSV\nEXAMPLE: tokito-cli pcb export glb --help"
        ),
        ["export", "placements"] => println!(
            "USAGE: tokito-cli export placements <BOARD.kicad_pcb> --output <FILE.csv>\nWrites source-frame component placement and identity rows."
        ),
        ["export", "pos"] => println!(
            "USAGE: tokito-cli pcb export pos <BOARD|PACKAGE> --format csv|ascii --output <FILE.csv|FILE.pos> [--side front|back|both] [--units in|mm] [--use-drill-file-origin] [--bottom-negate-x] [--smd-only] [--exclude-fp-th] [--exclude-dnp]\nWrites manufacturing-position CSV or ASCII with absolute board origin by default. --use-drill-file-origin subtracts setup/aux_axis_origin from X and uses originY - sourceY for POS Y; absent auxiliary origin falls back to (0, 0). Bottom X negation follows origin translation. CSV is compared with pinned KiCad 10.0.5 and 10.0.7 cases; ASCII is compared with pinned KiCad 10.0.5 cases. Gerber and variants remain unsupported. Output is never overwritten."
        ),
        ["export", "svg"] => println!(
            "USAGE: tokito-cli export svg <BOARD|PACKAGE> --output <FILE.svg> [--side top|bottom] [--layers LAYER,...] [--hide-DNP-footprints-on-fab-layers]\nWrites a partial SVG view from supported board geometry. The DNP option hides only DNP footprint text and graphics on F.Fab/B.Fab; it does not remove pads or affect other layers."
        ),
        ["export", "glb"] => println!(
            "USAGE: tokito-cli export glb <BOARD|PACKAGE> --output <FILE.glb> [--assemble-wrl --project-root <DIR> [--library-root <DIR>] [--model-var NAME=DIR] [--component-filter <PATTERN[,PATTERN...]>] [--no-dnp] [--no-unspecified] | --model-assignments <FILE.json> --project-root <DIR>] [--outline-chaining-tolerance 0.000001|0.001|0.01] [--allow-partial] [--json-report]\nExports a validated board shell. --assemble-wrl emits the bounded supported VRML97 subset (.wrl/.vrml). --component-filter applies case-sensitive comma-separated reference patterns; `*` matches any sequence and `?` one Unicode scalar in Tokito. Empty filter means unfiltered and a no-match filter emits only the board shell. --no-dnp filters only footprints carrying the native KiCad (attr dnp) marker, by unique UUID; a custom (property \"DNP\" \"yes\") does not trigger this filter. --no-unspecified excludes footprints with neither native smd nor through_hole type; pads, names and model availability never infer this type. All filters act before model resolution and report selected/excluded coverage; pathological matching work is refused. Selected unresolved models still require --allow-partial. Filters apply only to VRML-to-GLB assembly. --model-assignments emits explicitly assigned candidate bodies and always requires --allow-partial; component filters, --no-dnp and --no-unspecified are unsupported in that mode and in shell-only mode. STEP, general VRML, glTF/GLB model import and phone import are unsupported."
        ),
        ["package"] => println!(
            "USAGE: tokito-cli package <COMMAND>\nCOMMANDS:\n  build    Create a new v0.1 source-and-inventory package\n  verify   Verify paths, identities and payload integrity\nEXAMPLE: tokito-cli package build --help\nThis package is not admitted by the phone importer."
        ),
        ["package", "build"] => println!(
            "USAGE: tokito-cli package build <BOARD.kicad_pcb> --output <DIRECTORY>\nCreates the v0.1 source-and-inventory package; it is not phone-importable."
        ),
        ["package", "verify"] => println!(
            "USAGE: tokito-cli package verify <DIRECTORY> [--json]\nChecks package structure, digests and imported-source consistency."
        ),
        _ => {
            return Err(CliError::new(
                "unknown command help topic; run `tokito-cli --help`",
            ));
        }
    }
    Ok(())
}

fn help_topic_without_arguments(path: &[String]) -> &[String] {
    const LEAVES: &[&[&str]] = &[
        &["doctor"],
        &["version"],
        &["inspect"],
        &["project", "inspect"],
        &["project", "build"],
        &["project", "verify"],
        &["validate", "outline"],
        &["resolve", "models"],
        &["bom", "reconcile"],
        &["model", "inspect"],
        &["model", "derive-wrl"],
        &["model", "assignments", "template"],
        &["model", "assignments", "verify"],
        &["component", "model", "envelope"],
        &["component", "footprint", "inspect"],
        &["component", "footprint", "generate"],
        &["component", "footprint", "r0603"],
        &["component", "footprint", "library", "create"],
        &["component", "footprint", "library", "inspect"],
        &["component", "footprint", "library", "audit"],
        &["component", "footprint", "library", "lock"],
        &["component", "footprint", "library", "verify"],
        &["dataset", "inventory"],
        &["dataset", "bind"],
        &["dataset", "bind-external"],
        &["dataset", "lock"],
        &["dataset", "verify"],
        &["twin", "inspect"],
        &["twin", "prepare"],
        &["twin", "prototype"],
        &["twin", "select"],
        &["twin", "adapt-source"],
        &["twin", "audit-population"],
        &["export", "svg"],
        &["export", "glb"],
        &["export", "pos"],
        &["export", "placements"],
        &["package", "build"],
        &["package", "verify"],
    ];
    LEAVES
        .iter()
        .filter(|leaf| {
            path.len() >= leaf.len()
                && path
                    .iter()
                    .zip(leaf.iter())
                    .all(|(actual, expected)| actual == expected)
        })
        .max_by_key(|leaf| leaf.len())
        .map_or(path, |leaf| &path[..leaf.len()])
}

fn print_pcb_help(style: cli_presentation::Style) {
    macro_rules! println {
        ($($arg:tt)*) => { std::println!("{}", style.help(&format!($($arg)*))) };
    }

    println!(
        "Tokito CLI PCB\n\
         Existing physical-board operations. Input must be a routed .kicad_pcb\n\
         file or a validated Tokito source-and-inventory package.\n\
         \n\
         USAGE:\n\
         \ttokito-cli pcb inspect <BOARD|PACKAGE> [--json]\n\
         \ttokito-cli pcb validate outline <BOARD|PACKAGE> [--json]\n\
         \ttokito-cli pcb resolve models <BOARD|PACKAGE> --project-root <DIR> ...\n\
         \ttokito-cli pcb export placements <BOARD> --output <FILE.csv>\n\
        \ttokito-cli pcb export pos <BOARD|PACKAGE> --format csv|ascii --output <FILE.csv|FILE.pos> [--side front|back|both] [--units in|mm] ...\n\
         \ttokito-cli pcb bom reconcile <BOARD|PACKAGE> --bom <FILE> --format nxp-json|esp32-csv [--json]\n\
         \ttokito-cli pcb export svg <BOARD|PACKAGE> --output <FILE.svg> [--side top|bottom] [--hide-DNP-footprints-on-fab-layers]\n\
         \ttokito-cli pcb export glb <BOARD|PACKAGE> --output <FILE.glb> [--assemble-wrl ...] [--component-filter <PATTERN[,PATTERN...]>] [--allow-partial]\n\
         \n\
        Position CSV and ASCII support absolute or auxiliary drill-file origin, side, units, bottom-X and placement-class filters. Position Gerber, variants, general Gerber, schematic-to-PCB, STEP, symbol-library and jobset commands are not implemented."
    );
}

fn export_pos_command(args: &[String]) -> Result<(), CliError> {
    use pos_export::{PositionExportOptions, PositionSide, PositionUnits};

    const USAGE: &str = "usage: tokito-cli pcb export pos <BOARD|PACKAGE> --format csv|ascii --output <FILE.csv|FILE.pos> [--side front|back|both] [--units in|mm] [--use-drill-file-origin] [--bottom-negate-x] [--smd-only] [--exclude-fp-th] [--exclude-dnp]";
    if args.len() < 3 {
        return Err(CliError::new(USAGE));
    }

    let input = &args[2];
    let mut output = None::<PathBuf>;
    let mut format = None::<String>;
    let mut options = PositionExportOptions::default();
    let mut side_set = false;
    let mut units_set = false;
    let mut bottom_negate_set = false;
    let mut drill_origin_set = false;
    let mut smd_only_set = false;
    let mut exclude_fp_th_set = false;
    let mut exclude_dnp_set = false;

    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--format" if index + 1 < args.len() && format.is_none() => {
                format = Some(args[index + 1].clone());
                index += 2;
            }
            "--output" | "-o" if index + 1 < args.len() && output.is_none() => {
                output = Some(PathBuf::from(&args[index + 1]));
                index += 2;
            }
            "--side" if index + 1 < args.len() && !side_set => {
                options.side = match args[index + 1].as_str() {
                    "front" => PositionSide::Front,
                    "back" => PositionSide::Back,
                    "both" => PositionSide::Both,
                    _ => return Err(CliError::new("--side must be front, back, or both")),
                };
                side_set = true;
                index += 2;
            }
            "--units" if index + 1 < args.len() && !units_set => {
                options.units = match args[index + 1].as_str() {
                    "in" => PositionUnits::Inches,
                    "mm" => PositionUnits::Millimetres,
                    _ => return Err(CliError::new("--units must be in or mm")),
                };
                units_set = true;
                index += 2;
            }
            "--bottom-negate-x" if !bottom_negate_set => {
                options.bottom_negate_x = true;
                bottom_negate_set = true;
                index += 1;
            }
            "--use-drill-file-origin" if !drill_origin_set => {
                options.use_drill_file_origin = true;
                drill_origin_set = true;
                index += 1;
            }
            "--smd-only" if !smd_only_set => {
                options.smd_only = true;
                smd_only_set = true;
                index += 1;
            }
            "--exclude-fp-th" if !exclude_fp_th_set => {
                options.exclude_fp_th = true;
                exclude_fp_th_set = true;
                index += 1;
            }
            "--exclude-dnp" if !exclude_dnp_set => {
                options.exclude_dnp = true;
                exclude_dnp_set = true;
                index += 1;
            }
            "--gerber-board-edge" => {
                return Err(CliError::new(
                    "--gerber-board-edge applies only to Gerber output; this command supports CSV and ASCII only",
                ));
            }
            "--variant" if index + 1 < args.len() => {
                return Err(CliError::new(
                    "--variant is unsupported until board variant semantics are implemented",
                ));
            }
            "--format"
            | "--output"
            | "-o"
            | "--side"
            | "--units"
            | "--bottom-negate-x"
            | "--use-drill-file-origin"
            | "--smd-only"
            | "--exclude-fp-th"
            | "--exclude-dnp" => {
                return Err(CliError::new(format!(
                    "duplicate or incomplete POS option: {}",
                    args[index]
                )));
            }
            option if option.starts_with('-') => {
                return Err(CliError::new(format!("unsupported POS option: {option}")));
            }
            _ => return Err(CliError::new(USAGE)),
        }
    }

    let format = match format.as_deref() {
        Some("csv") => "csv",
        Some("ascii") => "ascii",
        Some(other) => {
            return Err(CliError::new(format!(
                "POS format {other:?} is unsupported; this implementation supports CSV and ASCII only"
            )));
        }
        None => {
            return Err(CliError::new(
                "--format csv or ascii is required for POS export",
            ));
        }
    };
    let expected_extension = if format == "csv" { "csv" } else { "pos" };
    let output = output.ok_or_else(|| {
        CliError::new(format!(
            "POS {format} export requires --output <FILE.{expected_extension}>"
        ))
    })?;
    if !output
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected_extension))
    {
        return Err(CliError::new(format!(
            "POS {format} output path must use a .{expected_extension} extension"
        )));
    }

    let board = load_board_input(Path::new(input))?;
    let (contents, row_count, units, side, origin, source_sha256) = match format {
        "csv" => {
            let exported = board.export_pos_csv(options)?;
            (
                exported.csv,
                exported.row_count,
                exported.units,
                exported.side,
                exported.origin,
                exported.source_sha256,
            )
        }
        "ascii" => {
            let exported = board.export_pos_ascii(options)?;
            (
                exported.ascii,
                exported.row_count,
                exported.units,
                exported.side,
                exported.origin,
                exported.source_sha256,
            )
        }
        _ => unreachable!("POS format is validated above"),
    };
    let origin_description = match origin {
        pos_export::PositionExportOrigin::Absolute => "absolute board origin".to_owned(),
        pos_export::PositionExportOrigin::DrillFile {
            x_mm,
            y_mm,
            declared: true,
        } => format!("drill-file origin ({x_mm:.6}, {y_mm:.6} mm)"),
        pos_export::PositionExportOrigin::DrillFile {
            declared: false, ..
        } => "drill-file origin default (0.000000, 0.000000 mm)".to_owned(),
    };
    write_new_file(&output, contents.as_bytes())?;
    println!(
        "Wrote {row_count} {format} position rows ({} units, {} side selection, {}; source sha256:{}) to {}",
        units.as_str(),
        match side {
            PositionSide::Front => "front",
            PositionSide::Back => "back",
            PositionSide::Both => "both",
        },
        origin_description,
        source_sha256,
        output.display()
    );
    Ok(())
}

pub(super) fn run(args: &[String]) -> Result<(), CliError> {
    let (style, args) = cli_presentation::parse(args)?;
    if args.is_empty() {
        print_help(style);
        return Ok(());
    }
    if args[0] == "--help" || args[0] == "-h" {
        if args.len() != 1 {
            return Err(CliError::new(
                "root help accepts no trailing arguments; use tokito-cli help COMMAND",
            ));
        }
        print_help(style);
        return Ok(());
    }
    if args[0] == "help" {
        return print_command_help(&args[1..], style);
    }
    if let Some(help_index) = args
        .iter()
        .position(|argument| argument == "--help" || argument == "-h")
    {
        return print_command_help(&args[..help_index], style);
    }
    if args[0] == "--version" || args[0] == "-V" {
        if args.len() != 1 {
            return Err(CliError::new("version accepts no trailing arguments"));
        }
        println!("tokito-cli {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args[0] == "version" {
        if args.len() != 1 {
            return Err(CliError::new("usage: tokito-cli version"));
        }
        println!("tokito-cli {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args[0] == "doctor" {
        return cli_doctor::run(args, style);
    }
    if args[0] == "project" && args.get(1).map(String::as_str) == Some("build") {
        if args.len() < 5 {
            return Err(CliError::new(
                "usage: tokito-cli project build <DIR> --output <NEW_DIRECTORY> [--board <EXACT_FILENAME.kicad_pcb>] [--expect-sha256 <HEX>] [--allow-partial] [--assemble-wrl --library-root <DIR> [--model-var NAME=DIR]] [--json]",
            ));
        }
        let project_root = PathBuf::from(&args[2]);
        let mut output = None;
        let mut board = None;
        let mut expected_sha256 = None;
        let mut allow_partial = false;
        let mut assemble_wrl = false;
        let mut library_roots = Vec::new();
        let mut variables = std::collections::BTreeMap::new();
        let mut json = false;
        let mut index = 3;
        while index < args.len() {
            match args[index].as_str() {
                "--output" if index + 1 < args.len() && output.is_none() => {
                    output = Some(PathBuf::from(&args[index + 1]));
                    index += 2;
                }
                "--board" if index + 1 < args.len() && board.is_none() => {
                    board = Some(args[index + 1].as_str());
                    index += 2;
                }
                "--expect-sha256" if index + 1 < args.len() && expected_sha256.is_none() => {
                    expected_sha256 = Some(args[index + 1].as_str());
                    index += 2;
                }
                "--allow-partial" if !allow_partial => {
                    allow_partial = true;
                    index += 1;
                }
                "--assemble-wrl" if !assemble_wrl => {
                    assemble_wrl = true;
                    index += 1;
                }
                "--library-root" if index + 1 < args.len() && library_roots.len() < 63 => {
                    library_roots.push(PathBuf::from(&args[index + 1]));
                    index += 2;
                }
                "--model-var" if index + 1 < args.len() && variables.len() < 64 => {
                    let (name, path) = args[index + 1]
                        .split_once('=')
                        .ok_or_else(|| CliError::new("--model-var must use NAME=DIR"))?;
                    if name.is_empty()
                        || path.is_empty()
                        || variables
                            .insert(name.to_owned(), PathBuf::from(path))
                            .is_some()
                    {
                        return Err(CliError::new("empty or duplicate model directory variable"));
                    }
                    index += 2;
                }
                "--json" if !json => {
                    json = true;
                    index += 1;
                }
                option => {
                    return Err(CliError::new(format!(
                        "unknown, duplicate, or incomplete project build option: {option}"
                    )));
                }
            }
        }
        let output = output.ok_or_else(|| CliError::new("project build requires --output"))?;
        if !assemble_wrl && (!library_roots.is_empty() || !variables.is_empty()) {
            return Err(CliError::new(
                "model roots require --assemble-wrl; no model files are read otherwise",
            ));
        }
        let resolver = if assemble_wrl {
            let mut config = model_resolver::ModelResolverConfig::new(&project_root);
            config.library_roots = library_roots;
            config.directory_variables = variables;
            Some(model_resolver::ModelResolver::new(config)?)
        } else {
            None
        };
        let report =
            project_bundle::build_project_bundle(project_bundle::ProjectBundleBuildOptions {
                project_root: &project_root,
                board_filename: board,
                expected_sha256,
                output: &output,
                allow_partial,
                model_resolver: resolver.as_ref(),
            })?;
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|error| CliError::new(error.to_string()))?
            );
        } else {
            println!(
                "Built source-bound project bundle at {} ({}; App import and AR registration remain unverified)",
                output.display(),
                report["source"]["sha256"]
                    .as_str()
                    .unwrap_or("source digest recorded")
            );
        }
        return Ok(());
    }
    if args[0] == "project" && args.get(1).map(String::as_str) == Some("verify") {
        if args.len() != 3 && !(args.len() == 4 && args[3] == "--json") {
            return Err(CliError::new(
                "usage: tokito-cli project verify <BUNDLE_DIRECTORY> [--json]",
            ));
        }
        let report = project_bundle::verify_project_bundle(Path::new(&args[2]))?;
        if args.get(3).map(String::as_str) == Some("--json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|error| CliError::new(error.to_string()))?
            );
        } else {
            println!(
                "Verified Tokito export bundle: {} artifacts, source sha256:{}",
                report["artifactCount"].as_u64().unwrap_or_default(),
                report["sourceSha256"].as_str().unwrap_or("unknown")
            );
        }
        return Ok(());
    }
    if args[0] == "project" {
        if args.get(1).map(String::as_str) != Some("inspect") || args.len() < 3 {
            return Err(CliError::new("use tokito-cli help project"));
        }
        let mut board = None;
        let mut expected_sha256 = None;
        let mut json = false;
        let mut index = 3;
        while index < args.len() {
            match args[index].as_str() {
                "--board" if board.is_none() && index + 1 < args.len() => {
                    board = Some(args[index + 1].as_str());
                    index += 2;
                }
                "--expect-sha256" if expected_sha256.is_none() && index + 1 < args.len() => {
                    expected_sha256 = Some(args[index + 1].as_str());
                    index += 2;
                }
                "--json" if !json => {
                    json = true;
                    index += 1;
                }
                _ => return Err(CliError::new("use tokito-cli help project")),
            }
        }
        let report = project_intake::inspect_project_with_sha256(
            Path::new(&args[2]),
            board,
            expected_sha256,
        )?;
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|error| CliError::new(error.to_string()))?
            );
        } else {
            println!("Project: {}", args[2]);
            println!("PCB candidates: {}", report.boards.len());
            for candidate in &report.boards {
                println!("  {} ({} bytes)", candidate.relative_path, candidate.bytes);
            }
            println!("Schematic candidates: {}", report.schematics.len());
            for candidate in &report.schematics {
                println!("  {} ({} bytes)", candidate.relative_path, candidate.bytes);
            }
            if let Some(selected) = &report.selected_board {
                println!(
                    "Selected: {} (sha256:{})",
                    selected.relative_path, selected.sha256
                );
            }
            for diagnostic in &report.diagnostics {
                println!("Diagnostic: {diagnostic}");
            }
        }
        if !report.accepted() {
            return Err(CliError::new(
                "project inspection refused; inspect diagnostics",
            ));
        }
        return Ok(());
    }
    if args[0] == "pcb" {
        if args.len() == 1
            || args
                .get(1)
                .is_some_and(|argument| argument == "--help" || argument == "-h")
        {
            print_pcb_help(style);
            return Ok(());
        }
        let mut delegated = match args[1].as_str() {
            "inspect" => vec!["inspect".to_owned()],
            "validate" => vec!["validate".to_owned()],
            "export" => vec!["export".to_owned()],
            "resolve" => vec!["resolve".to_owned()],
            "bom" => vec!["bom".to_owned()],
            _ => {
                print_pcb_help(style);
                return Err(CliError::new("unknown PCB command"));
            }
        };
        delegated.extend(args[2..].iter().cloned());
        return run(&delegated);
    }
    if args[0] == "resolve" && args.get(1).is_some_and(|value| value == "models") {
        return resolve_models(args);
    }
    if args[0] == "bom" && args.get(1).is_some_and(|value| value == "reconcile") {
        return reconcile_bom(args);
    }
    if args[0] == "model" && args.get(1).is_some_and(|value| value == "assignments") {
        return model_assignments_command(args);
    }
    if args[0] == "model" && args.get(1).is_some_and(|value| value == "derive-wrl") {
        return model_derive_wrl_command(args);
    }
    if args[0] == "model" && args.get(1).is_some_and(|value| value == "inspect") {
        return inspect_model(args);
    }
    if args[0] == "component" {
        if args.get(1).map(String::as_str) == Some("footprint") {
            return component_footprint_command(args);
        }
        return component_model_envelope_command(args);
    }
    if args[0] == "twin" {
        return twin_command(args);
    }
    if args[0] == "dataset" {
        return dataset_command(args);
    }
    if args[0] == "inspect" {
        if args.len() < 2 || args.len() > 3 || (args.len() == 3 && args[2] != "--json") {
            return Err(CliError::new(
                "usage: tokito-cli inspect <BOARD.kicad_pcb> [--json]",
            ));
        }
        let board = load_board_input(Path::new(&args[1]))?;
        if args.get(2).is_some_and(|flag| flag == "--json") {
            print!("{}", board.inspection_json());
        } else {
            println!("Board: {}", board.source.display());
            println!("KiCad format version: {}", board.version);
            println!("Units: millimetres");
            println!(
                "Thickness: {}",
                board
                    .thickness_mm
                    .map_or("unknown".to_owned(), |value| format!("{value} mm"))
            );
            println!("Footprints: {}", board.footprints.len());
            println!(
                "Pads: {}",
                board
                    .footprints
                    .iter()
                    .map(|footprint| footprint.pad_count)
                    .sum::<usize>()
            );
            println!(
                "Referenced 3D models: {}",
                board
                    .footprints
                    .iter()
                    .map(|footprint| footprint.model_count)
                    .sum::<usize>()
            );
            println!("Nets: {}", board.net_count);
            println!(
                "Tracks: {} segments, {} arcs; vias: {}; zones: {}",
                board.track_segment_count, board.track_arc_count, board.via_count, board.zone_count
            );
            println!("Edge.Cuts objects: {}", board.edge_cuts_count);
            println!(
                "Parsed SVG primitives: {} (SVG coverage remains partial)",
                board.geometry.item_count()
            );
            println!("Declared layers: {}", board.layers.join(", "));
            if board.diagnostics.is_empty() {
                println!("Import diagnostics: none for the fields this command reads");
            } else {
                println!("Import diagnostics:");
                for diagnostic in &board.diagnostics {
                    println!("  - {diagnostic}");
                }
            }
            for diagnostic in board.geometry.diagnostics() {
                println!("  - {diagnostic}");
            }
            println!(
                "Geometry exports: partial SVG; board-shell GLB; optional bounded local VRML assembly; STEP remains unsupported"
            );
        }
        return Ok(());
    }
    if args[0] == "validate" && args.get(1).is_some_and(|value| value == "outline") {
        if args.len() == 6 && args[3] == "--compare-chaining" && args[5] == "--json" {
            let tolerance = args[4]
                .parse::<f64>()
                .map_err(|_| CliError::new("invalid chaining tolerance"))?;
            let board = load_board_input(Path::new(&args[2]))?;
            let mut report = board.outline().compare_chaining(tolerance)?;
            report["sourceSha256"] = serde_json::json!(board.source_sha256);
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| CliError::new(e.to_string()))?
            );
            return Ok(());
        }
        if !(args.len() == 3 || args.len() == 4 && args[3] == "--json") {
            return Err(CliError::new(
                "usage: tokito-cli validate outline <BOARD|PACKAGE> [--json]",
            ));
        }
        let board = load_board_input(Path::new(&args[2]))?;
        if args.len() == 4 {
            print!("{}", board.outline_json());
        } else {
            println!(
                "Physical outline: {} ({} primitives, {} contours; mm, source Y-down)",
                if board.outline().is_valid() {
                    "valid within supported scope"
                } else {
                    "INVALID"
                },
                board.outline().primitives.len(),
                board.outline().contours.len()
            );
            for diagnostic in &board.outline().diagnostics {
                println!("  {:?}: {}", diagnostic.code, diagnostic.message);
            }
        }
        if !board.outline().is_valid() {
            return Err(CliError::new(
                "physical outline validation failed; source inventory remains importable",
            ));
        }
        return Ok(());
    }
    if args[0] == "export" && args.get(1).is_some_and(|value| value == "pos") {
        return export_pos_command(args);
    }
    if args[0] == "export" && args.get(1).is_some_and(|value| value == "svg") {
        if args.len() < 5 {
            return Err(CliError::new(
                "usage: tokito-cli export svg <BOARD.kicad_pcb> --output <FILE.svg> [--side top|bottom] [--layers LAYER,...] [--hide-DNP-footprints-on-fab-layers]",
            ));
        }
        let mut side = "top";
        let mut side_set = false;
        let mut output_path = None;
        let mut requested_layers = None;
        let mut hide_dnp_footprints_on_fab_layers = false;
        let mut index = 3;
        while index < args.len() {
            match args[index].as_str() {
                "--side" if !side_set && index + 1 < args.len() => {
                    side_set = true;
                    side = match args[index + 1].as_str() {
                        "top" => "top",
                        "bottom" => "bottom",
                        _ => return Err(CliError::new("--side must be 'top' or 'bottom'")),
                    };
                    index += 2;
                }
                "--output" if output_path.is_none() && index + 1 < args.len() => {
                    output_path = Some(PathBuf::from(&args[index + 1]));
                    index += 2;
                }
                "--layers" if requested_layers.is_none() && index + 1 < args.len() => {
                    let parsed = args[index + 1]
                        .split(',')
                        .map(str::trim)
                        .filter(|layer| !layer.is_empty())
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    if parsed.is_empty() {
                        return Err(CliError::new(
                            "--layers requires a comma-separated layer list",
                        ));
                    }
                    requested_layers = Some(parsed);
                    index += 2;
                }
                "--hide-DNP-footprints-on-fab-layers" if !hide_dnp_footprints_on_fab_layers => {
                    hide_dnp_footprints_on_fab_layers = true;
                    index += 1;
                }
                option => {
                    return Err(CliError::new(format!(
                        "unknown or incomplete SVG option: {option}"
                    )));
                }
            }
        }
        let output_path =
            output_path.ok_or_else(|| CliError::new("SVG export requires --output <FILE.svg>"))?;
        if !output_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
        {
            return Err(CliError::new("SVG output path must use a .svg extension"));
        }
        let board = load_board_input(Path::new(&args[2]))?;
        let svg = board.svg_with_options(
            SvgSide::try_from(side)?,
            requested_layers.as_deref(),
            SvgExportOptions {
                hide_dnp_footprints_on_fab_layers,
            },
        )?;
        write_new_file(&output_path, svg.contents.as_bytes())?;
        println!(
            "Wrote partial {side} SVG ({} geometry items, {:.3} × {:.3} mm) to {}",
            svg.rendered_items,
            svg.width_mm,
            svg.height_mm,
            output_path.display()
        );
        if !svg.warnings.is_empty() {
            eprintln!(
                "warning: SVG is partial; {} geometry diagnostics were recorded",
                svg.warnings.len()
            );
            for warning in svg.warnings.iter().take(10) {
                eprintln!("  - {warning}");
            }
            if svg.warnings.len() > 10 {
                eprintln!("  - additional diagnostics are available via inspect --json");
            }
        }
        return Ok(());
    }
    if args[0] == "export" && args.get(1).is_some_and(|value| value == "glb") {
        if args.len() < 5 {
            return Err(CliError::new(
                "usage: tokito-cli export glb <BOARD|TOKITO_PACKAGE> --output <FILE.glb> [--assemble-wrl --project-root <DIR> [--library-root <DIR>] [--model-var NAME=DIR] [--component-filter <PATTERN[,PATTERN...]>] [--no-dnp] [--no-unspecified] | --model-assignments <FILE.json> --project-root <DIR>] [--allow-partial] [--json-report]",
            ));
        }
        let mut output_path = None;
        let mut allow_partial = false;
        let mut json_report = false;
        let mut assemble_wrl = false;
        let mut assignments_path = None;
        let mut chaining_tolerance = None;
        let mut component_filter = None;
        let mut no_dnp = false;
        let mut no_unspecified = false;
        let mut project_root = None;
        let mut library_roots = Vec::new();
        let mut variables = std::collections::BTreeMap::new();
        let mut index = 3;
        while index < args.len() {
            match args[index].as_str() {
                "--outline-chaining-tolerance"
                    if index + 1 < args.len() && chaining_tolerance.is_none() =>
                {
                    chaining_tolerance = Some(
                        args[index + 1]
                            .parse::<f64>()
                            .map_err(|_| CliError::new("invalid outline chaining tolerance"))?,
                    );
                    index += 2;
                }
                "--output" if index + 1 < args.len() && output_path.is_none() => {
                    output_path = Some(PathBuf::from(&args[index + 1]));
                    index += 2;
                }
                "--allow-partial" if !allow_partial => {
                    allow_partial = true;
                    index += 1;
                }
                "--json-report" if !json_report => {
                    json_report = true;
                    index += 1;
                }
                "--assemble-wrl" if !assemble_wrl => {
                    assemble_wrl = true;
                    index += 1;
                }
                "--model-assignments" if index + 1 < args.len() && assignments_path.is_none() => {
                    assignments_path = Some(PathBuf::from(&args[index + 1]));
                    index += 2;
                }
                "--project-root" if index + 1 < args.len() && project_root.is_none() => {
                    project_root = Some(PathBuf::from(&args[index + 1]));
                    index += 2;
                }
                "--library-root" if index + 1 < args.len() && library_roots.len() < 63 => {
                    library_roots.push(PathBuf::from(&args[index + 1]));
                    index += 2;
                }
                "--model-var" if index + 1 < args.len() && variables.len() < 64 => {
                    let (name, path) = args[index + 1]
                        .split_once('=')
                        .ok_or_else(|| CliError::new("GLB --model-var must use NAME=DIR"))?;
                    if name.is_empty()
                        || path.is_empty()
                        || variables
                            .insert(name.to_owned(), PathBuf::from(path))
                            .is_some()
                    {
                        return Err(CliError::new(
                            "Empty or duplicate GLB model directory variable",
                        ));
                    }
                    index += 2;
                }
                "--component-filter" if index + 1 < args.len() && component_filter.is_none() => {
                    let raw_patterns = &args[index + 1];
                    let patterns: Vec<String> = if raw_patterns.is_empty() {
                        Vec::new()
                    } else {
                        raw_patterns
                            .split(',')
                            .map(str::trim)
                            .filter(|pattern| !pattern.is_empty())
                            .map(str::to_owned)
                            .collect()
                    };
                    if patterns.len() > 1024 {
                        return Err(CliError::new("GLB component filter exceeds 1024 patterns"));
                    }
                    component_filter = Some(patterns);
                    index += 2;
                }
                "--no-dnp" if !no_dnp => {
                    no_dnp = true;
                    index += 1;
                }
                "--no-unspecified" if !no_unspecified => {
                    no_unspecified = true;
                    index += 1;
                }
                option => {
                    return Err(CliError::new(format!(
                        "unknown, duplicate, or incomplete GLB option: {option}"
                    )));
                }
            }
        }
        let output_path =
            output_path.ok_or_else(|| CliError::new("GLB export requires --output <FILE.glb>"))?;
        if !output_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("glb"))
        {
            return Err(CliError::new("GLB output path must use a .glb extension"));
        }
        let board = load_board_input(Path::new(&args[2]))?;
        if let Some(path) = assignments_path {
            if component_filter.is_some() || no_dnp || no_unspecified {
                return Err(CliError::new(
                    "GLB component filters, --no-dnp and --no-unspecified currently require --assemble-wrl",
                ));
            }
            let board = if let Some(tolerance) = chaining_tolerance {
                board.with_outline_chaining_tolerance(tolerance)?
            } else {
                board
            };
            if assemble_wrl {
                return Err(CliError::new(
                    "--model-assignments and --assemble-wrl are separate export modes",
                ));
            }
            use crate::model_resolver::{ModelResolver, ModelResolverConfig};
            let mut config = ModelResolverConfig::new(
                project_root.ok_or_else(|| CliError::new("assignments require --project-root"))?,
            );
            config.library_roots = library_roots;
            config.directory_variables = variables;
            let resolver = ModelResolver::new(config)?;
            let manifest = crate::model_assignments::load_model_assignments(&path)?;
            let validated =
                crate::model_assignments::validate_model_assignments(&board, &manifest, &resolver)?;
            let export = board
                .export_glb_with_assignments(&validated, GlbExportOptions { allow_partial })?;
            write_new_file(&output_path, &export.bytes)?;
            if json_report {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&export.report)
                        .map_err(|_| CliError::new("cannot serialize assigned export receipt"))?
                );
            } else {
                println!(
                    "Wrote explicit partial assigned-body GLB to {}; physical geometry and placement remain unverified",
                    output_path.display()
                );
            }
            return Ok(());
        }
        let board = if let Some(tolerance) = chaining_tolerance {
            board.with_outline_chaining_tolerance(tolerance)?
        } else {
            board
        };
        let mut model_report_json = None;
        let mut dnp_filter_coverage = None;
        let export = if assemble_wrl {
            use crate::model_resolver::{ModelResolver, ModelResolverConfig};
            let mut filter = if let Some(references) = component_filter.as_deref() {
                crate::model_assembly::ModelAssemblyFilter::component_patterns(references)?
            } else {
                crate::model_assembly::ModelAssemblyFilter::default()
            };
            if no_dnp {
                filter = filter.excluding_dnp();
            }
            if no_unspecified {
                filter = filter.excluding_unspecified();
            }
            let mut config = ModelResolverConfig::new(project_root.ok_or_else(|| {
                CliError::new("GLB --assemble-wrl requires --project-root <DIR>")
            })?);
            config.library_roots = library_roots;
            config.directory_variables = variables;
            let resolver = ModelResolver::new(config)?;
            let model_export = board.export_glb_with_models_filtered(
                &resolver,
                GlbExportOptions { allow_partial },
                &filter,
            )?;
            if json_report {
                model_report_json = Some(model_export.report_json());
            }
            if no_dnp {
                dnp_filter_coverage = Some(model_export.dnp_filter);
            }
            GlbExport {
                bytes: model_export.bytes,
                report: model_export.report,
            }
        } else {
            if component_filter.is_some() || no_dnp || no_unspecified {
                return Err(CliError::new(
                    "GLB component filters, --no-dnp and --no-unspecified currently require --assemble-wrl",
                ));
            }
            if project_root.is_some() || !library_roots.is_empty() || !variables.is_empty() {
                return Err(CliError::new(
                    "GLB model roots require --assemble-wrl; board-shell export never reads model files",
                ));
            }
            board.export_glb(GlbExportOptions { allow_partial })?
        };
        write_new_file(&output_path, &export.bytes)?;
        if json_report {
            print!(
                "{}",
                model_report_json.unwrap_or_else(|| export.report.report_json())
            );
        } else {
            if let Some(filter) = &export.report.unspecified_filter {
                let dnp_count = dnp_filter_coverage.map_or(0, |dnp| dnp.components_excluded);
                println!(
                    "Wrote {} GLB ({} of {} component models emitted; {} unresolved; excluded {} unspecified component(s), {} DNP component(s), {} overlap) to {}",
                    export.report.scope,
                    export.report.component_models_emitted,
                    export.report.component_count,
                    export.report.component_models_unresolved,
                    filter.components_excluded,
                    dnp_count,
                    filter.overlap_with_dnp_components,
                    output_path.display()
                );
            } else if let Some(filter) = dnp_filter_coverage {
                println!(
                    "Wrote {} GLB ({} of {} component models emitted; {} unresolved; excluded {} DNP component(s) and {} model reference(s)) to {}",
                    export.report.scope,
                    export.report.component_models_emitted,
                    export.report.component_count,
                    export.report.component_models_unresolved,
                    filter.components_excluded,
                    filter.model_references_excluded,
                    output_path.display()
                );
            } else {
                println!(
                    "Wrote {} GLB ({} of {} component models emitted; {} unresolved) to {}",
                    export.report.scope,
                    export.report.component_models_emitted,
                    export.report.component_count,
                    export.report.component_models_unresolved,
                    output_path.display()
                );
            }
        }
        return Ok(());
    }
    if args[0] == "export" && args.get(1).is_some_and(|value| value == "placements") {
        if args.len() != 5 || args[3] != "--output" {
            return Err(CliError::new(
                "usage: tokito-cli export placements <BOARD.kicad_pcb> --output <FILE.csv>",
            ));
        }
        let board = load_board_input(Path::new(&args[2]))?;
        let csv = board.placements_csv()?;
        let output_path = Path::new(&args[4]);
        write_new_file(output_path, csv.as_bytes())?;
        println!(
            "Wrote {} placements to {}",
            board.footprints.len(),
            output_path.display()
        );
        return Ok(());
    }
    if args[0] == "package" && args.get(1).is_some_and(|value| value == "verify") {
        if args.len() < 3 || args.len() > 4 || (args.len() == 4 && args[3] != "--json") {
            return Err(CliError::new(
                "usage: tokito-cli package verify <DIRECTORY> [--json]",
            ));
        }
        let board = verify_package(Path::new(&args[2]))?;
        if args.get(3).is_some_and(|flag| flag == "--json") {
            println!(
                "{{\"schema\":\"tokito.package-verification.v0\",\"verified\":true,\"schemaVersion\":\"0.1.0\",\"sourceSha256\":\"{}\",\"mobileImportReady\":false,\"publisherAuthenticated\":false}}",
                board.source_sha256
            );
        } else {
            println!(
                "Verified Tokito v0.1 source-and-inventory package (byte integrity; publisher not authenticated)"
            );
        }
        return Ok(());
    }
    if args[0] == "package" && args.get(1).is_some_and(|value| value == "build") {
        if args.len() != 5 || args[3] != "--output" {
            return Err(CliError::new(
                "usage: tokito-cli package build <BOARD.kicad_pcb> --output <DIRECTORY>",
            ));
        }
        let board = load_kicad_board(Path::new(&args[2]))?;
        let output = Path::new(&args[4]);
        board.build_package(output)?;
        println!(
            "Wrote Tokito source-and-inventory package to {}",
            output.display()
        );
        return Ok(());
    }
    print_help(style);
    Err(CliError::new("unknown command"))
}

/// Asset inventory only: successful lookup never certifies model geometry.
fn resolve_models(args: &[String]) -> Result<(), CliError> {
    use crate::model_asset_export::{ModelAssetPayload, export_model_assets};
    use crate::model_resolver::{ModelResolutionStatus, ModelResolver, ModelResolverConfig};
    const MAX_REFERENCES: usize = 1024;
    const MAX_CAPTURED_BYTES: usize = 256 * 1024 * 1024;
    const MAX_GLTF_RESOURCES: usize = 4096;
    let usage = "usage: tokito-cli resolve models <BOARD|PACKAGE> --project-root <DIR> [--library-root <DIR>] [--model-var NAME=DIR] [--max-model-bytes N] [--max-batch-bytes N] [--extract-dir <NEW_DIR> [--allow-partial]] [--json]";
    if args.len() < 5 || args.len() > 267 {
        return Err(CliError::new(usage));
    }
    let mut project_root = None;
    let mut roots = Vec::new();
    let mut variables = std::collections::BTreeMap::new();
    let mut json = false;
    let mut file_budget = None;
    let mut batch_budget = None;
    let mut extract_dir = None;
    let mut allow_partial = false;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--project-root" if index + 1 < args.len() && project_root.is_none() => {
                project_root = Some(PathBuf::from(&args[index + 1]));
                index += 2;
            }
            "--library-root" if index + 1 < args.len() && roots.len() < 63 => {
                roots.push(PathBuf::from(&args[index + 1]));
                index += 2;
            }
            "--model-var" if index + 1 < args.len() && variables.len() < 64 => {
                let (name, path) = args[index + 1]
                    .split_once('=')
                    .ok_or_else(|| CliError::new(usage))?;
                if path.is_empty()
                    || variables
                        .insert(name.to_owned(), PathBuf::from(path))
                        .is_some()
                {
                    return Err(CliError::new("Empty or duplicate model directory variable"));
                }
                index += 2;
            }
            "--json" if !json => {
                json = true;
                index += 1;
            }
            "--max-model-bytes" if index + 1 < args.len() && file_budget.is_none() => {
                file_budget = Some(model_byte_budget(&args[index + 1], 64 * 1024 * 1024)?);
                index += 2;
            }
            "--max-batch-bytes" if index + 1 < args.len() && batch_budget.is_none() => {
                batch_budget = Some(model_byte_budget(&args[index + 1], MAX_CAPTURED_BYTES)?);
                index += 2;
            }
            "--extract-dir" if index + 1 < args.len() && extract_dir.is_none() => {
                extract_dir = Some(PathBuf::from(&args[index + 1]));
                index += 2;
            }
            "--allow-partial" if !allow_partial => {
                allow_partial = true;
                index += 1;
            }
            _ => return Err(CliError::new(usage)),
        }
    }
    let mut config = ModelResolverConfig::new(project_root.ok_or_else(|| CliError::new(usage))?);
    config.library_roots = roots;
    config.directory_variables = variables;
    let file_budget = file_budget.unwrap_or(64 * 1024 * 1024);
    let batch_budget = batch_budget.unwrap_or(MAX_CAPTURED_BYTES);
    config.max_file_bytes = file_budget as u64;
    let resolver = ModelResolver::new(config)?;
    let board = load_board_input(Path::new(&args[2]))?;
    let models = &board.engineering().models;
    if models.len() > MAX_REFERENCES {
        return Err(CliError::new("Model reference batch exceeds 1024 entries"));
    }
    let footprint_components: Vec<_> = board
        .engineering()
        .components
        .iter()
        .filter(|component| component.source.kind == "footprint")
        .collect();
    let mut components_by_ordinal = std::collections::BTreeMap::new();
    let mut uuid_counts = std::collections::BTreeMap::<&str, usize>::new();
    let mut reference_counts = std::collections::BTreeMap::<&str, usize>::new();
    for component in &footprint_components {
        components_by_ordinal.insert(component.source.ordinal, *component);
        if let Some(uuid) = component
            .source
            .source_uuid
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            *uuid_counts.entry(uuid).or_default() += 1;
        }
        if let Some(reference) = component
            .reference
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            *reference_counts.entry(reference).or_default() += 1;
        }
    }
    let mut captured_bytes = 0usize;
    let mut resolved = 0usize;
    let mut model_references_with_unique_owner_identity = 0usize;
    let mut gltf_resource_reference_count = 0usize;
    let mut reports = Vec::with_capacity(models.len());
    let mut extraction_assets = Vec::new();
    for model in models {
        let remaining_batch_bytes = batch_budget.saturating_sub(captured_bytes);
        let resolution = resolver.resolve_board_model_with_remaining_batch_bytes(
            &board,
            model,
            remaining_batch_bytes,
        );
        let resolution_status = resolution.status;
        if resolution_status == ModelResolutionStatus::BatchBudgetExceeded
            && !(allow_partial && extract_dir.is_some())
        {
            return Err(CliError::new("Model batch exceeds captured-byte budget"));
        }
        let mut projection = resolution.shared_projection();
        if let Some(asset) = resolution.asset {
            captured_bytes = captured_bytes
                .checked_add(asset.bytes().len())
                .filter(|bytes| *bytes <= batch_budget)
                .ok_or_else(|| CliError::new("Model batch exceeds captured-byte budget"))?;
            if extract_dir.is_some() {
                let format = asset.format().as_str().to_owned();
                let mut dependencies = Vec::new();
                let gltf_bundle = if format == "gltf" {
                    let remaining_batch_bytes = batch_budget.saturating_sub(captured_bytes);
                    let bundle = crate::gltf_bundle::resolve_gltf_bundle(
                        &asset,
                        &resolver,
                        remaining_batch_bytes,
                    );
                    let resource_reference_count = bundle.report["resourceReferences"]
                        .as_u64()
                        .and_then(|count| usize::try_from(count).ok())
                        .ok_or_else(|| CliError::new("glTF resource count is invalid"))?;
                    gltf_resource_reference_count = gltf_resource_reference_count
                        .checked_add(resource_reference_count)
                        .filter(|count| *count <= MAX_GLTF_RESOURCES)
                        .ok_or_else(|| {
                            CliError::new("glTF batch exceeds its 4096 resource-reference limit")
                        })?;
                    for dependency in &bundle.dependencies {
                        captured_bytes = captured_bytes
                            .checked_add(dependency.bytes.len())
                            .filter(|bytes| *bytes <= batch_budget)
                            .ok_or_else(|| {
                                CliError::new("Model batch exceeds captured-byte budget")
                            })?;
                    }
                    dependencies = bundle.dependencies;
                    projection["gltfBundle"] = bundle.report.clone();
                    Some(bundle.complete)
                } else {
                    None
                };
                if matches!(format.as_str(), "step" | "wrl" | "glb") || gltf_bundle == Some(true) {
                    extraction_assets.push(ModelAssetPayload {
                        model_index: reports.len(),
                        format,
                        sha256: asset.sha256().to_owned(),
                        bytes: asset.into_bytes(),
                        dependencies,
                    });
                }
            }
        }
        resolved += usize::from(resolution_status == ModelResolutionStatus::Resolved);
        let owner = model
            .source
            .component_ordinal
            .and_then(|ordinal| components_by_ordinal.get(&ordinal).copied());
        let owner_projection = owner.map(|component| {
            let uuid = component.source.source_uuid.as_deref();
            let reference = component.reference.as_deref();
            let unique_uuid = uuid
                .filter(|value| !value.is_empty())
                .is_some_and(|value| uuid_counts.get(value) == Some(&1));
            let unique_reference = reference
                .filter(|value| !value.is_empty())
                .is_some_and(|value| reference_counts.get(value) == Some(&1));
            let mut identity_issues = Vec::new();
            if !unique_uuid {
                identity_issues.push(if uuid.is_none_or(str::is_empty) {
                    "missing_component_uuid"
                } else {
                    "ambiguous_component_uuid"
                });
            }
            if !unique_reference {
                identity_issues.push(if reference.is_none_or(str::is_empty) {
                    "missing_component_reference"
                } else {
                    "ambiguous_component_reference"
                });
            }
            let selectable_identity_ready = identity_issues.is_empty();
            model_references_with_unique_owner_identity += usize::from(selectable_identity_ready);
            serde_json::json!({
                "sourceComponentOrdinal": component.source.ordinal,
                "componentUuid": uuid,
                "reference": reference,
                "identityIssues": identity_issues,
                "selectableIdentityReady": selectable_identity_ready
            })
        });
        projection["ownerComponent"] = owner_projection.unwrap_or(serde_json::Value::Null);
        reports.push(projection);
    }
    if allow_partial && extract_dir.is_none() {
        return Err(CliError::new(
            "--allow-partial requires --extract-dir <NEW_DIR>",
        ));
    }
    let mut extraction_report = None;
    if let Some(output) = extract_dir {
        let unexported = models.len().saturating_sub(extraction_assets.len());
        if unexported > 0 && !allow_partial {
            return Err(CliError::new(
                "model extraction is incomplete; review unresolved models and use --allow-partial to export resolved source payloads",
            ));
        }
        let manifest =
            export_model_assets(output, board.source().sha256, reports, &extraction_assets)?;
        reports = manifest["models"]
            .as_array()
            .cloned()
            .ok_or_else(|| CliError::new("model asset export returned no model rows"))?;
        extraction_report = Some(manifest);
    }
    let mut report = serde_json::json!({
        "schemaVersion":"1.0.0", "scope":"offline-model-asset-resolution",
        "sourceSha256":board.source().sha256,
        "modelReferences":models.len(), "resolvedAssets":resolved,
        "unresolvedReferences":models.len()-resolved, "capturedBytes":captured_bytes,
        "modelReferencesWithUniqueOwnerIdentity":model_references_with_unique_owner_identity,
        "modelReferencesWithoutUniqueOwnerIdentity":models.len()-model_references_with_unique_owner_identity,
        "geometryValidated":false, "componentSelectionReady":false, "mobileImportReady":false,
        "models":reports,
            "limits":{"maxReferences":MAX_REFERENCES,"maxBatchCapturedBytes":batch_budget,
                  "maxGltfResourceReferences":MAX_GLTF_RESOURCES,
                  "gltfResourceReferences":gltf_resource_reference_count,
                  "maxFileBytes":file_budget},
    });
    if let Some(extraction) = extraction_report {
        report["assetExport"] = serde_json::json!({
            "schema":extraction["schema"],
            "schemaVersion":extraction["schemaVersion"],
            "outputDirectory":extraction["outputDirectory"],
            "manifest":extraction["manifest"],
            "manifestSha256":extraction["manifestSha256"],
            "sourceBoardSha256":extraction["sourceBoardSha256"],
            "resolvedModelPayloads":extraction["resolvedModelPayloads"],
            "unexportedModelReferences":extraction["unexportedModelReferences"],
            "uniqueAssetFiles":extraction["uniqueAssetFiles"],
            "uniqueAssetBytes":extraction["uniqueAssetBytes"],
            "payloadBytes":extraction["payloadBytes"],
            "gltfDependencyFiles":extraction["gltfDependencyFiles"],
            "gltfDependencyPayloadBytes":extraction["gltfDependencyPayloadBytes"],
            "partialExport":extraction["partialExport"],
            "geometryDecoded":false,
            "componentSelectionReady":false,
            "mobileImportReady":false
        });
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|_| CliError::new("Cannot serialize model report"))?
        );
    } else {
        println!(
            "Resolved {resolved}/{} model assets ({captured_bytes} bytes); geometry and component selection remain unverified",
            models.len()
        );
        for model in reports {
            println!(
                "{}",
                serde_json::to_string(&model)
                    .map_err(|_| CliError::new("Cannot serialize model diagnostic"))?
            );
        }
    }
    Ok(())
}

fn model_byte_budget(value: &str, maximum: usize) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0 && *n <= maximum)
        .ok_or_else(|| {
            CliError::new("Model byte budget must be a positive integer within the hard limit")
        })
}

fn inspect_model(args: &[String]) -> Result<(), CliError> {
    use crate::document::{ModelReference, SourceLocator};
    use crate::model_mesh::{MeshLimits, ModelUnits, parse_vrml_mesh};
    use crate::model_resolver::{ModelAssetFormat, ModelResolver, ModelResolverConfig};
    if !(args.len() == 5 || args.len() == 6 && args[5] == "--json") || args[3] != "--units" {
        return Err(CliError::new(
            "usage: tokito-cli model inspect <FILE.wrl|vrml> --units millimetres|kicad-wrl [--json]",
        ));
    }
    let units = match args[4].as_str() {
        "millimetres" => ModelUnits::Millimetres,
        "kicad-wrl" => ModelUnits::KiCadLegacyWrlDeciinch,
        _ => {
            return Err(CliError::new(
                "Model units must be explicit: millimetres or kicad-wrl",
            ));
        }
    };
    let input = PathBuf::from(&args[2]);
    let input = if input.is_absolute() {
        input
    } else {
        std::env::current_dir()
            .map_err(|_| CliError::new("Cannot resolve selected model directory"))?
            .join(input)
    };
    let root = input
        .parent()
        .ok_or_else(|| CliError::new("Model file needs a local parent directory"))?;
    let mut config = ModelResolverConfig::new(root);
    config.max_file_bytes = MeshLimits::default().max_bytes as u64;
    let resolver = ModelResolver::new(config)?;
    let model = ModelReference {
        source: SourceLocator {
            kind: "selected-model".into(),
            ordinal: 0,
            component_ordinal: None,
            source_uuid: None,
            source_path: vec![],
        },
        path: input.to_str().map(str::to_owned),
        offset: None,
        legacy_at: None,
        rotation_deg: None,
        scale: None,
        hidden: false,
    };
    let resolved = resolver.resolve(&model);
    let asset = resolved.asset.ok_or_else(|| {
        CliError::new(
            resolved
                .diagnostic
                .unwrap_or("Model asset resolution failed"),
        )
    })?;
    if asset.format() != ModelAssetFormat::Wrl {
        return Err(CliError::new(
            "Only the declared bounded VRML97 Group/Transform/scene DEF/USE and triangle-mesh profile is supported",
        ));
    }
    let mesh = parse_vrml_mesh(asset.bytes(), units, MeshLimits::default())?;
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for point in mesh.parts.iter().flat_map(|part| &part.positions_mm) {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    let report = serde_json::json!({
        "schemaVersion":"1.0.0", "scope":"bounded-vrml97-group-transform-triangle-mesh",
        "sourceSha256":asset.sha256(), "sourceBytes":asset.bytes().len(),
        "sourceUnitProfile":args[4], "outputUnits":"millimetres",
        "rootShapes":mesh.root_shape_count, "meshParts":mesh.parts.len(),
        "vertices":mesh.parts.iter().map(|p| p.positions_mm.len()).sum::<usize>(),
        "triangles":mesh.parts.iter().map(|p| p.triangles.len()).sum::<usize>(),
        "materialDefinitions":mesh.material_definitions.len(), "localBoundsMm":{"min":min,"max":max},
        "diagnostics":mesh.diagnostics, "boardTransformsVerified":false,
        "componentSelectionReady":false,"mobileImportReady":false,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|_| CliError::new("Cannot serialize model mesh report"))?
    );
    Ok(())
}
