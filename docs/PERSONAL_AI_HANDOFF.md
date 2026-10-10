# Tokito CLI — current state and personal-AI handoff

Date: 2026-10-10. This is a code- and test-backed status report. It does not claim that the Tokito ecosystem, complete KiCad compatibility, selectable component-model digital twin, Desktop handoff, or phone AR flow is complete.

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
328 vertices, 276 triangles). This continuation adds pure-Rust point and parsed
triangle-mesh placement APIs based on the pinned KiCad 10.0.7 STEP exporter
behavior. Nine focused tests cover asymmetric front/back poses, frame reflection,
signed scale parity, triangle winding, and recomputed normals. Both APIs require
explicit front/back copper-plane inputs; the canonical source parser now derives
supported copper/core/prepreg stackup planes, but GLB scene assembly, component
UUID/refdes nodes, and independent comparison with a generated CAD export remain
open. The board-shell GLB remains partial and requires
`--allow-partial`.

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

The owner authorized CLI issues/Project updates and a feature-branch PR push
after full current local QA and independent review. Reviewed source commit
`5db49481ce62e859d13cf0504c75f6f6d1124add` was delivered to
[PR #11](https://github.com/TokitoAI/tokito-cli/pull/11), which remains open.
The tested source is unchanged by this subsequent delivery documentation. Actions/binary delivery stay
local; no direct main push. Issues #12 (resolver) and #13 (KiCad parity) track the
new work. Historical milestone letters D–H remain unchanged. See
[team requests](TEAM_HANDOFF.md), [model profile and gates](MODEL_RESOLUTION.md)
and [pinned ecosystem map](ECOSYSTEM_CONTRACT_MAP.md).

## Workspace, repositories, and ownership

The ecosystem root is F:\kannan\projects\tokito ecosystem. The active CLI project is tokito_cli. The duplicate CLI folder was consolidated with the user's authorization. The old F:\kannan\projects\Tokito path is gone. This task changed only the CLI repository. Other Tokito repositories, archived checkouts, Tokito physical old, and devboard_ar_dataset remain read-only and were not edited.

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

Milestone C's first slice triangulates the validated outer boundary plus direct cutouts, tessellates supported curves, and extrudes authored thickness into a self-contained GLB. Coordinates use metres, right-handed Y-up, board plane XZ, centered at the mid-plane. The exporter reparses its GLB, checks bounded/finite geometry and analytic/polygonized area, and rejects f32 position quantization that exceeds 1 µm or collapses/inverts a face. Shell-only output is always partial and the default refuses; callers must set `--allow-partial`. JSON/GLB coverage metadata lists component identities and source model references with bounded record/path counts. Absolute model paths are redacted to basenames. Source-derived point/parsed-mesh placement helpers now exist, but the shell exporter still emits no component meshes or stable selectors. Stackup parsing, scene integration, STEP decoding, and complete digital-twin output remain unimplemented.

SVG is partial visualization, not a KiCad plot or DRC result. Supported features include selected board and footprint graphics/text, simple pads/drills, tracks/arcs, vias, and stored filled-zone polygons. Custom pads and other unsupported geometry remain explicit limitations. Component identity is source-backed; font and plotting fidelity are not complete.

The committed real-source fixtures include SparkFun Basic, community Pico and UNO files, NXP MCXA ESC, the official KiCad RoyalBlue NFC board, and the routed Porcupine board. The Porcupine outline has one outer contour and five cutouts. Its copper arcs are distinct from its physical outline. The NFC fixture contains real outline gaps and must be rejected; its pinned Gerber supplies an independent 22-entity comparison. These examples validate parser behavior, not Tokito product-board qualification or manufacturing fitness.

## Latest local verification

The latest full local Windows x64 verification receipt is the newest
`verification.json` under `target/delivery/local/`. Its recorded versions, source
fingerprints, test counts, gate results, and binary hash are authoritative. It
passed ten stages: formatting, Rust tests, Clippy with warnings denied,
release build, four-board CLI subprocess E2E, geometry/SVG/Gerber reference E2E,
board-shell GLB E2E, model-resolution E2E, historical v0.1 compatibility, and a
release-binary acceptance journey that preserves a local machine-readable receipt
under `target/real-cli-e2e/`.
There are 106 Rust tests, including nine focused placement tests. No hosted
workflow is run. The verifier writes a timestamped receipt in that directory;
the newest receipt records the current source fingerprints and actual results.
Check that receipt before delivery instead of relying on older narrative counts.

The verification proves only the tested local source and Windows environment. It does not prove Linux/MSRV, a hosted CI run, full KiCad plotting/DRC conformance, fabrication validity, stackup extraction, assembled component selection, general VRML or STEP decoding, Desktop/MCP integration, phone import, or AR/device alignment. CLI subprocess E2Es exercise actual command-line journeys, not Android device or GUI acceptance. The GLB E2E checks container/accessor structure, finite positions, dimensions/thickness, cap area, oriented closed edges, explicit partial consent, determinism, package input, no overwrite and invalid-outline refusal; it is not a CAD viewer or phone renderer oracle. A previous LNK1104 occurred only in the runner's old target/verified output directory; a fresh isolated local verification build completed all gates. The exact operating-system cause was not established.

## Tokito ecosystem roles and connection boundaries

| Repository or folder | Role and relationship to CLI |
|---|---|
| tokito | Rust desktop schematic studio and agentic design application; current schematic output is not by itself a physical PCB layout. Future local engine consumer. |
| tokito-schematic-core | Deterministic schematic geometry, routing, hierarchy, and connectivity; not a physical PCB placement/routing engine. |
| tokito-cli | Active pure-Rust parser, canonical PCB document, outline validation, partial SVG, explicitly partial board-shell GLB, and v0.1 source/inventory package. Bounded model capture and strict flat-triangle WRL inspection are implemented; world transforms/selectors remain future work. |
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

Milestones A and B are complete locally for the limited supported scope. The near-parallel ambiguity defect was reproduced, fixed, independently reviewed, and covered by a public regression. Milestone C is active: the shell-only GLB slice, bounded offline model capture, strict WRL profile, source-derived point/mesh placement helpers, and strict supported stackup-plane derivation pass local tests. Per-component mesh integration and provenance, UUID/refdes nodes, full-coverage exports, and independent generated-export comparison remain next. Then version the native Tokito geometry/model package. Desktop and MCP connection follows after confirming actual desktop formats and separately granting sibling-repository write access. tokito-app import and AR follow the versioned package contract and require a real Android/iQOO device journey. Pure-Rust STEP support is a feasibility study after selectable GLB. Schematic-to-PCB needs a separate layout/placement/routing stage; the engine must not invent it.

The issue and project board order is recorded in the private CLI project. Do not use KiCad CLI as a production dependency, invent 3D models, treat a schematic as a board layout, label hash checks as signatures, or claim complete compatibility from inventory counts.

## Delivery and review

Local verification remains the acceptance gate; the repository has no workflow files, and no GitHub Actions run is requested. This continuation updates the existing feature branch and PR #11 after local checks and independent review. It does not push to `main`, merge, upload a binary, or edit a sibling repository. The local source history and recovery records remain preserved. The replacement repository's collaborator grant was verified; branch protection should not be assumed unless explicitly confirmed in GitHub settings.
