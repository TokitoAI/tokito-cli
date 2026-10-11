# Experimental STEP length-unit preflight

The pure-Rust STEP experiment now audits each declared geometric representation's
linked length unit before geometry evaluation. This is not a production STEP
adapter, a surface validator, or an assembly-placement certificate.

The audit follows representation → context → assigned unit → conversion measure
→ base unit. SI prefixes and explicit length conversion factors are supported.
Names such as `INCH` do not determine scale. Missing or duplicate units, invalid
length dimensions, wrong measure quantities, cycles and traversal limits produce
explicit refusal diagnostics. Angle units are recorded outside this audit's scope.

```powershell
cargo run --manifest-path experiments/step-evaluation/Cargo.toml --locked -- --units-only path/to/model.step
```

The experiment deliberately exits **2**, including when unit declarations resolve:
`admitted` remains false and `exportsProduced` remains empty. Declared contexts
are inspected; the audit does not infer active assembly roots. Geometry evaluation
retains its separate unqualified state.

JSON details are capped at 128 records. The full successful geometric row count
and SHA-256 cover all resolved representations, including those beyond the cap.
Digest rows, in ascending representation ID order, contain four little-endian
64-bit values: representation ID, context ID, unit ID, and the IEEE-754 bits of
metres per unit. Incomplete bounded analyses cannot claim `lengthUnitsResolved`.

## Actual local verification, 2026-10-10

- 17 experimental Rust unit tests passed.
- Five executable integration tests passed, covering prior negative-input behavior
  and the new unit-only mode, refusal flags, malformed input and Git-LFS pointers.
- Experimental Clippy with all targets and warnings denied passed.
- An independent Python linked-entity oracle compared all 17 hash-pinned ESP8266
  STEP sources against the actual Rust executable. Every full-row digest and count
  matched. The oracle parses a bounded fixture schema subset and does not evaluate
  geometric surfaces. Legacy author strings use byte-preserving decoding; unit
  names are never used to infer scale.

Local evidence: `target/github-delivery/step-length-unit-preflight-v6/receipt.json`.
Earlier failed oracle attempts remain recorded in the preceding receipt directories;
they exposed missing association/subtype handling and mixed source-string encodings
in the test oracle, not verified geometry defects.

Required next work remains face/trim completeness, evaluated surface comparisons,
occurrence/world transforms, component identity joins, and complete assembly
acceptance. No populated board has been admitted by this experiment.
