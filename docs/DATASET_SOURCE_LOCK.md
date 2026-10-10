# Dataset source locking

This is the first implemented step of [dataset pipeline issue #26](https://github.com/TokitoAI/tokito-cli/issues/26), before component qualification and App integration. It is an offline pure-Rust content verifier, not a Git client or CAD validator.

## Commands

~~~text
tokito-cli dataset lock <ROOT> --request <REQUEST.json> --output <NEW_LOCK.json>
tokito-cli dataset verify <ROOT> --lock <LOCK.json>
tokito-cli help dataset
~~~

A request has exactly four fields: schema `tokito.dataset-source-request.v1`, repository `https://github.com/TokitoAI/tokito-app-dataset`, a full lowercase 40-character `revision`, and a nonempty `assets` list. Each asset has exactly `boardId`, `role`, `path`, and `provenance`. The path is relative to the supplied root and uses forward slashes. Roles are `schematic`, `pcb`, `gerber`, `bom`, `photo`, `model`, `component_map`, `registration`, `provenance`, or `metadata`.

~~~json
{
  "schema": "tokito.dataset-source-request.v1",
  "repository": "https://github.com/TokitoAI/tokito-app-dataset",
  "revision": "257ea7e68ee2e700522bd1b19ad9cbf1c0c3980a",
  "assets": [{
    "boardId": "nxp-frdm-mcxn236",
    "role": "pcb",
    "path": "data/board-sources/nxp-frdm-mcxn236/generated-kicad/FRDM-MCXN236.imported.kicad_pcb",
    "provenance": "Imported design reference; physical revision equivalence unverified"
  }]
}
~~~

The new lock uses schema `tokito.dataset-source-lock.v1`, adds each asset's byte count and SHA-256, and sorts records by path. It writes no source files and refuses an existing output. The verifier checks every listed file, reports failures and per-board role counts, emits JSON, and returns exit code 2 on refusal. Unlisted files are outside the verification scope. A lock can inventory a known asset subset; it does not establish that a board or repository is complete.

## Integrity and qualification boundaries

The repository URL, revision, board ID, role and provenance are caller claims. `revisionAuthenticated` is always false: independently check the Git commit/tree/blob or LFS object identities before accepting a request. The real dataset test separately checks pinned Git/LFS identities for three materialized PCB files; this is test evidence, not an implemented production Git resolver. Hashes are computed over the actual payload, not an LFS pointer.

`physicalFitVerified`, `componentCrosswalkVerified`, `geometryQualified`, and `appAdmissionAuthorized` remain false. An imported PCB, a board-only shell and a component assembly are different inputs even if their role is `model`; retain derivation details in provenance until a typed qualification adapter is implemented. Photos and detections do not establish pad geometry or fitted-part identity.

JSON is bounded to 4 MiB, requests to 10,000 entries, each asset to 256 MiB, and aggregate payloads to 2 GiB. Duplicate JSON keys, unknown fields, duplicate case-folded paths, path traversal, device names, symlinks/reparse points and pointer-only assets are refused. The loader checks filesystem paths before reading; callers must supply a stable, nonconcurrently modified snapshot. Source content verification is point-in-time and does not freeze the filesystem. Output parent directories must already exist; no automatic downloads, repository pulls, binary uploads, or sibling edits occur.

## Current source evidence and remaining flow

On 2026-10-10 the inspected dataset main remains `257ea7e68ee2e700522bd1b19ad9cbf1c0c3980a`; latest open PR #6 is `5283bc059e30e51f4d3a81a674425936fc213214`, PR #7 is `ae27af81ccaf3a21a4127341a2244bfd48c2503c`, and PR #4 is `ca65648cdc3c10f7f02fbd7710ccba158e8c39b9`. PR inputs remain separate pinned candidates, not silently treated as merged main. Refresh heads before adopting subsequent updates.

The local sparse main intake has 67 materialized LFS objects and 344 pointer-only objects among 411 tracked LFS entries. It is not a complete offline dataset. Materialized imported PCB references for ESP32, MCXN236 and Ultimainboard are exercised by the optional dataset test. Raspberry Pi's reduced schematic does not provide exact PCB CAD; MCXN947 remains distinct from MCXN236; ESP8266 reference variant correspondence remains unresolved.

The [local Git/LFS inventory command](DATASET_GIT_INVENTORY.md) now resolves explicit commit/tree/blob identities and records unavailable assets, separately from this caller-declared content lock. Next: publisher/source acceptance, selected-asset materialization, staged update comparison, exact PCB/UUID/refdes/model crosswalk, unit/transform/geometry comparisons, and all-board qualification. Only after those gates should App import, entity colour highlighting, dialogue and physical registration begin. KiCad CLI behavior parity and KiCad Library Tools generator-family research remain separate active workstreams in [the goal tracker](GOAL_TRACKER.md).
