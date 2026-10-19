# Offline model assets and initial mesh adapter

Milestone C-06c supplies `model_resolver::ModelResolver` and `resolve models`.
Milestone C-06d supplies `model_mesh::parse_vrml_mesh` and `model inspect`.
Milestones C-06e/f now connect this limited VRML profile to GLB assembly; broad
model-format support and independent placement/consumer validation remain gated.

## Asset capture contract

`ModelResolverConfig::new(project_root)` requires an existing absolute local
directory. Explicit library/cache roots and directory-variable mappings are
optional. `${KIPRJMOD}` is built in; other `${NAME}` mappings must be supplied
and remain inside an approved root. Ambient environment/CWD searches, network
downloads, recursive searches and basename fallback are absent. KiCad's
`$(NAME)`/alias/search-order behavior is not yet supported or claimed equivalent.

Exact paths are checked under approved roots. Distinct matches are ambiguous;
traversal, UNC/device paths, alternate data streams, symlinks and reparse points
are rejected. Root spelling is conservative and component-exact on Windows too,
so case aliases can be rejected. Roots and variables are bounded to 64, path
text to 4096 bytes, and assets to 64 MiB by default (reusable API ceiling 256 MiB).
External asset bytes are captured with SHA-256; errors and shared projections
omit filesystem paths. Local API access to the actual path is explicitly
local-only.
Standard metadata/path checks detect ordinary mutation; they do not constitute
an adversarial race-proof filesystem sandbox. Library/license metadata is
caller-owned provenance, not automatically inferred from an adjacent file.

The CLI caps batches at 1024 references and 256 MiB of retained captured bytes.
`--max-model-bytes` and `--max-batch-bytes` may lower those byte limits. Every
model capture receives the remaining batch budget before a local file is read or
an embedded payload is decompressed. Local file metadata is checked first and
the read is capped at the smaller of the remaining batch bytes and per-file
limit, plus one detection byte. Embedded compressed input has its own bounded
per-model allowance; declared expanded output is checked independently against
both the per-model and remaining batch limits before decompression. This keeps a
small batch allowance from being misreported as a compressed-file size failure.
JSON glTF dependency reads use the same remaining-budget rule. Assets or
dependencies beyond the remaining budget
are reported as `batch_budget_exceeded` and are not retained; extraction refuses
that incomplete set unless `--allow-partial` is explicitly supplied with a new
`--extract-dir`. Repeated library API calls or retained byte snapshots still
need a caller-owned aggregate budget.

Each `resolve models --json` record also names its owning footprint when the
source locator can be joined: `ownerComponent` contains the source component
ordinal, source UUID, reference, identity diagnostics, and
`selectableIdentityReady`. The report totals model references whose owners
have or lack unique UUID/reference pairs, so multi-model components remain
visible. This establishes a source identity crosswalk only; it
does not set `componentSelectionReady`, validate the model geometry, or prove
that a downstream GLB/App selector works. The pinned Ultimainboard intake
currently maps all 310 model references to unique footprint identities.

The recognized asset extensions are STEP/STP, WRL/VRML, GLB and glTF. WRL and
VRML share the same bounded VRML97 adapter; the extension is not evidence that
the full VRML97 node system is supported. `resolved` means captured and
hash-identified bytes only; even malformed geometry is opaque at this stage.
Hidden, missing, malformed path, unknown variable, unsupported extension,
oversized, ambiguous and I/O cases remain distinct. A JSON inventory succeeds
with unresolved entries; invalid invocation/configuration or batch limits fail
the command. All geometry/selection/mobile readiness flags remain false.

## Embedded KiCad model payloads

`resolve models` also handles a board model whose URI is exactly
`kicad-embed://<name>`. It matches the exact name against the parsed board's
`embedded_files` records. A component-local record with data is used directly;
when it contains metadata only, the resolver finds the unique board-level
payload and checks the declared type and checksum against the component-local
metadata. A mismatch is reported as `embedded_metadata_conflict`; absent or
ambiguous payloads remain explicit failures. It then decodes KiCad's
pipe-delimited Base64 Zstandard payload with a pure-Rust decoder and verifies
the stored MMH3-128 checksum (current or legacy) or historical SHA-256 before
reporting the asset as resolved. Names are exact; the resolver does not
percent-decode them, search other boards, or create a temporary model file.
Shared JSON records `origin: embedded_board`, the embedded name, source locator,
decompressed size, and SHA-256; local filesystem paths are absent.

