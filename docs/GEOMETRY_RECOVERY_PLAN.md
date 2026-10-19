# Accurate component geometry: evidence and implementation plan

Updated 2026-10-19. This is the CLI's geometry recovery decision record.
Production dependencies remain pure Rust. Native CAD tools are development
reference oracles only. No new production STEP capability is admitted here.

## Independent result on the rejected spline probe

The current Rust diagnostic was rebuilt and run against the materialized
ESP8266 source asset with SHA-256
`7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f`.
It selects face `#2875`, representation `#5957`, context `#206`, in millimetres.
The source is the historical dataset candidate, not a claim about latest dataset
main. The dataset checkout's corresponding file is currently an LFS pointer;
the audit used the existing materialized copy, checked by its content digest.

FreeCAD 1.1.4 / OCCT 7.8.1 independently imported a frozen snapshot of those
exact bytes. Face 231 was checked against all 100 source boundary samples by
distance to its boundary edges; maximum distance was `1.6011864169946884e-15 mm`.
Four source-curve midpoint probes were then compared. The positive control is
on the surface; three rejected probes have nonzero support-surface residuals.

| Probe | Rust residual (mm) | OCCT natural support-face distance (mm) |
| --- | --- | --- |
| 0 | 0 | 3.1401849173675503e-16 |
| 1534 | 1.0749767218448296e-6 | 1.0749767222677508e-6 |
| 1535 | 1.07888315327835e-6 | 1.0788831531073023e-6 |
| 1536 | 1.0713741370539803e-6 | 1.0713741373647778e-6 |

At probe 1535, Rust and OCCT evaluate the surface at the same UV to within
`6.661338147750939e-16 mm`. The residual is nearly orthogonal to both sampled
tangent directions (cosines `5.07e-11` and `-2.76e-10`). This is independent
numerical evidence consistent with a stationary point and a source
curve/support-surface discrepancy, rather than a failed initial guess. It is
not a proof of the global closest point or all source geometry.

An important trap: OCCT's distance to the **trimmed topological face** is almost
zero for the same point, because that face includes the boundary edge. Returning
that nearest point to the supporting surface still leaves the `1.078883e-6 mm`
gap. The nearest imported boundary edge has tolerance `1.0808480880344237e-6 mm`.
That tolerance is an OCCT importer observation, not a source-authored per-edge
acceptance specification. The STEP context declares `1e-5 mm` uncertainty;
metadata alone must not authorize a geometry repair or export.

The current Rust `projectionConverged` flag means residual below
`controlNetDiagonal * 1e-10` (about `8.18e-11 mm` here), rather than numerical
stationarity. A nonzero closest-point residual cannot satisfy that condition.
Keep the present candidate refusal until a distinct tolerance-aware acceptance
contract is implemented and tested; changing that boolean to true would hide
the discrepancy.

The local parameterization also stretches directions differently. At probe
1535, the singular-value ratio of the sampled surface Jacobian is about
`34.93`. This number depends on parameter coordinates; it is not a physical
shape invariant. It demonstrates why good UV triangle angles do not ensure good
3D triangles. It does not establish that this discrepancy causes all previously
observed reversed facets.

Executed evidence:

- [Independent four-probe report](evidence/model16-projection-audit-20261018.json),
  SHA-256 `6384fe948475f1fa55aa3c3a0c989b5ad93061a76d41295fc1fb6c56e131fb35`.
- [Execution receipt and producer-source fingerprints](evidence/model16-projection-audit-20261018.receipt.json).
  The positive journey, no-overwrite, duplicate-index and source-digest mismatch
  checks passed. FreeCAD may return zero after a macro exception; the harness
  therefore checks the expected report/refusal and unchanged bytes explicitly.
- [Audit macro](../experiments/step-evaluation/references/audit_model16_projection.FCMacro)
  and [reproduction/E2E harness](../experiments/step-evaluation/tests/projection_audit_e2e.py).
  They do not modify the source or produce model exports. Source units are
  asserted from the selected Rust context and pinned fixture, not independently
  parsed from STEP by the audit. Finite differences are diagnostic estimates.

## What to study and reuse

