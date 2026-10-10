# Local build and delivery locations

On 2026-10-10 the user requested that all CLI outputs live inside this project. Existing outputs were moved on the same F: volume, preserving their contents:

| Previous directory | Current directory | Preserved inventory |
|---|---|---|
| `F:\TokitoCliBuilds` | `target/builds/` | 34,508 files; 16,014,004,500 bytes |
| `F:\TokitoCliDelivery` | `target/delivery/imported-f-drive/` | 12 files; 17,796,944 bytes |

File counts and aggregate lengths were checked before and after each move. No source, fixtures, verification receipts or binaries were deleted. The existing `target/delivery` folder was preserved, so imported historical deliveries have their own child folder.

Historical receipts and documentation retain their original paths as execution telemetry. Resolve those old paths using this table; do not rewrite historical evidence to pretend it was originally generated elsewhere.

New verification defaults to `target/local-verification/` and `target/delivery/local/`. Prototypes live under `target/prototypes/`. Dataset candidates and Git/LFS preparation live under `target/dataset-intake/`. All are ignored by Git and are not code/release uploads.

Build directories are generated caches, but they also contain source fixtures and qualification outputs. Assess individual contents and retained receipts before removing a cache; do not delete these directories wholesale as unwanted project source.
