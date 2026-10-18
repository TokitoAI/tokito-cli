# Tokito CLI command guide

Tokito prepares existing physical-board sources and reference 3D assets offline.
It does not turn a schematic into a routed PCB, qualify every model, or register
an AR scene. This guide separates executable examples from input templates.

## Launch and presentation

From the CLI repository, after building:

```powershell
cargo build --release --locked
$cli = ".\target\release\tokito-cli.exe"
& $cli --help
& $cli --color always --help
& $cli help component footprint generate
& $cli version
```

If `CARGO_TARGET_DIR` is configured, use the executable under that directory.
The CLI is a command-line program; launching it with no arguments shows help.
It is not an interactive shell, installer or background service.

At a PowerShell `PS>` prompt, use `& $cli --help`, not bare `help`, `-help`
or `--help`. Bare `help` invokes PowerShell's own help function; bare `--help`
is parsed as a PowerShell expression. These errors happen before Tokito runs.
An executable built in the default target can also be called directly as
`./target/release/tokito-cli.exe --help`; no PATH installation is required.

Root help groups commands into **Start here**, **Command groups**, **Workflow**,
**Help and presentation**, **Examples** and **Scope**. Full options live in
leaf help. Both `help pcb export glb` and `pcb export glb --help` work. Help
also accepts a positional input before `--help` without opening that input.

Leading `--color auto|always|never` and `--no-color` affect human help and
doctor presentation. Auto is conservative: it requires terminal stdout and a
true-colour indication (`COLORTERM=truecolor|24bit` or `WT_SESSION`), and is
disabled for `TERM=dumb`. `always` opts into RGB ANSI even when redirected.
Nonempty `NO_COLOR` overrides all modes. Machine JSON, CSV, SVG and GLB are never
decorated. There is no 16/256-colour fallback and no terminal background override.

| Use | Tokito token | RGB |
| --- | --- | --- |
| Title and command prefix | Mint `#08C9AA` | 8, 201, 170 |
| Help section heading | Indigo `#5456F0` | 84, 86, 240 |
| Passed check | Success `#3ECF8E` | 62, 207, 142 |
| Warning / not checked | Warning `#E0A43F` | 224, 164, 63 |
| Failed check | Danger `#EF5C68` | 239, 92, 104 |

These values come from the desktop-documented palette and the mobile mirror of
`tokito_ui::Tokens::dark()`. Evidence: desktop commit
`a5fee5dee7ed596f4b9c3cd78717eb64c12280b2`, `memory/ui-design-language.md` and
`native/src/theme.rs`; App commit `780f3fbef5e0c207c736b7ad4ab9a28f3e15ec50`,
`apps/mobile/src/theme.ts`. Desktop pins UI commit
`d3e6d3e219233661964fd0d1b1d9a74ab3ba9566`; that exact UI source was not locally
inspected. Essential body text retains the terminal's foreground colour.

## Start with a source check

```powershell
& $cli doctor
& $cli doctor ./board-project --json
& $cli doctor ./board-project --board Main.kicad_pcb
& $cli doctor ./board-project/Main.kicad_pcb
```

Without an input, doctor reports runtime information only. With a directory,
it uses bounded immediate-file project discovery and requires an exact choice
if several boards exist. With a PCB, it imports supported fields and checks
the supported physical outline. Model references are counted, not resolved or
qualified. Schematics are inventoried only. No input is changed or downloaded.

Representative successful source report, abbreviated from the actual schema:

```json
{
  "schema": "tokito.cli-doctor.v1",
  "scope": "source_and_outline_only",
  "status": "passed",
  "geometryQualified": false,
  "appAdmissionAuthorized": false,
  "arRegistrationVerified": false
}
```

With `--json`, doctor emits JSON before exit 2 when a source/outline check fails. A passed
doctor report does not authorize manufacturing, complete 3D geometry or AR.

## Reproducible 2D and package journey

The repository includes a pinned real SparkFun reference PCB. This example
creates a fresh output directory; it never overwrites an earlier result.

