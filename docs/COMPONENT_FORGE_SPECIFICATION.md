# Component Forge specification

Status: the explicit SMD footprint definition and `.kicad_mod` serializer are implemented as a bounded candidate workflow. IPC-7351 family calculation, through-hole/custom pads, 3D models, KLC checks and engineering qualification remain future work.

## Implemented request: `tokito.component-envelope.v1`

```json
{
  "schema": "tokito.component-envelope.v1",
  "board": { "sha256": "<64 lowercase hex characters>" },
  "component": {
    "reference": "R1",
    "uuid": "<source footprint UUID>",
    "manufacturer": "<source claim>",
    "mpn": "<source claim>",
    "package": "<source claim>"
  },
  "evidence": {
    "datasetCommit": "<40 lowercase hex characters>",
    "bomSourceSha256": "<64 lowercase hex characters>",
    "manufacturerUrl": "https://<source URL>"
  },
  "bodyMm": { "length": 3.0, "width": 3.0, "height": 0.85 }
}
```

Input is capped at 64 KiB. JSON objects have fixed required fields; duplicate and unknown keys are refused. Identity strings are nonblank, control-free, and bounded. Dimensions must be finite, positive, representable, and no greater than 100 mm. Output is a centered X/Y rectangle from Z=0 to the supplied height, serialized as a deterministic flat triangle VRML97 file. Existing output files are not overwritten.

Only the supplied board SHA-256 and the unique reference/UUID pair are checked against the loaded board. The dataset commit, BOM digest, manufacturer, MPN, package label, and HTTPS URL are syntactically bounded claims; the CLI does not fetch, rehash, or semantically verify them. These fields must not be described as verified provenance.

The dimensions are nominal envelope values, not manufacturing limits. The current request has no tolerance range, lead/terminal geometry, pin map, pin-one convention, model origin, mounting-plane offset, or board/assembly revision record. The output is not a footprint, not an exact CAD model, and not a physical-fit result.

## Implemented footprint definition: `tokito.footprint-definition.v1`

The command `component footprint generate <SPEC.json> --output <NEW.kicad_mod> [--json]` accepts a strict, maximum-64-KiB JSON definition. The example is [`examples/footprints/r0603-reference.json`](../examples/footprints/r0603-reference.json). It names the footprint, description/tags, board side, Reference and Value text, numbered SMD pads, simple line/rectangle/circle graphics, and source-evidence claims. Coordinates, dimensions, and stroke widths are in millimetres at the KiCad 0.000001 mm resolution; the third value in each `atMm` tuple is a rotation in degrees. Output uses exclusive creation; it never modifies a board or existing library file.

The Rust implementation forms a typed definition/node tree, validates it, then serializes a stable KiCad S-expression. This follows the observable separation between semantic geometry nodes and output serialization in KiCadModTree; it is an original implementation. The R0603 example records the SHA-256 of the pinned GitLab raw source bytes (`7190ac4a…`). A separate field records the locally installed KiCad 10.0.5 file hash (`03fc7902…`); the files differ in line endings and byte count, so these digests must not be interchanged. The offline E2E hashes the checked-in upstream source fixture and independently compares parsed pads and graphics. The CLI itself does not fetch or authenticate the external URL and does not validate a manufacturer drawing.

Supported pad shapes are circle, rectangle, oval and round-rectangle. Pad numbers may repeat because separate physical pads can share one electrical pin number; physical pad ordering stays as authored. Circular pads must have equal dimensions at the serializer's six-decimal-mm resolution. Library definitions must use canonical front-side layers; board-side flipping is a placement transform and a back-side library file is refused. Graphics are line, rectangle and circle on front silkscreen, fabrication or courtyard layers. Unknown JSON fields, duplicate JSON keys, unsupported shapes/layers, invalid dimensions, excess input and non-nanometre values are refused. A JSON report contains input/output hashes and keeps evidence/qualification separate.

The report always sets manufacturer land-pattern qualification, component-body generation, model qualification and board-placement verification to false. The spec supplies geometry; it does not derive or certify it.

## New `.pretty` library candidate

`component footprint library create <SPEC.json> --output <NEW.pretty> [--json]` writes the generated footprint into a newly created KiCad `.pretty` directory. It refuses an existing output directory, requires the `.pretty` suffix, and never edits another library. The report explicitly sets `registeredInFpLibTable` to false; registration, updating existing libraries, KLC validation, and multi-footprint/family generation are not implemented. The command packages the same caller-supplied explicit geometry as `component footprint generate`; it does not increase its qualification status.

`component footprint library inspect <LIBRARY.pretty> [--json]` hashes immediate `.kicad_mod` files and reports unsupported entry names without recursing. Its `footprintFileCount` is the number of files, including invalid or legacy-root files; each record carries a parse status. For each file, the shared bounded S-expression reader reports whether it parsed a `footprint` root and counts direct pad/model records; this is structural inventory, not complete KiCad semantic validation. `inventorySha256` covers the sorted footprint-file name/size/content-hash records only; it excludes ignored files and directories. Inspection does not validate footprint geometry, prove fit, or modify the library. The local directory is expected to remain stable while it is read; ordinary size changes are detected, but the operation is not an adversarial race-proof filesystem sandbox.

