# Tokito Native PCB Engine and Selectable 2D/3D Digital Twin

Goal ID: TOKITO-PCB-ENGINE-002. Goal 001's bounded Milestone B foundation is complete; this active goal advances the evidence-gated path to selectable assembled 3D and a clearly documented Tokito CLI. The current working tree adds a pure-Rust bounded VRML assembly path with strict stackup-plane and unique-UUID gates, an implemented-command help tree, and one mesh-bearing `ref:<refdes>` GLB node per component. The frozen implementation passed all ten local gates and independent static review; the post-run narrative update is recorded below. Complete model coverage, independent generated-export comparison, App import/device acceptance, and schematic-to-physical-layout conversion remain open. Use the newest `verification.json` under `target/delivery/local/` only after confirming its source fingerprints match. Milestones A and B remain complete for their declared scope. Only CLI source is writable; App/API/dataset repositories are inspected read-only. GitHub Actions remain disabled and direct main push is prohibited.

## 1. Product goal

Current continuation: the verified foundation and BOM reconciliation are delivered through merged
[PR #17](https://github.com/TokitoAI/tokito-cli/pull/17), head
`dae0a1a9e94e83fa4fa5f2ee5fa1ccbf6d48f45c`, merged as `f935b7500501eaadcab85ab8e0c53487c3101b6c` on 2026-10-10. PR #11 and #16 are historical merges.
Dataset qualification uses main at `257ea7e68ee2e700522bd1b19ad9cbf1c0c3980a`
and [issue #18](https://github.com/TokitoAI/tokito-cli/issues/18). The latest
local continuation adds an isolated STEP evaluation crate and three new local
verification gates, so the earlier ten-gate receipt does not qualify these new
files. The current thirteen-gate run passed at `target/delivery/local/20261010T032444Z-5c932454a5f241ba94d9deae388ea88f/verification.json`; independent review found no admission blocker. Two reproducible negative executable tests now cover malformed STEP and unmaterialized LFS pointers; final verification includes both before delivery.
See [KiCad-quality 3D gates](KICAD_3D_QUALITY_GATES.md) for the actual ESP32
comparison and the remaining geometry/pose/material acceptance requirements.

Subsequent real-corpus qualification materialized three PCB layouts and all 61
Ultimainboard STEP assets. The updated isolated probe adds per-shape provenance
observations; its current scoped formatting, five unit tests, two executable
integration tests and Clippy passed, followed by all-61 reader journeys and
independent FreeCAD references. The engine/verifier inputs still match the
thirteen-gate receipt; this is combined scoped evidence, not a new whole-tree
thirteen-gate run. See [qualification results](DATASET_STEP_QUALIFICATION.md)
and [issue #19](https://github.com/TokitoAI/tokito-cli/issues/19) for concrete gaps.

| Current task | Owner | Dependency | State | Evidence / next action |
| --- | --- | --- | --- | --- |
| Foundation delivery | Main + independent reviewer | Ten local gates | PR open | #17 matches tested implementation; no Actions or upload |
| Dataset target audit | Main + read-only reviewer | Current dataset main | Selected assets verified | Three PCB layouts + 61 STEP assets match pinned LFS pointers; ESP32 refuses six joins; NXP/Ultimainboard produce explicit partial shells |
| Outline behavior comparison | Main + read-only researcher | Same ESP32 bytes | Evidence captured | Two KiCad 10.0.5 GLBs; 10.0.7 source explains derived-contour/inference paths, not complete parity |
| Pure-Rust STEP evaluation | Scoped worker + Main | Pinned reader and real model corpus | Corpus observations captured | All 61 return meshes; warnings/invalid reference/discrepancies remain; production admission disabled; see DATASET_STEP_QUALIFICATION.md |
| Current-source QA/review | Main + independent reviewer | Frozen experiment and docs | Thirteen gates passed | 109 engine tests, three probe unit tests, two executable rejection tests and subprocess E2E; final snapshot verification required before push |

Build an original, pure-Rust, offline-first, Windows-first engine that imports supported PCB documents, validates physical geometry, renders usable 2D/3D views, and preserves component identity for selection. KiCad is not a normal-runtime dependency. STEP and schematic-to-PCB remain later work.

## 2. Current stage and progress

Milestones A and B are complete for their declared scope. The CLI wires parsed-triangle mesh placement into a bounded GLB assembly path and tests front/back board-side normals plus winding parity under signed model scales. `--assemble-wrl` requires caller-approved roots, matching source stackup/body thickness, and unique component UUID and reference identities. Each emitted component is represented by one mesh-bearing `ref:<refdes>` node with model-part primitives. One/two-component selection-rule and diagnostic-bound coverage passed the full local verifier; independent static review found no remaining code blocker. Independent generated-export comparison, App import and Android selection remain open.

Geometry evidence includes a pinned same-commit KiCad 7 profile for the Raspberry Pi Press RP2040 Game Controller: all 644 valid source Edge.Cuts lines match one-to-one with 0 mm maximum endpoint error, and the validator reports one outer contour plus four cutouts. A reduced LumenPnP vacuum-adapter extract adds a valid curved reference comparison for eight represented entities (four lines, two quarter-circle arcs, and two full-circle arcs) against its same-revision Gerber profile. The two footprint-owned 0.5 mm circles transform to board-space centers (121, 74.4) and (127, 80.4), preserve their graphic and footprint UUIDs, and appear in SVG. This derived extract is not the full upstream PCB; no raw upstream file checksum or complete-board equivalence is claimed. The NFC board remains an intentionally open curved negative case. Porcupine remains a valid curved board without an independent plot. Full curved-outline or rendered-SVG fidelity is not established.

The wider product goal remains active. The tested board-shell GLB and bounded WRL assembly slice are in place, with offline asset resolution, source-derived point/mesh placement, stackup/body consistency checks, and UUID-backed node metadata. General VRML, STEP, complete model coverage, independent KiCad export comparison, App compatibility and phone acceptance remain open. Do not infer physical board geometry from a schematic-only input.

## 3. Status legend

- ✅ Completed: implementation and acceptance verified with actual evidence.
- 🔄 Ongoing: started, acceptance incomplete.
- ⏸ Deferred: existing or planned work is outside the current authorized execution scope.
- ☐ Not started: no verified implementation.
- ❌ Failed or blocked: required gate failed or a concrete dependency is blocked.

Historical failures stay in the register even after resolution. Current implementation status is determined by the latest full verification receipt plus later scoped verification recorded in section 9; a partial receipt does not replace full acceptance.

## 4. Milestone dashboard

| ID | Milestone | Status | Remaining acceptance |
|---|---|---|---|
| A | Canonical Rust PCB engine and public API | ✅ | Broaden normalized interpretation only with evidence |
| B | Contours, topology and SVG accuracy | ✅ | Extend format coverage only with supported fixtures |
| C | Exact model resolution and selectable GLB | 🔄 | Board shell, offline model capture, strict local WRL inspection, point/mesh placement helpers, strict stackup-plane derivation, and mesh-bearing UUID/refdes selector nodes are implemented; full current-source local verification, independent export comparison, broad component coverage, App import and Android selection remain |
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
| B-05 | Sourced fixture expansion | ✅ | Pinned source files, clearly labeled reduced extracts, provenance and fixture hashes; actual gap and geometry evidence |
| B-06 | SVG/reference validation | ✅ | NFC matches 22 entities but is open; RP2040 matches 644 lines; original LumenPnP PCB/profile matches eight curved/circular entities plus direct SVG geometry; reduced fixture preserved; Porcupine checks identities/viewport |
| B-07 | v0.1 compatibility | ✅ | Five historical package fixtures; exact inspect, CSV, top/bottom SVG bytes and tamper checks |
| B-08 | Full local verification and independent review | ✅ | Fresh nine-gate/92-test run passes, with original curved reference, two reproduced B fixes, full E2E and v0.1 compatibility; independent source/evidence review found no concrete blocker |

## 6. Milestone C first slice

| ID | Task | Status | Acceptance evidence |
|---|---|---|---|
| C-01 | Triangulate validated outside contour and cutouts | ✅ | Pure-Rust Earcut output has valid indices, non-degenerate faces, centroid/material checks, and polygonized/analytic area checks |
| C-02 | Extrude sourced board thickness and transform to GLB frame | ✅ | Metres, right-handed Y-up, board on XZ plane, mid-plane centered; f32 quantization and collapsed-face checks enforced |
| C-03 | Require explicit consent for board-shell-only output | ✅ | Default CLI/API refusal; `--allow-partial` creates a shell labelled partial even with no components |
| C-04 | Report unresolved component/model coverage | ✅ | Bounded report includes component ref/UUID/source identity and model paths; absolute paths are redacted |
| C-05 | Real-board GLB subprocess journey and local receipt | ✅ | Porcupine curves + five cutouts, independent container/accessor/mesh/closed-edge checks, deterministic output, package input, invalid-outline and no-overwrite checks |
| C-06 | Resolve actual component meshes and stable selection IDs | 🔄 | Bounded VRML assembly emits one renderable `ref:<refdes>` node with UUID metadata. Current continuation adds one/multi-component, App-filter simulation, unsupported-side, and report-cap E2Es; the full current-source verifier passed. Independent transforms, broad coverage, App import and Android selection remain open. |
| C-07 | Independently reviewed current-source local gate | ✅ | Independent static review found no remaining code blocker; all ten local gates and 109 Rust tests passed. See the exact receipt and post-run narrative notes below. |

## 7. Ownership and delivery

Main owns requirements, integration, tracker, and final QA. The implementation and review agents were limited to scoped CLI work; they shared the checkout and preserved existing edits. Sibling repositories remain read-only.

Current execution boundary (2026-10-10, latest owner steering): implementation and binary delivery remain inside the CLI checkout. CLI issues and its repository-linked Project may be updated. A feature-branch PR push is authorized only after actual current-source local QA/E2E and independent review pass. No Actions, releases, binary uploads, direct-main push or sibling edits. Merge remains a separate current-head decision. Preserve historical remote state below.

The private repository is TokitoAI/tokito-cli. PR #8 was merged to main; its historical recorded remote head was 102e4adb069779ae4a4cc70f4dc8713b34688400. NAVANEETHVVINOD was verified to have Admin access; no invitation was sent. The CLI-specific roadmap at https://github.com/TokitoAI/tokito-cli/projects is linked to this repository; the organization-wide Tokito ecosystem project remains untouched. The roadmap tracks issues #1-#7, topology follow-up #9, resolver #12 and parity #13. PR #11 was later merged as recorded below. The current selector/help/assembly continuation is separate local work; any future delivery requires its own feature branch and PR after current-source verification and independent review. No main push or Actions run is included.

## 8. Milestone B acceptance

- The physical outline report uses source millimetres and Y-down coordinates, centerlines rather than stroke/margins, and source locators/UUIDs.
- Join tolerances and work limits are bounded and declared.
- Supported line/arc/circle topology rejects crossings, overlaps, invalid tangencies, degeneracy, branches, open chains, and ambiguous nesting.
- Real boards demonstrate curved outlines and cutouts. Copper routing arcs are not treated as physical-outline evidence.
- Existing inspection, CSV, package schema, and historical SVG bytes remain compatible. Invalid topology does not prevent an integrity-valid inventory package from being imported or verified.
- Geometry reference comparisons pass within disclosed coverage: RP2040 paired profile, unchanged original LumenPnP PCB/profile and direct SVG vector geometry, preserved reduced extract, and NFC negative reference. Full current-source local verification passes.
- GLB or solids are not generated before this topology gate.

## 9. Verification history and current receipt

The newest timestamped `verification.json` under `target/delivery/local/` is the authoritative current-source receipt. Older receipts below are historical and retain the input scope and result recorded at their own run. The current full gate and its exact counts are summarized at the end of this tracker after the receipt is created.

An initial default-sandbox verification attempt failed at the format gate before checking source: Cargo could not canonicalize the CLI path and returned Access is denied (OS error 5), so no receipt was created. The same local verifier was rerun with authorized filesystem access. That historical B audit reproduced and fixed two public API failures: distant near-parallel finite segments were rejected, and invalid reports retained certified geometry. All nine gates passed with 92 Rust tests (51 library, 8 engine API, 1 GLB API, 11 mesh API, 9 resolver API, 12 topology API). Receipt: `target/delivery/local/20261009T231642Z-6ddbef64fd4e454eba5e5538d36b0221/verification.json`. Binary: 1,567,232 bytes, SHA-256 `1454f726f0d4f16701ab34935a4ac539d1cfe8ed3d6d3701078e6991e5e6fd9b`. At that run, all 105 input fingerprints and the delivered binary hash/size matched. Later changes are captured by the newest receipt.

The unchanged original official LumenPnP PCB/profile now supplies an eight-entity valid curved reference. Maximum canonical endpoint error is 0 mm; maximum centre error is 1.0565238672857966e-6 mm. Direct actual SVG path/circle comparison has maximum geometry error 1.4142142622039706e-6 mm against a 5e-6 mm tolerance and rejects deliberately corrupted radius/sweep outputs. This is source-frame vector geometry, not raster rendering or component-model qualification. The gate evidence is directly traceable in `src/topology.rs`, `tests/topology_api.rs`, `tests/geometry_e2e.py`, `tests/compat_v0.py`, `docs/PCB_TOPOLOGY.md`, and `docs/PCB_FIXTURE_CORPUS.md`.

The earlier C-06 source passed all nine local gates with 90 Rust tests (51 library, 8 engine API, 1 GLB API, 11 mesh API, 9 resolver API, 10 topology API). Receipt: `target/delivery/local/20261009T220148Z-4a0a128be9404747bbf8b3c55d7ed3da/verification.json`. Binary: 1,566,720 bytes, SHA-256 `7d71ab51da56f52ad901fa676abf5bf651be4a513aac4fbc8d3cf73575700eb6`. All 98 recorded inputs matched immediately after the run. At the C-06 delivery, subsequent tracker and App/API documentation updates were narrative-only and code/build/test inputs matched. The later B audit now changes source and tests and requires its own fresh receipt. Independent source reviewers rechecked the resolver and mesh hardening and found no remaining concrete defect. Their reviews do not substitute for these executed tests.

The real-board model journey resolves six R0603 assets from explicit KiCad8 roots, verifies hashes and source ownership, decodes the real WRL profile and bounds, tests exact/one-byte-under budgets, retains package lookup and rejects malicious paths. This is local asset/mesh qualification, not component-pose, selection, mobile or KiCad rendering equivalence.

The following paragraph records the **previous baseline** and its then-current fingerprints; it predates C-06 source changes.

The earlier historical baseline Windows x64 receipt is `target/delivery/local/20261009T211314Z-570c977da3604eb4bde5e08b517702e9/verification.json` (verified 2026-10-09 21:13:14 UTC). It records eight passing gates: formatting, the Rust test gate, Clippy with warnings denied, release build, four-board CLI E2E, geometry E2E, GLB E2E, and v0.1 compatibility. The run output reported 69 Rust tests (50 library, 8 engine API, 1 GLB API, 10 topology API). The delivered `tokito-cli.exe` is SHA-256 `0362eeb16945253eb05563893cb51f72c9be9e97661bfd5d8b2484c937471b9c`, 1,432,576 bytes. The receipt fingerprints 88 inputs; a post-run comparison found all 88 still matched before this tracker edit. The tracker now records the receipt, so its own hash is the only expected mismatch; source, tests, fixtures and other documentation remain receipt-matched. No GitHub Actions or remote repository mutation was performed.

The geometry E2E checks four cases: the intentionally open NFC curved outline; the valid RP2040 straight outline; the reduced LumenPnP curved profile including component-owned circles; and Porcupine SVG geometry/identity and viewport enclosure. The NFC case matches 22 entities with maximum endpoint error 2.842170943040401e-14 mm and maximum arc-center error 2.1817713396709406e-6 mm. RP2040 matches all 644 profile entities with 0 mm endpoint error. The reduced LumenPnP extract matches eight profile entities with 0 mm endpoint error and maximum circle/arc center error 1.0565238672857966e-6 mm. This is not full upstream-board equivalence because neither raw upstream artifact was downloaded.

This Windows x64 receipt does not establish Linux/MSRV, hosted CI, full KiCad plot/DRC conformance, fabrication validity, complete component models/selectors, STEP, Desktop/MCP integration, phone import, or AR accuracy. The reduced LumenPnP fixture validates only represented geometry reconstructed from indexed records against its paired reconstructed profile.

## 10. Error and blocker register

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
| B-ERR-013 | Two closed contours with a 1.5× join-tolerance parallel gap could be certified | Ambiguous parallel contacts carried no point and the validator repeated the point-only predicate | ✅ Validator rejects ambiguous contacts with no allowed point; focused public regression and full local gate pass |
| B-ERR-014 | Valid stepped polygon rejected because distant near-parallel edges have close infinite support lines | Ambiguity check omitted finite projected interval proximity | ✅ Public regression reproduced before fix; finite interval proximity filter passes alongside genuine near-parallel rejection in fresh full verification |
| B-ERR-015 | Unknown/module Edge.Cuts invalidation left derived bounds/contours in API/JSON | Late diagnostics were appended after the geometry builder; invalid nesting also retained contours | ✅ Derived geometry discarded while primitives remain; API/JSON cases for unsupported sources, disconnected/nested contours pass in fresh full verification |
| B-ERR-016 | First fixed-source topology rerun hit linker LNK1104 | Reused Windows output could not replace topology test executable; exact OS cause unknown | ✅ Failed compilation preserved; fresh full nine-gate run and all 12 topology API tests pass |
| B-ERR-017 | Full runner launch failed before any gate started | Assumed Program Files PowerShell path was absent; current host is bundled PowerShell 7.6.5 | ✅ Located actual PowerShell 7.6.5 runtime; full nine-gate run passes in current host |
| B-ERR-018 | Default-sandbox full verification stopped at format | Cargo path canonicalization returned Access is denied (OS error 5); no receipt was created | ✅ Same local verifier rerun with authorized filesystem access; all nine gates and 92 Rust tests pass in receipt `target/delivery/local/20261009T231642Z-6ddbef64fd4e454eba5e5538d36b0221/verification.json`; environment boundary, not a source-code failure |
| C-ERR-001 | Preliminary scoped build commands could not locate Cargo; model E2E then reached an older binary and failed unknown-command | Rust bin directory absent from that shell PATH | ✅ Installed toolchain path supplied; current build, subprocess journey and full nine-gate verification passed. |
| C-ERR-002 | Resolver test reruns and first full revised-source gate hit Windows LNK1104 (tests exit 101) | Cannot replace existing test executable; exact locking source unknown | ✅ Failed run produced no receipt. Fresh isolated full nine-gate run passes; earlier build/evidence preserved. |
| C-ERR-003 | Case-folded root boundary and malformed-variable classification were insufficiently conservative | Path comparator ignored case; lookup preceded variable syntax validation | ✅ Exact component boundaries and malformed-path diagnostics; regressions, full gate and source re-review pass. |
| C-ERR-004 | Mesh parser errors could echo a source token; CLI help omitted budget options and implied model resolution changed GLB completeness | Unbounded token text in error formatting and incomplete help | ✅ Fixed help, 256-byte token cap and static redacted errors; tests, source re-review and full gate pass. |

## 11. Implementation evidence

Key CLI files include src/document.rs, src/lib.rs, src/cli.rs, src/geometry.rs, src/topology.rs, and src/glb.rs. Tests include tests/engine_api.rs, tests/glb_api.rs, tests/topology_api.rs, tests/geometry_e2e.py, tests/glb_e2e.py, tests/compat_v0.py, and tests/e2e_cli.py. Source and reference fixture provenance is in docs/PCB_FIXTURE_CORPUS.md; API, topology and GLB limits are in docs/CANONICAL_ENGINE.md, docs/PCB_TOPOLOGY.md, and docs/GLB_EXPORT_ARCHITECTURE.md.

The current reader is bounded and partial. Unknown or unsupported outline features must remain diagnostic and block certification; no API field means complete KiCad interpretation or fabrication readiness.

## 12. Completed milestone history

Milestone A delivered the importer, UUID-backed inventory, partial SVG, Tokito package v0.1, public canonical model, and local verification. Milestone B delivered supported physical outline validation, topology and cutout classification, exact SVG compatibility, and pinned positive/negative Gerber geometry comparisons.

The earlier-repository recovery bundle remains under target/recovery/github-before-recreation-20261009T150910Z. Old PR #9 and its failed artifact upload are historical; they are not current checks in the replacement repository.

## 13. Next work

This execution advances C-06 with secure offline model capture, CLI inventory, a strict real WRL mesh profile, and an implemented bounded GLB assembly path. One renderable `ref:<refdes>` node carries the footprint UUID and source ordinal; model hash and part ordinal belong to each primitive. The current continuation adds selector/accessor and bounded-diagnostic regressions; all ten local gates passed and independent static review found no remaining code blocker. The dataset owner supplied a newer Ultimainboard sample at c2bdf2369e13b71aaa8f13a91a23637faf339813; committed metadata is checked, but binary/model-pose/global-registration qualification is incomplete. All dataset boards are intended targets, with separate per-board gates. Next are independent asymmetric component-pose references, mounting-plane/frame comparisons, broader real-board model coverage, and App importer acceptance. The refreshed App/dataset boundaries are recorded in ECOSYSTEM_CONTRACT_MAP.md. Confirm KiCad 10.0.7 as reference baseline. Preserve historical milestone letters; project discovery/parity work is a later subtask rather than silently renaming milestones D–H. Keep Desktop/App, STEP and schematic-to-PCB edits deferred.

### Current C-06 task ledger

| Task | Owner | Dependency | State | Evidence / next gate |
|---|---|---|---|---|
| C-06a baseline | Main | A/B | ✅ | Earlier eight-gate/69-test receipt; feature branch and edits preserved |
| C-06b model sample | Research + Main | Exact library source | ✅ | Pinned official R0603 asset/hash/license; six UNO references; declared local-mesh gate passes; world-pose evidence still separate |
| C-06c resolver | Scoped worker + Main | Approved roots | ✅ | src/model_resolver.rs, nine API tests plus root-boundary unit test, CLI/subprocess tests; source re-review and full local gate pass |
| C-06d mesh adapter | Scoped worker | Pinned WRL + units | ✅ declared profile | src/model_mesh.rs, eleven API tests and real mesh CLI inspection pass; Transform/Inline/general VRML and STEP remain unsupported |
| C-06e transforms | Main | Reviewed pose semantics + explicit copper planes | ◐ | Public Rust point- and parsed-mesh placement APIs are wired to bounded VRML GLB assembly. Asymmetric front/back, winding, normal, and stackup-plane derivation tests pass. Independent export comparison remains open. |
| C-06f selectable nodes | Main | C-06e | ◐ | One renderable `ref:<refdes>` node carries unique footprint UUID/source ordinal; each mesh primitive carries its model hash and part ordinal. The inspected App filter selects by exact refdes node name; UUID is provenance, not its current selector. CLI filter simulation is tested; App acceptance remains open. |
| C-06g partial rules | Main | Existing shell | ✅ | Default refusal and explicit --allow-partial preserved; model lookup does not alter export |
| C-06h full QA / delivery (PR #11) | Main + reviewer | Integrated files | ✅ historical | The then-current nine-gate/90-test receipt and independent reviews supported PR #11; GitHub reports it merged at 2026-10-10 01:26 UTC as `7700eb7762ceaa8e82d1cc08d2abb1e62e33f564`. This does not verify the later local selector/help continuation. |

## 14. Active delivery board — TOKITO-PCB-ENGINE-002

This table is the current work plan. Completed means evidence has been inspected;
it does not extend the scope of a milestone or prove an untested integration.

| ID | Work item | State | Evidence, dependency, or next acceptance gate |
|---|---|---|---|
| G-01 | Verify the existing goal claim | ✅ | Goal 001/Milestone B was attained only for its declared canonical-engine and topology scope. The complete selectable 3D, Desktop, App, and schematic-to-layout goal is not attained. |
| G-02 | Correct local verification provenance | ✅ | The frozen implementation passed all ten gates; receipt fingerprints and binary hash are recorded below. Only this tracker and the handoff narrative were updated afterward. |
| G-03 | Exercise the release executable through real local subprocesses | ✅ local CLI | The full run exercised the release executable and saved its receipt under `target/real-cli-e2e/20261010T025713Z-16050e4c80c44904aab2e1df0f5be9bd/`. GUI/CAD-viewer/phone testing remains unavailable in this session. |
| G-04 | Record PR #11 delivery state | ✅ historical | PR #11 was locally verified, independently reviewed, pushed and merged as `7700eb7762ceaa8e82d1cc08d2abb1e62e33f564`. No Actions ran. This does not cover current uncommitted edits. |
| G-05 | Obtain current-head review and merge decision | ✅ historical | The owner authorized the post-verification merge of PR #11. This does not waive independent review or current-head checks for later PRs. |
| K-01 | KiCad CLI behavior reconnaissance | ✅ limited | Local KiCad CLI reports 10.0.5. Version/help was inspected for SVG, GLB, and POS, and one pinned Arduino Uno/U3 GLB capture was exercised. This documents limited 10.0.5 syntax/artifact behavior, not 10.0.7 parity. |
| K-02 | Pin and differentially test 10.0.7 behavior | ❌ blocked | Requires a designated 10.0.7 executable and pinned reference fixtures. Start with a bounded POS or SVG option subset, then compare parsed output, errors, units, origin, and side transforms. |
| C-08 | Parse declared stackup and derive outer-copper planes | ✅ bounded integration | Typed source-order records and strict copper/core/prepreg plane derivation with positive/refusal tests are implemented. VRML assembly requires its separation to match the board shell. Independent KiCad-export comparison remains open. |
| C-09 | Validate transforms against an independent mechanical/KiCad export oracle | ◐ | A repeatable KiCad 10.0.5 asymmetric Arduino Uno/U3 GLB capture and FreeCAD import smoke test exist in `tests/kicad_glb_reference.py` and `docs/KICAD_GLB_REFERENCE.md`. It is not 10.0.7 parity, does not cover non-zero model-local transforms or back-side placement, and does not yet compare component mesh geometry in Tokito's frame. |
| C-10 | Assemble qualified component mesh geometry into GLB | ◐ | `--assemble-wrl` joins the shell with a caller-approved bounded local VRML subset. It preserves explicit partial coverage for every missing/unsupported model. STEP and KiCad GLB mesh parity remain open. |
| C-11 | Emit deterministic UUID/refdes node and selector metadata | ◐ | Each renderable GLB node is named `ref:<refdes>` and carries its unique footprint UUID/source ordinal; model hash and part ordinal are on the mesh primitives. The current App selects by exact node name, so UUID remains a provenance join. CLI selection-rule tests do not replace App importer acceptance. |
| H-01 | Define schematic-to-physical handoff | ✅ design evidence | Desktop snapshot `34a5f55c8425ff7ef53efd33bc6b6e36d0e877f5` provides stable symbol UUIDs, PinKey identities, connectivity and optional footprint references. Its schematic coordinates are not physical layout coordinates. |
| H-02 | Implement or import an authored physical layout | ☐ | Requires outline, layers/stackup, placed footprints, side/rotation, routing and provenance. A schematic-only input must be rejected for physical 3D. |
| E/F-01 | Agree Desktop-to-App package transport and import contract | ❌ blocked | The App snapshot accepts a completed assembly context and GLB manifest but has no board-layout resource/import bridge. Owners must reconcile authorized Cloud transport with optional local immutable-bundle language. |

### Current error and blocker register

| ID | State | Meaning and resolution path |
|---|---|---|
| ERR-005 | Active limitation | No callable GUI Computer Use or Android/ADB target is available in this session. Direct CLI subprocess E2E is run; CAD-viewer, Desktop, phone, AR, and visual overlay acceptance are not claimed. |
| ERR-006 | Active limitation | Installed KiCad CLI is 10.0.5, not the proposed 10.0.7 baseline. Its help output is useful syntax evidence only. Install or nominate 10.0.7 before a parity claim. |
| ERR-007 | Resolved for PR #11; new work pending | GitHub confirms PR #11 merged. The current selector/help continuation remains uncommitted and unpushed; local verification/review passed; delivery still requires a separate feature branch/PR. |
| ERR-008 | Product blocker | The Desktop schematic and App assembly-context contracts are not a physical-layout handoff. Define and validate that separate contract before promising schematic-to-3D conversion. |

## 15. Significant transitions

- 2026-10-10: this local continuation is progress: acquired verified unchanged official curved reference files, reproduced two B public failures, fixed finite-segment ambiguity and invalid derived geometry, obtained independent source/evidence review, and passed all nine current-source gates with 92 Rust tests. All 105 receipt fingerprints matched before later narrative records. No push/PR/release/Actions/settings/sibling operation occurred in this continuation.

- 2026-10-10: a fresh local verifier run first stopped at the format gate under the default sandbox (Cargo path canonicalization returned Access is denied, OS error 5; no receipt); rerunning the same verifier with authorized filesystem access passed all nine gates and 92 tests. Current receipt: `target/delivery/local/20261009T231642Z-6ddbef64fd4e454eba5e5538d36b0221/verification.json`. Recording it changed only this tracker and PERSONAL_AI_HANDOFF after the run; implementation/test inputs remain matched.

- 2026-10-10: pushed reviewed source commit `5db49481ce62e859d13cf0504c75f6f6d1124add` to PR #11; verified its actual remote head and empty hosted-check list. Updated CLI issues #2/#4/#5/#12/#13 and verified the CLI-specific Project statuses. Main remained `17f74a3f94cf6b901ef4329cbcc3322fae6cadbb`; no merge, Actions or sibling changes. This later narrative delivery record does not change tested source.

- 2026-10-10: refreshed native App/API sources read-only, documented the mobile importer gap and project/design context-revision conditions, and checked newer Ultimainboard sample metadata. No device/deployed API test or CAD binary rehash was performed. All source/build/test receipt inputs and the delivered binary independently matched before feature PR delivery; only narrative documentation changed after the nine-gate run.

- 2026-10-09 22:26 IST: goal created; Milestone A baseline rerun passed.
- 2026-10-09 22:31 IST: geometry audit and public regression completed.
- 2026-10-09 23:08 IST: independent review drove the stable arc and unsupported module-outline fixes.
- 2026-10-09 23:20 IST: archived SVG goldens, analytic geometry checks, and the full local gate passed.
- 2026-10-10 (earlier scope): the board-shell-only GLB slice passed the Windows gate and remains partial; component models/selectors and complete Milestone C remain open.
- 2026-10-09: private replacement repository and collaborator access verified; PR #8 merged; CLI roadmap linked to the repository Projects tab; issue #9 tracks the audited topology fix. The organization-wide ecosystem project remains unchanged.
- 2026-10-10: added the reduced LumenPnP profile extract and checks for eight line/arc/circle entities, component-local circle transforms, UUID ownership, and SVG output. The full local Windows gate passed: 69 Rust tests and all eight verifier steps. A read-only evidence review found no remaining unsupported fixture or test claim. At the first post-run receipt comparison only the tracker had changed; later documentation edits are also newer than the receipt.
- 2026-10-10: documented command-by-command KiCad 10.0.7 parity (proposed baseline) and the current local ecosystem handoff. Read-only agent reviews confirmed that Desktop snapshot has schematic exports but no routed PCB contract, that the App snapshot has twin-manifest schemas but no board-import bridge, and that `tokito-app-dataset` is not available in the local workspace. These snapshot findings do not represent the friend's newer unpinned changes. No code tests were rerun for the documentation-only update.
- 2026-10-10: reran the full offline current-source verifier after the documentation updates. Formatting, 69 Rust tests, Clippy, release build, four-board CLI E2E, geometry E2E, GLB E2E and v0.1 compatibility all passed. The 88 hashed inputs matched before the tracker was updated with this receipt; the tested binary hash and size remained unchanged. No remote operation occurred.

Last updated: 2026-10-10. The frozen selector/help/assembly source passed all ten local gates and 109 Rust tests in `target/delivery/local/20261010T025719Z-b99c34e60c5b4cfbbdd9550e93204a71/verification.json`. Independent static review found no remaining code blocker. This tracker and PERSONAL_AI_HANDOFF.md were updated afterward to record actual results; their narrative hashes therefore differ from that receipt. Implementation, scripts, fixtures, tests and the delivered binary must still match the recorded fingerprints before delivery. Milestone B's declared local acceptance is satisfied. The wider selectable component digital-twin goal remains incomplete and active.

- 2026-10-10: implemented pure-Rust point- and parsed-mesh placement APIs from reviewed KiCad STEP-export behavior. Both take front/back copper-plane values as explicit inputs; canonical source parsing derives supported copper/core/prepreg stackup planes. Focused tests cover asymmetric poses, frame reflection, signed scale parity, winding, recomputed normals, simple and four-layer stackups, and refusal paths. Independent exported-geometry comparison remains open.
- 2026-10-10: wired a bounded local VRML assembly path to the GLB exporter. It accepts only caller-approved roots and flat triangle VRML, requires unique component UUIDs and consistent stackup/body thickness, emits UUID-backed nodes, and fails closed into explicit coverage for all other model conditions. A pinned one-resistor release subprocess E2E passes. STEP and KiCad GLB parity remain open.

## Current STEP geometry investigation — 2026-10-10

| Work | Owner | Dependency | State | Evidence / next action |
|---|---|---|---|---|
| Negative-torus candidate | Experiment worker + main | Real 57.step | Recovery observed, quality failed | Eight face IDs recovered; nearest-reference surface discrepancies block admission |
| Same-part controls and vertex comparison | Main + independent reviewer | Candidate sampler + FreeCAD | Executed | Affected vertex maximum 0.68639 mm; unchanged controls 0.00350 mm; see DATASET_STEP_QUALIFICATION.md |
| Original torus inverse regression | Experiment worker + Main | Reader inverse diagnosis | Analytic tests passed | Five tests and seven samples; no STEP trim support or production admission |
| Current QA | Main + reviewer | Frozen implementation | Fifteen gates passed | Receipt 20261010T050440Z-26878073a5b347fbb11e7427163d927e; corrected real BOM runs passed; later narrative clarifies anonymous/placeholder identities only |
| GitHub delivery | Main | Full acceptance and review | PR17 merged | Fifteen local gates, delivered-binary dataset runs and independent review; merge f935b75 matches tested head dae0a1a; no Actions |

Current implementation: the model-assignment manifest and assigned partial-WRL export are implemented on `codex/component-model-qualification`, awaiting the current full local verification, independent review and a separate PR. The manifest preserves all source components, binds exact source identities to the PCB SHA-256 and checks model bytes, explicit units and transforms. It does not certify an engineering body match, pose, physical revision or complete board. See `MODEL_ASSIGNMENTS.md`.

Next approved work: issue #20 independent per-dataset model/pose/geometry evidence, alongside issue #19 STEP surface/trim qualification. The new local branch is codex/component-model-qualification, based on merged main. Identity reconciliation is delivered; complete component CAD, independent pose/geometry acceptance, App integration and full KiCad parity remain incomplete. This goal remains active.
