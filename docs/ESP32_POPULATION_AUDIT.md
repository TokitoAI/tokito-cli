# ESP32 population correction

The first ESP32 selectable demo is **not a fully populated model**. Its 19 records
include four mounting-hole entries and only 15 rendered component bodies. Browser
selection and 4K rendering passed; those checks did not prove component completeness.

Dataset PR16 is pinned at `07c4519c964520e1a143b5f3c059b18dd108f037`.
The reviewed `ar-source-candidate/video-frame-7s.jpg` visibly contains resistors
and additional passives omitted from the model. Partial photo annotations explicitly
do not assert every visible part or source reference identity.

## Actual reference BOM comparison

The dataset's community-clone BOM contains 24 electrical component references:

| Class | Community reference BOM | Rendered provisional candidate |
|---|---:|---:|
| Resistors | 6 | 0 |
| Capacitors | 6 | 2 |
| LEDs | 1 | 2 |
| Transistors | 2 | 2 |
| Switches | 2 | 2 |
| USB receptacle | 1 | 1 |
| Header groups | 2 | 2 |
| RF module | 1 | 1 |
| USB/UART IC | 1 | 1 |
| Regulator | 1 | 1 |
| Diode | 1 | 1 |

These are comparative source counts, not an exact specimen inventory. The clone
has U1 as its RF module and U2 as regulator; the provisional photo candidate uses
U1 for regulator and U2 for interface IC. Matching labels cannot establish a join.
The clone's published dimensions also differ from the candidate's nominal ones.

