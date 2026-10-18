# Experimental STEP reader qualification probe

The explicit `--negative-torus-compat` research option retains original warnings,
records cloned-AST edits/fingerprints and remains unqualified. Its face-count
recovery on `57.step` is contradicted by large nearest-reference surface
discrepancies. See the [actual dataset evidence](../../docs/DATASET_STEP_QUALIFICATION.md#negative-major-radius-torus-experiment).
The `torus_surface_evidence` example produces bounded research samples only;
successful report generation never means production admission.

This isolated pure-Rust experiment evaluates the exact crates.io reader `oxideav-step = 0.0.1`, with default features disabled. It is not a Tokito production adapter, does not produce GLB, and always reports `admitted: false` and exits **2**, including when the reader returns a mesh without warnings. Existing CLI STEP support is not changed.

```powershell
cargo run --locked --manifest-path experiments/step-evaluation/Cargo.toml -- "explicit/materialized/model.step"
```

The digest-pinned ESP8266 model-16 investigation also has a read-only source
face command. It traces one `ADVANCED_FACE` through its rational B-spline
surface, bound, oriented 3D edge curves, and bounded inverse-projection
diagnostics:

```powershell
cargo run --locked --manifest-path experiments/step-evaluation/Cargo.toml -- `
  --diagnose-source-face 2875 `
  --source-representation 5957 path\to\embedded-models\16.step

python experiments/step-evaluation/tests/model16_source_face_trim_e2e.py `
  --binary path\to\tokito-step-evaluation.exe `
  --source path\to\embedded-models\16.step `
  --output target\model16-source-face-trim-e2e
```

This diagnostic always exits **2**, produces no export, and leaves admission
false. Its shared-kernel inverse scan has a separate aggregate work budget.
When an explicit shape representation is supplied, the probe verifies that
the representation contains the selected face and reports the context's
declared length uncertainty after unit conversion. This is provenance and
diagnostic metadata only; it does not relax numerical projection status or
serve as an acceptance tolerance.
The optional triangulation is a research candidate only. On pinned ESP8266
model 16, the current bounded run builds 12,801 vertices and 24,064 triangles;
sampled chord error is within tolerance, but the per-triangle centroid normal
gate still finds 3,995 nonpositive alignments. FreeCAD 1.1.4 independently
reproduces that count and minimum alignment. The diagnostic exits **2**, keeps
geometry unqualified, and produces no export. Centroid sampling is not a proof
of orientation across every point on a curved facet, and this single-face result
does not qualify a complete STEP solid. The E2E receipt confirms bounded
evidence and refusal behavior, not geometry quality.
The current evidence and the independent FreeCAD comparison macro are described
in [the face #2875 analysis](../../docs/STEP_FACE_2875_ANALYSIS.md).

The latest bounded FreeCAD audit samples the 32 candidate triangles with the
most negative centroid-normal alignment at each vertex, edge midpoint, and
centroid. All 224 sampled face normals remain nonpositive against their
candidate triangle normals. The lowest-U bin has 3,130 failures among 8,981
triangles (34.85%); the worst triangle's seven samples remain near -1
alignment. This localizes a stable orientation conflict but does not identify
whether the cause is UV connectivity, a singular parameterization,
triangulation, or source geometry. FreeCAD evaluates the Tokito triangles; it
has not produced an independent triangulation. The candidate stays refused and
unexported.

### Source-curve midpoint Spade remesh (2026-10-18)

A separate research candidate uses a constrained Spade remesh of the same
hash-pinned model-16 face. It starts with source-backed boundary samples, lets a
quality pass identify four needed boundary splits, and associates each split
with a unique source interval using its UV chord midpoint. For each matched
source edge, the STEP curve is evaluated at the interval's arithmetic parameter
midpoint, projected to UV, and used as the constrained boundary point. The final
remesh preserves constrained edges. The E2E independently rebuilds the source
boundary inventory and recomputes mesh incidence and trim containment; source
curve and surface evaluations themselves still come from the Rust diagnostic,
not an independent STEP evaluator. Of the four selected source-curve midpoint projections, one is marked non-converged at a residual of `1.07888315327835e-6` source units, below the `1e-5` probe-selection ceiling. That ceiling is not an admission tolerance or proof that the projection converged.

Dataset source revision: `5283bc059e30e51f4d3a81a674425936fc213214`; STEP
SHA-256: `7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f`.
The local E2E receipt is
`output/model16-source-curve-quality-e2e-v8-20261018/receipt.json` (SHA-256
`56840b9e6ccd461e7402a16a708638ff82d795589535639890bb4a97cf4797fe`); the
candidate SHA-256 is
`99b666928b6a0952c5669a419b35c59776ce2264b4a9e6b07db0012acc7a496a`.
The independent UV checks passed: 20,069 vertices, 57,128 edges, 37,060
triangles, 3,076 source-provenanced boundary vertices, Euler characteristic 1,
no non-manifold edges, no inconsistent interior edge winding, and no tested
vertices or edge midpoints outside the trim. UV area was
`0.9869575847275533` versus `0.9869575847275449` for the source trim, and the
minimum UV angle was `20.00227905484014` degrees. The optimized containment
oracle was checked against the original predicate on 1,089 points over the
actual trim plus 20 endpoint, sloped-edge, horizontal-edge, and bucket-seam
regression points. This is differential evidence for this normalized fixture
and these cases, not a proof over every finite floating-point input.

This is still only UV-domain/topology evidence for one face. Full 3D surface
deviation was not performed; the preceding source-surface candidate's normal
orientation failure remains unresolved; and this is not whole-solid
qualification. The candidate reports `geometryQualified=false`, `admitted=false`,
and `exportsProduced=[]`. It does not change production STEP/GLB
export.

To reproduce with a materialized dataset source and local Rust release tools:

```powershell
$source = 'path\to\embedded-models\16.step'
cargo build --locked --manifest-path experiments/step-evaluation/Cargo.toml `
  --release --bin tokito-step-evaluation --bin model16_spade_candidate `
  --target-dir target\model16-source-curve-quality-probes
python experiments/step-evaluation/tests/model16_source_face_trim_e2e.py `
  --binary target\model16-source-curve-quality-probes\release\tokito-step-evaluation.exe `
  --candidate-generator target\model16-source-curve-quality-probes\release\model16_spade_candidate.exe `
  --source $source `
  --output target\model16-source-curve-quality-e2e
```

One explicit input path is required. The probe refuses Git LFS pointer text and input beyond 32 MiB before parsing. It caps parser instances at 250,000, parameter depth at 48, individual strings at 1 MiB, triangles at 1,000,000 and curve samples at 16,384. Absolute chordal tolerance requested from the reader is 10 micrometres. These are resource controls, not a hard process timeout or a proven worst-case memory bound.

The output is machine-readable JSON. A reader-returned model includes source SHA-256, declared AP schema, reader-reported metres per output unit, part/root/child occurrence counts, triangles, shape/triangle colour counts, triangle face provenance entries, nonfinite positions, duration, and warning total. Up to 128 warning details of 512 Unicode characters each are retained; truncation is explicit. Any reader warning adds `reader_warnings_or_partial_geometry` to the rejection reasons. Fatal errors emit JSON and exit 2 without fabricated geometry fields.

All model-derived fields remain **unqualified reader observations**. Missing units can be defaulted by the upstream reader; units are not independently preflighted. Faces or representation items may be skipped; a nonempty mesh, provenance count, zero warnings or the requested tolerance does not establish completeness or geometric accuracy. Invisible items are omitted under the configured reader policy. Bounds union each shape's part-local coordinates, in the reader's model length units. Occurrence transforms are counted but not applied; bounds are explicitly not assembly-world bounds. Assembly placement, unit harmonisation, face completeness and engineering parity still require independent evidence.

## Dependency qualification

`Cargo.lock` pins resolved dependencies. `dependency-audit.json` records metadata package versions, native `links` declarations and target kinds. `windows-dependency-tree.txt` captures the actual Windows normal/build graph: Rust crates, no C/C++ CAD kernel, no `cc`, `cmake` or native geometry linkage. The SHA-256 dependency's CPU feature implementation uses platform intrinsics; other-target metadata includes `libc` for platform feature probing, not a CAD kernel. This is not an all-platform runtime qualification. The `registry`/mesh registry features of oxideav-step are disabled.

## Executed local verification (2026-10-10)

- `cargo fetch --manifest-path experiments/step-evaluation/Cargo.toml` resolved the pinned dependencies.
- `cargo fmt --manifest-path experiments/step-evaluation/Cargo.toml` completed.
- `cargo test --locked --manifest-path experiments/step-evaluation/Cargo.toml` passed **3 unit tests**: LFS rejection, streaming input byte cap, and warning/default non-admission reasons. The subsequent offline/locked run also passed **2 executable integration tests** for LFS-pointer and malformed STEP refusal, asserting exit 2, JSON non-admission and no exports. These subprocess journeys are reproduced by the root verifier.
- `cargo clippy --locked --manifest-path experiments/step-evaluation/Cargo.toml --all-targets -- -D warnings` passed after replacing field reassignment with an explicit options initializer.
- `cargo build --locked --manifest-path experiments/step-evaluation/Cargo.toml` passed.
- The built executable ran against explicit LFS-pointer and malformed negative files in its ignored `target/negative-e2e` directory. Both emitted parseable JSON with `admitted: false`, expected errors, and exit 2. `negative-e2e-results.json` records the actual results.
- `cargo tree --locked --manifest-path experiments/step-evaluation/Cargo.toml --target x86_64-pc-windows-msvc -e normal,build` and `cargo metadata --locked --manifest-path experiments/step-evaluation/Cargo.toml --format-version 1` were inspected for the dependency audit.

One intermediate test relink returned Windows LNK1104 (cannot open the test executable); the same exact test command reran successfully. No compile or test failure is hidden by that later result.

No materialized dataset STEP was available during the initial validation above. Subsequent runs on three pinned Ultimainboard assets returned meshes and were compared with independent FreeCAD bounds and face counts; see [dataset qualification](../../docs/DATASET_STEP_QUALIFICATION.md). These observations do not establish surface accuracy, complete geometry, KiCad parity or assembly placement. Production admission remains false.

Per-shape provenance summaries now retain stable part/shape indices and representation-item IDs, distinct nonzero face-ID counts, unassigned entries and triangle/provenance count mismatches. At most 128 summaries are retained with explicit total/truncation; aggregate diagnostics cover all shapes. `sourceFaceCompletenessVerified` is always false. The 2026-10-15 current-source run passed 34 unit tests, 5 executable integration tests, formatting, warnings-denied Clippy and the isolated build.

## Pinned sampled-surface comparison

`tests/surface_reference_e2e.py` compares the experimental mesh for one
hash-pinned ESP8266 model against the checked-in FreeCAD 1.1.4 reference in
`references/`. The harness validates the source hash, one-occurrence/one-shape
profile, unit-preflight result and clean reader diagnostics, then samples every
triangle's vertices, edge midpoints and centroid in both directions. A shifted
reference is a negative control for the distance threshold.

The reference can be regenerated with the checked-in macro using the pinned
FreeCAD 1.1.4 installation and the materialized dataset source. It refuses to
overwrite its output; compare the generated digest with the pinned reference
before updating the fixture:

```powershell
$freecadCmd = 'C:\Program Files\FreeCAD 1.1\bin\freecadcmd.exe'
$macro = 'experiments/step-evaluation/references/generate_reference.FCMacro'
$source = 'path/to/tokito-app-dataset/data/board-sources/esp8266-nodemcu-reference/structured-source/embedded-models/15.step'
$output = 'target/esp8266-model-15-freecad-reference.json'
& $freecadCmd $macro --pass $source $output
Get-FileHash -Algorithm SHA256 $output
```

The harness streams child stdout/stderr under separate byte caps and kills an
over-limit or timed-out child. It rejects degenerate mesh faces, checks closed
edge incidence and consistent triangle winding, compares signed volume, and
tests a reversed-face negative control. Its aggregate pairwise work budget
covers both directed comparisons and the shifted-reference negative control.
The report records the reference and evaluator-binary SHA-256 digests. It does
not compare per-face normal vectors.

```powershell
python experiments/step-evaluation/tests/surface_reference_e2e.py `
  --binary path\to\tokito-step-evaluation.exe `
  --source path\to\materialized\embedded-models\15.step `
  --output target\step-surface-reference.json
```

The pinned comparison passed on dataset source revision
`5283bc059e30e51f4d3a81a674425936fc213214`: 40 matching vertices, 76 triangles
on each mesh, 46 identical triangle coordinate triplets, and a maximum sampled
distance of `4.59e-15 mm` in either direction. The harness's pass means only
that these bounded samples match. It is neither a full Hausdorff/trim-coverage
proof nor component identity/placement verification. The evaluator still exits
2 with `admitted:false` and emits no GLB. See the [current goal evidence](../../docs/GOAL_TRACKER.md#step-surface-comparison-probe--esp8266-model-15-2026-10-15).

## Multi-model degenerate-triangle diagnostic

`tests/multi_surface_reference_e2e.py` compares three hash-pinned ESP8266
embedded STEP assets from dataset commit
`5283bc059e30e51f4d3a81a674425936fc213214` against FreeCAD 1.1.4 meshes. The
checked-in manifest pins each source SHA-256, FreeCAD build, and reported solid
and face counts. The evaluator binary and references are captured in an ignored
`target/` receipt; no generated evidence is silently treated as a fixture.

The actual surface run found:

| Model | Result | Evidence |
| --- | --- | --- |
| `0.step` | Bounded sample comparison passed | Reader 460 triangles, FreeCAD 1,932; maximum sampled distance 0.000880 mm; production admission false |
| `14.step` | Rejected | Four degenerate triangles on source face `#51071`, an `ADVANCED_FACE` over planar surface `#6066`; FreeCAD's index-based reference mesh also fails the closed-edge check |
| `16.step` | Rejected | 1,102 degenerate triangles on source face `#2875`, an `ADVANCED_FACE` over bounded rational `B_SPLINE_SURFACE` `#10410` (degrees 2 × 9) |

The exact STEP records are retained in the hash-pinned inputs. The planar face
in `14.step` means these degenerate triangles cannot be attributed only to
spline surface evaluation; inspect its outer boundary loop (`#26` → `#55436`)
before changing planar tessellation. For `16.step`, inspect the rational
surface's parameter-domain samples and boundary correspondence before choosing
a repair. Removing the reported triangles is not a repair: the candidate stays
open/inconsistently oriented, and `16.step` also exceeds the sampled-distance
tolerance. Both inputs remain rejected and no production STEP path is enabled.

`tests/diagnose_degenerate_surface.py` is a non-production experiment that
removes only triangles meeting the same scale-relative zero-area test, then
checks closure, winding, signed volume and bounded surface samples. On `14.step`,
removing four triangles left the candidate open and inconsistently oriented;
sampled distance was below 0.02 mm and volume was within tolerance, but those
checks do not repair the topology. On `16.step`, removing 1,102 triangles left
the candidate open and inconsistently oriented, and the reader-to-reference
maximum sampled distance was 0.0840 mm, above the 0.02 mm limit. Both filtered
volumes were within the 0.01% comparison tolerance. Volume agreement alone is
not a geometry pass.

The exact local receipts were produced under
`target/step-multi-surface-e2e-20261015j/receipt.json` and
`target/step-multi-surface-e2e-20261015k/degenerate-removal-diagnostic.json`.
These ignored outputs are local evidence; the source and tool digests are in
the receipts. The filter experiment does not modify source geometry, emit a
GLB, or qualify a component association or PCB pose. **Do not drop these
triangles in the product exporter.** STEP admission stays fail-closed until the
reader's per-face tessellation and the multi-solid reference topology are
understood and independent comparisons pass.

To repeat the diagnostic, first materialize the pinned source files and create
the three FreeCAD references with `tests/multi_surface_reference_e2e.py`, then
run:

```powershell
python experiments/step-evaluation/tests/diagnose_degenerate_surface.py `
  --binary C:\path\to\tokito-step-evaluation.exe `
  --models-root target\dataset-intake\...\embedded-models `
  --references-root target\step-multi-surface-e2e-...\references `
  --output target\step-degenerate-removal-diagnostic.json
```

Every run requires a new output path. This is a research diagnostic, not a
passing release gate or a production STEP adapter.

## Per-solid reference update (2026-10-16)

The v2 reference capture preserves independent solid ownership. All 76 FreeCAD
reference solids passed strict closure and winding in the first real run,
including all 41 in `14.step`; its compound retains 6,625 incidence-four contact
edges. The reader still rejects `14.step` and `16.step` for degenerate triangles.
See [the per-solid contract and evidence](../../docs/STEP_SOLID_REFERENCE.md). Historical failed
receipts above remain unchanged. This does not enable production STEP or AR.
