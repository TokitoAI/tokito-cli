# Tokito CLI — current state and personal-AI handoff

Date: 2026-10-09. This is a code- and test-backed status report. It does not claim that the Tokito ecosystem, KiCad compatibility, 3D digital twin, Desktop handoff, or phone AR flow is complete.

## Workspace, repositories, and ownership

The ecosystem root is F:\kannan\projects\tokito ecosystem. The active CLI project is tokito_cli. The duplicate CLI folder was consolidated with the user's authorization. The old F:\kannan\projects\Tokito path is gone. This task changed only the CLI repository. Other Tokito repositories, archived checkouts, Tokito physical old, and devboard_ar_dataset remain read-only and were not edited.

The new private repository is TokitoAI/tokito-cli (repository ID 1412077895). The user authorized the first code delivery through a feature branch and PR. The main branch is only an empty PR base; no CLI source is pushed directly to main. The local checkout retains its earlier history and working files; the remote feature branch presents the current verified project snapshot. The old repository's PR #9, issues, checks, branches, and Git bundle remain historical recovery evidence under target/recovery/github-before-recreation-20261009T150910Z. They are not live objects in the recreated repository.

GitHub collaborator access was checked on the replacement repository: NAVANEETHVVINOD has Admin access. No new invitation was sent. A dedicated private project, Tokito CLI - PCB Engine Roadmap, is at https://github.com/orgs/TokitoAI/projects/2. Roadmap issues are:
- Milestone B outline and v0.1 compatibility: https://github.com/TokitoAI/tokito-cli/issues/1
- Milestone C selectable GLB: https://github.com/TokitoAI/tokito-cli/issues/2
- Milestone D native Tokito physical-board package: https://github.com/TokitoAI/tokito-cli/issues/3
- Milestone E Desktop/MCP integration: https://github.com/TokitoAI/tokito-cli/issues/4
- Milestone F tokito-app offline import and AR: https://github.com/TokitoAI/tokito-cli/issues/5
- Milestone G pure-Rust STEP feasibility: https://github.com/TokitoAI/tokito-cli/issues/6
- Schematic-to-physical-layout handoff: https://github.com/TokitoAI/tokito-cli/issues/7

No GitHub Actions workflow is included, by explicit owner request to conserve the free-plan quota. Hosted checks and hosted binary uploads are not claimed. The repository includes a local verification runner and keeps its output under the ignored target directory.

## Current implementation

Tokito CLI is an original, pure-Rust, offline-first PCB import and geometry engine. It does not invoke KiCad CLI at runtime and does not translate KiCad C++ code. KiCad documented formats and observed behavior guided compatibility research.

The implemented input is a bounded subset of KiCad PCB S-expressions plus the CLI's own source-and-inventory Tokito package v0.1. The package preserves the original board source, inventory, and integrity metadata. It is not a complete geometry bundle and is not accepted by tokito-app. Other files containing the Tokito name are distinct formats and do not automatically parse as PCB documents.

The reusable Rust library exposes a typed PhysicalBoardDocument with source locators for components, pads, tracks, vias, zones, graphics, model references, nets, layers, and diagnostics. CLI dispatch is separate. Inspection, placement inventory, package verification, physical outline reports, and supported SVG projections consume the canonical records incrementally.

Milestone B adds a bounded physical outline report for supported board Edge.Cuts lines, arcs, and circles. It checks closure, degeneracy, intersection/overlap, disconnected and ambiguous contours, nesting, and cutouts; reports source identities and explicit diagnostics; and refuses unsupported outline features rather than certifying a partial shape. Physical bounds use source millimetres without drawing stroke or margin. The SVG viewport separately includes drawing stroke and its established display margin. Existing v0.1 inspections, CSV projections, and top/bottom SVGs are protected by exact historical fixtures.

SVG is partial visualization, not a KiCad plot or DRC result. Supported features include selected board and footprint graphics/text, simple pads/drills, tracks/arcs, vias, and stored filled-zone polygons. Custom pads and other unsupported geometry remain explicit limitations. Component identity is source-backed; font and plotting fidelity are not complete.

The committed real-source fixtures include SparkFun Basic, community Pico and UNO files, NXP MCXA ESC, the official KiCad RoyalBlue NFC board, and the routed Porcupine board. The Porcupine outline has one outer contour and five cutouts. Its copper arcs are distinct from its physical outline. The NFC fixture contains real outline gaps and must be rejected; its pinned Gerber supplies an independent 22-entity comparison. These examples validate parser behavior, not Tokito product-board qualification or manufacturing fitness.

## Latest local verification

