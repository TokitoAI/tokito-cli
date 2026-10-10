# Original LumenPnP vacuum-adapter reference

These files are unchanged original bytes from official `opulo-inc/lumenpnp`
commit `11f17018e61c1513400680852bd1f34afb207b3c`, independently checked against
Git blob IDs and SHA-256 after acquisition. The older reduced fixture remains
in its separate directory; it is not substituted for this full PCB.

| Local file | Upstream path | Bytes | Git blob | SHA-256 |
|---|---|---:|---|---|
| vac-adapter.kicad_pcb | pnp/pcb/vac-interposer/vac-adapter.kicad_pcb | 23419 | 6f3409e9dcacf117657f9f5f7901d6e154261e44 | 2b21f974c74f9521b45717f17e7f6f67489f7a47316db0f9eb2697f234edcce3 |
| vac-adapter-Edge_Cuts.gbr | pnp/pcb/vac-interposer/out/REV00/vac-adapter-Edge_Cuts.gbr | 1066 | c51857e82bd4bbbc119e8b0c6279436c0ee17e96 | 698d1cad11241bf7f225e8fa9a631fbbc7f5a74b995151b5ad328e52ad2785e1 |
| LICENSE | LICENSE | 35149 | f288702d2fa16d3cdf0035b15a9fcbc552cd88e7 | 3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986 |
| UPSTREAM_README.md | README.md | 1793 | 5c31021c60706a33947f72d608032ba25c3237e8 | babdd314c2fb59b784b9f6dda117ff0bb3fe4bd0998c1f8358f236f0a0414c11 |

Sources: [original PCB](https://github.com/opulo-inc/lumenpnp/blob/11f17018e61c1513400680852bd1f34afb207b3c/pnp/pcb/vac-interposer/vac-adapter.kicad_pcb),
[stored Gerber](https://github.com/opulo-inc/lumenpnp/blob/11f17018e61c1513400680852bd1f34afb207b3c/pnp/pcb/vac-interposer/out/REV00/vac-adapter-Edge_Cuts.gbr),
[upstream license](https://github.com/opulo-inc/lumenpnp/blob/11f17018e61c1513400680852bd1f34afb207b3c/LICENSE).
The pinned repository retains the GPL version 3 license text; no application
code is copied into Tokito. This fixture is an upstream PCB/profile input, not a
Tokito target board or fabrication approval.

Source format is `20211014`, thickness 1.6 mm, seven footprints, twelve pads,
eighteen tracks and no vias/zones. The physical boundary contains four lines,
two arcs and two footprint-owned circular cutouts. The stored profile identifies
KiCad 6.0.1, millimetres, original coordinates and 4.6 precision.
The full board also declares an unresolved `${KISYS3DMOD}` WRL model; this
fixture does not qualify component geometry, model placement or manufacturing.

Geometry E2E compares the imported contours and actual SVG path/circle geometry
against the stored external profile without generating a new KiCad reference.
This checks the eight profile entities, including radius/sweep and component
UUID ownership; it does not establish full KiCad plotting or renderer fidelity.
