"""Digest-pinned source-face trim diagnosis for the ESP8266 model-16 asset."""
import argparse
import json
import math
from pathlib import Path

from bounded_fingerprint import fingerprint
from surface_reference_e2e import capture_bounded

SOURCE_SHA256 = "7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f"
SOURCE_REPRESENTATION_ID = 5957
SOURCE_CONTEXT_ID = 206
DECLARED_SOURCE_UNCERTAINTY = 1e-5
EXPECTED_ORIENTED_EDGES = [2809, 470, 7109, 4933]
MAX_EDGE_CURVE_CONTROL_POINT_STORAGE_BUDGET = 65_536
EXPECTED_EDGE_SENSES = [
    (2809, True, True, True),
    (470, True, True, True),
    (7109, True, True, True),
    (4933, False, True, False),
]


def is_json_number(value: object) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def uncertainty_residual_summary(rows: list[dict], limit: float, field: str) -> dict:
    if not isinstance(rows, list) or not rows:
        raise ValueError(f"missing projection samples for {field}")
    residuals = []
    for row in rows:
        value = row.get(field)
        if (
            not is_json_number(value)
            or not math.isfinite(float(value))
            or value < 0
        ):
            raise ValueError(f"invalid projection residual for {field}")
        residuals.append(float(value))
    return {
        "sampleCount": len(residuals),
        "withinDeclaredSourceUncertainty": sum(value <= limit for value in residuals),
        "maximumResidualSourceUnits": max(residuals),
        "declaredSourceUncertaintySourceUnits": limit,
        "usedAsAdmissionTolerance": False,
    }


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
            [
                str(binary),
                "--diagnose-source-face",
                "2875",
                "--source-representation",
                str(SOURCE_REPRESENTATION_ID),
                str(source),
            ],
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
        selected_representation = report.get("selectedSourceRepresentation", {})
        declared_uncertainty = report.get("declaredSourceGeometricUncertainty", {})
        boundary_uncertainty = uncertainty_residual_summary(
            trim_candidate.get("boundarySamples", []),
            DECLARED_SOURCE_UNCERTAINTY,
            "projectionResidualSourceUnits",
        )
        midpoint_uncertainty = uncertainty_residual_summary(
            trim_candidate.get("boundaryRefinementProbes", []),
            DECLARED_SOURCE_UNCERTAINTY,
            "projectionResidualSourceUnits",
        )
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
            "trimCandidateBoundarySampleCount": len(trim_candidate.get("boundarySamples", [])),
            "trimCandidateBoundaryTrace": trim_candidate.get("boundaryTrace"),
            "sourceDrivenBoundaryRefinementProbeCount": len(
                trim_candidate.get("boundaryRefinementProbes", [])
            ),
            "retainedBoundaryRefinementProbeCount": sum(
                probe.get("retainedInCandidate") is True
                for probe in trim_candidate.get("boundaryRefinementProbes", [])
            ),
            "uvProjectionConvergence": {
                "sourceBoundarySamplesConverged": sum(
                    sample.get("projectionConverged") is True
                    for sample in trim_candidate.get("boundarySamples", [])
                ),
                "sourceBoundarySamplesNotConverged": sum(
                    sample.get("projectionConverged") is False
                    for sample in trim_candidate.get("boundarySamples", [])
                ),
                "sourceEdgeMidpointProbesConverged": sum(
                    probe.get("projectionConverged") is True
                    for probe in trim_candidate.get("boundaryRefinementProbes", [])
                ),
                "sourceEdgeMidpointProbesNotConverged": sum(
                    probe.get("projectionConverged") is False
                    for probe in trim_candidate.get("boundaryRefinementProbes", [])
                ),
            },
            "declaredSourceUncertainty": {
                "representationId": selected_representation.get("representationId"),
                "contextId": declared_uncertainty.get("representationContextId"),
                "valueInContextUnits": declared_uncertainty.get("valueInContextUnits"),
                "uncertaintyUnitMetresPerUnit": declared_uncertainty.get(
                    "uncertaintyUnitMetresPerUnit"
                ),
                "usedAsAdmissionTolerance": declared_uncertainty.get(
                    "usedAsAdmissionTolerance"
                ),
                "boundaryProjectionResiduals": boundary_uncertainty,
                "midpointProjectionResiduals": midpoint_uncertainty,
            },
        }
        if report.get("sourceSha256") != SOURCE_SHA256:
            raise ValueError("CLI source digest mismatch")
        if report.get("admitted") is not False or report.get("exportsProduced") != []:
            raise ValueError("diagnostic unexpectedly admitted geometry or produced exports")
        if (
            selected_representation.get("representationId") != SOURCE_REPRESENTATION_ID
            or selected_representation.get("representationItemId") != 6218
            or selected_representation.get("selectedRepresentationContainsFace") is not True
            or selected_representation.get("selectedRepresentationLengthUnits", {}).get(
                "contextId"
            )
            != SOURCE_CONTEXT_ID
            or declared_uncertainty.get("schema")
            != "tokito.step-source-geometric-uncertainty.v1"
            or declared_uncertainty.get("uncertaintyMeasureId") != 7888
            or declared_uncertainty.get("unitId") != 5056
            or declared_uncertainty.get("valueInContextUnits")
            != DECLARED_SOURCE_UNCERTAINTY
            or declared_uncertainty.get("usedAsAdmissionTolerance") is not False
            or boundary_uncertainty["withinDeclaredSourceUncertainty"]
            != boundary_uncertainty["sampleCount"]
            or midpoint_uncertainty["withinDeclaredSourceUncertainty"]
            != midpoint_uncertainty["sampleCount"]
        ):
            raise ValueError(
                "selected source representation uncertainty or projection evidence changed"
            )
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
            mesh_points = trim_candidate.get("meshPointsSourceUnits", [])
            boundary_trace = trim_candidate.get("boundaryTrace", {})
            boundary_samples = trim_candidate.get("boundarySamples", [])
            boundary_probes = trim_candidate.get("boundaryRefinementProbes", [])
            exact_projection_tolerance = (
                surface.get("controlNetDiagonalSourceUnits", 0) * 1e-10
            )
            if (
                len(vertices) < 3
                or len(vertices) > 60_000
                or len(surface_points) != len(vertices)
                or len(mesh_points) != len(vertices)
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
            edge_use_counts = {}
            edge_directions = {}
            mesh_adjacency = [set() for _ in vertices]
            for triangle in triangles:
                for a, b in ((triangle[0], triangle[1]), (triangle[1], triangle[2]), (triangle[2], triangle[0])):
                    edge = (min(a, b), max(a, b))
                    edge_use_counts[edge] = edge_use_counts.get(edge, 0) + 1
                    directions = edge_directions.setdefault(edge, [0, 0])
                    directions[0 if a < b else 1] += 1
                    mesh_adjacency[a].add(b)
                    mesh_adjacency[b].add(a)
            boundary_edges = [edge for edge, count in edge_use_counts.items() if count == 1]
            boundary_degrees = {}
            boundary_adjacency = {}
            for a, b in boundary_edges:
                boundary_degrees[a] = boundary_degrees.get(a, 0) + 1
                boundary_degrees[b] = boundary_degrees.get(b, 0) + 1
                boundary_adjacency.setdefault(a, set()).add(b)
                boundary_adjacency.setdefault(b, set()).add(a)
            topology = {
                "vertices": len(vertices),
                "edges": len(edge_use_counts),
                "triangles": len(triangles),
                "boundaryEdges": len(boundary_edges),
                "boundaryVertices": len(boundary_degrees),
                "nonManifoldEdges": sum(count > 2 for count in edge_use_counts.values()),
                "inconsistentInteriorEdges": sum(
                    count == 2 and edge_directions[edge] != [1, 1]
                    for edge, count in edge_use_counts.items()
                ),
                "boundaryVerticesWithDegreeOtherThanTwo": sum(degree != 2 for degree in boundary_degrees.values()),
                "eulerCharacteristic": len(vertices) - len(edge_use_counts) + len(triangles),
            }
            seen = set()
            mesh_components = 0
            for start in range(len(vertices)):
                if start in seen:
                    continue
                mesh_components += 1
                seen.add(start)
                stack = [start]
                while stack:
                    current = stack.pop()
                    for neighbour in mesh_adjacency[current] - seen:
                        seen.add(neighbour)
                        stack.append(neighbour)
            seen_boundary = set()
            boundary_components = 0
            for start in boundary_adjacency:
                if start in seen_boundary:
                    continue
                boundary_components += 1
                seen_boundary.add(start)
                stack = [start]
                while stack:
                    current = stack.pop()
                    for neighbour in boundary_adjacency[current] - seen_boundary:
                        seen_boundary.add(neighbour)
                        stack.append(neighbour)
            topology["connectedComponents"] = mesh_components
            topology["boundaryComponents"] = boundary_components
            receipt["summary"]["candidateMeshTopology"] = topology
            if topology != {
                "vertices": 3393,
                "edges": 9408,
                "triangles": 6016,
                "boundaryEdges": 768,
                "boundaryVertices": 768,
                "nonManifoldEdges": 0,
                "inconsistentInteriorEdges": 0,
                "boundaryVerticesWithDegreeOtherThanTwo": 0,
                "eulerCharacteristic": 1,
                "connectedComponents": 1,
                "boundaryComponents": 1,
            }:
                raise ValueError("candidate mesh incidence or connected topology changed")
            if (
                boundary_trace.get("sampleCount") != projection.get("boundarySampleCount")
                or boundary_trace.get("sourceSampleOrderPreserved") is not True
                or boundary_trace.get("candidateLoopClosureUv") is None
                or not isinstance(boundary_samples, list)
                or len(boundary_samples) != projection.get("boundarySampleCount")
            ):
                raise ValueError("candidate boundary provenance is missing or incomplete")
            if not isinstance(boundary_probes, list) or len(boundary_probes) != 1440:
                raise ValueError("source-driven boundary refinement probe count changed")
            retained_probe_count = 0
            discarded_probe_count = 0
            for probe in boundary_probes:
                vertex_index = probe.get("candidateVertexIndex")
                probe_uv = probe.get("candidateUv")
                source_point = probe.get("sourcePointSourceUnits")
                surface_point = probe.get("surfacePointSourceUnits")
                mesh_point = probe.get("meshPointSourceUnits")
                residual = probe.get("surfaceResidualSourceUnits")
                projection_residual = probe.get("projectionResidualSourceUnits")
                projection_converged = probe.get("projectionConverged")
                if (
                    probe.get("orientedEdgeId") not in EXPECTED_ORIENTED_EDGES
            or not is_json_number(probe.get("curveParameter"))
                    or not math.isfinite(float(probe["curveParameter"]))
                    or not isinstance(probe_uv, list)
                    or len(probe_uv) != 2
                    or not isinstance(source_point, list)
                    or len(source_point) != 3
                    or not isinstance(surface_point, list)
                    or len(surface_point) != 3
                    or not isinstance(mesh_point, list)
                    or len(mesh_point) != 3
                    or not isinstance(vertex_index, int)
                    or vertex_index < 0
                    or not isinstance(probe.get("retainedInCandidate"), bool)
                    or not is_json_number(residual)
                    or not math.isfinite(float(residual))
                    or residual < 0
                    or residual > 1e-5
                    or not is_json_number(projection_residual)
                    or not math.isfinite(float(projection_residual))
                    or projection_residual < 0
                    or projection_residual > 1e-5
                    or not isinstance(projection_converged, bool)
                    or (
                        projection_converged
                        and projection_residual > exact_projection_tolerance + 1e-14
                    )
                    or any(
                        not is_json_number(value) or not math.isfinite(float(value))
                        for value in [*probe_uv, *source_point, *surface_point, *mesh_point]
                    )
                ):
                    raise ValueError("source-driven boundary refinement probe is malformed")
                if probe["retainedInCandidate"]:
                    retained_probe_count += 1
                    if not 0 <= vertex_index < len(vertices):
                        raise ValueError("retained boundary probe is outside the final mesh")
                    if any(
                        abs(float(probe_uv[axis]) - float(vertices[vertex_index][axis])) > 1e-14
                        for axis in range(2)
                    ) or any(
                        abs(float(surface_point[axis]) - float(surface_points[vertex_index][axis]))
                        > 1e-14
                        for axis in range(3)
                    ) or any(
                        abs(float(mesh_point[axis]) - float(mesh_points[vertex_index][axis]))
                        > 1e-14
                        for axis in range(3)
                    ):
                        raise ValueError("retained boundary probe maps to the wrong mesh vertex")
                else:
                    discarded_probe_count += 1
                    if vertex_index < len(vertices):
                        raise ValueError("discarded boundary probe unexpectedly appears in the final mesh")
            if retained_probe_count == 0 or discarded_probe_count == 0:
                raise ValueError("boundary refinement must report retained and final-pass probes")
            projection_convergence = receipt["summary"]["uvProjectionConvergence"]
            if projection_convergence != {
                "sourceBoundarySamplesConverged": 73,
                "sourceBoundarySamplesNotConverged": 27,
                "sourceEdgeMidpointProbesConverged": 1048,
                "sourceEdgeMidpointProbesNotConverged": 392,
            }:
                raise ValueError("pinned UV projection convergence status changed")
            edge_ids = set()
            for index, sample in enumerate(boundary_samples):
                vertex_index = sample.get("candidateVertexIndex")
                residual = sample.get("surfaceResidualSourceUnits")
                projection_residual = sample.get("projectionResidualSourceUnits")
                candidate_uv = sample.get("candidateUv")
                source_point = sample.get("sourcePointSourceUnits")
                if (
                    sample.get("boundaryIndex") != index
                    or sample.get("orientedEdgeId") not in EXPECTED_ORIENTED_EDGES
                    or sample.get("edgeCurveId") is None
                    or sample.get("curveGeometryId") is None
                    or not isinstance(source_point, list)
                    or len(source_point) != 3
                    or not isinstance(candidate_uv, list)
                    or len(candidate_uv) != 2
                    or not isinstance(vertex_index, int)
                    or not 0 <= vertex_index < len(vertices)
                    or not is_json_number(residual)
                    or not math.isfinite(residual)
                    or residual < 0
                    or not is_json_number(projection_residual)
                    or not math.isfinite(projection_residual)
                    or projection_residual < 0
                    or projection_residual > 1e-5
                    or not isinstance(sample.get("projectionConverged"), bool)
                    or (
                        sample.get("projectionConverged")
                        and projection_residual > exact_projection_tolerance + 1e-14
                    )
                    or any(
                        not is_json_number(value) or not math.isfinite(float(value))
                        for value in [*source_point, *candidate_uv]
                    )
                ):
                    raise ValueError("candidate boundary provenance has an invalid sample")
                if any(
                    abs(float(candidate_uv[axis]) - float(vertices[vertex_index][axis])) > 1e-14
                    for axis in range(2)
                ):
                    raise ValueError("candidate boundary sample maps to the wrong retained vertex")
                point = sample.get("surfacePointSourceUnits")
                mesh_point = sample.get("candidateMeshPointSourceUnits")
                mesh_residual = sample.get("meshToSourceTrimResidualSourceUnits")
                if (
                    not isinstance(point, list)
                    or len(point) != 3
                    or any(
                        abs(float(point[axis]) - float(surface_points[vertex_index][axis])) > 1e-14
                        for axis in range(3)
                    )
                    or not isinstance(mesh_point, list)
                    or len(mesh_point) != 3
                    or not is_json_number(mesh_residual)
                    or not math.isfinite(float(mesh_residual))
                    or mesh_residual < 0
                    or mesh_residual > 1e-5
                    or any(
                        abs(float(mesh_point[axis]) - float(mesh_points[vertex_index][axis]))
                        > 1e-14
                        for axis in range(3)
                    )
                    or abs(
                        math.dist(
                            [float(value) for value in mesh_point],
                            [float(value) for value in source_point],
                        )
                        - float(mesh_residual)
                    )
                    > 1e-12
                ):
                    raise ValueError("candidate boundary sample maps to the wrong mesh or surface point")
                edge_ids.add(sample["orientedEdgeId"])
            closure = boundary_trace["candidateLoopClosureUv"]
            if (
                len(edge_ids) != len(EXPECTED_ORIENTED_EDGES)
                or not isinstance(closure, list)
                or len(closure) != 2
                or any(not is_json_number(value) or not math.isfinite(value) for value in closure)
                or math.hypot(float(closure[0]), float(closure[1])) > 1e-6
                or boundary_trace.get("ambiguousHalfPeriodStepCount") != 0
                or not isinstance(boundary_trace.get("uPeriodicBranchShiftCount"), int)
                or not isinstance(boundary_trace.get("vPeriodicBranchShiftCount"), int)
                or not is_json_number(boundary_trace.get("maximumUnwrappedUStep"))
                or not is_json_number(boundary_trace.get("maximumUnwrappedVStep"))
            ):
                raise ValueError("candidate periodic branch trace is invalid or does not close")
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
