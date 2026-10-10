# ESP8266 source assembly recovery

The current ESP8266 inspection work uses the hash-pinned original NodeMCU
Altium PCB and the dataset's extracted STEP files. No shell bodies or guessed
part names replace engineering geometry.

An original Rust development experiment under `experiments/altium-evaluation`
reads the CFB container with pinned pure-Rust `cfb=0.14.0`. It inventories streams
and inspects bounded records in the observed source profile. This is separate
from the shipping CLI and never invokes KiCad or FreeCAD. Container API evidence:
[cfb 0.14.0](https://docs.rs/crate/cfb/0.14.0).

Actual source SHA-256:
`007d603df0effe46bc254c978def4d3225a9c5123803cd115ad99c087a26ea74`.

## Verified observations

- `/Components6/Data`: 34 framed property records exactly match the dataset's
  independently published `structured-source/components.json`.
- `/Models/Data`: 17 framed model records, with 13 unique model IDs.
- `/ComponentBodies6/Data`: 62 framed body records. The observed component-index
  candidates cover all 34 components; each header has 15 bodies.
- Each body's explicit model ID resolves to source model records. Three duplicate
  ID groups account for four additional records: ordinals `[2,3]`, `[5,6,7]`, and
  `[12,13]`. Their extracted STEP bytes match within each group, but their source
  rotation/offset properties differ. Never choose the last record's placement
  metadata implicitly.
- All 17 embedded zlib model streams were decoded directly in bounded pure Rust
  and match the extracted STEP SHA-256 hashes and byte counts exactly. Truncated,
  corrupted, over-limit and trailing compressed data are refused.
- Eight Rust tests, Clippy with warnings denied, and the actual dataset subprocess
  journey passed. Receipt: `target/github-delivery/esp8266-altium-e2e-v2/receipt.json`.

## Still unverified

The body component-index field is an observed profile candidate, not independently
certified semantics. Embedded stream ordinal-to-STEP byte equivalence is verified;
the model-to-component relationship and body trailing geometry remain unverified. Rotation order,
local/board offsets, elevation, handedness, underside behavior and world placement
remain open. STEP surface and trim completeness also remain open.

The experiment exits 2 with no exports and `admitted=false`. Matching source
records does not establish a finished assembled 3D board or physical AR alignment.

Next: establish body-record association semantics
against an independent imported reference, then combine qualified STEP geometry
with verified transforms. Preserve multiple bodies under each component's identity
so selecting J1 or J2 can highlight all fifteen body occurrences together.

## STEP occurrence-world comparison

The bounded pure-Rust probe now composes parent/child occurrence transforms,
preserves repeated occurrences and applies the declared length scale once.
Depth, occurrence, triangle and vertex-work caps refuse oversized traversal;
invalid matrices, indices, cycles and nonfinite coordinates also refuse. The unit
audit retains a complete distinct-scale summary even when detailed rows are capped.

`tests/step_occurrence_world_e2e.py` binds source hashes and independent FreeCAD
bounds for `0.step`, `14.step` and `16.step`. Actual v3 receipt:
`target/prototypes/esp8266-occurrence-world-e2e-v3/receipt.json`.
Maximum bounds deltas are approximately 0, 0 and 0.049816 mm respectively, within
the declared 0.05 mm comparison tolerance. Tightening the probe tessellation from
0.01 to 0.001 mm did not resolve the third model's Z discrepancy. Its cause remains
open; a passing tolerance comparison does not certify surfaces, trims or board pose.
The probe still exits 2, emits no exports and retains false qualification flags.

Source body rotations/offsets need independent interpretation: adding component
rotation may double-rotate already absolute body metadata. Bottom header bodies,
mil-tagged body fields and raw model-table offsets require explicit verified
semantics. No assembled ESP8266 board has been admitted by this step.
