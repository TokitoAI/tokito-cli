# Local verification and delivery

The owner selected local builds and E2E to preserve the GitHub Actions quota. The repository workflow was removed by request; no hosted workflow runs on pushes or pull requests. Hosted CI and artifact upload results are not claimed. A later workflow can be added when the owner chooses.

Run the PowerShell 7 script from any directory:

```powershell
& 'F:\kannan\projects\tokito ecosystem\tokito_cli\scripts\verify-local.ps1' -CargoPath 'C:\Users\PC\.cargo\bin\cargo.exe'
```

The script uses cached offline dependencies and creates a fresh isolated build directory under target/local-verification/<run-id>, preserving earlier outputs and avoiding replacement of potentially locked Windows executables. Receipts/binaries remain under target/delivery/local/. Its ten gates are formatting, locked offline Rust tests, warnings-denied Clippy, release build, four-board CLI E2E, topology/SVG/Gerber E2E, GLB E2E, model-resolution/mesh-inspection E2E, historical v0.1 compatibility, and a release-binary real CLI acceptance journey. The latter invokes the built executable against positive and invalid board fixtures, verifies top/bottom SVG classification, partial-GLB consent/refusal/no-overwrite, package verification, and writes a local receipt under target/real-cli-e2e/. It does not drive a GUI, a CAD viewer, or a phone. The GLB journey checks container and mesh bounds/area/winding/closed edges, explicit partial consent, coverage, package input, determinism, no-overwrite and invalid-outline refusal. The real-model journey checks explicit roots, six UNO asset hashes, local WRL geometry/material inventory, units/bounds, byte budgets and malformed paths. Inputs and tested binary are fingerprinted; generated Python bytecode and __pycache__ paths are deliberately excluded from the source fingerprint. A receipt is written only if every gate passes.

Each verification.json records the local Windows architecture, actual Rust/Cargo/Python versions, passed checks, source-input SHA-256 values, and tested binary hash/size. The newest timestamped receipt under target/delivery/local/ is the current result; earlier receipts are historical.

The STEP-evaluation continuation adds three separate local gates for the isolated
`experiments/step-evaluation` crate: formatting, locked offline tests and
warnings-denied Clippy. Its manifest/lock/source are fingerprinted; nested
generated `target` output is excluded. The current runner therefore has thirteen
gates. Earlier ten-gate receipts verify only their original input snapshots and
do not qualify this experiment or enable production STEP import.

The full suite verifies the board-shell GLB, bounded model capture, flat-triangle WRL profile, pure-Rust point/mesh placement helpers, strict source-stackup derivation, one- and two-component assembly GLBs, exact-refdes selector behavior, unsupported-side diagnostics, and coverage truncation on Windows. The tests model the App's documented name filter but do not run the App importer or Android renderer/selection. The suite does not prove Linux/MSRV, hosted CI, general WRL/STEP, complete KiCad conformance, manufacturing validity, Desktop/MCP integration or AR. Placement and stackup tests are source-derived checks, not a KiCad-generated export comparison. Hashes establish integrity, not publisher identity. A verification receipt is authoritative only when its recorded source fingerprints match the final implementation/scripts/tests; narrative documentation can be updated afterward, but must not be represented as an input to that earlier receipt.

The recreated private TokitoAI/tokito-cli repository receives source through a feature branch and pull request; application code is not pushed directly to main. The user reviews and merges the PR. No hosted workflow runs on pushes or pull requests. The CLI-specific roadmap is linked from the repository's Projects tab at https://github.com/TokitoAI/tokito-cli/projects; the organization-wide ecosystem project remains separate and unchanged. Hosted CI, release, and binary upload are out of scope.

The former repository's PR #9, CI logs, issues, branches, and Git bundle remain under target/recovery/github-before-recreation-20261009T150910Z. They are historical recovery records, not live checks or pull requests on the recreated repository.
