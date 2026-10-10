# Tokito CLI

Tokito CLI is a new Rust project for importing existing PCB layouts and preparing board artifacts for the Tokito ecosystem. Its first supported input is a declared subset of KiCad `.kicad_pcb` files whose format date is between `20211014` (KiCad 6) and `20260206` (the version reviewed in KiCad 10.0.7). Older, newer, or missing format versions are rejected until their structure is reviewed. Schematic-to-PCB design comes later.

The canonical local workspace is `F:\kannan\projects\tokito ecosystem\tokito_cli`. The duplicate space-named checkout was removed after verifying its two documents were unchanged at the same commit and already superseded here. Sibling repositories remain read-only.

The [App/API native bridge audit](docs/APP_API_BRIDGE_AUDIT.md) pins the updated GitHub sources, distinguishes physical-session sync from board-package import, and records the remaining cloud/mobile contracts. [Team handoff](docs/TEAM_HANDOFF.md) lists what the App and dataset owners need to provide.

All boards in the dataset are intended targets, with support tracked per exact
revision and adapter. The [Ultimainboard sample audit and dataset intake](docs/DATASET_BOARD_HANDOFF.md)
record the pinned metadata checks, mixed STEP units, unverified model assignments
and remaining offline/registration requirements. Dataset availability does not
mean native Altium/STEP import or full-board AR is implemented.

## Current status

Milestones A and B add a reusable `tokito_cli` Rust library with a typed `PhysicalBoardDocument` and a physical `OutlineReport`. Milestone C adds an explicitly partial board-shell GLB, offline model asset resolution, a bounded flat VRML97 mesh adapter, and pure-Rust point and parsed-mesh placement helpers based on pinned KiCad 10.0.7 STEP behavior. The helpers require explicit outer-copper planes, normalize placed vertices to metres, validate triangle geometry, recompute normals, and correct winding for frame and signed-scale parity. They are not yet wired into GLB scene assembly; stackup-plane extraction, component nodes/selectors, and complete component coverage remain open. Inspection, placement, package, SVG and GLB share source identities; CLI dispatch stays separate. See the [public API](docs/CANONICAL_ENGINE.md), [topology limits](docs/PCB_TOPOLOGY.md), [model operations](docs/MODEL_RESOLUTION.md), and [GLB export architecture](docs/GLB_EXPORT_ARCHITECTURE.md).

The executable reads bounded KiCad PCB S-expressions, reports source/component inventory, exports placement CSV, and exports partial physical-scale SVG with layer selection and top/bottom views. `validate outline` provides a separate JSON or text physical topology report; unsupported, open, crossed, overlapping or ambiguous outlines are rejected for physical SVG and GLB export while their source inventory remains inspectable. `export glb` triangulates and extrudes a validated board shell, then independently validates the self-contained GLB container. Missing component geometry is always unresolved in this first slice; the command refuses by default and requires the explicit `--allow-partial` flag. Its JSON report identifies unresolved components and source model references. Geometry includes supported footprint graphics/text, simple pads/drills, segments/arcs, vias, and stored filled-zone polygons. SVG retains source IDs and parent footprint UUIDs. Text uses approximate SVG fonts; full KiCad plot fidelity and DRC are unverified.

`package build` preserves the source and inventory with SHA-256 digests. `package verify` validates the v0.1 contract, artifact/source integrity and source consistency; inspection, placement, SVG, and GLB exports can also consume that Tokito package directory. It remains a source-and-inventory package with no generated SVG/GLB/STEP embedded and is not accepted by the phone. Placement CSV retains excluded and placeholder footprints and uses raw source anchors; it is not a manufacturing-position file. Tokito is an original Rust implementation and does not invoke KiCad or copy/translate its code.

The pinned corpus includes SparkFun Basic, Pico/UNO community conversions, NXP MCXA ESC, the Raspberry Pi Press RP2040 Game Controller, an official KiCad NFC demo, the routed Porcupine board, and unchanged original LumenPnP vacuum-adapter PCB/stored profile. The RP2040 source and same-commit KiCad 7 Gerber profile match all 644 valid outline lines exactly; the NFC profile remains a 22-entity reference for an intentionally open outline that the validator rejects. Porcupine has five cutouts and 607 routed copper arcs. The original LumenPnP reference validates eight curved/circular outline entities and emitted SVG path/circle geometry; renderer fidelity remains unverified. These fixtures are not certified Tokito product boards. See [fixture provenance](docs/PCB_FIXTURE_CORPUS.md).

