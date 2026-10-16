# Frozen selectable board demos

Open a viewer or serve this directory locally:

- [ESP8266](esp8266/index.html): 34 selectable source components, 62 actual body instances.
- [ESP32 natural](esp32-natural/index.html): preferred dark finish with USB, switches, LED and RF module; 23 selectable bodies.
- [ESP32 source baseline](esp32/index.html): 23 selectable bodies; CP2102 U3 is missing.
- [Ultimaker](ultimaker/index.html): 310 selectable component groups; partial source assembly.
- [NXP refined](nxp-refined/index.html): accepted prototype, 351 selectable bodies; J7/J14/J21 not fitted, 20 placeholders remain.
- [NXP reference](nxp-reference/index.html): 354 selectable source components; baseline reference candidate.

Each folder contains a GLB, component map, asset manifest, viewer and 4K snapshot.
The viewers are self-contained and retain their tested source colours. These are
unregistered reference prototypes, with physical fit and AR guidance unverified.
ESP8266 is an explicit partial source-reference assembly; pads, drills, silkscreen and physical pose remain unqualified.

See [demo provenance, commands and limits](../docs/DEMO_BUNDLE.md).

Current viewer regeneration uses the [Ultimaker presentation
standard](../docs/THREED_PRESENTATION_STANDARD.md), with source colours preserved,
optional material-studio lighting and **Save 4K PNG**. The pinned GLBs and
component transforms remain the authoritative source assets. Existing named
screenshots retain their historical provenance; a new presentation does not
qualify previously missing or incomplete component geometry.
