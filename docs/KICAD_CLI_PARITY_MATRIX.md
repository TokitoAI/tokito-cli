# KiCad CLI compatibility matrix

Status: planning and evidence map only. This document does not claim that Tokito is a KiCad CLI clone.

## Reference target and meaning of compatibility

The current official [KiCad 10.0 CLI manual](https://docs.kicad.org/10.0/en/cli/cli.html) says it is based on KiCad 10.0.7 (documentation revision `efa5960e`) and documents six root command groups: `fp`, `jobset`, `pcb`, `sch`, `sym`, and `version`. KiCad 10.0.7 is the recommended reference baseline for a first parity matrix because the manual and the PCB format review already use that release. This is a proposal until the project owner confirms it. The local executable at `C:\Program Files\KiCad\10.0\bin\kicad-cli.exe` reported 10.0.5 on 2026-10-10. Its `pcb export svg`, `pcb export glb`, and `pcb export pos` help was captured as syntax reconnaissance only; no board export was run and it cannot establish 10.0.7 parity. The source archaeology links in [KICAD_CLI_BEHAVIOR.md](KICAD_CLI_BEHAVIOR.md) include 10.0.0 source snapshots; those links are useful design evidence, not proof that every 10.0.7 option behaves identically.

The requested end state is a Rust Tokito CLI with full behavioral compatibility for the selected KiCad CLI release and its documented command surface. This is the target, not the current state: the repository implements a small PCB-focused subset. Here, “exact” means matching documented user-visible commands and results for each declared option; it does not mean copying KiCad internals or source. Compatibility is earned per command and option only when Tokito reproduces observable behavior on pinned fixtures. Matching a command name does not establish matching file semantics, transforms, rendering, reports, error codes, or edge cases. Tokito remains an original pure-Rust implementation; the product workflow must not invoke KiCad CLI or require a KiCad installation.

This matrix distinguishes:

- **Syntax parity:** command path, options, defaults, validation and exit/result behavior.
- **Semantic parity:** the selected objects, coordinate frame, filters, units and transformations agree with the documented operation.
- **Artifact compatibility:** output is valid for a named consumer and has verified geometry and identity coverage. Byte-for-byte output equality is not assumed unless explicitly required.

Unsupported options must fail with a diagnostic. Silently accepting and ignoring an option is not parity.

2026-10-10 C-06 addition: offline exact-path asset capture, a flat-triangle
VRML97 local-mesh profile, and source-derived point/parsed-mesh placement helpers
are available through Tokito-native APIs/commands. The helpers do not establish
KiCad transform parity or provide stackup-derived planes, general WRL/STEP
support, or component geometry in the GLB scene. Source-behavior research
now pins KiCad 10.0.7 commit `69cafb907ebc09e7722a2a464b1d466fc2e3608d`;
the release remains a proposed compatibility reference until ratified. See
[model semantics and remaining gates](MODEL_RESOLUTION.md).

## Current command coverage

The CLI currently exposes Tokito-native command names; it does not implement the KiCad command tree. Evidence is in `src/cli.rs`, `src/lib.rs`, `docs/CANONICAL_ENGINE.md`, `docs/GLB_EXPORT_ARCHITECTURE.md`, and `docs/TOKITO_BOARD_PACKAGE_V0.md`.

| KiCad 10.0 family / operation | KiCad documented scope | Tokito CLI today | Compatibility status and next decision |
|---|---|---|---|
| Root help and `version` | Six command groups; scoped help; version formats | Global Tokito help/version only | No syntax parity. Add KiCad-like command paths only when their operations are implemented and testable. |
| `fp export svg`, `fp upgrade` | Footprint-library SVG and library conversion/upgrade | None | Defer; separate library adapter, unrelated to physical board conversion. |
| `jobset run` | Run selected or all project export jobs with stop-on-error behavior | None | Defer until individual outputs and their manifest are stable; later map a typed job manifest onto shared engine operations. |
| `pcb drc` | Electrical/manufacturing design-rule analysis and reports | `validate outline` checks a narrow physical-contour topology subset | Not equivalent. Outline validation is not DRC, connectivity checking, schematic parity, or manufacturability approval. |
| PCB 2D / manufacturing exports | Gerber, drill, DXF, GenCAD, IPC-2581, IPC-D-356, ODB++, PDF, PostScript, SVG, and related plot options | Partial SVG; source-frame placement inventory CSV | No manufacturing parity. SVG omits full KiCad plot/font semantics. CSV is not KiCad POS: it does not implement manufacturing origin, units, side filtering, bottom-side mirroring, or exclusion semantics. |
| PCB statistics | Board statistics and optional area calculations | Inspection inventory | Not equivalent; inventory fields do not claim KiCad statistics semantics. |
| `pcb export glb` and other 3D outputs | GLB and multiple CAD/mesh outputs with component/model filters, variants, origin, copper/graphics switches, and missing-model behavior | GLB of a validated board shell only; it is always classified partial and always requires explicit `--allow-partial`, even when the board has no components | Partial implementation, not 3D parity. No component meshes, selectable component nodes, 3D rendering, STEP, BREP, VRML, STL, 3D PDF, or equivalent model-substitution behavior. |
| `pcb import` | Convert supported non-KiCad PCB formats into KiCad PCB format | Read a supported subset of KiCad PCB into Tokito's canonical model | Opposite conversion direction and different scope. No non-KiCad PCB adapter is implemented. |
| `pcb render` | Camera-controlled rendered board image | None | Defer; a valid GLB mesh is not a renderer or a rendering-equivalence claim. |
| `pcb upgrade` | Update an older PCB file to the current KiCad format | None | Defer; preserve source files and do not rewrite user designs without a separately specified migration contract. |
| `sch erc`, schematic exports, `sch upgrade` | ERC; BOM, netlist and drawing exports; schematic upgrade | No KiCad schematic parser or operations | Defer until Desktop's current schematic/connectivity handoff is pinned. A schematic is not a physical board placement. |
| `sym export svg`, `sym upgrade` | Symbol-library SVG and conversion/upgrade | None | Defer as a separate symbol-library adapter. Tokito MCP/catalog integration is not a local KiCad symbol CLI implementation. |

The KiCad manual includes an HPGL export entry marked nonfunctional for the documented 10.0 release. The matrix follows implemented behavior, not command-list presence. Recheck the manual and pinned behavior fixtures when the reference release changes.

## Recommended implementation order

1. **Ratify the reference release and fixtures.** Use KiCad 10.0.7 as the proposed documentation baseline; record the exact command output fixtures and source format dates used by each compatibility test. A newer or older release needs a separately reviewed matrix.
2. **Finish the physical-board foundation.** Keep the canonical `PhysicalBoardDocument` as the shared Rust API and preserve current v0.1 package/CLI behavior.
3. **Selectable GLB (Milestone C-06).** Resolve only explicit local model roots; verify model format, hash, units, origin, rotation, top/bottom transforms and unique footprint identity. Produce per-component nodes and a deterministic identity map. Default to refusing incomplete models; the existing explicit partial-export flag remains the only route to a visibly incomplete result.
4. **Manufacturing-position export.** Add a separately named operation only after fixtures prove front/back selection, placement origin, units, bottom-side mirroring and excluded-footprint behavior. Do not relabel the existing inventory CSV.
5. **SVG compatibility.** Extend layer, page, origin, DNP and plotting semantics one option at a time, keeping unsupported text/pad/zone cases visible.
6. **Job manifests and additional formats.** Add local batch jobs after individual outputs have schemas and failure behavior. Add Gerber/drill and other manufacturing operations only with independent fixtures and acceptance tests.
7. **Schematic and library families.** Treat `sch`, `fp`, and `sym` as separate engines/adapters. Do not infer PCB placement, board outline, stackup or routing from schematic symbols or a BOM.

## Project-folder intake contract to implement later

The current CLI accepts a board file or its current Tokito package; it does not yet open an arbitrary project folder. A future project intake should:

1. Accept a user-selected project directory or an explicit board file, and report the exact selected source path and digest before export.
2. Detect supported project files without an unbounded recursive scan. If multiple candidate boards exist, ask the Desktop/user to select one; never guess based only on filenames.
3. If the folder contains a schematic but no physical PCB layout or Tokito physical-board document, report that a placement/routing stage is still required. Do not synthesize physical geometry from schematic coordinates.
4. Resolve model references only from the project and explicitly approved local library roots. Resolve known project variables such as `${KIPRJMOD}` only under declared roots; reject traversal and ambiguous paths. Do not download missing models during an offline export.
5. Keep source files read-only and write to a new explicit output directory. Refuse overwrites unless a future, separately verified option explicitly permits them.
6. Include source hashes, engine version, selected options, coordinate conventions, supported/omitted object counts, resolved/missing model report, and output hashes in a versioned artifact manifest.

## Acceptance gate for a parity claim

A command may be labeled compatible only after its exact supported option set, defaults and error cases are documented; primary-source behavior is pinned; independent fixtures exercise positive and negative cases; units/origins/transforms are verified; generated artifacts pass format and geometry checks; and unsupported options fail closed. Runtime verification must remain independent of KiCad. Reference artifacts may be captured separately and reviewed, but the Tokito workflow must not shell out to KiCad.

## References

- [KiCad 10.0 CLI manual](https://docs.kicad.org/10.0/en/cli/cli.html) (manual text states it is based on KiCad 10.0.7).
- [KiCad PCB S-expression format](https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/).
- [Tokito CLI behavior research](KICAD_CLI_BEHAVIOR.md).
- [Tokito canonical engine](CANONICAL_ENGINE.md).
- [GLB export and component-model gates](GLB_EXPORT_ARCHITECTURE.md).
