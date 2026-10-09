# Team handoff — 2026-10-10

CLI work remains separate from friend-owned App/dataset development. The CLI's
new model operations capture assets and inspect a bounded WRL mesh profile;
world-space component assembly and phone import remain unverified.

## Dataset owner

The user plans to use all dataset boards. Please provide a versioned per-board
index with exact specimen/revision, source layout, models, units/frame, hashes,
provenance and verification state. Preserve uncertain identities and keep
photographed MCXN236 distinct from MCXN947 reference CAD. The CLI must operate
after local transfer without live dataset/network calls.

Your Ultimainboard 2.1.4 sample at `c2bdf2369e13b71aaa8f13a91a23637faf339813`
has independently checked metadata counts for 120 assets, 61 embedded STEP models
and 574 UUID/refdes mappings. Only J22 has reviewed photo mapping; mixed units,
model assignments, complete geometry and global registration still need their
own evidence. Please send the materialized offline bundle, exact extraction
pipeline, model/unit/transform mapping and per-board index described in
[dataset intake and copyable Codex request](DATASET_BOARD_HANDOFF.md).

Earlier corpus snapshot: `9787b398231421656e64f40217ed7e17e9aac625`. Photos and
board-box annotations can support perception work; partial component seeds do
not qualify detector training or manufacturing identities. ESP32 clone KiCad is
a possible import fixture, not confirmed physical equivalence. Ultimaker's
Altium/STEP assets need their own adapter and reviewed registration.

## App owner

Please define the user-selected offline board-package import and mobile bridge:
version/schema, board/context revisions, asset size/hash limits, local storage,
integrity/signing verifier ownership, unsupported-coverage UI and rollback.
Confirm native `componentNodePrefix + refdes` selection (default `ref:`) and how
canonical UUID metadata should be carried. Provide one tiny accepted package and
failure cases, then test actual phone import and selection before AR claims.

Afan's API source already supports physical_sessions sync and generic artifacts.
Please confirm the deployed API revision, a physical-session metadata allowlist,
actual HTTP artifact size limits, and the relationship between cloud artifacts
and local board-package import. Generic JSON/file storage does not validate a
digital twin. See [source bridge audit](APP_API_BRIDGE_AUDIT.md).

Inspected snapshot: `780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50`. Native twin
validation/selection exists, but the mobile bridge exposes no board-package
import. Keep this contract synchronized at immutable revisions as work proceeds.

At that commit, `startBench` omits `contextRevision` while the native gate requires
it for project-bound board references. Please confirm who provisions
`filesDir/contexts/active.board-reference.json` and the twin manifest/GLB, then
return the verified revision from local import and forward it to a project/design
`openSession`. The current constructor requires a design ID when a revision is
present; please define project-only bench behavior separately.
The Rust assembly-context importer is not exposed through the inspected mobile
bridge, and its schema is distinct from Kotlin board-reference JSON. Please
share the intended accepted schema and one end-to-end import/selection receipt.

## CLI owner

Confirm KiCad 10.0.7 as the initial behavior reference. Recommended integration
is the shared Rust library in-process for Desktop, with the standalone CLI for
automation. Next engineering gates are asymmetric model transforms, a mounting
plane/frame policy, and UUID/refdes node mapping before selectable GLB. The
historical milestone letters remain unchanged. See [model gates](MODEL_RESOLUTION.md)
and [ecosystem snapshots](ECOSYSTEM_CONTRACT_MAP.md).
