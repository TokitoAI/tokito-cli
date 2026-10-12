# Tokito CLI — current state and personal-AI handoff

Historical engineering snapshot: 2026-10-10; workspace correction: 2026-10-12.
This report does not claim complete KiCad compatibility, Desktop handoff or phone AR acceptance. The active root is now `D:\kannan\projects\tokito ecosystem`; the verified F copy has been removed. See [current output locations](LOCAL_OUTPUT_LOCATIONS.md). Current-source verification receipts supersede the historical test descriptions below.

## Latest C-06 continuation

This update supersedes older statements below that model paths cannot be resolved.
The Rust resolver now captures explicitly approved local model files with bounded
reads, SHA-256 and structured redacted reports. `resolve models` supports KiCad
PCB and the v0.1 source package. Successful resolution is file capture, not mesh
or placement qualification. The first bounded mesh adapter parses a declared
flat triangle VRML97 profile with explicit units and materials; `model inspect`
reports local geometry. Full current-source QA status and the latest receipt are
authoritative in [GOAL_TRACKER.md](GOAL_TRACKER.md).

The authentic R0603 fixture supplies six referenced assets on the community UNO
layout and one independently inventoried local mesh (29 root Shapes, 26 meshes,
328 vertices, 276 triangles). The pure-Rust point and parsed triangle-mesh
placement APIs follow the pinned KiCad 10.0.7 STEP exporter behavior; nine
focused tests cover asymmetric front/back poses, frame reflection, signed scale
parity, winding and recomputed normals. Those APIs now feed the bounded
`--assemble-wrl` GLB path. Each component has one renderable node named
`ref:<refdes>`; it owns a mesh with separate primitives for imported model parts
and carries UUID/source metadata. CLI subprocess tests check one- and
two-component output, GLB accessors, and the App's exact-prefix visibility rule.
This does not establish App package intake, Android rendering/selection or KiCad
mesh parity. Assembly reads only caller-approved roots, checks body/stackup
thickness agreement, enforces a pre-retention aggregate triangle cap and bounded
coverage diagnostics, and reports unresolved model status codes. Both shell-only
and incomplete assembly output still require explicit `--allow-partial` consent.

Read-only remote snapshots are App `780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50`
and earlier dataset corpus `9787b398231421656e64f40217ed7e17e9aac625`; the newer Ultimainboard sample is `c2bdf2369e13b71aaa8f13a91a23637faf339813`. Its metadata counts were checked, but binary hashes, model assignments and physical registration were not independently qualified. The user plans all dataset boards, with separate per-board gates in DATASET_BOARD_HANDOFF.md. The dataset's photos,
board boxes and partial component seeds do not certify placement or BOM identity.
The App docs define a revisioned assembly-context bundle and a separate GLB plus
twin manifest; native selection is `componentNodePrefix + refdes` (default
`ref:`). The inspected mobile bridge does not expose board-package intake or a
final Desktop-to-phone delivery flow. App docs differ on whether cross-product
delivery is an authorized Cloud contract or a verified local bundle import, so
keep CLI `mobileImportReady` false until the App owner confirms the route,
canonical context/GLB pair, and signing/verification owner. Sibling repositories
were inspected read-only and not edited.

The later Desktop read-only snapshot is `TokitoAI/tokito` master
`34a5f55c8425ff7ef53efd33bc6b6e36d0e877f5`, which pins
`tokito-schematic-core` v0.4.4 at `445c05e90aaad8b1433e053f0f72aec83250ed2c`.
It provides stable schematic symbol UUIDs, pin keys, net membership and optional
footprint identifiers. These can form a versioned `SchematicToPhysicalHandoff`,
but schematic symbol positions and rotations are canvas data. They cannot
establish a board outline, routing, stackup, footprint centroid, side, 3D model,
or AR registration. The App has a consumer schema for an authored assembly
context and GLB manifest but no defined Desktop-to-App board-layout resource or
mobile importer. Keep schematic-to-physical conversion outside the CLI's
completed claims until an authored physical-layout stage exists.

API source `e52c0d87f053611ed831fb74ffa186c06d28dc0e` documents structured
`physical_sessions` and generic project-artifact upload/download capped at
16 MiB per artifact. The inspected contract has no typed board/GLB validation,
installation, or component-selection operation. Generic storage is only a
possible transport after the App defines and validates a package flow; it is not
an existing mobile bridge. See [App/API audit](APP_API_BRIDGE_AUDIT.md) for the
pinned native/FFI call chain and unresolved owner decisions.