The final local Windows x64 gate passed on 2026-10-09 with Rust/Cargo 1.94.1 and Python 3.11.5. It ran formatting, 40 library unit tests, eight engine API tests, nine topology API tests, Clippy with warnings denied, a release build, actual four-board subprocess E2E, geometry/SVG/Gerber reference E2E, and five historical v0.1 package compatibility journeys with exact inspect/CSV/top/bottom SVG output checks. The newest digest-backed verification.json is stored under target/delivery/local/.

The verification proves only the tested local source and Windows environment. It does not prove Linux/MSRV, a hosted CI run, full KiCad plotting/DRC conformance, fabrication validity, component model resolution, GLB/STEP generation, Desktop/MCP integration, phone import, or AR/device alignment. A previous LNK1104 occurred only in the runner's old target/verified output directory; a fresh ignored target/local-verification build completed all gates. The exact operating-system cause was not established.

## Tokito ecosystem roles and connection boundaries

| Repository or folder | Role and relationship to CLI |
|---|---|
| tokito | Rust desktop schematic studio and agentic design application; current schematic output is not by itself a physical PCB layout. Future local engine consumer. |
| tokito-schematic-core | Deterministic schematic geometry, routing, hierarchy, and connectivity; not a physical PCB placement/routing engine. |
| tokito-cli | Active pure-Rust parser, canonical PCB document, outline validation, partial SVG, and v0.1 source/inventory package. |
| tokito-app | The sole active mobile application combining companion and offline physical intelligence. App importer/device work needs its own authorization. |
| tokito-companion | Archived predecessor merged into tokito-app; not an active implementation target. |
| tokito-physical-runtime | Archived predecessor merged into tokito-app; not an active implementation target. |
| tokito-mcp | Rust MCP server used for the existing symbol/catalog workflows; no physical-board or 3D job API has been verified. |
| tokito-catalog | Hosted symbol-catalog client; optional enrichment, not required for offline PCB conversion. |
| tokito-api | Cloud accounts, plan, LLM gateway, and metering; not required for local conversion. |
| tokito-brain | Project knowledge and topic map; useful context, but live source remains authoritative. A reviewed desktop/agent-contract version pin is newer than one generated brain pin. |
| ui | Shared egui design-component library; no CLI runtime dependency. |
| tokito-infra | Deployment and operations configuration; not changed. |
| tokito-site | Product website and engineering journal; not changed. |
| homebrew-tokito | Desktop distribution tap; not changed. |
| tokito-legal | Policy/legal material; not changed. |
| tokito-dsvire | Datasheet figure retrieval experiment; not a required runtime component. |
| devboard_ar_dataset | Separate dataset scaffold; prior inspection found placeholder boxes and no accepted board/photo/label corpus. It is not needed by the parser milestone. |
| Tokito physical old | Preserved old physical-runtime checkout and evidence; not the active app and not modified. |

Desktop schematic JSON, design envelopes, project ZIPs, symbol-library files, connectivity files, and MCAD handoffs are separate contracts. No shipped .tokito_pcb physical format has been confirmed. The current v0.1 Tokito package is the only implemented native board-package adapter.

Desktop schematic coordinates use a project-specific unit system. The reviewed MCAD handoff used raw coordinates under millimetre-named fields and omitted instance UUIDs. It must not be treated as validated physical 3D/AR placement. A schematic has connectivity and symbols, not a board outline, component placement, or routed copper. The physical layout stage must supply those explicitly before a trustworthy 3D board can be generated.

## What remains

Milestones A and B are complete locally for the limited supported scope. The next implementation milestone is selectable GLB: triangulate a validated outline with real cutouts, use sourced thickness and models, map units/axes/normals correctly, preserve UUID picking, and reject missing assets transparently. Then version the native Tokito geometry/model package. Desktop and MCP connection follows after confirming actual desktop formats and separately granting sibling-repository write access. tokito-app import and AR follow the versioned package contract and require a real Android/iQOO device journey. Pure-Rust STEP support is a feasibility study after GLB. Schematic-to-PCB needs a separate layout/placement/routing stage; the engine must not invent it.

The issue and project board order is recorded in the private CLI project. Do not use KiCad CLI as a production dependency, invent 3D models, treat a schematic as a board layout, label hash checks as signatures, or claim complete compatibility from inventory counts.

## Delivery and review

Local verification remains the acceptance gate because hosted Actions were removed by owner request. The code is delivered on a feature branch and the PR stays open for owner approval. No merge, release, binary upload, or sibling-repository edit is part of this handoff. The local source history and recovery records remain preserved. The replacement repository's collaborator grant was verified; branch protection should not be assumed unless explicitly confirmed in GitHub settings.