# STEP references with independent solid ownership

This development-only comparison supports issue #19. The product engine remains
original Rust with pure Rust dependencies. FreeCAD is an independently pinned
reference tool, outside Tokito's product export workflow.

## Real source result

The saved compound mesh for ESP8266 `14.step` had 6,625 edges with four incident
triangles, zero boundary edges and balanced directions. Flattening had discarded
solid ownership. The new FreeCAD 1.1.4 capture preserves that compound and also
tessellates every solid independently at 0.01 mm deflection, without merging
indices between solids. The first real v2 run found:

| Pinned source | Reference solids passing strict checks | Tokito reader result |
| --- | --- | --- |
| `0.step` | 5 / 5 | Bounded surface comparison passed; production admission false |
| `14.step` | 41 / 41 | Rejected: four degenerate triangles on source face `#51071` |
| `16.step` | 30 / 30 | Rejected: 1,102 degenerate triangles on source face `#2875` |

All 41 independent solid meshes in `14.step` are closed, direction-balanced and
positively wound. Its compound still has all 6,625 incidence-four edges. This
resolves the reference ownership ambiguity without repairing the Rust reader
or qualifying an assembled board.

First receipt:
`F:\kannan\projects\tokito-cli-verification\step-solid-reference-20261016a\receipt.json`.
Its overall result is **failed**: the two reader failures remain. That run
predates the added tool-fingerprint and volume-completeness fields; a follow-up
capture uses the final comparison implementation.

## Versioned evidence contract

The multi-solid generator emits `tokito.freecad-reference-mesh.v2`:

| Field | Meaning |
| --- | --- |
| `inputSha256`, `datasetCommit`, `modelFile` | Exact source identity at dataset commit `5283bc059e30e51f4d3a81a674425936fc213214` |
| `freecadVersion`, `deflectionMm` | Pinned reference tool and meshing setting |
| Top-level `verticesMm` / `triangles` | Original compound tessellation, retained as diagnostics |
| `solids` | Complete ordered solid list with independent mesh indices and B-rep validity/face counts |
| `solidIndex` | Ordinal in this exact import; not a STEP entity ID or cross-version component identity |

Every solid requires exactly two incident triangles per edge, opposite edge
directions and positive signed volume. Missing/duplicate ordinals, invalid
meshes and incomplete inventories fail. Equal coordinates across solids keep
separate indices. Sampling concatenates arrays with offsets and no welding.
The signed solid-volume sum is explicitly not geometric-union volume. If any
volume is unavailable, the sum is null and its completeness flag is false.
Failed solids remain visible. Compound and summed per-solid face counts are
recorded without assuming identical shared-face ownership semantics.

Compound and combined per-solid representations each have one fixed
300,000-vertex/500,000-triangle budget. The per-solid budget is shared by all
solids, not multiplied by their count. Together the two representations can
use at most 600,000 vertices and 1,000,000 triangles; the complete JSON also
retains the 64 MiB byte cap. Nothing is silently truncated.

Multi-model and triangle-removal receipts use v2. Legacy multi-solid v1 captures
remain historical compound evidence and cannot acquire invented ownership.
The hash-pinned single-solid v1 fixture and acceptance path stay unchanged.
Additive integrity diagnostics do not weaken their gates.

## Implementation and tests

| File under `experiments/step-evaluation` | Responsibility |
| --- | --- |
| `references/generate_multi_reference.FCMacro` | Pinned compound and independent solid capture |
| `tests/multi_surface_reference_e2e.py` | Strict validation, bounded surface comparison and failed receipts |
| `tests/surface_reference_e2e.py` | Shared edge, winding and volume checks |
| `tests/diagnose_degenerate_surface.py` | Exploratory triangle-removal diagnostic; never product admission |
| `tests/test_solid_reference.py` | Eight ownership, refusal and receipt regressions |

The tests cover contact geometry, missing/duplicate ordinals, a missing face,
reversed winding, invalid/degenerate/nonfinite meshes, aggregate limits, legacy
refusal and failed-receipt retention. The receipt unit test injects explicitly
synthetic evaluator data; it is not native CAD E2E evidence.

The base verifier adds `step-reference-tests`, bringing its profile to 41 gates.
Development-tooling tests passing do not make the optional real STEP
qualification run pass. Its failures continue to block production admission.

To repeat the real comparison:

```powershell
python experiments/step-evaluation/tests/multi_surface_reference_e2e.py `
  --binary C:\path\to\tokito-step-evaluation.exe `
  --freecadcmd 'C:\Program Files\FreeCAD 1.1\bin\FreeCADCmd.exe' `
  --models-root C:\path\to\pinned\embedded-models `
  --output C:\path\to\new-reference-run
```

Paths are templates. Source hashes and tool version must match the manifest;
output must be new. The comparator records its script, shared helper and macro
hashes and detects changes during execution. Known reader failures produce a
failed receipt and nonzero exit. No GLB is emitted.

