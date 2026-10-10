# Dataset STEP qualification evidence

This is engineering research, not production STEP support. Dataset main is pinned
at `257ea7e68ee2e700522bd1b19ad9cbf1c0c3980a`; sibling repositories remain read-only.
Intake is isolated under the CLI's ignored `target/dataset-intake/257ea7e6`.

## Materialized sources

The selected three generated PCB layouts and all 61 Ultimainboard embedded STEP
assets were materialized through Git LFS. All 64 bodies match the size and SHA-256
in their pinned Git LFS pointers. Receipt: `materialization-receipt.json` in the
intake folder. This is selected-asset integrity, not whole-dataset verification.

All three PCB layouts pass inventory inspection. FRDM-MCXN236 and Ultimainboard
pass the current outline validator and produce explicit partial board-shell GLBs.
ESP32 still reports six open joins. PCB receipts are under
`pcb-qualification-corrected-20261010` and `board-shell-qualification-20261010`.
The initial harness put `--json` before the source argument and recorded usage
errors; those results are retained separately and are not product failures.
The successful corrected run uses the documented argument order.

## Real STEP samples and independent reference

FreeCAD 1.1.4, commit `4fd3bf320d9566a27e60069fc8387448aaa3a094`, imported the
same materialized inputs through its headless executable. It is an external
reference tool, not a runtime dependency. The Rust probe uses pinned
`oxideav-step` 0.0.1 without default features and always refuses production
admission. Both source and probe-binary hashes are recorded locally.

| Ultimainboard asset | Active length unit | Rust triangles | Distinct reader face IDs / FreeCAD faces | Maximum bounds difference, mm |
| --- | --- | ---: | ---: | ---: |
| `10.step` | millimetres | 428 | 83 / 83 | 0 |
| `0.step` | centimetres | 11,618 | 263 / 263 | 3.62e-15 |
| `18.step` | inches | 9,366 | 142 / 142 | 4.45e-16 |

FreeCAD reports one valid solid per sample. The probe reports no warnings and no
missing, zero, or excess triangle-provenance entries for these samples. Bounds
compare the reader's unit-scaled part-local AABB with the imported reference
shape AABB; these single-shape observations do not qualify assembly-world poses.
Equal face counts do not prove corresponding face identities, surface accuracy,
topology, or complete geometry. No production exports were admitted.

Receipts and raw reports are under `step-unit-qualification-20261010` and
`step-face-qualification-20261010`. The latter uses the probe's bounded per-shape
provenance summaries. Each summary retains part/shape indices and representation
item identity; aggregate counts cover all shapes even when detail is truncated.

## Unit-label correction

The prior sample manifest's "49 mm / 9 cm / 3 metres" is a recorded classification,
not independently verified active representation units. A whole-file SI-unit
search misclassifies conversion-based units. In `18.step`, the actual chain is:

`#4982` shape definition → `#106` advanced B-rep representation → `#1721`
representation context → `#4965` conversion-based `INCH` → `#4818` length measure
`0.0254` → `#4964` SI metre base.

Therefore the reader's `0.0254` metres per output unit is correct for that sample.
Unit preflight must resolve the selected representation context and conversion
graph, rather than selecting an arbitrary SI declaration or trusting a filename.
No dataset files or prior manifests were edited by this work.

## Remaining gates

The all-61 reader run completed without timeouts; each asset returned geometry.
FreeCAD imported all 39 distinct source hashes, reusing reference observations
for byte-identical duplicate assets. Reader-reported root units group as 48
millimetre, nine centimetre and four inch assets; these are reader observations,
not independently resolved unit contexts for every representation.

The results expose concrete qualification gaps:

- `55.step` warns that `GEOMETRIC_SET` was skipped.
- `57.step` warns that eight faces were skipped; its retained face-count sum is
  220 versus 228 reference faces. Production admission must reject this partial
  geometry.
- `46.step` is marked invalid by FreeCAD, despite reader mesh output.
- `58.step` has 35 parts / 17 meshed shapes. Its distinct face-count sum differs
  from the reference count (188 versus 255); occurrence reuse and representation
  selection must be reconciled before attributing the difference to missing
  geometry.
- Seven filename entries (five distinct source hashes) have single-part bounds
  diagnostics exceeding 0.01 mm, including repeated assets
  `47.step` / `48.step` at 3.539 mm. Coordinate-frame/occurrence alignment and
  included geometry have not been independently reconciled, so these are
  discrepancies to investigate, not a claimed surface-error measurement.

Face-count sums equal reference counts for 59 assets. This is count corroboration,
not face-identity, assembly-instance coverage or shape equivalence. Actual results
are in `step-all61-qualification-20261010/receipt.json`, `freecad-reference.json`
and `reference-comparison.json`. All production admission remains false.

Next
requirements are active-context units for every representation, source-face
identity/topology and surface-distance comparisons, material preservation,
assembly occurrence transforms, resource qualification and deterministic exports.
Component assignment and native-Altium-to-extracted-model equivalence remain
unverified. Keep production STEP admission disabled until these gates pass.
