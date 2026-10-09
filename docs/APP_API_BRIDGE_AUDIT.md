# App, native runtime and API bridge — 2026-10-10

## Evidence scope

Read-only GitHub inspection, with complete repository tree inventories and
targeted source/document reads for native/mobile/sync/artifact boundaries.
This does not mean every source, binary or vendor file was reviewed. No sibling
checkout was pulled or edited, no API service/device test was run, and source
availability does not establish deployment or interoperability.

The App tree inventory contained 447 file blobs at the pinned commit; the audit
focused on entry points, schemas, native bindings and their callers.

| Repository | Inspected branch / commit | Role |
|---|---|---|
| tokito-app | main / `780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50` | Unified mobile shell and embedded physical runtime; friend-owned changes continue |
| tokito-api | master / `e52c0d87f053611ed831fb74ffa186c06d28dc0e` | Rust cloud workspace, version 0.10.29; account, AI and shared-project infrastructure |
| tokito-app-dataset | main / `9787b398231421656e64f40217ed7e17e9aac625` | Separate friend-owned photo/annotation/CAD collection; geometry matching remains partial |
| tokito-cli | Current feature branch; local receipt in GOAL_TRACKER.md | Independent offline Rust physical geometry, local asset/mesh operations |

## App boundary

The mobile interface is
[`apps/mobile/modules/tokito-physical/src/index.ts`](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/apps/mobile/modules/tokito-physical/src/index.ts).
It exposes capabilities, openSession, pendingSyncSnapshots, markSyncEnqueued,
modelBundles, installModelBundle and rollbackModelBundle. Model capabilities
are dialogue, vision and voice. Those AI model installers are not board-package
installers and should not be reused as an undocumented geometry format.

The embedded `physical-runtime` includes a Rust core, Android runtime/AR code,
schemas and evidence/testing documentation. Its native twin contract binds board
and assembly-context revisions, GLB SHA-256/byte length, metre/right-handed Y-up
coordinates, origin/bounds and node-prefix naming. Native selection resolves
`componentNodePrefix + refdes` (default `ref:`). The mobile bridge above does not
expose board-package/assembly-context/GLB import, so these native capabilities
must not be described as a complete mobile-shell intake flow.

### What is wired today

| Layer | Actual source behavior | Remaining boundary |
|---|---|---|
| Expo physical bridge | `TokitoPhysicalModule.kt` creates a native session, stores project/design/context revision and launches the physical activity. | No board/context/GLB installer is exposed. AI model-bundle installation has a different contract. |
| Android physical activity | Loads `filesDir/contexts/active.board-reference.json`, then `active.digital-twin.json` and its GLB. | No producer/install path for these local files was found in the inspected mobile shell. |
| Native twin renderer | Checks context revisions and GLB integrity, then selects prefix-plus-refdes nodes. | CLI component assembly, local import and an actual phone journey are still required. |
| Embedded physical Rust core | Has assembly-context JSON import/store functions. | No corresponding C ABI/UniFFI export or mobile caller was found. Kotlin board-reference JSON and the Rust assembly-context schema are distinct contracts. |
| Cloud Rust UniFFI client | Auth, project reads, proposals, artifact downloads and versioned JSON sync. | No mobile FFI project publish or artifact-upload method was found, despite server/core support. |
| Physical-session sync | Snapshot JSON enters the mobile Rust outbox and `physical_sessions` resource. | This does not transfer GLB files or install a board reference. |

### Concrete revision mismatch for Afan

At this snapshot, `App.tsx:startBench` passes project/design IDs to `openSession`
without `contextRevision`. `PhysicalSessionContext.admitsBoardReference` requires
a non-null matching revision for project-bound sessions. Consequently these
caller arguments cannot admit a locally loaded board reference under that gate.
This is source evidence, not a reproduced phone failure. The context constructor
also requires a design ID when a revision is supplied, and a project ID when a
design ID is supplied. The local importer should return the verified revision
for a project-and-design session, and the caller should forward it when opening
that session. Project-only bench behavior needs a separate owner decision;
simply adding a revision would violate the current constructor contract. Any
change belongs to the App owner.

