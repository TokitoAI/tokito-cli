"""Bounded, bidirectional surface-sample comparison for one STEP candidate.

This is experimental QA only. A passing sample comparison does not admit STEP
geometry to the product exporter or prove component association/PCB placement.
"""

import argparse
import collections
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time

MAX_SOURCE_BYTES = 32 * 1024 * 1024
MAX_REPORT_BYTES = 16 * 1024 * 1024
MAX_BINARY_BYTES = 128 * 1024 * 1024
MAX_STDERR_BYTES = 64 * 1024
MAX_VERTICES = 100_000
MAX_TRIANGLES = 200_000
MAX_TOTAL_PAIRWISE_SAMPLE_TRIANGLE_WORK = 15_000_000
SURFACE_SAMPLE_LIMIT_MM = 0.000001
REFERENCE_SHA256 = "7df0f74104bd80138ecb04467255727d83e41db4de20e4315cd4fc62dbb69eca"


def bounded_read(path: Path, maximum: int) -> bytes:
    if maximum < 0:
        raise ValueError("byte limit must be non-negative")
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"not a regular file: {path}")
    # The descriptor read is authoritative. A preceding stat-size check alone
    # can be invalidated by a concurrent replacement or file growth.
    with path.open("rb") as stream:
        data = stream.read(maximum + 1)
    if len(data) > maximum:
        raise ValueError(f"file exceeds byte limit: {path}")
    return data


def finite_point(point):
    return (
        isinstance(point, list)
        and len(point) == 3
        and all(type(value) in (int, float) and math.isfinite(value) for value in point)
    )


def triangle_key(points, triangle, digits=8):
    return tuple(
        sorted(tuple(round(value, digits) for value in points[index]) for index in triangle)
    )


def dot(a, b):
    return sum(x * y for x, y in zip(a, b, strict=True))


def sub(a, b):
    return [x - y for x, y in zip(a, b, strict=True)]


def add(a, b):
    return [x + y for x, y in zip(a, b, strict=True)]


def mul(a, scale):
    return [x * scale for x in a]


def point_segment_distance(point, a, b):
    segment = sub(b, a)
    length_squared = dot(segment, segment)
    if length_squared == 0:
        return math.dist(point, a)
    fraction = max(0.0, min(1.0, dot(sub(point, a), segment) / length_squared))
    return math.dist(point, add(a, mul(segment, fraction)))


def point_triangle_distance(point, a, b, c):
    """Return point distance, safely handling a degenerate triangle as edges."""
    ab = sub(b, a)
    ac = sub(c, a)
    normal = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ]
    largest_edge_squared = max(dot(ab, ab), dot(ac, ac), dot(sub(c, b), sub(c, b)))
    if dot(normal, normal) <= largest_edge_squared * largest_edge_squared * 1e-24:
        return min(
            point_segment_distance(point, a, b),
            point_segment_distance(point, b, c),
            point_segment_distance(point, c, a),
        )

    ap = sub(point, a)
    d1, d2 = dot(ab, ap), dot(ac, ap)
    if d1 <= 0 and d2 <= 0:
        return math.dist(point, a)

    bp = sub(point, b)
    d3, d4 = dot(ab, bp), dot(ac, bp)
    if d3 >= 0 and d4 <= d3:
        return math.dist(point, b)

    vc = d1 * d4 - d3 * d2
    if vc <= 0 and d1 >= 0 and d3 <= 0:
        return math.dist(point, add(a, mul(ab, d1 / (d1 - d3))))

    cp = sub(point, c)
    d5, d6 = dot(ab, cp), dot(ac, cp)
    if d6 >= 0 and d5 <= d6:
        return math.dist(point, c)

    vb = d5 * d2 - d1 * d6
    if vb <= 0 and d2 >= 0 and d6 <= 0:
        return math.dist(point, add(a, mul(ac, d2 / (d2 - d6))))

    va = d3 * d6 - d5 * d4
    if va <= 0 and d4 - d3 >= 0 and d5 - d6 >= 0:
        edge_scale = (d4 - d3) / ((d4 - d3) + (d5 - d6))
        return math.dist(point, add(b, mul(sub(c, b), edge_scale)))

    denominator = va + vb + vc
    if abs(denominator) < 1e-30:
        return min(math.dist(point, vertex) for vertex in (a, b, c))
    v, w = vb / denominator, vc / denominator
    projection = add(a, add(mul(ab, v), mul(ac, w)))
    return math.dist(point, projection)


