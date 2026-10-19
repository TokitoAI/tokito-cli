# Versioned repository file map

Snapshot updated on 19 October 2026 for the component-geometry investigation.
This map is generated from `git ls-files --cached --others --exclude-standard`;
it includes tracked and new non-ignored project files, including hidden entry
files. Ignored build outputs, research source snapshots, local datasets and
`.git/` are excluded. Regenerate after versioned paths change.

481 project files are listed.

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
│   │   ├── presentation-profile.json
│   │   ├── reference-population-audit.json
│   │   └── standard-4k.png
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
│   │   ├── presentation-profile.json
│   │   ├── reference-population-audit.json
│   │   └── standard-4k.png
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
│   │   ├── prepare-report.json
│   │   ├── presentation-profile.json
│   │   └── standard-4k.png
│   ├── nxp-reference/
│   │   ├── active.digital-twin.glb
│   │   ├── active.digital-twin.json
│   │   ├── browser-acceptance.json
│   │   ├── component-index.json
│   │   ├── component-map.json
│   │   ├── index.html
│   │   ├── prepare-report.json
│   │   ├── presentation-profile.json
│   │   ├── selected-ic-4k.png
│   │   └── standard-4k.png
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
│   │   ├── presentation-profile.json
│   │   ├── refinement-e2e.json
│   │   ├── refinement.json
│   │   └── standard-4k.png
│   ├── ultimaker/
│   │   ├── active.digital-twin.glb
│   │   ├── active.digital-twin.json
│   │   ├── browser-acceptance.json
│   │   ├── component-index.json
│   │   ├── component-map.json
│   │   ├── index.html
│   │   ├── prepare-report.json
│   │   ├── presentation-profile.json
│   │   ├── selected-ic-4k.png
│   │   └── standard-4k.png
│   ├── README.md
│   ├── bundle-manifest.json
│   └── presentation-standard-verification.json
├── docs/
│   ├── evidence/
│   │   ├── adaptive-boundary-20261019.json
│   │   ├── esp32-presentation-local-verification-20261011.json
│   │   ├── esp32-presentation-real-cli-20261011.json
│   │   ├── esp8266-drilled-substrate-20261011.json
│   │   ├── glb-dnp-local-verification-20261013.json
│   │   ├── interior-retriangulation-20261019.json
│   │   ├── kicad-3d-cli-help-10.0.7.json
│   │   ├── kicad-dnp-10.0.7-structure-comparison.json
│   │   ├── kicad-dnp-local-verification-2026-10-14.json
│   │   ├── kicad-pcb-export-glb-help-10.0.5.txt
│   │   ├── kicad-pcb-export-glb-help-10.0.7.txt
│   │   ├── kicad-pcb-export-step-help-10.0.7.txt
│   │   ├── kicad-pcb-export-vrml-help-10.0.7.txt
│   │   ├── kicad-pcbnew-dll-static-strings.json
│   │   ├── metric-refinement-20261019.json
│   │   ├── mixed-pad-kicad-10.0.5.json
│   │   ├── model16-projection-audit-20261018.json
│   │   ├── model16-projection-audit-20261018.receipt.json
│   │   ├── nxp-refined-local-verification-20261011.json
│   │   ├── physical-refinement-20261019.json
│   │   ├── projection-solver-status-20261019.json
│   │   ├── priority-refinement-20261019.json
│   │   ├── surface-grid-20261019.json
│   │   └── ultimaker-presentation-browser-20261016.json
│   ├── quality/
│   │   ├── assets/
│   │   │   └── esp32-browser-selection-4k.png
│   │   └── 2026-10-17-windows-browser-e2e.md
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
│   ├── CLI_COMMAND_GUIDE.md
│   ├── CLI_RELEASE_READINESS.md
│   ├── CLI_V1_PLAN.md
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
│   ├── GEOMETRY_RECOVERY_PLAN.md
│   ├── GLB_EXPORT_ARCHITECTURE.md
│   ├── GOAL_TRACKER.md
│   ├── IMPLEMENTATION_GATES.md
│   ├── KICAD_10_0_5_FILTER_AUDIT.md
│   ├── KICAD_10_0_7_3D_EXPORT_BEHAVIOR.md
│   ├── KICAD_10_0_7_BEHAVIORAL_EVIDENCE.md
│   ├── KICAD_3D_QUALITY_GATES.md
│   ├── KICAD_CLI_BEHAVIOR.md
│   ├── KICAD_CLI_PARITY_MATRIX.md
│   ├── KICAD_COMPLETION_GATES.md
│   ├── KICAD_FOOTPRINT_TYPE_FILTER.md
│   ├── KICAD_GLB_REFERENCE.md
│   ├── LOCAL_OUTPUT_LOCATIONS.md
│   ├── MODEL_ASSIGNMENTS.md
│   ├── MODEL_RESOLUTION.md
│   ├── NATIVE_FORMAT_ARCHITECTURE.md
│   ├── NXP_REALISM_UPGRADE.md
│   ├── PCB_FIXTURE_CORPUS.md
│   ├── PCB_POSITION_EXPORT.md
│   ├── PCB_TOPOLOGY.md
│   ├── PERSONAL_AI_HANDOFF.md
│   ├── PR9_CI_DIAGNOSIS.md
│   ├── PRODUCT_COMPLETION_PLAN.md
│   ├── PROJECT_EXPORT_BUNDLE.md
│   ├── PROJECT_INTAKE_AND_FOOTPRINT.md
│   ├── REA_KICAD_CLI_10_0_5.md
│   ├── REPOSITORY_MAP.md
│   ├── SELECTABLE_PROTOTYPE.md
│   ├── SELECTABLE_PROTOTYPE_QA.md
│   ├── STEP_FACE_2875_ANALYSIS.md
│   ├── STEP_FACE_COVERAGE.md
│   ├── STEP_LENGTH_UNIT_PREFLIGHT.md
│   ├── STEP_PLANAR_REPAIR.md
│   ├── STEP_SOLID_REFERENCE.md
│   ├── TEAM_HANDOFF.md
│   ├── THREED_PRESENTATION_STANDARD.md
│   ├── TOKITO_BOARD_PACKAGE_V0.md
│   └── ULTIMAKER_SELECTABLE_PROTOTYPE.md
├── examples/
│   ├── footprints/
│   │   ├── header-1x02-oval-offset.json
│   │   ├── header-1x02-through-hole.json
│   │   ├── mixed-smd-through-hole.json
│   │   ├── mounting-hole-3mm-npth.json
│   │   ├── r0603-reference.json
│   │   └── sot-23-to236ab.json
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
│   │   │   ├── surface_distance.rs
│   │   │   ├── torus_inverse_regression.rs
│   │   │   └── torus_surface_evidence.rs
│   │   ├── references/
│   │   │   ├── README.md
│   │   │   ├── audit_model16_physical_refinement.FCMacro
│   │   │   ├── audit_model16_projection.FCMacro
│   │   │   ├── compare_model16_trim_face.FCMacro
│   │   │   ├── esp8266-model-15-freecad-1.1.4.json
│   │   │   ├── esp8266-multi-model-freecad-manifest.json
│   │   │   ├── generate_multi_reference.FCMacro
│   │   │   └── generate_reference.FCMacro
│   │   ├── src/
│   │   │   ├── bin/
│   │   │   │   └── model16_spade_candidate.rs
│   │   │   ├── adaptive_boundary.rs
│   │   │   ├── candidate_mesh.rs
│   │   │   ├── exact_predicate.rs
│   │   │   ├── face_coverage.rs
│   │   │   ├── face_representation.rs
│   │   │   ├── main.rs
│   │   │   ├── negative_torus.rs
│   │   │   ├── occurrence_world.rs
│   │   │   ├── physical_edge_flip.rs
│   │   │   ├── physical_surface_refinement.rs
│   │   │   ├── planar_face.rs
│   │   │   ├── planar_repair.rs
│   │   │   ├── priority_quality.rs
│   │   │   ├── priority_refinement.rs
│   │   │   ├── seeded_domain.rs
│   │   │   ├── source_uncertainty.rs
│   │   │   ├── spline_face_diagnostic.rs
│   │   │   ├── surface_grid.rs
│   │   │   ├── triangle_diagnostics.rs
│   │   │   ├── trim_polygon_candidate.rs
│   │   │   └── unit_preflight.rs
│   │   ├── tests/
│   │   │   ├── bounded_fingerprint.py
│   │   │   ├── check_planar_receipt.py
│   │   │   ├── diagnose_degenerate_surface.py
│   │   │   ├── model16_face_diagnostic_e2e.py
│   │   │   ├── model16_source_face_trim_e2e.py
│   │   │   ├── multi_surface_reference_e2e.py
│   │   │   ├── negative_cli.rs
│   │   │   ├── physical_refinement_e2e.py
│   │   │   ├── planar_cli.rs
│   │   │   ├── planar_repair_reference_e2e.py
│   │   │   ├── projection_audit_e2e.py
│   │   │   ├── surface_reference_e2e.py
│   │   │   ├── test_solid_reference.py
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
│   ├── cli_doctor.rs
│   ├── cli_presentation.rs
│   ├── component_forge.rs
│   ├── component_pose.rs
│   ├── dataset.rs
│   ├── dataset_external.rs
│   ├── dataset_git.rs
│   ├── digital_twin.rs
│   ├── document.rs
│   ├── embedded.rs
│   ├── flat_output.rs
│   ├── footprint_family.rs
│   ├── footprint_inspect.rs
│   ├── footprint_library.rs
│   ├── footprint_spec.rs
│   ├── geometry.rs
│   ├── glb.rs
│   ├── gltf_bundle.rs
│   ├── lib.rs
│   ├── main.rs
│   ├── model_assembly.rs
│   ├── model_asset_export.rs
│   ├── model_assignments.rs
│   ├── model_mesh.rs
│   ├── model_resolver.rs
│   ├── package_input.rs
│   ├── population_audit.rs
│   ├── pos_export.rs
│   ├── project_bundle.rs
│   ├── project_intake.rs
│   ├── source_candidate.rs
│   ├── strict_json.rs
│   ├── topology.rs
│   └── twin_viewer.rs
├── tests/
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
│   │   ├── embedded-models/
│   │   │   ├── README.md
│   │   │   └── step-sample.zst
│   │   ├── kicad-10.0.5/
│   │   │   ├── README.md
│   │   │   ├── R_0603_1608Metric.kicad_mod
│   │   │   └── r0603-reference-geometry.json
│   │   ├── kicad-footprint-libraries/
│   │   │   └── f8a45b4665c28a6280e46eefe9fd4df81d2cd17b/
│   │   │       ├── README.md
│   │   │       └── SOT-23.kicad_mod
│   │   └── open-source/
│   │       ├── arduino-uno-r3-community/
│   │       │   ├── Arduino UNO.kicad_pcb
│   │       │   └── LICENSE.txt
│   │       ├── kicad-resistor-models/
│   │       │   ├── Resistor_SMD.3dshapes/
│   │       │   │   ├── R_0603_1608Metric.wrl
│   │       │   │   └── R_0603_1608Metric.wrl.zst
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
│   ├── reference/
│   │   ├── kicad-cli-10.0.5/
│   │   │   ├── glb-no-dnp/
│   │   │   │   └── manifest.json
│   │   │   ├── pos/
│   │   │   │   ├── manifest.json
│   │   │   │   ├── nxp-mm-back.csv
│   │   │   │   ├── nxp-mm-both.csv
│   │   │   │   ├── nxp-mm-bottom-negate.csv
│   │   │   │   ├── nxp-mm-front.csv
│   │   │   │   ├── pico-mm-drill-origin-bottom-negate.csv
│   │   │   │   ├── pico-mm-drill-origin.csv
│   │   │   │   ├── spark-mm-both.csv
│   │   │   │   ├── spark-mm-drill-origin.csv
│   │   │   │   ├── spark-mm-exclude-fp-th.csv
│   │   │   │   ├── uno-in-both.csv
│   │   │   │   ├── uno-mm-both.csv
│   │   │   │   ├── uno-mm-exclude-dnp.csv
│   │   │   │   ├── uno-mm-exclude-fp-th.csv
│   │   │   │   └── uno-mm-smd.csv
│   │   │   ├── pos-ascii/
│   │   │   │   ├── manifest.json
│   │   │   │   ├── nxp-mm-back.pos
│   │   │   │   ├── nxp-mm-bottom-negate.pos
│   │   │   │   ├── nxp-mm-exclude-dnp.pos
│   │   │   │   ├── nxp-mm-front.pos
│   │   │   │   ├── pico-mm-drill-origin-bottom-negate.pos
│   │   │   │   ├── spark-mm-drill-origin.pos
│   │   │   │   ├── spark-mm-exclude-fp-th.pos
│   │   │   │   ├── uno-in-both.pos
│   │   │   │   └── uno-mm-both.pos
│   │   │   └── svg/
│   │   │       ├── manifest.json
│   │   │       ├── uno-fab-default.svg
│   │   │       └── uno-fab-hide-dnp.svg
│   │   └── kicad-cli-10.0.7/
│   │       ├── pos/
│   │       │   ├── manifest.json
│   │       │   ├── nxp-mm-back.csv
│   │       │   ├── nxp-mm-both.csv
│   │       │   ├── nxp-mm-bottom-negate.csv
│   │       │   ├── nxp-mm-front.csv
│   │       │   ├── pico-mm-drill-origin-bottom-negate.csv
│   │       │   ├── pico-mm-drill-origin.csv
│   │       │   ├── spark-mm-both.csv
│   │       │   ├── spark-mm-drill-origin.csv
│   │       │   ├── spark-mm-exclude-fp-th.csv
│   │       │   ├── uno-in-both.csv
│   │       │   ├── uno-mm-both.csv
│   │       │   ├── uno-mm-exclude-dnp.csv
│   │       │   ├── uno-mm-exclude-fp-th.csv
│   │       │   └── uno-mm-smd.csv
│   │       ├── svg/
│   │       │   ├── manifest.json
│   │       │   ├── pcb-export-svg-help.txt
│   │       │   ├── uno-fab-default.svg
│   │       │   ├── uno-fab-hide-dnp.svg
│   │       │   └── version.txt
│   │       └── type-filter/
│   │           └── manifest.json
│   ├── support/
│   │   └── test_scratch.rs
│   ├── altium_source_e2e.py
│   ├── bom_api.rs
│   ├── bom_e2e.py
│   ├── cli_experience_e2e.py
│   ├── command_completion_e2e.py
│   ├── compat_v0.py
│   ├── component_dataset_e2e.py
│   ├── component_envelope_e2e.py
│   ├── component_pose.rs
│   ├── dataset_e2e.py
│   ├── dataset_git_e2e.py
│   ├── dataset_twin_e2e.py
│   ├── e2e_cli.py
│   ├── e2e_temp.py
│   ├── embedded_dataset_e2e.py
│   ├── embedded_model_cli.rs
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
│   ├── footprint_inspect_e2e.py
│   ├── footprint_spec_e2e.py
│   ├── footprint_through_hole_kicad_e2e.py
│   ├── geometry_e2e.py
│   ├── glb_api.rs
│   ├── glb_e2e.py
│   ├── gltf_bundle_cli.rs
│   ├── kicad_dnp_glb_reference.py
│   ├── kicad_glb_reference.py
│   ├── kicad_pos_compat_e2e.py
│   ├── kicad_svg_reference_evidence.py
│   ├── kicad_type_filter_e2e.py
│   ├── model_assignments_api.rs
│   ├── model_assignments_e2e.py
│   ├── model_mesh_api.rs
│   ├── model_resolution_e2e.py
│   ├── model_resolver_api.rs
│   ├── nxp_refinement_e2e.py
│   ├── output_paths_audit.py
│   ├── presentation_browser.js
│   ├── project_bundle_e2e.py
│   ├── project_intake_e2e.py
│   ├── prototype_browser.js
│   ├── run_presentation_browser.cjs
│   ├── source_candidate_e2e.py
│   ├── step_candidate_mesh_e2e.py
│   ├── step_face_coverage_e2e.py
│   ├── step_occurrence_world_e2e.py
│   ├── step_units_e2e.py
│   ├── test_e2e_temp.py
│   ├── test_embedded_dataset_e2e.py
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