`component footprint library audit <LIBRARY.pretty> [--json]` adds a read-only convention audit over that inventory. It reports assets that did not parse as bounded footprint roots, duplicate parsed internal footprint names, and `.kicad_mod` stem/internal-name mismatches. The extension match is case-insensitive; the stem-to-internal-name comparison is exact. The deterministic report includes the inventory SHA-256 and ignored-entry count/names so reviewers can identify its scope. Findings are not KiCad load errors, KLC checks, semantic pad validation, geometry checks, or approval to repair files.

`component footprint library lock <LIBRARY.pretty> --output <NEW.lock.json> [--json]` records sorted file names, byte lengths and SHA-256 values for the inventoried `.kicad_mod` assets. The `.json` lock is path-independent, size-bounded, and written only to a new path outside the inspected `.pretty` directory. Public Rust lock-creation inputs are preflighted before strings are cloned. `component footprint library verify <LIBRARY.pretty> --lock <LOCK.json> [--json]` checks the lock schema and digest, then reports matching, missing, added and byte-changed footprint files; a drift report is also a nonzero command result. Unknown lock fields, malformed digest records, unsafe file names, oversized locks, duplicate/unsorted asset names and schema mismatches are refused. The lock excludes non-footprint entries and does not make the footprint library semantically valid, geometrically qualified, registered, or safe for fabrication.

## Future footprint family request

A later family-generation request must add exact package identity; source document revision and byte digest; dimensional min/nominal/max ranges where provided; terminal count/numbering and package view; typed mask/paste and drill semantics; origin, side and rotation conventions; courtyard/silkscreen/fabrication parameters; and an evidence status per value. Unsupported or absent values must produce diagnostics rather than package-family defaults. A separate family engine can calculate pads from toleranced body/terminal ranges and pinned IPC rules; the current command requires caller-supplied pad geometry.

The serializer emits to an explicit new path and never patches the input PCB or a source library. The R0603 example's generated `.kicad_mod` is parsed by an independent E2E reader and its pads, silk lines, fabrication outline, and courtyard compare exactly with the checked-in upstream KiCad `Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod` source snapshot at `51f8a59ed7d5f3bab9bb296c7b767bb033f3ba4c` (LF-byte SHA-256 `7190ac4a00125b807e54129ef0d87d87f2a658eeb74d025a7028203419b09f23`). The observed installed KiCad 10.0.5 copy is recorded separately as 2,055 CRLF bytes with SHA-256 `03fc7902b2661df01b4d828fdb6eab9eddf974e7130ed2b98892607387a50c4b`. The fixture retains the upstream attribution and license link. This validates one output's serialized geometry against one library record; it is not a comparison to the part manufacturer's recommended land pattern, does not prove KiCad command parity, and does not qualify other packages.

## Supported scope and exclusions

Implemented now: cuboid envelope candidate and explicit SMD geometry candidate generation; bounded `.pretty` inventory, read-only naming/parse audit, byte-identity lock/verify, and per-pad structural inspection. Not implemented: manufacturer-qualified or IPC-derived package families, through-hole/custom-pad geometry generation, terminals/leads, pin-one package generation, STEP/B-rep solids, photo-derived measurements, automatic part acquisition, semantic library upgrade/registration, or schematic-to-layout synthesis. See [architecture](COMPONENT_FORGE_ARCHITECTURE.md) and [qualification record](COMPONENT_FORGE_QUALIFICATION.md).

## Per-footprint pad inspection

`tokito-cli component footprint inspect <FILE.kicad_mod> [--json]` reads one bounded, regular footprint file and reports selected direct pad fields: ordinal, pad number, type, shape, `(at)` x/y and optional rotation, `(size)` width/height, optional `roundrect_rratio`, and layer names. Positions and sizes are source millimetres. The JSON includes the source basename, byte count and SHA-256; each pad has status for those reported fields and stable diagnostic codes. Unsupported types/shapes, custom-pad primitives, malformed coordinates/sizes/layers and duplicate geometry fields are surfaced as partial records. Other direct pad clauses are listed by name (atoms are counted as `uninterpreted_atom`) but their values are not decoded; `padRecordCoverage` is `selected-fields-only`, so `reported_fields_complete` does not mean the full KiCad pad record is interpreted.

This uses the existing bounded S-expression reader and does not invoke KiCad or add dependencies. The accepted pad type and shape names follow the [KiCad S-expression format reference](https://dev-docs.kicad.org/en/file-formats/sexpr-intro/) and the single-footprint-file contract is described in the [footprint library file format](https://dev-docs.kicad.org/en/file-formats/sexpr-footprint/). This command is structural inspection only: drill definitions, custom primitives, per-layer overrides, clearances, solder-mask/paste behavior, pad rendering, electrical meaning, KiCad acceptance, KLC compliance and fabrication qualification remain outside its scope.
