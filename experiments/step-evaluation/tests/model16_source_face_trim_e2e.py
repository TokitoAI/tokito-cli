"""Digest-pinned source-face trim diagnosis for the ESP8266 model-16 asset."""
import argparse
import json
import math
from pathlib import Path

from bounded_fingerprint import fingerprint
from surface_reference_e2e import capture_bounded

SOURCE_SHA256 = "7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f"
EXPECTED_ORIENTED_EDGES = [2809, 470, 7109, 4933]
MAX_EDGE_CURVE_CONTROL_POINT_STORAGE_BUDGET = 65_536
EXPECTED_EDGE_SENSES = [
    (2809, True, True, True),
    (470, True, True, True),
    (7109, True, True, True),
    (4933, False, True, False),
]


def run(binary: Path, source: Path, output: Path) -> dict:
    if output.exists():
        raise ValueError("output must be new")
    output.mkdir(parents=True)
    tracked = [
        (binary, 64 * 1024 * 1024),
        (source, 32 * 1024 * 1024),
        (Path(__file__), 1024 * 1024),
        (Path(__file__).with_name("bounded_fingerprint.py"), 1024 * 1024),
        (Path(__file__).with_name("surface_reference_e2e.py"), 2 * 1024 * 1024),
    ]
    before = {str(path): fingerprint(path, limit) for path, limit in tracked}
    receipt = {
        "schema": "tokito.step-model16-source-face-trim-e2e.v1",
        "passed": False,
        "admitted": False,
        "exportsProduced": [],
        "inputSha256": before,
        "scope": (
            "pinned source B-spline face/trim diagnostics and bounded projection observations; "
            "no repair, export, independent geometry qualification, or production admission"
        ),
    }
    try:
        if before[str(source)] != SOURCE_SHA256:
            raise ValueError("pinned model-16 source digest mismatch")
        code, stdout, stderr = capture_bounded(
            [str(binary), "--diagnose-source-face", "2875", str(source)],
            timeout_seconds=300,
            stdout_limit=8 * 1024 * 1024,
            stderr_limit=512 * 1024,
        )
        (output / "diagnostic.json").write_bytes(stdout)
        if code != 2 or stderr:
            raise ValueError("diagnostic must refuse admission with exit 2 and no stderr")
        report = json.loads(stdout)
        surface = report.get("surface", {})
        projection = report.get("trimProjection", {})
        evaluation = report.get("evaluation", {})
        trim_candidate = report.get("trimCandidate", {})
        edges = report.get("sourceOrientedEdgeIds", [])
        edge_senses = [
            (
                row.get("orientedEdgeId"),
                row.get("orientedEdgeSense"),
                row.get("edgeCurveSameSense"),
                row.get("effectiveLoopSense"),
            )
            for row in report.get("edges", [])
        ]
        receipt["summary"] = {
            "faceId": report.get("faceId"),
            "surfaceId": report.get("surfaceId"),
            "boundId": report.get("boundId"),
            "loopId": report.get("loopId"),
            "orientedEdgeIds": edges,
            "sourceBoundarySampleCount": projection.get("boundarySampleCount"),
            "readerInverseMaximumResidualSourceUnits": projection.get(
                "readerInverseMaximumResidualSourceUnits"
            ),
            "multiStartMaximumResidualSourceUnits": projection.get(
                "multiStartMaximumResidualSourceUnits"
            ),
            "sharedKernelInverseDistanceChecks": projection.get(
                "sharedKernelInverseDistanceChecks"
            ),
            "sharedKernelInverseGridDimensions": projection.get(
                "sharedKernelInverseGridDimensions"
            ),
            "sharedKernelInverseControlPointEvaluationEstimate": projection.get(
                "sharedKernelInverseControlPointEvaluationEstimate"
            ),
            "sourceEdgeCurveControlPointEvaluationEstimate": report.get(
                "sourceEdgeCurveControlPointEvaluationEstimate"
            ),
            "sourceEdgeCurveControlPointStorageCount": report.get(
                "sourceEdgeCurveControlPointStorageCount"
            ),
            "sourceEdgeCurveControlPointStorageBudget": report.get(
                "sourceEdgeCurveControlPointStorageBudget"
            ),
            "trimCandidateStatus": trim_candidate.get("status"),
            "trimCandidateVertexCount": trim_candidate.get("vertexCount"),
            "trimCandidateTriangleCount": trim_candidate.get("triangleCount"),
            "trimCandidateRefinementConverged": trim_candidate.get("refinementConverged"),
            "trimCandidateMaxSampledChordErrorSourceUnits": trim_candidate.get(
                "maxSampledChordErrorSourceUnits"
            ),
        }
        if report.get("sourceSha256") != SOURCE_SHA256:
            raise ValueError("CLI source digest mismatch")
        if report.get("admitted") is not False or report.get("exportsProduced") != []:
            raise ValueError("diagnostic unexpectedly admitted geometry or produced exports")
        if (
            report.get("faceId") != 2875
            or report.get("surfaceId") != 10410
            or report.get("boundId") != 8995
            or report.get("loopId") != 8007
            or edges != EXPECTED_ORIENTED_EDGES
            or edge_senses != EXPECTED_EDGE_SENSES
            or report.get("sourceOrientedEdgeLoopClosed") is not True
            or surface.get("entityType") != "RATIONAL_B_SPLINE_SURFACE_WITH_KNOTS"
            or surface.get("degrees") != [2, 9]
            or surface.get("controlNetShape") != [3, 18]
            or surface.get("vClosed") is not True
            or projection.get("boundarySampleCount") != 100
            or projection.get("sampleDetailsTruncated") is not False
            or projection.get("sharedKernelInverseDistanceChecks", 0) > projection.get(
                "sharedKernelInverseDistanceCheckBudget", -1
            )
            or projection.get("sharedKernelInverseControlPointEvaluationEstimate", 0)
            > projection.get("sharedKernelInverseControlPointEvaluationBudget", -1)
            or projection.get("diagnosticControlPointEvaluationCount", 0)
            > projection.get("diagnosticControlPointEvaluationBudget", -1)
            or report.get("sourceEdgeCurveControlPointEvaluationEstimate", 0)
            > report.get("sourceEdgeCurveControlPointEvaluationBudget", -1)
            or report.get("sourceEdgeCurveControlPointStorageBudget")
            != MAX_EDGE_CURVE_CONTROL_POINT_STORAGE_BUDGET
            or not isinstance(report.get("sourceEdgeCurveControlPointStorageCount"), int)
            or report.get("sourceEdgeCurveControlPointStorageCount", 0)
            > report.get("sourceEdgeCurveControlPointStorageBudget", -1)
            or projection.get("readerInverseMaximumResidualSourceUnits", 0) < 0.02
            or projection.get("multiStartMaximumResidualSourceUnits", float("inf")) > 1e-5
            or evaluation.get("repairAttempted") is not False
            or evaluation.get("independentReferenceComparisonPerformed") is not False
            or evaluation.get("geometryQualified") is not False
            or evaluation.get("admitted") is not False
            or evaluation.get("exportsProduced") != []
            or trim_candidate.get("status") != "candidate_only"
            or trim_candidate.get("vertexCount") != 3393
            or trim_candidate.get("triangleCount") != 6016
            or trim_candidate.get("refinementLevels") != 3
            or trim_candidate.get("refinementConverged") is not True
            or trim_candidate.get("refinementFailureReason") is not None
            or trim_candidate.get("sourceMeshModified") is not False
            or trim_candidate.get("geometryQualified") is not False
            or trim_candidate.get("admitted") is not False
            or trim_candidate.get("exportsProduced") != []
        ):
            raise ValueError("source-face diagnostic did not match pinned regression expectations")
        chord_error = trim_candidate.get("maxSampledChordErrorSourceUnits")
        chord_tolerance = trim_candidate.get("chordToleranceSourceUnits")
        if (
            not isinstance(chord_error, (int, float))
            or not math.isfinite(chord_error)
            or chord_error < 0
            or not isinstance(chord_tolerance, (int, float))
            or not math.isfinite(chord_tolerance)
            or chord_tolerance != 0.005
            or chord_error > chord_tolerance
        ):
            raise ValueError("pinned trim candidate failed its sampled chord refinement gate")
        if trim_candidate.get("status") == "candidate_only":
            vertices = trim_candidate.get("uvVertices", [])
            triangles = trim_candidate.get("triangles", [])
            surface_points = trim_candidate.get("surfacePointsSourceUnits", [])
            if (
                len(vertices) < 3
                or len(vertices) > 60_000
                or len(surface_points) != len(vertices)
                or not triangles
                or len(triangles) > 50_000
                or any(
                    len(triangle) != 3
                    or any(
                        not isinstance(index, int) or index < 0 or index >= len(vertices)
                        for index in triangle
                    )
                    for triangle in triangles
                )
            ):
                raise ValueError("research trim candidate topology is malformed")
        if any(fingerprint(path, limit) != before[str(path)] for path, limit in tracked):
            raise ValueError("source, binary, or test input changed during execution")
        receipt["diagnosticSha256"] = fingerprint(output / "diagnostic.json", 8 * 1024 * 1024)
        receipt["passed"] = True
    except Exception as error:  # Keep a machine-readable refusal receipt on failure.
        receipt["error"] = f"{type(error).__name__}: {error}"
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = run(args.binary, args.source, args.output)
    print(json.dumps({key: result.get(key) for key in ("passed", "error", "summary")}))
    raise SystemExit(0 if result["passed"] else 2)
