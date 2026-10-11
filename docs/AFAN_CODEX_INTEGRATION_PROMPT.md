# Pasteable integration task for Afan's Codex

Prepared on 11 October 2026 from the actual CLI source, demo artifacts and
handover documents. Copy the task below into Afan's App/Desktop development
session. This file does not send a message or grant repository permissions.
Pin the exact CLI branch commit available when starting; newer work may still
be local and must not be described as merged or delivered without checking.

---

Continue Tokito App/Desktop integration using the existing Tokito CLI assets
and Anandhu's dataset. Build a working offline component-selection prototype,
then qualify camera registration and YOLO correspondence on a real device.
Keep reference inspection available while physical guidance remains gated.

## Read actual contracts and preserve work

Read AGENTS.md and your current repository instructions. Inspect the current
branch, working tree and active integration changes; preserve other contributors'
work. Use read-only inspection for repositories outside your authorized scope.
Use scoped subagents for independent code/contract evidence and review when
available; the main session owns integration and current-source verification.

Start with these CLI documents:

- [Integration handover](AFAN_INTEGRATION_HANDOVER.md).
- [NXP realism upgrade](NXP_REALISM_UPGRADE.md).
- [Demo provenance](DEMO_BUNDLE.md) and [demo hash inventory](../demo/bundle-manifest.json).
- [Canonical Rust API](CANONICAL_ENGINE.md).
- [Current board/AR source audit](BOARD_AR_DATASET_AUDIT.md).
- [Earlier App/API audit](APP_API_BRIDGE_AUDIT.md); its pinned App snapshot is
  historical, so recheck every reported missing bridge against your latest code.

Read your latest App Technical Bible, digital-twin/assembly-context schemas,
AR presentation/runtime contracts, mobile bridge and capability gates. Inspect
the current API/Desktop contracts if integrating them; do not invent endpoints
or copy an old audit's omissions into today's implementation.

## Available outputs and exact limits

| Asset directory | Current meaning | Unfinished qualification |
|---|---|---|
| `demo/nxp-refined/` | Refined NXP Rev C source-reference prototype: 391 semantic references, 351 selectable package bodies; U8 and J10/J11 replaced with source STEP geometry | U1 and other placeholders remain; not every DNP option is reconciled; source/specimen fit and AR pose remain unqualified |
| `demo/nxp-reference/` | Earlier baseline with 354 package selectors; useful for regression comparison | Prefer refined candidate for the corrected prototype; never combine selectors/manifests from the two assets |
| `demo/esp8266/` | Rust experimental source assembly: 34 native component identities, 62 body instances, 405,194 triangles; 48,699,248-byte GLB | Source CP2102 NodeMCU reference, not certified photographed clone; pads/drills/silkscreen, surfaces and physical pose remain incomplete/unqualified |
| `demo/esp32/` | Partial community-layout assembly: 26 reference records, 23 selectable bodies | Missing CP2102 U3 and source/photo variant reconciliation |
| `demo/ultimaker/` | Source assembly with 574 references and 310 selectable groups | Missing bodies and physical registration; group-aware import required |

NXP J7, J14 and J21 are explicitly **not fitted**: keep semantic records and
show that state, rather than reporting them as missing model failures or drawing
fabricated connector bodies. Their empty pads/drills are not yet spatially
selectable in this prototype. Thirty-seven other NXP references have no package
bodies; a semantic inventory count is not a fitted or renderable component count.
Other source DNP options still require reviewed specimen evidence.

The shipping `tokito-cli` implements a bounded physical engine, supported KiCad
PCB import, inventory/placements, partial SVG, source packages, bounded local
VRML/assigned GLB assembly, twin inspection/preparation/selection and browser
prototypes. `experiments/altium-evaluation`, `experiments/step-evaluation` and
`experiments/twin-refinement` are original Rust experimental tools. Do not claim
their candidate geometry is a universal production Altium/STEP importer or
complete KiCad CLI/footprint-library parity. KiCad issues #13 and #24 remain open.
Independent FreeCAD tooling is off-device reference QA, not a production CLI
geometry dependency. Schematics still need physical placement, routing, outline
and stackup before becoming truthful board 3D.

