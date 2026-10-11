# ESP8266 parallel implementation checkpoint

Inspected `TokitoAI/tokito-app-dataset` main and all 12 open pull requests on
2026-10-11 through authenticated read-only GitHub API calls. Main is
`f81f6f85084c01b45658d1d8528ef06ac8b162a9`.
The original NodeMCU source subtree is still Git tree
`d7171a15cc06d9cbd0d7d2895724f719d32f420a`, byte-identical to its subtree in
`5283bc059e30e51f4d3a81a674425936fc213214`. The already materialized source is
therefore reused; its PCB SHA-256 is
`007d603df0effe46bc254c978def4d3225a9c5123803cd115ad99c087a26ea74`.

PR [12](https://github.com/TokitoAI/tokito-app-dataset/pull/12) is pinned at
`493dd2d6da950bdb2a66896502be8ad01abcff80`. Its ESP12E video index has 56
instances with null source-component IDs and GLB selectors; its own limitations
include incomplete board boundary, no verified metric landmarks, and no
source-matched selectable carrier meshes. PR
[18](https://github.com/TokitoAI/tokito-app-dataset/pull/18), pinned at
`b10052d912e8c6be5ea5b42fdb5031f262aece85`, adds photo/video review metadata;
the inspected component-mask review retains null source-component IDs.
These updates supply observations, not native body association or board transforms.
No ESP8266 assembly GLB or imported PCB was found in main's ESP8266 paths or
the open pull requests' changed CAD paths. Unrelated boards' GLBs remain separate.

The original Rust Altium experiment now emits
`candidateAssociations` (`tokito.altium-candidate-associations.v1`) directly
from the bounded decoded source streams. This is an executable source inventory
for later component selection, not an admitted assembly:

- 34 source component identity groups retain native Altium UNIQUEID, reference,
  and every raw component property. No KiCad UUID is invented.
- 62 body candidates retain source byte offsets, model IDs and all raw body
  properties. Each J1/J2 group contains all 15 observed body candidates.
- Explicit model IDs resolve to every matching source model record, preserving
  the duplicate groups `[2,3]`, `[5,6,7]`, and `[12,13]`, their differing
  placement properties, and each embedded STEP hash. No record is silently chosen.
- The report distinguishes matching embedded byte identities from matching
  placement properties. Missing models/bytes and out-of-range component indices
  remain explicit unresolved candidates. A missing decoded record stream reports
  an association error.
- Selected model ordinals and world transforms remain null. Component index
  semantics, geometry completeness, pose and assembly admission remain false.
  The experiment still exits 2 and emits no geometry exports.

| Owner | Dependencies | State | Evidence | Review/QA | Next action |
|---|---|---|---|---|---|
| Scoped ESP8266 worker / main | Pinned source/metadata | Candidate association and normalized source pose inputs implemented | Source-bound Rust report + full-row E2E receipt | Initial independent review completed; 14 offline Rust tests and actual E2E passed; follow-up review pending | Independently establish world pose and geometry |

Checks actually executed: offline Rust unit tests (14 passed), offline Clippy with
warnings denied, offline build, and `tests/esp8266_assembly_candidates_e2e.py`.
The subprocess comparison checks every component/body/model candidate against
raw stream records, independently published component extraction and materialized
STEP hashes, including ambiguity and all 62 body ordinals. It passes while
requiring all admission flags to remain false. The unit tests also exercise
unresolved/out-of-range associations and unavailable record streams.

CLI-owned ignored evidence is under
`target/dataset-intake/esp8266-latest-f81f6f85/`: `remote-snapshot.json` records
all inspected PR heads/changed ESP and CAD paths, source-subtree equality and
the exact hashes of six materialized metadata/doc assets. `assembly-e2e-v5/`
contains the actual candidate-association artifact and receipt. No new clone,
large LFS corpus, native CAD engine or package installation was used.

Independent review checked the saved source rows and all candidate STEP hashes.
Follow-up fixes bind the standalone artifact to `sourceSha256` and bind its exact
bytes in the receipt with `candidateAssociationsSha256`. The E2E now also checks
every copied body offset, component index, remaining payload count and component
ordinal. Missing candidate digests cannot establish matching embedded bytes; a
regression covers this case. Main reran the build, all 11 experiment tests, Clippy
and the actual dataset subprocess journey. The local verifier includes this
association journey after its source-extraction gate.

Follow-up original Rust normalization now records component and body source XY,
explicit mil-to-mm elevation, separate side/projection, and all source angles.
It rejects missing/unsupported/nonfinite or oversized inputs and never applies the
Models-table pose. All 62 actual body rows pass independent Python Decimal unit,
angle and side comparisons. These are usable assembly inputs, not world transforms:
no handedness conversion, stackup origin or physical placement is assumed. The v5
receipt binds the actual source and candidate report. See
[placement research](ESP8266_PLACEMENT_RESEARCH.md) for the pinned behavior evidence.

The body/model oracle in that test comes from the same decoded Rust report;
independent published component rows and STEP hashes add separate evidence, but
do not establish native binary framing or physical pose. Keep those limits explicit.

Remaining gates: independent interpretation of the body component-index field;
body/model/component rotation and elevation composition; underside behavior;
complete qualified STEP surfaces/trims; substrate/pad geometry; exact photographed
carrier revision crosswalk. The current evidence does not justify an assembled
component-selectable ESP8266 GLB, physical guidance, or AR alignment.

## Board-source follow-up

The current source pipeline additionally preserves `/Board6/Data` as 27 separate
CR-delimited sections, with unresolved legacy values as raw bytes and conflicting
duplicates explicit. Source E2E v8 checks its independently observed stream hash,
active two-copper-layer chain, modern physical stack declarations and all nine
contour records. An independent reviewer extracted the stream directly from the
CFB and matched every section, offset, length and property against the actual
Rust output. No parsing blocker was found.

Current checks: 15 experiment tests, formatting, Clippy, actual source E2E v8 and
62-body candidate E2E v8 passed. Source copper/core thickness is 0.39116 mm;
mask-inclusive declaration is 0.41148 mm. Neither proves physical thickness.
See [placement research](ESP8266_PLACEMENT_RESEARCH.md). The full delivery gate,
contour tessellation, complete STEP surfaces and world placement remain pending.

## Normalized board inputs — 2026-10-11

The active D: checkout now emits `boardGeometryInputs` from the original Rust
`board_inputs` module. It identifies contributing fields across separate metadata
sections, requires agreement between the linked active copper/core declarations
and the modern physical stack, and retains source section provenance. It converts
origin and contour coordinates to explicit mm and angles to degrees. Unknown,
missing, conflicting, nonfinite and out-of-budget values are refused.

Independent review found two profile-validation gaps: orphan stack rows without
names and numeric layer-index aliases. Both were fixed with regressions. Every
V9 row now requires a recognized ID/name pair and a canonical unique index.
Follow-up source review found no remaining blocker in that slice.

Main ran the integrated current-source formatting, 20 experiment tests, Clippy,
build, actual source E2E and 62-body association E2E on D:. All passed. Source E2E
independently Decimal-compares thickness, origin, every contour coordinate/radius/
angle and contributing section ordinal. Evidence is under
`D:\TokitoCliBuilds\active-esp8266-board-20261011/`.

This is assembly input preparation. Contour topology, complete STEP geometry,
native world placement, physical thickness and AR qualification remain false;
Z origin is null. No assembled ESP8266 GLB is claimed. The earlier 33-gate full
receipt predates this module; a new full delivery gate is required before pushing.

## Positioned candidate mesh checkpoint (2026-10-11)

The original Rust STEP experiment now accepts --candidate-mesh-data and
retains positioned vertices, triangles, face identities, source colours and
occurrence paths for all 17 published ESP8266 model files. The actual subprocess
journey preserved 123,336 vertices and 246,406 triangles. This is JSON candidate
mesh data, not an assembled board GLB or production STEP admission.

Independent FreeCAD bounds comparisons are recorded for every file. Model 11
has a 4.86826 mm maximum bounds difference; models 12/13 differ by 0.0492565 mm
and model 16 by 0.049816 mm. These differences and all reader warnings remain
explicit. Only the previously defined models 0/14/16 have a 0.05 mm regression
gate; the other comparisons are diagnostic. Geometry, surface completeness and
PCB placement stay unqualified. The test refuses nonfinite external bounds.

The local verifier now includes this candidate-data journey with the ESP8266
profile. The complete ESP32/ESP8266 profile has 34 gates. A fresh full receipt
is required for this push; earlier 33-gate receipts predate these additions.
