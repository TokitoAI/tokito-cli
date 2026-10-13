# REA investigation: KiCad CLI 10.0.5

This investigation supports issue #13. It establishes a narrow command-handler
and DNP-filter behavior reference, not full KiCad compatibility. Tokito's
implementation is original Rust and does not invoke KiCad, REA, Ghidra or Java.

## Toolchain and scope

The isolated development toolchain lives under
`D:\kannan\projects\tokito ecosystem\research\rea-toolchain`: Node 22.23.3,
Temurin JDK 21.0.12.1+1, Ghidra 12.1.4 and `rea-agents` 6.4.0.
The REA checkout is pinned at `d96550f2f61647ffa150597de73363fa90c4e169`;
the npm runtime records gitHead `a56f7bb668ab23fec8d33f7af353ac7ed2fe1895`.
These are different artifacts, not a claim that the checkout produced the runtime.

The portable Ghidra launcher is
`research\rea-toolchain\runtime\ghidra_12.1.4_PUBLIC\ghidraRun.bat`, relative
to the ecosystem root. It is not on the global PATH or registered with Windows
search. REA's Ghidra provider health check passed. The user-scoped Codex MCP
registration and bundled skill were installed with `rea setup --client codex
--yes`; `rea doctor` then reported the registration and skill ready. A running
Codex session may need a restart before the newly registered MCP provider is
visible. This is developer tooling only: Tokito's runtime does not depend on
REA, Ghidra, Node or Java. Java/tool variables were set only for analysis
processes. Hopper was not installed or used.

REA 6.4.0's package metadata accepts Node `^22.19.0 || ^24.11.0 || >=26.0.0`.
The isolated Node 22.23.3 toolchain satisfies that range; the machine's older
global Node 20 installation does not. The supplied `brew upgrade node` example
is macOS-specific and was not used on Windows.

| Download | Verified SHA-256 |
|---|---|
| Node Windows x64 | `2b0ff57b049cda1bbcea2240eec20467018713c1efe1f7360c2681859b90ed71` |
| Temurin JDK | `f9d6e191ab098c0d416e7d588a24420a8621cd2f4720dab2459b8b7b2d2d8b4e` |
| Ghidra | `ddac49f903da9d5bac833e5cc79395098b9c33cfd3279be5f31bd00387d2d4db` |

Tool requirements/provenance:
[REA Windows/Ghidra documentation](https://github.com/morluto/rea/blob/main/docs/windows-ghidra-p0.md),
[REA installation](https://github.com/morluto/rea/blob/main/docs/installation.md),
[official Ghidra release](https://github.com/NationalSecurityAgency/ghidra/releases/tag/Ghidra_12.1.4_build).

## Observed command boundary

The analyzed executable is `C:\Program Files\KiCad\10.0\bin\kicad-cli.exe`,
version 10.0.5, SHA-256
`393525236969434e24bc710334efe244fb285ef6596a1aea8e74353ef4db5477`.
REA/Ghidra produced approximately 1,650 procedure and 1,500 string records.
Searches found component-filter, no-dnp, no-unspecified, variant,
board-only/no-components and silkscreen switches.

The `component-filter` string at `0x1400614f8` was traced to
`FUN_140025920` and `FUN_14002c760`. The latter's derived analysis shows a
`JOB_EXPORT_PCB_3D` job being created, filter arguments transferred into job
fields and the `glb` output kind selected. This supports only the
parser-to-job boundary. It does not establish filter-pattern semantics,
variants, model resolution, mesh construction or all transforms. No pseudocode
was copied into Tokito.

Analysis of the larger `_pcbnew.dll` exceeded the 330,000 ms provider timeout.
It remains failed/incomplete; the geometry backend is not fully reverse engineered.
Full local reports are under `research\rea-analysis\kicad-cli-10.0.5` in the
ecosystem. Large reference GLBs and derived analysis are development evidence.

## Independent DNP output comparison

Installed KiCad exported the pinned NXP MCXA ESC fixture with and without
`pcb export glb --no-dnp`. The source has DNP references R2, R4 and TP13.
R2/R4 have model references and appear in the default GLB; TP13 has no model
reference. Filtering removes R2/R4, adds no named nodes and retains all other
named nodes.

| Capture | Bytes | Nodes | SHA-256 |
|---|---:|---:|---|
| Default | 563,540 | 168 | `142af08d2b712542b5eb86f7a80b753f38103e3c2562a354cd799b18489d0223` |
| `--no-dnp` | 558,908 | 164 | `30f123373cfa41bbdd53cb376a90b4a729f630d68ef291e02a52a73ed37c1312` |

The [reference receipt](../tests/reference/kicad-cli-10.0.5/glb-no-dnp/manifest.json)
pins the board, executable and captures. Initial Uno exploration was insufficient:
its DNP items were already absent from the default rendered output, so it could
not prove a filter delta. The NXP fixture resolved that gap.

To repeat the independent capture in a new directory (development tooling only):

```powershell
python tests/kicad_dnp_glb_reference.py --kicad-cli "C:\Program Files\KiCad\10.0\bin\kicad-cli.exe" --model-root "C:\Program Files\KiCad\10.0\share\kicad\3dmodels" --output target/new-kicad-dnp-reference
```

The script pins the source digest and oracle version, checks the exact named-node
delta and records current executable/output hashes. Model-library and GLB byte
equality across machines is not asserted. The reference tool is optional and
excluded from the offline delivery verifier, which tests Tokito independently.

NXP's references include STEP files; Tokito's WRL assembler does not implement
STEP. This comparison establishes DNP selection behavior, not equality of
NXP geometry or rendering. The [Uno/U3 transform reference](KICAD_GLB_REFERENCE.md)
also reran successfully; it remains separate, with documented frame differences.

## Original Rust implementation

`export glb ... --assemble-wrl --no-dnp` omits source DNP components before
model lookup/parsing. It requires a unique canonical/source UUID with one
well-formed UUID field; missing, duplicated or malformed identities fail closed.

Source totals remain unchanged. Filtered JSON/GLB coverage adds
`componentsIncluded` and `dnpFilter` with excluded component/model-reference
counts. Intentional exclusions are not unresolved geometry. Strict export
checks retained components; `wholeBoardDigitalTwinComplete` remains false.
Shell-only and explicit assignment exports reject this option.

The additive `GlbModelExportOptions`/`GlbModelExport` API preserves existing
options/report structs. Default bytes/reports are regression-tested against
the legacy unfiltered API. Tests also cover missing bodies, multiple missing
assets, malformed identities, retained selectable nodes and output refusal.

## Remaining gates

- Component-filter matching, variants, no-unspecified and other switches need
  independent command/output fixtures before implementation claims.
- STEP/general VRML, copper/silkscreen, origins and broader geometry/placement
  equivalence remain incomplete. This is not 10.0.7 parity.
- Footprint/Library Tools #24 remains separate: one 0603 candidate does not
  establish family parity or manufacturer-qualified land patterns.
- App admission, registered AR and physical-device qualification remain open.
  No phone tests were run for this filter.

Actions remain disabled. Current-source local verification and independent
review are required before branch/PR delivery.
