# Pinned STEP reference mesh

`esp8266-model-15-freecad-1.1.4.json` is a small, independently evaluated
FreeCAD tessellation for one STEP asset in the Tokito dataset sample.

- Dataset source revision: `TokitoAI/tokito-app-dataset` commit
  `5283bc059e30e51f4d3a81a674425936fc213214`.
- Source path: ESP8266 NodeMCU reference, `Models/15` / `embedded-models/15.step`.
- Source SHA-256: `2af97f10d6f1dad4aa9486f40471d2c5b6be941c93b906735515e591b059a7ca`.
- Reference: FreeCAD 1.1.4, commit
  `4fd3bf320d9566a27e60069fc8387448aaa3a094`, tessellation deflection 0.001 mm.
- Reference SHA-256:
  `7df0f74104bd80138ecb04467255727d83e41db4de20e4315cd4fc62dbb69eca`.

`generate_reference.FCMacro` reproduces the fixture from the exact FreeCAD
build and source SHA above. The macro fails closed for a different FreeCAD
version, source, invalid B-rep, or anything other than one solid. Regeneration
was independently rerun on 2026-10-15; its raw JSON SHA-256 matched the pinned
fixture exactly.

The fixture and `tests/surface_reference_e2e.py` support a bounded sampled
geometry comparison only. The STEP evaluator still exits with refusal status 2;
this reference does not identify a board designator, verify PCB placement, or
authorize production STEP-to-GLB export.
