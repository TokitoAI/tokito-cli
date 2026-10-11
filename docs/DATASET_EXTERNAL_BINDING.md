# External model source binding

`dataset bind-external` compares local model bytes to an explicit hash reference in source-pinned dataset provenance. It never downloads a URL, executes Git, changes a checkout or admits an engineering package.

```text
tokito-cli dataset bind-external <GIT_ROOT> --assets-root <CANDIDATE_ROOT> --request <REQUEST.json> --output <NEW_RECEIPT.json>
```

Request schema `tokito.dataset-external-request.v1` has exactly `schema`, `repository`, `revision` and `assets`. Repository is the canonical dataset URL; revision is a full lowercase commit. Each asset has exactly `boardId`, `role`, `path`, `provenancePath` and `claimPointer`. Only role `model` is supported. Paths are relative to the candidate root; provenance retains its source-relative path. Models may be renamed locally: content identity, not basename equivalence, is checked.

```json
{
  "schema": "tokito.dataset-external-request.v1",
  "repository": "https://github.com/TokitoAI/tokito-app-dataset",
  "revision": "5283bc059e30e51f4d3a81a674425936fc213214",
  "assets": [{
    "boardId": "nxp-frdm-mcxn236",
    "role": "model",
    "path": "frdm_mcxn236_ar_visible.glb",
    "provenancePath": "data/board-sources/nxp-frdm-mcxn236/ar-reference/provenance.json",
    "claimPointer": "/geometry_asset"
  }]
}
```

The profile requires strict JSON declaring `schema_version: "1.0"`. The selected JSON-pointer object must contain lowercase `sha256`, positive integer `bytes` and a passive HTTPS `url` string. Pointer escapes are checked. Unknown request fields, duplicate paths, pointer-only payloads, linked paths, changed provenance, missing files and digest/size mismatches refuse before publishing a receipt. Repository/tree/path-only locators remain unresolved; other schemas need explicit adapters.

The command first uses [immutable source binding](DATASET_GIT_INVENTORY.md) for the provenance document, then rechecks its staged hash/size before interpreting the claim. Receipt `tokito.dataset-external-binding.v1` includes that binding and each selected payload's path, digest, size, declared URL, provenance path and pointer. The pointer is caller-selected: `name`, `storage`, URL origin, package identity, model format and component associations are not certified. Role and board ID remain claims. Publisher, board revision, geometry, physical fit and App qualification flags remain false. Companion-manifest consistency is independently checked in the NXP E2E, not by this generic hash-reference adapter.

Limits: request/receipt 4 MiB; provenance 4 MiB per document and 16 MiB aggregate; external payload 256 MiB each and 2 GiB aggregate; 10,000 records. Existing Git storage/object limits also apply. These bounds are not a process-memory or concurrent-filesystem sandbox. Supply a stable snapshot. Outputs refuse overwrite; content verification uses the embedded provenance content lock, not this wrapper.

QA covers valid references/pointer escaping, altered/missing/pointer payloads, changed provenance, duplicates, fractional/oversized sizes, bad hashes, unsupported URL/version, locator-only claims and no overwrite. With dataset and twin inputs, the real NXP E2E binds its 5,250,580-byte GLB to pinned PR6 provenance, separately verifies companion manifest hash/size and checks 354 isolated selectors. `DatasetRepo` currently also requires `DatasetBomReport` in the verifier. This establishes no geometric or physical AR accuracy.
