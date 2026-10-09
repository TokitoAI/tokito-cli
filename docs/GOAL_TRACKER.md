# Tokito Native PCB Engine and Selectable 2D/3D Digital Twin

Goal ID: TOKITO-PCB-ENGINE-001. Current execution: Milestones A and B are complete for their declared scope.

## 1. Product goal

Build an original, pure-Rust, offline-first, Windows-first engine that imports supported PCB documents, validates physical geometry, renders usable 2D/3D views, and preserves component identity for selection. KiCad is not a normal-runtime dependency. STEP and schematic-to-PCB remain later work.

## 2. Current stage and progress

Milestone A and Milestone B are complete for their stated scope. B validates supported physical Edge.Cuts topology, classifies contained cutouts, preserves historical v0.1 output bytes, and uses pinned board/Gerber references. The current-source local verification gate passed after the documentation and verifier-target updates.

A+B task progress: 14 of 14 scoped tasks verified. This is not engineering completeness of the final digital-twin product. Milestone readiness: A and B complete; C through H planned.

## 3. Status legend

- ✅ Completed: implementation and acceptance verified with actual evidence.
- 🔄 Ongoing: started, acceptance incomplete.
- ☐ Not started: no verified implementation.
- ❌ Failed or blocked: required gate failed or a concrete dependency is blocked.

Historical failures stay in the register even after resolution. Current status is determined by the latest full verification receipt, not by a partial or earlier run.

## 4. Milestone dashboard

| ID | Milestone | Status | Remaining acceptance |
|---|---|---|---|
| A | Canonical Rust PCB engine and public API | ✅ | Broaden normalized interpretation only with evidence |
| B | Contours, topology and SVG accuracy | ✅ | Extend format coverage only with supported fixtures |
| C | Exact model resolution and selectable GLB | ☐ | Requires validated topology, sourced dimensions/models, UUID selectors |
| D | Versioned native Tokito physical-board package | ☐ | Preserve v0.1 compatibility; do not claim mobile readiness early |
| E | Desktop and MCP integration | ☐ | Confirm actual contracts and obtain separate sibling-repository write scope |
| F | tokito-app offline import and AR | ☐ | Needs stable package contract and real Android/iQOO device journey |
| G | Optional pure-Rust STEP/CAD support | ☐ | Feasibility study after GLB |
| H | Schematic-to-physical-layout handoff | ☐ | Requires explicit placement/routing stage; schematics alone are insufficient |

## 5. Completed Milestone B tasks

| ID | Task | Status | Acceptance evidence |
|---|---|---|---|
| B-01 | Audit and characterize geometry | ✅ | Demonstrated why endpoint degree alone cannot certify a physical outline; coordinate and projection boundaries documented |
| B-02 | Canonical physical contours | ✅ | Sourced line, arc, and circle normalization; bounded joins; centerline bounds |
| B-03 | Intersection/topology validation | ✅ | Reject open, degenerate, duplicate, branched, crossing, overlapping, and ambiguous contours |
| B-04 | Holes/cutouts and nesting | ✅ | One outer contour plus contained holes; refuse disconnected, nested-island, or ambiguous topology |
| B-05 | Sourced fixture expansion | ✅ | Pinned licensed board fixtures, provenance, source hashes, and actual gap evidence |
| B-06 | SVG/reference validation | ✅ | 22 pinned Gerber entities matched; 17 physical SVG arcs and five circles, IDs, sweeps, radii, and bounds checked on both views |
| B-07 | v0.1 compatibility | ✅ | Five historical package fixtures; exact inspect, CSV, top/bottom SVG bytes and tamper checks |
| B-08 | Full local verification and independent review | ✅ | Formatting, 57 Rust tests, Clippy, release build, four-board E2E, geometry/Gerber E2E, compatibility E2E; independent review found no remaining scoped blocker |

## 6. Ownership and delivery

Main owns requirements, integration, tracker, and final QA. The implementation and review agents were limited to scoped CLI work; they shared the checkout and preserved existing edits. Sibling repositories remain read-only.

The owner authorized a feature-branch push, issue tracking, a PR, and a private project. The sole existing CI workflow file was removed from this repository by request; no workflow files remain, so GitHub Actions are not expected to run. Hosted checks and hosted binary uploads are not claimed. Local verification and E2E remain required.

The replacement private repository is TokitoAI/tokito-cli. NAVANEETHVVINOD was verified to have Admin access; no invitation was sent. The dedicated private roadmap is https://github.com/orgs/TokitoAI/projects/2 with issues #1 through #7. No application source belongs directly on main. The local development history and earlier-repository recovery evidence remain preserved.

## 7. Milestone B acceptance

- The physical outline report uses source millimetres and Y-down coordinates, centerlines rather than stroke/margins, and source locators/UUIDs.
- Join tolerances and work limits are bounded and declared.
- Supported line/arc/circle topology rejects crossings, overlaps, invalid tangencies, degeneracy, branches, open chains, and ambiguous nesting.
- Real boards demonstrate curved outlines and cutouts. Copper routing arcs are not treated as physical-outline evidence.
- Existing inspection, CSV, package schema, and historical SVG bytes remain compatible. Invalid topology does not prevent an integrity-valid inventory package from being imported or verified.
- Independent Gerber comparisons and full current-source local verification pass.
- GLB or solids are not generated before this topology gate.

## 8. Current verification

