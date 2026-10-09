# LumenPnP vacuum-adapter geometry extract

This is a reduced regression fixture for curved physical-outline geometry. It is not the complete upstream PCB, is not a Tokito target-board support claim, and must not be represented as the source project's original binary/design file.

## Provenance

- Upstream project: [Opulo LumenPnP](https://github.com/opulo-inc/lumenpnp)
- Pinned mirrored source commit: [11f17018e61c1513400680852bd1f34afb207b3c](https://gitea.interbiznw.com/KobolSystems/lumenpnp/commit/11f17018e61c1513400680852bd1f34afb207b3c), a merge from the upstream `v3` branch
- Source PCB path: `pnp/pcb/vac-interposer/vac-adapter.kicad_pcb`
- Same-commit physical profile: `pnp/pcb/vac-interposer/out/REV00/vac-adapter-Edge_Cuts.gbr`

Both local files were reconstructed from the pinned commit’s indexed source records: the PCB extract from its board records and the Gerber extract from its Edge.Cuts profile records. Neither upstream artifact was downloaded byte-for-byte. The local PCB keeps board thickness, one footprint pose/identity, six root Edge.Cuts records, and two footprint-owned Edge.Cuts circles. The Gerber extract keeps four lines, two quarter-circle arcs, and two full-circle arcs. The original PCB and raw Gerber were not downloaded, so no upstream blob IDs or raw-file checksums are asserted; the SHA-256 values in `geometry_e2e.py` fingerprint only these local reduced files.

The checked-in circle records retain footprint UUID `aa130053-a451-4f12-97f7-3d4d891a5f83` and graphic UUIDs `15cef0c4-75f7-4294-bc4e-e717462bc021` and `fcb4c6b1-4e0b-49ae-a4be-e4853ff3b06b`. The E2E compares normalized board-space circle centers/radii and outline lines/arcs to the paired Gerber, then checks the generated SVG keeps each graphic UUID and owning component UUID.

This fixture does not validate parsing of the complete upstream PCB, pad/net/routing/model interpretation, manufacturing readiness, or all Gerber commands. See the upstream repository and notices for the applicable source terms; this reduced fixture does not reproduce its full notice/license bundle.
