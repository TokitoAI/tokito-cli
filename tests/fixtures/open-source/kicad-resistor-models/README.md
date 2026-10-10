# Official resistor mesh fixture

Asset: `Resistor_SMD.3dshapes/R_0603_1608Metric.wrl`, 14,831 bytes,
SHA-256 `eadfec9e71c67b1dd3910c72b45d811a06f0ab5397d0609354b10e9a3a4c1fdd`.
Git blob `0480a16cdd2d0ffd425b97d32a2ced97698c11bf`.

The official library release [8.0.0](https://gitlab.com/kicad/libraries/kicad-packages3D/-/tree/e8cbcb1e90d4962ac754faedae43632e3cd3abc9)
contains this exact blob. [Pinned model](https://gitlab.com/kicad/libraries/kicad-packages3D/-/blob/e8cbcb1e90d4962ac754faedae43632e3cd3abc9/Resistor_SMD.3dshapes/R_0603_1608Metric.wrl).
Local bytes were obtained unchanged through the official archived GitHub mirror at
commit `b8b3cfdfad88ba66f21002b3de51dc6f7d55ba5a`, with the same blob and SHA-256.
This records acquisition separately from generation compatibility; it is not a current floating library lookup.

The model header retains 2018 KiCad StepUp copyright and CC BY-SA 4.0 with the
electronic-design exception. The unchanged mirror `LICENSE.md` is retained
(SHA-256 `285b027a721577d0bc0198f36e948c6c282ddaa4b58fef8ed826749576eb0aef`).
No application source was copied.

Six Arduino UNO community PCB fixture model references name this path using
`${KICAD8_3DMODEL_DIR}`. Tests supply that variable explicitly. Resolving these
files does not qualify the community board against an actual Arduino specimen.

The flat VRML97 profile contains 29 root Shapes: three material-definition
Shapes and 26 geometry Shapes, with 328 vertices and 276 triangle faces.
Raw coordinate bounds are X ±0.315, Y ±0.157, Z 0…0.177. With the explicit
ordinary KiCad legacy WRL unit profile (0.1 inch = 2.54 mm per coordinate unit),
bounds are X ±0.8001 mm, Y ±0.39878 mm, Z 0…0.44958 mm.
There are no Transform/Inline nodes in this fixture. The adapter rejects those
nodes rather than applying an unverified unit heuristic or reading other files.

This fixture verifies local asset capture and the declared mesh profile. It
does not provide independent component world transforms, an asymmetric pose
reference, mount planes, PCB-to-photo registration, selectable GLB or AR evidence.