Primary references inspected:
- [Community clone design and BOM](https://github.com/BrijModi/ESP32-Dev-Kit-Clone),
  dataset-pinned source tree `ab7d6491243f16dd0895d63786f04da362e704fb`.
- [DOIT pinout source](https://github.com/playelek/pinout-doit-32devkitv1), which does
  not provide a complete matching physical-specimen BOM.
- [Espressif DevKitC reference](https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32/esp32-devkitc/user_guide.html),
  a different board family reference; it is not substituted for the 30-pin specimen.

## Rust coverage command

```text
tokito-cli twin audit-population <GLB> --component-map <JSON> --manifest <JSON> --reference-bom <esp32-csv>
```

The command validates the twin and bounded BOM before comparing authored classes.
It reports count deficits, absent reference labels and same-label class conflicts.
Classification from BOM fields is explicitly a heuristic. Complete population,
same physical board, identity joins and physical fit remain unverified even when
counts match. It neither adds components nor assigns their positions.

Actual local evidence: `target/prototypes/esp32-candidate-e2e-v2/reference-population-audit.json`.
78 library tests, Clippy and the actual ESP32 CLI export/audit journey passed.
The viewer now warns that zero missing *mapped* models does not prove full population.

Next: review complete visible-component coverage across the specimen photos; join
each observation to compatible source/package evidence; establish dimensions and
placement; then generate or import the missing geometry and rerun complete coverage,
transform, pixel-highlight and browser checks. An attempt to assemble the dataset
clone through the native WRL exporter currently refuses its outline at Tokito's
strict 0.000001 mm tolerance; no populated clone export was produced.

### Chaining tolerance comparison

Measured three unique endpoint gaps: 0.001655551, 0.007042000 and 0.009959051 mm.
This is a tolerance mismatch, not proof that KiCad rejects the layout. KiCad's
[published export dialog](https://docs.kicad.org/doxygen/dialog__export__step__base_8cpp_source.html)
offers 0.001 mm tight, 0.01 mm standard and 0.1 mm loose chaining profiles.
Those published values are behavior references; this does not establish a pinned
KiCad 10.0.7 executable comparison or full export parity.

The original Rust CLI now provides:

```text
tokito-cli validate outline <BOARD> --compare-chaining 0.01 --json
```

Actual ESP32 source results: tight has six unmatched endpoints; standard and loose
have eight unique pairs and zero unmatched or ambiguous endpoints. The existing
strict outline remains invalid. This comparison changes no coordinates and does
not validate intersections, contour area, cutouts, triangulation or assembled GLB.
Local records: `target/dataset-intake/esp32-population-audit-07c4519/chaining-*.json`.
Eighteen topology tests and Clippy passed at this comparison stage.

### Explicit native export policy

The Rust API now accepts `with_outline_chaining_tolerance(0.01)`, and GLB export
accepts `--outline-chaining-tolerance 0.01`. Strict 0.000001 mm remains the default;
0.001 and 0.01 mm are explicit supported export profiles. Loose 0.1 mm remains
comparison-only because it exceeds the declared tessellation budget.

The contour gate reruns unique pairing, intersections, area and containment. It
includes the short endpoint connections in the analytical area. Mesh midpoint
joins displace endpoints by at most half the selected tolerance; curved sampling
reserves this amount from its 0.01 mm chord/join budget. Source primitives and hash
are retained. Both shell and assembly GLBs record the chosen policy and displacement
bound. Crossings and unsupported diagnostics remain refusals.

82 library tests, Clippy and the actual dataset E2E passed. Receipt:
`target/github-delivery/esp32-chaining-native-e2e-v1/receipt.json`.
The opted-in native export currently produces **only the shell**, with all 24 source
model references withheld. The assembly gate finds 1.58 mm between outer copper
surfaces versus the 1.60 mm nominal shell. The physical stack also declares 0.01 mm
front and back solder mask. This distinction needs an independent placement-policy
comparison before modifying the assembly gate. Installed KiCad 10 library assets
also use STEP for the checked resistor model; they are not a supported WRL replacement.

Full component population, independent geometry acceptance, the photographed-board
identity joins and product delivery remain open. This local implementation has not
passed the full delivery runner or independent final review yet.

### Latest recheck and visible audit (2026-10-10)

Read-only live inspection found dataset main still at `257ea7e6`, ESP32 PR14
at `ddb70d0a`, and latest PR17 at `34fe631ae20f634b71f120d8851b34b39f507409`.
No later ESP32 population/model mapping was found. The community assembly is
one flattened mesh without a component crosswalk; it cannot be relabelled as the
photographed-board selectable assembly. The source author also publishes a
47.02 × 29.16 mm clone, unlike the candidate's nominal dimensions.

```text
tokito-cli twin prototype <GLB> --component-map <JSON> --manifest <JSON> --output <NEW_DIR> --allow-partial --reference-bom <esp32-csv>
```

This optional comparison is minted from the prepared validated index. Its source
hashes bind the prepared inputs, it is bundled as `reference-population-audit.json`,
and the viewer displays class counts and U1/U2 conflicts using text nodes. It
does not generate new geometry or promote any physical qualification flag.

Actual v3 prototype: `target/prototypes/esp32-candidate-e2e-v3/prototype/index.html`.
All current Rust tests and Clippy passed; actual subprocess E2E passed for the
prototype/audit and native explicit-chaining shell. Real Edge browser acceptance
required the population panel and checked resistor 6/0, capacitor 6/2, reference
conflicts, selection/highlight pixels, reset, GPU picking, orbit, zoom, offline
reload and a 3840 × 2160 screenshot. Existing geometry remains unchanged.

Independent review found and corrected a latent arc/circle area-bound defect:
the contribution now measures sector area minus chord wedges, rather than the
wedge area itself. Independent analytic regressions cover both sweep directions,
circles and shrinking error with resolution. Eighty-four library tests passed.
The corrected real-source shell receipt is
`target/github-delivery/esp32-chaining-native-e2e-v2/receipt.json`.

| Work | Owner | State/evidence | Next action |
|---|---|---|---|
| Latest dataset/BOM audit | Main + read-only reviewer | Complete for the pins above; missing coverage and identity conflicts confirmed | Recheck when new source assets land |
| Visible coverage comparison | Main | Local CLI + browser E2E passed; no geometry added | Preserve reference-only status |
| Geometry area-bound correction | Main + reviewer | Corrected; analytic tests and actual shell E2E passed | Include in full delivery verification |
| Exact photo-to-layout component joins | Main | Unverified; clone identities/dimensions differ | Establish complete reviewed observations and matching source/package evidence |
| Missing component bodies/terminals and assembly | Main | Incomplete; native assembly currently emits no bodies for this clone | Verify stackup placement policy, then resolve/qualify models and transforms |
| GitHub code delivery | Main | Held; full local delivery runner not rerun | Finish current-source delivery gates before PR push |

### Native community-reference assembly (next local slice)

The earlier stackup mismatch was a semantic bug in Tokito's gate. Nominal
thickness includes the declared masks, while mounting uses outer copper surfaces.
KiCad 10.0.7 tag dereferences to `69cafb907ebc09e7722a2a464b1d466fc2e3608d`.
Behavior references: [stackup thickness calculation](https://gitlab.com/kicad/code/kicad/-/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/pcbnew/board_stackup_manager/board_stackup.cpp#L518)
and [STEP body/copper placement](https://gitlab.com/kicad/code/kicad/-/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/pcbnew/exporters/step/step_pcb_model.cpp#L1564).
These were inspected for behavior only; no source was copied or translated.

The new original Rust `StackupPhysicalExtentsMm` distinguishes 1.51 mm body,
1.58 mm copper span and 1.60 mm mask-inclusive total. Both assembly paths verify
nominal thickness against the declared total. Missing/invalid thickness, wrong
mask side/order, duplicate/unordered copper IDs and unknown physical materials
refuse. A source-resistor regression independently checks its minimum mounted Y
at 0.840 mm: 1.51/2 + 0.035 copper + 0.05 clearance. GLB shell geometry still
uses the body-centred **nominal-thickness envelope**, explicitly labelled; it is
not a layered dielectric/copper/mask reconstruction. KiCad visual parity remains open.

Ten exact source-referenced WRL assets were provisioned from the official
[KiCad Packages3D 9.0.0 library](https://gitlab.com/kicad/libraries/kicad-packages3D/-/tree/d5db5cea003fe2b4b1c9b145f5fcbd5fdb48f8ca),
commit `d5db5cea003fe2b4b1c9b145f5fcbd5fdb48f8ca`. URLs, sizes and SHA-256 are
recorded in `target/dataset-intake/kicad9-esp32-library-d5db5cea/intake.json`.
This is development asset intake; the CLI does not download models at runtime.

The native Rust export now emits **18 bodies**, including all six resistors and
six capacitors, with source PCB UUIDs and separate `ref:` selectors. It does not
reuse Anandhu's provisional GLB. Actual source layout poses drive this separate
community-reference assembly; they are not asserted to match the photographed board.
Six source references remain withheld:

| Parts | Refusal | Next step |
|---|---|---|
| J1, SW1, SW2, U1 | WRL material values outside the supported [0,1] profile | Investigate source material semantics; do not clamp silently |
| D2 | A face collapses/inverts under GLB f32 conversion | Investigate source tessellation/local-frame preservation; do not drop faces silently |
| U3 | Absolute external STEP path outside approved roots | Resolve an explicit source-bound asset and qualify STEP support |

GLB preflight now withholds a complete failed model with `glb_precision_rejected`
before assembly retains it; other accepted bodies can participate in an explicitly
partial export. Both native and assigned assembly use the encoder's actual checks.
No failed faces or components are replaced with generic shells. Current native
material serialization at that initial slice used a generic component material;
the material correction below supersedes that behavior.

Evidence: `target/prototypes/esp32-clone-native-e2e-v1/receipt.json` and
`prototype/index.html`. Actual CLI E2E passed with 18 source UUID checks/selectors,
all R1–R6/C1–C6 present, explicit six refusals and unchanged PCB hash. Real Edge
browser tests passed the 6/6 resistor/capacitor comparison, all 18 selectors,
regulator highlight pixels, missing CP2102 message, offline resistor selection
and a 3840×2160 screenshot. Eighty-five library tests and Clippy passed.
Independent review found no remaining material blocker in this slice. The full
delivery runner remains required before pushing this changed source.

### Source material correction

The yellow/gold v1 appearance came from one hard-coded export material, not the
actual resistor/capacitor models. Both native and assigned assembly now retain
each mesh part's source diffuse RGB, opacity, emissive colour and solid flag.
Full Phong values remain in material provenance extras; neutral metallic/roughness
factors are explicitly a projection, not Phong/PBR equivalence. Deterministic
deduplication is bounded at 4096 materials. Unauthored materials use an explicitly
marked neutral VRML fallback. The viewer displays bounded emissive colour and
suppresses it during picking/highlighting; reset restores source presentation.
It does not yet implement per-material backface culling or full PBR.

Actual output: `target/prototypes/esp32-clone-native-e2e-v2/prototype/index.html`.
Nine serialized materials replace the single generic gold material. Independent
source-text comparison confirms the resistor's three diffuse colours. The old/new
BIN payload, nodes, UUIDs, accessors and buffer views are identical. Eighty-six
library tests, Clippy, actual native dataset E2E, and real Edge highlight/reset,
18-selector, offline and 4K screenshot checks passed. Six model refusals remain.
This improves presentation, not population or photographed-board equivalence.

The previous full 29-gate receipt
`target/delivery/local/20261010T154847Z-982078229fd4454b9bffd1d22611f11f/verification.json`
passed for the pre-material source. It cannot qualify the new material changes;
fresh full verification remains a code-delivery requirement.

### Central module and selection presentation

The v8 native reference prototype adds U1 from the pinned official WRL after a
documented derived asset removes only its unused invalid `GLASS-19` material.
The original source asset is preserved. The central-module receipt records both
hashes, 751 source mesh parts, 7,652 triangles, the footprint UUID and 90-degree
source rotation. Its dimensions agree with the independent STEP reference within
0.08 mm. There are now 19 selectable bodies; USB, two switches, one LED and CP2102
remain missing. This is the community source layout, not a certified specimen twin.

Output: `target/prototypes/esp32-central-module-e2e-v8/derived-module/prototype/index.html`.
Selection uses a separate projected bounding-box overlay. Actual browser checks
confirmed original WebGL colour pixels remain byte-identical while U1/U2 are
selected, reset clears the overlay, missing U3 refuses, offline selection works and
a 3840 x 2160 image renders. The outline may appear through occluders; it is not
an exact silhouette or AR overlay. Photo-realistic board finish, patterned copper,
silkscreen and source-material lighting remain open. Earlier v3 had a darker board
finish but provisional, less complete geometry; visual preference does not qualify
its component identities or physical dimensions.

### Switch-body recovery

The same explicit derivation successfully recovers both SW1/SW2 from the exact
source-referenced official `SW_SPST_B3U-1000P.wrl`. Only the unused `GLASS-19`
palette shape is removed; retained source bytes and used material definitions are
unchanged. Each switch retains 242 mesh parts and 4,538 source triangles. The
ordinary Rust board exporter supplies the saved source placement and UUID; no
photographic pose is guessed. `tests/esp32_switch_e2e.py` independently reconstructs
the byte deletion, checks hashes, exported triangle totals, both UUIDs and CLI
selectors, and preserves complete-export refusal. The local runner now includes it.

Actual source export passed at `target/prototypes/esp32-switches-e2e-v1/receipt.json`:
21 selectable bodies; USB J1, LED D2 and CP2102 U3 remain refused. USB's unused
palette derivation exposes a separate degenerate-triangle refusal; LED parses but
is refused during f32 placement/mesh qualification. Those gates are not relaxed.
This improves source-reference coverage, not photographed specimen equivalence.

Independent read-only source analysis isolated exact zero-area decimal faces:
LED Shape22/face1 and Shape50/face4 (2 of 360 triangles), and USB
Shape409/faces5–7, Shape411/face10, Shape473/faces0–1 and Shape494/face10
(7 of 4,847 triangles). Unit multiplication introduces tiny binary residuals for
the LED, so its zero-area faces reach the GLB orientation refusal. Positional
precision itself is not the established cause. USB Shape473 consists only of
zero-area faces. These are source-mesh observations, not a defect in the physical CAD.

Next implementation: explicit, bounded original-Rust derivation using exact
decimal source coordinates, with proof of collinearity before unit scaling.
Report source hashes, root/face ordinals, indices and byte spans for every removal;
preserve retained bytes and refuse unsafe/ambiguous cases. An empty geometry
Shape requires explicit recorded handling. Do not add an area epsilon or relax
retained-face placement, quantization or orientation gates. Expected counts from
independent analysis are LED358/50parts and USB4,840/483parts; actual Rust export
and independent geometry comparison must still establish them.

The switch v1 real browser journey passed21 selectors, nonblank synchronous
source-colour equality, overlay motion during orbit/zoom, resize, offline,4K and
context-loss suppression. Its numeric physical pose and surface fidelity remain
unverified. The stronger source-board-bound switch E2E subsequently passed at
`target/prototypes/esp32-switches-e2e-v2/receipt.json`.

### USB and LED exact-source derivation implemented

`model derive-wrl --remove-zero-area-faces` now proves collinearity with bounded
exact decimal arithmetic before removing faces. Optional unused-palette cleanup
composes explicitly; reports bind original, intermediate and derived hashes and
record every removed byte span. Remaining coordinates/material bytes are retained;
strict mesh parsing, placement and quantization gates still apply.

Actual E2E at `target/prototypes/esp32-usb-led-e2e-v2/receipt.json` independently
matches all removals with Python Fraction arithmetic and reconstructs the exact
derived bytes: USB 4,840 triangles / 483 parts, LED 358 / 50 parts. The ordinary
Rust GLB exporter produces 23 independently selectable bodies and still refuses
a complete export because U3 CP2102 is missing. Independent review accepted the
derivation proof. Browser tests passed 23 selectors, original material preservation,
offline selection, 4K capture, orbit/zoom/resize and context-loss suppression.
Source-reference geometry is not yet qualified against the photographed board.
