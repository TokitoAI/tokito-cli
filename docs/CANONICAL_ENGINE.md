# Canonical physical engine: Milestone A

The crate now exposes `tokito_cli` as a Rust library alongside the unchanged
`tokito-cli` binary. `src/main.rs` handles process arguments and exit status;
`src/cli.rs` handles command dispatch; `src/lib.rs` owns import, inspection,
placement, package and SVG operations. `src/document.rs` contains the typed
source-semantic physical inventory. This is an experimental Rust API, not a
new serialized board format or complete CAD geometry engine.

## Public operations

- `load_kicad_board(path)` loads a PCB within the existing reviewed format range.
- `load_board_input(path)` accepts that PCB or the fixed v0.1 Tokito package.
- `verify_package(path)` verifies and reimports the package's preserved source.
- `PhysicalBoardDocument::source()` declares source digest, format date,
  millimetres and the saved KiCad board frame with Y increasing downwards.
- `engineering()` returns layers, nets, components, pads, routed segments/arcs,
  vias, zones, graphics, model references and typed diagnostics.
- `geometry()` returns the supported board-space render primitives and outline
  bounds. Pad/text render angles have the SVG sign; source angles remain in
  engineering records.
- `outline()` returns supported physical Edge.Cuts contours, source locators,
  centerline bounds, cutout roles and fail-closed topology diagnostics.
- `outline_json()` is a separate report; `inspect --json` remains the v0.1
  projection. The CLI exposes `validate outline <BOARD|PACKAGE> [--json]`.
- `source_syntax()` retains the original parsed source, including uninterpreted
  fields; `source_node(locator)` resolves a canonical record's source path.
- `inspection_json()`, `placements_csv()`, `svg(SvgSide, layers)` and
  `build_package(path)` expose existing operations without process invocation.

The `inspect_engine` example is a separate Rust consumer. Run it with
`cargo run --offline --locked --example inspect_engine -- <PCB-or-package>`.
It performs no model download or cloud request.

## Authority and compatibility

Import creates one immutable `PhysicalBoardDocument`. Its typed engineering
inventory is parsed first. Legacy footprint inspection/placement fields derive
their identities, labels, valid poses and counts from those records. SVG selects
source objects through their canonical locators and uses the shared footprint
transform. Existing render helpers still interpret the selected saved nodes;
this incremental compatibility bridge is not a completely normalized renderer.

The frozen v0.1 projection retains original diagnostics, flag handling and
individually malformed XY scalars. New typed engineering diagnostics do not
silently change its inspection JSON, placement CSV or package schema. SVG stays
a separate artifact and `mobileImportReady` remains false. Old-package acceptance
must be checked against packages produced by the previous binary, in addition
to current-writer round trips.

Canonical paths are zero-based child ordinals excluding each list's head atom.
They distinguish repeated/anonymous pads without manufacturing a UUID. A
`component_ordinal` addresses the component's root child ordinal, not its index
in the component vector. Source UUIDs are retained exactly, including missing
or duplicate identities; diagnostics report ambiguity. Shared identity and
resolved-net text prevents repeated canonical records from copying large names.
Legacy render metadata still has its earlier allocations and is not a hard
total-memory budget.

First exact-name `Reference`/`Value` properties remain authoritative, including
an empty/missing value. Legacy `fp_text` fallback applies only when that property
is absent. All properties remain available and duplicate/conflicting properties
receive typed diagnostics. A unique net-table ID may resolve a net name; duplicate
IDs, missing names and conflicting saved names are not guessed into connectivity.

## Geometry and coverage limits

Saved child XY is footprint-relative; at angle theta it maps to
`(ox + x*cos(theta) + y*sin(theta), oy - x*sin(theta) + y*cos(theta))`.
Saved back-side children already carry their flip, so no additional reflection
is applied. Saved pad/text angles are absolute. Whole-board bottom SVG mirroring
is a display operation, separate from physical source coordinates.

Malformed coordinates remain absent. Polygon entries retain absent point slots
so consumers cannot silently join across invalid points. Unsupported pad/graphic
syntax retains source records and diagnostics. Diagnostics are capped at 500
records with an explicit truncation record when the importer reaches its budget.
Raw syntax preservation is not a claim that every unknown construct is diagnosed.

Zone boundaries and cached fills remain separate source data. The canonical
zone list also retains footprint-local zones and declares their CoordinateFrame;
legacy inspection counts board-root zones only, and its SVG projection does not
render footprint-local zones. No zone fill, connectivity solving or DRC is
performed. Modern model offsets, rotations, scales
and legacy model `at` fields remain separate; model assets are neither resolved
nor converted. No missing model is replaced by a fabricated body.

`geometry_complete()` remains false. The new outline gate checks only its
explicitly supported Edge.Cuts subset; it is not DRC, complete KiCad
interpretation, or fabrication validation. It accepts one outer contour and
direct cutouts, uses a declared 1e-6 mm join tolerance and bounded work.
Unsupported and legacy module Edge.Cuts fail closed. Invalid outlines remain
available for source-inventory inspection and package verification but cannot
be SVG-exported. See [PCB topology](PCB_TOPOLOGY.md) for limits and fixtures.

SVG uses stable physical arc math for Edge.Cuts and keeps the prior drawing
projection on other layers. The outline API reports centerline bounds; the SVG
view bounds include stroke and the existing display margin. Four prior board
outputs were compared byte-for-byte after this distinction was enforced.

No GLB/STEP, desktop-native schematic adapter, Desktop/MCP wiring or phone
package acceptance is implemented.

## Next dependency

Milestone A passed its original Windows x64/Rust 1.94.1 suite. Milestone B's first
full run passed 40 module and 17 public API tests, formatting, Clippy, release,
four-board CLI E2E, real topology/reference E2E and five historical packages.
The exact prior-binary comparison now also passes for all four boards; rerun the
full verification after the final compatibility fix before using the newest
receipt. See [local delivery](CI_AND_DELIVERY.md). Windows evidence does not
establish Linux/MSRV or visual CAD/device acceptance.

Milestone B implements contour topology and a pinned Gerber comparison. After
complete verification and independent review, next establish mesh triangulation,
sourced model transforms and UUID picking, then a versioned geometry package
extension and separately authorized Desktop/mobile adapters. Schematics need
physical PCB placement/routing before they can become a board digital twin.
