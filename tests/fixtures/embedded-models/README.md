# Embedded-model compatibility fixtures

`step-sample.zst` is a 34-byte Zstandard frame for the 32-byte test payload
`ISO-10303-21;\nEND-ISO-10303-21;\n`. It includes the frame content size field
required by KiCad's embedded-file decoder. The fixture was produced with the
KiCad 10.0.7 bundled `zstd.dll` at compression level 15. Its payload SHA-256 is
`25bdbce8f3930e778844aff9e6deb9dd4aa5f01315979d7f7eebc68bbc301313`; its
KiCad-seeded MMH3 x64/128 checksum is
`36B783AF2314FC403FDE27515CB137B3`.

The resistor WRL `.zst` fixture beside the open-source model is likewise a
KiCad-compatible frame, generated from the adjacent original WRL. The local
native comparison uses KiCad 10.0.7 to export the embedded model to GLB; this
checks import acceptance, not geometric equality between renderers.
