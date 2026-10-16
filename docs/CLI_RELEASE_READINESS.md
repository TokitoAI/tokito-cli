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

| Gate | Evidence / status | Next action |
| --- | --- | --- |
| Existing CLI foundation | Previous pushed head 6377dd9 passed 40 local gates | Rerun current expanded 41-gate profile |
| New reference integrity tooling | Eight unit tests and actual 76-solid reference capture pass; two reader qualification cases fail | Keep production STEP disabled; repair reader separately |
| Source review | Independent read-only review found no blocker after failed-receipt correction | Review any new implementation changes |
| Windows clean-machine installation | Not verified | Test packaged executable, help/doctor and real import/export on an independent Windows environment |
| Version/toolchain contract | Cargo declares Rust 1.85; MSRV verification unproven | Verify locked dependencies against declared MSRV or correct declaration from evidence |
| Security/dependency assessment | No current comprehensive advisory review established by these checks | Assess pinned dependencies and reachable input boundaries |
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
