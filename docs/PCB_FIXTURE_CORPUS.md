# PCB fixture corpus

Reviewed 2026-10-09. These are importer/geometry regression inputs, not certification of a Tokito target board. Original files, hardware/software notices and source hashes are retained under tests/fixtures/open-source. No board-specific parser rules are used.

| Fixture | Immutable source commit | Format date | Footprints / pads | Segments / routed arcs / vias / zones | Source SHA-256 |
|---|---|---|---|---|---|
| SparkFun Basic Board | 8c5e9cf973a822bfc29c88972633ea8e4f34b4fe | 20241229 | 20 / 29 | 0 / 0 / 0 / 1 | 4ab99cd7db06be8e91e119a950a9ecc1f69bfd0d9a2ed975c5e57a0e319c8a83 |
| Pico community conversion | 69bf9b9c1b333dc3d890a3d99896dd3bae89c84a | 20240108 | 57 / 232 | 546 / 0 / 111 / 25 | b424466206b45f47a381de0cc69257e9fd38cdf0364d552451199cd845b12aeb |
| UNO community recreation | 0a25ca25953efb3fb045616e7c676a554ceaabc1 | 20240108 | 56 / 265 | 831 / 0 / 75 / 5 | 1b078b77026f4e8fd5de7cc6c7b6fff3daf454014e5e0873b7a6d0cdd77958f9 |
| NXP MCXA ESC | bb59ab34b41931f373f09edbd66a54afc26b30d2 | 20241229 | 97 / 353 | 491 / 0 / 128 / 11 | d0edaed749ac81ccc2387c78fc78f2f1e035b798f3e6f3705490d270e2f90535 |
| RoyalBlue54L NFC antenna (KiCad official demo) | a1b51d79b96b7dc2d4c3b374742de32d9b2144a7 | 20241229 | 2 / 2 | 112 / 48 / 2 / 0 | cd87e6d1b20fe0d129d9a9c807af3118c67749eff44b1f032d85c047a814c6dc |
| Porcupine routed PCB | 84b0c5dd3643bedda88a9d8a2d2e97965ebc206d | 20211014 | 54 / 599 | 647 / 607 / 52 / 211 | 1731dd63f7d31e0f2112e297e06ee91bdbd2dfcf3421ed02ee75c764af718ef2 |

## Provenance and boundaries

