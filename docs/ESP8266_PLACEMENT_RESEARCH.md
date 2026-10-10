# ESP8266 source placement evidence

This document records behavior research for the original Rust source adapter.
It is not proof of an assembled physical twin. Dataset main is pinned at
`f81f6f85084c01b45658d1d8528ef06ac8b162a9`; native source PCB SHA-256 is
`007d603df0effe46bc254c978def4d3225a9c5123803cd115ad99c087a26ea74`.

## Independent format evidence

The inspected official KiCad source commit is
`bd256c07fc6263d1268ec53d0d3f66932123a1e1`.
Its [body record reader](https://github.com/KiCad/kicad-source-mirror/blob/bd256c07fc6263d1268ec53d0d3f66932123a1e1/pcbnew/pcb_io/altium/altium_parser_pcb.cpp#L1080)
reads a little-endian component index at payload bytes 7–9 and properties after
the subsequent nine bytes. Its
[body importer](https://github.com/KiCad/kicad-source-mirror/blob/bd256c07fc6263d1268ec53d0d3f66932123a1e1/pcbnew/pcb_io/altium/altium_pcb.cpp#L2173)
uses that index in the parsed component vector. This is independent implementation
evidence supporting the observed framing; native Altium equivalence still needs a
separate reference assembly comparison.

## Placement inputs

Body `MODEL.2D.X/Y` give absolute board locations; explicit `mil` values convert
to mm by multiplying by 0.0254. Body `MODEL.3D.DZ` is the explicit model elevation.
`MODEL.2D.ROTATION` and `MODEL.3D.ROTX/Y/Z` are angles in degrees. Component X/Y,
rotation and side plus body `BODYPROJECTION` contribute to side compensation.

The inspected importer uses body-specific pose, rather than adding Models-table
ROTX/Y/Z/DZ. Combining both would introduce an unsupported second transform.
For example, U1 body DZ is `19.685mil` (0.499999 mm), while its Models-table DZ
is an unqualified raw `196850`. J1/J2 have fifteen independently located body
records each, sharing model geometry. Preserve these body positions separately.

In the inspected importer's matching top-side case, behavior analysis simplifies
to `T(body_x, body_y, top_surface + dz) Rz(rz - body_angle) Ry(ry) Rx(rx)`.
Component angle cancels after conversion into footprint-local coordinates; adding
it again is unsupported. Matching underside composition includes a physical flip
and requires the bottom surface origin. This equation is a candidate behavior
reference, not an implemented or validated native world transform.

The importer's rotation order is explicitly ZYX in its
[model matrix function](https://github.com/KiCad/kicad-source-mirror/blob/bd256c07fc6263d1268ec53d0d3f66932123a1e1/3d-viewer/3d_math.cpp#L34).
Its [footprint matrix](https://github.com/KiCad/kicad-source-mirror/blob/bd256c07fc6263d1268ec53d0d3f66932123a1e1/3d-viewer/3d_canvas/board_adapter.cpp#L1112)
adds board-side placement. Verify these conventions independently before assigning
real component transforms.

## Duplicate models and completion gates

The importer retains the first duplicate model ID in its
[model table](https://github.com/KiCad/kicad-source-mirror/blob/bd256c07fc6263d1268ec53d0d3f66932123a1e1/pcbnew/pcb_io/altium/altium_pcb.cpp#L2749).
Tokito preserves every candidate instead. All duplicate STEP bytes in this source
agree by hash, so geometry deduplication can use content identity after geometry
qualification. Matching bytes does not verify table placement metadata.

Before an assembled export: establish board thickness and Z origins; independently
compare body rotations/elevations including underside; compose STEP internal
placements exactly once; qualify complete trimmed surfaces and board/pad geometry;
compare the photographed carrier revision. The existing FreeCAD reference checks
local STEP geometry, not assembled placement. Photo masks/YOLO boxes do not supply
these missing engineering transforms.

## Actual board-stream intake

The original Rust reader now selects `/Board6/Data`, SHA-256
`7d1f2d9e12ccbf710401c6b17de6ecf0c85981ecb0660189802db3112c7c4852`.
One length-framed 83,170-byte payload sits in an 83,174-byte stream. It contains
27 CR-separated metadata sections and legacy non-UTF8 filename bytes. Each section
is retained separately; non-ASCII values retain raw byte hex with unresolved
encoding. Conflicting duplicate values remain explicit. The first section's
properties are not a merge of every layer/view section.

Source E2E checks the linked copper layers 1→32→0 and modern physical stack
declarations: top/bottom copper 1.4 mil each, core 12.6 mil, masks 0.4 mil each.
Copper/core total is **0.39116 mm**; mask-inclusive source declaration is
**0.41148 mm**. Neither is a measurement of the held board. Do not replace these
declarations with a default 1.6 mm or sum inactive layer-cache values.

The nine source contour records include a closing repeat and four 90 mil
(2.286 mm) rounded corners. Source extents are 25.4 × 48.26 mm. XY origin is
(3079.9606,3523.3268) mil. Subtracting it agrees with the outer-coordinate range
of pinned V4 Gerber GKO SHA-256
`576e9403fc831f9227bf8f1ffe52c3c1fde0cf2c978d1c04645a7709969fe57a`.
That artwork also contains circular/overlapping paths; it is not safe to treat
every GKO stroke as one simple board perimeter.

Actual source evidence: `target/dataset-intake/esp8266-latest-f81f6f85/source-e2e-v8/`.
The source test checks every contour vertex and linked/modern thickness values;
it does not establish contour topology, native Z origin, fabrication thickness,
STEP surfaces, model world transforms or AR registration. No native Z-origin
contract was found. Renderer conventions remain candidate behavior evidence.