## Import the asset sets without losing provenance

For each selected directory, access these exact files together:

1. `active.digital-twin.glb`: actual geometry/materials/node hierarchy.
2. `active.digital-twin.json`: identity/revision, GLB hash and bytes, units/axes,
   origins/bounds and qualification flags.
3. `component-map.json`: stable source identities, readable references,
   selectors and verification/population status.
4. `component-index.json` and `prepare-report.json`: prepared coverage and
   selector/consumer compatibility; retain missing and unsupported diagnostics.
5. Evidence: NXP `refinement.json`, `refinement-e2e.json`,
   `evaluated-step-reference.json`, `native-components.json`, `U8-mesh.json`,
   `J10-mesh.json`, `J11-mesh.json`; ESP8266 `assembly-report.json`,
   `assembly-e2e-receipt.json`, evaluated STEP reports; browser acceptance and
   screenshots document their own tested artifact revision.
6. `index.html` is the standalone browser reference, not an Android importer.

Check membership and SHA-256 against `demo/bundle-manifest.json` where recorded,
and always verify each twin's exact `glbBytes`/`glbSha256` against actual bytes.
If a newly prepared directory has not yet entered the global inventory, bind
and review its complete artifact hashes before delivery. Do not consume LFS
pointers, mix asset revisions or trust file names alone.

The current refined NXP manifest declares GLB SHA-256
`49bac35c3941ebc403cf0faa40fb6fb21d31fdb104083c90eb5964014ca8b43a`
and 25,398,668 bytes. ESP8266 declares
`e929f29ff0fdc88e1b31c20e6820ff0925cff63bf51df2851276b4cf858aafc1`
and 48,699,248 bytes. Recheck these against the actual pinned artifacts; a later
regeneration changes the delivery identity.

The inventory-only v0.1 `.tokito-board` format remains distinct and
`mobileImportReady: false`. The App needs its own explicit validated installation
contract; do not activate a twin by renaming a package or reusing an AI-model
installer. Install atomically into owned offline storage, reject unsafe paths,
enforce resource/scene budgets and return the accepted context revision.

## Implement selection and coordinate handling first

Keep footprint UUID or native Altium UNIQUEID distinct from readable refdes,
detector class, photo annotation ID and GLB selector. Do not invent KiCad UUIDs
for native Altium records. Keep stable identity across picking/highlighting,
source/BOM metadata and dataset correspondence.

Load complete component groups and highlight all owned mesh descendants. In
Ultimaker, 94 components span multiple meshes; direct `ref:<refdes>` mesh lookup
alone is insufficient. Restore original materials on deselection. Missing,
ambiguous, unfitted and unsupported targets need distinct UI states.

Honor GLB `METRE` and `RIGHT_HANDED_Y_UP`. Retain source board millimetres and
declared conversion separately. NXP replacement STEP XYZ maps to `[x,z,-y]`
metres before retained parent Y rotation. USB4085's package origin is A1, not
its body centre; U8 uses the source's centred pad origin. Do not add another
centering or component rotation to already composed world geometry.

Keep whole-scene bounds separate from `boardPlaneBoundsMetres`, source substrate
node and named landmarks. A substrate axis-aligned box is not reviewed mounting
hole positions. NXP hole-based photo landmarks and outline-corner metadata must
never be silently interchanged. The 1.595 mm top-plane candidate is not a
measured physical datum.

Start with CLI inspection as an independent consumer:

```powershell
cargo build --release --offline --locked
./target/release/tokito-cli.exe help twin
./target/release/tokito-cli.exe twin inspect demo/nxp-refined/active.digital-twin.glb --component-map demo/nxp-refined/component-map.json --manifest demo/nxp-refined/active.digital-twin.json
./target/release/tokito-cli.exe twin select demo/nxp-refined/active.digital-twin.glb --component-map demo/nxp-refined/component-map.json --manifest demo/nxp-refined/active.digital-twin.json --reference U8
```

