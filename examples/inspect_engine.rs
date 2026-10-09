//! Real library-consumer example. Source model references are reported, never
//! resolved to guessed meshes. Run with one PCB path or v0.1 package directory.

use std::error::Error;
use tokito_cli::load_board_input;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let path = arguments
        .next()
        .ok_or("usage: inspect_engine <PCB-or-package-path>")?;
    if arguments.next().is_some() {
        return Err("expected exactly one PCB or package path".into());
    }
    let board = load_board_input(&path)?;
    let metadata = board.source();
    let engineering = board.engineering();
    println!("Source: {}", metadata.path.display());
    println!(
        "Format: KiCad {}, units: {:?}, frame: {:?}",
        metadata.format_version, metadata.units, metadata.coordinate_frame
    );
    println!("SHA-256: {}", metadata.sha256);
    println!(
        "Components: {}, pads: {}, tracks: {}, arcs: {}, vias: {}, zones: {}",
        engineering.components.len(),
        engineering.pads.len(),
        engineering.track_segments.len(),
        engineering.track_arcs.len(),
        engineering.vias.len(),
        engineering.zones.len()
    );
    for component in &engineering.components {
        println!(
            "Component {:?}: reference={:?}, uuid={:?}, layer={:?}, position_mm={:?}",
            component.source.source_path,
            component.reference,
            component.source.source_uuid,
            component.layer,
            component.source_board_pose.position_mm
        );
    }
    println!(
        "Supported render primitives: {}; geometry complete: {}",
        board.geometry().item_count(),
        board.geometry_complete()
    );
    for model in &engineering.models {
        println!("Unresolved source model reference: {:?}", model.path);
    }
    for diagnostic in &engineering.diagnostics {
        println!(
            "{:?} at {:?}: {}",
            diagnostic.code, diagnostic.source.source_path, diagnostic.message
        );
    }
    for diagnostic in board.geometry().diagnostics() {
        println!("Render projection diagnostic: {diagnostic}");
    }
    Ok(())
}
