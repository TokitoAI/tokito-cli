# Board twins and physical-photo qualification

Source audit: dataset main `a7fa0fed505e` and PR 23 head
`d427f1d99ec7a9d21794b5c349c2672b9c0baf2f`. This is an evidence checkpoint,
not completion of component geometry or automatic physical AR.

## Prepared model inventory

| Board | Prepared asset | Unfinished qualification |
| --- | --- | --- |
| ESP32 | Selectable community-layout reference candidate; 24 populated model records | Clone/physical variant correspondence and exact package completeness |
| FRDM-MCXN236 | Rev C source candidate; 354 selectable component references | 23 placeholder bodies, 37 geometryless references, fitted population and physical registration |
| Ultimainboard | 2.1.4 source candidate; 310 selectable component references | Missing package geometry and physical registration |
| ESP8266 | 34 native component identities / 62 body candidates, 17 evaluated source models | No completed assembled-board GLB; reference CP2102 versus photographed CH340-style clone |

These assets adapt authored source geometry; they are not invented reconstructions
of unknown photographed boards. Render resolution does not qualify geometry.

The Y3 identity correction is merged in dataset PR #27 (issue #26), with
dataset checks green. The broader photo/geometry/pose audit (#24) remains open.

## NXP comparison and correction

The dataset NXP GLB digest is
`ba9db807d2141189facb33039f57861d99a460cf595dd71bf80eb5f9cbe1268c`.
Its named component world bounds match the currently prepared source geometry
within one micrometre. Replacing the file does not resolve identity/pose errors.

Four reviewed mounting-hole pairs (BH1, BH4, BH2, BH3) establish a same-photo
planar correspondence. A separate check of 38 component mesh centers against
reviewed body polygons initially found 25 inside and a 15.52 px median center
offset. The supposed Y1 mask was 1,012.10 px from source Y1. Original-photo
inspection shows adjacent **Y3** silkscreen at that body in all three reviewed
views. Correcting the annotation/crosswalk, without moving CAD geometry, gives
26/38 centers inside and a 13.87 px median offset. These are diagnostic same-photo
numbers, not independent camera-pose or specimen accuracy. Small-part outliers
remain subject to annotation, perspective/height, and placement review.

The bottom photo shows `SCH-90828 REV C` and `700-90828 REV A1`. Schematic and
assembly revisions are different domains; neither alone certifies PCB artwork,
fitted BOM, or physical mesh completeness. Do not infer a B/C incompatibility
from the 2024 copyright text. The full source design contains DNP bodies; source
population must remain separate from photographed fitted population.

## Runtime handoff rules

YOLO board-family detection must select a compatible candidate registry entry;
generic package segmentation cannot establish a board/refdes identity. The new
dataset detector plus crop classifier supports five families, but was evaluated
on known specimens/sessions. Its closed classifier cannot certify an unknown
board or an exact clone revision. Never promote confidence to physical proof.

A camera-only diagnostic crop from the actual iQOO Ultimainboard view selected
Ultimainboard through the complete pipeline (box score 0.641; crop-class score
0.999995). The detector's initial class was NXP, demonstrating why that label
cannot independently choose the twin. Running on a whole UI screenshot instead
produced Raspberry Pi and a near-full-screen box. This is one diagnostic ROI,
not held-out raw-camera evaluation or a shipping crop policy. Production must
use raw frame pixels, preserve detector-to-crop transforms, and retain both
scores with the final classifier identity; none supplies a metric board pose.

Pose landmarks must agree between model metadata and twin metadata. The reviewed
NXP photo uses mounting-hole centers while the older twin uses outline corners.
Combining those changes scale and alignment. A selected model, a documentation
query, or a generic corner prediction does not grant qualified physical scope.

Large STEP face exports also require mobile draw-call optimization. The real
iQOO crashed on 63,171 Ultimainboard primitives. Pinned gltfpack 1.3.0 with named
nodes retained reduced the prepared FRDM/Ultimainboard scenes to 115/130
primitives, preserving all component references and metric bounds. Both render
on the iQOO; picking Ultimainboard U1 visibly highlights its full source mesh.
That verifies rendering/selection, not camera alignment. App admission now checks
scene budgets before entering the native renderer.

Track remaining work in CLI #26/#28, App #43 and dataset #24. Earlier issue notes
requiring the App to stay read-only are superseded by the user's explicit
authorization for cross-repository App/YOLO/AR integration. Qualification gates
are not superseded.

## Source-bound substrate metadata

`twin adapt-source` accepts optional `--board-plane-node <EXACT_SOURCE_NODE>`
after `--output`. The author chooses a reviewed substrate subtree in the
hash-bound source GLB. The CLI decodes its world-space POSITION geometry and
records `boardPlaneBoundsMetres` and `boardPlaneSourceNode`, retaining separate
whole-scene bounds. Missing, ambiguous, inactive, empty or degenerate subtrees
are rejected before an output directory is created. This does not infer a board
from its box, certify the selected node as physically accurate, or grant pose
qualification. A rotated substrate's axis-aligned bounds are not named hole
landmarks; those still require their explicit source/image correspondence.

The actual Ultimainboard source subprocess journey passed with 310 selectable
groups, eight explicit missing/unknown required bodies, byte-preserved GLB and
source transforms, and separately decoded 175 x 79 mm substrate bounds. The
new selector rejection and unchanged qualification gates are exercised in that
journey. Rust all-target tests (164 total), formatting and warnings-as-errors
Clippy pass on Windows; junction setup uses native path separators and parallel
fixture identities use an atomic sequence as well as time/process identity.
