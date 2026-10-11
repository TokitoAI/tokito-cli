# ESP32 source detail and natural presentation

## Preferred detailed prototype

`demo/esp32-natural/` uses the verified 23-body source assembly, including U1
RF module, J1 USB connector, SW1/SW2 switches and D2 LED. It supersedes the earlier
19-body material-only preview for presentation. It retains 26 reference records;
CP2102 U3 remains missing. This is the community source layout, not a verified
photographed specimen twin.

The baseline is `demo/esp32/`; its original GLB binary geometry and every
non-material JSON section remain byte-for-byte unchanged. Eleven explicit
material overrides apply the ESP8266 dark solder-mask palette, charcoal plastics,
silver tin/RF shield and warm gold/copper contacts. The profile is
`experiments/twin-refinement/profiles/esp32-natural-complete-source.json`.
No textures, board markings or component geometry are invented. Materials are
visual approximations, not measured physical reflectance.

The original Rust refinement tool applies the hash-pinned profile; the shipping
CLI then prepares the GLB/map/manifest and generates a self-contained viewer
with its hash-bound reference BOM comparison. `material-refinement-report.json`,
`material-acceptance.json` and `browser-acceptance.json` preserve scope and checks.
Actual browser acceptance passed all 23 selections, orbit/zoom/reset, offline
interaction after loading and a 3840x2160 screenshot. There are 30,754 source
triangles. Geometry and physical registration remain separately unqualified.

## Reproduce from the frozen baseline

```powershell
cargo build --manifest-path experiments/twin-refinement/Cargo.toml --locked
./experiments/twin-refinement/target/debug/tokito-twin-refinement.exe demo/esp32/active.digital-twin.glb demo/esp32/component-map.json demo/esp32/active.digital-twin.json experiments/twin-refinement/profiles/esp32-natural-complete-source.json target/esp32-natural-new --allow-partial
./target/release/tokito-cli.exe twin prototype target/esp32-natural-new/active.digital-twin.glb --component-map target/esp32-natural-new/component-map.json --manifest target/esp32-natural-new/active.digital-twin.json --output target/esp32-natural-new/prototype --allow-partial
```

The optional `--reference-bom` requires the exact staged dataset CSV, not an
inferred or fabricated BOM. Output directories must be new. These experiments
remain pure Rust and do not invoke KiCad or a native CAD kernel.

## Historical 19-body material preview


The original Rust twin-refinement tool applies eight explicit presentation
material overrides from `experiments/twin-refinement/profiles/esp32-natural-v9.json`
to the user-selected `esp32-central-module-e2e-v9` baseline. Its baseline SHA-256
is pinned in the profile. This is a visual material approximation, not measured
physical reflectance or a geometry upgrade.

The dark solder-mask palette matches the ESP8266 source preview. Silver RF shield
and tin terminals, charcoal plastics/resistors, warm copper and gold contacts use
explicit metallic/roughness factors with the existing studio-lighting viewer.

The GLB binary geometry and every non-material JSON section remain unchanged.
Component rows, identities and transforms remain unchanged. Asset hashes and
conservative qualification metadata are regenerated through the Rust admission
boundary. The CLI prototype retains the reference BOM comparison.

Local output: `target/prototypes/esp32-natural-materials-e2e-v1/prototype-with-bom/`.
Actual browser acceptance passed all 19 available selectors (26 reference records)
and a 3840x2160 screenshot, with no viewer error. Receipt: `material-acceptance.json`.
This selected baseline still has five missing required models. The frozen
GitHub demo/esp32 has a different source-generation history; this presentation
preview does not replace it or claim that missing bodies have been completed.