The per-asset output limit also bounds decompression. The compressed payload has
a separate capped allowance for Zstandard framing overhead, and trailing or
concatenated frames are rejected. Missing, ambiguous, non-model, malformed,
oversized, unsupported-extension, and checksum failures have explicit statuses.
This is payload extraction only: `STEP` remains unsupported by the production
mesh/GLB assembler. A successful embedded STEP lookup is not evidence that its
geometry, units, placement, or physical correspondence to the owning footprint
are correct. `.stpz` and
`.wrz` remain unsupported until their nested compression formats have explicit
bounded decoders; classifying their extensions alone would not make them
usable geometry inputs.

KiCad 10.0.7 serializes component metadata separately from its board-level
payload table. The repository includes regressions for that split layout and
for conflicting checksums. The MMH3-V1 implementation now models KiCad's legacy
four-byte tail padding and length accounting. Its independent WRL vector is
also accepted by the locally pinned KiCad 10.0.7 executable, which emits one
mesh without a checksum error. This verifies legacy checksum acceptance and
mesh presence, not geometric equivalence between the two exporters.

On the pinned Ultimainboard 2.1.4 generated-KiCad candidate (board SHA-256
`f763641f75470b4fb5b95391ae1ec40ab4ffe2a80bc28dafd2685901c08680b7`), the
resolver captured **310 of 310 embedded STEP references** (201,945,392 bytes)
after the legacy checksum correction. `tests/embedded_dataset_e2e.py` repeats
this as a source-hash-pinned subprocess check and requires every reference to
resolve from the board payload and the aggregate byte count to match the pinned
201,945,392-byte expectation. It also checks that owner-identity coverage is
reported consistently; this snapshot records 310 model references with unique
UUID/reference owners and zero owner-identity gaps. This is asset intake only: STEP mesh generation,
units, geometry, placement and full-board assembly remain unqualified. The
report's `geometryValidated`, `componentSelectionReady`, and `mobileImportReady`
values remain false for the payload-only workflow.

### Copy resolved source-model payloads

`resolve models --extract-dir <NEW_DIRECTORY>` can now copy the original resolved
single-file STEP/STP, WRL/VRML and GLB bytes into an offline folder under
`assets/<sha256>.<format>`. JSON glTF documents and their external resources
use an isolated content-addressed bundle folder. Identical single-file assets
are deduplicated, and each PCB model reference keeps its own manifest row with
the owning UUID and reference. `manifest.json` pins the source board SHA-256,
each payload hash and size, per-reference export status, and the overall
incompleteness flags. `payloadBytes` sums primary model and dependency bytes per
reference, while `uniqueAssetBytes`
reports the actual deduplicated file bytes written. The
destination must not already exist; the command refuses replacement. A complete
copy is required by default; `--allow-partial` is required when any model is
unresolved, a JSON glTF resource cannot be copied, or a model format is not
supported by this source exporter.

This is source export only. STEP and other model geometry is not decoded, no
footprint/model transforms are applied, dimensions or physical fit are not
verified, and the operation does not produce a component-selectable GLB or an
App-importable package. The manifest keeps `geometryDecoded`,
`geometryValidated`, `placementApplied`, `componentSelectionReady`, and
`mobileImportReady` false.

For JSON `.gltf` version 2.0, the exporter also resolves external
`buffers[*].uri` and `images[*].uri` references. Images must use exactly one of
`uri` or `bufferView`. Only data URIs and local relative paths are accepted.
Spaces in URI paths must be percent-encoded. Paths are decoded for filesystem
lookup and then checked for platform-safe components. Traversal, absolute paths,
network/file schemes, query/fragment syntax, Windows device names,
case-alias collisions, extension
payloads, and any dependency path rooted at `scene.gltf/` are refused. A
dependency whose first path segment collides with the emitted `scene.gltf`
file is also refused, including case-insensitive aliases.
Resources are read only from the same caller-approved roots as the source glTF,
subject to per-file and remaining aggregate byte limits. A dependency that would
exceed either limit is reported as incomplete before its payload is retained.
The original JSON document
is copied unchanged to `assets/gltf/<bundle-sha256>/scene.gltf`; its local
relative resources are copied below that folder using the decoded relative
paths, so the document's URI references continue to resolve. The manifest
records the dependency paths, sizes and hashes. A malformed glTF or any
unresolved resource prevents that glTF bundle from being exported; the caller
must pass `--allow-partial` to export other resolved model assets in that case.
This is core-file-closure packaging only: it does not validate the full glTF
schema or mesh semantics, extension-specific external files, geometry, units,
placement or physical fit. A document with extension usage is explicitly
incomplete until extension resources are understood and captured.

