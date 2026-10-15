# KiCad 10.0.7 PCB 3D export command surface

Status: version-pinned syntax and option-surface capture. This is research
evidence, not a claim of behavioral or geometry parity.

## Reference and reproduction

The captured executable is KiCad CLI 10.0.7 at
`D:/TokitoKiCadRef107/bin/kicad-cli.exe`, SHA-256
`cbd8197dd1f46b9a956f3d7973edc9a6a97960298731448ae9142dffe47abc7a`.
The reproducible help captures are checked in under
[`docs/evidence`](evidence/kicad-3d-cli-help-10.0.7.json):

- [`pcb export step --help`](evidence/kicad-pcb-export-step-help-10.0.7.txt)
  (3,468 bytes; SHA-256
  `750911020c9b5857e66066a921c68a2cc882ee0b073aaabd357220dc9b703bf9`).
- [`pcb export vrml --help`](evidence/kicad-pcb-export-vrml-help-10.0.7.txt)
  (1,788 bytes; SHA-256
  `684077aa4816530859e676be2f3dedddcf1579058fbc3692e30c09fb2babc0e9`).

Capture date: 2026-10-14 UTC. The executable and capture hashes are also
recorded in the JSON manifest. The command output is the primary evidence for
the option names and defaults below. The [official KiCad 10.0 CLI manual](https://docs.kicad.org/10.0/en/cli/cli.html)
is the user-facing reference.

## STEP board exporter

The native command is `pcb export step`. It accepts project-variable overrides,
an output path and overwrite flag, model-population filters, one or more board
variants, origin choices, component filters, conductor and board-artwork
switches, and STEP tessellation/optimization controls.

| Concern | Captured KiCad options | Captured default or behavior description |
|---|---|---|
| Output and variables | `--output`, `--force`, repeatable `--define-var KEY=VALUE` | The help output lists an empty-string output default; implicit output-path behavior is not independently measured. Overwrite is opt-in; variables may repeat. |
| Model population | `--no-unspecified`, `--no-dnp`, repeatable `--variant` | With no variant, the default variant is exported. Multiple variants support `${VARIANT}` in output paths. |
| Origin | `--grid-origin`, `--drill-origin`, `--user-origin` | Explicit grid/drill choices or a coordinate such as `25.4x25.4mm`. |
| Board and components | `--board-only`, `--no-board-body`, `--no-components`, `--component-filter` | Component filter is comma-separated reference designators with wildcards. |
| Copper and artwork | `--include-tracks`, `--include-pads`, `--include-zones`, `--include-inner-copper`, `--include-silkscreen`, `--include-soldermask` | Each is an explicit inclusion switch. |
| Via and shape handling | `--cut-vias-in-body`, `--fill-all-vias`, `--fuse-shapes`, `--no-extra-pad-thickness` | Controls via cuts/fill, fused geometry and pad thickness. |
| STEP tessellation/output | `--min-distance`, `--net-filter`, `--no-optimize-step` | `--min-distance` defaults to `0.01mm`; net filter uses wildcards; disabling optimization enables parametric curves. |
| Model substitution | `--subst-models` | Replaces VRML models with same-name STEP or IGS models. |

The help capture establishes that these options exist and records their
descriptions/defaults. It does not establish option interactions, precedence,
all error cases, the exact output geometry, tessellation tolerances, or parity
with Tokito. Several options need isolated native output fixtures before they
can be implemented or marked compatible.

## VRML board exporter

The native command is `pcb export vrml`. It shares output/overwrite, project
variables, DNP/unspecified model filters, variants and user origin with STEP.
It additionally exposes:

| Concern | Captured KiCad options | Captured default or behavior description |
|---|---|---|
| Units | `--units` | `mm`, `m`, `in`, or `tenths`; default is `in`. |
| External model files | `--models-dir`, `--models-relative` | Models are embedded when the directory is omitted/empty; relative paths are an explicit option used with a model directory. |

Notably, the captured VRML exporter has **no** `--component-filter` option and
no STEP exporter's copper/artwork controls. Do not copy the STEP option set
onto the VRML command or infer a selection behavior that was not observed.

## Tokito status and implementation boundary

Tokito's current `export glb --assemble-wrl` path is a local **model importer
and GLB assembler**. It reads a bounded VRML97 model subset and supports
Tokito-specific `--component-filter` and `--no-dnp` selection. These options
filter which source footprint models Tokito places into its GLB output; they
do not implement KiCad's `pcb export step` or `pcb export vrml` command.

The current Rust runtime does not export PCB geometry to STEP or board VRML and
does not tessellate STEP component assets into GLB. STEP model references are
reported as unsupported/unresolved rather than represented by substitute
cuboids. The isolated STEP-evaluation experiment is research only: recent
ESP8266 comparisons admitted one tested source mesh and rejected two others
because of degenerate/open/inconsistently oriented geometry or surface error.
It does not qualify a production importer.

## Required evidence before parity claims

1. Capture small pinned-board native exports for each option group and relevant
   combinations, including positive and negative/error cases.
2. Record input, executable, output hashes, model-library inputs, environment
   variables, commands, exit codes, and warnings in a manifest.
3. Compare normalized STEP/VRML structure and independently measured geometry:
   units, board/body bounds, origin, top/bottom poses, component transforms,
   missing-model behavior, mesh counts, and model identity.
4. Implement only after the observable contract and geometry acceptance rules
   are explicit. Unsupported flags must fail with a diagnostic; no silent
   acceptance or synthetic substitute geometry.
5. Keep KiCad out of Tokito's product runtime. It remains a version-pinned
   development reference for differential tests.

Until those gates pass, KiCad 3D-export parity and STEP-backed selectable board
coverage remain incomplete.