Offline builds require cached locked dependencies. For Desktop, prefer the
shared Rust engine API once its contract is accepted. Keep a versioned local
subprocess adapter where isolation is needed; preserve exit statuses, diagnostics
and partial consent. Do not duplicate physical geometry logic in App/Desktop.

## Then integrate YOLO and qualified AR

Use the latest pinned `tokito-app-dataset` main and relevant PR heads, materialize
LFS, and inspect current Navaneeth AR handoff, class maps, model hashes,
preprocessing/output schemas and evaluation evidence. Dataset inspection at
`b963a35e7828e7e06e619e62c7ddf613914165e4` found merged ESP32 cross-view unknown
packages, not verified source/component joins. A generic package class or high
confidence is not proof of a unique component or exact board revision.

Join detector observations to source identities only through reviewed compatible
board/specimen correspondence. If the mapping is absent, ambiguous or stale,
abstain from positional guidance or ask the user to disambiguate. A diode class
can name several components; it must not select an arbitrary mesh. Pins require
their own reviewed identity/orientation evidence.

Feed raw camera frames, retaining detector/crop/preview transforms and both
detector/classifier scores. Do not run the production detector on UI screenshots.
Existing source reports describe misses and misclassification; inspect those
failure cases rather than claiming detector readiness from a smoke pass.

Use the App's AR boundary: `sensor`, `upright_image`, `crop`, `preview`, `board`
and `design` frames; invertible matrices, one registration revision, monotonic
capture/expiry times and uncertainty. CameraX measured output metadata supplies
rotation/mirroring/crop-to-preview transforms. Never infer them from screen size.
Only accepted identity correspondence plus calibrated registration may create a
component target in physical AR. Manual 3D inspection remains explicitly
unregistered and grants no pose or guidance authority.

Use immutable latest-only scenes, increasing sequence and consistent revision.
On missing paths, stale observations, rejected submissions, camera/permission
loss or backgrounding, clear geometry and require registration again. Keep text
abstention accessible; do not retain the last apparently valid highlight.
Preserve `physicalGeometryVerified`, `arRegistrationVerified`, positional-guidance
and release-admission gates until independent evidence supports each operation.

## Verification, delivery and product roadmap

Run current-head unit/lint/build checks and actual offline import/selection
journeys. Test every available selector, material restoration, group ownership,
unfitted entries, missing targets, hash/revision mismatch, interruption and
resource refusal. Optimize large GLBs only while preserving names, identity,
bounds and selector coverage, and revalidate the resulting artifact/hash.
4K means desktop display resolution; it does not certify CAD or phone quality.

Run real-device acceptance for raw-camera inference, supported board views,
orientation, physical flips, crop, tracking loss, background/resume, permission
loss, stale generation/revision, alignment error, latency, memory, thermal
behavior and accessibility. Measure board-family, component and pin performance
separately on held-out data; record failures and unknowns. Source-reported device
results are not a substitute for rerunning the current asset/App revision.

Keep GitHub Actions disabled/absent as requested. Use actual local verification
and independent review before authorized PR pushes/merges; do not invent hosted
checks or test passes. Update issues/project with completed, failed, skipped and
unverified tasks. Do not publish binaries, models or unrelated changes without
the task's authorization.

After the prototype: finish missing package/terminal/pad/drill geometry and
independent pose/surface comparisons; reconcile exact physical population;
stabilize versioned intake and rollback; complete reviewed detector crosswalks
and device qualification; add Desktop/MCP adapters; design schematic-to-PCB
placement/routing; complete the agreed KiCad command and footprint-library
matrix with original Rust and independent comparisons. Optional API artifact
transport requires a supported asset-size policy: the old audited 16 MiB store
cap cannot carry the current large NXP/ESP8266 GLBs.

Return a compact report with current commits/artifact hashes, changed files,
implemented connections, actual test receipts, failures, remaining gates and
the exact next inputs needed. Do not declare the whole product complete from a
rendered board or one successful highlight.
