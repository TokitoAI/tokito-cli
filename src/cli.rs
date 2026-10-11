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
        let candidate = source_candidate::load(
            Path::new(&args[2]),
            Path::new(&args[4]),
            Path::new(&args[6]),
            Path::new(&args[8]),
            Path::new(&args[10]),
            (&args[12], &args[14]),
            args.get(18).map(String::as_str),
        )?;
        let output = Path::new(&args[16]);
        fs::create_dir(output)
            .map_err(|e| CliError::new(format!("cannot create new candidate directory: {e}")))?;
        write_new_file(&output.join("active.digital-twin.glb"), &candidate.glb)?;
        for (name, value) in [
            ("active.digital-twin.json", &candidate.manifest),
            ("component-map.json", &candidate.map),
            ("adaptation-report.json", &candidate.report),
        ] {
            write_new_file(
                &output.join(name),
                &serde_json::to_vec_pretty(value).map_err(|e| CliError::new(e.to_string()))?,
            )?;
        }
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
            // Exclusive creation prevents reuse or replacement of a user's folder.
            fs::create_dir(output).map_err(|e| {
                CliError::new(format!("cannot create new twin output directory: {e}"))
            })?;
            write_new_file(&output.join("active.digital-twin.glb"), &prepared.glb)?;
            for (name, value) in [
                ("active.digital-twin.json", &prepared.manifest),
                ("component-map.json", &prepared.component_map),
                ("component-index.json", &prepared.index),
                ("prepare-report.json", &prepared.report),
            ] {
                write_new_file(&output.join(name), &json_bytes(value)?)?;
            }
            if let Some(viewer) = viewer {
                write_new_file(&output.join("index.html"), viewer.as_bytes())?;
            }
            if let Some(population) = population {
                write_new_file(
                    &output.join("reference-population-audit.json"),
                    &json_bytes(&population)?,
                )?;
            }
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

fn print_help() {
    println!(
        "Tokito CLI 0.1.0\n\
         Offline-first Rust tools for existing PCB layouts.\n\
         \n\
         USAGE:\n\
         \ttokito-cli [--help|-h]\n\
         \ttokito-cli [--version|-V]\n\
         \ttokito-cli version\n\
         \ttokito-cli help [COMMAND...]\n\
         \ttokito-cli pcb [--help]\n\
         \ttokito-cli inspect <BOARD.kicad_pcb|TOKITO_PACKAGE_DIRECTORY> [--json]\n\
         \ttokito-cli bom reconcile <BOARD|PACKAGE> --bom <FILE> --format nxp-json|esp32-csv [--json]\n\
         \ttokito-cli validate outline <BOARD.kicad_pcb|TOKITO_PACKAGE_DIRECTORY> [--json]\n\
         \ttokito-cli resolve models <BOARD|PACKAGE> --project-root <DIR> [--library-root <DIR>] [--model-var NAME=DIR] [--max-model-bytes N] [--max-batch-bytes N] [--json]\n\
         \ttokito-cli model inspect <FILE.wrl> --units millimetres|kicad-wrl [--json]\n\
         \ttokito-cli model assignments template <BOARD|PACKAGE> --output <FILE.json>\n\
         \ttokito-cli model assignments verify <BOARD|PACKAGE> --manifest <FILE.json> --project-root <DIR> [--json]\n\
         \ttokito-cli component model envelope <SPEC.json> --board <BOARD|PACKAGE> --output <FILE.wrl> [--json]\n\
         \ttokito-cli twin inspect|prepare|prototype|select <GLB> --component-map <JSON> --manifest <JSON> [--output <NEW_DIR> [--allow-partial] | --reference <REF> | --class <LABEL>]\n\
         \ttokito-cli export placements <BOARD.kicad_pcb> --output <FILE.csv>\n\
         \ttokito-cli export svg <BOARD.kicad_pcb> --output <FILE.svg> [--side top|bottom] [--layers LAYER,...]\n\
         \ttokito-cli export glb <BOARD|TOKITO_PACKAGE> --output <FILE.glb> [--assemble-wrl --project-root <DIR> [--library-root <DIR>] [--model-var NAME=DIR] | --model-assignments <FILE.json> --project-root <DIR>] [--allow-partial] [--json-report]\n\
         \ttokito-cli package build <BOARD.kicad_pcb> --output <DIRECTORY>\n\
         \ttokito-cli package verify <DIRECTORY> [--json]\n\
         \n\
         COMMAND GROUPS:\n\
         \tpcb      Existing physical-board import, inspection, model resolution and exports.\n\
         \tmodel    Inspect the bounded local VRML profile.\n\
         \tbom      Reconcile dataset BOM references with PCB identities.\n\
         \tcomponent Generate an evidence-bound provisional component body envelope.\n\
         \ttwin     Audit and prepare per-component renderable selectors, or query authored labels.\n\
         \tdataset  Lock and verify explicitly listed offline dataset assets.\n\
         \tpackage  Build or verify a Tokito source-and-inventory package.\n\
         \n\
         VERSION:\n\
         \tversion, --version and -V print the Tokito CLI version.\n\
         \n\
         HELP:\n\
         \t-h and --help print root or command-specific help.\n\
         \t`help <COMMAND...>` prints help for a supported command path.\n\
         \n\
         PCB ALIASES:\n\
         \ttokito-cli pcb inspect <BOARD|PACKAGE> [--json]\n\
         \ttokito-cli pcb validate outline <BOARD|PACKAGE> [--json]\n\
         \ttokito-cli pcb export svg|glb|placements ...\n\
         \ttokito-cli pcb resolve models <BOARD|PACKAGE> ...\n\
         \ttokito-cli pcb bom reconcile <BOARD|PACKAGE> --bom <FILE> --format nxp-json|esp32-csv [--json]\n\
         \n\
         Use --help after a command path or `tokito-cli help <COMMAND...>` for command-specific help.\n\
         \n\
         CURRENT CAPABILITIES:\n\
         \tInspect board metadata and component placements.\n\
         \tExport placement rows in the source board coordinate frame.\n\
         \tExport partial physical-scale SVG views from supported geometry.\n\
         \tExport a board shell, or explicitly assemble the supported local VRML subset.\n\
         \tBuild a source-and-placement Tokito package (not mobile importable).\n\
         \tSTEP and schematic-to-PCB are not implemented; other model formats remain explicit partial coverage."
    );
}

