# Local verification and delivery

The owner has disabled GitHub Actions and hosted artifact uploads. Verification and binary delivery run locally; source delivery uses a feature branch and PR. A missing hosted check is never described as passed. Merge only after current-source local acceptance and independent review of the relevant head.

## Run the verifier

Use PowerShell 7 from the D-drive checkout:

```powershell
& 'D:\kannan\projects\tokito ecosystem\tokito_cli\scripts\verify-local.ps1' `
  -CargoPath 'C:\Users\PC\.cargo\bin\cargo.exe'
```

Default isolated runs use `target/local-verification/<run-id>` and deliver to `target/delivery/local/<run-id>`. To use the centralized artifact directories instead:

```powershell
.\scripts\verify-local.ps1 `
  -TargetBase 'D:\kannan\projects\tokito ecosystem\work\rust-target' `
  -DeliveryBase 'D:\kannan\projects\tokito ecosystem\work\cli-delivery'
```

Paths above are local examples, not required runtime locations. The runner requires absolute alternate parents and rejects reparse points. Each run gets a new directory and preserves prior receipts and binaries. [The migration path map](LOCAL_OUTPUT_LOCATIONS.md) resolves historical F and external-D receipts; do not rewrite historical execution telemetry.

## Current checks and evidence

The current baseline profile contains **41 checks**: format; embedded-dataset verifier tests; locked offline tests; warnings-denied Clippy; release build; subprocess CLI, KiCad 10.0.7 POS and footprint-type filtering, CLI experience, geometry, GLB, model-resolution, v0.1 compatibility, BOM, assignment, component-envelope, project-intake, project-bundle, project-bundle test-hook build/cleanup, footprint-family/specification/inspection, twin, dataset-source/Git, and real-CLI acceptance; STEP reference ownership/receipt tests and experiment format/tests/torus regression/Clippy/build; Altium experiment format/tests/Clippy/build; and twin-refinement format/tests/Clippy/build. Count actual `checks` in each receipt when selecting optional inputs. Older receipts describe their original source snapshots and profiles.

The main E2E runs real subprocesses on four pinned open-source PCB fixtures. It checks SVG/package/placement output, no-overwrite and refusal behavior, 14 hash-pinned KiCad 10.0.5 POS CSV comparisons and the bounded SVG DNP Fab-filter comparison. GLB tests check container structure, bounds, area, winding, closed edges, explicit partial consent and exact renderable component selectors. These fixtures are parser/export evidence, not proof that every dataset board is qualified.

The runner fingerprints source, tests, reference files, docs and scripts before and after verification. Generated Python bytecode, compiler target trees and KiCad `.kicad_prl` per-user editor preferences are excluded; the latter are machine-local UI state, not board-source fixtures. It copies only the tested executable, checks that copy's size/hash, and emits `verification.json` only when every selected check passes and the input fingerprints remain unchanged. A failed or skipped required test blocks delivery. The receipt records toolchain/platform, individual results, source hashes, binary hash/size and explicit limitations.

An old receipt remains valid for its own source. It is not current after a test, document or implementation edit changes a recorded input. Check the fingerprints instead of treating the newest timestamp as sufficient evidence.

## Optional real-dataset journeys

### Explicit cache reuse for the base profile

The default verifier still allocates an isolated new Cargo target. For a
follow-up base-profile run, an explicit existing verification cache can avoid
recompiling every dependency under limited memory/storage:

```powershell
pwsh -File scripts/verify-local.ps1 -TargetBase F:\kannan\projects\tokito-cli-verification -ReuseBuildTarget F:\kannan\projects\tokito-cli-verification\EXISTING_VERIFICATION_TARGET
```

Replace the last path with an actual existing target. It must be absolute,
strictly within the selected base, contain Cargo cache metadata, and have no
linked path/entry. Reuse is refused with optional dataset-profile inputs.
All formatting, Cargo tests/Clippy/builds and subprocess E2Es still execute;
the same frozen-input and tested-binary checks remain mandatory. Geometry test
outputs and delivery receipts have fresh run identifiers. The receipt records
`cachePolicy: reuse-existing-target`; this is **not a clean dependency build**.
Only use a trusted cache created by your own verification runs.

On a storage-constrained Windows machine, set `TEMP` and `TMP` in the invoking
process to an existing writable directory on the authorized build drive before
running the script. Child Python/Rust tests inherit those paths. This does not
change system environment settings, skip checks or clean unrelated files.
The 2026-10-16 base run passed all 40 gates with this setup; C: disk exhaustion
in the preceding run was an infrastructure failure, not a passing result.

| Parameters supplied together | Additional acceptance scope |
| --- | --- |
| `-DatasetRepo`, `-DatasetBomReport` | Pinned component/BOM join, hash binding and explicit candidate/shell limitations |
| `-DatasetTwinGlb`, `-DatasetTwinComponentMap`, `-DatasetTwinManifest`, `-DatasetTwinProvenance` | Pinned twin bytes, manifest/map and independently selectable component meshes |
| `-Esp32CandidateRoot`, `-Esp32ReferenceBom`, `-Esp32Pcb`, `-Esp32ModelLibrary` | Provisional/source-reference ESP32 subprocess journeys; exact KiCad9 library intake required |
| `-Esp8266SourceRoot`, `-Esp8266UnitReceipt`, `-Esp8266FreecadReference` | 17-model unit comparisons, positioned STEP candidates, Altium records/embedded bytes and source prototype journeys |
| `-NxpRefinementBaseline`, `-NxpRefinementSpec`, `-NxpRefinementPcb` | Explicit source-linked NXP refinement journey |

Required paired inputs must exist and match the selected profile. Dataset E2E produces its own passed receipt; the outer receipt records that evidence path and SHA-256. A BOM report digest establishes the supplied report bytes, not an independently materialized manufacturer BOM. Dataset hashes establish integrity, not geometry qualification or publisher identity.

## Limits and repository delivery

The bounded VRML adapter and source-derived placement tests do not establish general WRL/STEP import or KiCad 3D equivalence. Experimental Rust adapters remain separately gated. Baseline fixtures and optional dataset profiles do not drive the Android importer, renderer, physical phone, CAD GUI, Desktop or registered AR. Linux/MSRV, manufacturing validity, full KiCad compatibility and physical board fit require their own evidence.

The CLI-specific project is linked from [the repository Projects tab](https://github.com/TokitoAI/tokito-cli/projects). Keep broad issues open while their completion gates remain unmet. Application code goes through a feature branch/PR; no direct main push, hosted workflow or release upload is part of this flow. The historical repository's PR #9, logs and Git bundle are recovery records under `target/recovery/`, not live checks on current code.