```powershell
.\target\release\tokito-cli.exe resolve models .\board.kicad_pcb --project-root . --extract-dir .\target\board-model-source --json
.\target\release\tokito-cli.exe resolve models .\board.kicad_pcb --project-root . --extract-dir .\target\board-model-source-partial --allow-partial --json
```

The pinned Ultimainboard intake checked the exported payload directory: the CLI
summary's manifest hash matched the manifest bytes; all 310 rows had unique
UUIDs and unique references; each row's declared byte length matched its file;
all 39 assets matched canonical SHA-256 names and file hashes; and the directory
contained no unreferenced files. Their 24,722,098 unique bytes matched
`uniqueAssetBytes`. The manifest reported 201,945,392 bytes summed across model
references while keeping geometry and placement unqualified. The per-run receipt
is `C:\TokitoCliFinalVerification\target\20261014T175050Z-495fdfa4f991499da829b6b37f7a2bdf\embedded-dataset-e2e-20261015c.json`.
The E2E writes 24,722,098 unique payload bytes under its temporary target; the
201,945,392-byte figure is the sum across model references before deduplication.
Keep adequate free space before supplying this board to the full verifier.

Example (PowerShell, with the matching pinned board file):

```powershell
python tests/embedded_dataset_e2e.py --binary target/release/tokito-cli.exe --board .\target\dataset-intake\257ea7e6\data\board-sources\ultimainboard-2.1.4\generated-kicad\Ultimainboard-2.1.4.imported.kicad_pcb --expected-sha256 f763641f75470b4fb5b95391ae1ec40ab4ffe2a80bc28dafd2685901c08680b7 --expected-model-count 310 --expected-captured-bytes 201945392 --expected-format step --receipt target\embedded-dataset-e2e.json
```

The full local verifier accepts the same board hash, count, byte total and format through
`-EmbeddedDatasetBoard`, `-EmbeddedDatasetBoardSha256`,
`-EmbeddedDatasetExpectedModelCount`, `-EmbeddedDatasetExpectedCapturedBytes`,
and `-EmbeddedDatasetExpectedFormat`.

