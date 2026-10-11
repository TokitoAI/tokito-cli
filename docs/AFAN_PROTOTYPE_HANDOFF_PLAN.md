# Model-first prototype and App handoff plan

This is a work plan, not a completed handoff or product-readiness declaration.
Finish verified board geometry and selection before moving to positional AR.
Sibling App, API and dataset code remains read-only; no integration merge is implied.

## Order and completion gates

1. Finish ESP32 source-reference population and materials. Keep the native source
   layout separate from the photographed specimen; resolve the annotation identity
   conflict and missing bodies before claiming a complete specimen prototype.
2. Finish ESP8266 component assembly using verified source body associations,
   units, surface/trim geometry and placement. Do not guess absolute/local rotations.
3. Intake the latest NXP model and resolve Rev C design versus photographed revision.
   Recheck Ultimaker's latest contact/body additions and component ownership.
4. For each board, deliver actual GLB, map, hash manifest and provenance; test every
   available selector, missing-model reporting, material restoration, rotation,
   zoom and offline rendering. Independently compare bounds and placement against
   matching engineering sources. Record incomplete physical checks explicitly.
5. Only then prepare the complete integration handoff to Afan and test App import,
   board registration, detector-to-component joins and per-component AR highlighting.

## Handoff contents to prepare

| Area | Required contents |
|---|---|
| Assets | Pinned board variant, source hashes, GLB, component map, manifest/provenance, qualification and missing-model report |
| CLI | Supported commands/API, subprocess request/result/error contract, offline paths, explicit partial consent, cancellation/resource limits where implemented |
| App | Actual importer/selector contract, admitted assets, restoration of original materials, registration revision/expiry and suppression on missing transforms |
| Dataset | Exact PR/commit, LFS materialization, photo frames/units, annotation completeness, revision limits and separate detector/CAD identities |
| YOLO | Actual model artifact/version/hash, class mapping, preprocessing and coordinate transforms, confidence/abstention, measured independent evaluation and device performance |
| Validation | Current source/build fingerprints, local CLI and browser receipts, independent geometry comparisons, supported physical views and measured phone alignment |
| Merge | Explicit scoped integration branch/PR, compatible schema versions, ownership and test sequence; no wholesale copying of sibling repositories |
| Product roadmap | Remaining model coverage, geometry/material quality, exact identities, native packaging, Desktop/MCP/API integration and physical-device acceptance |

Do not assume a YOLO model is trained or deployment-ready from photo labels.
PR #18 dense ESP32 labels explicitly mark training export/evaluation false and
component source IDs unmapped. See [the pinned handoff interpretation](DATASET_AR_HANDOFF.md).
Detector boxes/masks are image observations; they require calibrated frame transforms
and accepted identity correspondence before they can target a mesh in AR.

The primary runtime remains local on the phone. PC preparation is currently accepted.
Desktop calls the reusable Rust engine or a versioned local CLI process for existing
physical layouts; schematics need a separate validated layout/placement/routing and
stackup stage before physical 3D. MCP/API integration must use their actual current
contracts, not inferred endpoints. Full KiCad CLI/footprint-tool parity remains open.
