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
FREECAD_REFERENCE_REPORT_SHA256 = "3e010ab4ae00395176a433f657095fcf11d8472c0d26ee66659f579a8c8874fc"
EXPECTED_ORIENTED_EDGES = [2809, 470, 7109, 4933]
MAX_EDGE_CURVE_CONTROL_POINT_STORAGE_BUDGET = 65_536
EXPECTED_FREECAD_U_BIN_COUNTS = [
    (8981, 3130),
    (1277, 47),
    (1680, 81),
    (1829, 104),
    (2080, 127),
    (2701, 219),
    (2635, 148),
    (2881, 139),
]
EXPECTED_ADJACENT_EDGE_U_BIN_COUNTS = [
    (13207, 9371),
    (1909, 105),
    (2281, 209),
    (2653, 317),
    (3615, 588),
    (3521, 506),
    (3893, 602),
    (4249, 698),
]
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


def validate_orientation_continuity(trim_candidate: dict) -> dict:
    continuity = trim_candidate.get("orientationContinuity")
    if (
        not isinstance(continuity, dict)
        or continuity.get("schema")
        != "tokito.step-trim-candidate-orientation-continuity.v1"
        or continuity.get("diagnosticOnly") is not True
        or continuity.get("admissionAffecting") is not False
        or continuity.get("sampleScope") != "final_refinement_level_only"
        or continuity.get("finalTriangleCount") != 24_064
        or continuity.get("observationCacheLimit") != 100_000
        or continuity.get("cachedObservationCountAcrossRefinementLevels", 0)
        > continuity.get("observationCacheLimit", -1)
    ):
        raise ValueError("orientation continuity diagnostic scope or bound changed")
    u_domain = continuity.get("uDomain")
    if (
        not isinstance(u_domain, list)
        or len(u_domain) != 2
        or any(not is_json_number(value) or not math.isfinite(float(value)) for value in u_domain)
        or abs(float(u_domain[0])) > 1e-14
        or abs(float(u_domain[1]) - 1.404960923061) > 1e-12
    ):
        raise ValueError("orientation continuity U domain changed")
    winding = continuity.get("uvWindingSigns", {})
    if winding != {"positive": 24_064, "negative": 0, "zero": 0}:
        raise ValueError("candidate UV winding-sign distribution changed")

    bins = continuity.get("parameterNormalAlignmentByU8")
    if not isinstance(bins, list) or len(bins) != 8:
        raise ValueError("orientation continuity must report eight canonical U bins")
    observed_u_bins = []
    for expected_index, row in enumerate(bins):
        if not isinstance(row, dict) or row.get("uBin") != expected_index:
            raise ValueError("orientation continuity U bins are not ordered")
        alignment = row.get("parameterNormalAlignment")
        if not isinstance(alignment, dict):
            raise ValueError("orientation continuity U-bin alignment summary is missing")
        count = alignment.get("count")
        nonpositive_count = alignment.get("nonpositiveCount")
        positive_count = alignment.get("positiveCount")
        if (
            not isinstance(count, int)
            or not isinstance(nonpositive_count, int)
            or not isinstance(positive_count, int)
            or count != positive_count + nonpositive_count
            or not is_json_number(alignment.get("minimumAlignment"))
            or not is_json_number(alignment.get("maximumAlignment"))
            or not is_json_number(alignment.get("meanAlignment"))
        ):
            raise ValueError("orientation continuity U-bin alignment statistics are malformed")
        observed_u_bins.append((count, nonpositive_count))
    if observed_u_bins != EXPECTED_FREECAD_U_BIN_COUNTS:
        raise ValueError("Rust U-bin alignment counts differ from the pinned FreeCAD reference")

    adjacency = continuity.get("edgeAdjacency", {})
    if (
        adjacency.get("uniqueEdgeCount") != 36_864
        or adjacency.get("boundaryEdgeCount") != 1_536
        or adjacency.get("interiorEdgeCount") != 35_328
        or adjacency.get("nonmanifoldEdgeCount") != 0
        or adjacency.get("inconsistentInteriorEdgeWindingCount") != 0
        or adjacency.get("adjacentTrianglePairCount") != 35_328
    ):
        raise ValueError("orientation continuity edge adjacency differs from the pinned mesh")
    normal_dot_summaries = {
        "geometricTriangleNormalDot": adjacency.get("geometricTriangleNormalDot", {}),
        "parameterNormalDot": adjacency.get("parameterNormalDot", {}),
    }
    for name, summary in normal_dot_summaries.items():
        compared = summary.get("comparedPairCount")
        undefined = summary.get("undefinedPairCount")
        if (
            not isinstance(compared, int)
            or not isinstance(undefined, int)
            or compared + undefined != 35_328
            or undefined != 0
            or not isinstance(summary.get("nonpositiveDotPairCount"), int)
            or not is_json_number(summary.get("meanDot"))
            or (
                name == "parameterNormalDot"
                and summary["nonpositiveDotPairCount"] != 0
            )
        ):
            raise ValueError(f"invalid adjacent-normal dot summary: {name}")
    adjacent_bins = continuity.get("adjacentNormalContinuityByU8")
    if not isinstance(adjacent_bins, list) or len(adjacent_bins) != 8:
        raise ValueError("adjacent normal continuity must report eight U bins")
    adjacent_bin_summary = []
    for expected_index, row in enumerate(adjacent_bins):
        if not isinstance(row, dict) or row.get("uBin") != expected_index:
            raise ValueError("adjacent-normal continuity U bins are not ordered")
        pair_count = row.get("adjacentTrianglePairCount")
        geometric = row.get("geometricTriangleNormalDot")
        parameter = row.get("parameterNormalDot")
        if (
            not isinstance(pair_count, int)
            or not isinstance(geometric, dict)
            or not isinstance(parameter, dict)
            or geometric.get("comparedPairCount", 0) + geometric.get("undefinedPairCount", 0)
            != pair_count
            or parameter.get("comparedPairCount", 0) + parameter.get("undefinedPairCount", 0)
            != pair_count
            or geometric.get("undefinedPairCount") != 0
            or parameter.get("undefinedPairCount") != 0
            or not isinstance(geometric.get("nonpositiveDotPairCount"), int)
            or not isinstance(parameter.get("nonpositiveDotPairCount"), int)
            or parameter.get("nonpositiveDotPairCount") != 0
        ):
            raise ValueError("adjacent-normal continuity U-bin statistics are malformed")
        adjacent_bin_summary.append(
            {
                "uBin": expected_index,
                "adjacentTrianglePairCount": pair_count,
                "geometricTriangleNormalDot": geometric,
                "parameterNormalDot": parameter,
            }
        )
    observed_adjacent_edge_u_bins = [
        (
            row["adjacentTrianglePairCount"],
            row["geometricTriangleNormalDot"]["nonpositiveDotPairCount"],
        )
        for row in adjacent_bin_summary
    ]
    if observed_adjacent_edge_u_bins != EXPECTED_ADJACENT_EDGE_U_BIN_COUNTS:
        raise ValueError("pinned candidate adjacent-geometric-normal U-bin diagnostic changed")
    if (
        sum(row["adjacentTrianglePairCount"] for row in adjacent_bin_summary) != 35_328
        or sum(
            row["geometricTriangleNormalDot"]["nonpositiveDotPairCount"]
            for row in adjacent_bin_summary
        )
        != normal_dot_summaries["geometricTriangleNormalDot"]["nonpositiveDotPairCount"]
        or sum(
            row["parameterNormalDot"]["nonpositiveDotPairCount"]
            for row in adjacent_bin_summary
        )
        != normal_dot_summaries["parameterNormalDot"]["nonpositiveDotPairCount"]
    ):
        raise ValueError("U-bin adjacent-normal statistics do not sum to the global totals")

    return {
        "schema": continuity["schema"],
        "diagnosticOnly": continuity["diagnosticOnly"],
        "admissionAffecting": continuity["admissionAffecting"],
        "finalTriangleCount": continuity["finalTriangleCount"],
        "cachedObservationCountAcrossRefinementLevels": continuity[
            "cachedObservationCountAcrossRefinementLevels"
        ],
        "observationCacheLimit": continuity["observationCacheLimit"],
        "uvWindingSigns": winding,
        "uBinTriangleAndNonpositiveCounts": [
            {"uBin": index, "count": count, "nonpositiveCount": nonpositive}
            for index, (count, nonpositive) in enumerate(observed_u_bins)
        ],
        "edgeAdjacency": {
            key: adjacency[key]
            for key in (
                "uniqueEdgeCount",
                "boundaryEdgeCount",
                "interiorEdgeCount",
                "nonmanifoldEdgeCount",
                "inconsistentInteriorEdgeWindingCount",
                "adjacentTrianglePairCount",
            )
        },
        "adjacentNormalDotSummaries": normal_dot_summaries,
        "adjacentNormalContinuityByU8": adjacent_bin_summary,
        "pinnedFreecadReference": {
            "reportSha256": FREECAD_REFERENCE_REPORT_SHA256,
            "matchedPinnedAggregateConstants": True,
            "referenceFileLoadedByThisTest": False,
            "comparedPerTriangleAgainstIndependentTriangulation": False,
            "scope": "E2E compares Rust aggregate counts with pinned constants attributed to this report SHA; it does not open or hash the report file, and FreeCAD evaluated the same Rust candidate",
        },
    }


