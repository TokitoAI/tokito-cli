# KiCad SOT-23 comparison fixture

`SOT-23.kicad_mod` is an unmodified LF-byte snapshot of the official KiCad
footprint-library file at commit
`f8a45b4665c28a6280e46eefe9fd4df81d2cd17b`. Its SHA-256 is
`0d88c86c1b6d8c1aae4aa8c09386264c089d4451a128b6ccd63ed253f4300b15`.

The upstream file is from the official KiCad Footprints library and is
licensed under CC-BY-SA 4.0 with KiCad's stated exception. See the
[upstream license](https://gitlab.com/kicad/libraries/kicad-footprints/-/blob/f8a45b4665c28a6280e46eefe9fd4df81d2cd17b/LICENSE.md)
and [source file](https://gitlab.com/kicad/libraries/kicad-footprints/-/blob/f8a45b4665c28a6280e46eefe9fd4df81d2cd17b/Package_TO_SOT_SMD.pretty/SOT-23.kicad_mod).

This fixture is used for bounded syntax and supported-geometry comparisons.
A match to the library item does not qualify the manufacturer's land pattern,
prove physical fit, or imply that the CLI verifies external source evidence.