```powershell
$pcb = "tests/fixtures/open-source/sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb"
$project = Split-Path $pcb
$out = "target/readme-demo-" + (Get-Date -Format "yyyyMMdd-HHmmss")
New-Item -ItemType Directory -Path $out | Out-Null
& $cli doctor $pcb
& $cli inspect $pcb
& $cli validate outline $pcb
& $cli export svg $pcb --side top --output "$out/top.svg"
& $cli export svg $pcb --side bottom --output "$out/bottom.svg"
& $cli export placements $pcb --output "$out/source-placements.csv"
& $cli pcb export pos $pcb --format csv --units mm --side both --output "$out/manufacturing-pos.csv"
& $cli pcb export pos $pcb --format ascii --units mm --side both --output "$out/manufacturing-pos.pos"
& $cli package build $pcb --output "$out/source-package"
& $cli package verify "$out/source-package" --json
& $cli project inspect $project --json
& $cli project build $project --output "$out/project-bundle" --allow-partial --json
& $cli project verify "$out/project-bundle" --json
```

Actual inspect output from the 2026-10-16 live demo included:

```text
KiCad format version: 20241229
Units: millimetres
Thickness: 1.6 mm
Footprints: 20
Pads: 29
Referenced 3D models: 7
Nets: 9
Tracks: 0 segments, 0 arcs; vias: 0; zones: 1
```

The actual top SVG contained 109 supported geometry items at 51.900 × 51.900 mm.
It remained **partial**: four dimension-geometry diagnostics were reported.
The manufacturing-position CSV had 17 rows; component count and position-row
count differ because these commands have different inclusion rules.

Actual CSV excerpt:

```csv
Ref,Val,Package,PosX,PosY,Rot,Side
"FID2","Fiducial_0.5mm","Fiducial_0.5mm_Mask1mm",165.100000,-118.110000,180.000000,bottom
"FID3","Fiducial_0.5mm","Fiducial_0.5mm_Mask1mm",121.920000,-74.930000,180.000000,bottom
"C2","10uF","C_0603_1608Metric",136.525000,-106.680000,-90.000000,top
```

The ASCII exporter can write the same 17 SparkFun placements to a new position
file. Its live output began:

    ### Footprint positions - created on 2026-10-18T08:50:47 ###
    ### Printed by Tokito CLI version 0.1.0
    ## Unit = mm, Angle = deg.
    ## Side : All
    # Ref     Val               Package                              PosX       PosY       Rot  Side
    FID2      Fiducial_0.5mm    Fiducial_0.5mm_Mask1mm           165.1000  -118.1100  180.0000  bottom
    FID3      Fiducial_0.5mm    Fiducial_0.5mm_Mask1mm           121.9200   -74.9300  180.0000  bottom
    C2        10uF              C_0603_1608Metric                136.5250  -106.6800  -90.0000  top

ASCII output caps each identity field at 1,024 UTF-8 bytes and the complete
file at 32 MiB. It rejects control, line-separator and bidi-format characters.
Wide/combining Unicode display-column behavior has not been qualified; use CSV
when that alignment matters.

The partial project bundle contains source identity, diagnostics, top/bottom
views, placement/selector inventories and a GLB coverage report. Integrity
verification is separate from geometry qualification and App admission.

## Local model and component assembly journey

`twin prototype` uses the [Ultimaker presentation
standard](THREED_PRESENTATION_STANDARD.md) and emits `presentation-profile.json`
beside the offline viewer. Use its Lighting selector for the neutral reference
or material-studio mode, and **Save 4K PNG** for a full 3840 × 2160 model capture.
Unsupported device limits produce an explicit export error. Source model
precision and complete coverage still require independent qualification.

The following uses an **authored single-resistor test layout** and a pinned
real library body. It demonstrates commands, not an independently qualified
physical board. Continue using a fresh `$out` from the previous example.

```powershell
$testProject = "tests/fixtures/assembly-e2e"
$testBoard = "$testProject/single-resistor.kicad_pcb"
$models = "tests/fixtures/open-source/kicad-resistor-models"
$body = "$models/Resistor_SMD.3dshapes/R_0603_1608Metric.wrl"
& $cli resolve models $testBoard --project-root $testProject --library-root $models --json
& $cli resolve models $testBoard --project-root $testProject --library-root $models --extract-dir "$out/captured-models" --json
& $cli model inspect $body --units kicad-wrl --json
& $cli export glb $testBoard --assemble-wrl --project-root $testProject --library-root $models --output "$out/resistor.glb" --json-report
& $cli model assignments template $testBoard --output "$out/assignments-template.json"
```

