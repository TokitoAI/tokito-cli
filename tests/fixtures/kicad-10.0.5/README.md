# KiCad R0603 comparison fixture

`R_0603_1608Metric.kicad_mod` is an unmodified snapshot of the official KiCad
footprint-library file at commit `51f8a59ed7d5f3bab9bb296c7b767bb033f3ba4c`.
Its upstream LF-byte SHA-256 is
`7190ac4a00125b807e54129ef0d87d87f2a658eeb74d025a7028203419b09f23`.
The source URI and the locally observed KiCad 10.0.5 CRLF-file digest are
recorded separately in `r0603-reference-geometry.json`; the latter is not the
digest of the GitLab raw response.

This upstream fixture is from the official KiCad Footprints library and is
licensed under CC-BY-SA 4.0 with KiCad's stated exception. See the
[upstream license](https://gitlab.com/kicad/libraries/kicad-footprints/-/blob/master/LICENSE.md)
and [source file](https://gitlab.com/kicad/libraries/kicad-footprints/-/blob/51f8a59ed7d5f3bab9bb296c7b767bb033f3ba4c/Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod).

The fixture is used only as a pinned syntax/geometry comparison input. A match
to this library item does not qualify the manufacturer's land pattern or prove
physical fit for a board or component.