| Upstream / pinned revision | Useful mechanism | Tokito decision and qualification gap |
| --- | --- | --- |
| [Truck `c2a1da87313e7879b0f4efe118da8410f8f0699a`](https://github.com/ricosjp/truck/tree/c2a1da87313e7879b0f4efe118da8410f8f0699a) | Rust B-rep topology, analytic/NURBS geometry, STEP import/export and trim-aware tessellation; its STEP-to-mesh example applies assembly occurrence transforms | Best next alternative whole-model experiment. Do not replace the reader without source-face accounting: conversion errors and absent face meshes can yield partial output. Run the same corpus, count every source face and occurrence, and impose resource budgets. Transitive dependencies/MSRV and real-corpus results have not been qualified here. |
| [Curvo `6f6a51847a50092507936e9fd5735feedfecb69c`](https://github.com/mattatz/curvo/tree/6f6a51847a50092507936e9fd5735feedfecb69c) | NURBS evaluation/derivatives, projection and trimmed-surface adaptive tessellation | Useful independent evaluator/face experiment. It is not a STEP semantic or shell-closure adapter. Reviewed constraint code can omit failed insertions/constraints; wrapper checks or fixes are required before any completeness claim. |
| [Spade `c8befc96bbbc1898a89cb19f9f3104a848936374`](https://github.com/Stoeoef/spade/tree/c8befc96bbbc1898a89cb19f9f3104a848936374) | Robust 2D constrained Delaunay predicates, refinement and explicit completion controls | Retain existing exact crate `2.15.1`; add surface-space checks and shared-edge correspondence around it. A completed UV refinement does not certify 3D geometry. |
| [Ruststep `cd89e5a7cd9a6baadcb456392a5888f4cbc6b707`](https://github.com/ricosjp/ruststep/tree/cd89e5a7cd9a6baadcb456392a5888f4cbc6b707) | STEP Part 21 syntax/schema graph | Consider only if entity fidelity is the bottleneck. It does not supply surface evaluation, tessellation or solid qualification. |
| [OCCT meshing architecture, pinned documentation](https://github.com/Open-Cascade-SAS/OCCT/blob/3d097a0328e71b826377d4814ab05ec3c3d23871/dox/user_guides/mesh/mesh.md) | Shared edge discretization, face meshing, separate linear/angular deflection and shape tolerance handling | Behavioral/mathematical reference and independent oracle; no native OCCT dependency in production. Reproduce outcomes and invariants with original Rust implementations, not a translation. |

Truck's relevant example is
[`truck-stepio/examples/step-to-mesh.rs`](https://github.com/ricosjp/truck/blob/c2a1da87313e7879b0f4efe118da8410f8f0699a/truck-stepio/examples/step-to-mesh.rs).
Curvo's actual trim representation is
[`src/surface/trimmed_surface.rs`](https://github.com/mattatz/curvo/blob/6f6a51847a50092507936e9fd5735feedfecb69c/src/surface/trimmed_surface.rs).
These source inspections establish candidate capabilities, not successful
integration with Tokito. No new dependency was added by this investigation.

## Generating missing component bodies

The archived KiCad Packages3D generator has moved into the footprint-generator
repository. The current official source was inspected at
[`a903a695bf4e69905e9e56548f44dd46cdc1130e`](https://gitlab.com/kicad/libraries/kicad-footprint-generator/-/tree/a903a695bf4e69905e9e56548f44dd46cdc1130e).
Its layout has changed again: resistor SMD code is now
`src/generators/resistor/SMD/model.py`, dimensions are in
`data/resistor/SMD/cq_parameters.yaml`, and `library_utils` is being consolidated
into the same repository. Older `3d-model-generators` paths are not current at
this pin. Read-only source objects are retained in ignored
`target/research/kicad-generator-a903a695`; no generator was executed.

Observed method: the SMD resistor generator takes dimensions and terminal-band
parameters, builds distinct ceramic/body, top and metal-terminal solids,
applies rounded edges/boolean operations, and exports an assembly with separate
materials. The current dependency declaration uses CadQuery `2.6.1` and
CadQuery-OCCT `>=7.8.1,<7.9`. Automatic terminal thickness/fillet estimates in
that generator are appearance assumptions; they are not measured package data.

Tokito should implement original Rust **dimension-driven package families**
through one versioned specification:

1. Exact MPN/package and manufacturer drawing revision/hash; dimensions with
   units, min/typ/max where provided, pin count/pitch, standoff, terminal extent,
   pin-one convention and evidence for each value. Keep unprovided details
   unknown or explicitly approximate.
2. Separate body, terminals and markings/material groups; generate surfaces
   rather than count a nominal cuboid envelope as a complete component.
3. Use the same reviewed package dimensions to inform 2D and 3D generators,
   while keeping PCB land-pattern process allowances separate from physical
   terminal dimensions. A pad is not a component terminal.
4. Verify total dimensions, body height, terminal numbering/location, fit to
   source pads and local-to-board transforms before assigning to a footprint.
5. Reuse exact source-qualified library models before generating replacements.
   BOM labels/photos can propose identities; they cannot determine hidden pads,
   dimensions or internal surfaces. Report unresolved/missing/approximate data.

This is a separate workstream from repairing imported STEP. Even perfect STEP
tessellation cannot invent absent component geometry or establish MPN identity.

## Geometry pipeline and physical mathematics

```mermaid
flowchart LR
    A[Source CAD and exact component identity] --> B[Units, topology and occurrence graph]
    P[Reviewed package dimensions] --> G[Original Rust package-family generator]
    G --> B
    B --> C[Shared 3D edges and oriented surface trims]
    C --> D[Tolerance-aware adaptive surface meshing]
    D --> E[Independent dimensions, deviation and shell checks]
    E --> F[Selectable GLB plus source and quality manifest]
```

For a parametric surface `S(u,v)`, retain rational weights, knot multiplicities,
surface/edge senses, 3D boundary curves and available UV pcurves. Maintain the
separate equations `C(t)` for source edges and `S(u,v)` for the support surface;
record their measured disagreement rather than assuming exact equality.

Physical differential lengths satisfy `ds² = duvᵀ G duv`, where
`J = [∂S/∂u, ∂S/∂v]` and `G = JᵀJ`. Triangle insertion/refinement must evaluate
physical edge length, chord deviation and normal change. UV angle refinement
alone is insufficient. Prefer stable normalized/QR or damped solves over raw
normal equations when conditioning is poor; keep singularity, periodic seam,
knot-span and work-budget diagnostics explicit.

The implementation needs distinct results for `residual_met`, numerical
stationarity, singular Jacobian, iteration limit and resource refusal.
Stationarity with residual above the approved geometric tolerance must never
authorize export. Declared STEP uncertainty, measured curve/surface discrepancy,
importer-derived tolerance and Tokito's requested mesh accuracy must remain
separate values. Do not introduce a global tolerance increase to clear one face.

Discretize each topological edge once and share its samples across adjacent
faces. When a boundary split is necessary, reevaluate its source curve and
corresponding surface parameters, record the discrepancy and preserve the
approved boundary policy. Independently check trim holes/seams, normals,
deviation, self-intersection and edge incidence. Qualify claimed solids for
closure, consistent orientation and volume; distinguish intentional open
surface assets from solid components.

Use the preserved STEP/B-rep as an engineering source and a typed Tokito
geometry/provenance document as the normalized contract. Use **GLB/glTF 2.0** for
delivery and display: one selectable node per component, UUID/refdes metadata,
separate terminal/body primitives, materials, explicit transforms and metres.
GLB tessellation does not retain exact CAD surfaces or inherently supply 4K
quality. STL is unsuitable as the authoritative component/identity/material
contract. See [Khronos's specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html).

## Implemented solver-status separation (2026-10-19)

The experimental Rust diagnostic now emits additive `projectionSolver` metadata
for initial samples, retained boundary samples, source-edge refinement probes
and quality probes. It records the actual termination reason and iteration count,
residual tolerance/acceptance, final-UV finite-difference tangent observations,
and normalized Jacobian determinant. All five final-point evaluations are
charged to existing surface/control-point budgets.

The old `projectionConverged`/`multiStartBestConverged` values, solver arithmetic,
seed selection and mesh admission decisions are preserved. Stationarity evidence
cannot authorize geometry. Domain boundaries explicitly make no constrained
stationarity claim; collapsed/parallel tangents are unavailable or singular.
Finite differences measure secants near knots/seams and do not prove exact
stationarity, differentiability or a global nearest point.

The real source-face E2E passed, validating 6,248 additive observations and
refusing four malformed observation mutations. Comparing
against the previous diagnostic found all 6,248 legacy projection records equal.
Probe 1535 reports `no_decreasing_step` after two iterations and observed interior
tangent orthogonality, while its residual remains `1.07888315327835e-6 mm` against
`8.182579005321623e-11 mm` diagnostic tolerance. The Spade candidate still refuses
that exact selected probe and produces no candidate output. The existing trim
candidate still contains 3,995 nonpositive triangle-normal observations; this
change does not repair those triangles.

The expanded JSON is 9,852,339 bytes. Research-only consumer/harness input caps
are now 16 MiB; geometry inventory/work limits are unchanged. A real 16 MiB+1
input was refused with no output. The four-probe FreeCAD audit and its three
refusal controls passed again with the new diagnostic. Unit regressions cover
UV rescaling, off-surface targets, boundaries, rank deficiency, final-UV sampling
and budget exhaustion. See the [captured evidence](evidence/projection-solver-status-20261019.json).

## Physical-space refinement experiment (2026-10-19)

A separate, source-digest-pinned research mode now initializes a UV constrained
Delaunay mesh, then subdivides shared **nonboundary** edges when actual surface
centroid/edge-midpoint chord deviation or signed normal alignment fails the
requested target. It never invokes Spade angle refinement, adds boundary
vertices, changes original boundary coordinates or alters the existing candidate
refusal. This is physical-error-driven subdivision, not metric-aware Delaunay.

The loop preflight rejects crossings/degeneracy, exposes the 100-to-96 sample
mapping and four exact junction duplicates, and refuses removal of distinct
near-duplicate coordinates. Shared midpoint templates preserve conformity;
finite child UV areas, boundary/interior edge incidence and orientation are
checked. Limits: 12 rounds, 25,000 vertices, 50,000 triangles and 400,000 surface
samples, with additional existing evaluator/control-point caps. Singular,
nonfinite, no-progress and budget failures never establish acceptance.

For the pinned ESP8266 source, requested chord tolerance is `0.00001 mm` and
normal target is `cos(5 degrees)`. These are explicit experimental targets, not
values inferred from the STEP uncertainty or a source-curve repair policy.

| Measurement | Initial CDT | Final after four subdivision rounds |
| --- | --- | --- |
| Triangles | 94 | 15,740 |
| Vertices | 96 | 7,919 |
| Maximum sampled interior chord error | 0.2351396201 mm | 0.0011713830 mm |
| Maximum sampled locked-boundary chord error | 0.0002954254 mm | 0.0002954254 mm |
| Nonpositive normal observations | 29 | 2,312 |

The candidate stops **incomplete: mesh budget exhausted**; the next proposed
subdivision would exceed a mesh cap. Absolute orientation failures remain, so
the lower chord error is not evidence of a usable repaired face. Fixed boundary
segments alone also prevent the requested chord target. An earlier ear-clipped
initializer reached 0.00218186 mm/2,825 nonpositive normals; the CDT initializer
improves those final measurements without clearing either gate.

FreeCAD 1.1.4 / OCCT 7.8.1 independently evaluated all 7,919 vertices, all unique
edge midpoints and 15,740 triangle centroid/analytic-normal observations.
Maximum vertex disagreement is `2.9976021664879227e-15 mm`; independent chord
maxima and the nonpositive normal count match the Rust observations. All 254
stride-selected trim-containment centroids are inside at `1e-7 mm`. Parameter
wrapping uses Rust-reported periods, with OCCT independently reporting that V
is periodic and closed. This is not exhaustive trim coverage or an independent
parameter-correspondence proof.

The [captured evidence](evidence/physical-refinement-20261019.json) contains six
passed source/oracle/refusal checks and the separate legacy refusal journey.
Five Rust tests cover plane, concave/rescaled domains, curved fixed-boundary
obstruction, all edge-split templates, nonfinite/error inputs, zero normal
threshold and distinct boundary sample refusal. No new production dependency
or export capability is added.

```powershell
python experiments/step-evaluation/tests/physical_refinement_e2e.py `
  --binary '<experimental-build>/tokito-step-evaluation.exe' `
  --freecad 'C:\Program Files\FreeCAD 1.1\bin\freecadcmd.exe' `
  --source '<materialized-dataset>/embedded-models/16.step' `
  --output 'target/research/new-physical-refinement'
```

Next: bounded metric/chart-aware tessellation and a reviewed source-curve/
support-surface tolerance contract, then complete trim/orientation/shared-edge
and solid checks. Keep this experiment outside production geometry admission.

## Single-anchor metric preconditioning (2026-10-19)

The isolated `--research-metric-source-face` mode now computes finite-difference
surface tangents at the sampled loop bounding-box center. With `J=[Su Sv]`, it
constructs an upper-triangular matrix `M` satisfying `MᵀM=JᵀJ/s²`, where `s` is
one common positive scale. CDT uses `q=M(uv-anchor)`; every resulting vertex is
mapped back by exact identity to its original UV sample. No inverse-coordinate
roundtrip, snapping, source vertex movement or boundary subdivision occurs.

Singular/ill-conditioned tangents, excessive anisotropy, nonfinite or
nonpositive chart determinants and degenerate/reversed original-UV triangles
refuse. Exterior classification now floods from unconstrained convex-hull edges
without crossing constraint segments, replacing a near-boundary centroid test.
The preflighted simple ring, exact coordinate inventory, `n-2` initial face count
and final incidence checks remain required. This is one affine initial chart;
it does not implement a variable Riemannian metric or general chart splitting.

| Same pinned source/targets | Baseline UV CDT | Local metric initial CDT |
| --- | --- | --- |
| Final vertices / triangles | 7,919 / 15,740 | 24,911 / 49,724 |
| Initial sampled interior chord error | 0.2351396201 mm | 0.0037155393 mm |
| Final sampled interior chord error | 0.0011713830 mm | 0.0002864520 mm |
| Final nonpositive normals | 2,312 | 10 |
| Fixed-boundary chord error | 0.0002954254 mm | 0.0002954254 mm |
| Termination | Mesh-budget incomplete | Mesh-budget incomplete |

Both current-source E2E runs passed six checks each with independent FreeCAD
all-vertex, all-unique-edge midpoint, all-triangle centroid and analytic-normal
comparison. Metric vertex disagreement is at most `3.0867394805640034e-15 mm`;
nonpositive counts and minimum alignments match. Only 255 metric / 254 baseline
centroids are stride-sampled for trim containment; all are inside, without a
full-coverage claim. The legacy source-face refusal journey also passed. See
[captured receipts and producer hashes](evidence/metric-refinement-20261019.json).

Refinement is not monotonic: the metric candidate has zero nonpositive normals
in rounds 0–2, two in rounds 3–4 and ten in rounds 5–6. Sampled interior error
bottoms at `0.0002590502 mm` in round 3 and then worsens. This is evidence against
blindly raising budgets or assuming uniform midpoint subdivision always improves
physical triangles. The final mesh still has 376 failures against the
`cos(5 degrees)` normal target and misses the `0.00001 mm` chord target.

Next implement bounded interior edge retriangulation/quality-aware subdivision,
then reviewed source-curve boundary sampling and complete surface/solid checks.
Retain the incomplete status and legacy refusal until those pass. The metric
initializer itself does not repair the source curve/support-surface discrepancy.

Add `--metric` to `tests/physical_refinement_e2e.py` to reproduce the isolated
metric path with the same pinned source, FreeCAD oracle and bounded captures.

## Bounded interior edge flips (2026-10-19)

The isolated `--research-retriangulated-source-face` path adds a postprocess to
the metric candidate. It never moves vertices. An interior edge can change
only when it has two opposite incidences, its alternate diagonal is absent,
both replacement original-UV triangles are strictly positive (a convex local
quadrilateral), both replacement sampled signed normals are positive and the
pair minimum improves, and the maximum pair centroid/all-edge chord error does
not increase. Mutation happens after every check; failed evaluation refuses
the candidate. Constraints cannot flip, and newly introduced diagonals are
tracked across triangle-disjoint changes to avoid stale-adjacency conflicts.

The pass uses deterministic edge order, at most three passes / 128 attempts and
400,000 chord samples. The adapter gives postprocessing its own explicit bounded
stage: the same 50-million control-point cap as refinement, with separate
reported counts. Actual refinement and postprocess counts are 39,824,838 and
17,467,380 respectively; this is not a claim that their combined work is below
one stage's cap. Final edge incidence and all centroid/unique-edge chord errors
are recalculated. Unchanged triangles retain valid cached normals because no
coordinates move; changed triangles receive new measurements. Final status and
normal-target failures are recomputed, and refinement history is labelled as
pre-retriangulation.

The real source accepted four flips in 19 attempts / two passes. All 24,911
vertices, 49,724 triangles, 96 fixed constraint segments and source mappings
retain their inventories. Nonpositive normals fall from ten to **six**, and
`cos(5 degrees)` target failures fall from 376 to **370**. Maximum sampled
interior error remains `0.0002864520 mm`; fixed-boundary error remains
`0.0002954254 mm`, so status is **interior_retriangulation_incomplete**.
The accepted pair maxima include fixed-boundary samples; their nonincrease
does not imply monotonicity of every interior-only statistic.

The [actual seven-check E2E evidence](evidence/interior-retriangulation-20261019.json)
includes a freshly executed metric baseline, exact coordinate/mapping comparison,
full flip-log replay and independent FreeCAD all-vertex/chord/analytic-normal
comparison. The oracle additionally recomputes every accepted old/new pair's
normals and chord maxima; all four meet acceptance independently. Trim
containment remains limited to 255 sampled centroids. Existing source-face
refusal still passes without admitting the rejected projection.

Three Rust test functions exercise an analytic curved surface, no-improvement,
nonfinite/error input, concavity, worsened chord error and actual bounded-sample
exhaustion. In the analytic case the old pair maximum is exactly `21/4` and the
replacement maximum is `15/4`, with positive new normals; target alignment is
still incomplete, proving positive normals alone do not qualify geometry.

Run `tests/physical_refinement_e2e.py --retriangulate` with the same explicit
binary/source/oracle/output arguments to reproduce this isolated mode. The
remaining six inversions, boundary sampling, curve/support discrepancy,
complete trim coverage and whole-solid gates stay open. This is connectivity
improvement, not a qualified STEP repair or production exporter.

## Adaptive source-parameter boundary sampling (2026-10-19)

The isolated `--research-adaptive-boundary-source-face` mode subdivides native
source-curve parameter intervals before the metric CDT, physical refinement and
bounded interior flips. Each interval tests source-curve chord deviation at
fractions 1/4, 1/2 and 3/4 and, separately, support-surface deviation at linearly
interpolated UVs. An interval exceeding either sampled chord target splits at
its projected native-curve parameter midpoint. This is sampled evidence, not a
certified maximum-deviation bound.

All original input rows, source coordinates, UVs and solver observations remain
unchanged. Added points retain loop identity, native parameter, source position,
projected support position and separate termination/residual/stationarity
observations. Explicit original-row to expanded-row to mesh-vertex mappings
retain the four original duplicate junction samples. Reversed parameter runs
are supported; source-knot crossings, parameter/UV stagnation, invalid branches,
nonfinite data and work exhaustion refuse. A new mode is necessary because
expanded source intervals replace the original unsplit mesh constraints; older
research paths preserve their existing boundary inventories and behavior.

Sampling has separate limits of 4,096 retained input rows, depth 16 and 16,384
curve/projection probes, including discarded probes and recursive parents.
Native-curve work is capped at 20 million control-point evaluations and the
sampling stage at 50 million surface-control evaluations. Refinement and flips
retain their own explicit stage budgets. This is one face with one inventory
per source edge, not a shared-edge implementation across adjacent faces.

The independent FreeCAD audit identifies each imported edge from its complete
original point signature and requires unique one-to-one bindings. It verifies
direct parameter correspondence against every original sample before comparing
retained and discarded probes, source-curve chords, support chords and geometric
residuals. It refuses differing parameterization rather than silently fitting a
mapping. The E2E harness replays recursion and every final leaf interval and
checks source identities through the final mesh mapping. Source curve and
support-surface geometry remain distinct; the mesh still evaluates support
positions. Whole-solid, seam, complete-trim and production export gates remain
open even if this one-face experiment meets its sampled targets.

Actual pinned-source execution passed eight dedicated E2E checks, including
independent source-sample tamper refusal. It retains all 100 original rows and
adds 480 source-parameter samples, producing 576 distinct boundary vertices,
12,069 total vertices and 23,560 triangles. The candidate has **zero nonpositive
normals** throughout refinement; no interior flip is needed. Four triangles
still miss the `cos(5 degrees)` target (minimum alignment `0.9873307048`), and
sampled interior error `0.00006014931 mm` exceeds the `0.00001 mm` target. It
stops at the unchanged mesh budget, remaining incomplete and unqualified.

Sampled boundary support chord error is `0.000004617252 mm`, below the requested
target and about 64 times smaller than the previous fixed boundary. FreeCAD
independently confirms zero inversions and chord errors, all 580 retained
rows, all 3,168 quarter probes and all 576 leaf intervals. Native curve points
agree within `2.702e-15 mm`; mesh surface vertices within `2.713e-15 mm`.
Only 254 triangle centroids are checked for trim containment, all inside; this
does not establish complete trim coverage. The maximum independently observed
curve/support gap remains `0.000001080838 mm` and is not repaired.

Observed sampling work is 52,704 native-curve control evaluations and 2,649,510
surface-control evaluations. Refinement uses 11,343,780 and the final flip/audit
stage 8,285,112 surface-control evaluations, each separately below its stage
cap. Next implement priority/budget-aware conforming interior refinement and
remeasure the four normal-target failures, without moving original/source
samples or widening tolerances. This result removes the previous sampled
inversion and fixed-boundary blockers for this face, not its interior-error,
source-gap, shared-seam or whole-solid qualification gates.

The [checkpoint receipts](evidence/adaptive-boundary-20261019.json) preserve
actual eight-check adaptive and seven-check prior-mode journeys, the legacy
projection refusal, producer hashes and three byte-identical historical
diagnostic comparisons. An initial legacy command used a nonexistent hyphenated
binary name and did not reach test execution; rerunning with the actual
`model16_spade_candidate.exe` passed. This invocation mistake is not recorded as
a test pass.

## Reserved priority refinement and transition checks (2026-10-19)

The isolated `--research-prioritized-source-face` path retains adaptive source
boundary sampling and the original mesh/evaluator caps. It ranks normal-failing
triangle obligations first, then sampled chord-error ratios, with stable
triangle/edge tie-breaking. Triangle obligations select their interior edges as
groups; shared edges are counted once. For `s` interior splits on a conforming
disk, growth is exactly `+s` vertices, `+2s` triangles and `+3s` edges.

Before mutation, reserve the complete next quality pass: `F + E + 5s` chord
samples and at most `5F + E + 13s` surface calls, including four normal-derivative
evaluations per triangle. The callback reads the adapter's actual remaining
surface/control-point counters, so preflight and refinement share the same
50-million control-point cap. Midpoint coordinates reuse already measured edge
samples. Initial quality measurement is also reserved; an unavailable initial
pass refuses rather than publishing unevaluated geometry.

A geometry-blind partial-fill experiment produced 49 normal-target failures
from the prior four, despite zero inversions. It is a failed quality experiment,
not an accepted improvement. The guarded implementation preflights affected
children, tries both legal two-edge diagonals, and protects previously passing
parents from creating new normal-target failures. Already failing parents may
produce multiple positive but below-target children; global monotonic failure
counts are not guaranteed. A stricter early experiment requiring each bad parent
to retain at most one bad child stalled coarse initialization and was discarded.

Unsafe edges remove every accepted group owning them. The selected union only
shrinks; every changed neighbouring mask is rechecked. Measurements are cached
by immutable parent identity, split mask and diagonal variant. The pass limit is
16 and the child-normal evaluation limit 200,000. Before each additional normal
evaluation, the actual remaining budget must still fit that evaluation and the
entire next mesh measurement. Incomplete preflight retains the last fully
measured mesh. Actual subdivision applies precisely the checked diagonal.

Optional normal-target flips in this mode also consider positive triangles
below the target. They retain the three-pass/128-attempt bounds and require
positive replacement normals, improved pair minimum, no additional pair target
failures and no worse sampled pair chord maximum. Other modes keep their
previous inversion-only selection behavior.

The E2E harness replays every selected midpoint, subdivision/alternate template,
parent UV area, topology and resource reservation from the recorded initial CDT
through the final flip log. It compares against a freshly generated adaptive
baseline; native CAD checks remain separate independent geometry evidence.
These mechanisms do not qualify source curves, complete trims or whole solids.

Actual release-binary execution passed nine dedicated E2E checks, including
full subdivision/alternate-template/area/reservation replay and an independent
FreeCAD comparison. The final face has 24,668 vertices, 48,758 triangles and
576 fixed boundary edges, with **zero sampled inversions and zero five-degree
normal-target failures**. Minimum analytic alignment is `0.9963682806`, above
`cos(5 degrees) = 0.9961946981`. Sampled interior error is only slightly better,
`0.00005968069 mm` versus the adaptive baseline's `0.00006014931 mm`; it still
exceeds the `0.00001 mm` target. Boundary error stays `0.000004617252 mm`.

Four alternate diagonals avoid the former normal failures in round 2. In the
partial round, five preflight passes prune 220 groups, reducing the initial
12,931 selected edges to 12,599. No final cleanup flip is needed. Preflight and
refinement together use 43,343,910 control-point evaluations; postprocessing
uses its separate 17,129,610. Chord sampling uses 204,041 / 400,000. The final
round cannot reserve another full measurement pass: 123,260 surface calls
remain, below the 317,215 required even before another split. It retains the
last completely measured mesh, with incomplete status and no admission.
For zero-selection decisions, reported next-pass costs describe this unmet
hypothetical measurement requirement; no such pass is executed.

Independent native evaluation agrees with vertices within `2.720e-15 mm` and
confirms the sampled normal/chord measurements. Trim containment checks only
256 centroids, all inside; complete coverage is not established. The source
curve/support gap remains approximately `0.000001080838 mm`. The
[checkpoint evidence](evidence/priority-refinement-20261019.json) includes nine
priority checks, seven prior-mode checks, the unchanged legacy refusal, four
byte-identical historical diagnostics and explicitly limited failed exploratory
outcomes. Two old-target `LNK1104` attempts failed with unconfirmed cause;
verification used a fresh shorter D: build target. None of these results
qualifies a whole component, STEP exporter or production release.

## Ordered implementation and acceptance

| Owner | Dependency | State / evidence | Next action |
| --- | --- | --- | --- |
| Main / read-only reviewer | Failed source-curve probe | Separate solver termination/residual/orthogonality observations implemented; real source refusal and independent audit pass | Define measured curve/support-surface discrepancy contract; preserve refusal until physical geometry qualifies |
| Main | Current Spade experiment | Reserved priority splits and guarded transitions reach zero sampled normal-target failures; interior chord target remains unmet | More efficient physical-space interior meshing/error control, reviewed shared-edge discrepancy handling and general chart support |
| Main | Pinned Truck candidate | Source capability review only | Isolated original-Rust corpus adapter; fail on every conversion omission or missing face; audit selected dependency graph and budgets |
| Main | Exact source/package inventory | Existing assets/candidates only | Source-qualified resistor/IC/header family specs and full body/terminal generation; issue #24 |
| Main / independent oracle | Candidate faces and shared edges | Whole-solid acceptance open | Bidirectional sampled deviation, trim membership, seams, closed-shell topology, orientation and per-component placement |
| Main | All advertised supported profiles | Production STEP and full component generation incomplete | Integrate only profiles that pass local E2E and independent comparisons; issues #19/#24/#47 remain open |

Reproduce the audit after generating the source-face diagnostic with the current
experimental Rust binary:

```powershell
python experiments/step-evaluation/tests/projection_audit_e2e.py `
  --freecad 'C:\Program Files\FreeCAD 1.1\bin\freecadcmd.exe' `
  --source '<materialized-dataset>/embedded-models/16.step' `
  --diagnostic '<new-run>/diagnostic.json' --output 'target/research/new-audit'
```

The harness enforces a 120-second timeout and bounded process output per FreeCAD
invocation. The raw source/diagnostic hashes and generator fingerprints in the
receipt are required when comparing runs. Passing this audit verifies these
measurements and refusals; it does not repair a face, produce GLB, qualify a
component or close the production issues.
