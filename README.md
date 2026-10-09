# Tokito CLI

Tokito CLI is a new Rust project for importing existing PCB layouts and preparing board artifacts for the Tokito ecosystem. Its first supported input is a declared subset of KiCad `.kicad_pcb` files whose format date is between `20211014` (KiCad 6) and `20260206` (the version reviewed in KiCad 10.0.7). Older, newer, or missing format versions are rejected until their structure is reviewed. Schematic-to-PCB design comes later.

The canonical local workspace is `F:\kannan\projects\tokito ecosystem\tokito_cli`. The duplicate space-named checkout was removed after verifying its two documents were unchanged at the same commit and already superseded here. Sibling repositories remain read-only.

## Current status

Milestones A and B add a reusable `tokito_cli` Rust library with a typed `PhysicalBoardDocument` and a physical `OutlineReport`. Inspection, placement, package and partial SVG operations share source identities; CLI dispatch stays separate. The outline API classifies contours/cutouts, retains source locators and refuses unsupported or invalid physical boundaries. Milestone B passed local verification; see [the public API](docs/CANONICAL_ENGINE.md) and [topology limits](docs/PCB_TOPOLOGY.md).

The executable reads bounded KiCad PCB S-expressions, reports source/component inventory, exports placement CSV, and exports partial physical-scale SVG with layer selection and top/bottom views. `validate outline` provides a separate JSON or text physical topology report; unsupported, open, crossed, overlapping or ambiguous outlines are rejected for physical SVG export while their source inventory remains inspectable. Geometry includes supported footprint graphics/text, simple pads/drills, segments/arcs, vias, and stored filled-zone polygons. SVG retains source IDs and parent footprint UUIDs. Text uses approximate SVG fonts; full KiCad plot fidelity and DRC are unverified.

`package build` preserves the source and inventory with SHA-256 digests. `package verify` validates the v0.1 contract, artifact/source integrity and source consistency; inspection, placement and SVG exports can also consume that Tokito package directory. It remains a source-and-inventory package with no SVG/GLB/STEP embedded and is not accepted by the phone. Placement CSV retains excluded and placeholder footprints and uses raw source anchors; it is not a manufacturing-position file. Tokito is an original Rust implementation and does not invoke KiCad or copy/translate its code.

The pinned corpus includes SparkFun Basic, Pico/UNO community conversions, NXP MCXA ESC, an official KiCad NFC demo and the routed Porcupine board. The latter has five cutouts and 607 routed copper arcs; a pinned Gerber profile provides an independent 22-entity reference for the separate NFC geometry report, whose real outline gaps remain rejected. These fixtures are not certified Tokito product boards. See [fixture provenance](docs/PCB_FIXTURE_CORPUS.md).

The CLI's v0.1 Tokito package schema and source/inventory writer are implemented. This is an initial exchange format, not the complete canonical board package or a phone import contract. The mobile app separately validates a GLB manifest and board-context revision; its current runtime does not accept this CLI directory. See [the package contract and limits](docs/TOKITO_BOARD_PACKAGE_V0.md). Other vendor formats remain separate, explicitly scoped adapters.

## Research

- [KiCad CLI behavior analysis](docs/KICAD_CLI_BEHAVIOR.md)
- [Tokito ecosystem integration boundaries](docs/ECOSYSTEM_INTEGRATION.md)
- [Implementation gates](docs/IMPLEMENTATION_GATES.md)
- [Native formats and 3D architecture](docs/NATIVE_FORMAT_ARCHITECTURE.md)
- [Canonical Rust engine API](docs/CANONICAL_ENGINE.md)
- [Current results and personal-AI handoff](docs/PERSONAL_AI_HANDOFF.md)

Build locally with `cargo check --offline --locked` after dependencies are cached. The dependency graph uses Rust crates: pinned `serde` and `serde_json` for strict package JSON plus their locked Rust dependencies. The shipped engine has no native CAD dependency. Run `cargo test --offline --locked`, `cargo fmt --all --check`, and `cargo clippy --offline --locked --all-targets -- -D warnings`.

Verification and binary delivery stay local to conserve the GitHub Actions quota. The repository's workflow file was removed before PR creation; the PowerShell 7 runner performs the full local gates, release build, E2E and historical compatibility checks. Binaries and receipts are saved under ignored `target/delivery/local/`. See [local delivery](docs/CI_AND_DELIVERY.md).

```text
tokito-cli inspect board.kicad_pcb --json
tokito-cli validate outline board.kicad_pcb --json
tokito-cli export svg board.kicad_pcb --side top --output top.svg
tokito-cli export svg board.kicad_pcb --side bottom --output bottom.svg
tokito-cli package build board.kicad_pcb --output new-package
tokito-cli package verify new-package --json
tokito-cli inspect new-package --json
tokito-cli export svg new-package --side top --output package-top.svg
```

Desktop-native schematic JSON, project archives and symbol libraries are distinct document kinds whose adapters remain future work; accepting a Tokito board package does not imply they already produce physical PCBs.

The input-byte ceiling limits bytes read from disk; parser tokens and the in-memory syntax tree can consume more memory than the source file size.

## Constraints

- Pure Rust dependency graph; no C/C++ bindings or native CAD runtime.
- Existing board layouts first; schematic-to-PCB later.
- Offline package generation and local phone use must not depend on sign-in or network.
- Unsupported geometry or missing models must produce explicit diagnostics; partial data must not be labeled a faithful digital twin.
- KiCad format documentation and behavior can guide compatibility. KiCad source code remains GPL-licensed and is not copied or translated here.
