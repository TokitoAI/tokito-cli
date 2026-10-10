# KiCad-quality 3D qualification

The target is an accurate populated board, with board contours/cutouts, drilled
holes, component bodies, correct units/poses, materials, and deterministic
component identities. The current bounded VRML assembly does not satisfy the
complete target. Keep the Rust engine independent of KiCad at runtime.

## Target corpus

Use `TokitoAI/tokito-app-dataset` for target-board engineering assets. This audit
pins main at `257ea7e68ee2e700522bd1b19ad9cbf1c0c3980a`. The sibling checkout
matches that commit and remains read-only. Regression fixtures elsewhere test
parser behavior, not target-board qualification.

The [dataset CAD export report](https://github.com/TokitoAI/tokito-app-dataset/blob/257ea7e68ee2e700522bd1b19ad9cbf1c0c3980a/data/board-sources/CAD_EXPORTS.md)
records three candidate layouts: ESP32 community clone, Ultimainboard 2.1.4,
and FRDM-MCXN236 Rev C. Their generated STEP/GLB references are board-only.
Recorded DRC errors/warnings are 19/128, 293/569, and 1087/315 respectively;
these are conversion candidates requiring engineering review. The photographed
ESP8266 variant and Raspberry Pi 3B still lack an exact qualified layout here.
MCXN947 is a different board and cannot supply MCXN236 design truth.

## Actual ESP32 comparison, 2026-10-10

Input is the materialized native community layout under
`data/board-sources/esp32-devkit-30pin/community-clone/extracted/BrijModi-ESP32-Dev-Kit-Clone-ab7d649/ESP32 Dev Kit.kicad_pcb`.
Its SHA-256 is
`d5a70c26a4c48b4a6f5e697363593ca56da6ebe499a3dc8d0ebb115b8a8f345f`,
matching the generated-layout LFS pointer. This proves source byte identity,
not exact correspondence to the photographed specimen.

The tested Tokito release executable imports inventory successfully, but physical
outline validation returns exit 2 with six `OpenContour` diagnostics. The three
nearest-gap values, each appearing twice, are 0.001655551, 0.007042, and
0.009959051 mm. Tokito's fixed join tolerance is `1e-6 mm` (1 nm).

Installed KiCad **10.0.5** successfully exports that same input with both:

```text
kicad-cli pcb export glb --board-only --output default.glb INPUT
kicad-cli pcb export glb --board-only --min-distance 0.000001mm --output strict.glb INPUT
```

Both outputs parse as complete GLB v2 containers with one mesh. Aggregated local
POSITION accessor extents are 47.008041 × 1.51 × 29.1846 mm; this is not a
world-space scene evaluation or a full geometric equivalence test. Artifacts,
parsed bounds and hashes remain in ignored `target/dataset-intake/`.

This is a verified behavior difference. It does not prove that `--min-distance`
uses the same outline gate as Tokito, nor authorize silent source repair. Source
research and independent geometric comparison must explain the difference before
adding a compatibility policy. Dataset provenance cites 10.0.7, but this local
experiment is only 10.0.5 evidence.

Pinned KiCad 10.0.7 source declares a 0.01 mm default chaining threshold and
builds derived contours that can snap endpoints/rebuild arcs. Outline inference
may substitute a bounding rectangle after failure. A successful external export
therefore does not prove an exact authored contour. Preserve strict source
diagnostics; any future compatibility profile must record original/derived
geometry and each adjustment, and must never silently label inferred geometry
as physical truth. The two local reference GLBs also differ at an accessor
coordinate by 7.042 µm, even though their aggregated bounds match.
Behavior sources: [board outline construction](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/pcbnew/board.cpp),
[derived polygon chaining](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/pcbnew/convert_shape_list_to_polygon.cpp),
and [3D exporter](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/pcbnew/exporters/step/exporter_step.cpp).

## Pure-Rust STEP evaluation

The published `truck-stepio` 0.3.0 shell-import example contradicts its stale
overview text saying STEP input is future work. It is a geometry candidate, but
its released example does not prove dataset units/materials/assembly placement.
`oxideav-step` 0.0.1 is another candidate with units/occurrences/B-rep support;
its skip/default behavior must not be mistaken for complete CAD. See the
[format-adapter research](GLB_EXPORT_ARCHITECTURE.md#format-adapter-research-candidates-not-dependencies).
The new STEP probe is an isolated qualification experiment. Production STEP
admission remains unavailable until real dataset assets, face completeness,
units, transforms, resource limits and independent reference comparisons pass.

## Required quality gates

| Gate | Current evidence | Remaining acceptance |
| --- | --- | --- |
| Target source identity | Pinned dataset commit and ESP32 source hash | Materialize/hash every required LFS asset; match exact physical revisions |
| Outline/cutouts | Analytic Tokito gate and measured ESP32 refusal | Differential closure policy, contour/area/bounds comparisons and disclosed tolerances |
| Board body | Current sourced shell extrusion | Compare substrate/copper/mask thickness, holes/slots and layer geometry against references |
| Component bodies | Bounded flat-triangle VRML assembly | Real dataset STEP solids/faces, assembly transforms, units and materials; explicit unsupported entities |
| Component placement | Synthetic top/back, signed scale and stackup tests | Independent asymmetric mesh/landmark comparisons with nonzero model transforms |
| Selection | CLI exact-name simulation and UUID provenance | Real App importer/renderer journey on a device |
| Production dependency | Pure Rust; no KiCad runtime invocation | Preserve this boundary as CAD adapters are added |

Do not treat a STEP syntax parser as a solid tessellator, a valid GLB container as
correct CAD, or a board-only model as a populated assembly. Schematic-only input
still requires a separate authored physical layout, placement, routing and
stackup stage before physical 3D.

Track target qualification in [issue #18](https://github.com/TokitoAI/tokito-cli/issues/18),
mesh/pose comparison in [issue #15](https://github.com/TokitoAI/tokito-cli/issues/15),
and the current assembly foundation in [PR #17](https://github.com/TokitoAI/tokito-cli/pull/17).
