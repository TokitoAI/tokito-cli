# Tokito CLI GLB export architecture

Status: Milestone C is active. The initial implementation slice is a board-shell-only GLB. It is not a selectable component model or a complete digital twin.

## Confirmed behavior

- The CLI is a pure-Rust, offline runtime. It does not invoke KiCad or require Cloud services.
- Inputs are the currently supported KiCad PCB subset and the source-bearing Tokito package accepted by `load_board_input`.
- Mesh input comes only from a valid physical `OutlineReport`. Unsupported, open, self-crossing, ambiguous, disconnected, or otherwise invalid outlines stop mesh generation.
- Overall thickness must be present, finite, and positive. KiCad documents `(general (thickness ...))` as the overall board thickness; that authored value is the source, not an inferred default ([KiCad board file format](https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/)).
- Arc and circle tessellation has a declared maximum chord deviation of 0.01 mm. The curve approximation adds at most the already-declared 1 µm contour-join tolerance when source endpoints are joined by the topology validator.
- The physical slab is centered on its mid-plane. The Tokito GLB frame is right-handed, metres, Y-up: source `X` maps to GLB `X`, source `Y` (down) maps to `-Z`, and board height maps to `Y`; the board mid-plane is `Y=0` and its top face is `+Y`.
- GLB positions are f32. Every emitted coordinate must remain finite and within a 1 µm conversion-error budget after f32 quantization, and quantization may not collapse or invert a face. This bounds supported physical extents independently of the broader source-parser coordinate ceiling.
- The GLB v2 is self-contained: JSON and geometry bytes are embedded, with no external file paths or required extensions. Its JSON and binary chunks follow the Khronos container ordering and 4-byte alignment rules. glTF uses right-handed coordinates, +Y up, and metres ([Khronos glTF 2.0 specification](https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/Specification.adoc)).
- This board-shell-only output is always considered partial, even for a source with no footprints. The default refuses it; `--allow-partial` is the explicit opt-in. The JSON report identifies unresolved components, their source model-reference paths, counts, and any report truncation. Absolute model paths are reduced to basenames to avoid embedding workstation directory names in a shared GLB. It never inserts proxy boxes or fabricated models.
- A board-shell-only GLB does not claim component selection. UUID selectors are added only when real component meshes can be resolved and mapped to verified component identity.

## First implementation slice

1. Reuse the canonical validated contours; do not parse a second copy of Edge.Cuts.
2. Tessellate lines, three-point arcs, and complete circles under fixed per-file vertex and byte budgets.
3. Flatten the single outer contour and its classified cutouts for triangulation. The pinned pure-Rust `earcut` crate supports polygon holes; its result is still checked for valid indices, non-degenerate triangles, and area agreement before it is admitted ([earcut 0.4.11 API](https://docs.rs/earcut/0.4.11/earcut/struct.Earcut.html)).
4. Create top and bottom cap triangles and boundary walls for the outside and every cutout. Keep cap and wall normals separate at the sharp board edges.
5. Convert millimetres to metres once, apply the declared axis mapping, and calculate accessor bounds from emitted float positions.
6. Build one board-shell mesh and label its coverage in glTF metadata and the CLI report. Do not expose it as a component-selectable assembly.
7. Reparse the completed GLB with the pinned Rust glTF reader and independently check container lengths, scene/mesh structure, embedded buffer, accessors, finite coordinates, face winding/normals, and bounds before returning bytes. The first mesh is intentionally non-indexed; it carries explicit triangle vertices and normals.
8. Exercise both failure-by-default and explicit partial export using the real routed Porcupine board with its curved outside and five physical cutouts.

## Component model phase

The canonical importer currently preserves raw model references and transform fields, but it does not resolve paths, load component meshes, establish source-file provenance, or validate model transforms. KiCad board files show model references with path, position/offset, scale, and rotation fields; the presence of those fields is not proof that a file exists or that its transform has been correctly interpreted ([KiCad board file format](https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/)). The KiCad 9 PCB Editor manual documents STEP, VRML, and IDF model inputs, path variables such as `${KIPRJMOD}` and `${KICAD9_3DMODEL_DIR}`, optional model embedding, and that some standard footprints intentionally reference model files that do not exist yet ([KiCad 9 PCB Editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.pdf)).

Before component GLB nodes are added, the engine must:

- define explicit, local search roots and reject path traversal outside those roots;
- resolve supported mesh formats with recorded file hashes and source paths;
- determine and test model units, local origin, XYZ rotation order, top/bottom placement, and old/new offset semantics against primary documentation and real reference cases;
- reject unsupported, missing, hidden, or ambiguous model references with a per-component coverage report;
- require stable, unique footprint UUIDs before assigning selector IDs, and emit a deterministic UUID-to-node map;
- validate assembled component bounds and placement without changing the authored board placement.

No STEP/WRL conversion, model download, KiCad CLI invocation, or model transform inference is part of the board-shell slice. STEP, WRL, and other CAD formats remain separate adapters until independently validated. Phone import, mobile rendering, registration, AR picking, and Tokito Desktop/MCP integration remain later milestones.

The six current open-source board fixtures contain 206 external model references (120 `.step`, 19 `.stp`, and 67 `.wrl`) but no model files in the fixture corpus. They can exercise missing-model reporting, not successful component coverage. Full selectable export therefore needs a separately pinned, provenance-recorded model corpus before it can pass the complete-coverage gate. The KiCad 9 manual confirms STEP and VRML as footprint model inputs; it notes that a missing model path can be intentional, so a raw reference alone cannot count as resolved ([KiCad 9 PCB Editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.pdf)).

### Format-adapter research (candidates, not dependencies)

- `truck-stepio` 0.3.0 documents STEP writing but explicitly says STEP input is still future work. It is not currently a STEP importer ([crate status](https://docs.rs/truck-stepio/0.3.0/truck_stepio/)).
- `ruststep` 0.4.0 documents a STEP parser/toolkit, but no validated conversion from its parsed entities into complete renderable solids has been demonstrated here ([crate source](https://docs.rs/crate/ruststep/0.4.0/source/)). Treat it as a syntax/semantic research candidate, not a usable board-component mesh importer.
- `oxideav-vrml` 0.0.1 advertises a pure-Rust VRML97 reader/writer and scene conversion, but it has not been tested against the exact KiCad model corpus, unit normalization, material behavior, or resource limits. It remains an unadopted prototype candidate ([crate docs](https://docs.rs/oxideav-vrml/0.0.1/oxideav_vrml/)).
- KiCad publishes versioned official 3D model libraries; select and pin a release matching the board/library generation when building a real test corpus, rather than using a floating branch ([KiCad library downloads](https://www.kicad.org/libraries/download/), [official 3D model repository](https://gitlab.com/kicad/libraries/kicad-packages3D)).

The first successful component-model adapter should be selected only after a small pinned corpus proves clean parsing, units, transforms, geometry bounds, and independent GLB output. Supporting GLB/GLTF component models would avoid CAD conversion for models supplied in that format, but it would not resolve the STEP/WRL references found in the current KiCad fixtures.

## Test limits

Unit tests use mathematical synthetic outlines for degeneracy and transformation cases; they are not manufacturing evidence. The routed Porcupine fixture is a real source board and exercises curves and holes, but no external CAD/3D viewer is being treated as an oracle. Local tests validate the generated container, emitted geometry invariants, closed/oppositely-oriented mesh edges, and deterministic CLI journey. They do not prove KiCad 3D-viewer equivalence, fabrication fitness, Android rendering, or camera alignment.