The exporter uses exclusive creation, staged files and hash checks, and refuses
linked path components before writing. Its filesystem operations are path-based,
not anchored to open directory handles; use a trusted writable parent and a new
destination. It is not race-proof against a concurrent process able to replace
directories in that parent.
KiCad-source references:
[embedded file serialization](https://docs.kicad.org/doxygen/embedded__files_8cpp_source.html),
[MMH3 implementation](https://docs.kicad.org/doxygen/mmh3__hash_8h_source.html),
[KiCad hash formatting](https://docs.kicad.org/doxygen/hash__128_8h_source.html),
[KiCad 10.0.7 board parser/serializer](https://gitlab.com/kicad/code/kicad/-/raw/10.0.7/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr.cpp?ref_type=tags).

Example (PowerShell):

```powershell
.\tokito-cli.exe resolve models .\board.kicad_pcb --project-root 'F:\boards\project' --library-root 'F:\models\KiCad8' --model-var 'KICAD8_3DMODEL_DIR=F:\models\KiCad8' --json
.\tokito-cli.exe model inspect 'F:\models\KiCad8\Resistor_SMD.3dshapes\R_0603_1608Metric.wrl' --units kicad-wrl --json
```

## Mesh profile

The original bounded parser accepts `Group`, `Transform`, scene `DEF`/`USE`,
`Shape`, planar polygon `IndexedFaceSet`/`Coordinate` data, and Material fields
with material DEF/USE. Scene references must point to an earlier supported node;
reused geometry is expanded under its new parent transform and counts against
shape, vertex and triangle budgets. Nested translation, axis-angle rotation,
nonzero scale and center are composed in source order. It preserves local
source-shape identity, material parameters, source crease angle and solid flags.
When VRML supplies no explicit Normal node, it derives per-corner normals from
`creaseAngle` across two-use, oppositely oriented manifold edges. It angle-weights
the connected smooth fan and leaves non-manifold edges hard. Normals use
inverse-transpose matrices through scene and component transforms.
`Transform.scaleOrientation` follows the VRML97 matrix order
`T * C * R * SR * S * -SR * -C`.
Unit profiles are explicit: millimetres or ordinary legacy KiCad WRL coordinates
(0.1 inch = 2.54 mm per unit). No automatic unit inference is performed.

Unknown nodes/fields, `Inline`, textures, explicit supplied normals,
non-planar or degenerate polygons, duplicate fields/definitions, invalid indices,
nonfinite coordinates and exceeded byte/token/depth/shape/vertex/triangle budgets
are rejected. Planar polygons are triangulated only when the result preserves
source winding and projected area. It is a bounded VRML profile, not complete
VRML97, STEP, or STEP-export support.
The [pinned official resistor](../tests/fixtures/open-source/kicad-resistor-models/README.md)
is the positive fixture; mathematical negative cases are explicitly synthetic.

## Transform gates before assembly

Source archaeology is pinned to KiCad 10.0.7 source commit
`69cafb907ebc09e7722a2a464b1d466fc2e3608d`. These observations are behavior
references, not copied algorithms or successful renderer comparisons:

- Legacy model `at` is inches ×25.4; modern `offset` is millimetres ([parser](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr_parser.cpp#L929)).
- KiCad CLI STEP and the 3D viewer have distinct placement paths. The public Rust point API follows the pinned STEP exporter; an independent asymmetric comparison against a KiCad-produced STEP/GLB or reviewed mechanical oracle remains pending. The OpenGL viewer transform is still unqualified ([renderer](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/3d-viewer/3d_rendering/opengl/render_3d_opengl.cpp#L1076)).
- The OpenGL viewer mounts components on outer paste-layer surfaces; the CLI STEP exporter instead uses outer copper surfaces. Neither plane should be inferred from Tokito's ±thickness/2 slab without stackup evidence ([viewer adapter](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/3d-viewer/3d_canvas/board_adapter.cpp#L1039), [STEP stack walk](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/pcbnew/exporters/step/step_pcb_model.cpp#L1509), [STEP mount pose](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/pcbnew/exporters/step/step_pcb_model.cpp#L3405)).
- KiCad WRL scaling depends on top-level Transform behavior; rejecting Transform keeps this first profile explicit ([reader](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/plugins/3d/vrml/v2/vrml2_transform.cpp#L207)).
- Tokito's saved-Y-down to GLB −Z mapping gives the placed STEP-to-GLB basis negative parity. The mesh-placement helper now corrects triangle winding using that fixed reflection composed with signed model-scale parity, then recomputes flat normals. Asymmetric tests verify the implemented matrix and normal direction; comparison against an independent KiCad-generated or reviewed mechanical reference is still required before claiming viewer equivalence.

Next gates: independent asymmetric export/mechanical comparison and reviewed
per-component world bounds; verify the `ref:<refdes>` selector group through the
actual App importer; then expand the pinned model corpus. Board-shell GLB always
requires `--allow-partial`. An assembled GLB that omits any component/model also
requires that explicit flag; exact per-model statuses and bounded truncation are
reported. Complete supported model coverage still does not establish a complete
digital twin or App/AR readiness.

## KiCad CLI STEP placement: pinned behavior contract

The source baseline used here is KiCad 10.0.7 commit 69cafb907ebc09e7722a2a464b1d466fc2e3608d. The STEP exporter supplies each model's footprint position relative to the board origin, side, footprint angle, model offset, model rotation, and scale. Its placement routine derives front/back model planes from the board body plus the outer copper layers and uses a 0.05 mm clearance. It does not treat the board slab's half-thickness as the complete mount-plane definition.

For a model-space point already expressed in millimetres, the Rust point-placement API applies scale, model rotations around X/Y/Z by the negative stored angles, the model offset and selected copper-plane/clearance offset, the back-side 180-degree X rotation when applicable, the footprint Z rotation, then the board translation (x, -y). This operation order follows the pinned exporter and the documented Open CASCADE composition rule; no KiCad or Open CASCADE runtime is linked or invoked.

The point API maps a placed model point to Tokito's shell frame in GLB metres: x=(board_x-origin_x+local_x)/1000, y=local_z/1000, z=(-(board_y-origin_y)+local_y)/1000. This STEP-to-GLB basis has negative determinant. The parsed-mesh placement helper applies that point transform to a bounded triangle mesh, composes reflection parity with negative model scales, adjusts triangle winding, recomputes flat normals, and rejects invalid indices, degenerate triangles, non-finite results, or meshes over their geometry budgets. `EngineeringData::stackup` retains physical source order and exposes strict centred outer-copper planes for supported copper/core/prepreg stacks; missing, malformed, or unsupported outer material refuses. This placement path is integrated into GLB scene assembly for the accepted VRML subset. It does not parse STEP and does not claim equality with a KiCad-generated export. Asymmetric tests verify the declared source-derived contract mathematically, but independent renderer comparison remains pending.

Primary references:
- KiCad's pinned STEP exporter inputs and board-origin handling: https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/pcbnew/exporters/step/exporter_step.cpp
- KiCad's pinned STEP model composition and copper-plane placement: https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/pcbnew/exporters/step/step_pcb_model.cpp#L3716
- Open CASCADE's composition definition for Multiply: https://dev.opencascade.org/doc/refman/html/classgp___trsf.html
