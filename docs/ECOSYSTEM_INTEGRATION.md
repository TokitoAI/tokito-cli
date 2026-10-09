# Tokito ecosystem integration boundaries

These findings come from read-only inspection of the current local ecosystem checkouts. This document describes current behavior separately from future integration work.

## Desktop schematic studio

The Rust desktop app is schematic-first. It stores a schema-v2 schematic document and exports schematic JSON, connectivity/netlist text, SVG/PDF, BOM data, and an MCAD handoff. It does not currently create or route a physical PCB layout. Its 3D preview is a schematic-derived footprint preview, not a board digital twin.

Therefore the first Tokito CLI import flow must start from an existing board layout supplied by a user. The present Desktop cannot yet hand the CLI a routed `.kicad_pcb`. Later, Desktop can invoke the shared Rust engine or a local CLI job using a versioned typed request. Do not treat schematic coordinates as PCB coordinates or infer board geometry from the MCAD preview.

`tokito-schematic-core` owns schematic topology, geometry, routing, and connectivity. It is not a PCB placement/layout engine. Reuse it only when a future schematic-to-PCB workflow has a defined mapping contract.

The separate `ui` repository is an egui component library. Desktop integration may use it for UI work later; the CLI engine should have no GUI dependency.

## MCP and catalog

`tokito-mcp` is a symbol-catalog MCP service. It supports symbol search, retrieval, compatibility queries, procurement hints, generated-symbol resolution, and provenance. It does not accept PCB layouts or generate board geometry.

Online symbol lookup can be an optional preparation aid. Core import, validation, artifact generation, and phone use must remain local and functional offline. Never send Tokito Cloud credentials to the symbol MCP. A future local MCP server should adapt typed Tokito CLI engine operations rather than expand the hosted catalog's permissions.

## Tokito App

`tokito-app` is the sole active mobile app. Physical sessions are offline-first and do not require sign-in. Its native runtime already defines a digital-twin asset manifest and expects a GLB plus an assembly-context revision, explicit coordinate conventions, provenance, license, and integrity metadata.

The current app has no public document-picker/import API for a PCB/digital-twin package. Its existing model-bundle installer is for signed dialogue/vision model artifacts and must not be repurposed as a board-package format. A package produced by this CLI cannot be treated as installable until the app gains an explicit, versioned intake path and validates the signing/integrity contract.

Initial integration should be file-based and user-mediated: CLI emits a versioned development package; Desktop can later open or produce the same package; App later imports it through a dedicated native path. Do not add direct Desktop-to-phone filesystem access or assume current Cloud authentication is an integration contract.

## Dataset and old checkout

The separate `devboard_ar_dataset` repository is a scaffold: its three GLBs are boxes, its board dimensions are unverified, and its image/label/PCB/reference folders do not contain board samples. Keep this repository as a standalone data-preparation project; do not bundle it into the App or use its placeholder models as validation evidence.

The old physical checkout contains local FRDM-MCXN236 design-source archives, PDFs/datasheets, photographs, and an unreviewed capture session under ignored directories. Its ignored `build/ar-assets` and `build/reference-packages` mix generated outputs with useful board package evidence. They remain in the old checkout because moving assets would edit another project and requires a manifest/digest and licensing review.

The two untracked Python utilities `tools/expand_board_targets.py` and `tools/test_expand_board_targets.py` were copied with hash verification to the workspace recovery folder. Their behavior expands reviewed component landmarks from authored placements and self-checks existing coordinates. They are potentially useful evidence for a future offline board-reference preparation tool, not CLI import code to merge blindly.

## Sources inspected

- `tokito/docs/SCHEMATIC_EDITOR.md`, `tokito/src/handlers/designs.rs`, `tokito/native/src/app/studio/viewer3d.rs`
- `tokito-schematic-core/README.md`
- `tokito-mcp/README.md`, `tokito-mcp/crates/server/src/mcp/server.rs`
- `tokito-app/AGENTS.md`, `tokito-app/physical-runtime/docs/RUNTIME_CONTRACT.md`, `tokito-app/physical-runtime/schemas/digital-twin.asset-manifest.schema.json`
- `tokito-app/physical-runtime/docs/FRDM_MCXN236_REFERENCE_PACKAGE.md`
- `tokito-brain/topics/platform/repos.md`
- `devboard_ar_dataset/devboard_ar_dataset/README.md`, `manifest.json`, `tools/make_models.py`
