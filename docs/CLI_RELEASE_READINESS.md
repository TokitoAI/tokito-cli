# CLI release readiness — 2026-10-16

Scope: the standalone CLI. AR, YOLO, mobile and desktop integration are outside
this release decision. No release date or production claim follows from a green
test suite alone.

## Candidate release boundary

A Windows preview can expose supported PCB inspection/outline validation,
partial source-linked SVG/placement exports, source-bound offline bundles,
bounded VRML assembly with selectable component identities, footprint/library
candidates, model resolution, dataset integrity tooling, help and doctor.
Unsupported geometry and missing models must remain explicit. Source-specific
candidate models are not manufacturer-qualified engineering truth.

## Gates

The expanded local profile now has 45 base gates, adding the independent Rust
sample-distance validator's tests and optimized build. The opt-in planar repair
has a passing pinned model-14 reference comparison, documented in
[STEP_PLANAR_REPAIR.md](STEP_PLANAR_REPAIR.md); this is not general STEP admission.
Final branch delivery requires its current-source receipt. Older checkpoints
below retain their original gate counts and scope.

The subsequent experimental `--diagnose-face` command classifies the model-16
failure without repairing or admitting it. Independent review required a
relative-area underflow correction, now covered by scale-invariance tests.
That diagnostic is progress toward identifying a repair, not geometry
qualification or an added production export capability.

The current delivery receipt, model-14 reference receipt and model-16 face
diagnostic receipt are linked from PR #45 after the final local verification.
The local build reuses an existing Cargo target cache; this is not a clean-cache
build. Independent clean-machine installation remains unverified.

| Gate | Evidence / status | Next action |
| --- | --- | --- |
| Existing CLI foundation | Latest current-source local run passed 45/45 gates; its receipt and 350 matching source fingerprints are linked from PR #45. The run uses a trusted target cache, not a clean cache. | Retain the receipt with the delivered PR revision; rerun after further source changes |
| New reference integrity tooling | Eight unit tests and actual 76-solid reference capture pass; two reader qualification cases fail | Keep production STEP disabled; repair reader separately |
| Source review | Independent read-only review found no remaining code blocker; its cross-frame documentation finding was corrected and covered by scoped evidence | Review subsequent implementation changes |
| Windows clean-machine installation | Not verified | Test packaged executable, help/doctor and real import/export on an independent Windows environment |
| Version/toolchain contract | Cargo minimum corrected to 1.87 after real 1.85 dependency failure; actual Rust 1.87.0 locked offline all-target check passed | Verify locked dependencies against declared MSRV or correct declaration from evidence |
| Security/dependency assessment | OSV checked 188 unique registry name/version pairs across root and experiment locks; patched ruzstd advisory; no matches after update (2026-10-16) | Keep advisory matching current; broader reachability/security review remains separate |
| Reproducible delivery | Local hashed executable/receipt available for prior head | Bind final candidate bytes and documentation to current checks |
| General STEP and KiCad parity | Not complete; known degenerate triangles in two pinned models | Exclude from preview promises; required before advertising these capabilities |
| Cross-platform support | Current run is Windows only | Verify each platform before distributing its binary |
| GitHub delivery | PR #45 open; no hosted workflows by owner choice | Push only after local gates; merge only under applicable approval/review rules |

## Release recommendation

Ship a clearly scoped **Windows preview / release candidate** after the current
local suite and clean-machine delivery smoke test pass and known limitations are
published. Do not call it a production-ready KiCad replacement. A production
release requires an agreed supported-format/feature contract, actual installation
and dependency evidence, compatibility/upgrade checks, and closure of blockers
for every advertised feature. General STEP support requires its own geometry
qualification; AR and YOLO do not block this CLI-only release.

No reliable calendar date is established yet. Report gate completion rather than
inventing a deadline. The next engineering task is original Rust planar-face
triangulation, followed by the separately failing rational-spline case.

## V1 contract and filesystem requirements

[The v1 plan](CLI_V1_PLAN.md) records requested quality, missing-model, simulation
and view work without marking prototypes complete. Outputs currently require
hard-link support (tested local NTFS); FAT/exFAT support is not established.
Twin manifest-last rollback handles ordinary failures conservatively; it is not
process-kill/crash-atomic directory publication.

Actual compiler probe: Rust 1.85 failed in the locked earcut 0.4.11 dependency
(`unsigned_is_multiple_of`), despite no higher MSRV in dependency metadata.
The declared minimum is being corrected to Rust 1.87, where that API stabilized;
the actual Rust 1.87.0 `cargo check --locked --offline --all-targets` passed.
This establishes compilation, not full tests on the minimum toolchain.
Source: [Rust primitive API](https://doc.rust-lang.org/core/primitive.usize.html#method.is_multiple_of).

## Dependency advisory remediation

The 2026-10-16 OSV query found RUSTSEC-2026-0344 in ruzstd 0.8.1, reachable
through compressed embedded-model decoding. Reading a bounded stream does not
prove bounded internal decompression allocation. Root and twin-refinement locks
were updated to pinned ruzstd 0.9.1, the patched version. A second complete
query of 188 unique registry package versions returned no advisory matches;
this is known-advisory evidence, not a security certification. Receipts are
`target/dependency-osv-audit-20261016.json` and
`target/dependency-osv-audit-patched-20261016.json`. Rust 1.87 all-target checking
also passed with the patched dependency. The earlier 43-gate run was intentionally
interrupted before delivery when the advisory was found; a full rerun is required.

Sources: [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2026-0344.html),
[OSV batch API](https://google.github.io/osv.dev/post-v1-querybatch/).

## Current hardening checkpoint

All **43/43 local base gates** passed at
`target/delivery/local/20261016T074610Z-6690b2a5ba6142b2b23586cace9b156b/verification.json`.
The source/test/reference/doc hashes and delivered executable matched before
this narrative update. Every gate executed; trusted Cargo cache reuse does not
mean a clean dependency build. The run covers normal commands and feature-gated
subprocess failures during all three twin exports, plus 175 root tests and the
existing board/export/footprint/library/package journeys. A documentation-inclusive
run follows this update before push.

PE import-table inspection of the tested executable found VCRUNTIME140.dll and
Windows UCRT imports. Receipt: `target/windows-loader-imports-20261016.json`.
This is static import evidence only. An independent-machine Windows install
test and a documented prerequisite/portable-build choice remain release gates.
No 1.0 version bump or production readiness claim is made.
