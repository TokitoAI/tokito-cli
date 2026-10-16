# KiCad 10.0.7 SVG DNP-filter reference

This records one version-pinned native behavior comparison for the Tokito SVG
export investigation. The KiCad CLI is used only as a development reference;
Tokito's runtime does not invoke it.

## Captured reference

- Native reference: KiCad CLI 10.0.7, observed from
  `D:/TokitoKiCadRef107/bin/kicad-cli.exe`; SHA-256
  `cbd8197dd1f46b9a956f3d7973edc9a6a97960298731448ae9142dffe47abc7a`.
- Input: `tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb`;
  SHA-256 `1b078b77026f4e8fd5de7cc6c7b6fff3daf454014e5e0873b7a6d0cdd77958f9`.
- Capture time: 2026-10-14 22:15 UTC.
- The pinned native SVGs and machine-readable evidence are in
  [`tests/reference/kicad-cli-10.0.7/svg`](../tests/reference/kicad-cli-10.0.7/svg/manifest.json).

## Reproduction

Run each command with the 10.0.7 executable and the pinned board:

```powershell
kicad-cli pcb export svg --mode-single --layers F.Fab --page-size-mode 2 --exclude-drawing-sheet --output uno-fab-default.svg "Arduino UNO.kicad_pcb"
kicad-cli pcb export svg --mode-single --layers F.Fab --page-size-mode 2 --exclude-drawing-sheet --hide-DNP-footprints-on-fab-layers --output uno-fab-hide-dnp.svg "Arduino UNO.kicad_pcb"
```

The default output has 6,065 path elements, 107 text elements, and two `1M`
reference labels. With DNP Fab hiding enabled it has 6,002 paths, 103 text
elements, and no `1M` labels. Both outputs report the same 68.58 × 53.34 mm
board-area viewBox.

The KiCad 10.0.5 captures for the same source have the same structural counts
and extents. After replacing only the SVG `<title>` element (which contains the
output filename and generation timestamp) with a fixed marker, both versions'
default output hashes match, and both versions' hidden-DNP output hashes match.
Raw output hashes differ because the title timestamp differs.

## What this establishes

For this one Uno board and `F.Fab` layer, KiCad 10.0.7 preserves the same
`--hide-DNP-footprints-on-fab-layers` behavior observed in 10.0.5: DNP-owned Fab
drawings and labels are omitted, while the remaining page geometry and visible
labels retain identical normalized output. The Tokito E2E separately verifies
that its opt-in implementation removes DNP-owned geometry by source identity
and preserves the other footprint geometry on the same fixture.

This is one focused regression comparison. It does not establish general SVG
plot parity, every layer combination, variants, all DNP styles, other KiCad CLI
commands, or full manufacturing output equivalence. Tokito SVG remains
explicitly partial; inspect the parity matrix for unsupported behaviors.

## Required local regression check

`scripts/verify-local.ps1` runs
`tests/kicad_svg_reference_evidence.py` as a required local gate. That offline
validator checks the pinned 10.0.7 executable version/help captures, source and
output hashes, SVG metrics, DNP label suppression, and normalized equality with
the 10.0.5 captures. It validates the recorded evidence; it does not rerun
KiCad 10.0.7 or expand this one-board comparison into general SVG parity.

The official [KiCad 10.0.7 release note](https://www.kicad.org/blog/2026/10/KiCad-10.0.7-Release/)
and [Windows download page](https://www.kicad.org/download/windows/) identify
10.0.7 as the stable release used for this comparison.
