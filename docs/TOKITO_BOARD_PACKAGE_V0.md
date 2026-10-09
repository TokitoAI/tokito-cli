# Tokito board package v0.1

Version `0.1.0` is an offline directory package for preserving a supported KiCad PCB source and the CLI's current placement inventory. It is an early Tokito-native interchange artifact, not a complete canonical board model.

Build one with:

```text
tokito-cli package build <BOARD.kicad_pcb> --output <NEW_DIRECTORY>
```

The output directory must not already exist. The CLI stages the package beside the destination, reserves the final name, and renames the completed directory only after writing all files and checking that the copied source still has the byte count and SHA-256 observed during import. It refuses an existing destination. A reader may briefly observe the reserved empty directory at publication time; treat a directory without `manifest.json` as incomplete. Directory publication has been tested on the project's current Windows NTFS volume; other filesystems may reject replacement of the reserved empty directory and return an error. Individual output files also require atomic hard-link publication support; unsupported filesystems return an error without replacing an existing destination. The current F: project volume is NTFS.

```text
manifest.json
source/board.kicad_pcb
derived/inspection.json
derived/placements.csv
```

The manifest follows [the v0.1 JSON Schema](schemas/board-package.schema.json). It records the supported source format version, byte count, source digest, derived artifact digests, placement row count, producer version, and explicit capability/limitation flags. Package IDs are deterministic from the first 16 hexadecimal characters of the source SHA-256. Generated files omit timestamps and machine-specific absolute source paths.

The package preserves the original input but the inspection and placements contain only fields understood by this prototype. The placement CSV is an inventory, not a manufacturing position file; it retains UUID-backed graphic placeholders. The package has no complete board geometry, SVG, GLB, STEP, `tokito-app` board-reference asset, or app digital-twin manifest. `mobileImportReady` is always false.

The Android runtime currently validates a separate GLB plus its digital-twin manifest against a board-context revision. A successful CLI package build therefore does not mean that the phone can import or display the package. A later package version should add a verified canonical board model and only then join the app's existing contracts through a separately tested app import journey.

The source file is copied with its original contents, but adjacent license and provenance files are not discovered or copied automatically. Keep those records with the package when needed. The CLI does not determine or replace upstream licensing conditions.

## Native intake and verification

`tokito-cli package verify <DIRECTORY> [--json]` verifies the v0.1 fixed manifest layout, source/artifact SHA-256 and bytes, exact current-engine placement/inspection consistency, schema and capability values. `inspect`, `export placements`, and `export svg` also accept the directory and reimport its verified preserved PCB source. This is the first native Tokito input adapter; it does not accept desktop schematic JSON as physical layout.

Manifest input is capped at 64 KiB, each derived file at 64 MiB, PCB source at 256 MiB. Duplicate JSON keys, unknown versions, escaping/arbitrary asset paths and package member links/reparse points are rejected. Fixed paths are selected by the implementation, never followed from manifest text. The source is parsed freshly before any operation. Verification compares the stored inspection to current engine output; rebuild earlier development packages whose cached inspection predates geometry fixes. No released package compatibility is implied by the prototype producer version.

The existing v0.1 schema and artifact inventory remain unchanged. SVG is a separate export and remains partial. Verification establishes local content consistency and integrity, not a signature, trusted publisher, full CAD validity or mobile compatibility. Unreferenced extra files are ignored. Concurrent hostile filesystem mutation is not a supported intake environment; a future importer needs immutable staging before installing assets.
