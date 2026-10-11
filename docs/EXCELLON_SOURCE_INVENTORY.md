# Explicit Excellon source inventory

The original Rust experimental reader adds `inspect-drill <file>` to
`tokito-altium-evaluation`. It does not invoke KiCad or a native CAD library.
It reports source coordinates in millimetres, tool diameters, hole centres,
G85 slot endpoints, source lines, plating declarations and the input SHA-256.
An inventory exits with status 2 and `admitted: false`: parsing a fabrication
file does not admit a physical model or register it to a board/photo.

## Supported source profile

Inputs must declare `M48`, one `;FILE_FORMAT=integer:fractional`, explicit
`INCH`/`METRIC` and `LZ`/`TZ`, a `;TYPE=PLATED`/`NON_PLATED` claim, tool
diameters and a terminated `M30` program. Coordinates are absolute with modal
X/Y; `G90`, `G05` and G85 canned slots are supported. For this source profile,
LZ means retained leading zeros (missing trailing digits are right-padded),
whereas TZ retains trailing zeros. Decimal coordinates are scaled directly.
Unknown commands, relative mode, absent/duplicate declarations, undefined
tools, missing initial axes, truncation and post-termination data refuse.
The ASCII input budget is 1 MiB, with bounded records, tools and features.

This is a bounded profile, not general Excellon or KiCad parity. The LZ
interpretation agrees with the actual Altium output and KiCad's
[version-pinned Excellon header behavior](https://github.com/KiCad/kicad-source-mirror/blob/bd256c07fc6263d1268ec53d0d3f66932123a1e1/gerbview/excellon_image.h#L251).
The independent Decimal test shares this interpretation; it independently
checks arithmetic and record correspondence, not the format specification.

## Real ESP8266 source checks

Files come from the pinned NodeMCU reference dataset intake, directory
`gerbers-verified/NODEMCU_DEVKIT_V1.0_GERBER_V4`. They explicitly declare
`2:5`, `INCH,LZ` and their plating status.

| Source suffix | Holes | Slots | SHA-256 |
|---|---:|---:|---|
| `RoundHoles-Plated.TXT` | 275 | 0 | `e8e4c5a926c7b079cbbac1081e1ef9e0c62317dff1e361a14f5c4961a6f422f6` |
| `RoundHoles-NonPlated.TXT` | 4 | 0 | `aa0a5b4d3cf4e88b9d8b1ce9d1f10a1dc613762ae2e1762b71a229f2e8116f8f` |
| `SlotHoles-Plated.TXT` | 0 | 2 | `0a78fdcfba2ebe783076f1285f8b329918dfa175a9df4d9969305e5886aa6c71` |

`tests/esp8266_drill_inventory_e2e.py` invokes the actual Rust executable,
checks these hashes, all 279 centres and both slot endpoint pairs, tool
diameters/IDs, source lines and five invalid-input journeys. Decimal arithmetic
is the comparison oracle; the 1e-12 mm bound checks floating-point arithmetic,
not engineering tolerances. Coordinate vectors and tool counts are also checked.
The receipt is `target/prototypes/esp8266-drill-inventory-e2e.json`.
The local verification script includes this journey when the complete ESP8266
input set is supplied. The previous 35-gate delivery receipt predates this
change and does not verify the new source slice.

Independent source investigation found the 30 T4 header-hole centres agree
with J1/J2 source body positions after subtracting the declared PCB origin;
the maximum distance is about 0.000128515 mm. The per-axis source drill step
is 0.000254 mm. This correspondence is evidence for investigating that frame,
not whole-board registration or physical revision qualification. T1 holes
must not automatically be classified as vias or component pins.

## Experimental drilled substrate

The separate v4 Rust candidate includes 279 round openings and two slots.
Actual-output checks passed oriented closed-edge incidence, top/bottom area
equality and all opening-centre checks. Receipt:
`target/prototypes/esp8266-drilled-source-e2e-v4/drilled-substrate-e2e.json`.
FLOAT32 delivery quantization is recorded explicitly; maximum point displacement
is 0.000001902546 mm. Source coordinates remain in the inventory.

The frozen `demo/esp8266/` remains the earlier solid-substrate assembly. Browser
acceptance, complete cutout coverage, plating, pads, solder, physical revision
qualification and AR admission remain open.