PR #11 is merged: live read-only GitHub state on 2026-10-10 reports merge commit
`7700eb7762ceaa8e82d1cc08d2abb1e62e33f564`, merged at 2026-10-10 01:26 UTC.
Its prior tested source does not include the current local selector/help
continuation. Any later delivery needs fresh local verification, independent
review, and its own feature branch/PR. Actions and binary delivery stay local;
direct main push is prohibited. Issues #12 (resolver) and #13 (KiCad parity)
track the related work. Historical milestone letters D–H remain unchanged. See
[team requests](TEAM_HANDOFF.md), [model profile and gates](MODEL_RESOLUTION.md)
and [pinned ecosystem map](ECOSYSTEM_CONTRACT_MAP.md).

## Workspace, repositories, and ownership

The active ecosystem root is `D:\kannan\projects\tokito ecosystem`, and the CLI project is `tokito_cli`. The duplicate CLI folder was previously consolidated. The old F workspace paths are gone. The 2026-10-12 maintenance was separately authorized to preserve F-only work, remove the verified F ecosystem, consolidate D artifacts and refresh clean ecosystem checkouts from GitHub. Sibling feature code remains outside this task's write scope. Use `tokito-app-dataset`; `devboard_ar_dataset` is historical data, not the selected active source.

The private repository is TokitoAI/tokito-cli (repository ID 1412077895). PR #8, `feat/native-pcb-engine-milestone-b`, has been merged. Milestone C changes are being developed locally on `codex/milestone-c-selectable-glb`; GitHub Actions remain disabled by owner request. Delivery stays on a feature branch/PR, with `main` untouched until the agreed merge step. No sibling repository is being changed. The old repository's PR #9, issues, checks, branches, and Git bundle remain historical recovery evidence under target/recovery/github-before-recreation-20261009T150910Z. They are not live objects in the recreated repository.

GitHub collaborator access was checked on the replacement repository: NAVANEETHVVINOD has Admin access. No new invitation was sent. The private project Tokito CLI - PCB Engine Roadmap is linked from the repository Projects tab at https://github.com/TokitoAI/tokito-cli/projects. GitHub still identifies it as organization-owned project #2; the link surfaces this CLI-specific board under the repository, while the separate overall Tokito ecosystem project remains untouched. Roadmap issues are:
- Milestone B outline and v0.1 compatibility: https://github.com/TokitoAI/tokito-cli/issues/1
- Milestone C selectable GLB: https://github.com/TokitoAI/tokito-cli/issues/2
- Milestone D native Tokito physical-board package: https://github.com/TokitoAI/tokito-cli/issues/3
- Milestone E Desktop/MCP integration: https://github.com/TokitoAI/tokito-cli/issues/4
- Milestone F tokito-app offline import and AR: https://github.com/TokitoAI/tokito-cli/issues/5
- Milestone G pure-Rust STEP feasibility: https://github.com/TokitoAI/tokito-cli/issues/6
- Schematic-to-physical-layout handoff: https://github.com/TokitoAI/tokito-cli/issues/7
- Follow-up: fail closed on ambiguous near-parallel Edge.Cuts contacts: https://github.com/TokitoAI/tokito-cli/issues/9

No GitHub Actions workflow is included, by explicit owner request to conserve the free-plan quota. Hosted checks and hosted binary uploads are not claimed. The repository includes a local verification runner and keeps its output under the ignored target directory.

## Current implementation

Tokito CLI is an original, pure-Rust, offline-first PCB import and geometry engine. It does not invoke KiCad CLI at runtime and does not translate KiCad C++ code. KiCad documented formats and observed behavior guided compatibility research.

The implemented input is a bounded subset of KiCad PCB S-expressions plus the CLI's own source-and-inventory Tokito package v0.1. The package preserves the original board source, inventory, and integrity metadata. It is not a complete geometry bundle and is not accepted by tokito-app. Other files containing the Tokito name are distinct formats and do not automatically parse as PCB documents.

The reusable Rust library exposes a typed PhysicalBoardDocument with source locators for components, pads, tracks, vias, zones, graphics, model references, nets, layers, and diagnostics. CLI dispatch is separate. Inspection, placement inventory, package verification, physical outline reports, and supported SVG projections consume the canonical records incrementally.

Milestone B adds a bounded physical outline report for supported board Edge.Cuts lines, arcs, and circles. It checks closure, degeneracy, intersection/overlap, disconnected and ambiguous contours, nesting, and cutouts; reports source identities and explicit diagnostics; and refuses unsupported outline features rather than certifying a partial shape. Physical bounds use source millimetres without drawing stroke or margin. The SVG viewport separately includes drawing stroke and its established display margin. Existing v0.1 inspections, CSV projections, and top/bottom SVGs are protected by exact historical fixtures.

Milestone C triangulates the validated outer boundary plus direct cutouts, tessellates supported curves, and extrudes authored thickness into a self-contained GLB. Coordinates use metres, right-handed Y-up, board plane XZ, centered at the mid-plane. The exporter checks GLB framing, geometry/accessors, analytic/polygonized area, and f32 position precision. Shell-only output is always partial and the default refuses. Bounded flat-triangle WRL models may be assembled through explicit local roots; each component's mesh-bearing node follows `ref:<refdes>` and carries UUID metadata. Per-model omission reasons and truncation are present in coverage. Absolute paths are redacted. STEP, general VRML, phone package intake, actual App selection, independent CAD geometry parity, and complete digital-twin output remain unimplemented.

