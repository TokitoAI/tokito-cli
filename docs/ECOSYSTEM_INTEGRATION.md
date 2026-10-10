# Tokito ecosystem integration boundaries

Desktop/catalog observations below are local snapshots. App/dataset and API observations were refreshed read-only from pinned GitHub revisions on 2026-10-10. Later friend-owned changes are not implied. See the [producer/consumer map](ECOSYSTEM_CONTRACT_MAP.md) and [native App/API bridge audit](APP_API_BRIDGE_AUDIT.md) for exact commits, source traces and deployment limitations.

## Desktop schematic studio

The inspected Rust Desktop snapshot (`master`, `e25ba7b2783f002318be07a9287a2476a1d19bff`) is schematic-first. It stores a schema-v2 schematic document and exports schematic JSON, connectivity/netlist text, SVG/PDF, BOM data, and an MCAD handoff. That snapshot does not create or route a physical PCB layout. Its 3D preview is a schematic-derived footprint preview, not a board digital twin. The friend is updating the app, so refresh these observations from a supplied current commit before implementing an integration.

Therefore the first Tokito CLI import flow must start from an existing board layout supplied by a user. The inspected Desktop snapshot cannot hand the CLI a routed `.kicad_pcb`; the friend's newer Desktop state is unverified. Later, Desktop can invoke the shared Rust engine or a local CLI job using a versioned typed request. Do not treat schematic coordinates as PCB coordinates or infer board geometry from the MCAD preview.

`tokito-schematic-core` owns schematic topology, geometry, routing, and connectivity. It is not a PCB placement/layout engine. Reuse it only when a future schematic-to-PCB workflow has a defined mapping contract.

The separate `ui` repository is an egui component library. Desktop integration may use it for UI work later; the CLI engine should have no GUI dependency.

## MCP and catalog

`tokito-mcp` is a symbol-catalog MCP service. It supports symbol search, retrieval, compatibility queries, procurement hints, generated-symbol resolution, and provenance. It does not accept PCB layouts or generate board geometry.

Online symbol lookup can be an optional preparation aid. Core import, validation, artifact generation, and phone use must remain local and functional offline. Never send Tokito Cloud credentials to the symbol MCP. A future local MCP server should adapt typed Tokito CLI engine operations rather than expand the hosted catalog's permissions.

## Tokito App

The refreshed `tokito-app` snapshot is `main` at `780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50`. Its native runtime defines a twin manifest and expects GLB/context revision, coordinates, provenance and integrity metadata. Native component selection uses prefix-plus-refdes; future CLI artifacts also need UUID metadata. The friend is actively changing the application, so bind integration to an agreed immutable revision.

The mobile bridge exposes sessions/sync and dialogue/vision/voice AI model bundles but no board-package/assembly-context/GLB intake. Native twin capabilities therefore do not establish an end-to-end mobile-shell import. The current CLI v0.1 package cannot be treated as installable until that separate intake/integrity contract exists.

The native activity can load local board-reference and twin files, but their
mobile-shell provisioning path was not found. The project-bound session caller
also omits the context revision required by the native admission gate. Afan owns
the importer/revision handoff; exact source links and the distinction between
physical Rust import and cloud UniFFI are in the bridge audit.

Initial integration should be file-based and user-mediated: CLI emits a versioned development package; Desktop can later open or produce the same package; App later imports it through a dedicated native path. Do not add direct Desktop-to-phone filesystem access or assume current Cloud authentication is an integration contract.

## Dataset and old checkout

The dataset remote is pinned at `9787b398231421656e64f40217ed7e17e9aac625`. It contains photos/board boxes, partial component seeds and CAD/model references with incomplete physical-specimen mapping. ESP32 clone is the inspected KiCad source; Ultimaker has Altium/STEP assets. MCXN236 photos must remain separate from MCXN947 CAD. The newer Ultimainboard sample at `c2bdf2369e13b71aaa8f13a91a23637faf339813` adds provenance, a derived KiCad reference and identity/unit metadata. Materialized assets, component-model assignment and global registration remain unverified; see [dataset intake](DATASET_BOARD_HANDOFF.md). The user intends all dataset boards, each subject to its own readiness gates. The old local `devboard_ar_dataset` remains an unverified placeholder scaffold, not equivalent evidence.

## Tokito API

The active API default branch is `master`, pinned at `e52c0d87f053611ed831fb74ffa186c06d28dc0e`. Existing cloud sync already includes `physical_sessions`, project/design metadata and generic artifacts; no dedicated board/GLB conversion or validated mobile package intake was found. Physical-session payloads are generic JSON, and the artifact store's 16 MiB cap does not establish its effective HTTP/deployed limit. The [bridge audit](APP_API_BRIDGE_AUDIT.md) records source handlers, privacy/schema gaps and deployment inputs. Core local CLI preparation and phone use must remain functional without this cloud path.

## Old checkout preservation

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
