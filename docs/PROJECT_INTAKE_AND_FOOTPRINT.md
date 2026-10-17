# Project intake and first footprint family

Status: bounded project intake and initial offline export bundle, 2026-10-11. The Rust `project build/verify` path is implemented as local unmerged work. These features do not make the CLI compatible with KiCad's full command set.

## Existing PCB project intake

`project inspect <DIR> [--board <EXACT_FILENAME.kicad_pcb>] [--expect-sha256 <64-HEX>] [--json]` lists only immediate regular `.kicad_pcb` and `.kicad_sch` files. It will not scan recursively, include symlink entries seen during discovery, or write into the project. Discovery is capped at 4,096 directory entries and 256 source candidates. Candidates are sorted by filename. The selection is reopened by path; a different process changing that directory concurrently can change what is read. Use an immutable/trusted project snapshot and `--expect-sha256` when pinning a source revision.

The sole PCB is selected automatically. More than one PCB requires an exact top-level filename, and a schematic-only project is refused with an explicit diagnostic. The selected PCB is parsed by the existing reviewed-format importer, and its actual parsed bytes supply the reported SHA-256 and format version. `--expect-sha256` rejects a different board revision. A schematic is inventoried only; its symbol positions cannot supply component placement, routing, stackup or 3D geometry. `project build` now packages the selected PCB together with derived inspection, placement, outline, SVG, GLB coverage and component-selector data; see [Project export bundle](PROJECT_EXPORT_BUNDLE.md). This is an offline CLI handoff, not an App-importable digital twin or AR registration result.

Example:

```powershell
tokito-cli project inspect C:\Boards\MyProject --board MyProject.kicad_pcb --json
```

## Two-terminal 0603 footprint candidate

`component footprint r0603 --output <NEW.kicad_mod> [--json]` serializes one deterministic `.kicad_mod` candidate. Its two round-rectangle pads have numbers 1/2, front copper/mask/paste layers, nominal centres at ±0.825 mm, and size 0.8 × 0.95 mm. Fab, silkscreen and courtyard geometry come from a typed Rust specification with explicit millimetres and validation. Output uses exclusive creation; an existing file is never replaced.

The comparison reference is the installed **KiCad 10.0.5** `Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod`, SHA-256 `03fc7902b2661df01b4d828fdb6eab9eddf974e7130ed2b98892607387a50c4b` on this workstation. Tests independently compare pad numbers, coordinates, sizes, layers, fab/courtyard bounds and silkscreen segments when an asset exists at that local path. Portable constant-based tests still run when KiCad is absent. On this workstation, the installed file hash was checked separately and KiCad 10.0.5 successfully plotted the CLI-generated `.kicad_mod` from a temporary `.pretty` library. This is a library geometry comparison, **not** a manufacturer-approved land pattern, verified component body, or accepted footprint for a particular target board. No 3D model is generated or silently attached.

KiCad's [10.0 CLI manual](https://docs.kicad.org/10.0/en/cli/cli.html) documents `fp export svg` and `fp upgrade`; neither is a footprint generator. This Rust family addresses the separate [KiCad Library Tools](COMPONENT_FORGE_OPEN_SOURCE_REVIEW.md) workflow. It is one family among many, and issue [#24](https://github.com/TokitoAI/tokito-cli/issues/24) remains open. The broader [CLI compatibility matrix](KICAD_CLI_PARITY_MATRIX.md) and [product plan](PRODUCT_COMPLETION_PLAN.md) remain the authoritative completion gates.

## Explicit plated and non-plated hole candidates

`component footprint generate examples/footprints/header-1x02-through-hole.json --output <NEW.kicad_mod> --json` exercises round plated through-hole pads; [`header-1x02-oval-offset.json`](../examples/footprints/header-1x02-oval-offset.json) exercises oval slots with pad-local offsets. Each plated pad declares `padType: "thru_hole"`, exactly one round `drillMm` or two-dimension `drillOvalMm`, optional `drillOffsetMm: [x,y]`, and ordered `*.Cu` / `*.Mask` layers. Validation rejects missing or conflicting drills, drills outside the supported pad boundary, drills on SMD pads, and mixed technologies.

`component footprint generate examples/footprints/mounting-hole-3mm-npth.json --output <NEW.kicad_mod> --json` exercises a non-plated round mounting hole. It uses `padType: "np_thru_hole"`, permits an empty pad number, and requires circular pad size to equal the round drill, with `*.Cu` / `*.Mask` layers. The footprint-level attribute remains `through_hole`; the pad itself is explicitly `np_thru_hole`. `component footprint library create` places either candidate in a new `.pretty` directory without registering or modifying a library table.

These examples are synthetic geometry for exercising the format path; neither is a real connector or mounting-hole specification. KiCad 10.0.7 readback previously accepted centered round plated and non-plated candidates; the installed KiCad 10.0.5 readback also accepted round-offset and oval-offset plated candidates. These checks establish file/plot acceptance only: they do not qualify dimensions, annular ring, plating, fabrication tolerances, manufacturer patterns, or electrical/physical fit. Mixed pad technologies, IPC-7351 derivation, KLC validation, 3D bodies, and board placement remain unsupported.

## Verification

The local verifier runs Rust formatting/tests/Clippy/release build plus actual `project inspect` and `component footprint` subprocess journeys. Project tests cover nonrecursive discovery, explicit selection, SHA mismatch, schematic-only refusal, malformed board and preserved input. Footprint tests cover the geometry comparison, deterministic SMD, plated round/oval/offset-hole and centered non-plated-hole serialization, drill/layer/technology validation, overwrite refusal and unsupported-family refusal. A separate, optional KiCad 10.0.7 SVG readback checks format acceptance; the shipping CLI never invokes KiCad.
