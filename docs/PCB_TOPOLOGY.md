# Physical PCB outline topology

Milestone B adds a physical Edge.Cuts report alongside the existing v0.1
inspection and rendering projections. Its Rust implementation is
`src/topology.rs`; public consumers access it through
`PhysicalBoardDocument::outline()` and `outline_json()`. The separate CLI command
`tokito-cli validate outline <BOARD.kicad_pcb|TOKITO_PACKAGE_DIRECTORY> [--json]`
prints this report. Invalid input returns exit code 2, including with `--json`;
the JSON report is printed before the CLI error. The existing `inspect --json`
schema is not changed.

The report expresses source coordinates in millimetres with KiCad's saved
board-frame Y-down convention. A contour is made from supported source line,
three-point arc, circle, rectangle or polygon graphics on Edge.Cuts. Component
local graphics use the saved component pose once; the bottom-view mirror is
never applied to physical coordinates. Source locators and UUIDs remain attached
to primitives and diagnostics. Rectangle and polygon source stroke/fill do not
change physical centerlines or bounds. Invalid outlines have no certified
contours/bounds and cannot pass the SVG physical-outline gate.

Endpoint joins use Euclidean distance at or below `1e-6 mm` (1 nanometre). The
engine does not quantize coordinates or transitively weld a chain of endpoints.
Multiple possible partners are ambiguous. Positive overlap/intersection is
checked as geometry independently from join tolerance. Near-parallel ambiguity
also requires proximity of the finite projected intervals; distant support
lines alone do not establish a segment contact. Arc intersections,
extrema, signed area and containment use analytic circle calculations; no polygon
tessellation is used for acceptance.

The present boundary is intentionally conservative: up to 4096 normalized
primitives, two million bounded pair-work operations, at most 200 diagnostics
plus an explicit truncation diagnostic, and coordinates/radii no larger than
1,000,000 mm. It accepts one outer contour with directly contained cutouts. Open,
degenerate, branching, self-crossing, overlapping, touching/ambiguous,
disconnected, or island nesting is rejected. Unknown/malformed Edge.Cuts objects,
Edge.Cuts pads/properties/zones, and legacy module-owned boundary geometry fail
closed with source diagnostics instead of disappearing from a supposedly valid
boundary. These limits and accepted records are an adapter contract, not a
statement of complete KiCad semantics.

`export svg` continues to make a partial drawing. It requires the physical
outline gate to pass, uses physical Edge.Cuts arc math and centerline view bounds,
and leaves non-Edge.Cuts drawing projection and v0.1 inventory bytes compatible.
The SVG viewport includes rendered outline stroke and its existing display
margin; centerline-only physical bounds are reported separately by the API.
The topology API itself does not perform DRC, copper connectivity, fabrication
validation, triangulation, or 3D model resolution. A separate opt-in GLB exporter
uses a valid report to triangulate/extrude a board shell; it does not add component
models. STEP is unimplemented. Source inventory and v0.1 packages with a
physically invalid outline can still be imported and integrity-verified; their
SVG and GLB exports are refused.

Invalid reports retain source primitives and diagnostics for investigation but
clear derived certified bounds/contours, including unsupported records detected
after normalization and invalid contour nesting/disconnection.

## Evidence

The synthetic/public regression suite is `tests/topology_api.rs`; module tests
are in `src/topology.rs`. The real CLI and independent reference journey is
`tests/geometry_e2e.py`:

- Porcupine: 37 normalized Edge.Cuts primitives (32 outer edges and five
  circles), one outer contour, five cutouts, 607 routed copper arcs, two SVG
  views, source identities and deterministic output.
- KiCad RoyalBlue54L NFC demo: 22 reference-matched outline entities from the
  stored Gerber, 12 arcs and 10 lines. Maximum endpoint error measured by the
  test was about `2.84e-14 mm`; maximum source-vs-Gerber arc-center error was
  `2.182e-6 mm`, below the stated `5e-6 mm` tolerance. The original board has
  approximately `0.0035355 mm` open endpoint gaps and must remain rejected.

- Original Opulo LumenPnP vacuum adapter: unchanged official PCB and stored
  KiCad 6.0.1 profile at commit `11f17018e61c1513400680852bd1f34afb207b3c`.
  Eight entities include four lines, two arcs and two footprint-owned circles.
  The E2E additionally compares actual SVG path/circle geometry directly with
  the external Gerber, including radius/sweep and negative corrupted outputs.
  This is vector geometry evidence, not raster rendering or full plot fidelity.

Fixture pinning, license notices, exact digests and feature boundaries are in
[`PCB_FIXTURE_CORPUS.md`](PCB_FIXTURE_CORPUS.md). Historical inventory/SVG/package
compatibility is checked separately by `tests/compat_v0.py` and the retained
pre-A executable comparison in `target/milestone-a-baseline/`.
