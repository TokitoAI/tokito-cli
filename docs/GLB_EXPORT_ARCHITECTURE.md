# Tokito CLI GLB export architecture

Status: Milestone C is active. A board-shell GLB is available, alongside an evidence-gated component-assembly path for the bounded local VRML subset. Neither output is a complete digital twin, and the App import journey is not implemented.

## Confirmed behavior

- The CLI is a pure-Rust, offline runtime. It does not invoke KiCad or require Cloud services.
- Inputs are the currently supported KiCad PCB subset and the source-bearing Tokito package accepted by `load_board_input`.
- Mesh input comes only from a valid physical `OutlineReport`. Unsupported, open, self-crossing, ambiguous, disconnected, or otherwise invalid outlines stop mesh generation.
- Overall thickness must be present, finite, and positive. KiCad documents `(general (thickness ...))` as the overall board thickness; that authored value is the source, not an inferred default ([KiCad board file format](https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/)).
- Arc and circle tessellation has a declared maximum chord deviation of 0.01 mm. The curve approximation adds at most the already-declared `1e-6 mm` (1 nm) contour-join tolerance when source endpoints are joined by the topology validator.
- The physical slab is centered on its mid-plane. The Tokito GLB frame is right-handed, metres, Y-up: source `X` maps to GLB `X`, source `Y` (down) maps to `-Z`, and board height maps to `Y`; the board mid-plane is `Y=0` and its top face is `+Y`.
- GLB positions are f32. Every emitted coordinate must remain finite and within a 1 µm conversion-error budget after f32 quantization, and quantization may not collapse or invert a face. This bounds supported physical extents independently of the broader source-parser coordinate ceiling.
- The GLB v2 is self-contained: JSON and geometry bytes are embedded, with no external file paths or required extensions. Its JSON and binary chunks follow the Khronos container ordering and 4-byte alignment rules. glTF uses right-handed coordinates, +Y up, and metres ([Khronos glTF 2.0 specification](https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/Specification.adoc)).
- A board-shell-only output is always considered partial, even for a source with no footprints. The default refuses it; `--allow-partial` is the explicit opt-in. Its JSON report identifies unresolved components and source model references, counts, and report truncation. Absolute model paths are reduced to basenames in shared GLB metadata. It never inserts proxy boxes or fabricated models.
- `export glb --assemble-wrl --project-root <absolute DIR>` may additionally emit supported local VRML model parts. Each uniquely identified component has one renderable scene node named `ref:<reference>`; that node owns one glTF mesh with a separate primitive for each imported model part. The node carries the unique footprint UUID/source ordinal, and each primitive carries the model source ordinal, source asset SHA-256, and part ordinal. This matches the current App's documented `componentNodePrefix + refdes` visibility filter (default prefix `ref:`). Subprocess tests check this node/mesh shape and emulate selecting one component; App package intake and Android selection remain unverified. Missing/duplicate identities, model lookup problems, unsupported formats, old `at` transforms, invalid stackup, and mesh/placement failures are reported as model-specific unresolved coverage.

## First implementation slice

### Component and source DNP filtering

`export glb --assemble-wrl --component-filter <PATTERNS> --no-dnp` selects
source references using bounded, case-sensitive `*`/`?` patterns and excludes
native source DNP footprints before model resolution. Either option may be used
alone. Defaults include all components; shell-only and assigned-body exports
reject these options. Missing, ambiguous or malformed DNP source UUIDs refuse
export.

`componentCount`/`modelReferences` remain total source counts for compatibility.
`selectedComponentCount`/`selectedModelReferences` and
`filteredComponentCount`/`filteredModelReferences` report the active selection.
Filtered JSON also adds `componentsIncluded` and, for native DNP filtering,
`dnpFilter` with `enabled`, `componentsExcluded` and `modelReferencesExcluded`.
Deliberate exclusions are not unresolved geometry. Strict completeness checks
retained selected models; it does not certify a complete physical digital twin.

The additive `export_glb_with_model_options` API takes `GlbModelExportOptions`
and returns `GlbModelExport`; `export_glb_with_models_filtered` combines a
`ModelAssemblyFilter` with the same export pipeline. Its `report_json()` includes
selection and DNP receipts. Existing unfiltered bytes and DNP coverage behavior
remain covered by compatibility tests.
See [REA and independent KiCad comparison](REA_KICAD_CLI_10_0_5.md).

### Shared geometry pipeline

