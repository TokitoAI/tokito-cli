# CLI release readiness — 2026-10-17

Scope: the standalone CLI. AR, YOLO, mobile and desktop integration are outside
this release decision. No release date or production claim follows from a green
test suite alone.

## Candidate release boundary

The shared Ultimaker presentation profile is implemented for existing packaged
demos and future generated viewers. Six real-board Edge journeys passed actual
3840 × 2160 PNG generation/download, state restoration, lighting roundtrips,
offline selection and simulated device-limit refusal. Source GLB/map/manifest
bytes remain unchanged. See [presentation evidence](evidence/ultimaker-presentation-browser-20261016.json)
and [the standard](THREED_PRESENTATION_STANDARD.md). This is presentation
acceptance; geometry qualification and production readiness remain separate.

A Windows preview can expose supported PCB inspection/outline validation,
partial source-linked SVG/placement exports, source-bound offline bundles,
bounded VRML assembly with selectable component identities, footprint/library
candidates, model resolution, dataset integrity tooling, help and doctor.
Unsupported geometry and missing models must remain explicit. Source-specific
candidate models are not manufacturer-qualified engineering truth.

## Gates

The expanded local profile has 48 gates, including temp-path confinement tests,
the final output-tree audit, the independent Rust sample-distance validator,
footprint/library and dataset journeys, pinned KiCad 10.0.7 comparisons, the
required SVG reference-evidence validator, and the STEP, Altium and twin-refinement
experiment crates. The current-source Windows root verification passed 48/48
gates after the bounded worst-triangle FreeCAD audit was added. It used Cargo
and Rust 1.94.1, Python 3.11.5, and a trusted existing Cargo target cache; it is
not a clean-cache or independent-machine run. Its external receipt is
`F:\\kannan\\projects\\tokito-cli-verification\\orientation-gate-release-final3-20261017\\verification.json`;
the PR records its hash and the release binary hash. These local checks do not
establish installation or production qualification.

The opt-in planar repair has a passing pinned model-14 reference comparison,
documented in [STEP_PLANAR_REPAIR.md](STEP_PLANAR_REPAIR.md); this is not
general STEP admission. The model-16 rational-spline trim candidate remains
rejected by containment and orientation evidence; area convergence remains
unverified.
Older checkpoints below retain their original gate counts and scope.

The experimental `--diagnose-source-face` command inspects the model-16
rational B-spline surface and ordered 3D trim edges, then computes a bounded
research triangulation candidate. Its new refinement gate requires sampled
chord error and positive triangle-centroid normal alignment, using the STEP
face sense. A sign-semantics review found and corrected double-counting of UV
winding; regressions cover both face senses and triangle windings. The pinned
run returns 12,801 vertices and 24,064 triangles; chord error passes, but
3,995/24,064 centroid normal alignments are nonpositive. Rust and FreeCAD 1.1.4
independently evaluate the same Rust candidate triangles and agree on that count
and the minimum alignment; FreeCAD does not triangulate the face independently.
The candidate remains unqualified, the command exits with refusal, and it
produces no export. Centroid sampling does not prove orientation over every
point on curved facets; the finding does not repair the reader, qualify a
complete solid or add production STEP support.

