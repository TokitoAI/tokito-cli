# Local build and delivery locations

## Active workspace — 2026-10-12

The active ecosystem is `D:\kannan\projects\tokito ecosystem`; CLI work uses
its `tokito_cli` folder. The earlier relocation is retained as historical
evidence under `docs/workspace-maintenance/20261012/previous-20261011/` in the
ecosystem. Current maintenance receipts are under
`D:\kannan\projects\tokito ecosystem\docs\workspace-maintenance\20261012`.

The authorized F: ecosystem copy was removed after every remaining file was
SHA-256 verified: 64,333 matched D and 65 differing or missing files were copied
to `recovery/f-drive-unique-20261012T172630Z-9461d57a/` under the ecosystem.
Four verified Git history bundles preserve the dirty F repository histories.
Source, models, receipts, executables and useful local edits were retained;
only reviewed compiler-cache subtrees were discarded. Historical F: receipt
paths remain execution telemetry; use the migration path maps to locate them.

External D artifact folders are consolidated under the ecosystem's `work/`:

| Former D directory | Current ecosystem-relative directory |
| --- | --- |
| `D:\TokitoCliBuilds` | `work/cli-builds/` |
| `D:\TokitoCliDelivery` | `work/cli-delivery/` |
| `D:\TokitoCliDatasetAssets` | `work/cli-dataset-assets/` |
| `D:\cargo-target` | `work/rust-target/` |

These are local artifacts, not source repositories. The move receipt records
file hashes before and after each same-volume move. New default verification
outputs still use the active CLI's ignored `target`; explicit alternate paths
should use the centralized `work/` directories. Recovery contains F-only
differences, not a second active checkout. Do not apply them over newer D work.

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
