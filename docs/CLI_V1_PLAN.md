# Tokito CLI v1 requirements and release ledger

Updated 2026-10-16. CLI only: AR and YOLO are outside this release.
Version 1 is a target, not the current package version or a completion claim.
The implementation remains original Rust with pure Rust dependencies. KiCad
10.0.7 behavior, Library Tools, footprint and model libraries are references;
full command, generator and library parity is not complete.

## Quality requirements

| ID | Requirement / proof needed | Current evidence / next work |
| --- | --- | --- |
| V1-01 | Every implemented command has successful and refusal subprocess coverage | Audit found 39 operational leaves plus help/flags/aliases. Three optional-only paths receive a mandatory base test; alias/error review continues |
| V1-02 | Consistent CLI parsing, version identity and actionable errors | Fix trailing root options, duplicate SVG options, invalid-Unicode argv and hard-coded producer/executable versions |
| V1-03 | Dependable no-overwrite output with recoverable failures | Twin outputs now pre-serialize, write manifest last and conservatively roll back unchanged authored files on ordinary errors. Unit fault tests added; process-kill atomicity not claimed |
| V1-04 | Complete pinned KiCad command/export/error compatibility | Partial; issue #13 and the compatibility matrix retain unimplemented families/options |
| V1-05 | Footprint/Library Tools coverage and engineering standards | Partial; explicit SMD/TH/NPTH candidates and library inventory/hash locks exist. Family coverage, IPC calculations, KLC and manufacturer-qualified land patterns remain open (#24) |
| V1-06 | Accurate 3D bodies, terminals, contours, cuts and materials | Bounded VRML works; STEP reference ownership verified for 76 solids. Planar and rational-spline reader defects remain (#19) |
| V1-07 | Units, alignment, mounting plane, rotations and bottom transforms | Existing source/pose regressions; independent per-board dimensions and all-component geometry qualification still required |
| V1-08 | Separately selectable components and classification provenance | Existing UUID/ref/native-ID selectors; distinguish authored class claims, heuristic hints and unknowns. Never promote a name/photo guess to verified identity |
| V1-09 | Missing-component resolution | Inventory BOM/footprint/model discrepancies; resolve exact part/package/revision from approved local libraries or source-provenanced manufacturer/open assets; verify dimensions and pins before assignment; refuse or explicitly partial if unavailable |
| V1-10 | 4K output and animation | Require actual 3840x2160 renderer evidence separately from mesh/placement tolerances. GLB has no intrinsic resolution. Animation must preserve source transforms and selection IDs |
| V1-11 | Electrical/current visualization | User selected imported verified simulation results first. Require source netlist/hash, component/pin map, quantity units, solver/version, time axis and provenance; refuse revision/pin mismatches. Do not infer voltage/current from geometry or photos |
| V1-12 | PCB/breadboard/other views | Distinct physical layout adapters and pin connectivity contracts required. PCB coordinates cannot be reused as breadboard hole coordinates. Unsupported environments remain explicit |
| V1-13 | Release/toolchain/dependencies | Correct root MSRV to 1.87 after actual 1.85 dependency failure; verify separately from development experiment toolchain. Independent-machine Windows delivery and dependency/advisory review remain required |
| V1-14 | Scale and bounded resource behavior | Preserve input/triangle/path/recursion/work caps; test large bounded fixtures and refusal. No arbitrary-board throughput guarantee from small tests |

## Missing component workflow

1. Pin board source, BOM, revision and existing model library inventory.
2. Report unmatched reference, UUID/native identity, missing body and ambiguous
   package/model separately. A DNP footprint is not automatically a missing part.
3. Search exact manufacturer part/package first, then approved version-pinned
   open libraries. Record asset origin/hash, units, dimensions and restrictions.
4. Parameterize an original Rust body only from reviewed dimensions; pads,
   terminals, pin numbering and pin-one orientation must be supported by evidence.
5. Verify local-to-footprint-to-board transforms and independent dimensions,
   holes, height and collision checks under declared tolerances. Never silently
   stretch or mirror a model to fit a photograph.
6. Keep photo-based correspondence as a separate reviewed observation. Preserve
   explicit unresolved status; shell/cuboid substitution is not completion.
7. Re-export coverage and selectable identities; compare payload and geometry
   before admitting the replacement as qualified.

## Simulation and views architecture

A future simulation-result adapter consumes verified external result files.
Canonical electrical identity (nets, pins and component source IDs) is separate
from mesh selection and physical coordinates. A result stores quantity/unit and
time-domain or frequency-domain metadata, not a decorative flow-speed guess.
Current arrows represent signed conventional current using declared orientation.
Missing/unsupported simulation models suppress electrical claims. Transport to a
viewer must expose revision, scale, units, provenance and interpolation policy.
No native circuit solver is selected for the first implementation.

Animation and views are consumers of validated source/geometry/results. Exploded,
section, top/bottom and assembly views must preserve original transforms and
expose derived presentation transforms explicitly. A breadboard adapter needs
actual hole/pin topology and a separate placement stage. No production physical,
electrical or thermal correctness is established by a polished visualization.

## Delivery order and gates

1. Close implemented-command coverage and output/error reliability gaps.
2. Verify toolchain, dependencies, packaged Windows behavior and installation.
3. Repair/independently qualify STEP planar faces, then spline cases and full
   bodies; add per-component dimensional/placement checks.
4. Implement remaining agreed KiCad exports and footprint/library families with
   pinned comparisons. Keep unsupported commands explicit.
5. Add verified simulation-result intake and additional view/environment adapters
   after their formats and identity contracts are specified and tested.
6. Run current-source format, tests, Clippy, build, all-command E2E and independent
   review before each branch push. Keep Actions disabled. A production v1 tag
   requires all advertised v1 capabilities and delivery gates to pass; do not
   bump to 1.0.0 to imply missing work is complete.

See [release readiness](CLI_RELEASE_READINESS.md), [KiCad completion gates](KICAD_COMPLETION_GATES.md),
[compatibility matrix](KICAD_CLI_PARITY_MATRIX.md) and [STEP evidence](STEP_SOLID_REFERENCE.md).
