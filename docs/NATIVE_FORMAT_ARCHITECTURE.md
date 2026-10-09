# Native formats and ecosystem architecture

Reviewed 2026-10-09 against local brain 00cbaa8fa3db107b61a02452f95d3dd35bc95f63 and desktop e25ba7b2783f002318be07a9287a2476a1d19bff. Brain is a source map; live code defines implemented behavior. No sibling repository was edited.

## Document kinds and adapters

| Input | Meaning | Current CLI handling |
|---|---|---|
| Reviewed .kicad_pcb | Existing physical board placement/routing | Bounded inventory and partial SVG |
| Tokito board package v0.1 directory | Fixed manifest + preserved PCB + derived inventory | Verify integrity/contract; inspect and SVG export from preserved source |
| Desktop schematic_document.json, schema_version 2 | Native schematic document, UUID symbols, connectivity/hierarchy | Architecture identified; dedicated adapter not implemented |
| Desktop design JSON | Envelope containing design/BOM/schematic_document | Dedicated envelope adapter not implemented |
| Desktop project ZIP | Project manifest and per-design documents/exports | Not implemented; future read-only extraction must not execute database.sql |
| .tokito_sym | tokito_symbol_lib symbol library | Symbol definitions; not a board input; adapter not implemented |
| .net | Connectivity and footprint hints | Future layout input; not physical placement |
| tokito_mcad_handoff_v1 | Schematic-derived footprint hints | Must not be used as authoritative PCB placement |
| Hypothetical .tokito_pcb | No shipped contract found | Do not invent a parser or report support |

Authoritative native schematic topology belongs to tokito-schematic-core CoreDocument. Desktop persists schema-v2 SchematicDocument. Schematic coordinates use 40 world units per 1.27 mm; MCAD handoff currently writes raw positions under x_mm/y_mm and omits symbol-instance UUID. That is a source finding for separately authorized desktop work, not a conversion rule to adopt.

## Current implementation boundary

The executable and public engine library remain one Rust crate. Main handles the process boundary, cli.rs dispatches commands, and lib.rs exposes PhysicalBoardDocument and operations. document.rs supplies typed source-semantic records; geometry.rs supplies the existing supported render projection; package_input.rs and strict_json.rs enforce native intake. Canonical identities/poses/counts feed the frozen inventory projection, and SVG selects source nodes by canonical locators. Render helpers still interpret retained saved nodes; a completely normalized renderer and topology graph are later work. See [Milestone A API and limits](CANONICAL_ENGINE.md).

Native package intake validates the fixed layout, schema/producer contract, source and derived digests, and placement/inspection consistency with freshly parsed source. Duplicate JSON keys, unknown package versions, arbitrary manifest paths, links/reparse points, and unsupported source versions are errors. This milestone keeps v0.1 inspection/placement projections unchanged and tests prior-binary packages; future breaking diagnostic/geometry changes need an explicit compatibility decision.

The package remains source-and-placement-inventory. SVG is a separate command and is not silently added to v0.1. mobileImportReady remains false. This avoids a breaking manifest change while permitting local package round trips.

## Target engine architecture

1. Input router identifies a document kind and version. Never identify arbitrary JSON as physical geometry.
2. Format adapters return either SchematicDocument or PhysicalBoardDocument with an explicit loss/capability report.
3. PhysicalBoardDocument holds stable source identities, origin/units, layer/net relationships, outline contours and cutouts, footprint instances, pad IDs, drills, model references/transforms, source digest and diagnostics.
4. Operation layer validates coverage before placement/SVG/GLB/STEP generation. No successful placeholder operation.
5. Package writer records immutable source/assets, artifact digests, provenance, coordinate transforms and capability states.
6. Thin CLI now calls the public pure Rust library. Future local MCP and Desktop adapters consume a pinned engine library or versioned bounded subprocess job; those integrations remain unimplemented. The initial Rust API is experimental and must be stabilized before independent consumers rely on it.
7. Optional catalog enrichment belongs outside deterministic offline import/export. Do not embed Cloud AI/provider loops in the engine.