The release gate is scripts/verify-local.ps1. It uses target/local-verification and writes a tested executable plus verification.json under target/delivery/local/. Use the newest timestamped receipt.

The latest full local Windows x64 run passed formatting, 40 library unit tests, eight engine API tests, nine topology API tests, Clippy with warnings denied, release build, four-board CLI subprocess E2E, geometry/SVG/Gerber E2E, and five v0.1 compatibility packages with exact historical inspect/CSV/top/bottom SVG outputs. The most recent receipt records toolchain versions, source-input hashes, tested binary hash, and platform.

This local Windows evidence does not establish Linux/MSRV, hosted CI, full KiCad plot/DRC conformance, fabrication validity, GLB/STEP generation, Desktop/MCP integration, phone import, or AR/device accuracy.

## 9. Error and blocker register

| ID | Failure | Cause | Resolution |
|---|---|---|---|
| B-ERR-001 | Self-crossing outline passed the old endpoint-degree check | Endpoint degree does not establish absence of crossings | ✅ Canonical physical topology gate added; public bow-tie regression passes |
| B-ERR-002 | Scoped topology build hit linker LNK1104 in a Windows target directory | Environment-specific executable lock/path behavior | ✅ Isolated target build passed; scoped tests passed |
| B-ERR-003 | Tiny translated contour area differed due cancellation | Area computed using large absolute coordinates | ✅ Translation-local area calculation |
| B-ERR-004 | Positive sub-join-tolerance overlap was accepted | Join tolerance concealed smaller geometric overlap | ✅ Positive overlap checked separately |
| B-ERR-005 | Geometry E2E expected bounds from an open outline | Test contradicted fail-closed report contract | ✅ Invalid outlines have no certified bounds |
| B-ERR-006 | Prior comparison found silk/fab SVG sweep changes | Stable physical arc math had affected all layers | ✅ Restrict stable math to physical Edge.Cuts; exact outputs pass |
| B-ERR-007 | A translated 1 µm semicircle had inaccurate SVG radii | Renderer squared large absolute coordinates | ✅ Physical Edge.Cuts rendering uses a local-frame circumcenter |
| B-ERR-008 | A module-owned cutout validated but was omitted from SVG | Legacy module outline was not projected | ✅ Unsupported module Edge.Cuts fails closed with locator |
| B-ERR-009 | Shallow translated arc area lost precision | Cancellation in sweep-minus-sine calculation | ✅ Small-angle series and regression |
| B-ERR-010 | Expanded geometry E2E had Python IndentationError | Test edit left an empty branch | ✅ Removed branch; reference/E2E passes |
| B-ERR-011 | Exact comparison first used a stale release binary | Command targeted an older target path | ✅ Fresh release binary passes all four comparisons |
| B-ERR-012 | Final verifier test link hit LNK1104 in target/verified | That target repeatedly failed to link while a new isolated target compiled; exact OS cause is unknown | ✅ Runner uses target/local-verification; complete current-source gate passed |

## 10. Implementation evidence

Key CLI files include src/document.rs, src/lib.rs, src/cli.rs, src/geometry.rs, and src/topology.rs. Tests include tests/engine_api.rs, tests/topology_api.rs, tests/geometry_e2e.py, tests/compat_v0.py, and tests/e2e_cli.py. Source and reference fixture provenance is in docs/PCB_FIXTURE_CORPUS.md; API and topology limits are in docs/CANONICAL_ENGINE.md and docs/PCB_TOPOLOGY.md.

The current reader is bounded and partial. Unknown or unsupported outline features must remain diagnostic and block certification; no API field means complete KiCad interpretation or fabrication readiness.

## 11. Completed milestone history

Milestone A delivered the importer, UUID-backed inventory, partial SVG, Tokito package v0.1, public canonical model, and local verification. Milestone B delivered supported physical outline validation, topology and cutout classification, exact SVG compatibility, pinned reference comparisons, and the complete local gate.

The earlier-repository recovery bundle remains under target/recovery/github-before-recreation-20261009T150910Z. Old PR #9 and its failed artifact upload are historical; they are not current checks in the replacement repository.

## 12. Next work

Follow issues in the private CLI roadmap. First implement C: triangulate validated boundaries with cutouts, source thickness and models, verify metre/Y-up transforms, winding/normals/bounds, and UUID selection in GLB. Then define D's versioned native Tokito geometry/model package. E requires confirmed Desktop schemas and separate permission before sibling changes. F requires app permission and a real offline Android/iQOO import, selection, and AR journey. G is a separate pure-Rust STEP feasibility study. H defines the required physical placement/routing handoff from schematic generation.

Do not silently replace missing models with boxes, treat a schematic as a physical layout, label hashes as signatures, or claim complete support from inventory counts.

## 13. Significant transitions

- 2026-10-09 22:26 IST: goal created; Milestone A baseline rerun passed.
- 2026-10-09 22:31 IST: geometry audit and public regression completed.
- 2026-10-09 23:08 IST: independent review drove the stable arc and unsupported module-outline fixes.
- 2026-10-09 23:20 IST: archived SVG goldens, analytic geometry checks, and the full local gate passed.
- 2026-10-09: private replacement repository and collaborator access verified; private CLI project and issues #1-#7 created. Code delivery remains through a feature branch and PR.

Last updated: 2026-10-09. The latest verification timestamp is in the newest local receipt under target/delivery/local/.