def capture_bounded(command, timeout_seconds, stdout_limit, stderr_limit):
    """Run a child while draining each pipe under a strict in-memory cap."""
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    buffers = [bytearray(), bytearray()]
    limits = [stdout_limit, stderr_limit]
    overflow = threading.Event()

    def drain(stream, destination, limit):
        while True:
            chunk = os.read(stream.fileno(), 64 * 1024)
            if not chunk:
                return
            remaining = limit - len(destination)
            if len(chunk) > remaining:
                if remaining > 0:
                    destination.extend(chunk[:remaining])
                overflow.set()
                return
            destination.extend(chunk)

    readers = [
        threading.Thread(target=drain, args=(stream, buffer, limit), daemon=True)
        for stream, buffer, limit in zip(
            (process.stdout, process.stderr), buffers, limits, strict=True
        )
    ]
    for reader in readers:
        reader.start()

    deadline = time.monotonic() + timeout_seconds
    timed_out = False
    while process.poll() is None:
        if overflow.is_set():
            process.kill()
            break
        if time.monotonic() >= deadline:
            timed_out = True
            process.kill()
            break
        time.sleep(0.01)

    process.wait()
    for reader in readers:
        reader.join(timeout=5)
    if any(reader.is_alive() for reader in readers):
        raise ValueError("child output reader did not stop after process exit")
    if overflow.is_set():
        raise ValueError("child output exceeded its streaming byte limit")
    if timed_out:
        raise ValueError("child process exceeded its time limit")
    return process.returncode, bytes(buffers[0]), bytes(buffers[1])


def self_test_distance_and_output_limits():
    collinear = point_triangle_distance([0, 1, 0], [0, 0, 0], [1, 0, 0], [2, 0, 0])
    coincident = point_triangle_distance([1, 2, 3], [0, 0, 0], [0, 0, 0], [0, 0, 0])
    if not math.isclose(collinear, 1.0) or not math.isclose(coincident, math.sqrt(14)):
        raise ValueError("degenerate-triangle distance regression")
    try:
        checked_mesh([[0, 0, 0], [1, 0, 0], [2, 0, 0]], [[0, 1, 2]])
    except ValueError as error:
        if "degenerate" not in str(error):
            raise
    else:
        raise ValueError("degenerate mesh validation regression")
    try:
        capture_bounded(
            [sys.executable, "-c", "import sys; sys.stdout.write('x' * 4096)"],
            timeout_seconds=5,
            stdout_limit=1024,
            stderr_limit=1024,
        )
    except ValueError as error:
        if "streaming byte limit" not in str(error):
            raise
    else:
        raise ValueError("streaming subprocess output-limit negative control failed")
    with tempfile.TemporaryDirectory(prefix="tokito-step-bounded-read-") as temporary:
        exact_limit = Path(temporary) / "exact-limit.step"
        exact_limit.write_bytes(b"12345678")
        if bounded_read(exact_limit, 8) != b"12345678":
            raise ValueError("bounded source read rejected an exact-limit file")
        oversized = Path(temporary) / "over-limit.step"
        oversized.write_bytes(b"x" * 9)
        try:
            bounded_read(oversized, 8)
        except ValueError as error:
            if "byte limit" not in str(error):
                raise
        else:
            raise ValueError("bounded source read accepted an over-limit file")
    tetra_points = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]]
    tetra_triangles = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]
    tetra_integrity = mesh_integrity(tetra_points, tetra_triangles)
    if (
        not tetra_integrity["closed"]
        or not tetra_integrity["consistentlyOriented"]
        or not math.isclose(tetra_integrity["signedVolumeMm3"], 1 / 6)
    ):
        raise ValueError("mesh integrity tetrahedron regression")
    reversed_tetra = [triangle[:] for triangle in tetra_triangles]
    reversed_tetra[0][1], reversed_tetra[0][2] = reversed_tetra[0][2], reversed_tetra[0][1]
    if mesh_integrity(tetra_points, reversed_tetra)["consistentlyOriented"]:
        raise ValueError("mesh integrity reversed-face negative control failed")