Avoid depending on complete desktop or catalog crates: bundled SQLite/native dependencies and catalog ring/cc dependencies conflict with the current pure-Rust constraint. Schematic-core is reusable only through its actual schematic contract; it does not create a PCB.

## Coordinates established for SVG

Source units are mm. Saved child positions are footprint-relative; at parent angle θ:

- board X = origin X + local X cos θ + local Y sin θ
- board Y = origin Y − local X sin θ + local Y cos θ

Saved back-side geometry already carries its flip, so no second local reflection is applied. Saved pad/text angles are absolute and use the opposite sign in SVG. Whole-board bottom view is a separate horizontal mirror about the outline center. SVG width/viewBox includes outline stroke and a 0.5 mm display margin; it is not nominal fabrication dimensions.

Exact-coordinate regressions verify (10,20,90) plus child (2,3) -> (13,18) on either source side, pad absolute 30 -> SVG -30. This is independently researched serialization behavior, not a visual conformance claim.

## GLB iteration prerequisites

Future mesh frame: metres, right-handed Y-up, board in X/Z. Proposed source mapping (x,y,z above board) -> (x/1000,z/1000,y/1000) with recorded origin. Verify winding, normals, source angle and top/bottom placement mathematically before exporting. Do not apply the mirrored 2D display view to mesh coordinates.

- Build robust closed contours/cutouts and self-intersection tests, then triangulate/extrude the actual board with drill holes.
- Resolve explicit local model roots and model offsets/scales/rotations; do not expand arbitrary machine environment or fetch network assets implicitly.
- Prefer known glTF/GLB meshes first. KiCad STEP/VRML references need separate pure-Rust conversion coverage. Unresolved components stay unresolved, not silently replaced by boxes.
- Preserve source component UUID -> glTF node index -> renderer ref-name mapping. Reference names are mutable labels and are not global IDs. SVG currently emits data-component-id on footprint primitives.
- Validate actual GLB scene accessors/indices/transforms, node mapping, bounds and hashes, then compare picking in a viewer. Container validity alone is insufficient.
- Phone contract is separate: board/assembly-context revision, GLB digest/size, metric bounds/origin, component node prefix. Current limits: GLB 64 MiB, manifest 64 KiB. SHA-256 is not publisher authentication.
- App currently reads fixed private context files and exports session/model-bundle APIs; no board-package picker/import API exists. A user-selected offline importer is proposed future app work. Desktop must not write into mobile private storage.

STEP remains a separate geometry/topology spike. SVG progress is not proof of STEP capability.

## Verified source anchors

- tokito/src/models/schematic_document.rs: SchematicDocument and SCHEMATIC_DOC_SCHEMA_VERSION.
- tokito/src/handlers/designs.rs: native design JSON envelope.
- tokito/src/services/project_archive.rs: project ZIP layout.
- tokito/src/services/mcad_export.rs: schematic-derived handoff.
- tokito/native/src/editor/core_runtime.rs: authoritative CoreDocument boundary.
- tokito-catalog/src/symbol_format/lib_file.rs: tokito_symbol_lib.
- tokito-app/physical-runtime/android/ar-runtime/src/main/kotlin/ai/tokito/physical/ar/DigitalTwinAsset.kt: GLB intake contract.
- tokito-app/docs/PRODUCT_BOUNDARY.md and apps/mobile/modules/tokito-physical/android/src/main/java/expo/modules/tokitophysical/TokitoPhysicalModule.kt: mobile boundary, no PCB importer.
- Brain platform/pins.md is stale for agent-contract v0.10.18 versus desktop manifest v0.10.23. No version pins were changed here.
- [KiCad 9.0.0 formatter](https://raw.githubusercontent.com/KiCad/kicad-source-mirror/9.0.0/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr.cpp), [9.0.0 parser](https://raw.githubusercontent.com/KiCad/kicad-source-mirror/9.0.0/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr_parser.cpp), [10.0.0 rotation behavior](https://gitlab.com/kicad/code/kicad/-/raw/10.0.0/libs/kimath/src/trigo.cpp). Consulted for behavior only; no source copied or translated.
