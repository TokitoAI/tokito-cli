# CLI release readiness — 2026-10-19

Scope: the standalone CLI. AR, YOLO, mobile and desktop integration are outside
this release decision. No release date or production claim follows from a green
test suite alone.

## Canonical patch research status

The experimental Rust evaluator now constructs a two-face mesh using shared
source-curve boundary vertices before quality measurement. It preserves separate
complementary UV regions, measures an explicit boundary displacement allowance
and checks the expected open-patch topology. Passing sampled patch checks does
not qualify a component solid, complete trims or production STEP export. General
surface/trim coverage, full-solid validation and complete component generation
remain release blockers. See
[the patch contract](GEOMETRY_RECOVERY_PLAN.md#canonical-two-face-patch-2026-10-19)
and [checkpoint evidence](evidence/shared-patch-20261019.json).

## Shared-edge research status

The experimental Rust shared-edge audit passed nine actual source/native/
refusal checks. Independent evaluation corroborates a sampled incident-support
gap of `4.6454902159258955e-6 mm`; the report preserves source warnings and
unknown trim/tolerance status. It does not repair/sew a shell, qualify a
component or add production STEP exports. Complete solids, missing component
bodies and local tolerance-policy validation remain release blockers. See
[the selected geometry solution](GEOMETRY_RECOVERY_PLAN.md#selected-solution-and-next-implementation-gates)
and [checkpoint evidence](evidence/shared-edges-20261019.json).

## Multi-bound planar face research status

The original Rust research evaluator now creates a diagnostic triangulation for
one pinned planar ESP8266 STEP face with one declared `FACE_OUTER_BOUND` and an
additional generic `FACE_BOUND`. Under ISO 10303-42, when an outer bound is
explicitly declared, the other face loops lie inside it and are inner loops;
the evaluator retains the raw entity type and reports that interpreted role.
A local FreeCAD 1.1.4 / OCCT 7.8.1 comparison uniquely
maps both sampled loops to distinct imported wires and finds a unique sampled
geometric correspondence from each of the six STEP `EDGE_CURVE` IDs to an
imported edge, checking source samples and both endpoints. The maximum measured
source-edge sample gap is `1.9984e-15` source units. This does not prove exact
topological identity or continuous curve agreement. The comparison also checks
sampled area, all 516 triangle centroids and triangle winding. Rust and an
independent Python check examine 132,870 triangle pairs for interior overlap.
The sampled chord target is met, but continuous curve error is not certified
and the result is not admitted to the product exporter. This remains one-face
diagnostic evidence: complete STEP component/solid qualification and the
product adapter remain open. It does not close issue #19. See
[ISO 10303-42:2021 face/bound semantics](https://www.steptools.com/stds/smrl/data/resource_docs/geometric_and_topological_representation/sys/5_schema.htm),
the
[geometry evidence](GEOMETRY_RECOVERY_PLAN.md#pinned-planar-face-with-an-additional-bound-2026-10-19)
and [captured receipt](evidence/planar-multibound-face-20261019.json).

## Diagnostic STEP-to-GLB occurrence export

The isolated Rust STEP research evaluator can now write a bounded GLB candidate
from its occurrence-world mesh. It keeps assembly occurrences as separate
selectable nodes and retains per-triangle source face IDs, material groups and
degenerate-triangle diagnostics. On the pinned ESP8266 model-16 input, the local
E2E decoded 8,270 triangles across three occurrence nodes and 30 primitives;
520 zero-area source triangles were retained and explicitly marked. The GLB
container, accessors, float32 positions, generated normals, color projection,
selector inventory and no-overwrite/malformed-input refusals passed.

This is a visualization candidate from the same STEP reader being serialized;
the E2E is not an independent geometry reference. Selection is by STEP
occurrence path, not PCB reference or UUID. STEP-to-board placement, source-face
completeness, shell/solid validity, tessellation fidelity and physical material
semantics remain unverified. No production exporter changed, and the artifact
remains `admitted:false`, `geometryQualified:false` and
`productionExport:false`. See the [experimental command contract](../experiments/step-evaluation/README.md#diagnostic-step-to-glb-occurrence-export)
and the [geometry recovery plan](GEOMETRY_RECOVERY_PLAN.md#diagnostic-step-to-glb-occurrence-candidate-2026-10-19).
Hashes, decoded counts and the local artifact path are captured in the
[STEP diagnostic GLB evidence receipt](evidence/step-diagnostic-glb-20261019.json).

## Latest code and verification checkpoint

PR #45 contains mixed per-pad SMD/plated-THT/NPTH footprint generation as a
bounded, unqualified geometry candidate. The current branch adds a bounded POS
ASCII subset beside CSV, with nine pinned KiCad 10.0.5 cases: eight exact
matches and one Pico comparison that preserves all named rows and compares
unreferenced values as an unordered set. The harness verifies exact tool
versions and hashes, and limits normalization to the two documented headers
and CRLF line endings. ASCII output has per-field and total-size caps plus
control/bidi-format rejection. Local KiCad 10.0.5 loading/plotting of the
synthetic mixed-pad footprint does not qualify its semantics or manufacturer
land pattern; see the [readback receipt](evidence/mixed-pad-kicad-10.0.5.json).

The latest full Windows verifier run passed **48/48 gates** on 2026-10-19 at
18:48:36 UTC. Its local receipt is
`target/delivery/local/20261019T184836Z-39d0ba8086ef46efb1a67757005962dc/verification.json`
(SHA-256 `0d73d3da1e35170d9b376d2d1198a716a69d856219c3a7d507a8d97a4943fc9d`).
The run used a new isolated Cargo target and tested the executable/test sources
committed in PR #45 commit `ada2a8b`; this readiness paragraph was added after
that run and is documentation-only. Optional live dataset-board profiles were
not supplied. The pinned ESP8266 model-16 diagnostic GLB E2E was run separately
and is recorded in
[`step-diagnostic-glb-20261019.json`](evidence/step-diagnostic-glb-20261019.json).
These results are not an independent-machine, installation, cross-platform or
all-dataset-board test.

The updated face-bound, fail-closed orientation-predicate, and bounded integer
oracle result was separately
rerun against the pinned STEP source with the release evaluator and FreeCAD 1.1.4 /
OCCT 7.8.1: **5/5** focused checks passed. Receipt:
`D:/tokito-cli-v1-verify-20261019/20261019T154055Z-fae0f3f0ef6c443f8bf26d4e2f3b4757/research/planar-multibound-face-20261019-v16/receipt.json`
(SHA-256 `d655c2531646ba343d30f0a678c757f3d60c6d3e6fb3baa7daa99a8ab0836492`).
The candidate still reports `geometryQualified=false`, `completeSolidVerified=false`,
`admitted=false`, and no production exports. This establishes only sampled
correspondence for one planar face, not continuous curve equivalence, a complete
solid, or production STEP admission.
No GitHub Actions ran; PR #45 remains open with no hosted checks or independent
PR approval.

This verifies the currently implemented local command suite; it does not close
production gates below. Tokito CLI V1 remains not production-ready.

The 2026-10-19 experimental solver-status change separates residual acceptance
from finite-difference tangent observations, preserving the selected source-face
refusal. Its [source-face/independent-audit evidence](evidence/projection-solver-status-20261019.json)
does not clear any production geometry gate. Complete-solid and missing-body
qualification remain open.

The new physical-refinement research mode reduces measured interior chord
error but still reports 2,312 nonpositive normals and an incomplete mesh-budget
outcome. Its [independent evidence](evidence/physical-refinement-20261019.json)
does not qualify the face or clear the general STEP/complete-solid gate.

The optional metric-preconditioned research path reduces the measured remaining
nonpositive normals from 2,312 to ten and sampled interior error to 0.000286452 mm.
It still misses its targets and remains incomplete. [Paired independent evidence](evidence/metric-refinement-20261019.json)
does not clear any source-boundary, whole-solid or general STEP release gate.

A further isolated interior-retriangulation pass accepts four independently
verified flips and reduces nonpositive normals from ten to six, retaining all
source coordinates/constraints. Global chord maxima remain unchanged. Its
[seven-check evidence](evidence/interior-retriangulation-20261019.json) supports
that narrow improvement; general STEP/whole-solid gates remain blocked.

The opt-in adaptive source-boundary experiment removes the sampled inversions
for pinned model16 and reaches `0.000004617252 mm` sampled boundary chord error.
Eight dedicated E2E/oracle/refusal checks pass. Four normal-target failures and
interior error `0.00006014931 mm` remain; source/support discrepancy and solid
qualification are unresolved. This improves one face, not production STEP
support. See the [adaptive sampling evidence and limits](GEOMETRY_RECOVERY_PLAN.md#adaptive-source-parameter-boundary-sampling-2026-10-19).

The reserved-priority research path passes nine dedicated E2E checks and reaches
zero sampled inversions and zero five-degree normal-target failures on the
pinned face. Interior chord error `0.00005968069 mm` still misses its target;
source-gap, trim and solid qualification remain unresolved. See the
[priority checkpoint](evidence/priority-refinement-20261019.json). V1 is still
not production-ready; this path enables no production STEP export.

## Candidate release boundary

The shared Ultimaker presentation profile is implemented for existing packaged
demos and future generated viewers. Six real-board Edge journeys passed actual
3840 × 2160 PNG generation/download, state restoration, lighting roundtrips,
offline selection and simulated device-limit refusal. Source GLB/map/manifest
bytes remain unchanged. See [presentation evidence](evidence/ultimaker-presentation-browser-20261016.json)
and [the standard](THREED_PRESENTATION_STANDARD.md). This is presentation
acceptance; geometry qualification and production readiness remain separate.

The [geometry recovery audit](GEOMETRY_RECOVERY_PLAN.md) now independently
reproduces one rejected source curve/support-surface gap and quantifies local
parameter distortion. Its four probe comparisons and three refusal checks are
separate development evidence; they do not repair the surface or enable STEP
export. No production release gate is cleared by that research result.

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
gates after the POS ASCII changes. Each full run writes the authoritative
current-source result and hashes to
`target/delivery/local/<run-id>/verification.json`. The receipt records its
toolchain, executed gates and outcome. Reusing the isolated D: Cargo target is
not a clean-cache or independent-machine run. Local checks do not establish
installation or production qualification.

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
| Existing CLI foundation | The latest full Windows verifier passed 48/48 gates after the POS ASCII changes; the matching per-run receipt is authoritative. | Retain the exact receipt with the PR revision; rerun after any tracked change |
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

Tokito CLI V1 is not ready for a production release: clean-machine installation, KiCad/footprint-library parity,
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
superseded by the documentation-inclusive 48/48 verification on the earlier
source revision and recorded with PR #45.

PE import-table inspection of the tested executable found VCRUNTIME140.dll and
Windows UCRT imports. Receipt: `target/windows-loader-imports-20261016.json`.
This is static import evidence only. An independent-machine Windows install
test and a documented prerequisite/portable-build choice remain release gates.
No 1.0 version bump or production readiness claim is made.


### Directional surface-grid research checkpoint

The original Rust seeded face experiment meets sampled interior/boundary chord
and five-degree normal targets; independent FreeCAD checks agree. This is one
unqualified supporting-surface face, not complete component or STEP admission.
Source-curve discrepancy, complete trims, cross-face seams and whole-solid
checks remain open. See [the evidence-backed contract](GEOMETRY_RECOVERY_PLAN.md#directional-interior-grid-initialization-2026-10-19).
