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

## Directional interior-grid initialization (2026-10-19)

The next original Rust experiment adds `--research-grid-source-face`. It studies
Truck's pinned [directional parameter division](https://github.com/ricosjp/truck/blob/c2a1da87313e7879b0f4efe118da8410f8f0699a/truck-geotrait/src/algo/surface.rs#L219)
and [interior CDT insertion](https://github.com/ricosjp/truck/blob/c2a1da87313e7879b0f4efe118da8410f8f0699a/truck-meshalgo/src/tessellation/triangulation.rs#L570),
without adding a Truck dependency or translating KiCad code. The reviewed
Monstertruck pin `d2ad1ed7aa420b7f32cd2c1e3ad319b46d593652` also demonstrates
typed work truncation. Neither library has passed Tokito's corpus here; their
optional dependency graphs and compiler requirements remain unqualified.

Tokito clips source knot breaks to the sampled UV bounding box, explicitly
lifts periodic breaks into the selected chart, and retains every clipped knot
while adding directional dyadic divisions. It measures physical U/V midpoint
chords on boundary and midlines and **both cell diagonals**. A bilinear-only
probe misses the saddle `S(u,v)=(u,v,u*v)`; analytic regression tests cover this
case, pure-U curvature, affine surfaces and anisotropic parameter units.
One-directional refinement supplies an interior cross-section in the other
axis. Immutable UV evaluation caching records actual calls, not nominal probe
counts. These discrete samples cannot prove a continuous deviation bound.

The bounded sampler allows 257 values per axis, 8,192 interior proposals,
8,192 cells per pass, 12 subdivision rounds and 60,000 unique evaluations.
It reserves each complete sampling pass and retains the last fully measured
grid on truncation. Initial over-budget grids remain explicitly untested.
Evaluation failures/nonfinite values refuse. The real initializer stops at its
axis limit with **33 U / 130 V values**, 16,835 unique evaluations and 3,968
proposals; it does **not** claim its stricter `0.000005 mm` sampler target.

The simple-loop domain filter preserves the boundary prefix, rejects duplicate,
near-constraint and exterior proposals explicitly, and caps predicate work at
eight million. The real case retains 3,937 seeds and rejects 31 near-constraint
proposals. This conservative floating-point filter is not full CAD trim proof.
The CDT independently preserves the original sampled constraints, floods the
exterior through unconstrained edges, verifies every retained seed is used and
requires `F = 2V - B - 2`. Final subdivision checks exact boundary-edge inventory,
positive original-UV winding, opposite interior-edge incidence and Euler one.
On-boundary, duplicate and exterior-seed regressions must refuse rather than
silently split a constraint, collapse an identity or omit a retained seed.

Seeded initialization shares refinement's existing 50-million control-work cap;
it is not a hidden extra allowance. The initial quality reservation accounts for
all seed evaluations already consumed. Historical modes keep their boundary-only
initialization and byte-compatible diagnostics. Priority replay now retains the
initial UV inventory for seeded runs and replays every accepted split/template
through the final topology. The source-coordinate mappings are unchanged.

The independent real-source journey passed nine checks. After one refinement
round, the face has **16,260 vertices / 31,942 triangles**, zero nonpositive
normals and zero five-degree normal-target failures. Independent FreeCAD error
is `0.000008749145837345152 mm` interior and `0.000004617251771241803 mm`
boundary, both below the unchanged `0.00001 mm` mesh target. Minimum analytic
normal alignment is `0.9998499022957288`; maximum vertex disagreement is
`3.093e-15 mm`. No cleanup flip is needed. Refinement including grid sampling
uses 22,205,178 control evaluations. The refinement sampler records 106,064
surface evaluations (including initial vertices); the grid separately records
16,835 unique evaluations and 55,368 chord probes. Postprocessing
uses its separate 11,227,194 control evaluations. The boundary-sampling stage
still has its separately recorded existing allowance.

**Sampled face targets now pass, but the component is not qualified.** The
source-curve/support discrepancy remains about `0.000001080838 mm`. Only 256
trim centroids are checked; complete trim coverage, cross-face correspondence,
watertightness, self-intersection, whole-solid deviation and physical component
placement remain open. Status is `sampled_targets_met_unqualified`, not
`geometryQualified` or `admitted`. No production STEP export or model output is
introduced. See [checkpoint evidence](evidence/surface-grid-20261019.json).

## Shared-edge support audit (2026-10-19)

The new original Rust `--research-shared-edges` mode follows the selected
solid's actual `CLOSED_SHELL`, face bounds, loops, oriented uses and shared
`EDGE_CURVE` identities. It preserves source senses and unknown flags. For the
pinned face `2875` in solid `6218` / representation `5957`, all four edges have
two oppositely oriented uses. Edges `3478` and `9149` share the same neighbouring
face `10475`; they must not create two distinct neighbour identities. Face
`8127` has an outer bound and another bound, whose role remains
`inner_or_unspecified`. Surface `42` declares `self_intersect=.T.`: this is a
source warning, not independent proof that the physical surface intersects.
No source pcurves or per-edge tolerance are available in this fixture.

Each unique source curve is evaluated once at 16 intervals per native knot
span. The resulting 100 canonical parameter/XYZ samples are reused by both
incident supports, with explicit identity-preserving traversal permutations.
The canonical digest is SHA-256 over successive little-endian IEEE754 f64
`parameter,x,y,z` values, without a header; signed zero is retained. Python
independently replays the encoding. This fixed sampling is not adaptive edge
discretization, a complete trimmed-interval proof or a certified error bound.

Plane projection uses the declared axis placement. NURBS projection retains
stationarity/residual observations and does not prove a globally closest point.
Every projection is explicitly unverified for trim membership. Plane results
have no numerical-solver residual threshold; their threshold availability is
reported as unavailable rather than counted as successful acceptance.

| Source edge | Other face / support | Independent maximum support separation (mm) |
| --- | --- | --- |
| 3400 | 8127 / plane 6835 | 1.4756016888346606e-11 |
| 3478 | 10475 / spline 7558 | 0 at sampled parameters |
| 5409 | 8445 / spline 42 | 4.6454902159258955e-6 |
| 9149 | 10475 / spline 7558 | 0 at sampled parameters |

The nine-check native journey binds the selected face uniquely from all four
curve signatures, then identifies neighbours through shared native topological
edges. It independently evaluates all 100 curve samples and 200 support points
at Rust-reported UVs. Maximum independent point disagreement is
`2.2232198742534223e-15 mm` for curves and `2.8435583831733384e-15 mm` for
supports. It recomputes residuals, separations and maxima; wrong owner/source,
usage, overwrite and mutated-support/gap controls refuse. This corroborates
the sampled seam discrepancy; it does not repair it or prove optimal projection.

One aggregate budget covers all incident surfaces, including a reservation
before inverse-grid construction and seam evaluation. Limits are 8,192
canonical probes, 16 supports, 8,192 coarse-grid points, 65,536 retained surface
controls, four million surface calls and 50 million control-work units.
The real run charges 10,104 surface calls, comprising 9,581 explicitly measured
NURBS evaluations and 523 preparation estimates. Analytic plane projections
are separately counted (33). Charged surface-control work is 431,958,
including 25,164 preparation estimates. These bounded work units are not a
complete CPU-instruction count. Surface preparation is reused for the two
edges sharing support `7558`; allowances do not reset per neighbour.

See [captured evidence](evidence/shared-edges-20261019.json), the
[Rust topology audit](../experiments/step-evaluation/src/shared_edge_topology.rs),
[support evaluator](../experiments/step-evaluation/src/spline_face_diagnostic/shared_edges.rs)
and [native comparison](../experiments/step-evaluation/tests/shared_edges_e2e.py).
All admission/qualification flags remain false; no geometry is exported.

## Pinned planar face with an additional bound (2026-10-19)

The next bounded Rust experiment handles only source face `8127` in the pinned
ESP8266 `16.step` model (`representation 5957`, owner `6218`, SHA-256
`7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f`). The
face uses plane `6835`, an outer loop with four line edges, and a second loop
carried by a generic `FACE_BOUND`. For this face, the source declares exactly
one `FACE_OUTER_BOUND`; ISO 10303-42 defines the other loops as inside that
outer loop, with the face interior outside those remaining loops. The selected
generic `FACE_BOUND` therefore has inner-boundary semantics and is triangulated
as a hole after strict containment and non-intersection checks. The experiment
preserves both the raw STEP entity type and the interpreted face role. This
classification is specific to a face with one explicit outer bound; the
general topology inventory continues to preserve role uncertainty for other
faces until their complete surface/embedding context is evaluated.

Reference: [ISO 10303-42:2021 geometric and topological schema](https://www.steptools.com/stds/smrl/data/resource_docs/geometric_and_topological_representation/sys/5_schema.htm), clauses 5.5.18–5.5.20 (`face_bound`, `face_outer_bound`, and `face`).

The research command evaluates full clamped B-spline edge domains at an
explicit quarter/midpoint chord target, validates line endpoints against their
STEP line, preserves source edge and vertex identities, projects samples into
the plane frame, checks the sampled rings for crossings/touches, and meshes
the outer loop plus the candidate hole. It checks triangulated area and mesh
edge incidence and emits the vertices and triangles only in a diagnostic JSON
report:

```powershell
tokito-step-evaluation --research-planar-multibound-face 8127 `
  --chord-tolerance 0.00001 `
  --source-representation 5957 --source-owner 6218 `
  '<materialized-dataset>/embedded-models/16.step'
```

The real source run produced 516 boundary vertices and 516 triangles. Rust and
the Python oracle check all 132,870 triangle pairs with a bounded 2D
separating-axis test, in addition to mesh-edge incidence, area and sampled
material checks. Its sampled material area is
`24.61191117813682` source-units²; the independent FreeCAD 1.1.4 / OCCT 7.8.1
trimmed face measures `24.611904380234375` source-units². FreeCAD binds the two
sampled loops to distinct source wires (four and two edges) with maximum point
gaps of `0` and `1.9984e-15` source units. All 516 triangle centroids are inside
the uniquely matched native face, minimum triangle/native-face-normal
alignment is `1.0`, and the maximum sampled quarter/midpoint deviation is
`4.6221e-6` source units against a requested `1e-5` target. The FreeCAD oracle
uniquely assigns the two source rings to two imported wires and finds a unique
sampled geometric correspondence from each of the six ordered STEP
`EDGE_CURVE` IDs to an imported edge, checking source samples and both
endpoints. The largest retained source-edge sample-to-imported-edge gap is
`1.9984e-15` source units (the explicit acceptance ceiling is `1e-7`). This
does not prove exact topological identity or continuous curve agreement, nor is
it a certified maximum curve error or tolerance policy. Source topology now
identifies the second loop as an inner boundary for this planar face, but this
does not qualify the complete trimmed surface, any non-planar face, or the
solid.

The ring-crossing and point-containment checks now use the first-stage
binary64 `orient2d` determinant error filter described by Shewchuk. The code
accepts a sign only when it lies outside the calculated error bound; ambiguous
signs, non-finite arithmetic, and subnormal/underflow cases fail closed. It
does not implement Shewchuk's adaptive exact-arithmetic fallback, so this is a
conservative filter rather than an exact predicate: it may refuse valid but
ill-conditioned input and does not certify the mesh or source curves. Unit
regressions cover nearly-collinear points, near-crossing segments, near-boundary
containment, a subnormal determinant from normal products, an underflowed error
bound and product underflow. Reference: [Shewchuk, *Adaptive Precision
Floating-Point Arithmetic and Fast Robust Geometric Predicates*](https://people.eecs.berkeley.edu/~jrs/papers/robustr.pdf).

The [five-check local E2E evidence](evidence/planar-multibound-face-20261019.json)
records the pinned source, current release binary, Rust/Python inputs, FreeCAD audit
and v15 receipt hashes. It tests source/wire/edge binding, triangle overlap,
wrong-owner/source refusal and FreeCAD output no-overwrite behavior. This
candidate does not repair or mutate the STEP file and is not a production
export. It verifies one face only; the complete component still has
unqualified non-planar faces, shell validity, self-intersection, volume,
occurrence frame, and component-placement obligations. The source shell's
topological closure preflight remains distinct from a geometric closed-solid
proof.

## Canonical two-face patch (2026-10-19)

The next Rust experiment constructs an actual open mesh for source faces
`2875` / `10475`, with one physical vertex inventory shared by their source
edges. Their supports `10410` / `7558` have identical control data but their
face loops trim complementary halves of a V-closed surface. Reusing one face's
UV polygon for both would mesh the same half twice. Each face therefore builds
and unwraps its own ordered loop, retaining distinct UV views of shared XYZ.

All six curves require clamped full native parameter intervals with endpoints
exactly matching their referenced source vertices. The experiment refuses
closed-edge interval ambiguity and endpoint mismatch. Adaptive source-parameter
sampling preserves knot breaks and measures quarter/midpoint chord error;
one canonical sequence is then used by both incident faces. This is sampled
evidence, not a continuous curve-deviation certificate or general STEP trim
support. The second face contributes curves `9005` / `4186` on edges
`7239` / `4818`; it does not borrow the first face's unrelated boundary.

The canonical registry keys endpoints by `VERTEX_POINT` identity, edge interiors
by edge identity plus exact parameter bits, and interior mesh points by face
and local vertex identity. Coincident but distinct source IDs are not welded.
Both shared edges (`3478`, `9149`) retain identical physical vertex sequences
with opposite use. Both faces are disks; their union is an annulus-like open
patch with Euler characteristic **0 and two open boundary cycles**. It is not
a closed component body.

`refine_canonical_seeded` takes fixed boundary XYZ separately from the support
evaluator. It applies those coordinates before the first quality evaluation;
surface probes and derivative queries continue evaluating the original surface.
Interior refinement therefore measures actual triangles with canonical
boundaries, rather than overwriting vertices after checking a different mesh.
Regression tests use an offset analytic boundary to demonstrate that measured
error changes, while ordinary support-only behavior remains unchanged.

The opt-in research command requires an explicit positive boundary displacement
allowance no larger than half the chord target. This is an experimental mesh
policy, not automatic acceptance of STEP uncertainty or modification of source
surfaces. Source coordinates are unchanged; mesh boundary displacement from
each support is measured separately and over-allowance points refuse. The
current pair's maximum sampled boundary displacement is approximately
`1.08083727943e-6 mm`, below its explicit `5e-6 mm` allowance. The original
source-curve/support discrepancy is not repaired or erased.

The first strict-target source run has **32,454 global vertices / 63,884
triangles**, 1,086 unique canonical boundary vertices and 64 shared seam
segments. Each face has 16,260 local vertices / 31,942 triangles. The separate
lifted V intervals are approximately `[-0.70248046153044,0]` and
`[0,0.70248046153420]`. Sampled interior chord errors are approximately
`8.7491458e-6 mm` and `8.7488717e-6 mm`; boundary errors are approximately
`4.732543e-6 mm` and `4.764120e-6 mm`. Both remain below `1e-5 mm`, with no
sampled nonpositive normals or five-degree normal failures.

One aggregate limit covers both surfaces' preparation, projection, grid
sampling, quality and refinement: four million surface calls and 50 million
control-work units. The observed run uses 880,980 call units / 47,572,920
control-work units, including 442 preparation-call estimates / 23,868
preparation-control estimates. Curve sampling uses its separate 32,768-probe /
20-million-control-work limits and bounded retained curve controls. Mesh caps
are 50,000 global vertices and 100,000 triangles. Exhaustion refuses rather
than approving a partial unmeasured patch.

The native acceptance journey must bind both faces independently from their
respective four curve signatures, replay all six edge intervals, compare
canonical/source and interior/support points, and recompute triangle chord and
analytic-normal measurements from final mesh coordinates. Native trim probes
must identify the complementary regions explicitly; finite probes do not prove
complete trim coverage or exclude every self-intersection. See
[checkpoint evidence](evidence/shared-patch-20261019.json). Full trim coverage,
geometric self-intersection, whole-solid qualification and production STEP
admission remain false even when sampled patch checks pass.

The executed native journey passes **14 checks**. Its unique four-curve-per-face
bindings independently select native faces 231 and 238. All six curves, 1,086
canonical vertices, 3,264 leaf-quarter probes, all 32,454 global mesh vertices
and all 63,884 triangles are audited. Source-curve point disagreement is at most
`2.817430187029739e-15 mm`; interior support-point disagreement is at most
`3.750188105401294e-15 mm`. Native chord/analytic-normal measurements agree
with the producer; all 512 sampled trim centroids (256 per face) are inside
their corresponding native faces. These 512 probes are not full trim coverage.
All six ascending native topological edge parameter ranges exactly match the
reported source domains and retained sample endpoints. Native edge traversal
orientation is recorded separately. A deliberately changed domain is rejected
by this independent range gate; evaluating the untrimmed curve alone would not
have established topological interval correspondence.
Wrong inputs, invalid/excess/insufficient boundary allowances, overwrite,
altered physical points, seam identities and UV mapping are rejected.

## Selected solution and next implementation gates

The upstream checks were refreshed on 2026-10-19. The official footprint
generator URL now redirects to [KiCad Library Tools](https://gitlab.com/kicad/libraries/kicad-library-tools),
which groups footprint, symbol and 3D generators. The retained revision above
remains the reproducible code reference; do not follow a mutable branch when
implementing package arithmetic. [Truck](https://github.com/ricosjp/truck)
remains the alternative pure-Rust B-rep/STEP experiment, not an already
qualified replacement. [OCCT shape healing](https://dev.opencascade.org/doc/overview/html/occt_user_guides__shape_healing.html)
documents separate analysis, modification and status reporting, including
curve/pcurve deviation, missing seams, wire orientation and tolerance limits.
Its native implementation remains a development reference only.

Implement two complementary routes, with the existing source/identity contract:

1. **Existing STEP:** keep exact units, occurrence transforms and face ownership;
   advance this audit to adaptive shared edge samples and explicit UV trims,
   including holes and periodic seams. Use one canonical edge inventory across
   incident faces. Evaluate physical chord and normal error, not UV angles alone.
2. **Missing package bodies:** use the reviewed manufacturer/package dimensions
   to generate a small supported family in original Rust. Start with SMD
   resistor bodies and distinct terminals, then leaded IC/header families.
   Preserve the complete drawing, terminal and material evidence. A footprint
   pad pattern alone is insufficient to generate a qualified package body.

The implemented patch begins a **local boundary reconciliation policy**:
record source uncertainty, requested mesh error, measured curve/support gaps,
any actual boundary displacement and the affected source IDs separately. A
sampled discrepancy below a mesh target does not itself approve a repair.
Proposed canonical-edge meshing must demonstrate that both incident face
boundaries conform to the same samples while staying within the reviewed
physical error allowance. Preserve the source unchanged and record every
correction. Refuse ambiguous projection, excess error, unknown trims or budget
exhaustion; do not silently weld nearby vertices or globally inflate tolerance.
The preceding shared-edge audit only measures these quantities; the new patch
implements opt-in canonical boundary anchoring for the pinned two-face case.
General profiles, ambiguous intervals and full-shell adoption remain unqualified.

Then qualify each complete component: all faces accounted for, trimmed holes
preserved, closed consistently oriented shell when a solid is claimed,
nondegenerate triangles, self-intersection checks, bidirectional reference
deviation, volume and dimension comparisons. Compare placement, rotation and
top/bottom transforms separately at board level. Only qualified profiles may
enter the production exporter. Unresolved models stay explicitly incomplete.

Use STEP/B-rep plus a provenance manifest for engineering source data and
GLB for selectable presentation. One component UUID/refdes remains the parent
of its body/terminal/material primitives. 4K is a render-size option, independent
of geometric accuracy. Neither a format conversion nor increased triangle
count repairs a bad source or supplies missing physical dimensions.

## Ordered implementation and acceptance

| Owner | Dependency | State / evidence | Next action |
| --- | --- | --- | --- |
| Main / read-only reviewer | Failed source-curve probe | Solver-status/shared-edge audits and explicit canonical anchoring for the pinned two-face patch implemented; native patch journey passes 14 checks | Generalize reviewed boundary reconciliation to complete trims and source faces; preserve production refusal until physical geometry qualifies |
| Main | Current Spade experiment | Directional seeding and canonical-boundary refinement meet both complementary faces' sampled targets; open-patch seam topology checked | Full trims/solid comparison, holes and general chart support |
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
