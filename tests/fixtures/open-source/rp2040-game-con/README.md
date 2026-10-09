# RP2040 Game Controller fixture

This fixture is copied from Chapter 11 of the Raspberry Pi Press *Raspberry Pi Pico Projects* KiCad project.

- Upstream project: https://github.com/raspberrypipress/raspberry-pi-pico-with-kicad/tree/6af0b2e65f8d00fa874883c827b29bc13087c757/eg/ch11/rp2040_game_con
- Immutable upstream commit: `6af0b2e65f8d00fa874883c827b29bc13087c757`
- PCB source: `eg/ch11/rp2040_game_con/rp2040_game_con.kicad_pcb` (Git blob `86f86768e0a91fb00317d160dc44d17cae63b46f`; SHA-256 `359ab533c810a2e5ee4881d7578e6e1d33beb55c8003f3a4dad792b6f00d23bd`)
- Fabrication archive: `eg/ch11/rp2040_game_con/gerbers.zip` (Git blob `416dcfb82060acfe8c4c14ee4c45fac5b237a418`; SHA-256 `75903c1a7422cf8ac7ecf9ea45acfd299f68b34a00c16003a159bfab13354160`)
- Extracted reference layer: `rp2040_game_con-Edge_Cuts.gm1` (SHA-256 `b823eb715f230855a4d86945391a17ccd822e00bce498a09381d1cac91808e7f`)
- Upstream license: CERN Open Hardware Licence Version 2 - Permissive (CERN-OHL-P-2.0). The original `LICENSE.txt` is retained here.

The local test validates the board through Tokito's public parser and compares every reported Edge.Cuts line with the paired profile export. The 644 line entities match one-to-one with a maximum endpoint error of 0 mm; the outline has one outer contour and four cutouts. This is a parser/topology regression fixture, not a Tokito product-board certification or a manufacturing acceptance report.
