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

The recognized asset extensions are STEP/STP, WRL/VRML, GLB and glTF. WRL and
VRML share the same bounded VRML97 adapter; the extension is not evidence that
the full VRML97 node system is supported. `resolved`
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

The original bounded parser accepts `Group`, `Transform`, scene `DEF`/`USE`,
`Shape`, planar polygon `IndexedFaceSet`/`Coordinate` data, and Material fields
with material DEF/USE. Scene references must point to an earlier supported node;
reused geometry is expanded under its new parent transform and counts against
shape, vertex and triangle budgets. Nested translation, axis-angle rotation,
nonzero scale and center are composed in source order. It preserves local
source-shape identity, material parameters, source crease angle and solid flags;
face normals are derived as flat normals. It does not smooth normals.
Unit profiles are explicit: millimetres or ordinary legacy KiCad WRL coordinates
(0.1 inch = 2.54 mm per unit). No automatic unit inference is performed.

Unknown nodes/fields, `scaleOrientation`, `Inline`, textures, supplied normals,
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