`resistor.glb` has a component selector linked to source identity. The template
requires explicit model assignments before it can provide resolved model
coverage. Edit a **new copy** with an exact model hash, units and transform, then:

```powershell
& $cli model assignments verify $testBoard --manifest reviewed-assignments.json --project-root approved-model-root --json
& $cli export glb $testBoard --model-assignments reviewed-assignments.json --project-root approved-model-root --output new-candidate.glb --allow-partial --json-report
```

These two commands are input templates: the reviewed files are not shipped as
ready-to-use manufacturer evidence. `component model envelope SPEC.json --board
BOARD --output NEW.wrl --json` additionally emits a nominal rectangular body
from explicit dimensions and identity claims. It produces no terminals or
qualified package surface; a synthetic example is never a real component model.

Opt-in `--component-filter R1,U1`, `--no-dnp` and `--no-unspecified` filter
VRML assembly before resolution. `--no-unspecified` means no native SMD/THT
type; it does not mean “missing model.” Selected missing models refuse output
unless `--allow-partial` is explicit.

`model derive-wrl INPUT.wrl --output NEW.wrl --units kicad-wrl
--remove-unused-invalid-materials` and/or `--remove-zero-area-faces` explicitly
derive a **changed candidate** and emit a change report. At least one flag is
required. The original is preserved; deleting faces is not proof of solid repair.

## Footprint and library commands

```powershell
& $cli component footprint r0603 --output "$out/nominal-r0603.kicad_mod" --json
& $cli component footprint generate examples/footprints/r0603-reference.json --output "$out/spec-r0603.kicad_mod" --json
& $cli component footprint library create examples/footprints/mixed-smd-through-hole.json --output "$out/MixedHeader.pretty" --json
& $cli component footprint inspect "$out/spec-r0603.kicad_mod" --json
& $cli component footprint library create examples/footprints/sot-23-to236ab.json --output "$out/Demo.pretty" --json
& $cli component footprint library inspect "$out/Demo.pretty" --json
& $cli component footprint library audit "$out/Demo.pretty" --json
& $cli component footprint library lock "$out/Demo.pretty" --output "$out/library.lock.json" --json
& $cli component footprint library verify "$out/Demo.pretty" --lock "$out/library.lock.json" --json
```

The explicit 0603 example reports `status: explicit-smd-geometry-candidate`,
`padCount: 2`, `graphicCount: 4`, `manufacturerLandPatternQualified: false` and
`componentBodyGenerated: false`. Other supplied specs cover SOT-23, round or
oval-offset plated header pads, a centered circular non-plated mounting hole,
and mixed SMD/PTH/NPTH pad technology in one footprint. The mixed fixture is
synthetic format coverage: KiCad sets the footprint attribute to `through_hole`
when the pads include a plated through-hole pad. It is not a qualified connector
land pattern. A local KiCad 10.0.5 format/plot readback checks that the generated
candidate can be plotted; its receipt is in
[`docs/evidence/mixed-pad-kicad-10.0.5.json`](evidence/mixed-pad-kicad-10.0.5.json).
The shipping CLI does not invoke KiCad. Library locks verify byte identity;
audits report naming/collision observations.
Neither is a KLC or manufacturing certificate, and no `fp-lib-table` is changed.

## Selectable twin and viewer commands

```powershell
$glb = "demo/esp32-natural/active.digital-twin.glb"
$map = "demo/esp32-natural/component-map.json"
$manifest = "demo/esp32-natural/active.digital-twin.json"
& $cli twin inspect $glb --component-map $map --manifest $manifest
& $cli twin select $glb --component-map $map --manifest $manifest --reference R6
& $cli twin select $glb --component-map $map --manifest $manifest --class diode
& $cli twin prepare $glb --component-map $map --manifest $manifest --output "$out/prepared" --allow-partial
& $cli twin prototype $glb --component-map $map --manifest $manifest --output "$out/viewer" --allow-partial
```