def surface_samples(points, triangles):
    for triangle in triangles:
        a, b, c = (points[index] for index in triangle)
        yield a
        yield b
        yield c
        yield [(a[i] + b[i] + c[i]) / 3 for i in range(3)]
        yield [(a[i] + b[i]) / 2 for i in range(3)]
        yield [(b[i] + c[i]) / 2 for i in range(3)]
        yield [(c[i] + a[i]) / 2 for i in range(3)]


def directed_sample_distance(source_points, source_triangles, target_points, target_triangles):
    target_mesh = [tuple(target_points[index] for index in tri) for tri in target_triangles]
    distances = [
        min(point_triangle_distance(point, *triangle) for triangle in target_mesh)
        for point in surface_samples(source_points, source_triangles)
    ]
    return max(distances), sum(distances) / len(distances), len(distances)


def checked_mesh(points, triangles):
    if (
        not isinstance(points, list)
        or not isinstance(triangles, list)
        or not points
        or len(points) > MAX_VERTICES
        or len(triangles) > MAX_TRIANGLES
    ):
        raise ValueError("mesh exceeds vertex/triangle limits or is empty")
    if any(not finite_point(point) for point in points):
        raise ValueError("mesh contains an invalid point")
    for triangle in triangles:
        if (
            not isinstance(triangle, list)
            or len(triangle) != 3
            or any(type(index) is not int or not 0 <= index < len(points) for index in triangle)
        ):
            raise ValueError("mesh contains an invalid triangle index")
        a, b, c = (points[index] for index in triangle)
        ab, ac = sub(b, a), sub(c, a)
        normal = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ]
        largest_edge_squared = max(dot(ab, ab), dot(ac, ac), dot(sub(c, b), sub(c, b)))
        if dot(normal, normal) <= largest_edge_squared * largest_edge_squared * 1e-24:
            raise ValueError("mesh contains a degenerate or near-collinear triangle")


def mesh_integrity(points, triangles):
    """Check closed edge incidence, consistent winding, and signed volume."""
    edge_incidence = collections.Counter()
    edge_direction = collections.Counter()
    six_times_volume = 0.0
    for i, j, k in triangles:
        a, b, c = points[i], points[j], points[k]
        cross = [
            b[1] * c[2] - b[2] * c[1],
            b[2] * c[0] - b[0] * c[2],
            b[0] * c[1] - b[1] * c[0],
        ]
        six_times_volume += dot(a, cross)
        for start, end in ((i, j), (j, k), (k, i)):
            key = (min(start, end), max(start, end))
            edge_incidence[key] += 1
            edge_direction[key] += 1 if start < end else -1
    return {
        "closed": bool(edge_incidence)
        and all(count == 2 for count in edge_incidence.values()),
        "consistentlyOriented": bool(edge_incidence)
        and all(direction == 0 for direction in edge_direction.values()),
        "undirectedEdgeCount": len(edge_incidence),
        "signedVolumeMm3": six_times_volume / 6.0,
    }


