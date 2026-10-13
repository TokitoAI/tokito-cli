# KiCad GLB reference capture

A separate [REA investigation and DNP comparison](REA_KICAD_CLI_10_0_5.md)
records installed 10.0.5 command-handler evidence and the NXP R2/R4 filter delta.
It does not extend this capture into full geometry or 10.0.7 equivalence.

This document records one repeatable black-box reference capture. It exists to
test a known external artifact before Tokito emits component mesh nodes. It is
not a runtime dependency, a substitute for a supported KiCad adapter, or a
claim of KiCad 10.0.7 compatibility.

## Pinned 10.0.5 capture

The source fixture is the community Arduino Uno R3 at
`tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb`,
SHA-256 `1b078b77026f4e8fd5de7cc6c7b6fff3daf454014e5e0873b7a6d0cdd77958f9`.
Its Edge.Cuts outline is asymmetric and `U3` is a front-side footprint at
`(134.1501, 97.0026)` mm, rotated 90 degrees.

KiCad 10.0.5 with its installed QFN model emits a named `U3` node with:

```text
translation (m): [0.1341501, 0.001595, 0.0970026]
rotation (xyzw): [0, 0.7071067811865475, 0, 0.7071067811865475]
```

The model is
`Package_DFN_QFN.3dshapes/QFN-32-1EP_5x5mm_P0.5mm_EP3.3x3.3mm.step`, whose
installed file hash is
`5fa713ea46b36980895812ed853577889b7cb06d275041a39b99bc0be305dc79`.
The first observed GLB was 496,008 bytes with SHA-256
`5242bdb229de91ab0489dda1c4a9c091e0e876c3bbe5c454ffec647a52776a7a`.
A second successful run had the same checked node values and byte length but
SHA-256 `99740fd45f4f7d67ba4662e2c7ff1cc91ada4a5cb9196ed3fbf87e957e816971`.
The capture records each artifact hash but intentionally does not require
byte-identical KiCad output.

Run the independent capture locally:

```powershell
python tests/kicad_glb_reference.py `
  --kicad-cli 'C:\Program Files\KiCad\10.0\bin\kicad-cli.exe'
```

The script insists on version 10.0.5 by default, hashes the exact KiCad
executable, board and model, runs `pcb export glb` with an explicit
model-library variable, and validates the glTF 2 JSON node. Each run copies the
board into a unique owned directory below `target/kicad-reference-10.0.5/`,
then retains the copied board, generated GLB, KiCad-local settings, command
receipt and per-run hashes there. It never writes beside the committed fixture
or overwrites a prior capture. It is deliberately outside `verify-local.ps1`,
which must stay pure-Rust and offline with no KiCad dependency.

## What it proves and what remains

The capture proves a current external executable can emit a non-origin,
90-degree component node from the pinned input. An optional FreeCAD smoke can
be run with:

```powershell
python tests/kicad_glb_reference.py `
  --kicad-cli 'C:\Program Files\KiCad\10.0\bin\kicad-cli.exe' `
  --freecad-cmd 'C:\Program Files\FreeCAD 1.1\bin\FreeCADCmd.exe'
```

It records the resolved FreeCAD executable's version and SHA-256 plus its
imported-object and non-null-shape counts in the receipt. The development run
on FreeCAD 1.1.4 produced 364 non-null shapes. This is an import smoke test,
not a geometric equivalence measurement, and is not a C-09
transform-comparison acceptance gate.

KiCad GLB uses a different board-plane direction than Tokito's current shell:
the observed external node uses positive source-board Y in GLB Z, while Tokito
uses negative saved Y in GLB Z. C-09 therefore remains incomplete until a
reviewed frame conversion compares mesh geometry, an asymmetric non-zero
model-local transform, front/back placement, and a designated 10.0.7 binary.
