# STEP face presence gate

This development-only gate advances issue #19. It does not admit a STEP model,
produce a GLB, or establish full KiCad export parity. Production remains Rust-only
and independent of a KiCad runtime.

The original Rust audit walks shell/face membership from each emitted shape's
source item. Its supported profile includes manifold solid BReps, BReps with
voids, closed/open shells, oriented closed shells, oriented faces and face surfaces.
It includes cavity shells, refuses malformed entity arities, duplicate shell/set
references, dangling membership references, cycles and unsupported profiles.
The audit has a shared 100,000-visit budget and a depth limit of 32.

Every emitted shape is compared despite the 128-shape display cap. Missing,
unexpected or unassigned face IDs and mismatched triangle/provenance counts fail
the presence comparison. Expected IDs have a SHA-256 over ascending unique
little-endian u64 IDs. Face bounds and surface references are not evaluated by
this gate; `sourceTraversalComplete` means shell/face membership traversal only.
`sourceFacePresenceMatches` never means accurate tessellation, a valid solid,
correct units, placement, or coverage of source items omitted by the reader.
All geometry, solid and pose qualification fields remain false.

## Actual official-library comparison

Source: KiCad Packages3D commit
`a0244fe3442823dbb052ebc4820b4c2951e1742c` (9.0.9).
The CLI-owned intake manifest contains exact URLs and SHA-256 hashes; automatic
replacement of the PCB's existing model references remains forbidden.

| Asset | Independent declared face inventory | Emitted face IDs | Mesh triangles | Admission |
|---|---:|---:|---:|---|
| ESP32-WROOM-32 | 751 | 751 | 9,672 | Refused |
| USB Micro-B Molex 47346-0001 | 484 | 484 | 10,166 | Refused |
| SW SPST B3U-1000P | 81 | 81 | 2,302 | Refused |

`tests/step_face_coverage_e2e.py` independently inventories explicit face IDs in
these three hash-pinned, single-solid text fixtures and compares full ID digests
against the actual Rust probe. It does not reuse the Rust traversal. All three
comparisons passed on 2026-10-10; no missing or unexpected face IDs were found.
This fixture-specific text inventory is not a general STEP parser.

```powershell
python tests/step_face_coverage_e2e.py --probe path/to/tokito-step-evaluation.exe --asset-root target/dataset-intake/kicad9-modern-step-a0244fe3
```

The updated experiment passed 21 Rust unit tests, five executable integration
tests, Clippy with warnings denied and a build. Initial new tests failed because
their synthetic STEP header was malformed; correcting the fixture header resolved
that failure. Independent review also identified missing arity/duplicate checks;
both were implemented with negative regression tests.

Evidence is under `target/github-delivery/esp32-modern-step-probes/`. The previous
full 29-gate delivery receipt remains evidence for the earlier material correction,
not these subsequent experiment/doc changes. No new push or hosted CI run occurred.

## Remaining gates

- USB unit preflight currently refuses 2,888 two-dimensional parametric contexts
  without assigned length units. Its two three-dimensional representation rows
  resolve to millimetres. Investigate parametric-context semantics explicitly;
  do not infer a scale or silently ignore them.
- Compare evaluated surfaces, trim boundaries, dimensions and cavities against
  an independent CAD reference. Face membership alone cannot prove geometry.
- Resolve source-item omission, visibility policy and mixed-unit representation
  handling before claiming complete engineering geometry.
- Verify occurrence/world transforms and component-to-model assignments before
  assembling these replacement candidates into an ESP32 board.
- Rerun required current-source delivery verification before any code push.
