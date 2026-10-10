# Dataset board intake and Ultimainboard sample

Updated 2026-10-10. Anandhu owns the dataset; Afan owns App/Desktop work.
The user intends to use all boards in the dataset. Readiness is per specimen,
revision, source adapter and verification gate, not a single corpus-wide flag.
Only CLI documentation was changed during this read-only dataset audit.

## Pinned Ultimainboard 2.1.4 sample

Sample commit: `c2bdf2369e13b71aaa8f13a91a23637faf339813` in
`TokitoAI/tokito-app-dataset`. The producer identifies the held revision from
photographed silkscreen. We inspected committed metadata/source relationships;
we did not independently review the photograph or materialize/hash CAD binaries.

| Item | Independently checked committed metadata | Qualification limit |
|---|---|---|
| Asset inventory | 120 unique paths exist in the complete repository tree; each has a valid SHA-256 field. | File bytes and LFS objects were not downloaded or rehashed. |
| Embedded STEP | 61 entries; declared units aggregate to 49 mm, 9 cm, 3 metres; 39 distinct declared hashes. | Actual STEP headers and geometry were not decoded by this CLI. There are 63 total STEP assets, including two separate exports. |
| Component identity | 574 unique imported footprint UUIDs and 574 unique refdes. 572 joins use refdes plus footprint name; J32/J33 use refdes plus value because importer names are truncated. | Model-stream assignment to components remains unverified; UUID/refdes joins alone do not establish model pose. |
| Photo evidence | One reviewed J22 link; 573 footprint photo mappings unverified. Component labels contain 57 seeds, 56 unverified and one reviewed J22. | J22's four contacts do not establish whole-board registration. |
| Native source | Pinned upstream Altium PcbDoc path exists; upstream Git blob ID matches recorded provenance. | Not an independent full-file SHA-256 or fabrication review. |
| Offline transport | Individual source assets and Git LFS metadata exist. | No board-specific archive is tracked in the inspected tree; this audit did not fetch or validate a separate archive. |

The native source is Altium. Its imported KiCad PCB is a derived reference,
not proof that this CLI supports Altium. The producer's provenance records
KiCad CLI 10.0.7 import with 574 footprints, 2,330 tracks, 511 vias and 190 zones.
It also records later zero-sized-pad warnings and DRC results of 293 errors,
569 warnings and one unconnected item; fabrication approval is false. We did
not rerun the conversion or DRC. Source preparation by the dataset owner does
not introduce KiCad as a runtime dependency of Tokito CLI.

The assembly STEP/GLB conversion is a geometry-only reference candidate with
physical match unverified and source colors lost. Native mil coordinates,
imported millimetres/degrees and mixed STEP units require explicit conversions.
No global photo-registration matrix was supplied. Hash or repository checks
cannot establish those geometric relationships.

Sources:
[sample README](https://github.com/TokitoAI/tokito-app-dataset/blob/c2bdf2369e13b71aaa8f13a91a23637faf339813/docs/samples/ultimainboard-2.1.4/README.md),
[manifest](https://github.com/TokitoAI/tokito-app-dataset/blob/c2bdf2369e13b71aaa8f13a91a23637faf339813/docs/samples/ultimainboard-2.1.4/manifest.json),
[assets](https://github.com/TokitoAI/tokito-app-dataset/blob/c2bdf2369e13b71aaa8f13a91a23637faf339813/docs/samples/ultimainboard-2.1.4/assets.json),
[identity map](https://github.com/TokitoAI/tokito-app-dataset/blob/c2bdf2369e13b71aaa8f13a91a23637faf339813/docs/samples/ultimainboard-2.1.4/footprint-mapping.json),
[J22 evidence link](https://github.com/TokitoAI/tokito-app-dataset/blob/c2bdf2369e13b71aaa8f13a91a23637faf339813/docs/samples/ultimainboard-2.1.4/j22-photo-source-link.json),
[producer verification](https://github.com/TokitoAI/tokito-app-dataset/blob/c2bdf2369e13b71aaa8f13a91a23637faf339813/docs/samples/ultimainboard-2.1.4/verification.json).

## Request to Anandhu's Codex

Please provide the following without marking uncertain geometry as verified:

1. A versioned offline archive, or the exact local path to fully materialized
   Git LFS assets, with archive SHA-256, unpacked byte count, per-file hashes and
   relative paths. Include native and derived layouts, provenance and every
   referenced model; exclude machine-local absolute paths from shared manifests.
2. Exact Altium extraction/import commands, tool versions, input/output hashes
   and warnings. Preserve native component IDs and the derived footprint join
   method/confidence, including the J32/J33 exceptions.
3. Whether STEP units were read from headers, the explicit source-to-millimetre
   conversion for each file, and model streams 0–60 mapped to component identity
   and transforms. Leave unknown assignments explicitly unknown.
4. Canonical board origin, axes, units, front/back and rotation order; source-to-
   canonical and per-model transforms with an asymmetric reference test. Keep
   assembly geometry match separate from structural parsing and hashes.
5. Board/photo landmarks, correspondence confidence and a reviewed registration
   transform, or a clear statement that only J22 is currently linked. Preserve
   MCXN236 and MCXN947 as different board entries.
6. A small versioned index covering every intended board: exact specimen/revision,
   source format/hash, available models, identity-map coverage, photo evidence,
   unresolved items and immutable sample commit. Agree on mobile installation
   with Afan; the current CLI v0.1 package is not an App import format.

These inputs guide subsequent adapters and integration. They do not block the
independent KiCad/WRL engine work. Next use of the Ultimainboard sample should
first verify materialized assets, then inspect the derived PCB with explicit
diagnostics; STEP parsing and native Altium support remain separate future gates.
