# Project intake and first footprint family

Status: first bounded product slices, 2026-10-11. Both operations are original Rust and work offline. They do not make the CLI compatible with KiCad's full command set.

## Existing PCB project intake

`project inspect <DIR> [--board <EXACT_FILENAME.kicad_pcb>] [--expect-sha256 <64-HEX>] [--json]` lists only immediate regular `.kicad_pcb` and `.kicad_sch` files. It will not scan recursively, include symlink entries seen during discovery, or write into the project. Discovery is capped at 4,096 directory entries and 256 source candidates. Candidates are sorted by filename. The selection is reopened by path; a different process changing that directory concurrently can change what is read. Use an immutable/trusted project snapshot and `--expect-sha256` when pinning a source revision.

The sole PCB is selected automatically. More than one PCB requires an exact top-level filename, and a schematic-only project is refused with an explicit diagnostic. The selected PCB is parsed by the existing reviewed-format importer, and its actual parsed bytes supply the reported SHA-256 and format version. `--expect-sha256` rejects a different board revision. A schematic is inventoried only; its symbol positions cannot supply component placement, routing, stackup or 3D geometry. This command prepares a trustworthy handoff for later project export; it does not yet generate a batch of output artifacts.

Example:

```powershell
tokito-cli project inspect C:\Boards\MyProject --board MyProject.kicad_pcb --json
```

## Two-terminal 0603 footprint candidate

`component footprint r0603 --output <NEW.kicad_mod> [--json]` serializes one deterministic `.kicad_mod` candidate. Its two round-rectangle pads have numbers 1/2, front copper/mask/paste layers, nominal centres at ±0.825 mm, and size 0.8 × 0.95 mm. Fab, silkscreen and courtyard geometry come from a typed Rust specification with explicit millimetres and validation. Output uses exclusive creation; an existing file is never replaced.

The comparison reference is the installed **KiCad 10.0.5** `Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod`, SHA-256 `03fc7902b2661df01b4d828fdb6eab9eddf974e7130ed2b98892607387a50c4b` on this workstation. Tests independently compare pad numbers, coordinates, sizes, layers, fab/courtyard bounds and silkscreen segments when an asset exists at that local path. Portable constant-based tests still run when KiCad is absent. On this workstation, the installed file hash was checked separately and KiCad 10.0.5 successfully plotted the CLI-generated `.kicad_mod` from a temporary `.pretty` library. This is a library geometry comparison, **not** a manufacturer-approved land pattern, verified component body, or accepted footprint for a particular target board. No 3D model is generated or silently attached.

KiCad's [10.0 CLI manual](https://docs.kicad.org/10.0/en/cli/cli.html) documents `fp export svg` and `fp upgrade`; neither is a footprint generator. This Rust family addresses the separate [KiCad Library Tools](COMPONENT_FORGE_OPEN_SOURCE_REVIEW.md) workflow. It is one family among many, and issue [#24](https://github.com/TokitoAI/tokito-cli/issues/24) remains open. The broader [CLI compatibility matrix](KICAD_CLI_PARITY_MATRIX.md) and [product plan](PRODUCT_COMPLETION_PLAN.md) remain the authoritative completion gates.

## Verification

The local verifier runs Rust formatting/tests/Clippy/release build plus actual `project inspect` and `component footprint` subprocess journeys. Project tests cover nonrecursive discovery, explicit selection, SHA mismatch, schematic-only refusal, malformed board and preserved input. Footprint tests cover the geometry comparison, deterministic serialization, invalid parameters, overwrite refusal and unsupported-family refusal. An optional separate KiCad SVG readback can check format acceptance on a machine with KiCad, but the shipping CLI never invokes KiCad.