def _uv_point(value: object) -> tuple[float, float]:
    if (
        not isinstance(value, list)
        or len(value) != 2
        or any(not is_json_number(component) or not math.isfinite(float(component)) for component in value)
    ):
        raise ValueError("candidate contains an invalid UV point")
    return (float(value[0]), float(value[1]))


def _point_key(point: tuple[float, float]) -> tuple[float, float]:
    return tuple(0.0 if component == 0.0 else component for component in point)


def _uv_angle_degrees(
    vertex: tuple[float, float], first: tuple[float, float], second: tuple[float, float]
) -> float:
    a = (first[0] - vertex[0], first[1] - vertex[1])
    b = (second[0] - vertex[0], second[1] - vertex[1])
    denominator = math.hypot(*a) * math.hypot(*b)
    if denominator == 0.0:
        return 0.0
    cosine = max(-1.0, min(1.0, (a[0] * b[0] + a[1] * b[1]) / denominator))
    return math.degrees(math.acos(cosine))


def point_in_polygon(point: tuple[float, float], loop: list[tuple[float, float]]) -> bool:
    if point_on_polygon_boundary(point, loop):
        return True
    inside = False
    previous = len(loop) - 1
    for current, a in enumerate(loop):
        b = loop[previous]
        if (a[1] > point[1]) != (b[1] > point[1]):
            crossing_x = (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
            if point[0] < crossing_x:
                inside = not inside
        previous = current
    return inside


def point_on_polygon_boundary(point: tuple[float, float], loop: list[tuple[float, float]]) -> bool:
    for index, start in enumerate(loop):
        end = loop[(index + 1) % len(loop)]
        dx = end[0] - start[0]
        dy = end[1] - start[1]
        length_squared = dx * dx + dy * dy
        if length_squared == 0.0:
            continue
        offset_x = point[0] - start[0]
        offset_y = point[1] - start[1]
        parameter = (offset_x * dx + offset_y * dy) / length_squared
        if -1e-10 <= parameter <= 1.0 + 1e-10:
            projected = (start[0] + parameter * dx, start[1] + parameter * dy)
            if math.hypot(point[0] - projected[0], point[1] - projected[1]) <= 1e-10:
                return True
    return False


def _boundary_polyline_covered(
    boundary_edges: list[tuple[tuple[float, float], tuple[float, float]]],
    loop: list[tuple[float, float]],
) -> bool:
    intervals: list[list[tuple[float, float]]] = [[] for _ in loop]
    for edge_a, edge_b in boundary_edges:
        matches: list[tuple[float, int, float, float]] = []
        for index, start in enumerate(loop):
            end = loop[(index + 1) % len(loop)]
            dx = end[0] - start[0]
            dy = end[1] - start[1]
            length_squared = dx * dx + dy * dy
            if length_squared == 0.0:
                continue

            def locate(point: tuple[float, float]) -> tuple[float, float]:
                offset_x = point[0] - start[0]
                offset_y = point[1] - start[1]
                parameter = (offset_x * dx + offset_y * dy) / length_squared
                projected = (start[0] + parameter * dx, start[1] + parameter * dy)
                return parameter, math.hypot(point[0] - projected[0], point[1] - projected[1])

            ta, da = locate(edge_a)
            tb, db = locate(edge_b)
            if (
                da <= 1e-9 and db <= 1e-9
                and -1e-9 <= ta <= 1.0 + 1e-9
                and -1e-9 <= tb <= 1.0 + 1e-9
            ):
                matches.append((max(da, db), index, max(0.0, min(1.0, ta)), max(0.0, min(1.0, tb))))
        if not matches:
            return False
        _, index, ta, tb = min(matches)
        intervals[index].append((min(ta, tb), max(ta, tb)))

    for spans in intervals:
        spans.sort()
        cursor = 0.0
        for start, end in spans:
            if start > cursor + 1e-6 or start < cursor - 1e-6:
                return False
            cursor = max(cursor, end)
        if abs(cursor - 1.0) > 1e-6:
            return False
    return True


def independently_measure_spade_candidate(candidate: dict, diagnostic: dict) -> dict:
    """Recompute UV mesh metrics and incidence without trusting candidate summaries."""
    raw_triangles = candidate.get("trianglesUv")
    raw_loop = candidate.get("trimUvLoop")
    if not isinstance(raw_triangles, list) or not isinstance(raw_loop, list):
        raise ValueError("candidate UV geometry is missing")
    triangles: list[tuple[tuple[float, float], ...]] = []
    vertices: set[tuple[float, float]] = set()
    edges: dict[tuple[tuple[float, float], tuple[float, float]], list[int]] = {}
    uv_area = 0.0
    minimum_angle = math.inf
    maximum_area = 0.0
    loop = [_uv_point(point) for point in raw_loop]
    for raw_triangle in raw_triangles:
        if not isinstance(raw_triangle, list) or len(raw_triangle) != 3:
            raise ValueError("candidate contains malformed UV triangle")
        triangle = tuple(_uv_point(point) for point in raw_triangle)
        a, b, c = triangle
        twice_area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        if not math.isfinite(twice_area) or twice_area == 0.0:
            raise ValueError("candidate contains degenerate UV triangle")
        area = abs(twice_area) * 0.5
        uv_area += area
        maximum_area = max(maximum_area, area)
        for index in range(3):
            minimum_angle = min(minimum_angle, _uv_angle_degrees(
                triangle[index], triangle[(index + 1) % 3], triangle[(index + 2) % 3]
            ))
        normalized = tuple(_point_key(point) for point in triangle)
        vertices.update(normalized)
        for first, second in ((normalized[0], normalized[1]), (normalized[1], normalized[2]), (normalized[2], normalized[0])):
            key = tuple(sorted((first, second)))
            direction = 1 if first <= second else -1
            record = edges.setdefault(key, [0, 0])
            record[0] += 1
            record[1] += direction
        centroid = (
            sum(point[0] for point in triangle) / 3.0,
            sum(point[1] for point in triangle) / 3.0,
        )
        loop_points = [_uv_point(point) for point in raw_loop]
        if not point_in_polygon(centroid, loop_points):
            raise ValueError("candidate triangle centroid lies outside the pinned trim loop")

    diagnostic_samples = diagnostic.get("trimCandidate", {}).get("boundarySamples")
    if not isinstance(diagnostic_samples, list) or len(diagnostic_samples) != 100:
        raise ValueError("pinned diagnostic boundary samples changed")
    ordered: list[tuple[float, float] | None] = [None] * len(diagnostic_samples)
    for sample in diagnostic_samples:
        index = sample.get("boundaryIndex")
        if not isinstance(index, int) or isinstance(index, bool) or not 0 <= index < len(ordered) or ordered[index] is not None:
            raise ValueError("pinned diagnostic boundary sample index is invalid")
        ordered[index] = _uv_point(sample.get("candidateUv"))
    unique_loop: list[tuple[float, float]] = []
    for point in ordered:
        if point is None:
            raise ValueError("pinned diagnostic boundary sample is absent")
        if not unique_loop or unique_loop[-1] != point:
            unique_loop.append(point)
    if unique_loop[0] == unique_loop[-1]:
        unique_loop.pop()
    if len(unique_loop) != 96 or len(loop) != len(unique_loop) or any(
        math.hypot(actual[0] - expected[0], actual[1] - expected[1]) > 1e-12
        for actual, expected in zip(loop, unique_loop)
    ):
        raise ValueError("candidate UV trim loop differs from source diagnostic samples")

    if not {_point_key(point) for point in loop}.issubset(vertices):
        raise ValueError("candidate mesh does not preserve every original trim-loop vertex")
    outside_mesh_vertex_count = sum(not point_in_polygon(point, loop) for point in vertices)
    unique_edges = [
        (edge[0], edge[1])
        for edge in edges
    ]
    outside_edge_midpoint_count = sum(
        not point_in_polygon(((a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5), loop)
        for a, b in unique_edges
    )
    trim_area = abs(sum(
        loop[index][0] * loop[(index + 1) % len(loop)][1]
        - loop[(index + 1) % len(loop)][0] * loop[index][1]
        for index in range(len(loop))
    )) * 0.5
    if outside_mesh_vertex_count or outside_edge_midpoint_count:
        raise ValueError("candidate mesh has vertices or edge midpoints outside the trim polygon")
    if not math.isclose(uv_area, trim_area, rel_tol=1e-10, abs_tol=1e-12):
        raise ValueError("candidate triangle area does not cover the trim polygon area")

    boundary_edges = [edge for edge, (count, _) in edges.items() if count == 1]
    nonmanifold_edges = sum(count > 2 for count, _ in edges.values())
    inconsistent_winding = sum(count == 2 and direction != 0 for count, direction in edges.values())
    euler = len(vertices) - len(edges) + len(raw_triangles)
    return {
        "vertices": len(vertices),
        "edges": len(edges),
        "triangles": len(raw_triangles),
        "boundaryEdges": len(boundary_edges),
        "nonmanifoldEdges": nonmanifold_edges,
        "inconsistentInteriorEdgeWindingCount": inconsistent_winding,
        "eulerCharacteristic": euler,
        "boundaryPolylineCovered": _boundary_polyline_covered(boundary_edges, loop),
        "boundaryLoopVerticesPreserved": True,
        "allMeshVerticesInsideOrOnTrim": True,
        "allEdgeMidpointsInsideOrOnTrim": True,
        "trimLoopArea": trim_area,
        "uvAreaMatchesTrimLoopArea": True,
        "outsideMeshVertexCount": outside_mesh_vertex_count,
        "outsideEdgeMidpointCount": outside_edge_midpoint_count,
        "uvArea": uv_area,
        "minimumUvAngleDegrees": minimum_angle,
        "maximumUvTriangleArea": maximum_area,
    }


def run(
    binary: Path,
    source: Path,
    output: Path,
    candidate_generator: Path | None = None,
) -> dict:
    if output.exists():
        raise ValueError("output must be new")
    tracked = [
        (binary, 64 * 1024 * 1024),
        (source, 32 * 1024 * 1024),
        (Path(__file__), 1024 * 1024),
        (Path(__file__).with_name("bounded_fingerprint.py"), 1024 * 1024),
        (Path(__file__).with_name("surface_reference_e2e.py"), 2 * 1024 * 1024),
    ]
    if candidate_generator is not None:
        tracked.append((candidate_generator, 64 * 1024 * 1024))
        candidate_source = Path(__file__).parents[1] / "src" / "bin" / "model16_spade_candidate.rs"
        candidate_manifest = Path(__file__).parents[1] / "Cargo.toml"
        candidate_lock = Path(__file__).parents[1] / "Cargo.lock"
        tracked.extend(
            [
                (candidate_source, 2 * 1024 * 1024),
                (candidate_manifest, 1024 * 1024),
                (candidate_lock, 4 * 1024 * 1024),
            ]
        )
    before = {str(path): fingerprint(path, limit) for path, limit in tracked}
    output.mkdir(parents=True)
    receipt = {
        "schema": "tokito.step-model16-source-face-trim-e2e.v1",
        "passed": False,
        "admitted": False,
        "exportsProduced": [],
        "inputSha256": before,
        "scope": (
            "pinned source B-spline face/trim diagnostics, bounded projection observations, "
            "and an isolated research-only Spade candidate; no production repair/export "
            "or geometry admission"
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
        orientation_continuity_summary = validate_orientation_continuity(trim_candidate)
        receipt["summary"] = {
            "faceId": report.get("faceId"),
            "surfaceId": report.get("surfaceId"),
            "boundId": report.get("boundId"),
            "loopId": report.get("loopId"),
            "orientedEdgeIds": edges,
            "faceSameSense": report.get("faceSameSense"),
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
            "trimCandidateChordConverged": trim_candidate.get("chordConverged"),
            "trimCandidateOrientationConverged": trim_candidate.get("orientationConverged"),
            "triangleOrientationCheckCount": trim_candidate.get("orientationCheckCount"),
            "minimumTriangleNormalAlignment": trim_candidate.get(
                "minimumTriangleNormalAlignment"
            ),
            "nonpositiveTriangleNormalCount": trim_candidate.get(
                "nonpositiveTriangleNormalCount"
            ),
            "orientationContinuity": orientation_continuity_summary,
            "trimCandidateRefinementConverged": trim_candidate.get("refinementConverged"),
            "trimCandidateRefinementFailureReason": trim_candidate.get(
                "refinementFailureReason"
            ),
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
            or report.get("faceSameSense") is not True
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
            or trim_candidate.get("vertexCount") != 12801
            or trim_candidate.get("triangleCount") != 24064
            or trim_candidate.get("refinementLevels") != 4
            or trim_candidate.get("chordConverged") is not True
            or trim_candidate.get("orientationConverged") is not False
            or trim_candidate.get("orientationCheckCount") != 24064
            or trim_candidate.get("nonpositiveTriangleNormalCount") != 3995
            or not is_json_number(
                trim_candidate.get("minimumTriangleNormalAlignment")
            )
            or trim_candidate.get("minimumTriangleNormalAlignment") >= 0
            or trim_candidate.get("refinementConverged") is not False
            or trim_candidate.get("refinementFailureReason")
            != "trim_candidate_normal_orientation_not_reached"
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
                "vertices": 12801,
                "edges": 36864,
                "triangles": 24064,
                "boundaryEdges": 1536,
                "boundaryVertices": 1536,
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
            if not isinstance(boundary_probes, list) or len(boundary_probes) != 2976:
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
                "sourceEdgeMidpointProbesConverged": 2167,
                "sourceEdgeMidpointProbesNotConverged": 809,
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
        if candidate_generator is not None:
            candidate_path = output / "spade-candidate.json"
            code, candidate_stdout, candidate_stderr = capture_bounded(
                [
                    str(candidate_generator),
                    "--research-remesh-model16",
                    str(source),
                    str(output / "diagnostic.json"),
                    str(candidate_path),
                ],
                timeout_seconds=300,
                stdout_limit=256 * 1024,
                stderr_limit=512 * 1024,
            )
            if code != 0 or candidate_stderr or not candidate_path.is_file():
                raise ValueError("research Spade candidate generator did not complete cleanly")
            if candidate_path.stat().st_size > 32 * 1024 * 1024:
                raise ValueError("research Spade candidate exceeded its output size bound")
            candidate = json.loads(candidate_path.read_bytes())
            candidate_summary = json.loads(candidate_stdout)
            topology = candidate.get("topology", {})
            triangles = candidate.get("trianglesUv", [])
            diagnostic = json.loads((output / "diagnostic.json").read_bytes())
            independently_measured = independently_measure_spade_candidate(candidate, diagnostic)
            expected_candidate_digest = fingerprint(candidate_path, 32 * 1024 * 1024)
            if (
                candidate.get("schema") != "tokito.spade-model16-remesh-candidate.v1"
                or candidate.get("researchOnly") is not True
                or candidate.get("sourceFaceId") != 2875
                or candidate.get("sourceSha256") != before[str(source)]
                or candidate.get("diagnosticSha256") != receipt["diagnosticSha256"]
                or candidate.get("generatorSourceSha256")
                != before[str(candidate_source)]
                or candidate.get("spadeVersion") != "2.15.1"
                or candidate.get("angleLimitDegrees") != 20.0
                or candidate.get("maxUvTriangleArea") != 0.00005
                or candidate.get("keepConstraintEdges") is not False
                or candidate.get("refinementComplete") is not True
                or candidate.get("geometryQualified") is not False
                or candidate.get("admitted") is not False
                or candidate.get("exportsProduced") != []
                or candidate_summary.get("geometryQualified") is not False
                or candidate_summary.get("admitted") is not False
                or candidate_summary.get("exportsProduced") != []
                or not 3 <= len(triangles) <= 50_000
                or topology.get("vertices", 0) > 20_096
                or topology.get("triangles") != len(triangles)
                or topology.get("boundaryPolylineCovered") is not True
                or topology.get("originalBoundarySamplesPreserved") is not True
                or topology.get("nonmanifoldEdges") != 0
                or topology.get("inconsistentInteriorEdgeWindingCount") != 0
                or topology.get("eulerCharacteristic") != 1
                or candidate.get("geometryEvaluation", {}).get("sourceEdgeCurveReevaluation")
                != "not_performed"
                or candidate.get("geometryEvaluation", {}).get("fullSurfaceDeviationBound")
                != "not_performed"
                or candidate_summary.get("candidateOutput") != str(candidate_path)
                or candidate_summary.get("sourceSha256") != before[str(source)]
                or candidate_summary.get("diagnosticSha256") != receipt["diagnosticSha256"]
                or candidate_summary.get("generatorSourceSha256")
                != before[str(candidate_source)]
                or candidate_summary.get("spadeVersion") != "2.15.1"
                or candidate_summary.get("refinementComplete") is not True
                or candidate_summary.get("vertices") != independently_measured["vertices"]
                or candidate_summary.get("triangles") != independently_measured["triangles"]
                or candidate_summary.get("nonmanifoldEdges") != independently_measured["nonmanifoldEdges"]
                or candidate_summary.get("boundaryPolylineCovered")
                != independently_measured["boundaryPolylineCovered"]
                or not math.isclose(
                    candidate_summary.get("minimumUvAngleDegrees", math.nan),
                    independently_measured["minimumUvAngleDegrees"],
                    rel_tol=1e-10,
                    abs_tol=1e-10,
                )
                or independently_measured["vertices"] != topology.get("vertices")
                or independently_measured["edges"] != topology.get("edges")
                or independently_measured["triangles"] != topology.get("triangles")
                or independently_measured["boundaryEdges"] != topology.get("boundaryEdges")
                or independently_measured["nonmanifoldEdges"] != topology.get("nonmanifoldEdges")
                or independently_measured["inconsistentInteriorEdgeWindingCount"]
                != topology.get("inconsistentInteriorEdgeWindingCount")
                or independently_measured["eulerCharacteristic"] != topology.get("eulerCharacteristic")
                or independently_measured["boundaryPolylineCovered"]
                != topology.get("boundaryPolylineCovered")
                or not independently_measured["boundaryPolylineCovered"]
                or not math.isclose(independently_measured["uvArea"], candidate.get("uvArea", math.nan), rel_tol=1e-10, abs_tol=1e-12)
                or not math.isclose(independently_measured["minimumUvAngleDegrees"], candidate.get("minUvAngleDegrees", math.nan), rel_tol=1e-10, abs_tol=1e-10)
                or independently_measured["minimumUvAngleDegrees"] < 20.0 - 1e-8
                or independently_measured["maximumUvTriangleArea"] > 0.00005 * (1.0 + 1e-8)
            ):
                raise ValueError("research Spade candidate failed its bounded topology/provenance contract")
            receipt["summary"]["researchSpadeCandidate"] = {
                "candidateSha256": expected_candidate_digest,
                "generatorSourceSha256": candidate["generatorSourceSha256"],
                "spadeVersion": candidate["spadeVersion"],
                "triangleCount": len(triangles),
                "independentlyMeasuredUv": independently_measured,
                "topology": topology,
                "geometryQualified": False,
                "admitted": False,
                "exportsProduced": [],
                "scope": candidate.get("scope"),
            }
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
    parser.add_argument("--candidate-generator", type=Path)
    args = parser.parse_args()
    result = run(args.binary, args.source, args.output, args.candidate_generator)
    print(json.dumps({key: result.get(key) for key in ("passed", "error", "summary")}))
    raise SystemExit(0 if result["passed"] else 2)
