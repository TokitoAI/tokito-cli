# PR #9 check failure and proposed fix

Inspected 2026-10-09. PR head: c6badc73f02a153a9b9bf4b638210b11d7ddbcd
Run: https://github.com/TokitoAI/tokito-cli/actions/runs/37921147042

Both Windows and Ubuntu Rust 1.85 jobs passed formatting, all 15 Rust tests, Clippy, release builds and actual four-board subprocess E2E. Their only failed step was Upload verified build candidate: GitHub reported that artifact storage quota had been hit. The final CLI required gate failed because the verification jobs also contain that upload. There is no demonstrated CLI source failure in this run.

Current local verification was repeated successfully using scripts/verify-local.ps1. Receipt: target/delivery/local/20261009T141245Z-a71c79a69e3a4601b39b062d00f8f219/verification.json. The receipt establishes local evidence; it does not replace GitHub's failed historical result or mark any remote check successful.

Prepared proposal: target/delivery/pr9-ci-proposed.yml (ignored local file). It removes only the artifact-upload step. Formatting, tests, warnings-denied Clippy, release build, real E2E, the Windows/Ubuntu matrix, read-only token and failure-sensitive final gate remain intact. The candidate parses successfully and its required-step/gate/permission assertions pass. The actual .github/workflows/ci.yml was not modified.

The owner explicitly selected: keep everything local and leave GitHub's workflow and failed historical checks unchanged. The proposal is retained only as an unapplied local reference. No workflow edit, setting change, code push, issue/PR edit, rerun, artifact upload, release or merge was made during this diagnosis. The historical checks remain failed by that decision; local passes must not be represented as a remote pass. No approval question remains pending for this CI diagnosis.

The attached AI roadmap is architecture input for later engine maturation. It does not prove that a reusable canonical engine, robust full topology, GLB or Desktop integration already exist, and none was implemented as part of this CI diagnosis.
