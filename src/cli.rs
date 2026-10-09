use super::*;

fn print_help() {
    println!(
        "Tokito CLI 0.1.0\n\
         Offline-first Rust tools for existing PCB layouts.\n\
         \n\
         USAGE:\n\
         \ttokito-cli inspect <BOARD.kicad_pcb|TOKITO_PACKAGE_DIRECTORY> [--json]\n\
         \ttokito-cli validate outline <BOARD.kicad_pcb|TOKITO_PACKAGE_DIRECTORY> [--json]\n\
         \ttokito-cli export placements <BOARD.kicad_pcb> --output <FILE.csv>\n\
         \ttokito-cli export svg <BOARD.kicad_pcb> --output <FILE.svg> [--side top|bottom] [--layers LAYER,...]\n\
         \ttokito-cli export glb <BOARD|TOKITO_PACKAGE> --output <FILE.glb> [--allow-partial] [--json-report]\n\
         \ttokito-cli package build <BOARD.kicad_pcb> --output <DIRECTORY>\n\
         \ttokito-cli package verify <DIRECTORY> [--json]\n\
         \n\
         CURRENT CAPABILITIES:\n\
         \tInspect board metadata and component placements.\n\
         \tExport placement rows in the source board coordinate frame.\n\
         \tExport partial physical-scale SVG views from supported geometry.\n\
         \tExport a board-shell-only GLB; unresolved component models require --allow-partial.\n\
         \tBuild a source-and-placement Tokito package (not mobile importable).\n\
         \tComponent 3D models, STEP, and schematic-to-PCB are not implemented."
    );
}

pub(super) fn run(args: &[String]) -> Result<(), CliError> {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_help();
        return Ok(());
    }
    if args[0] == "--version" || args[0] == "-V" {
        println!("tokito-cli 0.1.0");
        return Ok(());
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
                "Geometry exports: partial SVG and explicit board-shell-only GLB; component models and STEP are not implemented"
            );
        }
        return Ok(());
    }
    if args[0] == "validate" && args.get(1).is_some_and(|value| value == "outline") {
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
                "usage: tokito-cli export glb <BOARD|TOKITO_PACKAGE> --output <FILE.glb> [--allow-partial] [--json-report]",
            ));
        }
        let mut output_path = None;
        let mut allow_partial = false;
        let mut json_report = false;
        let mut index = 3;
        while index < args.len() {
            match args[index].as_str() {
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
        let export = board.export_glb(GlbExportOptions { allow_partial })?;
        write_new_file(&output_path, &export.bytes)?;
        if json_report {
            print!("{}", export.report.report_json());
        } else {
            println!(
                "Wrote {} GLB ({} of {} component models emitted; {} unresolved) to {}",
                if export.report.partial_export {
                    "explicit partial board-shell-only"
                } else {
                    "board-shell-only"
                },
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