The CLI's v0.1 Tokito package schema and source/inventory writer are implemented. This is an initial exchange format, not the complete canonical board package or a phone import contract. The App remote snapshot inspected on 2026-10-10 defines native GLB/context validation and reference-based selection, but its mobile bridge has no board-package import operation. See [the package contract and limits](docs/TOKITO_BOARD_PACKAGE_V0.md) and the [pinned ecosystem contract map](docs/ECOSYSTEM_CONTRACT_MAP.md). Other vendor formats remain separate, explicitly scoped adapters.

## Research

- [KiCad CLI behavior analysis](docs/KICAD_CLI_BEHAVIOR.md)
- [KiCad CLI compatibility matrix](docs/KICAD_CLI_PARITY_MATRIX.md) (10.0.7 is proposed as the behavior baseline pending confirmation)
- [Tokito ecosystem integration boundaries](docs/ECOSYSTEM_INTEGRATION.md)
- [Tokito ecosystem contract map](docs/ECOSYSTEM_CONTRACT_MAP.md)
- [Implementation gates](docs/IMPLEMENTATION_GATES.md)
- [Native formats and 3D architecture](docs/NATIVE_FORMAT_ARCHITECTURE.md)
- [Canonical Rust engine API](docs/CANONICAL_ENGINE.md)
- [Current results and personal-AI handoff](docs/PERSONAL_AI_HANDOFF.md)

Build locally with `cargo check --offline --locked` after dependencies are cached. The pure-Rust dependency graph includes pinned `earcut` for contour triangulation and `gltf` for independent GLB parsing, plus pinned `serde` and `serde_json`; it has no native CAD dependency. Run `cargo test --offline --locked`, `cargo fmt --all --check`, and `cargo clippy --offline --locked --all-targets -- -D warnings`.

Verification and binary delivery stay local to conserve the GitHub Actions quota. The repository's workflow file was removed before PR creation; the PowerShell 7 runner performs the full local gates, release build, E2E and historical compatibility checks. Binaries and receipts are saved under ignored `target/delivery/local/`. See [local delivery](docs/CI_AND_DELIVERY.md).

```text
tokito-cli inspect board.kicad_pcb --json
tokito-cli resolve models board.kicad_pcb --project-root "F:\boards\project" --library-root "F:\models\KiCad8" --model-var "KICAD8_3DMODEL_DIR=F:\models\KiCad8" --json
tokito-cli model inspect resistor.wrl --units kicad-wrl --json
tokito-cli validate outline board.kicad_pcb --json
tokito-cli export svg board.kicad_pcb --side top --output top.svg
tokito-cli export svg board.kicad_pcb --side bottom --output bottom.svg
tokito-cli export glb board.kicad_pcb --output board-shell.glb --allow-partial --json-report
tokito-cli package build board.kicad_pcb --output new-package
tokito-cli package verify new-package --json
tokito-cli inspect new-package --json
tokito-cli export svg new-package --side top --output package-top.svg
tokito-cli export glb new-package --output package-shell.glb --allow-partial --json-report
```

Desktop-native schematic JSON, project archives and symbol libraries are distinct document kinds whose adapters remain future work; accepting a Tokito board package does not imply they already produce physical PCBs.

The input-byte ceiling limits bytes read from disk; parser tokens and the in-memory syntax tree can consume more memory than the source file size.

## Constraints

- Pure Rust dependency graph; no C/C++ bindings or native CAD runtime.
- Existing board layouts first; schematic-to-PCB later.
- Offline package generation and local phone use must not depend on sign-in or network.
- Unsupported geometry or missing models must produce explicit diagnostics; partial data must not be labeled a faithful digital twin.
- KiCad format documentation and behavior can guide compatibility. KiCad source code remains GPL-licensed and is not copied or translated here.