1. Reuse the canonical validated contours; do not parse a second copy of Edge.Cuts.
2. Tessellate lines, three-point arcs, and complete circles under fixed per-file vertex and byte budgets.
3. Flatten the single outer contour and its classified cutouts for triangulation. The pinned pure-Rust `earcut` crate supports polygon holes; its result is still checked for valid indices, non-degenerate triangles, and area agreement before it is admitted ([earcut 0.4.11 API](https://docs.rs/earcut/0.4.11/earcut/struct.Earcut.html)).
4. Create top and bottom cap triangles and boundary walls for the outside and every cutout. Keep cap and wall normals separate at the sharp board edges.
5. Convert millimetres to metres once, apply the declared axis mapping, and calculate accessor bounds from emitted float positions.
6. Build one board-shell mesh and label its coverage in glTF metadata and the CLI report. The shell-only mode has no component selectors.
7. Reparse the completed GLB with the pinned Rust glTF reader and independently check container lengths, scene/mesh structure, embedded buffer, accessors, finite coordinates, face winding/normals, and bounds before returning bytes. The first mesh is intentionally non-indexed; it carries explicit triangle vertices and normals.
8. Exercise both failure-by-default and explicit partial export using the real routed Porcupine board with its curved outside and five physical cutouts.

## Implemented component model slice

The pure-Rust assembly path resolves model files only through the caller's explicit approved project/library roots and variables. It accepts a bounded VRML97 profile from `.wrl` and `.vrml` files: triangle or validated planar-polygon `IndexedFaceSet` shapes inside `Group`, `Transform`, scene `DEF`/`USE`, and Material `DEF`/`USE`, with declared KiCad legacy WRL units. Inline `Normal { vector [...] }` values support per-vertex and per-face shading; `normalIndex` is mapped to original source faces before polygon triangulation. Per-vertex indices must match each source face's corner count and separator positions; per-face indices must contain exactly one entry per source face. Source vectors are normalized only within a 1e-3 unit-length rounding tolerance. When no normal node is authored, generated normals honor `creaseAngle` across two-use, oppositely oriented manifold edges. Shared generated corners are angle-weighted; non-manifold edges remain hard. Both authored and generated normals are carried through scene and board transforms with inverse-transpose matrices, while reflected transforms reverse triangle winding so solid-material back-face culling retains the intended faces. The parsed triangles are positioned using the canonical component pose and stackup-derived outer-copper planes, converted to the same centred GLB frame as the board shell, and written as non-indexed mesh primitives with one normal per corner. One renderable node named `ref:<refdes>` owns all primitives for one component and carries the UUID/source identity; the node name is required by the inspected App visibility filter, while UUID metadata preserves the source join. This is a schema-level alignment only, not proof that App package intake or Android rendering/selection works.

The shell thickness and stackup-derived copper separation must agree within 1 nm. This prevents an assembled component from being mounted on a board body whose own physical thickness contradicts the placement plane. All other source model formats, including KiCad STEP, are retained as explicit non-emitted diagnostics. A source can reach complete *supported-model coverage* only when every source model meets these narrow gates; this still does not claim artwork, copper, manufacturing, physical-fit, optical-registration, App, or AR completeness.

The CLI subprocess E2E includes `tests/fixtures/assembly-e2e/single-resistor.kicad_pcb` with a pinned official R0603 KiCad WRL asset and generates a two-component variant. It parses the GLB container and JSON, checks buffer/accessor ranges, finite positions and normalized normals, per-mesh bounds, mesh-bearing `ref:R1`/`ref:R2` renderable nodes and UUID metadata, then emulates the App's exact-prefix selection of one node. It also verifies that a missing model is refused by default and explicit partial consent produces the exact `missing` diagnostic. The Rust API test independently parses the valid GLB. This is a bounded fixture and CLI contract proof, not actual App/device selection, Uno/U3 STEP conversion, or KiCad mesh parity.

## Further component model work

The canonical importer preserves raw model references, transform fields, and physical stackup order. Public Rust point- and mesh-placement helpers encode the reviewed KiCad STEP pose order, require stackup-derived front/back copper planes, normalize output to metres, and recompute normals after placement. The mesh helper accounts for the negative-parity STEP-to-GLB basis and signed model-scale parity when correcting triangle winding. Asymmetric synthetic tests cover placement, winding, normals, supported multi-layer stackups, and refusal paths. The external KiCad reference still has not established mesh geometry parity. KiCad board files show path, position/offset, scale and rotation fields; those fields do not establish correct world placement ([KiCad board file format](https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/)). The KiCad 9 PCB Editor manual documents STEP, VRML and IDF inputs, `${KIPRJMOD}`/versioned model-root variables, optional embedding and intentionally missing standard models ([manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.pdf)).

Remaining geometry and integration qualification:

- compare emitted model placement and mesh bounds to a reviewed independent reference, including nonzero transforms and back-side components;
- determine and test model units, local origin, XYZ rotation order, top/bottom placement, and old/new offset semantics against primary documentation and real reference cases;
- keep rejecting unsupported, missing, hidden, or ambiguous model references with per-model status codes and bounded report truncation;
- request an App-side importer/renderer test for exact `ref:<refdes>` selection; migrate to UUID selectors only with a coordinated contract update;
- validate assembled component bounds and placement without changing the authored board placement;
- validate component vertices again after f32 quantization so rounding cannot collapse or invert a face, matching the board-shell precision guard.

No model download or KiCad invocation occurs in either export. The separate WRL/VRML inspection profile operates offline with explicitly supplied units and supports a bounded Group/Transform/DEF/USE mesh subset, including the standard VRML97 `scaleOrientation` order from [ISO/IEC 14772-1:1997, Transform and IndexedFaceSet](https://www.web3d.org/documents/specifications/14772/V2.0/part1/nodesRef.html). `IndexedFaceSet` faces with three or more vertices are accepted only when planar, non-degenerate, within configured vertex/triangle budgets and triangulated with source winding and area preserved. Inline VRML97 `Normal` vectors, per-vertex/per-face `normalIndex` mapping, and generated default `creaseAngle` normals are supported; explicit normals are associated with their original source faces before polygon triangulation. Per-face index arrays contain exactly one entry per source face. Normal vectors are checked against unit length with a 1e-3 input-rounding tolerance. `Normal` DEF/USE, non-planar or malformed faces, `Inline`, textures and other unimplemented VRML nodes are rejected. This does not implement full VRML97. STEP model tessellation and STEP board export remain unimplemented. Phone import, mobile rendering, registration, AR picking and Desktop/MCP integration remain later milestones.

The six current open-source board fixtures contain 206 external model references (120 `.step`, 19 `.stp`, and 67 `.wrl`) but no model files in the fixture corpus. They can exercise missing-model reporting, not successful component coverage. Full selectable export therefore needs a separately pinned, provenance-recorded model corpus before it can pass the complete-coverage gate. The KiCad 9 manual confirms STEP and VRML as footprint model inputs; it notes that a missing model path can be intentional, so a raw reference alone cannot count as resolved ([KiCad 9 PCB Editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.pdf)).

### Format-adapter research (candidates, not dependencies)

- `truck-stepio` 0.3.0 has a published STEP shell-import/tessellation example, despite stale module text saying input is future work. Its released example does not establish unit, material, assembly-instance or complete-face coverage for this corpus. Current upstream adds assembly traversal; evaluate a pinned revision and explicit omissions rather than adopting a broad CAD-fidelity claim ([published example](https://docs.rs/crate/truck-stepio/0.3.0/source/examples/step-to-mesh.rs), [pinned assembly example](https://github.com/ricosjp/truck/blob/88ed005249e5e3a6b07f62425399435905cd3ab6/truck-stepio/examples/step-to-mesh.rs)).
- `oxideav-step` 0.0.1 provides an isolated pure-Rust reader candidate when default features are disabled. Its source handles units, occurrences, face colors and B-rep tessellation, but may skip unmeshable faces, use identity for failed occurrence transforms, and default unresolved units. It requires strict preflight, warning rejection, face-completeness accounting and independent geometry qualification before production use. Downloading or compiling the crate does not establish dataset support ([pinned source](https://github.com/OxideAV/oxideav-step/tree/171a3d8122ff115171f66bd7948194f75aecc3f7)).
- `ruststep` 0.4.0 documents a STEP parser/toolkit, but no validated conversion from its parsed entities into complete renderable solids has been demonstrated here ([crate source](https://docs.rs/crate/ruststep/0.4.0/source/)). Treat it as a syntax/semantic research candidate, not a usable board-component mesh importer.
- `oxideav-vrml` 0.0.1 advertises a pure-Rust VRML97 reader/writer and scene conversion, but it has not been tested against the exact KiCad model corpus, unit normalization, material behavior, or resource limits. It remains an unadopted prototype candidate ([crate docs](https://docs.rs/oxideav-vrml/0.0.1/oxideav_vrml/)).
- KiCad publishes versioned official 3D model libraries; select and pin a release matching the board/library generation when building a real test corpus, rather than using a floating branch ([KiCad library downloads](https://www.kicad.org/libraries/download/), [official 3D model repository](https://gitlab.com/kicad/libraries/kicad-packages3D)).

The first successful component-model adapter should be selected only after a small pinned corpus proves clean parsing, units, transforms, geometry bounds, and independent GLB output. Supporting GLB/GLTF component models would avoid CAD conversion for models supplied in that format, but it would not resolve the STEP/WRL references found in the current KiCad fixtures.

## Test limits

Unit tests use mathematical synthetic outlines for degeneracy and transformation cases; they are not manufacturing evidence. The routed Porcupine fixture is a real source board and exercises curves and holes, but no external CAD/3D viewer is being treated as an oracle. Local tests validate the generated container, emitted geometry invariants, closed/oppositely-oriented mesh edges, and deterministic CLI journey. They do not prove KiCad 3D-viewer equivalence, fabrication fitness, Android rendering, or camera alignment.