SVG is partial visualization, not a KiCad plot or DRC result. Supported features include selected board and footprint graphics/text, simple pads/drills, tracks/arcs, vias, and stored filled-zone polygons. Custom pads and other unsupported geometry remain explicit limitations. Component identity is source-backed; font and plotting fidelity are not complete.

The committed real-source fixtures include SparkFun Basic, community Pico and UNO files, NXP MCXA ESC, the official KiCad RoyalBlue NFC board, and the routed Porcupine board. The Porcupine outline has one outer contour and five cutouts. Its copper arcs are distinct from its physical outline. The NFC fixture contains real outline gaps and must be rejected; its pinned Gerber supplies an independent 22-entity comparison. These examples validate parser behavior, not Tokito product-board qualification or manufacturing fitness.

## Latest local verification

The frozen selector/help/assembly source passed all ten local stages and 109 Rust
tests on Windows x64: formatting, tests, warnings-denied Clippy, release build,
CLI subprocess E2Es, geometry/SVG/Gerber references, GLB E2E, model-resolution
E2E, historical v0.1 compatibility, and the release-binary acceptance journey.
The exact receipt is
`target/delivery/local/20261010T025719Z-b99c34e60c5b4cfbbdd9550e93204a71/verification.json`.
The real-binary receipt is under
`target/real-cli-e2e/20261010T025713Z-16050e4c80c44904aab2e1df0f5be9bd/`.
Independent static review found no remaining code blocker. This handoff and the
tracker were updated after verification to record actual results; their narrative
hashes differ from that receipt. Source, scripts, fixtures, tests and binary must
still match before delivery. No hosted workflow ran.

The verification proves only the tested local source and Windows environment. It does not prove Linux/MSRV, a hosted CI run, full KiCad plotting/DRC conformance, fabrication validity, stackup extraction, assembled component selection, general VRML or STEP decoding, Desktop/MCP integration, phone import, or AR/device alignment. CLI subprocess E2Es exercise actual command-line journeys, not Android device or GUI acceptance. The GLB E2E checks container/accessor structure, finite positions, dimensions/thickness, cap area, oriented closed edges, explicit partial consent, determinism, package input, no overwrite and invalid-outline refusal; it is not a CAD viewer or phone renderer oracle. A previous LNK1104 occurred only in the runner's old target/verified output directory; a fresh isolated local verification build completed all gates. The exact operating-system cause was not established.

## Tokito ecosystem roles and connection boundaries

| Repository or folder | Role and relationship to CLI |
|---|---|
| tokito | Rust desktop schematic studio and agentic design application; current schematic output is not by itself a physical PCB layout. Future local engine consumer. |
| tokito-schematic-core | Deterministic schematic geometry, routing, hierarchy, and connectivity; not a physical PCB placement/routing engine. |
| tokito-cli | Active pure-Rust parser, canonical PCB document, outline validation, partial SVG, board-shell GLB, bounded flat-triangle VRML assembly with mesh-bearing refdes selectors, and v0.1 source/inventory package. It does not yet prove App import, generated-CAD geometry parity, STEP support, or schematic-to-physical layout conversion. |
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

Milestones A and B are complete for their limited supported scope. Milestone C now has bounded VRML-to-GLB assembly with one mesh-bearing `ref:<refdes>` node per component, multi-part model primitives, and per-model omission reasons. Next are independent asymmetric geometry checks, stronger real-board/model coverage, and verification through the App's real importer. Then define/version the complete geometry package. Desktop and MCP adapters follow only after a physical-layout handoff and library/job contract exist. App import and AR require changes in the App repository plus a real Android/iQOO device journey. Pure-Rust STEP support remains a feasibility study. Schematic-to-PCB needs a separate placement/routing stage; the engine must not invent it.

The issue and project board order is recorded in the private CLI project. Do not use KiCad CLI as a production dependency, invent 3D models, treat a schematic as a board layout, label hash checks as signatures, or claim complete compatibility from inventory counts.

## Delivery and review

Local verification remains the acceptance gate; the repository has no workflow files, and no GitHub Actions run is requested. PR #11 is already merged; these selector/help/assembly edits are separate local work and have not been delivered. Any future delivery requires current-source local checks, independent review, and a separate feature branch/PR. Do not push to `main`, merge, upload a binary, or edit a sibling repository as part of this local continuation. The local source history and recovery records remain preserved. The replacement repository's collaborator grant was verified; branch protection should not be assumed unless explicitly confirmed in GitHub settings.
