# NXP FRDM-MCXN236 prototype refinement

## Source baseline and scope

Dataset main inspected at `b963a35e7828e7e06e619e62c7ddf613914165e4`.
No open dataset PRs were present at inspection. Its populated Rev C candidate
is the same baseline already delivered in `demo/nxp-reference/`, not a new
independently qualified board model. The earlier alleged Rev B marking was
retracted: “NXP B.V.” is copyright text. MCXN947 is a different reference board.

The baseline has 391 semantic reference records and 354 package selectors.
Thirty-seven references have no package bodies (mounting holes, test points and
solder jumpers). The native BOM additionally marks 63 references DNP; all 63
were drawn in the old model. Thus neither 391 nor 354 is a verified fitted
component count. The user's approximate 385 count remains a separate specimen
observation, not a replacement for the source inventory.

Publisher quality labels in the old model comprise 306 exact-package claims,
25 package approximations and 23 placeholders. These labels do not constitute
independent surface, population or placement qualification.

## Implemented refinement

`experiments/twin-refinement/` is an original Rust, offline, explicitly partial
source-mesh refinement tool. It validates the baseline with the public Tokito
engine, appends source STEP candidate meshes, retains other geometry bytes,
records every declared transform change and material override, and revalidates
the resulting asset. It has no KiCad/native CAD runtime dependency.

| Reference | Change | Source geometry |
|---|---|---|
| U8 | Replace VFBGA98 box; restore centred PCB pad origin | KiCad SOT1982-1 package, 134,832 emitted triangles |
| J10 / J11 | Replace USB connector boxes; restore A1 footprint origins | KiCad GCT USB4085, 16,510 emitted triangles each |
| J7 | Remove unfitted 2×6 socket body | Native BOM `bom-item-034`, row 48, DNP; user confirms empty footprint |
| J14 / J21 | Remove unfitted audio-jack bodies | Native BOM `bom-item-040`, row 54, DNP; user confirms empty footprints |

The corrected v3 prototype keeps 391 reference identities and **351 package
selectors**. The three unpopulated footprints remain semantic records and are
labelled “not fitted”; their missing package selectors are not model failures.
The current view does not yet add their source copper rings/drill geometry or
make those empty footprint regions spatially selectable. Do not substitute a
fake contact marker for that geometry. The other source DNP assembly options
remain explicitly unresolved for the held specimen; this step does not
automatically remove or claim physical population for all 63.

The explicit source files and spec are local under
`target/component-intake/nxp-refinement-20261011/`. Output is
`target/prototypes/nxp-source-refined-e2e-v3/`, with a standalone viewer under
`prototype/`. The experimental command requires `--allow-partial`; it does not
turn the resulting asset into an admitted physical twin.

## Placement evidence

Imported PCB SHA-256:
`c0d1389ce0710539104a3f2c4ca5f382c6b5fa2cce0edde315b4d73360f75b26`.
The old model introduced package-position offsets not justified by its native
pad origins. Corrections preserve the source parent rotation and candidate
top-plane datum while setting XY from the PCB footprint:

| Reference | Source footprint XY, mm | Old parent glTF XYZ, mm | Corrected parent glTF XYZ, mm |
|---|---|---|---|
| U8 | 22.208007, −24.881002 | 22.858307, 1.595, −25.580802 | 22.208007, 1.595, −24.881002 |
| J10 | 3.054883, −36.619002 | −0.970117, 1.595, −35.594002 | 3.054883, 1.595, −36.619002 |
| J11 | 3.054883, −21.506002 | −0.970117, 1.595, −20.481002 | 3.054883, 1.595, −21.506002 |

U8's 98 named pads match the centred companion footprint, with maximum local
coordinate difference about 0.000995 mm. The USB pattern matches its companion's
16 signal pads and four shell contacts within about 0.000012 mm. USB4085's origin
is A1, **not its body centre**. Source STEP XYZ maps to `[x,z,-y]` glTF metres
before the retained parent Y rotation. Old box half-height translations are
removed. The 1.595 mm top-plane candidate remains physically unqualified.

U1 also has an unjustified old XY offset; it needs a complete SOT2172-1 body
before accepting its placement. Do not treat the still-rendered U1 box as a
complete BGA model. Official package information specifies 9×9 mm, 0.5 mm pitch
and 184 terminals; electrical pad diameters alone do not establish solder-ball
diameters or the moulded-body details.

## Source hashes and presentation

| Input | SHA-256 / pin |
|---|---|
| U8 STEP | `333b8a84d84cee9d74c5eb6a9ebb00289627a96cd5efa05c45d930cf63dd7dd1`; KiCad packages3D `8d4070daa0fa3f7a7f3c176f4ca7bffc4df392be` |
| USB STEP | `82235f7275d07f720e3c050f781397f4bef47fdc7e15dd507d68ccba861f1a35`; KiCad packages3D `8975715167d1a76f9b44be155ac02326f704cdbb` |
| Native components/BOM join | `a96f612e11974dce7de88490ba25901c5547d845e2c26ccd2a52cd3d23a7436b`; dataset main pin above |

The refiner omits only explicitly accepted, exactly zero cross-product triangles
in the original source millimetre frame: 196 for U8 and 20 per USB model. It
records every source ordinal and refuses tiny nonzero triangles. It does not
weld vertices or silently remove unsupported surfaces. Active model hashes are
separate from retained historical placeholder metadata.

Eleven material overrides are explicit prototype presentation approximations,
not recovered measured colours. The viewer uses authored colour/roughness/
metallic factors with approximate GGX studio lighting and display gamma; picking
uses unlit IDs. It has no measured environment, texture, physical BRDF or shadow
qualification. A 3840×2160 screenshot confirms available desktop output
resolution, not CAD accuracy or phone performance.

## Verification and remaining gates

`tests/nxp_refinement_e2e.py` compares every emitted replacement coordinate,
normal and colour with the source reports, checks f32 triangles remain nonzero
with correct winding, verifies PCB-origin corrections, retains unrelated node
data and existing geometry bytes, and checks the three BOM-bound suppressions.
It also exercises explicit consent, overwrite, hash and population refusals.
FreeCAD 1.1 was used only for off-device QA: both STEP files imported as valid
B-reps and their independently evaluated bounds matched the Rust mesh bounds.
Bounds agreement alone is not a surface or pose certification.

The accepted v3 viewer passed all 351 available package selections, not-fitted labels for J7/J14/J21, orbit/zoom/reset and offline interaction after page load. Its actual 3840x2160 screenshot and browser receipt are frozen in `demo/nxp-refined/`. All 41 fresh local verification gates passed before delivery; no GitHub Actions run is part of this workflow.

Next gates: verify/render empty pads and drill openings; replace U1 and the
remaining switch/IC placeholders; check each remaining source placement;
reconcile DNP options with reviewed specimen photos; confirm underside geometry;
then validate App import, component identity crosswalk and real-device
registration. Keep geometry, specimen fit, positional guidance and release
admission false until their own evidence exists.

Primary references: [NXP SOT1982-1](https://www.nxp.com/packages/SOT1982-1),
[NXP SOT2172-1](https://www.nxp.com/packages/SOT2172-1),
[GCT USB4085](https://gct.co/connector/usb4085), and
[Würth switch datasheet](https://www.we-online.com/components/products/datasheet/434123025826.pdf).

The complete receipt is [recorded here](evidence/nxp-refined-local-verification-20261011.json). Documentation completion notes were added after the run; implementation and test inputs remained unchanged. The separate fresh-binary drilled-substrate receipt is [here](evidence/esp8266-drilled-substrate-20261011.json).
