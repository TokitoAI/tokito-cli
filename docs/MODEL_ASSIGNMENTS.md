# Offline component model assignments

This original pure-Rust boundary supplements existing PCB data without rewriting
the dataset or claiming that a BOM contains a physical layout. A footprint is the
pad/land pattern and its graphics; a component body is a separate 3D asset. Existing
PCB footprint coordinates remain authoritative. Generating new manufacturing
footprints requires package dimensions, pad/pin numbering and a reviewed land
pattern; a part value or BOM label is insufficient.

```mermaid
flowchart LR
  BOM[Dataset BOM: candidate identity and population] --> Review[Exact part/package review]
  PCB[PCB source: UUID, refdes, pose, side, stackup] --> Template[Hash-bound assignment template]
  Review --> Manifest[Explicit local model assignments]
  Template --> Manifest
  CAD[Approved local CAD assets] --> Capture[Bounded capture and SHA-256 verification]
  Manifest --> Capture
  Capture --> Mesh[Supported WRL or bounded STEP mesh candidate]
  PCB --> Pose[Shared canonical placement transform]
  Mesh --> Pose
  Pose --> GLB[Selectable partial GLB plus per-component receipt]
  GLB --> Engineering[Independent dimensions, pin alignment and surface checks]
```

## Commands

```text
tokito-cli model assignments template board.kicad_pcb --output assignments.json
tokito-cli model assignments verify board.kicad_pcb --manifest assignments.json --project-root C:\approved-models --json
tokito-cli export glb board.kicad_pcb --model-assignments assignments.json --project-root C:\approved-models --output assigned.glb --allow-partial --json-report
```

Board arguments also accept supported Tokito packages. Templates retain every
source component with `model: null`, exact UUID/refdes/ordinal, identity issues,
unknown population and no invented transforms. The manifest is pinned to the
original board SHA-256. Explicit models require path, original-byte SHA-256,
`millimetres`, `kicad-wrl`, or `step-declared` units, scale, rotationDeg and
offsetMm. `step-declared` means the bounded STEP reader's single audited length
unit is converted to millimetres; mixed or missing units fail closed. No guessed
offsets are supplied. Paths resolve only within
approved roots, with no recursive filename matching or network downloads.

Parsing is capped at 4 MiB, 10,000 assignments and 4096 bytes per field. Duplicate
keys, conflicting identities, wrong board hashes, changed model bytes, nonfinite
transforms and zero scales are refused. Assets are bounded individually and in
aggregate; STEP input is capped at 32 MiB and the other currently accepted model
formats retain their existing format-specific cap. Missing assets and unsupported
geometry remain explicit unresolved states. This path reads supported STEP
tessellation for an explicitly assigned component, including some B-rep inputs
handled by the pinned Rust reader. It does not establish general surface/trim
coverage, closed-solid validity, source-face completeness, or physical fit, and
it does not admit glTF/GLB component parsing.

## Rendering and qualification

This is an assignment-only export mode; it cannot be combined with
`--assemble-wrl`. Authored PCB model references remain unchanged and unattempted,
and assigned bodies never inflate authored-reference emission counts. Primitive
provenance records manifest SHA-256, entry index and captured asset SHA-256;
canonical component provenance retains its real saved source node. Each rendered
component has a mesh-bearing UUID-backed `ref:<refdes>` node.

STEP-assigned meshes carry the original asset hash, STEP format and unqualified
geometry diagnostics into each primitive. The output retains STEP source colors
as a display projection, not as validated physical material properties. Every
component remains selectable by its actual PCB reference and UUID; STEP internal
occurrence names do not replace those board identities. Candidate geometry is
available only through the explicit `--allow-partial` export path.

Explicit `not-fitted` and `no-part-to-order` claims exclude bodies but remain in
the per-component report. Claims are not verified population evidence. Unassigned,
anonymous, placeholder and unresolved records remain visible. Reports are bounded
with correct totals and truncation flags. A body requires unique source identity,
supported geometry, finite canonical pose and consistent supported stackup.

Every assigned export requires `--allow-partial`, even if every identifiable
component has a body. Rendered coverage is distinct from engineering acceptance:
exact manufacturer part, package/pin-one alignment, mounting height, physical
revision, body dimensions and independent reference surfaces remain unverified.
No board shell, generic box or guessed body counts as a resolved component.

## Evidence and remaining work

The subprocess tests independently decode GLB positions, check hand-calculated
front/back poses and manifest provenance, exercise a real pinned KiCad resistor
body, and verify population exclusions and hash/board refusal paths. STEP
fixtures check declared-unit conversion, top/back placement, exact reference/UUID
selectors, a single planar OPEN_SHELL B-rep face, missing-unit refusal, and
preservation of the established WRL verification-report fields. The OPEN_SHELL
fixture proves only that this planar face can be tessellated; it is not a closed
solid. Neither fixture is evidence that any dataset component matches its
assigned model or that the imported geometry is physically complete.
Existing component-pose tests cover nonzero local transforms and signed-scale
winding. Actual dataset templates are inventory evidence only until real model
assignments and independent engineering comparisons are provided.

DigiKey enrichment is optional future desktop preparation. Its official
[ProductDetails interface](https://developer.digikey.com/products/product-information-v4/productsearch?page=0)
supports exact part lookup and its [Media interface](https://developer.digikey.com/products/product-information-v4/productsearch/media)
provides available media; this is not a universal footprint/model generation
service. Local generation must remain independent of network availability.
KiCad documents [footprint geometry](https://dev-docs.kicad.org/en/file-formats/sexpr-footprint/)
and [separate footprint-model transforms](https://dev-docs.kicad.org/en/file-formats/sexpr-intro/).

Next gates: exact dataset part/model associations, broader STEP B-rep surface
and trim coverage, topology and closed-solid qualification, pin/pad
correspondence, independent pose and body-dimension comparisons, and
whole-populated-board acceptance. See
[BOM qualification](BOM_COMPONENT_QUALIFICATION.md) and
[STEP qualification](DATASET_STEP_QUALIFICATION.md).