Sources:
[Expo Android bridge](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/apps/mobile/modules/tokito-physical/android/src/main/java/dev/tokito/physical/bridge/TokitoPhysicalModule.kt),
[session caller](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/apps/mobile/src/App.tsx#L133),
[revision gate](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/physical-runtime/app/src/main/java/dev/tokito/physical/session/PhysicalSessionStore.kt#L11),
[activity file loading](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/physical-runtime/app/src/main/java/dev/tokito/physical/MainActivity.kt),
[physical Rust import](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/physical-runtime/native/runtime-core/src/lib.rs#L1230),
[cloud FFI](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/crates/ffi/src/lib.rs#L317),
[physical snapshot serialization](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/apps/mobile/src/sync/physicalSync.ts).

Sources:
[assembly schema](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/physical-runtime/schemas/assembly.context-bundle.schema.json),
[twin schema](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/physical-runtime/schemas/digital-twin.asset-manifest.schema.json),
[native twin selection](https://github.com/TokitoAI/tokito-app/blob/780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50/physical-runtime/android/ar-runtime/src/main/kotlin/ai/tokito/physical/ar/DigitalTwinView.kt).

## API inventory and actual registered routes

`crates/api` composes HTTP routes. `crates/companion` owns shared-project/sync
handlers and durable state; its historical name remains implementation code in
the active API, not a request to develop the archived companion repository.
Auth, agent contracts/handlers, conversations, metering/billing, part/provider
services and DS-ViRe provide separate cloud capabilities. No native physical
engine or CLI library binding was found in this API workspace.

| Surface | Registered behavior |
|---|---|
| POST /v1/sync/devices | Register an owned device |
| GET /v1/sync/bootstrap | Bootstrap authorized shared resources |
| POST /v1/sync/commands | Versioned/idempotent JSON resource command |
| GET /v1/sync/events | Durable resumable SSE updates |
| /v1/companion/{devices,bootstrap,commands,events} | Migration aliases to the same sync handlers |
| GET /v1/companion/projects; PUT/GET /v1/companion/projects/:project_id | Project review snapshots |
| PUT/GET /v1/companion/projects/:project_id/artifacts/:name | Generic raw artifact bytes and metadata |
| Project proposals and decisions | Publish/list proposals and record decisions; approval is not automatic edit application |
| Project/design conversation threads and messages | Durable shared engineering conversations |
| Agent run status/events/cancel | Durable agent job lifecycle |
| Chat/responses/agent turns, parts and DS-ViRe routes | Server-owned AI/catalog/datasheet services |

Source registration: [API main](https://github.com/TokitoAI/tokito-api/blob/e52c0d87f053611ed831fb74ffa186c06d28dc0e/crates/api/src/main.rs#L96),
[conversation routes](https://github.com/TokitoAI/tokito-api/blob/e52c0d87f053611ed831fb74ffa186c06d28dc0e/crates/api/src/conversations.rs#L33),
[DS-ViRe routes](https://github.com/TokitoAI/tokito-api/blob/e52c0d87f053611ed831fb74ffa186c06d28dc0e/crates/dsvire-api/src/lib.rs#L213).

## Sync and artifact contracts

Companion handlers authenticate bearer JWTs and scope state/events to the
authenticated account. Commands carry device_id, command_id, project_id,
optional design_id, resource, base_version and JSON payload. The store verifies
registered-device ownership, retry identity and optimistic version; changed
retries/stale versions conflict. Serialized payload has a 2 MiB store limit.

Allowed resources include project, design, schematic, bom, chat, suggestions,
physical_sessions, dsvire_jobs and conversations. The last two are server-authored
and forbidden to client sync commands. Physical sessions can use project or
design scope: physical-session synchronization is already implemented.

`physical_sessions` payload is generic JSON, not a typed PCB/digital-twin schema.
The documented no-raw-camera/audio rule is not enforced by a demonstrated
field/schema allowlist in the inspected store. Treat it as a client contract
pending a concrete metadata allowlist; keep physical assets/camera streams local
unless their separate explicit transport contract permits transfer.

Artifact metadata contains name, content_type, sha256 and bytes. Store hashing is
implemented; an optional upload x-content-sha256 is checked and downloads expose
a digest header. Names reject traversal/nested paths. The store cap is 16 MiB;
the effective HTTP body limit remains unverified because the inspected raw Bytes
route composition does not visibly override the framework body-limit policy.
No board geometry, GLB semantics, stable source revision, component mapping or
digital-twin manifest is validated by generic artifact storage.

Sources: [handlers/authentication](https://github.com/TokitoAI/tokito-api/blob/e52c0d87f053611ed831fb74ffa186c06d28dc0e/crates/companion/src/lib.rs#L72),
[command schema](https://github.com/TokitoAI/tokito-api/blob/e52c0d87f053611ed831fb74ffa186c06d28dc0e/crates/companion/src/store.rs#L107),
[command validation](https://github.com/TokitoAI/tokito-api/blob/e52c0d87f053611ed831fb74ffa186c06d28dc0e/crates/companion/src/store.rs#L448),
[resource allowlist](https://github.com/TokitoAI/tokito-api/blob/e52c0d87f053611ed831fb74ffa186c06d28dc0e/crates/companion/src/store.rs#L1066),
[artifact storage](https://github.com/TokitoAI/tokito-api/blob/e52c0d87f053611ed831fb74ffa186c06d28dc0e/crates/companion/src/store.rs#L805).

The agent contract covers schematic/BOM edit operations and compact selections.
Its source says Desktop edit types are duplicated, not consumed from an extracted
shared crate ([contract](https://github.com/TokitoAI/tokito-api/blob/e52c0d87f053611ed831fb74ffa186c06d28dc0e/crates/agent-contract/src/lib.rs#L11)).
Do not equate those design/schematic snapshots with physical component placement.

## Next integration contract

Keep CLI conversion deterministic and offline. Desktop can use the same Rust
library in-process once its physical input contract exists. Reuse account,
project/design identity and versioned sync for optional metadata exchange;
generic artifact transport is a potential delivery mechanism, not a working
board import contract.

Before integrating, agree on versioned board-package/GLB manifest, immutable
source and model hashes, revision relationships, units/frame/origin, UUID/refdes
mapping, completeness policy, byte/path limits and offline installation. The
App owner must expose a dedicated local importer/bridge and test it on-device.
Any cloud delivery must confirm its effective HTTP limits and deployment version.

Questions for Afan: which API revision is deployed; which physical-session
metadata fields are permitted; who currently provisions the local context/twin
files; how the verified revision will reach a project/design `openSession` and
what project-only bench sessions should permit; what board importer
the mobile bridge will expose; and how UUID identity should coexist with current
refdes node names? These are
integration inputs, not blockers for independent CLI resolver/mesh development.
[SHARED_PROJECTS.md](https://github.com/TokitoAI/tokito-api/blob/e52c0d87f053611ed831fb74ffa186c06d28dc0e/docs/SHARED_PROJECTS.md)
explicitly separates contract implementation from production deployment.
