# Implementation gates

## Inputs available now

- The CLI carries six pinned real fixtures: SparkFun Basic, community Pico/UNO, NXP MCXA ESC, the official KiCad RoyalBlue54L NFC demo, and the Porcupine routed PCB. These add a real plotted curved outline and a valid board with five physical cutouts plus 607 routed copper arcs. The NFC outline has measured gaps and is expected to fail closed. See [provenance and counts](PCB_FIXTURE_CORPUS.md). The supplied VTOL project remains supplemental, not reference truth. There is no GLB/STEP corpus or certified Tokito target-board package.
- The old physical checkout contains an Allegro `.brd` source and supporting NXP files, but it is a separate ignored local dataset with source/licensing/provenance constraints. It is not an accepted KiCad parser fixture.
- `devboard_ar_dataset` has no actual PCB, photo, or label samples; its GLBs are placeholders.

The implemented outline validator is documented in [PCB_TOPOLOGY.md](PCB_TOPOLOGY.md). It is an analytic, bounded topology check for a supported subset, not full CAD/DRC conformance. Custom-pad rendering, complete model transforms, solids/triangulation, model identity and manufacturer validation remain future work.

## Current executable slice

`tokito-cli inspect` reads only KiCad PCB format dates from `20211014` through the reviewed `20260206` and reports the fields it recognizes. It rejects older formats (which can use legacy `module` records) and unreviewed newer versions. `tokito-cli export placements` writes reference, placeholder status, value, library, side, source-frame XY/rotation, UUID, pad/model counts, DNP, and position-exclusion columns. Repeated graphics placeholders such as `G***` remain UUID-identified inventory rows; duplicate component references and missing or duplicate UUIDs remain errors. The reader consumes no more than 256 MiB from disk and enforces token, node, and nesting limits; parser memory can exceed the source-byte ceiling. Unknown or malformed source constructs can still be outside this prototype's understanding; no faithful-geometry claim is made.

The real fixture tests preserve source identities and inventory counts. Partial SVG imports supported outlines, embedded graphics/text, simple pads/drills, segments/arcs, vias and stored filled polygons, with explicit side/layer selection and omissions. The physical report normalizes supported line/arc/circle boundaries, classifies direct cutouts, refuses invalid topology and retains source locators. Exact-coordinate tests cover saved footprint-relative XY and absolute pad angles, including back-side import without a second reflection. SVG carries source IDs and parent UUID metadata. Fonts, complex/custom pads and unsupported drawing types remain partial.

A v0.1 package writer and native reader preserve/reimport source and validate the fixed manifest, hashes and current-source inspection/placement consistency. The package schema remains unchanged; SVG is exported separately and mobileImportReady is false. Milestone A implements a public Rust PhysicalBoardDocument API with typed source records, canonical-derived inventory and locator-selected SVG inputs. Its retained raw-node render bridge remains partial. GLB, STEP, native desktop schematic adapters, Desktop/MCP wiring and phone import remain unimplemented. See [engine API and limits](CANONICAL_ENGINE.md).

First complete Windows/Rust 1.94.1 Milestone B run passed 40 unit and 17 public API tests, Clippy, release, four-board CLI E2E, topology/reference E2E and five historical package checks. The pinned NFC reference comparison matched all 22 plotted Edge.Cuts entities with endpoint tolerance 1e-6 mm and arc-center/radius tolerance 5e-6 mm. The pre-A binary differential comparison also passes exact inspect/CSV/top/bottom SVG bytes on all four original boards after preserving legacy math outside Edge.Cuts. The complete local runner is being repeated after test and input-fingerprint updates. Use its newest receipt; do not rely on the earlier B receipt for the final tree. Windows evidence does not prove Linux/MSRV, full KiCad visual parity or device/AR acceptance. See [local delivery](CI_AND_DELIVERY.md).

## Phased implementation

1. **Format and inspection:** bounded S-expression reader, declared source-version support, board/component inventory, placement export, machine-readable diagnostics.
2. **Tokito package v0.1:** source-and-inventory directory with hashes; current data remains incomplete and is not accepted by the phone runtime.
3. **Canonical board model and 2D views:** Milestone A's public source-semantic engine and Milestone B's bounded physical outline topology are implemented locally. Final full verification and independent review are the remaining acceptance gates.
4. **Mobile mesh:** GLB only for geometry with validated outlines and sourced/tessellated models; record per-component coverage and test app import.
5. **STEP:** separate pure-Rust CAD spike; validate units, assembly transforms, topology preservation, and unsupported entity reporting before making it a capability.
6. **Desktop/MCP adapters:** after the engine and package contracts stabilize, add local Desktop and optional local MCP adapters without changing the hosted symbol catalog.
7. **Schematic-to-PCB:** only after Desktop defines physical placement/routing data and stable links from schematic UUID/pin identity to board instances.

## Integration is not yet end to end

The current desktop produces schematics and netlists but not physical board layouts. The current mobile runtime has a digital-twin asset contract but no public package-import API. These are concrete integration gaps. The CLI can establish the shared format and local artifact engine without modifying either repo; adapter and app changes require separate scoped work in those repositories.

## Old checkout cleanup inventory

The old physical checkout's tracked tree has no working-tree modifications. Its canonical physical and companion merge is preserved in `tokito-app`; the old HEAD has a migration tag there. A verified Git bundle in `recovery/tokito-physical-old-20261009/all-git-refs.bundle` preserves every local ref, including two experimental local branch histories that are not app refs. Hash-verified copies of the two untracked Python utilities and one emulator screenshot are beside the bundle.

Generated Gradle build outputs, `.gradle`, `.kotlin`, Python bytecode caches, and emulator snapshots/cache files are cleanup candidates. Keep `datasets/local`, `build/ar-assets`, `build/reference-packages`, the old `.git`, AVD definitions, untracked Python source, and all tracked source until relevant user data is transferred or explicitly retired. A deletion command for generated files was rejected by the command policy, so this note does not claim cleanup succeeded.
