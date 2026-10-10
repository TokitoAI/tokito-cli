# KiCad and component-generation completion gates

The user's 2026-10-10 requirement makes [CLI parity #13](https://github.com/TokitoAI/tokito-cli/issues/13) and [footprint/library generation #24](https://github.com/TokitoAI/tokito-cli/issues/24) implementation goals. Research, prototypes, nominal envelopes and shells do not complete them. Shipping code remains original Rust, independent of KiCad runtime/native CAD bindings. Sibling source stays read-only; delivery uses local verification and PRs with Actions disabled.

## Scope and evidence

The [CLI matrix](KICAD_CLI_PARITY_MATRIX.md) tracks command families/options/export/refusal behavior against pinned references. [Generator references](COMPONENT_FORGE_OPEN_SOURCE_REVIEW.md) pin Library Tools, footprints and packages3D. Inventory upstream generator families and parameter schemas explicitly. A first working family is a milestone, not full library parity. Each operation/family requires implementation, independent output comparisons and recorded reference version/commit, input/output hashes, options, coverage and gaps. Local KiCad 10.0.5 observations are not 10.0.7 equivalence.

| Gate | Required evidence | Current state |
|---|---|---|
| Commands/options/exports/errors | Version-pinned operation matrix; original Rust code and independent positive/negative output comparisons | Incomplete; see operation matrix |
| Footprint/library features | Family inventory; dimensions/tolerances; pads/numbering/drill/mask/paste/fab/silk/courtyard/origin; deterministic .kicad_mod; independent manufacturer/library comparisons | Incomplete; cuboid generator produces no footprint |
| Component bodies | Source-qualified surfaces, terminals, pin-one details, units, model coverage, tessellation and materials | Incomplete; experimental STEP reader/envelopes are not qualified bodies |
| Contours and cutouts | Independent source comparison and valid topology including curved edges, holes, crossings and ambiguity | Bounded profiles tested; full parity unverified |
| Placement | Independent dimensions, position, rotation, top/bottom and mounting-plane comparisons under declared units | Bounded pose tests pass; full dataset equivalence unverified |
| Individual selection | One entity per component with exact reference/footprint UUID where present; explicit native-ID crosswalk for other formats; no duplicate/shared ambiguity | NXP mesh selectors prepared; source UUID/PCB association and App acceptance unresolved |
| Missing/unqualified models | Explicit diagnostics and strict refusal unless partial consent; shells/cuboids never count as complete bodies | Existing refusal paths tested; full coverage open |
| 4K visualization | Actual 3840x2160 render/export where supported, separately from geometry/tessellation/material/placement accuracy | Unverified; GLB has no inherent image resolution |
| Dataset E2E | Pinned real inputs; per-board passed/failed/unsupported/unverified; independent geometry comparisons | Selected journeys tested; all-board qualification incomplete |
| Delivery | Current-source local formatting/tests/Clippy/build/E2E, inspected outputs and independent review before branch push | Verifier exists; every source slice requires its own receipt |

Engineering tolerances must come from source requirements, not convenient passing thresholds. Photos/designators do not establish hidden pads or physical revision. Altium UNIQUEID is not a KiCad footprint UUID.

## ESP8266 first

Refresh dataset main/open PRs before accepting updates. PR6 `5283bc059e30e51f4d3a81a674425936fc213214` contains NodeMCU reference sources with 34 components, 41 nets, seven schematics, Gerbers and 17 STEP streams. The inspected tree has no ESP8266 assembly GLB, .kicad_pcb, component/model transform map or independently qualified model units. Its photographed CH340-style clone differs from the USB-UART reference variant. Bind actual source assets, then verify associations/units/geometry before selectable-twin claims. See [ESP8266 intake](ESP8266_SOURCE_INTAKE.md).
