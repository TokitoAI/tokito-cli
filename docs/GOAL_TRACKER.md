# Tokito Native PCB Engine and Selectable 2D/3D Digital Twin

## Current execution checkpoint — 2026-10-11

Active CLI: `D:\kannan\projects\tokito ecosystem\tokito_cli`. The complete
ecosystem copy passed full-file SHA-256 and 18-repository Git-state comparisons;
the old F: copy remains because automatic review blocked its removal. Use D:.

| Priority | Owner | State | Actual evidence | Next gate |
|---|---|---|---|---|
| ESP8266 assembly | CLI / independent reviewer | Source inputs and positioned candidate meshes implemented; assembled GLB incomplete | 17 model journeys, 34 native identities, 62 bodies; 20 Altium and 34 STEP unit tests; full local suite passed | Reconcile model 11 bounds/surface evaluation, contour topology and independently compared world placement |
| NXP improvement | CLI / dataset owner | Latest PR #21 payload materialized and hash-checked | PR head `de945d0d36a6132e28e247d4f566f2e300301c14`; 5,250,580-byte GLB; actual CLI inspect succeeds | Resolve 23 placeholder bodies and photographed layout/population matching |
| AR and YOLO integration | App owner / CLI | Deferred until asset and identity gates | Existing App coordinate/revision/expiry contract; candidate dataset mappings | Verified identity crosswalk, accepted importer and physical-device evaluation |
| Documentation and PR delivery | CLI | Verified checkpoint pushed to PR #29; main unchanged | 34-gate receipt, independent review, exact demo hashes and current CLI inspect/select | Review/merge decision; full product qualification remains open |
| Afan handoff | CLI / App owner | Plan retained; final handoff incomplete | `AFAN_PROTOTYPE_HANDOFF_PLAN.md` | Verified assets, commands, contracts, limitations and merge sequence |

