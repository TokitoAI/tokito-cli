# Versioned repository file map

Snapshot updated on 11 October 2026 for project intake, the 0603 footprint slice, and POS auxiliary-origin reference coverage. This lists
source, documentation, scripts, fixtures and frozen models; ignored build caches,
local datasets, binaries and `.git/` are excluded. The README explains module
responsibilities and architecture. Regenerate when versioned paths change.

316 versioned/planned delivery files, including this map.

```text
tokito_cli/
├── demo/
│   ├── esp32/
│   │   ├── active.digital-twin.glb
│   │   ├── active.digital-twin.json
│   │   ├── component-index.json
│   │   ├── component-map.json
│   │   ├── index.html
│   │   ├── native-selected-resistor-4k.png
│   │   ├── prepare-report.json
│   │   └── reference-population-audit.json
│   ├── esp32-natural/
│   │   ├── active.digital-twin.glb
│   │   ├── active.digital-twin.json
│   │   ├── browser-acceptance.json
│   │   ├── component-index.json
│   │   ├── component-map.json
│   │   ├── esp32-natural-4k.png
│   │   ├── index.html
│   │   ├── material-acceptance.json
│   │   ├── material-profile.json
│   │   ├── material-refinement-report.json
│   │   ├── prepare-report.json
│   │   └── reference-population-audit.json
│   ├── esp8266/
│   │   ├── active.digital-twin.glb
│   │   ├── active.digital-twin.json
│   │   ├── assembly-e2e-receipt.json
│   │   ├── assembly-report.json
│   │   ├── browser-acceptance.json
│   │   ├── component-index.json
│   │   ├── component-map.json
│   │   ├── esp8266-selected-module-4k.png
│   │   ├── evaluated-step-comparison.json
│   │   ├── evaluated-step-reference.json
│   │   ├── index.html
│   │   └── prepare-report.json
│   ├── nxp-reference/
│   │   ├── active.digital-twin.glb
│   │   ├── active.digital-twin.json
│   │   ├── browser-acceptance.json
│   │   ├── component-index.json
│   │   ├── component-map.json
│   │   ├── index.html
│   │   ├── prepare-report.json
│   │   └── selected-ic-4k.png
│   ├── nxp-refined/
│   │   ├── J10-mesh.json
│   │   ├── J11-mesh.json
│   │   ├── U8-mesh.json
│   │   ├── active.digital-twin.glb
│   │   ├── active.digital-twin.json
│   │   ├── browser-acceptance.json
│   │   ├── component-index.json
│   │   ├── component-map.json
│   │   ├── evaluated-step-reference.json
│   │   ├── index.html
│   │   ├── native-components.json
│   │   ├── nxp-refined-4k.png
│   │   ├── prepare-report.json
│   │   ├── refinement-e2e.json
│   │   └── refinement.json
│   ├── ultimaker/
│   │   ├── active.digital-twin.glb
│   │   ├── active.digital-twin.json
│   │   ├── browser-acceptance.json
│   │   ├── component-index.json
│   │   ├── component-map.json
│   │   ├── index.html
│   │   ├── prepare-report.json
│   │   └── selected-ic-4k.png
│   ├── README.md
│   └── bundle-manifest.json
├── docs/
│   ├── evidence/
│   │   ├── esp32-presentation-local-verification-20261011.json
│   │   ├── esp32-presentation-real-cli-20261011.json
│   │   ├── esp8266-drilled-substrate-20261011.json
│   │   └── nxp-refined-local-verification-20261011.json
│   ├── schemas/
│   │   └── board-package.schema.json
│   ├── AFAN_CODEX_INTEGRATION_PROMPT.md
│   ├── AFAN_INTEGRATION_HANDOVER.md
│   ├── AFAN_PROTOTYPE_HANDOFF_PLAN.md
│   ├── APP_API_BRIDGE_AUDIT.md
│   ├── ARCHITECTURE.md
│   ├── BOARD_AR_DATASET_AUDIT.md
│   ├── BOM_COMPONENT_QUALIFICATION.md
│   ├── CANONICAL_ENGINE.md
│   ├── CI_AND_DELIVERY.md
│   ├── COMPONENT_FORGE_ARCHITECTURE.md
│   ├── COMPONENT_FORGE_OPEN_SOURCE_REVIEW.md
│   ├── COMPONENT_FORGE_QUALIFICATION.md
│   ├── COMPONENT_FORGE_SPECIFICATION.md
│   ├── DATASET_AR_HANDOFF.md
│   ├── DATASET_BOARD_HANDOFF.md
│   ├── DATASET_EXTERNAL_BINDING.md
│   ├── DATASET_GIT_INVENTORY.md
│   ├── DATASET_SOURCE_LOCK.md
│   ├── DATASET_STEP_QUALIFICATION.md
│   ├── DEMO_BUNDLE.md
│   ├── DIGITAL_TWIN_SELECTION.md
│   ├── ECOSYSTEM_CONTRACT_MAP.md
│   ├── ECOSYSTEM_INTEGRATION.md
│   ├── ESP32_NATURAL_MATERIALS.md
│   ├── ESP32_POPULATION_AUDIT.md
│   ├── ESP8266_PARALLEL_PROGRESS.md
│   ├── ESP8266_PLACEMENT_RESEARCH.md
│   ├── ESP8266_SOURCE_ASSEMBLY.md
│   ├── ESP8266_SOURCE_INTAKE.md
│   ├── EXCELLON_SOURCE_INVENTORY.md
│   ├── GLB_EXPORT_ARCHITECTURE.md
│   ├── GOAL_TRACKER.md
│   ├── IMPLEMENTATION_GATES.md
│   ├── KICAD_3D_QUALITY_GATES.md
│   ├── KICAD_CLI_BEHAVIOR.md
│   ├── KICAD_CLI_PARITY_MATRIX.md
│   ├── KICAD_COMPLETION_GATES.md
│   ├── KICAD_GLB_REFERENCE.md
│   ├── PCB_POSITION_EXPORT.md
│   ├── LOCAL_OUTPUT_LOCATIONS.md
│   ├── MODEL_ASSIGNMENTS.md
│   ├── MODEL_RESOLUTION.md
│   ├── NATIVE_FORMAT_ARCHITECTURE.md
│   ├── NXP_REALISM_UPGRADE.md
│   ├── PCB_FIXTURE_CORPUS.md
│   ├── PCB_TOPOLOGY.md
│   ├── PERSONAL_AI_HANDOFF.md
│   ├── PR9_CI_DIAGNOSIS.md
│   ├── PRODUCT_COMPLETION_PLAN.md
│   ├── PROJECT_INTAKE_AND_FOOTPRINT.md
│   ├── REPOSITORY_MAP.md
│   ├── SELECTABLE_PROTOTYPE.md
│   ├── SELECTABLE_PROTOTYPE_QA.md
│   ├── STEP_FACE_COVERAGE.md
│   ├── STEP_LENGTH_UNIT_PREFLIGHT.md
│   ├── TEAM_HANDOFF.md
│   ├── TOKITO_BOARD_PACKAGE_V0.md
│   └── ULTIMAKER_SELECTABLE_PROTOTYPE.md
├── examples/
│   └── inspect_engine.rs
├── experiments/
│   ├── altium-evaluation/
│   │   ├── src/
│   │   │   ├── assembly.rs
│   │   │   ├── association.rs
│   │   │   ├── board_inputs.rs
│   │   │   ├── drill.rs
│   │   │   ├── embedded.rs
│   │   │   ├── main.rs
│   │   │   ├── pose_inputs.rs
│   │   │   ├── records.rs
│   │   │   └── substrate.rs
│   │   ├── Cargo.lock
│   │   └── Cargo.toml
│   ├── step-evaluation/
│   │   ├── examples/
│   │   │   ├── torus_inverse_regression.rs
│   │   │   └── torus_surface_evidence.rs
│   │   ├── src/
│   │   │   ├── candidate_mesh.rs
│   │   │   ├── face_coverage.rs
│   │   │   ├── main.rs
│   │   │   ├── negative_torus.rs
│   │   │   ├── occurrence_world.rs
│   │   │   └── unit_preflight.rs
│   │   ├── tests/
│   │   │   ├── negative_cli.rs
│   │   │   └── units_cli.rs
│   │   ├── .gitignore
│   │   ├── Cargo.lock
│   │   ├── Cargo.toml
│   │   ├── README.md
│   │   ├── dependency-audit.json
│   │   ├── negative-e2e-results.json
│   │   └── windows-dependency-tree.txt
│   └── twin-refinement/
│       ├── profiles/
│       │   ├── esp32-natural-complete-source.json
│       │   └── esp32-natural-v9.json
│       ├── src/
│       │   ├── main.rs
│       │   └── strict_json.rs
│       ├── Cargo.lock
│       └── Cargo.toml
├── scripts/
│   ├── evaluate-step-reference.py
│   ├── real-cli-e2e.ps1
│   └── verify-local.ps1
├── src/
│   ├── model_mesh/
│   │   └── zero_area.rs
│   ├── assigned_assembly.rs
│   ├── bom.rs
│   ├── cli.rs
│   ├── component_forge.rs
│   ├── component_pose.rs
│   ├── dataset.rs
│   ├── dataset_external.rs
│   ├── dataset_git.rs
│   ├── digital_twin.rs
│   ├── document.rs
│   ├── footprint_family.rs
│   ├── geometry.rs
│   ├── glb.rs
│   ├── lib.rs
│   ├── main.rs
│   ├── model_assembly.rs
│   ├── model_assignments.rs
│   ├── model_mesh.rs
│   ├── model_resolver.rs
│   ├── package_input.rs
│   ├── pos_export.rs
│   ├── population_audit.rs
│   ├── project_intake.rs
│   ├── source_candidate.rs
│   ├── strict_json.rs
│   ├── topology.rs
│   └── twin_viewer.rs
├── tests/
│   ├── reference/
│   │   └── kicad-cli-10.0.5/
│   │       └── pos/ (version-pinned CSV output fixtures and SHA-256 manifest)
│   ├── fixtures/
│   │   ├── assembly-e2e/
│   │   │   └── single-resistor.kicad_pcb
│   │   ├── compat-v0/
│   │   │   ├── arduino-uno-r3-community/
│   │   │   │   ├── derived/
│   │   │   │   │   ├── inspection.json
│   │   │   │   │   ├── placements.csv
│   │   │   │   │   ├── svg-bottom.svg
│   │   │   │   │   └── svg-top.svg
│   │   │   │   └── manifest.json
│   │   │   ├── nxp-mcxa-esc/
│   │   │   │   ├── derived/
│   │   │   │   │   ├── inspection.json
│   │   │   │   │   ├── placements.csv
│   │   │   │   │   ├── svg-bottom.svg
│   │   │   │   │   └── svg-top.svg
│   │   │   │   └── manifest.json
│   │   │   ├── raspberry-pi-pico-community/
│   │   │   │   ├── derived/
│   │   │   │   │   ├── inspection.json
│   │   │   │   │   ├── placements.csv
│   │   │   │   │   ├── svg-bottom.svg
│   │   │   │   │   └── svg-top.svg
│   │   │   │   └── manifest.json
│   │   │   ├── sparkfun-default-kicad/
│   │   │   │   ├── derived/
│   │   │   │   │   ├── inspection.json
│   │   │   │   │   ├── placements.csv
│   │   │   │   │   ├── svg-bottom.svg
│   │   │   │   │   └── svg-top.svg
│   │   │   │   └── manifest.json
│   │   │   ├── synthetic-bow-tie/
│   │   │   │   ├── derived/
│   │   │   │   │   ├── inspection.json
│   │   │   │   │   └── placements.csv
│   │   │   │   ├── source/
│   │   │   │   │   └── board.kicad_pcb
│   │   │   │   └── manifest.json
│   │   │   └── README.md
│   │   └── open-source/
│   │       ├── arduino-uno-r3-community/
│   │       │   ├── Arduino UNO.kicad_pcb
│   │       │   └── LICENSE.txt
│   │       ├── kicad-resistor-models/
│   │       │   ├── Resistor_SMD.3dshapes/
│   │       │   │   └── R_0603_1608Metric.wrl
│   │       │   ├── LICENSE.md
│   │       │   └── README.md
│   │       ├── kicad-royalblue54l-nfc/
│   │       │   ├── LICENSE
│   │       │   ├── RoyalBlue54L-NFC-Antenna-EdgeCuts.gbr
│   │       │   ├── RoyalBlue54L-NFC-Antenna.kicad_pcb
│   │       │   └── UPSTREAM_README.md
│   │       ├── lumenpnp-vac-interposer/
│   │       │   ├── README.md
│   │       │   ├── vac-adapter-Edge_Cuts.gbr
│   │       │   └── vac-adapter_profile.kicad_pcb
│   │       ├── lumenpnp-vac-interposer-original/
│   │       │   ├── LICENSE
│   │       │   ├── README.md
│   │       │   ├── UPSTREAM_README.md
│   │       │   ├── vac-adapter-Edge_Cuts.gbr
│   │       │   └── vac-adapter.kicad_pcb
│   │       ├── nxp-mcxa-esc/
│   │       │   ├── LICENSE
│   │       │   └── MR-ESC-MCXA-AM32.kicad_pcb
│   │       ├── porcupine-routed/
│   │       │   ├── LICENSE
│   │       │   ├── UPSTREAM_README.md
│   │       │   └── porcupine_routed.kicad_pcb
│   │       ├── raspberry-pi-pico-community/
│   │       │   ├── LICENSE.txt
│   │       │   └── RPI-PICO-R3a-PUBLIC - Official.kicad_pcb
│   │       ├── rp2040-game-con/
│   │       │   ├── LICENSE.txt
│   │       │   ├── README.md
│   │       │   ├── rp2040_game_con-Edge_Cuts.gm1
│   │       │   └── rp2040_game_con.kicad_pcb
│   │       └── sparkfun-default-kicad/
│   │           ├── LICENSE.md
│   │           ├── README.md
│   │           └── SparkFun_Basic_Board_Example.kicad_pcb
│   ├── altium_source_e2e.py
│   ├── bom_api.rs
│   ├── bom_e2e.py
│   ├── compat_v0.py
│   ├── component_dataset_e2e.py
│   ├── component_envelope_e2e.py
│   ├── component_pose.rs
│   ├── dataset_e2e.py
│   ├── dataset_git_e2e.py
│   ├── dataset_twin_e2e.py
│   ├── e2e_cli.py
│   ├── engine_api.rs
│   ├── esp32_candidate_e2e.py
│   ├── esp32_chaining_e2e.py
│   ├── esp32_native_browser.js
│   ├── esp32_switch_e2e.py
│   ├── esp32_zero_area_e2e.py
│   ├── esp8266_assembly_candidates_e2e.py
│   ├── esp8266_browser.js
│   ├── esp8266_drill_inventory_e2e.py
│   ├── esp8266_drilled_substrate_e2e.py
│   ├── esp8266_prototype_e2e.py
│   ├── footprint_family.rs
│   ├── footprint_family_e2e.py
│   ├── geometry_e2e.py
│   ├── glb_api.rs
│   ├── glb_e2e.py
│   ├── kicad_glb_reference.py
│   ├── model_assignments_api.rs
│   ├── model_assignments_e2e.py
│   ├── model_mesh_api.rs
│   ├── model_resolution_e2e.py
│   ├── model_resolver_api.rs
│   ├── nxp_refinement_e2e.py
│   ├── prototype_browser.js
│   ├── project_intake_e2e.py
│   ├── source_candidate_e2e.py
│   ├── step_candidate_mesh_e2e.py
│   ├── step_face_coverage_e2e.py
│   ├── step_occurrence_world_e2e.py
│   ├── step_units_e2e.py
│   ├── test_evaluated_step_bounds.py
│   ├── topology_api.rs
│   └── twin_e2e.py
├── .gitattributes
├── .gitignore
├── AGENTS.md
├── Cargo.lock
├── Cargo.toml
└── README.md
```
