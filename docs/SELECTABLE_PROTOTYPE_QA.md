# Selectable prototype QA — 2026-10-10

Local working-tree prototype; not pushed. Core and viewer output stay inside CLI ownership; sibling repositories were read-only. Independent source review found no blocking issue.

## Earlier NXP-only verification (before the Ultimaker changes)

- `cargo test --locked --offline --all-targets` passed, including 66 library tests and Rust integration tests. Four new viewer tests cover unsafe labels, content binding, geometry bounds and unsupported profiles.
- `cargo fmt --all --check` and `cargo clippy --locked --offline --all-targets -- -D warnings` passed.
- Debug build and actual `twin prototype` command succeeded on the pinned NXP dataset GLB/map/manifest.
- `tests/twin_e2e.py` passed, including prototype output, explicit partial consent, no overwrite and actual GLB embedding; prior geometry payload/transform checks remain intact.
- Real Edge browser acceptance via Playwright CLI passed: IC U11 highlight changes actual pixels; resistor R100 selects separately; reset restores original pixels; missing body BH1 is explicit; GPU surface picking returns J6; orbit and zoom change rendered pixels; fresh page content renders and selects with networking disabled.
- Browser source counts match independent calculations: 391 records, 354 mapped component meshes, 108,920 triangles, 134,860 vertices and 871 draw batches.
- The 3840 x 2160 screenshot was opened with Pillow and its PNG verified. The 1600 x 1000 selected-IC screenshot was visually inspected.

Final artifact directory: `target/prototypes/nxp-revc-selectable-v2/`.
HTML SHA-256: `2ab9673f914dabed2898fb9f45f7e091ac8a235d33128e98e15a3aa8576953f9`.
Prepared GLB SHA-256: `9113e74057ca87f3ed3910b6e8ea5989bdb54f17bbdb98fccdc8d58d71b7be5b`.
Real source geometry is reused from `target/dataset-intake/pr6-mcxn236-ab1420fb/`, bound by the original map, manifest and dataset provenance.

## Failures and corrections

- Browser automation disallowed file URL navigation, so QA used a temporary loopback-only HTTP server. The artifact itself has no server/network dependencies; fresh offline rendering was tested separately.
- The first browser test requested R1, which is absent from this map. Corrected to the actual source reference R100; the complete journey then passed.
- Evidence writing initially failed with ENOSPC. Only duplicate HTML/GLB files created in this session's prototype-v1 output were removed after matching their hashes to v2. No source, prior user work or old blocked build-cache cleanup was performed. The complete evidence journey was rerun successfully.

## Unverified and delivery gates

This is source-model inspection with source base-colour lighting and opacity, not full PBR or registered AR. Exact source-to-photographed revision equivalence, package dimensions/material accuracy, complete fitted-part coverage, PCB UUID crosswalks and Android acceptance remain unverified. The 37 bodyless entries and one unmapped renderable are reported; none are invented substitute bodies.

The new whole working tree has not run `scripts/verify-local.ps1` or a new release build. This scoped QA does not authorize a code push; full current-source delivery verification remains required. Existing merged PR receipts do not qualify these new changes. GitHub Actions remain disabled.

Ultimainboard multi-mesh adaptation and its real source E2E are now implemented. Evaluate a locally bundled, version-pinned Three.js viewer for PBR and richer interaction without changing the Rust geometry engine.

## Current Ultimaker acceptance

The `tests/source_candidate_e2e.py` journey passed against dataset PR #13 commit `aa9db20f574c4ba0273a5f5a03460f5cafb39cdb`, with its actual receipt at `target/prototypes/ultimaker-source-e2e-v2/e2e-receipt.json`. It verifies byte-preserved input geometry, transforms and materials, independently compared bounds, all 574 identities, 310 body groups, and explicit missing/unknown coverage. Browser rendering reports the independent expected 712,527 triangles, 125 line segments, 811,884 positions and 841 draw batches.

The shared browser journey is being rerun for both source profiles. Current delivery requires a fresh whole-tree local verifier receipt including both dataset gates and the source-candidate gate. Earlier 66-test results and merged-PR receipts do not qualify this expanded source. Browser screenshots and local receipts remain under ignored `target/`; consult the actual newest receipt for results, not an older narrative claim.

Ultimaker browser acceptance passed: U1 visible highlight, C74 multi-mesh visible highlight, restored colours, missing U9 message, GPU picking (R71), orbit/zoom, fresh offline loading and 3840 x 2160 screenshot. NXP R100 is occluded in the reset camera view; its selection identity is checked separately from the U11 visible-pixel highlight. An initial overly broad pixel assertion was corrected to reflect that visibility distinction. The fresh full verifier found a Clippy test-module ordering error; the test module was moved after implementation items without changing behavior, and full verification must be rerun.