Delivered code/model checkpoint: commit `74bee98ca07bdcc75c93ae7b9658542808654968`
on [PR #29](https://github.com/TokitoAI/tokito-cli/pull/29). The
[demo bundle](DEMO_BUNDLE.md) contains ESP32/Ultimaker/NXP baseline models with
23/310/354 selectable bodies or groups. ESP32 U3 remains missing; Ultimaker's
multi-mesh groups need App import work. The latest NXP PR #21 asset is a separate
intake. No assembled ESP8266 board or registered AR is delivered.

Current verification: `target/delivery/local/20261010T203350Z-8f30e028a04547c2a052b9dc6dd1b9a8/verification.json`.
All 34 local gates passed; the bundle, guides and this progress update were
reviewed separately after that receipt. They are not claimed as its inputs.

No schematic-only physical PCB conversion, complete ESP8266 assembly or phone AR
completion is claimed. Sibling repositories remain read-only. Hosted Actions are
not used. These current rows supersede older snapshot states below without changing
their historical execution evidence.

## Current priority — dataset pipeline before App integration (2026-10-10)

The active goal remains **TOKITO-PCB-ENGINE-002**; it is not complete. The user now requires the dataset pipeline to be implemented and tested before connecting Tokito App. App, API and dataset source remain read-only. Model accuracy requires geometric evidence, not a texture-resolution label.

Delivery baseline: PR #21 and [PR #25](https://github.com/TokitoAI/tokito-cli/pull/25) are merged; main merge `49edb46e2173deb626cd875712cc197861acd546` has the exact tested tree of head `6620e9730388b603c4821ac18f0878e42ea04326`. All 22 local gates passed at `F:\TokitoCliDelivery\20261010T120942Z-a2b01b60e3574d0aa28c46c193666782\verification.json`; independent review found no blocker. PR #27 is closed as a same-head duplicate. New external-binding and offline selectable-prototype work is local and requires its own verification before delivery. See [prototype](SELECTABLE_PROTOTYPE.md).

| Order | Task and owner | State | Evidence / next gate |
|---|---|---|---|
| 1 | CLI + evidence review: audit dataset main and open PRs | Complete for inspected pins | Main `257ea7e6`, PR #6 `5283bc0`, PR #7 `ae27af8`, PR #4 `ca65648`; refresh before adopting later updates |
| 2 | Rust CLI: prepare selectable dataset twin | Passed for pinned NXP asset | 354 isolated selectable component meshes; geometry payload and transforms preserved; one unmapped mesh reported |
| 3 | CLI: candidate staging, source lock and offline asset verification | Content lock/inventory/selected binding delivered; external binding final gate pending | External hash-reference adapter and real NXP E2E implemented. ESP8266 reference source: 130 staged/bound files and 17 experimental STEP decodes. Automatic materialization/admission remain open |
| 4 | CLI: all-board file/feature inventory | Git metadata primitive delivered; qualification open | Explicit commit/tree/blob hashes, LFS payload comparisons, tracked-only source inventory and missing/pointer/mismatch states. Physical board identity, complete assets and per-format qualification remain open; see DATASET_GIT_INVENTORY.md |
| 5 | CLI + dataset evidence: component crosswalk and model qualification | Open / evidence blocked | Exact board revision, PCB hash, UUID/refdes, MPN, units, transforms and quality; NXP photographed B versus design C remains unresolved |
| 6 | Rust CLI: qualified admission and accepted-update comparison | Open | After qualification, compare staged pinned inputs against accepted lock; report changes without silently replacing accepted data |
| 7 | CLI QA + independent reviewer: all-board actual subprocess E2E | Open | Per-board passed/failed/unsupported/unverified results; missing input is not success |
| 8 | CLI release: review and merge qualified pipeline PR | Open | Current-source local checks, independent review and approved dependency sequence; no hosted Actions |
| 9 | Rust Component Forge: qualified footprint generator | Open | Separate missing footprint from missing body model; manufacturer land-pattern evidence, pads/numbering/layers and independent comparisons |
| 9a | CLI behavior researcher + Rust engine: KiCad CLI export parity | Ongoing / incomplete | Study pinned command/option behavior and source boundaries; reproduce selected SVG/placement/GLB semantics in Rust and independently compare outputs. Local KiCad 10.0.5 is reconnaissance, not 10.0.7 parity |
| 9b | Component Forge researcher + Rust generator: KiCad Library Tools workflow | Architectural assessment complete; generator families open | Pin Library Tools/KicadModTree references, study package parameters, geometry/node/serialization stages, then implement and verify each Rust footprint/body family independently |
| 10 | App owner: import, colour highlighting, dialogue and registration | Deferred | Start only after dataset qualification and separate sibling-write authorization; physical device gates remain mandatory |

GitHub tasks: [dataset pipeline #26](https://github.com/TokitoAI/tokito-cli/issues/26), [selection/handoff #23](https://github.com/TokitoAI/tokito-cli/issues/23), [footprint generator #24](https://github.com/TokitoAI/tokito-cli/issues/24), and [envelope slice #22](https://github.com/TokitoAI/tokito-cli/issues/22). They are tracked in the CLI-specific Project 2, not the overall ecosystem project. Completed implementation items remain distinct from unresolved physical accuracy and admission.

Error resolved: the first full verification encountered a temporary BOM fixture filename collision. An atomic sequence fixes unique naming; the subsequent full run passed. Remaining blockers are missing engineering/registration evidence, not a demonstrated failure of the tested selector preparation.

### Ordered CLI flow

The user requires full implemented/comparison-backed #13 and #24 coverage, accurate component bodies/terminals/pads and component UUID/refdes selection. [Completion gates](KICAD_COMPLETION_GATES.md) define these requirements and separate 4K visualization from CAD accuracy. ESP8266 is the next priority; [intake evidence](ESP8266_SOURCE_INTAKE.md) distinguishes the published reference corpus from an unverified photographed variant and unproven assembly.

Dataset snapshot → verified materialized inventory/source lock → exact board/component crosswalk → geometry/unit/transform qualification → existing PCB exports or `twin inspect` / `twin prepare` / `twin select` → all-board E2E and independent review → qualified handoff → separately authorized App integration.

The existing twin commands, earlier PCB exporters, and offline `dataset lock` / `dataset verify` content primitives are implemented. The current continuation adds `dataset inventory` for bounded local Git/LFS identity inspection; automated candidate materialization, admission and update comparison remain planned work. Vision detections are observations and do not establish hidden pad geometry or physical part identity. See [source-lock boundaries and commands](DATASET_SOURCE_LOCK.md).

Two research tracks feed this flow explicitly: [KiCad CLI behavior #13](https://github.com/TokitoAI/tokito-cli/issues/13) supplies command, export, transform and refusal reference cases; [KiCad Library Tools footprint generation #24](https://github.com/TokitoAI/tokito-cli/issues/24) supplies package-parameter, pad, layer, courtyard and serialization reference cases. Each selected behavior needs an original Rust implementation and an independent comparison before acceptance. Dataset PR assets are separately pinned candidates, never implicit accepted replacements for main. Full KiCad CLI and footprint/library parity remain incomplete.

## Earlier goal history

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

## Component Forge — first evidence-gated slice (2026-10-10)

This is an additive workstream under the existing `TOKITO-PCB-ENGINE-002` goal; it does not restart or complete the native goal. The goal API currently reports that goal as `active` and does not expose objective editing. This tracker and CLI issue #22 hold the current task status.

| Task | State | Evidence / next action |
|---|---|---|
| Read and challenge the personal-AI plan | ✅ | Accepted the staged architecture, rejected the implied broad all-family delivery as unsupported by current evidence, preserved the existing repository and issue history |
| Classify the real MCXN236 U11 gap | ✅ | U11 has existing pads 1–9 with custom geometry and no model reference; it is a model gap, not a missing footprint |
| Research generator and CAD references | ✅ | Pinned KiCad generator/library references and reviewed Truck, ruststep, Python CAD, image-registration, and optional part-lookup roles; no new production dependencies admitted |
| Strict package/body evidence record | ◐ | Current request binds board hash and unique refdes/UUID; BOM/manufacturer fields are explicitly unverified source claims. Add a richer tolerance-aware package spec later |
| Nominal candidate envelope and generic selectable GLB | ✅ bounded | Deterministic 8-vertex/12-triangle WRL; fixture E2E emits one candidate `ref:R1` GLB mesh after explicit partial consent; not an engineering model |
| Actual dataset U11 run | ✅ refusal path | Generates the 3 × 3 × 0.85 mm candidate and assignment manifest. U11 is withheld from GLB; missing canonical stackup is confirmed, and the exporter reports the combined `missing_stackup_or_outline` state |
| Footprint geometry and `.kicad_mod` serializer | ☐ | No footprint generated. Select a true missing-footprint case and recover/verify complete manufacturer land-pattern coordinates before implementing |
| U11 component-bearing GLB | ☐ | Requires a qualified same-board stackup/outline path and sourced geometry/pose evidence; never infer from the related Gerber job without an adapter and provenance check |
| GUI/CAD visual comparison | ⏸ | Not run in this session; command-level E2E does not substitute for a viewer comparison |
| GitHub delivery | 🔄 | Issue #22 and the CLI Project track the slice. PR #21 remains an open dependency. Create a dependent feature PR only after complete current-source verification and independent review; no Actions, merge, or sibling writes |

The actual dataset E2E is included as an optional local verifier gate with `-DatasetRepo` and `-DatasetBomReport`; the generic component envelope journey is in the portable default gate set. Details and evidence limits are in [Component Forge qualification](COMPONENT_FORGE_QUALIFICATION.md), with design in [architecture](COMPONENT_FORGE_ARCHITECTURE.md), schema in [specification](COMPONENT_FORGE_SPECIFICATION.md), and tool decisions in [open-source review](COMPONENT_FORGE_OPEN_SOURCE_REVIEW.md). The current full-source verification receipt remains the newest receipt under `target/delivery/local/` and is authoritative only if every recorded fingerprint matches the final tree.

## Per-component twin selection — CLI handoff (2026-10-10)

The preceding component-envelope source passed 19 local gates in the F: delivery receipt at 20261010T075121Z. The added source below requires its own current-source receipt. The verifier now runs 18 portable gates or 20 with both dataset profiles; use the newest matching receipt in the selected delivery base (F:\TokitoCliDelivery for this work).

| Task | Owner | State | Evidence / next action |
|---|---|---|---|
| Audit updated dataset PR #6 and App main | Read-only evidence agents | Complete | PR head 5283bc0 adds capture evidence and no changed CAD binaries; main remains pinned |
| Expose source revision mismatch | CLI documentation | Complete | NXP B marking differs from available C design; reviewed board-reference manifest absent |
| Audit isolated renderable selectors | Rust CLI | Implemented | Source NXP has 354 reference parents and zero direct renderable selectors |
| Prepare mesh names and manifest hashes | Rust CLI | Implemented | Focused fixture and NXP runs passed; payload and all non-name node values preserved after float-roundtrip correction |
| Exact ref/class-claim query | Rust CLI | Implemented | Unique, ambiguous, missing-model and no-match states; no positional guidance claim |
| Current-source E2E / review / PR | CLI QA + reviewer | Release gate | Full 20-gate local runner and independent review before branch delivery; no hosted Actions |
| Board-reference and PCB crosswalk | CLI + dataset | Open | Source hash, UUID/refdes, reviewed revision/targets/context and decoded bounds |
| App colour selection and import bridge | App owner | Open | Scoped App edits need permission; per-entity material restoration and device acceptance |

See [the complete CLI flow, evidence and staged AR plan](DIGITAL_TWIN_SELECTION.md). Footprint generation, exact component CAD, complete KiCad parity, App import and registered physical guidance remain unfinished.

## Deadline prototype delivery (2026-10-10)

| Task | Owner | State | Evidence / next action |
|---|---|---|---|
| Immutable Ultimaker intake | Main | Done | Dataset PR #13 `aa9db20` hash-checked GLB/map/index; coverage origin remains a source claim |
| Source adapter and multi-mesh ownership | Rust workers + main | Source E2E passed | 574 references, 310 body groups, 94 multi-mesh groups; original JSON precision retained |
| Offline selectable inspection demo | Viewer worker + main | Browser acceptance in progress | Component highlights, picking, orbit/zoom, missing-body reporting, 4K output |
| Independent review | Read-only reviewer | No code blocker found | Final acceptance requires current verification receipts |
| Full current-source verification | Main | Required before push | Fresh 23-gate local receipt plus both browser journeys |
| PR/issues/CLI Project update | Main | Depends on verified source | Feature branch only; no Actions or direct main push |
| Complete KiCad/footprint parity and physical App acceptance | CLI/App owners | Open | Issues #13/#24 incomplete; prototype reuses candidate source bodies without certifying physical alignment |

Build and delivery folders were moved intact under this checkout: `target/builds` and `target/delivery/imported-f-drive`. Previous receipts retain historical execution paths; [output mapping](LOCAL_OUTPUT_LOCATIONS.md) documents their new locations. No sibling repository was edited.

## ESP prototype priority and native unit progress (2026-10-10)

| Task | Owner | State | Evidence / next action |
|---|---|---|---|
| ESP8266 declared STEP length units | CLI main + scoped worker | Local checks passed | 17 unit tests, five subprocess tests, Clippy and independent full-row comparisons on all 17 pinned models; [scope and receipt](STEP_LENGTH_UNIT_PREFLIGHT.md) |
| ESP32 latest candidate intake | CLI main | Hash verified | Dataset PR16 pinned `07c4519c964520e1a143b5f3c059b18dd108f037`; actual GLB has 16 nodes/meshes and 15 selectors; `target/dataset-intake/esp32-candidate-07c4519/cli-intake.json` |
| ESP32 individually selectable inspection | CLI main | Local prototype passed | Actual CLI preserved source geometry/materials, selected all 15 bodies; browser passed IC pixels, diode identity, restoration, missing-model message, picking, orbit/zoom/offline and 4K screenshot. Receipts in `target/prototypes/esp32-candidate-e2e-v1/`. Nominal package shapes/dimensions/identity still unqualified |
| ESP8266 assembled selectable inspection | CLI main | Open | Existing 17 STEP sources still need qualified geometry evaluation and component/world-transform joins; unit success is insufficient |
| ESP8266 original source body recovery | CLI main | Bounded source audit passed | 34 component records match dataset extraction, 62 body candidates resolve across 17 model records / 13 IDs; duplicate transform metadata retained. Six tests, Clippy and real subprocess passed; [remaining gates](ESP8266_SOURCE_ASSEMBLY.md) |
| NXP replacement | Dataset owner | External work | User reports new model in progress; inspect its published pinned assets before replacing the current Rev C candidate |
| AR and product completion | CLI/App owners | Open | Follow accepted inspection prototypes with explicit import and registration gates; sibling repositories remain read-only |

The STEP changes are local and not yet qualified for a new push by the full CLI
delivery runner. PR29's previously verified prototype baseline is separate. GitHub
Actions remain disabled. The goal remains active; full KiCad parity is incomplete.

### ESP32 population defect confirmed

The first prototype's successful browser checks establish selection behavior, not
full population. Source photos visibly contain omitted passives. The new Rust
`twin audit-population` command reports the community-reference comparison: six
resistors versus zero rendered, six capacitors versus two, and incompatible U1/U2
label meanings. [Population correction and next gates](ESP32_POPULATION_AUDIT.md).
The v2 demo retains the existing candidate geometry and explicitly exposes incomplete
coverage; missing components have not yet been generated or physically placed.

ESP8266 embedded-byte gate is now passed: all 17 direct Rust-decompressed model
streams match extracted STEP bytes/hashes; eight experiment tests and dataset E2E
passed. Component-index semantics, face/trim completeness and assembly pose remain open.

ESP32 outline refusal was narrowed to a chaining-tolerance mismatch. Three unique
source gaps measure 0.001655551–0.009959051 mm. New explicit `validate outline
--compare-chaining` diagnostics show eight unique endpoint pairs at the published
KiCad standard 0.01 mm profile, while strict Tokito validation still refuses.
Comparison is not an export/admission gate; source coordinates remain untouched.
Eighteen topology tests, actual three-profile CLI comparisons and Clippy passed.

The explicit export chaining policy is now implemented locally: opt-in 0.001/0.01
mm tolerance, contour revalidation, connection-area accounting, curve/join error
allocation and policy metadata. 82 library tests, Clippy and actual ESP32 shell
E2E passed. All24 component model references remain withheld by the assembly
stackup-span gate (1.58 mm copper span versus1.60 mm nominal including declared
mask). Full verifier and independent review remain delivery gates; no new push.

Latest ESP32 recheck: dataset PR17 `34fe631a` contains no newer ESP32 component
mapping. The v3 demo now bundles and displays the reference BOM comparison via
`twin prototype --reference-bom`; actual CLI and browser E2E passed, including
required visible resistor/capacitor deficits and identity conflicts. Full
population remains incomplete. The arc/circle analytic error bound was corrected
after independent review; 84 library tests, all Rust integration/doc tests,
Clippy and the real-source shell E2E passed. Current changes remain local pending
the full delivery runner. See [the detailed task ledger](ESP32_POPULATION_AUDIT.md).

Native reference assembly advanced: separate typed body/copper/coating extents
fix the ESP32 nominal-thickness comparison, with malformed-stackup refusals and
a 0.840 mm independent mounted-resistor regression. Exact official KiCad9 library
assets are commit/hash pinned locally. Native Rust emits 18 separately selectable
bodies, including all six reference resistors and capacitors; six source models
remain refused. Current shell is a labelled nominal envelope and component
materials are still generic. Actual source CLI and real browser E2E passed for
the separate community-reference prototype; photographed-board equivalence is
unverified. Both research issues #13/#24 remain incomplete. Full local verifier
now includes experimental source-record/unit checks and optional pinned ESP32/
ESP8266 E2E inputs; no hosted Actions were added.

Material quality correction: native and assigned exports now preserve per-part
source diffuse RGB, alpha, emissive and solid fields with bounded deterministic
deduplication and original Phong provenance. Nine materials replace generic gold
in the new ESP32 reference v2 demo; source geometry/BIN/UUIDs remain identical.
86 library tests, Clippy, actual native E2E and real browser colour restoration /
18 selectors / offline / 4K checks passed. Earlier full29-gate verification passed
before this source change and is not reused as current evidence. Six missing/
unsupported models, patterned copper/mask/silk, full PBR/culling and physical fit
remain quality tasks. No new code push has occurred.

The material correction subsequently passed a fresh full 29-gate run:
`target/delivery/local/20261010T160138Z-4ed5e1cd8e0947dfa7afd9e41af40714/verification.json`.
Later STEP experiment changes are separate and do not inherit that receipt.

### Current handoff reconciliation

PR #11 is confirmed merged at remote head `40e60efcf32a8128446a3ab96afb559fa7858699`;
it is historical foundation delivery, not verification of current uncommitted work.
See [dataset AR handoff](DATASET_AR_HANDOFF.md) for PR #18's pinned working contract.

| Task | Owner | Dependency | State / QA | Next action |
|---|---|---|---|---|
| ESP32 central module | CLI | Official pinned WRL / source layout | 19-body local export and material-preserving selection browser checks passed | Full fresh delivery verification; missing models and photo joins |
| ESP8266 occurrence world | CLI + independent reviewer | Declared units and FreeCAD reference | Three-model bounds comparison passed at 0.05 mm; one unresolved 0.049816 mm delta | Review traversal; qualify surfaces and Altium board transforms |
| Dataset/App AR identity contract | CLI / App | Dataset PR #18 reviewed inputs | Documented; no qualified joins or phone acceptance | Materialize latest models/annotations, then establish exact crosswalks |
| Current source PR | CLI | Current local QA and independent review | Not pushed; historical receipts do not qualify it | Finish gates, then feature PR without Actions |

Model-first owner steering is captured in
[Afan prototype handoff plan](AFAN_PROTOTYPE_HANDOFF_PLAN.md): finish model/identity
quality, then App AR, then a complete source/asset/YOLO/integration/product handoff.
This document is a plan, not completed delivery. PR18's actual dense ESP32 annotation
has 28 masks per image across three images; its source component IDs remain unmapped
and training export/evaluation flags are false.

A fresh 30-gate local runner passed at
`target/delivery/local/20261010T184243Z-5a6a87804e3545f089b45443d70aa387/verification.json`.
Subsequent browser review reproduced a stale selection box after graphics context
loss. The fix clears/refuses overlay rendering after loss and adds synchronous
snapshot capture; real v9 browser execution passed nonblank colour equality,
orbit/zoom overlay changes, matching resized bitmap dimensions, offline selection,
4K rendering and context-loss suppression. That later source change does not inherit
the earlier full receipt. Fresh scoped Rust checks and full delivery review remain
separate evidence. No new source push or merge has occurred.

ESP32 switch recovery now exports21 independently selectable bodies through the
ordinary Rust CLI: exact source palette-only derivation, source-byte splice/hash
checks, saved UUIDs,4,538triangles per switch and explicit full-assembly refusal.
Actual CLI E2E and browser21-selector/material/orbit/zoom/resize/context-loss/offline/
4K journeys passed. Independent review verified hashes and retained the physical
geometry/pose limits. USB/LED exact-zero-face derivation and CP2102 model resolution
remain open; see [the population audit](ESP32_POPULATION_AUDIT.md). The full local
runner now includes the switch journey and must be rerun for this source before
delivery. No new source push or merge; sibling code unchanged.

### Official ESP32 STEP candidate face-presence work

| Task | Owner | Dependency | State/evidence | Next action |
|---|---|---|---|---|
| Bounded source shell/face comparison | CLI | Materialized official assets | Implemented; 21 experiment tests, five executable tests and Clippy passed | Extend independent geometry comparisons |
| Real model membership comparison | CLI QA | Rust probe | Passed: module 751, USB 484, switch 81 face IDs; full digests independently matched | Verify surfaces, cavities and dimensions |
| Malformed topology review | Independent reviewer / CLI | Audit implementation | Arity and duplicate-reference gaps corrected with regressions | Final scoped review |
| Parametric-context units | CLI | USB source semantics | Incomplete; 2D contexts without assigned length units refused | Explicit schema-aware handling |
| Replacement model admission / full board | CLI | Units, surfaces, transforms | Not admitted; no new populated GLB | Qualify before assembly |

See [the gate, exact scope and current evidence](STEP_FACE_COVERAGE.md). The goal
remains active; issues #13, #19 and #24 remain incomplete. No sibling write,
GitHub Action or code push occurred in this step.

### Parallel ESP8266 and ESP32 checkpoint

| Work | Owner | State | Verified evidence | Next action |
|---|---|---|---|---|
| ESP8266 source-to-model candidate inventory | Scoped worker / main | Implemented; normalized source pose inputs; assembly unqualified | 34 components, 62 bodies, 17 model records; source-bound v5 receipt; 14 experiment tests and all 62 Decimal pose comparisons passed | Verify world placement and complete surfaces |
| ESP32 exact zero-area source derivation | Main / independent reviewer | Implemented; source-reference prototype only | Exact Fraction comparison: 7 USB and 2 LED faces; 23 selectable bodies; actual browser selection/offline/4K/context-loss pass | Resolve CP2102 and photographed revision correspondence |
| Current-source Rust verification | Main | Scoped checks passed | 91 root unit tests plus integration/doc tests; root and Altium Clippy | Run fresh full local delivery gate before any code push |
| Delivery | Main | Pending | No new source push or merge | Full current-source local gate and independent release review |

ESP8266 evidence: `target/dataset-intake/esp8266-latest-f81f6f85/assembly-e2e-v5/`.
ESP32 evidence: `target/prototypes/esp32-usb-led-e2e-v2/` including hash-bound
`browser-qa-receipt.json`. The local verifier now includes switch recovery,
exact-zero-area derivation and ESP8266 candidate associations in dependency order.
No full-board, physical-fit or phone/AR completion is claimed.

ESP8266 board-stream intake now preserves 27 separate legacy property sections,
unresolved non-ASCII bytes and conflicting duplicates. Actual source E2E v8
checks the pinned stream hash, linked two-copper-layer stack and all nine contour
records; declared copper/core thickness is 0.39116 mm, not an assumed 1.6 mm.
Current experiment verification passed 15 Rust tests, formatting, Clippy and
the source + 62-body candidate E2E journeys. Numeric pose v5 received independent
review without blockers; independent Board6 review also matched all 27 sections,
offsets, properties and stream bytes without blockers. Geometry,
world placement and the full delivery gate are still incomplete.

### ESP8266 selectable prototype checkpoint

| Owner | State | Evidence | Next action |
|---|---|---|---|
| Main / assembly worker | Source-derived prototype implemented | 34 selectable IDs, 62 bodies, 405,194 component triangles; `demo/esp8266/` | Physical/surface/pose qualification, pads and drills |
| Main / independent reviewer | 35 local gates and browser acceptance passed | 20261011T023649Z local receipt; all 34 browser selections, offline and 4K | PR delivery; no Actions or main push |
| Main / documentation maintainer | Afan handover prepared | `docs/AFAN_INTEGRATION_HANDOVER.md` | App admission, YOLO identities and registration |

KiCad parity #13, footprint/library #24, physical AR, product-level STEP/Altium
and schematic-to-physical-layout remain incomplete. Goal remains active.

### Source-derived board openings checkpoint

| Owner | Dependencies | State | Evidence / review | Next action |
|---|---|---|---|---|
| Main | Pinned V4 Excellon sources | Original bounded Rust inventory implemented locally | 37 experimental Rust tests passed; actual subprocess compared 279 holes and 2 slots to Decimal arithmetic; 5 refusals passed | Validate frame, containment and overlapping openings |
| Independent PR auditor | Parser and real-file test | No demonstrated runtime blocker; two test assertion gaps corrected | Coordinate lengths and unique tool counts now checked; real-file journey reran successfully | Review substrate triangulation and normals |
| Main | Current source and complete inputs | Local verification flow extended; no new push | New drill E2E gate registered; previous 35-gate receipt does not cover this slice | Fresh complete verification before delivery |

See [the source profile and geometry gate](EXCELLON_SOURCE_INVENTORY.md).
The frozen ESP8266 demo remains solid; the separate drilled v4 candidate passed edge, area and opening-centre checks. No physical, AR, full
KiCad parity or product completion is claimed.

### Accepted NXP v3 checkpoint

391 reference records, 351 selectable bodies; U8/J10/J11 source-body replacements and J7/J14/J21 not-fitted suppression. Actual source E2E, seven refusal journeys and all 351 browser selections passed. All 41 fresh local delivery gates passed, plus a separate fresh-binary drilled-substrate E2E. Twenty source placeholders, physical geometry and AR qualification remain open. See [source evidence](NXP_REALISM_UPGRADE.md) and [Afan prompt](AFAN_CODEX_INTEGRATION_PROMPT.md).

### ESP32 source detail and documentation delivery

| Owner | Dependencies | State | Evidence / QA | Next action |
|---|---|---|---|---|
| Main | Verified 23-body source baseline and material profile | Full source detail preferred over older 19-body colour preview | Geometry/transforms/IDs unchanged; USB, switches, LED and module retained; all23selectors and4K browser passed | Physical qualification and missing U3 |
| Documentation maintainer | Actual modules/assets/contracts | README rebuilt with previews, architecture, commands and full versioned-file map | Local links and all63 artifact hashes checked | Independent release review and PR |
| Main | Current Rust source, cached build dependencies | Formatting, tests, Clippy, release and real CLI E2E passed | docs/evidence/esp32-presentation-local-verification-20261011.json | Push via PR; no Actions |

KiCad parity and qualified phone AR remain open. This is a prototype presentation
and documentation delivery; the wider engine goal remains active.
