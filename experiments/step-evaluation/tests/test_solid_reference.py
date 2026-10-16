"""Development-reference ownership regressions; these are not CAD fixtures."""

import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import multi_surface_reference_e2e as multi
import diagnose_degenerate_surface as diagnostic


def adjacent_tetrahedra():
    points = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]]
    first = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]
    second = [[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]]
    return {
        "schema": "tokito.freecad-reference-mesh.v2",
        "solidCount": 2,
        "verticesMm": points + [[0, 0, -1]],
        "triangles": first + [[4 if index == 3 else index for index in row] for row in second],
        "solids": [
            {"solidIndex": 0, "valid": True, "faceCount": 4,
             "verticesMm": copy.deepcopy(points), "triangles": copy.deepcopy(first)},
            {"solidIndex": 1, "valid": True, "faceCount": 4,
             "verticesMm": points[:3] + [[0, 0, -1]], "triangles": second},
        ],
    }


class SolidReferenceTests(unittest.TestCase):
    def test_shared_coordinates_preserve_strict_independent_solids(self):
        points, triangles, evidence = multi.checked_solid_reference(adjacent_tetrahedra(), 2)
        self.assertEqual((len(points), len(triangles)), (8, 8))
        self.assertTrue(evidence["allReferenceSolidsClosedAndConsistentlyOriented"])
        compound = evidence["compoundReferenceIntegrity"]
        self.assertFalse(compound["closed"])
        self.assertEqual(compound["boundaryEdgeCount"], 0)
        self.assertEqual(compound["nonManifoldEdgeCount"], 3)
        self.assertEqual(compound["edgeIncidenceHistogram"], {2: 6, 4: 3})
        self.assertEqual(compound["directionImbalanceEdgeCount"], 0)
        self.assertAlmostEqual(evidence["referenceSolidVolumeSumMm3"], 1 / 3)
        self.assertEqual([row["solidIndex"] for row in evidence["referenceSolidIntegrity"]], [0, 1])
        self.assertTrue(all(index >= 4 for index in triangles[-1]))

    def test_missing_solid_and_duplicate_or_nonordinal_identity_refuse(self):
        for change in ("missing", "duplicate", "wrong", "bool"):
            reference = adjacent_tetrahedra()
            if change == "missing":
                reference["solids"].pop()
            else:
                reference["solids"][1]["solidIndex"] = {"duplicate": 0, "wrong": 3, "bool": True}[change]
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, "inventory"):
                multi.checked_solid_reference(reference, 2)

    def test_missing_face_fails_and_retains_both_solid_results(self):
        reference = adjacent_tetrahedra()
        reference["solids"][0]["triangles"].pop()
        _, _, evidence = multi.checked_solid_reference(reference, 2)
        rows = evidence["referenceSolidIntegrity"]
        self.assertFalse(evidence["allReferenceSolidsClosedAndConsistentlyOriented"])
        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[0]["boundaryEdgeCount"], 3)
        self.assertFalse(rows[0]["passed"])
        self.assertTrue(rows[1]["passed"])

    def test_entirely_reversed_solid_is_not_accepted_by_balanced_edges(self):
        reference = adjacent_tetrahedra()
        reference["solids"][1]["triangles"] = [row[::-1] for row in reference["solids"][1]["triangles"]]
        _, _, evidence = multi.checked_solid_reference(reference, 2)
        row = evidence["referenceSolidIntegrity"][1]
        self.assertTrue(row["closed"])
        self.assertTrue(row["consistentlyOriented"])
        self.assertFalse(row["outwardWinding"])
        self.assertFalse(row["passed"])

    def test_invalid_index_degenerate_and_nonfinite_solid_remain_visible(self):
        for change in ("index", "degenerate", "nonfinite"):
            reference = adjacent_tetrahedra()
            solid = reference["solids"][0]
            if change == "index":
                solid["triangles"][0][0] = 99
            elif change == "degenerate":
                solid["triangles"][0] = [0, 0, 1]
            else:
                solid["verticesMm"][0][0] = float("nan")
            with self.subTest(change=change):
                _, _, evidence = multi.checked_solid_reference(reference, 2)
                self.assertEqual(len(evidence["referenceSolidIntegrity"]), 2)
                self.assertFalse(evidence["referenceSolidIntegrity"][0]["passed"])
                self.assertIn("error", evidence["referenceSolidIntegrity"][0])
                self.assertTrue(evidence["referenceSolidIntegrity"][1]["passed"])
                self.assertFalse(evidence["referenceSolidVolumeSumComplete"])
                self.assertIsNone(evidence["referenceSolidVolumeSumMm3"])

    def test_aggregate_budget_is_shared_across_solids(self):
        with patch.object(multi, "MAX_REFERENCE_VERTICES", 7):
            with self.assertRaisesRegex(ValueError, "aggregate"):
                multi.checked_solid_reference(adjacent_tetrahedra(), 2)

    def test_legacy_compound_cannot_be_used_as_per_solid_evidence(self):
        reference = adjacent_tetrahedra()
        reference["schema"] = "tokito.freecad-reference-mesh.v1"
        with self.assertRaisesRegex(ValueError, "legacy compound membership is unknown"):
            multi.checked_solid_reference(reference, 2)

    def test_diagnostic_writes_failed_receipt_with_every_solid_result(self):
        # Unit-level injection tests receipt handling, not native CAD conversion.
        with tempfile.TemporaryDirectory(prefix="tokito-reference-test-") as directory:
            root = Path(directory)
            binary = root / "unit-evaluator"
            binary.write_bytes(b"unit-only evaluator identity")
            manifest = {"schema": "tokito.esp8266-step-reference-manifest.v1",
                        "datasetCommit": "5283bc059e30e51f4d3a81a674425936fc213214",
                        "freecadVersion": ["unit-only"], "tessellationDeflectionMm": 0.01,
                        "models": []}
            for name in ("0.step", "14.step", "16.step"):
                source = root / name
                source.write_bytes(b"unit-only source identity")
                digest = hashlib.sha256(source.read_bytes()).hexdigest()
                manifest["models"].append({"file": name, "sourceSha256": digest,
                                           "solidCount": 2, "faceCount": 8})
                reference = adjacent_tetrahedra()
                reference["solids"][0]["triangles"].pop()
                reference.update({"inputSha256": digest, "datasetCommit": manifest["datasetCommit"],
                                  "modelFile": name, "freecadVersion": manifest["freecadVersion"],
                                  "deflectionMm": 0.01, "faceCount": 8, "valid": True})
                (root / (Path(name).stem + ".freecad.json")).write_text(json.dumps(reference), encoding="utf-8")
            manifest_path = root / "manifest.json"
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

            def unit_capture(command, **_kwargs):
                tetra = adjacent_tetrahedra()["solids"][0]
                candidate = {"sourceSha256": hashlib.sha256(Path(command[-1]).read_bytes()).hexdigest(),
                             "admitted": False, "exportsProduced": [],
                             "candidateMeshData": {"readerWarnings": [], "geometryQualified": False,
                                 "pcbPlacementQualified": False, "occurrences": [{"shapes": [{
                                     "verticesMm": tetra["verticesMm"], "triangles": tetra["triangles"],
                                     "sourceTriangleFaceIds": [1, 2, 3, 4]}]}]}}
                return 2, json.dumps(candidate).encode(), b""

            output = root / "diagnostic.json"
            argv = ["diagnostic", "--binary", str(binary), "--models-root", str(root),
                    "--references-root", str(root), "--output", str(output)]
            with patch.object(multi, "DEFAULT_MANIFEST", manifest_path), \
                    patch.object(multi.surface, "capture_bounded", side_effect=unit_capture), \
                    patch.object(sys, "argv", argv), patch("builtins.print"):
                with self.assertRaises(SystemExit) as caught:
                    diagnostic.main()
            self.assertEqual(caught.exception.code, 2)
            receipt = json.loads(output.read_text(encoding="utf-8"))
            self.assertFalse(receipt["diagnosticCompleted"])
            self.assertFalse(receipt["productionStepAdmitted"])
            self.assertEqual(len(receipt["models"]), 3)
            for row in receipt["models"]:
                self.assertEqual(row["status"], "failed")
                self.assertEqual(len(row["referenceSolidIntegrity"]), 2)
                self.assertFalse(row["referenceSolidIntegrity"][0]["passed"])
                self.assertTrue(row["referenceSolidIntegrity"][1]["passed"])
                self.assertIn("referenceSha256", row)


if __name__ == "__main__":
    unittest.main()
