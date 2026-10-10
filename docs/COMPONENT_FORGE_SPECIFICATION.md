# Component Forge specification

Status: the envelope request schema below is implemented. Parametric PCB footprint schema and `.kicad_mod` output are future work.

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

## Future footprint request

A later `tokito.footprint.generate` request must be a new explicit, versioned schema with an exact package identity; source document revision and byte digest; dimensional min/nominal/max ranges where provided; terminal count/numbering and package view; pad dimensions and coordinates; mask/paste and drill semantics; origin, side, and rotation conventions; courtyard/silkscreen/fabrication parameters; and an evidence status per value. Unsupported or absent values must produce diagnostics rather than package-family defaults.

The serializer must emit to an explicit new path and never patch the input PCB or a source library. Geometry construction should be independent of the KiCad text serializer so that positions, dimensions, numbering, clearances, and bounds can be independently tested before writing `.kicad_mod` syntax. A generated footprint is a candidate until its output is parsed independently and compared with the exact source land pattern.

## Supported scope and exclusions

Implemented now: cuboid envelope generation only. Not implemented: footprint audit/generation/verification, package family rules, terminals, leads, pin-one geometry, STEP/B-rep solids, photo-derived measurements, automatic part acquisition, or schematic-to-layout synthesis. See [architecture](COMPONENT_FORGE_ARCHITECTURE.md) and [qualification record](COMPONENT_FORGE_QUALIFICATION.md).