## Next geometry gate

Implement original Rust linear planar-face triangulation for `14.step` face
`#51071`, preserving every boundary segment, orientation, units and provenance.
Compare against the validated solids and independent face evidence. Investigate
`16.step`'s rational spline separately. Deleting degenerate triangles, relaxing
closure or relying on volume alone is not a repair. STEP, component association,
PCB pose and App/AR admission remain unqualified.

## Original Rust planar-face diagnostic (2026-10-16)

The evaluation binary now accepts an explicit, non-admitting inspection mode:

```powershell
tokito-step-evaluation --planar-face 51071 --source-tolerance 0.000001 C:\path\to\14.step
```

This bounded profile supports one explicit PLANE with one outer LINE edge loop.
It resolves source vertex identities, edge/curve/bound/face senses and plane
axes, checks line and plane residuals, refuses intersecting or unsupported
boundaries, and triangulates while preserving every boundary segment. It does
not replace the reader mesh. The tolerance is explicitly in source units.
Output always has `admitted: false`, no exports, and process exit 2; successful
face inspection is not successful model qualification.

On the pinned ESP8266 dataset commit
`5283bc059e30e51f4d3a81a674425936fc213214`, model `14.step` SHA-256
`78db318947fa4d67e3d1f5ad9e7fbe3c44c9b0ffca621a8b8a0bb6dd93282b2b`,
face #51071 produces 146 triangles from 148 vertices. Maximum residuals are
2.6017241161468974e-7 source units against LINE geometry and
3.885780586188048e-16 against the plane, within the explicit 1e-6 tolerance.
An independent Python Decimal check verifies nonzero triangle areas, winding,
total projected area, every boundary segment and balanced interior edges.

```powershell
python experiments/step-evaluation/tests/check_planar_receipt.py RECEIPT.json SOURCE.step
```

The ignored local receipt is
`F:\kannan\projects\tokito-cli-verification\planar-face-final-20261016.json`.
Exact bounded integer accumulation distinguishes genuine binary64 collinearity
from cancelled floating products; uncertain nonzero orientation still refuses.
Regression tests include the cancelled-zero determinant, subnormal/extreme
inputs, 10,000 integer-reference comparisons, source senses/scales, malformed
geometry, work caps and executable success/refusal diagnostics. Independent
source review found no concrete blocker; this is not whole-body qualification.

Remaining gates: establish face-owned representation units and reader/model
frame conversion, integrate without changing other faces, compare repaired
whole solids against the pinned independent reference, then address #2875's
rational-spline defect separately. Full STEP, component geometry, placement,
KiCad parity, 4K rendering and electrical/view features remain unqualified or
unfinished where listed in the v1 plan.

## Explicit face representation and units (2026-10-16)

An optional selection now checks topology membership and resolves that
representation's context directly, without relying on truncated global rows:

```powershell
tokito-step-evaluation --planar-face 51071 --source-tolerance 0.000001 --source-representation 58073 C:\path\to\14.step
```

On the same pinned file, representation #58073 contains solid #46427, whose
outer shell #20483 contains face #51071. Context #50436 declares length unit
#35545 as millimetres (`metresPerUnit: 0.001`). The actual diagnostic resolves
these identities and units, retains 148 vertices / 146 triangles, and passes
the independent decimal receipt checker. Local receipt:
`F:\kannan\projects\tokito-cli-verification\planar-face-owned-20261016.json`.

This is explicitly selected membership, not enumeration of every possible
owner. At most 512 representation items share the existing 100,000-work,
32-depth topology budget. Duplicate/dangling/cyclic/unsupported topology,
multiple containing items, missing or ambiguous length units, non-3D context
and invalid CLI selections refuse. A placement item in a representation is
not implicitly applied to every solid; assembly/model-frame conversion remains
unverified. Selected context evidence is nested in
`selectedSourceRepresentation`; conservative top-level admission flags remain
false. No production model or assembly repair is emitted.

Actual STEP experiment tests pass: 42 unit tests and six subprocess integration
tests, including selected representation acceptance and malformed selection
refusals. Clippy passes. A transient Windows linker file lock initially failed
two attempts; the subsequent complete test run passed. Independent read-only
review found no concrete blocker in this bounded diagnostic slice.

## Final isolated capture

The completed frozen-tool capture is
`F:\kannan\projects\tokito-cli-verification\step-solid-reference-20261016c\receipt.json`.
All 76 reference solids pass, tool-input fingerprints remained unchanged, and
the same two reader failures remain explicit. FreeCAD user/system settings and
logs were isolated per model inside the run using its documented configuration
flags. The preceding `20261016b` run failed on default-profile configuration
write/parse errors and remains failed. Normal FreeCAD settings were not repaired
or deleted. The single-solid v1 regression also passed (maximum sampled distance
4.58784e-15 mm), without granting production STEP admission.
