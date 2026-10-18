# KiCad CLI compatibility matrix

Status: planning and evidence map only. This document does not claim that Tokito is a KiCad CLI clone.

## Reference target and meaning of compatibility

The official [KiCad 10.0 CLI manual](https://docs.kicad.org/10.0/en/cli/cli.html) is based on KiCad 10.0.7 (documentation revision `efa5960e`) and documents six root command groups: `fp`, `jobset`, `pcb`, `sch`, `sym`, and `version`. KiCad 10.0.7 is the selected parity baseline. The machine has both a 10.0.5 reference installation at `C:\Program Files\KiCad\10.0\bin\kicad-cli.exe` and a pinned 10.0.7 executable at `D:\TokitoKiCadRef107\bin\kicad-cli.exe` (SHA-256 recorded in the 10.0.7 POS manifest). The 10.0.5 help captures for SVG, GLB, STEP, VRML, STL, BREP, POS, and footprint SVG are syntax reconnaissance; selected output evidence is recorded in [KICAD_10_0_5_FILTER_AUDIT.md](KICAD_10_0_5_FILTER_AUDIT.md), [KICAD_GLB_REFERENCE.md](KICAD_GLB_REFERENCE.md), and [the REA investigation](REA_KICAD_CLI_10_0_5.md). New 10.0.7 POS evidence now covers 14 cases across four pinned boards. One NXP DNP structure comparison also exists across both releases; neither establishes broad parity. KiCad 10.0.0 source archaeology links in [KICAD_CLI_BEHAVIOR.md](KICAD_CLI_BEHAVIOR.md) are design evidence, not proof of later-release behavior.

The requested end state is a Rust Tokito CLI with full behavioral compatibility for the selected KiCad CLI release and its documented command surface. This is the target, not the current state: the repository implements a small PCB-focused subset. Here, “exact” means matching documented user-visible commands and results for each declared option; it does not mean copying KiCad internals or source. Compatibility is earned per command and option only when Tokito reproduces observable behavior on pinned fixtures. Matching a command name does not establish matching file semantics, transforms, rendering, reports, error codes, or edge cases. Tokito remains an original pure-Rust implementation; the product workflow must not invoke KiCad CLI or require a KiCad installation.

This matrix distinguishes:

- **Syntax parity:** command path, options, defaults, validation and exit/result behavior.
- **Semantic parity:** the selected objects, coordinate frame, filters, units and transformations agree with the documented operation.
- **Artifact compatibility:** output is valid for a named consumer and has verified geometry and identity coverage. Byte-for-byte output equality is not assumed unless explicitly required.

Unsupported options must fail with a diagnostic. Silently accepting and ignoring an option is not parity.

2026-10-10 C-06 addition: Tokito now has a narrow command-help tree (`help`,
`--help`, and `-h` for implemented operations), offline exact-path asset
capture, a bounded VRML97 Group/Transform/DEF/USE triangle-mesh profile, stackup-derived planes, and a Rust GLB
assembly path for that supported subset. These are Tokito-native commands, not
KiCad syntax/semantic parity. Independent placement comparison and actual App
consumer acceptance remain open. Source-behavior research pins KiCad 10.0.7
commit `69cafb907ebc09e7722a2a464b1d466fc2e3608d`. POS CSV output now has a
separate 10.0.7 corpus; other broad exporter and model semantics remain
unqualified. See [model semantics and remaining gates](MODEL_RESOLUTION.md).

## Current command coverage

The CLI currently exposes Tokito-native command names; it does not implement the KiCad command tree. Evidence is in `src/cli.rs`, `src/lib.rs`, `docs/CANONICAL_ENGINE.md`, `docs/GLB_EXPORT_ARCHITECTURE.md`, and `docs/TOKITO_BOARD_PACKAGE_V0.md`.

| KiCad 10.0 family / operation | KiCad documented scope | Tokito CLI today | Compatibility status and next decision |
|---|---|---|---|
| Root help and `version` | Six command groups; scoped help; version formats | Tokito global help/version and contextual help for implemented commands; no KiCad version-format variants | Partial discoverability only; Tokito does not implement KiCad's six root groups. |
| `fp export svg`, `fp upgrade` | Footprint-library SVG and library conversion/upgrade | Explicit SMD, plated through-hole round/oval/offset, centered circular non-plated-hole, and mixed per-pad candidates; bounded line/rectangle/circle/simple-polygon graphics; new `.pretty` creation; bounded read-only inventory and deterministic content lock/verify for immediate `.kicad_mod` files | KiCad 10.0.7 format documentation/source pins the footprint attribute and mixed-pad inference rule. Local KiCad 10.0.5 readback accepts round-offset and oval-offset candidates and plots SOT-23 and mixed-pad fixtures; receipt: `docs/evidence/mixed-pad-kicad-10.0.5.json`. Readback proves file/plot acceptance only. The R0603 candidate's two F.Cu pad SVG paths match 10.0.5. This does not implement the KiCad `fp` command tree, IPC family generation, full footprint semantic/geometry inspection, safe library updates/registration, manufacturer qualification, KLC validation or broad library generation. |
| `jobset run` | Run selected or all project export jobs with stop-on-error behavior | None | Defer until individual outputs and their manifest are stable; later map a typed job manifest onto shared engine operations. |
| `pcb drc` | Electrical/manufacturing design-rule analysis and reports | `validate outline` checks a narrow physical-contour topology subset | Not equivalent. Outline validation is not DRC, connectivity checking, schematic parity, or manufacturability approval. |
| PCB 2D / manufacturing exports | Gerber, drill, DXF, GenCAD, IPC-2581, IPC-D-356, ODB++, PDF, PostScript, SVG, and related plot options | Partial SVG; opt-in KiCad-spelled DNP Fab text/graphics suppression with identity checks; frozen source-frame placement inventory; `pcb export pos --format csv` implements side, units, bottom-X mirror, absolute/auxiliary origin and tested SMD/THT/DNP filters | POS CSV subset matches the 14 captured KiCad 10.0.7 cases on Uno, NXP, SparkFun and Pico fixtures: 12 line-ending-normalized byte matches and two Pico cases with equivalent named-row order and empty-reference row values. SVG DNP suppression has a single-Uno fixture comparison in both 10.0.5 and 10.0.7; normalized native outputs match exactly for this `F.Fab` case. It does not prove general SVG parity. ASCII/Gerber POS, variants and other manufacturing formats remain unsupported. SVG omits full KiCad plot/font semantics. See [POS evidence](PCB_POSITION_EXPORT.md), [10.0.5 SVG evidence](../tests/reference/kicad-cli-10.0.5/svg/manifest.json), and [10.0.7 SVG evidence](../tests/reference/kicad-cli-10.0.7/svg/manifest.json). |
| PCB statistics | Board statistics and optional area calculations | Inspection inventory | Not equivalent; inventory fields do not claim KiCad statistics semantics. |
| `pcb export glb` and other 3D outputs | GLB and multiple CAD/mesh outputs with component/model filters, variants, origin, copper/graphics switches, and missing-model behavior | Validated shell GLB plus bounded `.wrl`/`.vrml` mesh assembly; exact/`*`/`?` reference selection and native `--no-dnp`/`--no-unspecified` filters are implemented for this Tokito path with source/selected/excluded coverage. Planar VRML polygon faces are triangulated after geometry checks. Offline source packaging now captures JSON glTF local buffer/image dependencies, but does not import them as geometry. | Narrow partial implementation, not 3D parity. One NXP DNP selection/structure delta matches installed KiCad 10.0.5 and 10.0.7; component-pattern probes are pinned to 10.0.5. STEP board/model export and filtering, glTF/GLB model import, board-VRML exporters, BREP, full VRML97, Unicode wildcard parity, variants, renderer equivalence and App admission remain open. See [REA evidence](REA_KICAD_CLI_10_0_5.md) and [filter audit](KICAD_10_0_5_FILTER_AUDIT.md). |
| `pcb import` | Convert supported non-KiCad PCB formats into KiCad PCB format | Read a supported subset of KiCad PCB into Tokito's canonical model | Opposite conversion direction and different scope. No non-KiCad PCB adapter is implemented. |
| `pcb render` | Camera-controlled rendered board image | None | Defer; a valid GLB mesh is not a renderer or a rendering-equivalence claim. |
| `pcb upgrade` | Update an older PCB file to the current KiCad format | None | Defer; preserve source files and do not rewrite user designs without a separately specified migration contract. |
| `sch erc`, schematic exports, `sch upgrade` | ERC; BOM, netlist and drawing exports; schematic upgrade | No KiCad schematic parser or operations | Defer until Desktop's current schematic/connectivity handoff is pinned. A schematic is not a physical board placement. |
| `sym export svg`, `sym upgrade` | Symbol-library SVG and conversion/upgrade | None | Defer as a separate symbol-library adapter. Tokito MCP/catalog integration is not a local KiCad symbol CLI implementation. |

The native footprint-type filter has [six captured KiCad 10.0.7 selection
comparisons](KICAD_FOOTPRINT_TYPE_FILTER.md), including native type/modifier
semantics, DNP overlap and component-pattern combinations. This is a synthetic
metadata fixture with real model assets, not whole-board geometry parity.

The KiCad manual includes an HPGL export entry marked nonfunctional for the documented 10.0 release. The matrix follows implemented behavior, not command-list presence. Recheck the manual and pinned behavior fixtures when the reference release changes.

The pinned KiCad 10.0.7 STEP and VRML board-export option surfaces, including
the important differences between the two commands and Tokito's current
VRML-model-to-GLB importer, are recorded in
[KICAD_10_0_7_3D_EXPORT_BEHAVIOR.md](KICAD_10_0_7_3D_EXPORT_BEHAVIOR.md).
Those captures are syntax/option evidence only; they are not 3D output parity.

The new Tokito-native `project inspect` command provides bounded, explicit PCB selection and source hashing. It does not implement KiCad project manager/jobset behavior, and it does not use schematics to invent a physical board. See [its exact scope and 0603 footprint candidate](PROJECT_INTAKE_AND_FOOTPRINT.md).

## Recommended implementation order

1. **Ratify the reference release and fixtures.** Use KiCad 10.0.7 as the proposed documentation baseline; record the exact command output fixtures and source format dates used by each compatibility test. A newer or older release needs a separately reviewed matrix.
2. **Finish the physical-board foundation.** Keep the canonical `PhysicalBoardDocument` as the shared Rust API and preserve current v0.1 package/CLI behavior.
3. **Selectable GLB (Milestone C-06).** Resolve only explicit local model roots; verify model format, hash, units, origin, rotation, top/bottom transforms and unique footprint identity. The bounded VRML exporter emits one renderable `ref:<refdes>` node per component with separate mesh-part primitives, and CLI tests exercise selecting one of multiple components. Independently compare transforms/geometry and verify the App importer before claiming consumer interoperability. Default to refusing incomplete models; the existing explicit partial-export flag remains the only route to a visibly incomplete result.
4. **Manufacturing-position export.** A separate CSV operation is implemented and compared against KiCad 10.0.5 and 10.0.7 for 14 shared cases covering side, units, bottom-X mirroring, absolute and auxiliary origins, SMD/THT/DNP filters and excluded-footprint behavior. Next add independent error/default coverage and extend other formats one verified option at a time. Do not relabel the existing inventory CSV.
5. **SVG compatibility.** The DNP Fab-hide option has a pinned single-Uno F.Fab comparison in both 10.0.5 and 10.0.7. Extend layer, page, origin and plotting behavior one option at a time, keeping unsupported text/pad/zone cases visible; the completed narrow capture does not establish general plotting parity.
6. **Job manifests and additional formats.** Add local batch jobs after individual outputs have schemas and failure behavior. Add Gerber/drill and other manufacturing operations only with independent fixtures and acceptance tests.
7. **Schematic and library families.** Treat `sch`, `fp`, and `sym` as separate engines/adapters. Do not infer PCB placement, board outline, stackup or routing from schematic symbols or a BOM.

## Implemented bounded project-folder intake and remaining scope

The CLI implements `project inspect`, `project build` and `project verify` for
bounded top-level discovery and source-bound offline bundles. Tests exercise
source selection/hashes, ambiguity, unsafe paths, unchanged inputs, partial
consent, no overwrite and bundle tamper detection. The supported folder scope
is explicit KiCad PCB discovery; this is not arbitrary project-manager,
jobset, recursive multi-board or schematic-to-layout parity. Its contract is:

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
