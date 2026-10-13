# Existing-project export bundle

Status: implemented as local, unmerged work on `codex/project-export-bundle`.
Run `scripts/verify-local.ps1` against the current source before delivery. This
command prepares one selected, existing physical PCB for offline handoff. It is
not a KiCad project validator, a schematic-to-PCB router, or the Tokito App's
accepted digital-twin package.

## Build and verify

```powershell
tokito-cli project inspect "C:\Boards\MyBoard" --json
tokito-cli project build "C:\Boards\MyBoard" `
  --board MyBoard.kicad_pcb `
  --expect-sha256 <64-hex-source-digest> `
  --output "D:\TokitoExports\MyBoard-2026-10-11" `
  --assemble-wrl `
  --library-root "C:\LocalModels\KiCad" `
  --allow-partial `
  --json
tokito-cli project verify "D:\TokitoExports\MyBoard-2026-10-11" --json
```

The output parent must already exist. The destination must be a new directory,
outside and non-overlapping with the project directory. During finalization the
builder reserves that destination exclusively, publishes files without
replacing existing paths, and writes `manifest.json` last as the complete-bundle
marker. A concurrent build to the same destination fails without replacing the
successful bundle. The source selection is top-level only; a sole PCB is
selected automatically, and multiple PCBs require an exact `--board` filename.
`--expect-sha256` pins the board revision. The source is rechecked before
publication and copied byte-for-byte into the bundle.

The default GLB is a board-shell candidate. `--assemble-wrl` enables only the
current bounded local VRML model subset. Missing or unsupported component
models refuse the build unless `--allow-partial` is explicit. An explicit
partial build records the omissions; it does not replace a missing part with a
placeholder body.

## Bundle layout

```text
<bundle>/
├── manifest.json
├── source/
│   └── board.kicad_pcb
└── derived/
    ├── inspection.json
    ├── placements.csv
    ├── outline.json
    ├── board.top.svg
    ├── board.bottom.svg
    ├── board.glb
    ├── glb-coverage.json
    └── component-map.json
```

The manifest records the original PCB path within its source project, importer
version, source size and SHA-256, and a size/SHA-256/media-type entry for every
derived artifact. The component map preserves each imported footprint's UUID
and reference. A `ref:<reference>` selector is included only when that identity
is unique and the active GLB scene contains reachable mesh geometry for it.
Placeholder references, duplicate identities, and components without emitted
meshes have explicit reasons and no selectable geometry claim.

## Verify semantics and limits

`project verify` checks the exact directory layout, rejects symlinks and
reparse points, enforces bounded sizes, verifies each digest, reimports the
embedded PCB, and recomputes component selector reachability from the GLB. It
returns integrity evidence, not a signed provenance statement: a party able to
edit the bundle can also rewrite its manifest and hashes.

Build and verify assume the caller controls the output parent directory. The
exclusive destination and file creation protect against ordinary competing
CLI builds and existing paths; they do not claim safety against a malicious
same-user process swapping filesystem objects concurrently.

The bundle intentionally sets `geometryComplete`, `mobileImportReady`,
`appDigitalTwinManifestIncluded`, and `arRegistrationVerified` to `false`. It
does not invent the App's active board-context revision, phone calibration,
board pose, photo-to-CAD correspondence, or model qualification. A renderable
selector only proves that a mesh can be addressed; it does not prove the part's
physical dimensions or placement are correct. The Rust importer/exporters still
cover a subset of PCB features, and SVG completeness warnings remain in the
manifest.

This is the first repeatable CLI handoff step. App import and registered AR
remain separate acceptance gates owned with the App integration: agree on the
revisioned app manifest inputs, qualify the board/model geometry, import this
bundle offline, select a component in the renderer, and verify registered
highlights on the target phone and exact physical board revision.

## Local acceptance

`tests/project_bundle_e2e.py` runs the normal release CLI over a single-board
fixture. The local verifier separately builds one binary with the non-default
`e2e-test-hooks` feature for a deterministic post-copy cleanup failure; normal
builds do not include that fault hook. The normal-binary E2E checks pinned
source bytes, placements and UUIDs, both SVG views, the selectable GLB
node/component map, coverage, bundle verification, explicit partial consent,
refusal to overwrite existing output (including an empty directory), competing
publishers, ambiguous board selection, nested output refusal, invalid-outline
refusal, and tamper detection. The full repository release gate is
`scripts/verify-local.ps1`; no GitHub Actions workflow is used.