fn print_command_help(path: &[String]) -> Result<(), CliError> {
    if path.len() == 1 && path[0] == "pcb" {
        print_pcb_help();
        return Ok(());
    }
    let path = if path.first().is_some_and(|part| part == "pcb") {
        &path[1..]
    } else {
        path
    };
    match path
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => print_help(),
        ["version"] => println!("USAGE: tokito-cli version\nPrint the Tokito CLI version."),
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
            "USAGE: tokito-cli resolve models <BOARD|PACKAGE> --project-root <DIR> [--library-root <DIR>] [--model-var NAME=DIR] [--max-model-bytes N] [--max-batch-bytes N] [--json]\nCaptures local model assets under caller-approved roots; it does not decode them."
        ),
        ["model", "assignments"]
        | ["model", "assignments", "template"]
        | ["model", "assignments", "verify"] => println!(
            "USAGE: tokito-cli model assignments template <BOARD|PACKAGE> --output <FILE.json>\n       tokito-cli model assignments verify <BOARD|PACKAGE> --manifest <FILE.json> --project-root <DIR> [--json]\nAssigned bodies: export glb <BOARD> --model-assignments <FILE.json> --project-root <DIR> --output <FILE.glb> --allow-partial --json-report\nAssignment and rendered coverage do not certify part identity, pose or geometry."
        ),
        ["component"] | ["component", "model"] | ["component", "model", "envelope"] => println!(
            "USAGE: tokito-cli component model envelope <SPEC.json> --board <BOARD|PACKAGE> --output <FILE.wrl> [--json]\nGenerates only a dimensioned rectangular body envelope from explicit source dimensions. This does not generate a footprint, terminals, or a qualified physical CAD model."
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
        ["twin"]
        | ["twin", "inspect"]
        | ["twin", "prepare"]
        | ["twin", "prototype"]
        | ["twin", "select"] => println!(
            "USAGE: tokito-cli twin adapt-source <GLB> --component-map <JSON> --geometry-index <JSON> --provenance <JSON> --coverage <JSON> --board-id <ID> --board-revision <REV> --output <NEW_DIR>\n       tokito-cli twin prototype <GLB> --component-map <JSON> --manifest <JSON> --output <NEW_DIR> [--allow-partial] [--reference-bom <esp32-csv>]\n       tokito-cli twin inspect <GLB> --component-map <JSON> --manifest <JSON>\n       tokito-cli twin prepare <GLB> --component-map <JSON> --manifest <JSON> --output <NEW_DIR> [--allow-partial]\n       tokito-cli twin select <GLB> --component-map <JSON> --manifest <JSON> --reference <REF>|--class <AUTHORED_LABEL>\nprototype uses the same preparation gates and writes an offline index.html with orbit, zoom and per-component highlighting. Moves selector names onto isolated mesh-bearing nodes without changing geometry or transforms. Outputs an unregistered reference candidate; board-reference admission, physical fit, App import and colour rendering require separate acceptance."
        ),
        ["twin", "adapt-source"] => println!(
            "USAGE: tokito-cli twin adapt-source <GLB> --component-map <JSON> --geometry-index <JSON> --provenance <JSON> --coverage <JSON> --board-id <ID> --board-revision <REV> --output <NEW_DIR> [--board-plane-node <EXACT_SOURCE_NODE>]\nThe optional node must name one active, nonempty source subtree. Its decoded world bounds are retained separately from whole-scene bounds; author selection does not grant physical or pose qualification."
        ),
        ["model"] | ["model", "inspect"] => println!(
            "USAGE: tokito-cli model inspect <FILE.wrl> --units millimetres|kicad-wrl [--json]\nInspects the bounded flat VRML triangle profile."
        ),
        ["export"] | ["export", "placements"] => println!(
            "USAGE: tokito-cli export placements <BOARD.kicad_pcb> --output <FILE.csv>\nWrites source-frame component placement and identity rows."
        ),
        ["export", "svg"] => println!(
            "USAGE: tokito-cli export svg <BOARD|PACKAGE> --output <FILE.svg> [--side top|bottom] [--layers LAYER,...]\nWrites a partial SVG view from supported board geometry."
        ),
        ["export", "glb"] => println!(
            "USAGE: tokito-cli export glb <BOARD|PACKAGE> --output <FILE.glb> [--assemble-wrl --project-root <DIR> [--library-root <DIR>] [--model-var NAME=DIR] | --model-assignments <FILE.json> --project-root <DIR>] [--outline-chaining-tolerance 0.000001|0.001|0.01] [--allow-partial] [--json-report]\nExports a validated board shell. --assemble-wrl additionally emits the bounded supported local VRML subset. --model-assignments emits explicitly assigned candidate bodies and always requires --allow-partial. STEP, general VRML, glTF/GLB model import and phone import are unsupported."
        ),
        ["package"] | ["package", "build"] => println!(
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

fn print_pcb_help() {
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
         \ttokito-cli pcb bom reconcile <BOARD|PACKAGE> --bom <FILE> --format nxp-json|esp32-csv [--json]\n\
         \ttokito-cli pcb export svg <BOARD|PACKAGE> --output <FILE.svg> [--side top|bottom]\n\
         \ttokito-cli pcb export glb <BOARD|PACKAGE> --output <FILE.glb> [--assemble-wrl ...] [--allow-partial]\n\
         \n\
         No schematic-to-PCB, STEP conversion, Gerber, symbol, footprint-library or jobset command is implemented yet."
    );
}

pub(super) fn run(args: &[String]) -> Result<(), CliError> {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_help();
        return Ok(());
    }
    if args[0] == "help" {
        return print_command_help(&args[1..]);
    }
    if let Some(help_index) = args
        .iter()
        .position(|argument| argument == "--help" || argument == "-h")
    {
        return print_command_help(&args[..help_index]);
    }
    if args[0] == "--version" || args[0] == "-V" {
        println!("tokito-cli 0.1.0");
        return Ok(());
    }
    if args[0] == "version" {
        if args.len() != 1 {
            return Err(CliError::new("usage: tokito-cli version"));
        }
        println!("tokito-cli 0.1.0");
        return Ok(());
    }
    if args[0] == "pcb" {
        if args.len() == 1
            || args
                .get(1)
                .is_some_and(|argument| argument == "--help" || argument == "-h")
        {
            print_pcb_help();
            return Ok(());
        }
        let mut delegated = match args[1].as_str() {
            "inspect" => vec!["inspect".to_owned()],
            "validate" => vec!["validate".to_owned()],
            "export" => vec!["export".to_owned()],
            "resolve" => vec!["resolve".to_owned()],
            "bom" => vec!["bom".to_owned()],
            _ => {
                print_pcb_help();
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
    if args[0] == "export" && args.get(1).is_some_and(|value| value == "svg") {
        if args.len() < 5 {
            return Err(CliError::new(
                "usage: tokito-cli export svg <BOARD.kicad_pcb> --output <FILE.svg> [--side top|bottom] [--layers LAYER,...]",
            ));
        }
        let mut side = "top";
        let mut output_path = None;
        let mut requested_layers = None;
        let mut index = 3;
        while index < args.len() {
            match args[index].as_str() {
                "--side" if index + 1 < args.len() => {
                    side = match args[index + 1].as_str() {
                        "top" => "top",
                        "bottom" => "bottom",
                        _ => return Err(CliError::new("--side must be 'top' or 'bottom'")),
                    };
                    index += 2;
                }
                "--output" if index + 1 < args.len() => {
                    output_path = Some(PathBuf::from(&args[index + 1]));
                    index += 2;
                }
                "--layers" if index + 1 < args.len() => {
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
        let svg = board.svg(SvgSide::try_from(side)?, requested_layers.as_deref())?;
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
                "usage: tokito-cli export glb <BOARD|TOKITO_PACKAGE> --output <FILE.glb> [--assemble-wrl --project-root <DIR> [--library-root <DIR>] [--model-var NAME=DIR] | --model-assignments <FILE.json> --project-root <DIR>] [--allow-partial] [--json-report]",
            ));
        }
        let mut output_path = None;
        let mut allow_partial = false;
        let mut json_report = false;
        let mut assemble_wrl = false;
        let mut assignments_path = None;
        let mut chaining_tolerance = None;
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
        let export = if assemble_wrl {
            use crate::model_resolver::{ModelResolver, ModelResolverConfig};
            let mut config = ModelResolverConfig::new(project_root.ok_or_else(|| {
                CliError::new("GLB --assemble-wrl requires --project-root <DIR>")
            })?);
            config.library_roots = library_roots;
            config.directory_variables = variables;
            let resolver = ModelResolver::new(config)?;
            board.export_glb_with_models(&resolver, GlbExportOptions { allow_partial })?
        } else {
            if project_root.is_some() || !library_roots.is_empty() || !variables.is_empty() {
                return Err(CliError::new(
                    "GLB model roots require --assemble-wrl; board-shell export never reads model files",
                ));
            }
            board.export_glb(GlbExportOptions { allow_partial })?
        };
        write_new_file(&output_path, &export.bytes)?;
        if json_report {
            print!("{}", export.report.report_json());
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
    print_help();
    Err(CliError::new("unknown command"))
}

/// Asset inventory only: successful lookup never certifies model geometry.
fn resolve_models(args: &[String]) -> Result<(), CliError> {
    use crate::model_resolver::{ModelResolutionStatus, ModelResolver, ModelResolverConfig};
    const MAX_REFERENCES: usize = 1024;
    const MAX_CAPTURED_BYTES: usize = 256 * 1024 * 1024;
    let usage = "usage: tokito-cli resolve models <BOARD|PACKAGE> --project-root <DIR> [--library-root <DIR>] [--model-var NAME=DIR] [--max-model-bytes N] [--max-batch-bytes N] [--json]";
    if args.len() < 5 || args.len() > 265 {
        return Err(CliError::new(usage));
    }
    let mut project_root = None;
    let mut roots = Vec::new();
    let mut variables = std::collections::BTreeMap::new();
    let mut json = false;
    let mut file_budget = None;
    let mut batch_budget = None;
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
    let mut captured_bytes = 0usize;
    let mut resolved = 0usize;
    let mut reports = Vec::with_capacity(models.len());
    for model in models {
        let resolution = resolver.resolve(model);
        if let Some(asset) = &resolution.asset {
            captured_bytes += asset.bytes().len();
            if captured_bytes > batch_budget {
                return Err(CliError::new("Model batch exceeds captured-byte budget"));
            }
        }
        resolved += usize::from(resolution.status == ModelResolutionStatus::Resolved);
        reports.push(resolution.shared_projection());
    }
    let report = serde_json::json!({
        "schemaVersion":"1.0.0", "scope":"offline-model-asset-resolution",
        "sourceSha256":board.source().sha256,
        "modelReferences":models.len(), "resolvedAssets":resolved,
        "unresolvedReferences":models.len()-resolved, "capturedBytes":captured_bytes,
        "geometryValidated":false, "componentSelectionReady":false, "mobileImportReady":false,
        "models":reports,
        "limits":{"maxReferences":MAX_REFERENCES,"maxBatchCapturedBytes":batch_budget,
                  "maxFileBytes":file_budget},
    });
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
            "usage: tokito-cli model inspect <FILE.wrl> --units millimetres|kicad-wrl [--json]",
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
            "Only the declared flat triangle VRML97 profile is supported",
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
        "schemaVersion":"1.0.0", "scope":"flat-triangle-vrml97-local-mesh",
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
