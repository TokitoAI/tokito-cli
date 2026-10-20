# Selectable prototype delivery

The `demo/` directory contains frozen CLI prototype outputs for a local judging
demo. It deliberately includes the actual models alongside the source code;
build caches, executable binaries, photos and bulk STEP datasets are excluded.

| Bundle | Component records | Selectable bodies/groups | Remaining qualification |
|---|---:|---:|---|
| ESP8266 NodeMCU source reference | 34 | 34 | Physical pose/surfaces, pads/drills/silkscreen and AR unqualified |
| ESP32 natural presentation | 26 | 23 | Same source geometry with explicit materials; CP2102 U3 and specimen qualification remain open |
| ESP32 community-layout reference | 26 | 23 | CP2102 U3 missing; photographed clone correspondence and physical geometry unverified |
| Ultimainboard 2.1.4 | 574 | 310 | Four package bodies and four spacers unresolved; physical fit and registration unverified |
| NXP refined accepted prototype | 391 | 351 | 20 placeholders remain; J7/J14/J21 not fitted; physical/AR unqualified |
| NXP Rev C source-reference baseline | 391 | 354 | Physical population/geometry unverified; latest dataset PR #21 intake is separate |

ESP32 is a Rust CLI-generated partial assembly using source PCB placements and
the approved VRML library. Its exact-zero-face derivation retains remaining
source coordinates and materials. Ultimaker and NXP preserve and adapt dataset
source meshes into selectable references; they are not newly reconstructed CAD.
See [ESP32](ESP32_POPULATION_AUDIT.md),
[Ultimaker](ULTIMAKER_SELECTABLE_PROTOTYPE.md) and
[dataset provenance](DATASET_BOARD_HANDOFF.md) for source pins and unresolved work.
No cuboid or board shell is counted as a complete missing component.

Ultimaker's 94 multi-mesh groups work in this web viewer, but its preparation
report says `appNameFilterCompatible: false`. Afan's App needs group-aware import
and selection before this asset can be presented as App-compatible.

The NXP bundle is the browser-tested baseline, not an assertion that the latest
Anandhu PR #21 asset has been visually or physically accepted. ESP8266 source
records and positioned STEP candidate mesh data are implemented in Rust, but
the new selectable ESP8266 source-reference GLB is included under `demo/esp8266/`, with explicit partial/qualification limits. See
[ESP8266 progress](ESP8266_PARALLEL_PROGRESS.md).

## Run locally

From the repository root:

```powershell
python -m http.server 8765 --bind 127.0.0.1 --directory demo
```

Open `http://127.0.0.1:8765/esp32/index.html`,
`http://127.0.0.1:8765/ultimaker/index.html`, or
`http://127.0.0.1:8765/nxp-reference/index.html`.
Select a component by reference, click visible geometry, orbit, zoom or reset.
Each viewer contains its own GLB payload and requires no CDN.

Build the CLI with `cargo build --release --offline --locked` when the locked
dependencies are cached. Inspect or select the packaged assets:

```powershell
./target/release/tokito-cli.exe twin inspect demo/esp32/active.digital-twin.glb --component-map demo/esp32/component-map.json --manifest demo/esp32/active.digital-twin.json
./target/release/tokito-cli.exe twin select demo/esp32/active.digital-twin.glb --component-map demo/esp32/component-map.json --manifest demo/esp32/active.digital-twin.json --reference U1
```

## Evidence boundary

Current presentation refresh: all six packaged viewers use the versioned
[Ultimaker standard](THREED_PRESENTATION_STANDARD.md). Real Edge browser checks
passed 4K PNG encoding/download, state restoration, both lighting modes and
offline selection. See [the browser receipt](evidence/ultimaker-presentation-browser-20261016.json)
and [migration receipt](../demo/presentation-standard-verification.json).
GLB, map and manifest bytes and existing component-field values are unchanged;
indexes gain current explicit diagnostic metadata. New `standard-4k.png`
images are actual model captures. Earlier screenshots and results below retain
their historical scope. Refreshed manifest entries use portable logical
generation IDs (`presentation-refresh/v1/...`); previous prototype paths are
retained as historical provenance. Exact local builder identity is recorded in
the migration receipt rather than used as a package link.

`demo/bundle-manifest.json` binds each packaged artifact to its historical
prototype path or logical generation ID, byte size and SHA-256. Copies were
compared byte-for-byte. The
current verified release CLI successfully inspected all three packaged boards
and selected ESP32 U1, Ultimaker C74 and NXP U11.

Historical browser acceptance and 3840×2160 snapshots are retained. Packaging
does not imply a new browser run: unchanged viewer/model bytes preserve those
specific results. 4K is visualization resolution, not a CAD accuracy certificate.

Current code verification passed all 34 local Windows gates at
`target/delivery/local/20261010T203350Z-8f30e028a04547c2a052b9dc6dd1b9a8/verification.json`.
This documentation and frozen bundle were added afterward and checked
separately; they were not inputs to that receipt. GitHub Actions, releases and
binary uploads remain unused. Physical fit, App import, YOLO identity matching
and registered phone AR remain separate unfinished acceptance gates.

See [the final ESP8266 acceptance and Afan handover](AFAN_INTEGRATION_HANDOVER.md#esp8266-delivery-acceptance--11-october-2026) for the new 35-gate receipt, real browser tests, evaluated bounds comparisons and remaining limitations. Open `/esp8266/index.html` when serving `demo/`.

The preferred ESP32 presentation is `demo/esp32-natural/`; [source detail and material evidence](ESP32_NATURAL_MATERIALS.md) preserves its baseline, palette and comparison gates.
