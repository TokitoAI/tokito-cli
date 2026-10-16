# Native footprint-type filtering for model assembly

`export glb --assemble-wrl --no-unspecified` excludes model references owned by
footprints without the native `smd` or `through_hole` attribute. This implements
one selection behavior from the KiCad 10.0.7 GLB exporter. It does not implement
STEP export or certify the geometry of a component or board.

```powershell
tokito-cli export glb board.kicad_pcb --assemble-wrl --project-root . --no-unspecified --output typed-components.glb --json-report
```

The flag applies before model resolution. It combines with reference patterns
and `--no-dnp`; missing models on retained components still require explicit
`--allow-partial`. Shell-only and model-assignment modes reject this flag, as
do duplicate occurrences. Default outputs omit the new optional filter metadata
and preserve the previous unfiltered behavior.

## Type and identity contract

The classifier reads the owning footprint's raw source. The canonical compact
attribute projection is insufficient because it omits repeated attribute
records and nested content. Private lexical evidence also retains whether
footprint attributes or filter-relevant structural heads (`kicad_pcb`,
`footprint`, `module`, `uuid`) used quoted strings; active type filtering rejects
those instead of confusing a quoted string with a native type token. Ordinary
quoted property values remain supported. Exactly one
`smd` or `through_hole` type establishes
a specified type. An absent or empty `attr`, or a record containing only known
non-type modifiers, is unspecified. Pads, footprint names, custom properties,
BOM entries, reference prefixes and model availability do not infer this type.
This mounting-type filter is separate from electrical component classification.

Known modifiers are `virtual`, `board_only`, `exclude_from_pos_files`,
`exclude_from_bom`, `dnp`, `allow_missing_courtyard`, and
`allow_soldermask_bridges`, as observed in the pinned 10.0.7 format parser.
Repeated attribute records, nested content, unknown tokens, and duplicate or
conflicting type tokens are refused. Those refusals are deliberate ambiguity
checks; invalid-input error equivalence with KiCad is not claimed.

An excluded footprint must have exactly one nonempty source UUID matching one
canonical component. Missing, repeated, malformed or ambiguous UUIDs refuse
the operation. This follows the existing native DNP filtering boundary; legacy
timestamp-only identities are not qualified for this filter.

The public `ModelAssemblyFilter::excluding_unspecified()` builder activates the
same rule. `GlbCoverageReport::unspecified_filter` is `None` by default. When
enabled, JSON and GLB metadata contain identical `coverage.unspecifiedFilter`
records with component and model-reference exclusion counts and
`overlapWithDnpComponents`. Individual filter counts may overlap; the existing
selected/filtered totals count their union once.

## Independent selection evidence

The capture in
[`tests/reference/kicad-cli-10.0.7/type-filter/manifest.json`](../tests/reference/kicad-cli-10.0.7/type-filter/manifest.json)
records the reference executable, generated fixture and model digests, command
recipe, native output hashes, component-node names, and normalized capture logs.
The fixture has twelve identities. It deliberately reuses one real KiCad
resistor body with different footprint attributes; it is a metadata test, not a
physically valid design. KiCad substitutes the recorded STEP companion using
`--subst-models`; Tokito independently assembles the pinned WRL asset. Only the
resulting component-selection identities are compared.

| Case | Native and Tokito selected references |
| --- | --- |
| Default | R1–R12 |
| `--no-unspecified` | R1, R2, R5, R11, R12 |
| `--no-dnp` | R1, R2, R3, R4, R7, R8, R9, R10, R11 |
| Both flags | R1, R2, R11 |
| Pattern `R3,R5,R6,R11` plus type filter | R5, R11 |
| Same pattern plus both filters | R11 |

R7 has an SMD pad but no footprint type, and is correctly excluded by the type
filter. R8 has an empty `attr`, which the native reference accepts as
unspecified. R6 is both DNP and unspecified, so the combined report explicitly
records one overlapping component. Custom `Type=smd` properties do not affect
the result.

The native capture also records KiCad's refusal of `(attr "smd")` (exit code
3). Tokito refuses the same malformed source when this filter is active; exact
native error text and exit-code parity are not claimed. The manifest additionally
pins the default GLB and report digests produced by the previously delivered
Tokito executable (`c8e96deec6409ae2ecf48d53f942722da2c56514cad66b45596ee8a91abfae60`).
Normal E2E compares current default outputs against these actual pre-change
binary outputs, rather than comparing two wrappers around the current code.

Reference capture is a separate, explicit development action:

```powershell
python tests/kicad_type_filter_e2e.py --capture-reference --kicad-cli <PINNED_10_0_7_EXE> --step-model <R_0603_1608Metric.step> --baseline-binary <PREVIOUSLY_DELIVERED_TOKITO_EXE> --output target/new-type-filter-capture
```

Ordinary verification runs only Tokito and reads the checked-in reference:

```powershell
python tests/kicad_type_filter_e2e.py --binary <TOKITO_CLI_EXE>
```

The focused API tests and all six offline CLI reference cases passed locally.
The full verifier includes this E2E; its fresh run passed all 39 gates at
`target/delivery/local/20261016T044433Z-ee1815c9117240c3afb9efd45ab6f6a2/verification.json`.
All recorded inputs matched before the post-run documentation pointer update. General
KiCad 3D parity, geometry equivalence, physical fit and App/AR acceptance are
not established by these tests.

## Work record

| Owner | Dependency | State | Evidence/review | Next action |
| --- | --- | --- | --- | --- |
| Main agent | Pinned native executable and real model files | Verified locally | Six captured cases; API, subprocess tests and full 39-gate verifier pass | Deliver through PR after the next CLI usability fixes |
| Read-only reviewer | Implementation and capture files | Source review passed | Quoted attribute and structural-head findings corrected; no remaining concrete blocker by inspection | Main agent runs full verification |

## Primary references

- [KiCad 10.0 GLB CLI documentation](https://docs.kicad.org/10.0/en/cli/cli.html#pcb-export-glb).
- [KiCad footprint attribute format](https://dev-docs.kicad.org/en/file-formats/sexpr-intro/index.html#_footprint_attributes).
- [Pinned 10.0.7 source-format parser](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr_parser.cpp), consulted for accepted attribute semantics only; product implementation is original Rust.
