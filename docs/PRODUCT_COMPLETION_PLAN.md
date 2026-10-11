# From selectable prototype to end-to-end Tokito CLI

Status: active engineering plan, 2026-10-11. The CLI is not a complete KiCad replacement or an accepted phone/AR pipeline. The [command matrix](KICAD_CLI_PARITY_MATRIX.md) and [completion gates](KICAD_COMPLETION_GATES.md) define the detailed acceptance criteria.

## Verified starting point

The current engine imports a reviewed subset of existing `.kicad_pcb` files and Tokito v0.1 source packages. It exposes a Rust physical-board document, bounded outline checks, source-frame placement inventory, partial SVG, board-shell/limited WRL GLB, model/dataset qualification tools, and selection of components in prepared GLBs. The frozen ESP32, ESP8266, NXP and Ultimaker prototypes are demonstrations with explicit missing-model and physical-fit limits. [PR #36](https://github.com/TokitoAI/tokito-cli/pull/36) merged at `0d0b6dde4bf9eb8f0cce102bc56c13fb7fba9611`; its ESP32 presentation work changed materials, not physical geometry.

KiCad's [10.0 CLI manual](https://docs.kicad.org/10.0/en/cli/cli.html) describes six command families and says it is based on 10.0.7. The installed Windows reference executable is 10.0.5. Command names, reference outputs and errors must be pinned to one version before a compatibility claim. KiCad `fp` exports or upgrades footprint libraries; footprint *generation* is a separate KiCad Library Tools/KicadModTree workflow. Tokito will implement observable behavior independently in pure Rust and will not run KiCad in the product path.

## Dependency-ordered work

| Gate | Owner | State | Acceptance evidence | Next action |
| --- | --- | --- | --- | --- |
| Existing-board project intake | CLI | In progress | Bounded read-only discovery, explicit choice among multiple boards, selected path/hash, schematic-only refusal, actual CLI E2E | Review and test `project inspect`; add export handoff after source selection is stable |
| KiCad CLI behavior [#13](https://github.com/TokitoAI/tokito-cli/issues/13) | CLI + reference reviewer | Partial | Per-command pinned syntax, options, positive/negative fixtures, output and exit comparisons | PCB position CSV is compared on four real fixtures against installed 10.0.5, including auxiliary-origin transforms; capture 10.0.7 evidence, then expand SVG/export/validation families and keep unsupported commands explicit |
| Footprint/library workflow [#24](https://github.com/TokitoAI/tokito-cli/issues/24) | CLI + dataset owner | Research only | Typed package parameters, deterministic `.kicad_mod`, pad numbering/layers/drills/mask/paste, fab/silk/courtyard; independent manufacturer/library comparisons | Build a narrow generator using verified existing fixtures; choose a missing footprint only when complete manufacturer land-pattern evidence is pinned |
| Dataset source and body qualification [#18](https://github.com/TokitoAI/tokito-cli/issues/18), [#20](https://github.com/TokitoAI/tokito-cli/issues/20), [#26](https://github.com/TokitoAI/tokito-cli/issues/26) | CLI + dataset owner | Partial | Immutable board revision/source, exact ref/UUID crosswalk, model units/pose, source-vs-output geometry and physical-photo acceptance | Refresh latest dataset main/PR heads, verify each active board without promoting candidate geometry silently |
| Native Tokito physical package [#3](https://github.com/TokitoAI/tokito-cli/issues/3) | CLI | Open | Versioned canonical geometry, models, provenance, diagnostics, component selectors; v0.1 compatibility | Add a new schema after engine/import fields stabilize |
| Desktop schematic-to-layout [#7](https://github.com/TokitoAI/tokito-cli/issues/7) | Desktop + CLI | Open | Physical outline, stackup, footprint assignment, placement, routing, net/pin crosswalk and independent DRC | Define the handoff before claiming schematic-to-3D; schematic symbols alone do not locate physical parts |
| Phone/App/AR [#5](https://github.com/TokitoAI/tokito-cli/issues/5), [#23](https://github.com/TokitoAI/tokito-cli/issues/23) | App + CLI | Open | Exact asset importer, selected-component highlight, registered board pose, real-device orientation/flip/latency tests | Integrate only after candidate assets and identity maps are qualified; sibling repo remains read-only here |

The first manufacturer footprint audit found **missing 3D bodies**, but no established missing PCB land pattern among the inspected active boards. It checked dataset `main` at `38864c7af0d77c6a1548a1c0e5e243888ca8ec42` (no open dataset PRs at inspection). NXP U11 already has nine custom pads; the current Ultimaker source inventory reports 574 footprints and 1,322 pads, while several parts still need body/physical-fit qualification. Do not generate an invented pad arrangement from photos or BOM fields.

## Product completion test

A supported project opens without modifying source files; an existing physical PCB is selected explicitly; every emitted view names its source digest, units, frame and incomplete features; 2D/3D placement and pad/model geometry match independent references within documented tolerances; all required components have qualified geometry or a conspicuous refusal. Desktop, CLI and App share a versioned package contract. On-device tests must demonstrate component selection and registered AR on the exact board revision. Every declared KiCad-compatible operation has version-pinned success and error cases; operations not implemented are reported as unsupported.

Local format, unit, Clippy, release and real subprocess E2E gates precede a branch/PR push. The owner has disabled GitHub Actions for cost; a missing hosted check is never reported as a pass. Current-source evidence and independent review precede a merge. No source-only schematic-to-3D or complete KiCad parity is claimed until these gates pass.
