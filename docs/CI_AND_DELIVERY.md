# Local verification and delivery

The owner selected local builds and E2E to preserve the GitHub Actions quota. The repository workflow was removed by request; no hosted workflow runs on pushes or pull requests. Hosted CI and artifact upload results are not claimed. A later workflow can be added when the owner chooses.

Run the PowerShell 7 script from any directory:
F:\kannan\projects\tokito ecosystem\tokito_cli\scripts\verify-local.ps1 -CargoPath C:\Users\PC\.cargo\bin\cargo.exe

The script uses cached dependencies and writes its build under target/local-verification and receipts/binaries under target/delivery/local/. It runs formatting, locked offline Rust tests, warnings-denied Clippy, a release build, four-board subprocess E2E, topology/SVG/Gerber E2E, and historical v0.1 package/output compatibility. It fingerprints source, tests, examples, docs, README, project instructions, workflow inputs if any, and toolchain configuration before and after the gates. It hashes the release executable around E2E and writes a receipt only if every gate passes.

Each verification.json records the local Windows architecture, actual Rust/Cargo/Python versions, passed checks, source-input SHA-256 values, and tested binary hash/size. The newest timestamped receipt under target/delivery/local/ is the current result; earlier receipts are historical.

The local run does not prove Linux/MSRV, hosted CI, complete KiCad conformance, manufacturing validity, GLB/STEP generation, Desktop/MCP integration, app import, or AR/device behavior. Hashes establish integrity, not publisher identity.

The recreated private TokitoAI/tokito-cli repository receives source through a feature branch and pull request; application code is not pushed directly to main. The user reviews and merges the PR. No hosted workflow runs on pushes or pull requests. The CLI-specific roadmap is linked from the repository's Projects tab at https://github.com/TokitoAI/tokito-cli/projects; the organization-wide ecosystem project remains separate and unchanged. Hosted CI, release, and binary upload are out of scope.

The former repository's PR #9, CI logs, issues, branches, and Git bundle remain under target/recovery/github-before-recreation-20261009T150910Z. They are historical recovery records, not live checks or pull requests on the recreated repository.
