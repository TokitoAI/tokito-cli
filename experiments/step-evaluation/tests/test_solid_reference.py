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
from bounded_fingerprint import fingerprint
import planar_repair_reference_e2e as planar_reference


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
    def test_emitted_shape_face_provenance_gate_rejects_partial_or_promoted_evidence(self):
        row = {"representationItemId": 10, "sourceTraversalComplete": True,
               "expectedSourceFaces": 2, "emittedDistinctSourceFaces": 2,
               "missingSourceFaceCount": 0, "unexpectedSourceFaceCount": 0,
               "unassignedTriangleEntries": 0, "triangleProvenanceCountMatches": True,
               "sourceFacePresenceMatches": True,
               "expectedSourceFaceIdsSha256": "a" * 64}
        report = {"sourceFaceCompletenessVerified": False,
                  "sourceFaceCoverage": {"shapeDetails": [row], "shapeDetailsTruncated": False,
                      "shapesInspected": 1, "unmatchedShapeFaceSets": 0,
                      "allEmittedShapeFaceSetsMatch": True,
                      "allSourceRepresentationItemsEnumerated": False,
                      "allSourceShapesCovered": False, "geometryQualified": False},
                  "emittedShapeFaceProvenanceAudit": {
                      "schema": "tokito.step-emitted-shape-face-provenance.v1",
                      "shapeDefinitionsInspected": 1,
                      "shapeDefinitionsWithUnmatchedFaceSets": 0,
                      "everyEmittedShapeDefinitionHasCompleteFaceProvenance": True,
                      "allSourceRepresentationItemsEnumerated": False,
                      "allSourceShapesCovered": False, "geometryQualified": False}}
        accepted = planar_reference.audit_emitted_shape_face_provenance(report)
        self.assertTrue(accepted["everyEmittedShapeDefinitionHasCompleteFaceProvenance"])
        self.assertFalse(accepted["allSourceShapesCovered"])
        self.assertFalse(accepted["geometryQualified"])

        mutations = [
            lambda value: value["sourceFaceCoverage"].update(shapeDetails=[]),
            lambda value: value["sourceFaceCoverage"].update(shapeDetailsTruncated=True),
            lambda value: value["sourceFaceCoverage"].update(unmatchedShapeFaceSets=1),
            lambda value: value["sourceFaceCoverage"].update(unmatchedShapeFaceSets=False),
            lambda value: value["sourceFaceCoverage"].update(allSourceRepresentationItemsEnumerated=True),
            lambda value: value["sourceFaceCoverage"].update(allSourceShapesCovered=True),
            lambda value: value["sourceFaceCoverage"].update(geometryQualified=True),
            lambda value: value["sourceFaceCoverage"]["shapeDetails"][0].update(missingSourceFaceCount=1),
            lambda value: value["sourceFaceCoverage"]["shapeDetails"][0].update(missingSourceFaceCount=False),
            lambda value: value["sourceFaceCoverage"]["shapeDetails"][0].update(unexpectedSourceFaceCount=1),
            lambda value: value["sourceFaceCoverage"]["shapeDetails"][0].update(unassignedTriangleEntries=1),
            lambda value: value["sourceFaceCoverage"]["shapeDetails"][0].update(triangleProvenanceCountMatches=False),
            lambda value: value["sourceFaceCoverage"]["shapeDetails"][0].update(sourceFacePresenceMatches=False),
            lambda value: value["sourceFaceCoverage"]["shapeDetails"][0].update(emittedDistinctSourceFaces=1),
            lambda value: value["sourceFaceCoverage"]["shapeDetails"][0].update(expectedSourceFaces=True),
            lambda value: value["sourceFaceCoverage"]["shapeDetails"][0].update(expectedSourceFaceIdsSha256="not-a-digest"),
            lambda value: value.update(sourceFaceCompletenessVerified=True),
            lambda value: value["emittedShapeFaceProvenanceAudit"].update(allSourceShapesCovered=True),
            lambda value: value["emittedShapeFaceProvenanceAudit"].update(allSourceRepresentationItemsEnumerated=True),
            lambda value: value["emittedShapeFaceProvenanceAudit"].update(geometryQualified=True),
            lambda value: value["emittedShapeFaceProvenanceAudit"].update(shapeDefinitionsInspected=True),
            lambda value: value["emittedShapeFaceProvenanceAudit"].update(shapeDefinitionsWithUnmatchedFaceSets=False),
        ]
        for mutate in mutations:
            candidate = copy.deepcopy(report)
            mutate(candidate)
            with self.subTest(mutation=mutate), self.assertRaises(ValueError):
                planar_reference.audit_emitted_shape_face_provenance(candidate)

    def test_planar_comparison_missing_input_keeps_failed_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            result=planar_reference.run(root/"missing",root/"source",root/"reference","0"*64,root/"output")
            self.assertFalse(result["passed"])
            self.assertIn("FileNotFoundError",result["error"])
            self.assertFalse(json.loads((root/"output/receipt.json").read_text())["passed"])
    def test_exact_point_cache_preserves_every_surface_sample(self):
        mesh=adjacent_tetrahedra()["solids"][0]
        points,triangles=mesh["verticesMm"],mesh["triangles"]
        samples=list(multi.multi_surface_samples(points,triangles,multi.selected_triangle_indices(len(triangles))))
        for target in [points,[[x+2,y+3,z+4] for x,y,z in points]]:
            tree=multi.TriangleBvh(target,triangles)
            uncached_work=[0]
            distances=[tree.nearest_distance(p,uncached_work) for p in samples]
            cached_work=[0]
            result=multi.directed_bvh_sample_distance(points,triangles,target,triangles,cached_work)
            self.assertEqual(result[:3],(max(distances),sum(distances)/len(distances),len(samples)))
            self.assertLess(cached_work[0],uncached_work[0])
    def test_fingerprints_stream_and_enforce_exact_byte_limits(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/"data"
            path.write_bytes(b"abc")
            self.assertEqual(fingerprint(path,3),hashlib.sha256(b"abc").hexdigest())
            with self.assertRaises(ValueError):
                fingerprint(path,2)
            with self.assertRaises(ValueError):
                fingerprint(path,-1)
            with self.assertRaises(FileNotFoundError):
                fingerprint(Path(temporary)/"missing",3)
            path.write_bytes(b"")
            self.assertEqual(fingerprint(path,0),hashlib.sha256(b"").hexdigest())
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
