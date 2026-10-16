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
