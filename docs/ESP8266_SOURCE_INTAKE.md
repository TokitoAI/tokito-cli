# ESP8266 reference intake and geometry evidence

Inspected dataset main `257ea7e68ee2e700522bd1b19ad9cbf1c0c3980a` and PR6 at `5283bc059e30e51f4d3a81a674425936fc213214`, then its updated heads `3e2fec0c2274ea9d9e179bfe70bbb99d3767ba81` and `1f7beddac3f24ba6c447e9adbaa52364e6de751b`. PR7 remains `ae27af81ccaf3a21a4127341a2244bfd48c2503c`; PR4 remains `ca65648cdc3c10f7f02fbd7710ccba158e8c39b9`. The PR6 updates add Ultimainboard/ESP32 candidates, delivery documentation and NXP revision/video evidence, with no changed ESP8266 paths. This audit does not certify unpublished updates. Pins remain separate; staged source binding below is explicitly against the earlier immutable commit.

The PR6 subtree `data/board-sources/esp8266-nodemcu-reference/` is materialized into the CLI-owned candidate `target/dataset-intake/esp8266-pr6-5283bc0-source-candidate/`. Preparation used explicit pinned Git/LFS reads; shipping `dataset bind` independently checked source objects against every staged payload. No sibling checkout was modified.

| Evidence | Actual outcome |
|---|---|
| Source materialization | 130 files; 31,731,305 bytes; 44 LFS payloads verified |
| Pinned source binding | Passed; tree 52b1b69abcb86e4821be1490d8dbcc4a6fa8f56f; binding SHA-256 a24b43d649da81ddf80e187e2753b5680ec104aa38ac94554d7c7719f0ee4aba |
| Design inventory | 34 components, 41 nets, seven Altium schematics, PCB/offset sources and Gerber archives/extractions |
| Models | 17 materialized STEP streams; all hashes match pinned LFS and extraction metadata |
| Rust experimental decode | All 17 produce meshes; zero reported warnings/nonfinite positions/provenance count mismatches; no production admission |
| Reader unit observations | 10 mm, 1 cm and 6 inch models by reported scale; conversion/context correctness still needs independent qualification |
| Independent FreeCAD 1.1.4 import | All 17 shapes report valid; this does not prove Rust surface/tessellation equivalence |
| Bounds-method issue | 11.step's naive B-rep/control-point envelope differs from mesh/vertex bounds by 4.8682646 mm. Source has identity root placement and 160 face IDs; extreme bounds match B-spline control poles. No missing-geometry defect is established by that mismatch |
| Independently evaluated reference mesh | FreeCAD tessellated all 17 at 0.001 mm reference deflection: 24,275,608 triangles. Normalized bounds match numerically for all 14 single-part profiles, including 11.step. Models 0, 14 and 16 require assembly-frame comparison. Frame equivalence and surface fidelity remain unqualified; bounds alone are insufficient |
| Assembly/crosswalk | No ESP8266 assembly GLB, .kicad_pcb or model/component transform map found in inspected tree |
| Physical revision | Unverified: photographed wider CH340-style clone differs from reference USB-UART variant |

Ignored local evidence: `target/github-delivery/esp8266-pr6-materialization-receipt.json`, source request/binding JSON, and `esp8266-step-preflight/` Rust/FreeCAD reports. Source records use mil placements and native Altium UNIQUEID strings, not KiCad footprint UUIDs. Do not transfer refdes/poses between the photographed clone and reference design without an exact crosswalk.

The experimental reader exits 2 for every model because units, face completeness and assembly pose are not independently admitted. Mesh output and a valid external B-rep are observations, not proof that a complete model/board can ship. Follow [completion gates](KICAD_COMPLETION_GATES.md): reconcile unit contexts and native model/body associations, independently compare evaluated geometry, recover contour/pad/placement data, then export individually selectable entities with explicit identities and omission diagnostics. Production STEP/native Altium adapters and 4K renderer acceptance remain open.

## Reproduced evaluated-reference comparison (2026-10-11)

`scripts/evaluate-step-reference.py` is off-device FreeCAD/OCCT QA tooling, not
a CLI runtime dependency. It imports materialized source STEP files, checks
B-rep validity, evaluates triangle vertices at an explicit deflection, and
records evaluated bounds separately from `Shape.BoundBox`. Each row binds the
source bytes and evaluator version. It refuses an existing output, invalid
geometry, nonfinite coordinates, and changed input bytes. The native evaluator
allocation occurs before the post-tessellation mesh budget check; run it as an
isolated, time-bounded QA process, not in the application hot path.

A fresh 0.01 mm FreeCAD evaluation and the existing Rust candidate-mesh journey
compared all 17 actual dataset files. All 17 normalized bounds differences were
zero, including `11.step`. This replaces the old control-pole-envelope mismatch
as the current evaluated-bounds observation; it does not certify corresponding
surfaces, trim fidelity, Altium body transforms or the assembled board. All
geometry/placement admission flags remain false.

Local evidence is under `D:/TokitoCliBuilds/model-integration/evidence/`:
`freecad-evaluated-reference.json`, `step-candidates/receipt.json`, and
`associations/receipt.json`. The source body journey also reproduced all 34
component identities and 62 body associations. Thirty header body instances
are bottom-side candidates; do not apply top-side transforms to them.