The viewer contains `index.html`, GLB, component map/index, manifest and a
preparation report. It supports orbit/zoom and component selection. The ESP32
reference has 26 semantic records and 23 renderable selectors; it has not become
a fully populated, qualified physical board by passing preparation. This command
repackages an existing model; it does not generate new STEP geometry.

Additional input templates:

```text
tokito-cli twin audit-population GLB --component-map MAP --manifest MANIFEST --reference-bom ESP32.csv
tokito-cli twin adapt-source GLB --component-map MAP --geometry-index INDEX --provenance PROVENANCE --coverage COVERAGE --board-id ID --board-revision REV --output NEW_DIRECTORY
```

Adaptation needs all five files from the **same pinned source candidate**.
The actual complete Ultimaker candidate retry reported 574 components and 310
renderables; package bodies J34/U9/K1/L2 and four spacers remained unresolved.
Physical geometry and registration flags stayed false.

## Dataset and BOM commands

Use the current `tokito-app-dataset`, not the old `devboard_ar_dataset`.
These templates require exact, materialized assets and versioned request JSON:

```text
tokito-cli bom reconcile BOARD --bom FILE --format nxp-json|esp32-csv --json
tokito-cli dataset inventory GIT_ROOT --revision FULL_COMMIT
tokito-cli dataset bind GIT_ROOT --assets-root ROOT --request REQUEST.json --output NEW_BINDING.json
tokito-cli dataset bind-external GIT_ROOT --assets-root ROOT --request EXTERNAL_REQUEST.json --output NEW_BINDING.json
tokito-cli dataset lock ROOT --request REQUEST.json --output NEW_LOCK.json
tokito-cli dataset verify ROOT --lock LOCK.json
```

The BOM retry against the materialized ESP32 layout reported 24 BOM references,
zero unmatched/ambiguous references, and two board components absent from the
BOM. A successful identity join does not verify manufacturer identity or pose.

Inventory/binding verifies immutable Git identities and selected local payloads;
locks verify declared asset bytes. External binding requires source-pinned
provenance claims and never fetches the URL. A local untracked provenance file
cannot be authenticated by pinning an unrelated `main` commit. See
[offline dataset intake](DATASET_STEP_QUALIFICATION.md) for qualification limits.

## Errors and recovery

| Message or condition | Action |
| --- | --- |
| `PCB input is a Git LFS pointer` | Supply the actual payload and verify its SHA-256 against the pointer. The CLI does not fetch or silently replace the input. |
| `cannot read source candidate input '…/geometry-coverage.json'` | Supply the full pinned candidate including coverage; do not invent a coverage file. |
| `requested asset is not tracked at source pin` | Select the correct source commit/PR candidate; never bypass immutable binding. |
| Several PCB files | Use `project inspect` and `doctor --board EXACT_FILENAME`; no automatic guess is made. |
| Missing/unsupported selected model | Resolve a source-qualified model, or explicitly request a visibly partial result. |
| Existing output | Choose a new output path. No overwrite flag is implemented. |

The initial live demo had two input failures (an LFS pointer and absent candidate
coverage). Hash-verified materialized/pinned retries succeeded. Expected safety
refusals remain refusals, not successful exports. The earlier “missing URL” demo
label was inaccurate: source binding refused its untracked provenance first.

## Verification status

The current CLI implementation passed all **40 base-profile local gates**:
`target/delivery/local/20261016T053247Z-133a12e1d9f543aa807ca1bfde07f42b/verification.json`.
The release binary SHA-256 is `51cc8c7dc0f961ce57760b7bad9e6f13710ad9c201c3dfb9a43f4538271efa9d`.
This includes real subprocess checks for grouped/positional help, colours and
NO_COLOR, plain JSON, doctor scopes/refusals, LFS diagnostics and missing
candidate inputs. Root tests, Clippy, release build, four-board exports,
footprint/library workflows and the other base-profile journeys also passed.

The run reused an explicitly selected trusted Cargo cache and executed every
gate; it is not a clean dependency build. Process-local temporary files used F:
after an earlier run failed on C: disk exhaustion. Earlier paging and stale-help
assertion failures are retained in the goal tracker. No optional whole-dataset,
App/AR or GUI-rendering qualification is claimed. These post-run narrative notes
will be included in the final verification before delivery. GitHub Actions and
sibling source modifications are outside this work.
