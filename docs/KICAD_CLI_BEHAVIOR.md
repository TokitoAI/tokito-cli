# KiCad CLI behavior analysis

Status: behavior research and independent partial implementation. On 2026-10-10, the locally installed KiCad CLI reports 10.0.5, while the proposed reference baseline is 10.0.7. Its GLB command was exercised once against a pinned asymmetric Arduino Uno fixture with a resolved QFN model; the captured named component node is documented in [KiCad GLB reference capture](KICAD_GLB_REFERENCE.md). This is a version-specific artifact check, not behavioral or 10.0.7 parity. No KiCad implementation code has been copied or translated. Current geometry/package behavior and exact transform evidence are recorded in [native architecture](NATIVE_FORMAT_ARCHITECTURE.md); full CLI/CAD feature parity is not claimed. See the [command-by-command compatibility matrix](KICAD_CLI_PARITY_MATRIX.md) for the current Tokito surface, target baseline proposal, and implementation order.

## Observed command model

The official [KiCad 10.0 CLI manual](https://docs.kicad.org/10.0/en/cli/cli.html) lists exactly six top-level groups: `fp`, `jobset`, `pcb`, `sch`, `sym`, and `version`. The manual is based on KiCad 10.0.7. The command implementation references below pin source snapshots at 10.0.0, so they are architectural observations rather than a version-matched proof of 10.0.7 behavior. A PCB export command is a front-end for a typed operation: it parses a command-specific option set, fills an export-job description, and dispatches that job into KiCad's shared application processing path. This keeps file loading and export work out of the argument parser.

The useful design lesson for Tokito is to keep the terminal interface thin. Parse a typed request, validate it, invoke one shared board-engine operation, and return a structured result. Desktop, CLI, and a future local MCP adapter should call that same engine rather than each reimplementing the format logic.

## PCB operations and semantics

The documented PCB interface covers more than one generic export:

| Operation | Observable behavior relevant to Tokito |
|---|---|
| Position export | Select front/back/both; choose ASCII, CSV, or Gerber; select units and placement origin; control bottom-side X mirroring and exclusions. A placement file is a manufacturing/export view, so Tokito should separately preserve raw board coordinates and clearly name any transformed convention. |
| SVG export | Select layers and common layers; choose one-file or multi-file output; mirror, theme, scale, page fit, drawing sheet, zone checks, and DNP presentation can change output. SVG therefore needs an explicit layer list and coordinate frame in its job record. |
| GLB and mesh export | Generate a 3D representation with switches for board body, components, variants, model substitution, origin, tracks, pads, zones, silkscreen and soldermask. Component filters and missing model handling affect coverage. |
| STEP export | Shares many 3D inclusion/origin options, with STEP-specific optimization and point-merging tolerance. This is a CAD solid export, with a different completeness bar from a lightweight mobile mesh. |
| Other outputs | Drill, Gerber, PDF, DXF, IPC-2581, ODB++, BOM/netlist and statistics are distinct operations with different output shapes and validation needs. They are not initial Tokito scope. |

KiCad 10.0 documents overwrite behavior, variants, coordinate origins, units, and filtering as explicit command options. Tokito should likewise make destructive overwrite opt-in, record option values in a job/result manifest, and never silently change the board origin or component selection.

The official 10.0 manual marks `pcb export hpgl` as nonfunctional. This is a useful reminder that command presence does not prove capability. Tokito must report only operations that are actually implemented and verified.

## Independent Tokito architecture

1. Bounded format reader: accept the reviewed KiCad PCB format-date range `20211014` through `20260206`; reject missing, pre-v6, and newer versions until their structure is reviewed. Read at most 256 MiB from disk, and apply token, node-count, and nesting limits. The byte ceiling does not bound total parser memory.
2. Canonical physical-board model: retain source identity, units, axes/handedness, outline and cutouts, component/pad identities, layers, nets, source locations, model transforms, and import diagnostics.
3. Capability-checked operation: validate required geometry/model coverage before selecting SVG, placement, GLB, or STEP output.
4. Deterministic artifact writer: stage outputs, validate them, then publish atomically; include source digest, engine version, option values, artifact digests, and completeness findings.
5. Adapters: expose the same typed operations to the CLI first, then Desktop and local MCP as separate front ends.

The import reader must distinguish “syntax parsed” from “geometry understood.” Unknown syntax may be preserved for provenance, but it cannot silently count as converted geometry. Each export should say which board objects were represented, omitted, approximated, or unsupported.

## Feasibility and capability gates

- Placement export is a good first operation because component anchors and identities are explicit in a PCB file, but front/back coordinate conventions still require fixture-based verification.
- `export placements` remains the raw inventory: it preserves each footprint's source-frame anchor and reports `exclude_from_pos_files`. A separate typed `pcb export pos --format csv` operation now matches the tested side/unit/bottom-X, absolute and auxiliary origins, and placement-class filter subset against KiCad 10.0.5 fixtures. Variants, ASCII/Gerber output and the rest of KiCad's POS option surface remain unsupported; see [the exact contract and reference snapshots](PCB_POSITION_EXPORT.md).
- Partial SVG is implemented for supported outlines/graphics/pads/tracks/vias/stored zone fills, with tested layer/side selection, exact footprint transforms and source identity metadata. The opt-in `--hide-DNP-footprints-on-fab-layers` option suppresses only UUID-owned DNP footprint text and graphics on `F.Fab`/`B.Fab`; it leaves pads, non-DNP footprints and other layers unchanged, and refuses missing, malformed, repeated or cross-footprint-ambiguous DNP UUID identity fields. A pinned Arduino Uno comparison against KiCad 10.0.5 captures the two `1M` Fab labels with the default and none with the option. This is one option-level behavior, not full KiCad SVG output, font/layout/plot semantics, complex-pad support, or 10.0.7 parity; export completeness stays partial. See [the SVG reference manifest](../tests/reference/kicad-cli-10.0.5/svg/manifest.json).
- GLB can be a later mesh capability using tessellated board and sourced component models. A generated board-only mesh must not imply component-level twin coverage.
- STEP is a separate research gate. A pure-Rust library candidate exists, but previous source review found cases where conversion can omit geometry while still returning success, and STEP input unit normalization needs proof. Do not advertise STEP until completeness, units, assemblies, and unsupported entities are checked against real fixtures.

## Primary references

- [KiCad 10.0 CLI manual](https://docs.kicad.org/10.0/en/cli/cli.html)
- [Official PCB S-expression file format](https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/)
- [KiCad 10.0.7 reviewed PCB format version](https://gitlab.com/kicad/code/kicad/-/blob/10.0.7/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr.h)
- [Official S-expression syntax](https://dev-docs.kicad.org/en/file-formats/sexpr-intro/)
- [KiCad CLI command registration source, 10.0.0](https://gitlab.com/kicad/code/kicad/-/blob/10.0.0/kicad/kicad_cli.cpp)
- [KiCad 3D export command source, 10.0.0](https://gitlab.com/kicad/code/kicad/-/blob/10.0.0/kicad/cli/command_pcb_export_3d.cpp)
- [KiCad SVG export command source, 10.0.0](https://gitlab.com/kicad/code/kicad/-/blob/10.0.0/kicad/cli/command_pcb_export_svg.cpp)
- [KiCad placement export command source, 10.0.0](https://gitlab.com/kicad/code/kicad/-/blob/10.0.0/kicad/cli/command_pcb_export_pos.cpp)

KiCad source is GPL-3.0-or-later. The sources above were consulted only to understand externally visible command behavior and option semantics. No source text or implementation is included in this project.