- [SparkFun source](https://github.com/sparkfun/SparkFun_Default_KiCad_Setup/tree/8c5e9cf973a822bfc29c88972633ea8e4f34b4fe), Example-Basic Board/SparkFun_Basic_Board_Example.kicad_pcb; hardware CC BY-SA 4.0. The original fixture README and LICENSE.md are retained.
- [Pico source](https://github.com/sabogalc/project-piCo/tree/69bf9b9c1b333dc3d890a3d99896dd3bae89c84a), KiCad Projects/Pico C/RPI-PICO-R3a-PUBLIC - Official.kicad_pcb; root WTFPL v2 notice retained. This is a community conversion, including USB-C-related changes; the filename's “Official” is not evidence of OEM or Tokito validation. Git blob d676f77dcfc3d1f6bb7befa5ef3a3f34c3d8e303.
- [UNO source](https://github.com/sabogalc/KiCad-Arduino-Boards/tree/0a25ca25953efb3fb045616e7c676a554ceaabc1), KiCad Projects/Uno/Arduino Uno/Arduino UNO.kicad_pcb; root WTFPL v2 notice retained. Community recreation with USB-C changes; not an exact OEM UNO R3. Git blob a74f6fb692fc303609305bf2e5c4b0093155d148.
- [NXP source](https://github.com/NXP-Robotics/MR-ESC-MCXA-AM32/tree/bb59ab34b41931f373f09edbd66a54afc26b30d2), MR-ESC-MCXA-AM32.kicad_pcb; exact pinned LICENSE is Solderpad Hardware License 2.1, SPDX Apache-2.0 WITH SHL-2.1. Git blob 82edf2d190b6215eb01cb31c4dd46da5f0ce43ac. This is an MCXA ESC, not FRDM-MCXN236.

Pico/UNO add real graphical/footprint arcs; NXP exercises custom-pad omission. The NFC fixture is a real curved Edge.Cuts design with 12 arcs and 10 lines, plus 48 routed-copper arc objects. Its committed KiCad Gerber profile is an independent reference for all 22 outline entities. The source board contains two ~0.0035355 mm (~3.5355 µm) open gaps, so the engine correctly refuses to certify or SVG-export it rather than snapping the endpoints together. The KiCad Gerber itself records the original unsnapped export. This fixture is useful precisely as a real negative case; it is not a valid board-outline demonstration.

Porcupine supplies a valid curved outline with 32 outer-contour line/arc edges and five explicit 2.05 mm circular Edge.Cuts cutouts. Its 607 root copper arcs are routed track objects, distinct from the 17 outline arcs. The actual CLI classifies one outer contour and five cutouts and exports both partial SVG views. Root/footprint source zone counts report present data only, not proof that every stored copper fill or design rule was understood.

These two designs replace the earlier gap in real routed-arc and cutout coverage. SVG remains partial: this corpus does not establish full KiCad parity, DRC, manufacturability, GLB/STEP correctness, Tokito target-board support, or device AR acceptance.

The subagent also identified [PMW3360 breakout](https://github.com/jfedor2/pmw3360-breakout/tree/13738eece595fefb1246524429a3c1d3ba7e6ebf), CC0, with legacy KiCad 5 Edge.Cuts arcs and cutouts. It was not downloaded into the supported corpus because format 20171130 is outside the current adapter's declared range. Legacy module and arc syntax needs its own adapter work.

- [KiCad RoyalBlue54L NFC demo](https://github.com/KiCad/kicad-source-mirror/tree/a1b51d79b96b7dc2d4c3b374742de32d9b2144a7/demos/royalblue54L_feather/nfc_antenna/RoyalBlue54L-NFC-Antenna), PCB and matching Gerber paths under the pinned commit. Hardware design is CERN-OHL-Pv2 per retained upstream notice. Exact source, LICENSE, README, and `-EdgeCuts.gbr` are committed. The reference header records KiCad 9.0.0-rc2, mm coordinates, and original coordinate mode. Its stored export declares 22 entities (12 G02 arcs, 10 G01 lines); the corpus test matches each source entity by endpoints (1e-6 mm) and checks arc center/radius against Gerber IJ offsets (5e-6 mm).
- [Porcupine routed PCB](https://github.com/anarion80/porcupine/tree/84b0c5dd3643bedda88a9d8a2d2e97965ebc206d/output/porcupine/pcbs/porcupine_routed.kicad_pcb), exact board and upstream LICENSE/README retained. Hardware license is SHL-2.1 as stated by the upstream repository. It exercises a valid curved perimeter, five circular cutouts, and 607 root routed copper arcs. It has no separately committed independent plot reference.

## User-supplied VTOL project

The external Downloads project VTOL_H743_R0 was inspected read-only; it is not copied into this repository or used as independent CAD truth. Its board SHA-256 is 0f2079d47f8304d9349084fcaf6bd3463252a1528b99560aa0c31e97cd095db7.

Source audit: format 20241229, six copper layers, 1.6 mm thickness, 116 footprints, 448 physical pad objects, 126 named nets plus empty net zero, 2,114 segments, 262 vias, no routed arcs, three zones/filled polygons, rectangular 99 × 89 mm outline, 104 model declarations with 21 unique paths. Physical pad objects differ from its supplied report's 415 unique numbered pads. Twenty model paths reference bundled KIPRJMOD files; one uses KICAD9_3DMODEL_DIR and needs an explicitly configured library root. README records missing bodies and an unmanufactured, unpowered, flight-unqualified design. Its supplied ERC/DRC reports were not rerun.

## Remaining coverage

No independent KiCad-export visual oracle, true routed-arc real fixture, full outline topology/intersection proof, manufacturer-model fidelity, GLB picking, or device AR registration has passed. Exported SVGs remain explicitly partial.
