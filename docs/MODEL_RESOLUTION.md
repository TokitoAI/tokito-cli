# Offline model assets and initial mesh adapter

Milestone C-06c supplies `model_resolver::ModelResolver` and `resolve models`.
Milestone C-06d begins with `model_mesh::parse_vrml_mesh` and `model inspect`.
These operations preserve the current board-shell GLB behavior; component assembly remains gated.

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
Asset bytes are captured with SHA-256; errors and shared projections omit
filesystem paths. Local API access to the actual path is explicitly local-only.
Standard metadata/path checks detect ordinary mutation; they do not constitute
an adversarial race-proof filesystem sandbox. Library/license metadata is
caller-owned provenance, not automatically inferred from an adjacent file.

The CLI caps batches at 1024 references and 256 MiB of accepted captured bytes.
`--max-model-bytes` and `--max-batch-bytes` may lower those byte limits. The
aggregate check happens after a bounded read/hash, so a failing batch can
process at most one additional per-file budget before it stops. Repeated library
API calls or retained byte snapshots need a caller-owned aggregate budget.

The recognized asset extensions are STEP/STP, WRL, GLB and glTF. `resolved`
means captured file bytes only; even malformed geometry is opaque at this stage.
Hidden, missing, malformed path, unknown variable, unsupported extension,
oversized, ambiguous and I/O cases remain distinct. A JSON inventory succeeds
with unresolved entries; invalid invocation/configuration or batch limits fail
the command. All geometry/selection/mobile readiness flags remain false.

Example (PowerShell):

```powershell
.\tokito-cli.exe resolve models .\board.kicad_pcb --project-root 'F:\boards\project' --library-root 'F:\models\KiCad8' --model-var 'KICAD8_3DMODEL_DIR=F:\models\KiCad8' --json
.\tokito-cli.exe model inspect 'F:\models\KiCad8\Resistor_SMD.3dshapes\R_0603_1608Metric.wrl' --units kicad-wrl --json
```

## Mesh profile

The original bounded parser accepts flat VRML97 Shapes, triangle-only
IndexedFaceSet/Coordinate data, Material fields and material DEF/USE. It preserves
local source-shape identity, material parameters, source crease angle and solid
flags; face normals are derived as flat normals. It does not smooth normals.
Unit profiles are explicit: millimetres or ordinary legacy KiCad WRL coordinates
(0.1 inch = 2.54 mm per unit). No automatic unit inference is performed.

Unknown nodes/fields, Transform/Inline, textures, supplied normals, non-triangle
faces, duplicate fields/definitions, invalid indices, nonfinite coordinates,
degenerate triangles and exceeded byte/token/depth/shape/vertex/triangle budgets
are rejected. It is a small qualified profile, not general VRML or STEP support.
The [pinned official resistor](../tests/fixtures/open-source/kicad-resistor-models/README.md)
is the positive fixture; mathematical negative cases are explicitly synthetic.

## Transform gates before assembly

Source archaeology is pinned to KiCad 10.0.7 source commit
`69cafb907ebc09e7722a2a464b1d466fc2e3608d`. These observations are behavior
references, not copied algorithms or successful renderer comparisons:

- Legacy model `at` is inches ×25.4; modern `offset` is millimetres ([parser](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr_parser.cpp#L929)).
- Local model scale, signed XYZ rotation and offset composition, then footprint rotation/backside pose, need asymmetric numerical references ([renderer](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/3d-viewer/3d_rendering/opengl/render_3d_opengl.cpp#L1076)).
- The viewer mounts components on outer paste-layer surfaces, which cannot be equated automatically with Tokito's ±thickness/2 slab ([adapter](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/3d-viewer/3d_canvas/board_adapter.cpp#L1039)).
- KiCad WRL scaling depends on top-level Transform behavior; rejecting Transform keeps this first profile explicit ([reader](https://github.com/KiCad/kicad-source-mirror/blob/69cafb907ebc09e7722a2a464b1d466fc2e3608d/plugins/3d/vrml/v2/vrml2_transform.cpp#L207)).
- Tokito's current source Y-down to GLB −Z mapping must be checked against asymmetric component landmarks, handedness and winding before claiming viewer equivalence. The symmetric resistor alone cannot qualify orientation.

Next gates: independent asymmetric pose references and mounting-plane policy;
validated per-component world bounds; deterministic UUID/refdes-to-node mapping;
then GLB assembly and real consumer selection. Existing `--allow-partial`
remains required for board-shell exports.
