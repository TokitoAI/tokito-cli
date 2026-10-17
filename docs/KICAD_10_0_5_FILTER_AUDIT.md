# KiCad 10.0.5 3D export filter audit

Status: syntax and bounded output probes captured from the locally installed `kicad-cli` 10.0.5 on 2026-10-13. This is a version-specific reference, not complete behavioral parity. The installed CLI was used only as a development reference; the Tokito product path remains Rust-only and does not invoke KiCad. KiCad's `pcb export vrml --help` does not expose `--component-filter`; that option appears in the installed GLB and STEP exporters.

## Captured 10.0.5 export flags

These lists were read directly from the installed executable's `--help` output on 2026-10-13. They are a syntax inventory, not a claim that Tokito implements them.

- `pcb export glb`: `--output`, repeatable `--define-var`, `--force`, `--no-unspecified`, `--no-dnp`, repeatable `--variant`, `--grid-origin`, `--drill-origin`, `--subst-models`, `--board-only`, `--cut-vias-in-body`, `--no-board-body`, `--no-components`, `--component-filter`, `--include-tracks`, `--include-pads`, `--include-zones`, `--include-inner-copper`, `--include-silkscreen`, `--include-soldermask`, `--fuse-shapes`, `--fill-all-vias`, `--no-extra-pad-thickness`, `--min-distance`, `--net-filter`, and `--user-origin`.
- `pcb export step`: the GLB selection/geometry flags plus `--no-optimize-step`.
- `pcb export vrml`: `--output`, repeatable `--define-var`, `--force`, `--no-unspecified`, `--no-dnp`, repeatable `--variant`, `--user-origin`, `--units` (`mm`, `m`, `in`, or `tenths`; help reports `in` as the default), `--models-dir`, and `--models-relative`.
- `fp export svg`: `--output`, `--layers`, repeatable `--define-var`, `--theme`, `--footprint`, `--sketch-pads-on-fab-layers`, `--hide-DNP-footprints-on-fab-layers`, `--sketch-DNP-footprints-on-fab-layers`, `--crossout-DNP-footprints-on-fab-layers`, and `--black-and-white`.

Observed 3D selection defaults: absent `--variant` selects the default variant; variants can repeat and `${VARIANT}` can distinguish their output files. `fp export svg --footprint` selects one footprint from a library; it is separate from PCB 3D component filtering.

## KiCad 10.0.5 component-filter probes

The installed executable SHA-256 was `393525236969434e24bc710334efe244fb285ef6596a1aea8e74353ef4db5477`. The pinned Uno fixture SHA-256 was `1b078b77026f4e8fd5de7cc6c7b6fff3daf454014e5e0873b7a6d0cdd77958f9`. GLBs were written to isolated temporary directories and component-reference node names were read from the output GLB JSON. These are observations for this fixture, not a complete board-independent contract. The `?` probes used ASCII references only; Tokito's one-Unicode-scalar behavior is not claimed as KiCad parity:

| Filter | Observed result |
|---|---|
| omitted or empty string | `U3`, `U4`, `J9`, `F2` component nodes remain |
| `U3` | only `U3` component node remains |
| `U*` | `U3`, `U4` remain |
| `U?` | `U3`, `U4` remain on this board; its U references each have one digit |
| `U??` | board-only output; `?` matched one character in this probe |
| `u3` or `u*` | board-only output; matching was case-sensitive in these probes |
| `U3,R1` | `U3` is emitted; KiCad warns that matched `R1` has no model asset |
| `R404`, `ZZ*` | successful board-only output with no no-match error |
| `U3,U3`, `U3,`, `,U3`, `U3,,U4`, ` U3 `, `,` | duplicate patterns are harmless, empty comma items are ignored, surrounding whitespace is trimmed; all-empty input behaves as no filter |
| `*` | all named component-reference nodes remain |

The extra edge-case probes used the same installed executable and Uno fixture; their GLBs are in `C:\Users\PC\AppData\Local\Temp\tokito-kicad-filter-edges-343wop7q`. The first wildcard probe outputs are in `C:\Users\PC\AppData\Local\Temp\tokito-kicad-filter-probe-rxa06w_e`.

## Filter surface observed

