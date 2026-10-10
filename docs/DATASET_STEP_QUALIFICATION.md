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

## Negative-major-radius torus experiment

The eight skipped faces in `57.step` reference tori with negative major radii.
An explicit development-only `--negative-torus-compat` option clones the parsed
AST, normalizes those radii and toggles the reader's affected face sense. Original
bytes remain unchanged; original warnings and separate AST fingerprints are
reported. This is a candidate heuristic, not an implementation of OCCT's full
orientation semantics: OCCT distinguishes edge-loop sense from final face
orientation. The default reader and production admission are unchanged.

On the pinned asset, the candidate recovers eight face IDs (228 versus the
default's 220) and reports zero derived-reader warnings. However, the independent
FreeCAD 1.1.4 surface check does **not** establish correct geometry. Across 250
triangle-centroid samples covering all eight affected face IDs, nearest-reference
surface distance has median **0.21893656595 mm** and maximum
**0.79343805063 mm**. The reference is valid and has 228 faces. Distance is measured
against a compound of reference faces, avoiding solid-containment zero distances.
The composed reader frame remains unqualified; source-to-reference face identity,
surface normals, trim coverage and worst-case deviation remain unverified.
These discrepancies block treating recovered face counts as a geometry fix.

Controls from eight unchanged faces of the same reader part produced 131 centroid
samples with maximum distance **0.00405380850 mm**. The 750 sampled affected
triangle vertices have maximum distance **0.68639072817 mm**, compared with
**0.00349781607 mm** for 393 control vertex observations (shared vertices may
repeat). This makes a general frame/unit mismatch less likely and shows the
affected discrepancy is not solely triangle-centroid chord approximation. It
still does not establish source-face correspondence or certify all transforms.

`examples/torus_surface_evidence.rs` is development tooling with at most 256
occurrence visits, depth 32, 32 samples per affected face/shape occurrence, 8192
total samples and 128 warning details of 512 characters. It rejects empty
sampling and reports sampled/missing selected face IDs. `--control-faces` selects
up to eight unchanged face IDs from parts containing affected faces. Its process exit 0 means
only that the research report was produced; `admitted` remains false. The main
probe always exits 2. Neither tool exports a production model.

Local reports are `target/dataset-intake/torus-surface-samples.json` and
`torus-reference-surface.json`. The affected experiment passed nine unit tests,
two executable rejection tests, formatting, warnings-denied all-target Clippy,
and the actual example/FreeCAD runs. These scoped checks do not replace the
earlier whole-engine verification receipt. Initial example compilation failed
on a nonexistent identity method, then on an incomplete local edit; both were
corrected before successful build and checks.

An original Rust analytic regression in `examples/torus_inverse_regression.rs`
isolates a spindle-torus inverse ambiguity. For the magnitude of source surface
`#3871` (minor radius greater than major radius), an outer-only inverse round
trip at the inner equator differs by **2.49189545339 mm**; a second branch
reconstructs the point. Seven analytic samples have selected-branch maximum
residual **1.0739e-15 mm**. Five regression tests passed, including axis-pole
ambiguity, finite input validation and negative-major reparameterization. This
is mathematical evidence, not a parsed STEP trim fix: source trim domains,
periodic continuity, pole behavior and face orientation still require an
original implementation or a suitable supported reader extension. No cached
dependency source was changed, and no production STEP adapter was enabled.
