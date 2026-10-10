# Dataset AR handoff and CLI boundary

Read-only evidence: dataset PR #18, commit
`3de8da502749b664068846fb0347677b333bad91`,
[NAVANEED_AR_HANDOFF.md](https://github.com/TokitoAI/tokito-app-dataset/blob/3de8da502749b664068846fb0347677b333bad91/docs/NAVANEED_AR_HANDOFF.md).
This is a working branch, not a final training release. Read the actual Git LFS
payloads before consuming photos, models or CAD. Sibling repositories stay read-only.

## Required joins

The CLI must keep board variant, source commit/hash, footprint UUID/reference,
component-map identity and selectable mesh identity distinct from detector class
and photo annotation identity. Same labels or class counts do not establish a join.
Each accepted join needs compatible source/physical variant evidence. Unknown links
remain candidates; missing geometry is explicit. No automatic join is currently
qualified by this handoff.

The App owns camera calibration, pose tracking, registration and presentation.
CLI exports do not establish a board pose or authorize positional guidance.
YOLO annotation/training is separate from CAD export. The handoff reports 84 ESP32
body masks across three photos of one specimen/session; this is neither 84 unique
components nor 84 verified meshes. Incomplete labels must not be treated as ordinary
complete YOLO ground truth. Literal header aliases are point metadata, not inferred
chip-pin numbers.

## Board gates

| Board | Published evidence | Remaining acceptance gate |
|---|---|---|
| ESP32 30-pin | Updated dense photo masks; source-clone geometry | Reconcile variant and reviewed photo identities with native meshes; missing models and realistic board finish remain open |
| ESP-12E/NodeMCU | Partial annotation and source STEP corpus | Surface/trim, source body association and board placement before assembled selectable export |
| FRDM-MCXN236 | Rev C populated GLB, SHA-256 `ba9db807d2141189facb33039f57861d99a460cf595dd71bf80eb5f9cbe1268c`, 5,250,580 bytes | Materialize/hash-check asset and resolve photographed revision before joining |
| Ultimainboard 2.1.4 | Source candidates and partial reviews | Verify missing population, source identities and photo registration |

The historical `data/ar-alignment/manifest.json` may contain stale input hashes.
Regenerate and validate alignment against the exact current inputs, retaining
metric frames and semantic landmark IDs. Outline corners cannot replace mounting
hole centres. Front pans do not prove underside or flip coverage.

## Delivery acceptance

Provide Afan a pinned GLB, component map, manifest/provenance, supported and missing
selectors, variant qualification state and actual local/browser receipts. Rendering
at 3840 x 2160 is a display check; it does not qualify CAD geometry or physical fit.
Phone acceptance must measure alignment error, supported views, lifecycle loss,
rotation/flip behavior and uncertainty suppression through the App's AR contract.
No trained detector, phone alignment or whole-board physical qualification is
claimed by the current CLI foundation.