| Export family | Filter/configuration options shown by KiCad 10.0.5 help | Tokito status |
|---|---|---|
| `pcb export glb` | `--no-unspecified`, `--no-dnp`, repeated `--variant`, `--component-filter` with comma-separated refdes and wildcards; `--define-var`; model substitution; grid/drill/user origins; board/component toggles; tracks, pads, zones and layer toggles; geometry fusion/tolerance; net filter | The local Rust GLB assembly applies case-sensitive exact/`*`/`?` patterns to board references, trims surrounding whitespace, ignores empty items, and treats empty/all-empty filters as unfiltered. `--no-dnp` excludes footprints carrying KiCad's native `attr dnp` marker; a custom `DNP=yes` property does not activate this native-equivalent filter. A bounded board-wide matcher rejects pathological work before any model is resolved. Unknown/no-match selections produce a board-only GLB; nonselected model paths are not resolved. Selected missing/unsupported models still refuse by default and need explicit `--allow-partial`, a deliberate Tokito safety behavior. Coverage reports carry requested patterns and source/selected/filtered/DNP-excluded counts. Other KiCad filters, variants and board-render controls are not implemented. This behavior is not yet proven against KiCad 10.0.5 output for DNP fixtures. |
| `pcb export step` | Same component/variant filters and most geometry toggles as GLB; adds `--no-optimize-step` | No Tokito STEP board exporter or production STEP model importer. The shared GLB selector can skip a filtered-out STEP source reference before resolution; a selected STEP source is not tessellated and reports `unsupported_asset_format`. This is not STEP output filtering or a STEP artifact; both remain unimplemented. |
| `pcb export vrml` | `--no-unspecified`, `--no-dnp`, repeated `--variant`, `--define-var`, `--user-origin`, units, external-model folder, relative model paths; no `--component-filter` in installed help | No board VRML exporter. The source-model reader accepts `.wrl` and `.vrml` files with a bounded VRML97 `Group`/`Transform`/scene `DEF`/`USE` profile, including planar polygon `Shape`/`IndexedFaceSet` faces that pass planarity, area, winding and resource-budget checks. It handles translation, axis-angle rotation, nonzero scale, center and reflected winding. Component filtering is only on supported model references assembled into GLB. `Inline`, scale orientation, textures, supplied normals, non-planar faces, VRML 1.0 and other unsupported nodes remain refused. |

The selection and DNP coverage fields advance the GLB coverage report to `reportVersion` 1.2.0; Tokito package format v0.1 is unchanged. Consumers should tolerate unknown fields within a minor report version.

The KiCad 10.0 CLI manual describes `--no-dnp` as excluding 3D models for components marked Do Not Populate and `--no-unspecified` as excluding models for footprints with an Unspecified footprint type. `--variant` selects named board variants and can produce separate output paths; `--component-filter` selects only the named reference designators and supports wildcard matches. The manual is based on 10.0.7; the local executable is 10.0.5, so the manual is context and local 10.0.5 captures are the behavior baseline for this audit. These descriptions establish syntax and intent. Each behavior still needs a real board fixture, exact selected/excluded identities and an output-level comparison before Tokito claims compatibility.

### Remaining parity gaps

1. GLB reference wildcard selection has installed 10.0.5 comparisons for ASCII exact, `*`, `?`, empty, all-empty, whitespace, duplicate, case-sensitive and no-match patterns. Tokito adds a board-wide work cap; no Unicode wildcard parity is claimed. Broaden the fixtures and compare selected node sets/transforms for more boards; filtered exports with unresolved selected assets intentionally require `--allow-partial` in Tokito.
2. Tokito does not emit STEP or board VRML. The existing selector skips unselected STEP source references in the GLB workflow; this does not create STEP output filtering or a STEP artifact. No STEP export/filter parity can be claimed until an original Rust exporter and independently qualified geometry representation exist.
3. The VRML97 input reader remains a bounded subset. Scene `DEF`/`USE` and resource-neutral `Group`/`Transform` cases are accepted with shape/vertex/triangle budgets; planar polygon faces are triangulated only after geometric checks. `Inline`, scale orientation, texture/normal/color indexing fields, non-planar faces, primitives and VRML 1.0 remain unsupported. More source-corpus comparisons are required before claiming broad VRML support.
4. Native DNP filtering now uses the source `attr dnp` marker and has a synthetic E2E fixture; a pinned KiCad 10.0.5 output comparison is still required. The legacy inspection projection continues to expose its property-backed DNP field. Unspecified footprint type, named variants, board geometry switches, net filters and their option interactions still need independent output fixtures and shared selection contracts.
5. Keep STEP conversion as its own gate. STEP occurrence/mesh experiments report false admission because units, warnings, occurrence placement, face completeness and geometry accuracy are not sufficiently qualified. Filtering records cannot make an unqualified STEP reader safe.

## Relationship to footprint/library tooling

Board-export filtering and footprint-library generation are separate KiCad workflows. The pinned Library Tools audit is in [Component Forge review](COMPONENT_FORGE_OPEN_SOURCE_REVIEW.md). Its family data/IPC calculation, typed KicadModTree nodes, deterministic serializer and KLC checks inform issue #24. The Tokito [footprint-spec path](COMPONENT_FORGE_SPECIFICATION.md) serializes bounded homogeneous SMD, plated round/oval/offset through-hole, and centered circular non-plated-hole candidates; `.pretty` lock/verify adds file-level identity checks, not KiCad semantic validation or derived package land patterns.

## References

- [KiCad 10.0 CLI manual](https://docs.kicad.org/10.0/en/cli/cli.html), which identifies STEP, GLB and VRML export operations.
- [KiCad PCB S-expression format](https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/), for source board identities and DNP/model attributes.
- [Pinned KiCad Library Tools documentation](https://gitlab.com/kicad/libraries/kicad-library-tools/-/blob/6ef686944ab44b3d3d75965a66730ac65d057e63/docs/index.rst), for footprint generator/library behavior.
- [Tokito KiCad completion gates](KICAD_COMPLETION_GATES.md) and [command matrix](KICAD_CLI_PARITY_MATRIX.md).
