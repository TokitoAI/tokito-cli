# Experimental STEP reader qualification probe

This isolated pure-Rust experiment evaluates the exact crates.io reader `oxideav-step = 0.0.1`, with default features disabled. It is not a Tokito production adapter, does not produce GLB, and always reports `admitted: false` and exits **2**, including when the reader returns a mesh without warnings. Existing CLI STEP support is not changed.

```powershell
cargo run --locked --manifest-path experiments/step-evaluation/Cargo.toml -- "explicit/materialized/model.step"
```

One explicit input path is required. The probe refuses Git LFS pointer text and input beyond 32 MiB before parsing. It caps parser instances at 250,000, parameter depth at 48, individual strings at 1 MiB, triangles at 1,000,000 and curve samples at 16,384. Absolute chordal tolerance requested from the reader is 10 micrometres. These are resource controls, not a hard process timeout or a proven worst-case memory bound.

The output is machine-readable JSON. A reader-returned model includes source SHA-256, declared AP schema, reader-reported metres per output unit, part/root/child occurrence counts, triangles, shape/triangle colour counts, triangle face provenance entries, nonfinite positions, duration, and warning total. Up to 128 warning details of 512 Unicode characters each are retained; truncation is explicit. Any reader warning adds `reader_warnings_or_partial_geometry` to the rejection reasons. Fatal errors emit JSON and exit 2 without fabricated geometry fields.

All model-derived fields remain **unqualified reader observations**. Missing units can be defaulted by the upstream reader; units are not independently preflighted. Faces or representation items may be skipped; a nonempty mesh, provenance count, zero warnings or the requested tolerance does not establish completeness or geometric accuracy. Invisible items are omitted under the configured reader policy. Bounds union each shape's part-local coordinates, in the reader's model length units. Occurrence transforms are counted but not applied; bounds are explicitly not assembly-world bounds. Assembly placement, unit harmonisation, face completeness and engineering parity still require independent evidence.

## Dependency qualification

`Cargo.lock` pins resolved dependencies. `dependency-audit.json` records metadata package versions, native `links` declarations and target kinds. `windows-dependency-tree.txt` captures the actual Windows normal/build graph: Rust crates, no C/C++ CAD kernel, no `cc`, `cmake` or native geometry linkage. The SHA-256 dependency's CPU feature implementation uses platform intrinsics; other-target metadata includes `libc` for platform feature probing, not a CAD kernel. This is not an all-platform runtime qualification. The `registry`/mesh registry features of oxideav-step are disabled.

## Executed local verification (2026-10-10)

- `cargo fetch --manifest-path experiments/step-evaluation/Cargo.toml` resolved the pinned dependencies.
- `cargo fmt --manifest-path experiments/step-evaluation/Cargo.toml` completed.
- `cargo test --locked --manifest-path experiments/step-evaluation/Cargo.toml` passed **3 unit tests**: LFS rejection, streaming input byte cap, and warning/default non-admission reasons. The subsequent offline/locked run also passed **2 executable integration tests** for LFS-pointer and malformed STEP refusal, asserting exit 2, JSON non-admission and no exports. These subprocess journeys are reproduced by the root verifier.
- `cargo clippy --locked --manifest-path experiments/step-evaluation/Cargo.toml --all-targets -- -D warnings` passed after replacing field reassignment with an explicit options initializer.
- `cargo build --locked --manifest-path experiments/step-evaluation/Cargo.toml` passed.
- The built executable ran against explicit LFS-pointer and malformed negative files in its ignored `target/negative-e2e` directory. Both emitted parseable JSON with `admitted: false`, expected errors, and exit 2. `negative-e2e-results.json` records the actual results.
- `cargo tree --locked --manifest-path experiments/step-evaluation/Cargo.toml --target x86_64-pc-windows-msvc -e normal,build` and `cargo metadata --locked --manifest-path experiments/step-evaluation/Cargo.toml --format-version 1` were inspected for the dependency audit.

One intermediate test relink returned Windows LNK1104 (cannot open the test executable); the same exact test command reran successfully. No compile or test failure is hidden by that later result.

No materialized `tokito-app-dataset` STEP model was available during this probe's validation. **No dataset STEP success, KiCad parity, face completeness or real assembly placement is claimed.** The negative files are not target-board fixtures. Next qualification must use pinned, materialized dataset sources and independently established units, requested source-face inventory, transform composition and reference geometry comparisons before any production admission decision.