def run(binary: Path, source: Path, reference_path: Path, output: Path):
    self_test_distance_and_output_limits()
    source_bytes = bounded_read(source, MAX_SOURCE_BYTES)
    reference_bytes = bounded_read(reference_path, MAX_REPORT_BYTES)
    binary_bytes = bounded_read(binary, MAX_BINARY_BYTES)
    if hashlib.sha256(reference_bytes).hexdigest() != REFERENCE_SHA256:
        raise ValueError("pinned FreeCAD reference digest mismatch")
    reference = json.loads(reference_bytes)
    source_sha = hashlib.sha256(source_bytes).hexdigest()
    if reference.get("schema") != "tokito.freecad-reference-mesh.v1":
        raise ValueError("unexpected reference schema")
    if reference.get("inputSha256") != source_sha:
        raise ValueError("reference is not bound to the provided STEP source")
    if reference.get("valid") is not True or reference.get("solidCount") != 1:
        raise ValueError("pinned reference is not one valid solid")

    return_code, stdout, stderr = capture_bounded(
        [str(binary), "--candidate-mesh-data", str(source)],
        timeout_seconds=90,
        stdout_limit=MAX_REPORT_BYTES,
        stderr_limit=MAX_STDERR_BYTES,
    )
    if return_code != 2 or stderr:
        raise ValueError("experimental evaluator did not refuse production admission cleanly")
    candidate = json.loads(stdout)
    mesh = candidate.get("candidateMeshData")
    world = candidate.get("assemblyOccurrenceWorld")
    if (
        candidate.get("sourceSha256") != source_sha
        or candidate.get("admitted") is not False
        or candidate.get("exportsProduced") != []
        or not isinstance(mesh, dict)
        or not isinstance(world, dict)
        or world.get("independentRepresentationUnitsResolved") is not True
        or mesh.get("lengthScaleModelUnitsToMm") != 1.0
        or mesh.get("readerWarnings") != []
        or mesh.get("geometryQualified") is not False
        or mesh.get("pcbPlacementQualified") is not False
    ):
        raise ValueError("candidate changed identity or non-admission status")
    occurrences = mesh.get("occurrences")
    if not isinstance(occurrences, list) or len(occurrences) != 1:
        raise ValueError("candidate is outside the pinned single-occurrence profile")
    shapes = occurrences[0].get("shapes")
    if not isinstance(shapes, list) or len(shapes) != 1:
        raise ValueError("candidate is outside the pinned single-shape profile")

    source_shape = shapes[0]
    candidate_points = source_shape.get("verticesMm")
    candidate_triangles = source_shape.get("triangles")
    reference_points = reference.get("verticesMm")
    reference_triangles = reference.get("triangles")
    checked_mesh(candidate_points, candidate_triangles)
    checked_mesh(reference_points, reference_triangles)
    candidate_integrity = mesh_integrity(candidate_points, candidate_triangles)
    reference_integrity = mesh_integrity(reference_points, reference_triangles)
    for label, integrity in (
        ("reader", candidate_integrity),
        ("FreeCAD reference", reference_integrity),
    ):
        if not integrity["closed"] or not integrity["consistentlyOriented"]:
            raise ValueError(f"{label} mesh is open or has inconsistent triangle winding")
    volume_tolerance_mm3 = max(1e-12, abs(reference_integrity["signedVolumeMm3"]) * 1e-10)
    if (
        candidate_integrity["signedVolumeMm3"] * reference_integrity["signedVolumeMm3"] <= 0
        or abs(
            candidate_integrity["signedVolumeMm3"] - reference_integrity["signedVolumeMm3"]
        )
        > volume_tolerance_mm3
    ):
        raise ValueError("reader/reference signed volume or global orientation mismatch")
    if len(candidate_triangles) != len(reference_triangles):
        raise ValueError("reader/reference triangle-count regression")
    per_pass_work = len(candidate_triangles) * len(reference_triangles) * 7
    total_pairwise_work = per_pass_work * 3
    if total_pairwise_work > MAX_TOTAL_PAIRWISE_SAMPLE_TRIANGLE_WORK:
        raise ValueError("surface comparison exceeds pairwise work limit")

    candidate_vertices = {tuple(round(x, 8) for x in point) for point in candidate_points}
    reference_vertices = {tuple(round(x, 8) for x in point) for point in reference_points}
    if candidate_vertices != reference_vertices:
        raise ValueError("reader/reference vertex-set mismatch")
    triangle_counts = collections.Counter(
        triangle_key(candidate_points, triangle) for triangle in candidate_triangles
    )
    reference_counts = collections.Counter(
        triangle_key(reference_points, triangle) for triangle in reference_triangles
    )
    matched_triangles = sum((triangle_counts & reference_counts).values())
    if matched_triangles != 46:
        raise ValueError(f"pinned reader/reference triangle delta changed: {matched_triangles}/76")
    distinct_source_faces = len(set(source_shape.get("sourceTriangleFaceIds", [])))
    if distinct_source_faces != reference.get("faceCount"):
        raise ValueError("reader/reference face counts changed; investigate before interpreting surfaces")
    forward = directed_sample_distance(
        candidate_points, candidate_triangles, reference_points, reference_triangles
    )
    reverse = directed_sample_distance(
        reference_points, reference_triangles, candidate_points, candidate_triangles
    )

    # Comparator negative control: translation by 0.01 mm must exceed the gate.
    shifted_points = [[point[0] + 0.01, point[1], point[2]] for point in reference_points]
    negative_control = directed_sample_distance(
        candidate_points, candidate_triangles, shifted_points, reference_triangles
    )[0]
    if negative_control <= SURFACE_SAMPLE_LIMIT_MM:
        raise ValueError("surface-comparison negative control did not trip the tolerance")

    sample_match = (
        forward[0] <= SURFACE_SAMPLE_LIMIT_MM
        and reverse[0] <= SURFACE_SAMPLE_LIMIT_MM
        and forward[2] == reverse[2]
    )
    if not sample_match:
        raise ValueError("bidirectional sampled surface comparison exceeds tolerance")
    if output.exists():
        raise FileExistsError(f"refusing to overwrite report: {output}")

    result = {
        "schema": "tokito.step-surface-reference-e2e.v1",
        "passed": True,
        "sourceSha256": source_sha,
        "evaluatorBinarySha256": hashlib.sha256(binary_bytes).hexdigest(),
        "reader": "oxideav-step=0.0.1/default-features=false",
        "reference": "FreeCAD 1.1.4@4fd3bf320d9566a27e60069fc8387448aaa3a094",
        "referenceSha256": hashlib.sha256(reference_bytes).hexdigest(),
        "referenceTessellationDeflectionMm": reference["deflectionMm"],
        "readerTriangleCount": len(candidate_triangles),
        "referenceTriangleCount": len(reference_triangles),
        "readerUniqueVertexCount": len(candidate_vertices),
        "referenceUniqueVertexCount": len(reference_vertices),
        "readerSourceFaceIdCount": distinct_source_faces,
        "referenceFaceCount": reference["faceCount"],
        "matchingTriangleCoordinateTriplets": matched_triangles,
        "readerMeshClosedAndConsistentlyOriented": True,
        "referenceMeshClosedAndConsistentlyOriented": True,
        "readerUndirectedEdgeCount": candidate_integrity["undirectedEdgeCount"],
        "referenceUndirectedEdgeCount": reference_integrity["undirectedEdgeCount"],
        "readerSignedVolumeMm3": candidate_integrity["signedVolumeMm3"],
        "referenceSignedVolumeMm3": reference_integrity["signedVolumeMm3"],
        "signedVolumeToleranceMm3": volume_tolerance_mm3,
        "sampleCountEachDirection": forward[2],
        "totalPairwiseSampleTriangleWork": total_pairwise_work,
        "readerToReferenceMaximumSampleDistanceMm": forward[0],
        "referenceToReaderMaximumSampleDistanceMm": reverse[0],
        "negativeControlShiftMm": 0.01,
        "negativeControlMaximumSampleDistanceMm": negative_control,
        "sampledSurfaceMatches": sample_match,
        "overallStepAdmitted": False,
        "componentAssociationVerified": False,
        "pcbPlacementVerified": False,
        "scope": "bounded bidirectional vertex/edge-midpoint/centroid samples plus closed-edge/global-winding checks; not a full Hausdorff, face-identity, per-face-normal or trim-coverage proof",
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, indent=2, allow_nan=False)
        stream.write("\n")
    print(
        "PASS experimental STEP surface sample: "
        f"{matched_triangles}/{len(candidate_triangles)} triangles have identical records; "
        f"bidirectional maximum {max(forward[0], reverse[0]):.6g} mm; "
        "production admission remains false"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--reference", type=Path, default=Path(__file__).parents[1] / "references" / "esp8266-model-15-freecad-1.1.4.json")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    run(
        args.binary.resolve(strict=True),
        args.source.resolve(strict=True),
        args.reference.resolve(strict=True),
        args.output.resolve(),
    )
