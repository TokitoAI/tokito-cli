# Local build and delivery locations

## Active workspace — 2026-10-11

The active ecosystem is `D:\kannan\projects\tokito ecosystem`; CLI work uses
its `tokito_cli` folder. The relocation compared SHA-256 for all 159,930 files
(66,816,607,839 bytes) and preserved the HEAD, branch and working-tree status of
18 repositories. Verification is retained at
`D:\kannan\projects\tokito-migration-evidence-20261011/`.

The original F: copy still exists: automatic approval review blocked its removal.
Use D: for new work; do not maintain two development copies. No sibling source
content was changed by the relocation. Historical absolute F: receipt paths remain
execution telemetry; resolve their suffixes beneath the relocated D: ecosystem.

Temporary isolated ESP8266 work uses `D:\TokitoCliBuilds`, and the pinned NXP
PR #21 intake uses `D:\TokitoCliDatasetAssets\nxp-intake-20261011`. These folders
are separate local artifacts, not additional source repositories. New default
CLI builds and delivery outputs remain under the active CLI's ignored `target`.

## Earlier output consolidation

On 2026-10-10 the user requested that all CLI outputs live inside this project. Existing outputs were moved on the same F: volume, preserving their contents:

| Previous directory | Current directory | Preserved inventory |
|---|---|---|
| `F:\TokitoCliBuilds` | `target/builds/` | 34,508 files; 16,014,004,500 bytes |
| `F:\TokitoCliDelivery` | `target/delivery/imported-f-drive/` | 12 files; 17,796,944 bytes |

File counts and aggregate lengths were checked before and after each move. No source, fixtures, verification receipts or binaries were deleted. The existing `target/delivery` folder was preserved, so imported historical deliveries have their own child folder.

Historical receipts and documentation retain their original paths as execution telemetry. Resolve those old paths using this table; do not rewrite historical evidence to pretend it was originally generated elsewhere.

New verification defaults to `target/local-verification/` and `target/delivery/local/`. Prototypes live under `target/prototypes/`. Dataset candidates and Git/LFS preparation live under `target/dataset-intake/`. All are ignored by Git and are not code/release uploads.

Build directories are generated caches, but they also contain source fixtures and qualification outputs. Assess individual contents and retained receipts before removing a cache; do not delete these directories wholesale as unwanted project source.