The current delivery receipt, model-14 reference receipt and model-16 face
diagnostic receipt are recorded in PR #45. The latest bounded FreeCAD audit
found that all 224 normal samples over the 32 worst triangles remain
nonpositive; the lowest-U bin has 3,130/8,981 failures (34.85%). Its cause remains
unestablished, the diagnostic has not compared an independently generated
triangulation, and no geometry is admitted. See the exact hashes and limits in
[the face #2875 investigation](STEP_FACE_2875_ANALYSIS.md). Independent clean-machine
installation remains unverified. The 48-gate run includes an offline validator
for the pinned KiCad 10.0.7 SVG reference evidence; it verifies recorded source
and output hashes and bounded comparisons, but does not rerun KiCad.

| Gate | Evidence / status | Next action |
| --- | --- | --- |
| Existing CLI foundation | Current-source local Windows run passed 48/48 gates after the bounded FreeCAD audit update; the receipt and binary hash are bound in the external evidence record and PR #45. The run reuses a trusted target cache, not a clean cache. | Retain the evidence with the PR revision; rerun after further implementation changes |
| New reference integrity tooling | Eight unit tests and actual 76-solid reference capture pass; two reader qualification cases fail | Keep production STEP disabled; repair reader separately |
| Source review | Independent read-only review caught and verified a face-sense/UV-winding sign correction; it notes centroid sampling is diagnostic rather than a continuous orientation proof | Keep admission closed; investigate adaptive/continuity-aware refinement |
| Windows clean-machine installation | Not verified | Test packaged executable, help/doctor and real import/export on an independent Windows environment |
| Version/toolchain contract | Cargo minimum corrected to 1.87 after real 1.85 dependency failure; actual Rust 1.87.0 locked offline all-target check passed | Verify locked dependencies against declared MSRV or correct declaration from evidence |
| Security/dependency assessment | OSV checked 188 unique registry name/version pairs across root and experiment locks; patched ruzstd advisory; no matches after update (2026-10-16) | Keep advisory matching current; broader reachability/security review remains separate |
| Reproducible delivery | Local hashed executable/receipt available for current branch source | Keep the receipt local and bind it to the PR update; independent install/upgrade test is still required |
| General STEP and KiCad parity | Not complete; known degenerate triangles in two pinned models | Exclude from preview promises; required before advertising these capabilities |
| Cross-platform support | Current run is Windows only | Verify each platform before distributing its binary |
| GitHub delivery | PR #45 open; no hosted workflows by owner choice | Push only after local gates; merge only under applicable approval/review rules |

## Release recommendation

The current branch has passed its local 48-gate suite, but is not ready for a
production release: clean-machine installation, KiCad/footprint-library parity,
qualified STEP geometry, and component-model completeness remain open. A
clearly scoped Windows preview / release candidate could be considered only
after independent installation and known limitations are published. Do not
call it a production-ready KiCad replacement. A production
release requires an agreed supported-format/feature contract, actual installation
and dependency evidence, compatibility/upgrade checks, and closure of blockers
for every advertised feature. General STEP support requires its own geometry
qualification; AR and YOLO do not block this CLI-only release.

No reliable calendar date is established yet. Report gate completion rather than
inventing a deadline. The latest model-16 face candidate has 12,801 source-curve
mesh vertices inside the trimmed face, while 200 of 12,801 support-surface
evaluations lie outside the trim. Its 24,064 triangles include 3,995 with
nonpositive centroid-normal alignment (minimum `-0.9999979635`), independently
confirmed by FreeCAD. The candidate remains unqualified and no export is
produced. The next STEP task is continuity-aware UV branch selection per
ordered source edge, followed by adaptive angular as well as chordal refinement
and independent trim, orientation, closure and convergence gates. This
face-level investigation does not qualify the full STEP solid or assembly.

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
also passed with the patched dependency. The full 45-gate local run completed
after the dependency update; independent advisory reachability review remains
separate from this release gate.

Sources: [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2026-0344.html),
[OSV batch API](https://google.github.io/osv.dev/post-v1-querybatch/).

## Historical hardening checkpoint (superseded)

At this earlier checkpoint, **43/43 local base gates** passed at
`target/delivery/local/20261016T074610Z-6690b2a5ba6142b2b23586cace9b156b/verification.json`.
The source/test/reference/doc hashes and delivered executable matched before
this narrative update. Every gate executed; trusted Cargo cache reuse does not
mean a clean dependency build. The run covers normal commands and feature-gated
subprocess failures during all three twin exports, plus 175 root tests and the
existing board/export/footprint/library/package journeys. This checkpoint was
superseded by the documentation-inclusive 48/48 current-source verification
described in the Gates section above and recorded with PR #45.

PE import-table inspection of the tested executable found VCRUNTIME140.dll and
Windows UCRT imports. Receipt: `target/windows-loader-imports-20261016.json`.
This is static import evidence only. An independent-machine Windows install
test and a documented prerequisite/portable-build choice remain release gates.
No 1.0 version bump or production readiness claim is made.
