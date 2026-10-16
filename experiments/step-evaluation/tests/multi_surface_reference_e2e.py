#!/usr/bin/env python3
"""Compare three pinned multi-solid ESP8266 STEP meshes with FreeCAD.

This is experimental evidence only. It never emits product geometry or admits
STEP to the production exporter.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import sys

import surface_reference_e2e as surface

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "references" / "esp8266-multi-model-freecad-manifest.json"
DEFAULT_SOURCE_ROOT = (
    ROOT.parent.parent
    / "target/dataset-intake/esp8266-pr6-5283bc0-source-candidate/data/board-sources"
    / "esp8266-nodemcu-reference/structured-source/embedded-models"
)
DEFAULT_FREECADCMD = Path(r"C:\Program Files\FreeCAD 1.1\bin\freecadcmd.exe")
MAX_REFERENCE_JSON_BYTES = 64 * 1024 * 1024
MAX_CAPTURE_BYTES = 96 * 1024 * 1024
MAX_STDERR_BYTES = 512 * 1024
MAX_SURFACE_SAMPLES = 300_000
MAX_REFERENCE_VERTICES = 300_000
MAX_REFERENCE_TRIANGLES = 500_000
MAX_TRIANGLE_DISTANCE_EVALUATIONS = 50_000_000
SAMPLE_TOLERANCE_MM = 0.02
VOLUME_RELATIVE_TOLERANCE = 1e-4
LEAF_TRIANGLE_COUNT = 12


class Node:
    __slots__ = ("minimum", "maximum", "left", "right", "start", "end")

    def __init__(self, minimum, maximum, left=-1, right=-1, start=0, end=0):
        self.minimum = minimum
        self.maximum = maximum
        self.left = left
        self.right = right
        self.start = start
        self.end = end


class TriangleBvh:
    def __init__(self, points, triangles):
        if not triangles:
            raise ValueError("cannot build a surface index for an empty triangle list")
        self.triangle_points = []
        self.bounds = []
        self.centers = []
        for triangle in triangles:
            a, b, c = (points[index] for index in triangle)
            self.triangle_points.append((a, b, c))
            minimum = tuple(min(a[axis], b[axis], c[axis]) for axis in range(3))
            maximum = tuple(max(a[axis], b[axis], c[axis]) for axis in range(3))
            self.bounds.append((minimum, maximum))
            self.centers.append(tuple((minimum[axis] + maximum[axis]) * 0.5 for axis in range(3)))
        self.order = list(range(len(triangles)))
        self.nodes = []
        self.root = self._build(0, len(self.order))

    def _build(self, start, end):
        minimum = [math.inf, math.inf, math.inf]
        maximum = [-math.inf, -math.inf, -math.inf]
        center_minimum = [math.inf, math.inf, math.inf]
        center_maximum = [-math.inf, -math.inf, -math.inf]
        for position in range(start, end):
            triangle_index = self.order[position]
            lower, upper = self.bounds[triangle_index]
            center = self.centers[triangle_index]
            for axis in range(3):
                minimum[axis] = min(minimum[axis], lower[axis])
                maximum[axis] = max(maximum[axis], upper[axis])
                center_minimum[axis] = min(center_minimum[axis], center[axis])
                center_maximum[axis] = max(center_maximum[axis], center[axis])
        node_index = len(self.nodes)
        self.nodes.append(None)
        if end - start <= LEAF_TRIANGLE_COUNT:
            self.nodes[node_index] = Node(tuple(minimum), tuple(maximum), start=start, end=end)
            return node_index
        axis = max(range(3), key=lambda item: center_maximum[item] - center_minimum[item])
        ordered = sorted(self.order[start:end], key=lambda item: self.centers[item][axis])
        middle = start + len(ordered) // 2
        self.order[start:end] = ordered
        left = self._build(start, middle)
        right = self._build(middle, end)
        self.nodes[node_index] = Node(tuple(minimum), tuple(maximum), left, right)
        return node_index

    @staticmethod
    def _box_distance_squared(point, node):
        total = 0.0
        for axis in range(3):
            if point[axis] < node.minimum[axis]:
                delta = node.minimum[axis] - point[axis]
                total += delta * delta
            elif point[axis] > node.maximum[axis]:
                delta = point[axis] - node.maximum[axis]
                total += delta * delta
        return total

    def nearest_distance(self, point, work):
        best_squared = math.inf

        def visit(node_index):
            nonlocal best_squared
            node = self.nodes[node_index]
            if self._box_distance_squared(point, node) >= best_squared:
                return
            if node.left < 0:
                for ordered_index in range(node.start, node.end):
                    work[0] += 1
                    if work[0] > MAX_TRIANGLE_DISTANCE_EVALUATIONS:
                        raise ValueError("surface comparison exceeded triangle-distance budget")
                    a, b, c = self.triangle_points[self.order[ordered_index]]
                    distance = surface.point_triangle_distance(point, a, b, c)
                    distance_squared = distance * distance
                    if distance_squared < best_squared:
                        best_squared = distance_squared
                return
            left = self.nodes[node.left]
            right = self.nodes[node.right]
            left_distance = self._box_distance_squared(point, left)
            right_distance = self._box_distance_squared(point, right)
            if left_distance <= right_distance:
                visit(node.left)
                visit(node.right)
            else:
                visit(node.right)
                visit(node.left)

        visit(self.root)
        if not math.isfinite(best_squared):
            raise ValueError("surface index returned no nearest triangle")
        return math.sqrt(best_squared)


def directed_bvh_sample_distance(source_points, source_triangles, target_points, target_triangles, work):
    sample_indices = selected_triangle_indices(len(source_triangles))
    target = TriangleBvh(target_points, target_triangles)
    # The same exact vertex is sampled by adjacent triangles. Memoize only
    # identical coordinate tuples; retain every sample in the aggregate and
    # retain all existing tolerances and the triangle-distance work cap.
    cache = {}
    distances = []
    for point in multi_surface_samples(source_points, source_triangles, sample_indices):
        key = tuple(point)
        if key not in cache:
            if len(cache) >= MAX_SURFACE_SAMPLES:
                raise ValueError("surface query cache limit exceeded")
            cache[key] = target.nearest_distance(point, work)
        distances.append(cache[key])
    return max(distances), sum(distances) / len(distances), len(distances), len(target.nodes)


def selected_triangle_indices(triangle_count):
    maximum = MAX_SURFACE_SAMPLES // 4
    if triangle_count <= maximum:
        return range(triangle_count)
    return ((index * triangle_count) // maximum for index in range(maximum))


def multi_surface_samples(points, triangles, selected_indices):
    for index in selected_indices:
        triangle = triangles[index]
        a, b, c = (points[index] for index in triangle)
        yield a
        yield b
        yield c
        yield [(a[axis] + b[axis] + c[axis]) / 3 for axis in range(3)]


def checked_reference_mesh(points, triangles):
    if (
        not isinstance(points, list)
        or not isinstance(triangles, list)
        or not points
        or len(points) > MAX_REFERENCE_VERTICES
        or not triangles
        or len(triangles) > MAX_REFERENCE_TRIANGLES
    ):
        raise ValueError("FreeCAD reference exceeds the bounded mesh limits")
    if any(not surface.finite_point(point) for point in points):
        raise ValueError("FreeCAD reference contains a non-finite vertex")
    for triangle in triangles:
        if (
            not isinstance(triangle, list)
            or len(triangle) != 3
            or any(type(index) is not int or index < 0 or index >= len(points) for index in triangle)
        ):
            raise ValueError("FreeCAD reference contains an invalid triangle index")
        a, b, c = (points[index] for index in triangle)
        ab, ac = surface.sub(b, a), surface.sub(c, a)
        normal = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ]
        largest_edge_squared = max(
            surface.dot(ab, ab),
            surface.dot(ac, ac),
            surface.dot(surface.sub(c, b), surface.sub(c, b)),
        )
        if surface.dot(normal, normal) <= largest_edge_squared * largest_edge_squared * 1e-24:
            raise ValueError("FreeCAD reference contains a degenerate triangle")


def checked_solid_reference(reference, expected_solid_count):
    """Validate v2 ownership; never infer solid membership from a compound mesh.

    Compound arrays and the sum of all solid arrays each have one fixed budget.
    Equal coordinates across solids retain independent indices. Invalid solids
    remain in the evidence; callers must refuse comparison when the gate fails.
    """
    if not isinstance(reference, dict) or reference.get("schema") != "tokito.freecad-reference-mesh.v2":
        raise ValueError("per-solid comparison requires a v2 reference; legacy compound membership is unknown")
    solids = reference.get("solids")
    if (
        type(expected_solid_count) is not int or expected_solid_count <= 0
        or type(reference.get("solidCount")) is not int
        or reference["solidCount"] != expected_solid_count
        or not isinstance(solids, list) or len(solids) != expected_solid_count
        or any(not isinstance(row, dict) or type(row.get("solidIndex")) is not int
               or row["solidIndex"] != index for index, row in enumerate(solids))
    ):
        raise ValueError("FreeCAD solid inventory is incomplete, duplicated or out of order")
    compound_points, compound_triangles = reference.get("verticesMm"), reference.get("triangles")
    checked_reference_mesh(compound_points, compound_triangles)
    total_vertices = total_triangles = 0
    # Bound aggregate work BEFORE doing per-solid triangle/integrity scans.
    for solid in solids:
        if not isinstance(solid.get("verticesMm"), list) or not isinstance(solid.get("triangles"), list):
            raise ValueError("FreeCAD solid mesh arrays are missing")
        total_vertices += len(solid["verticesMm"])
        total_triangles += len(solid["triangles"])
        if total_vertices > MAX_REFERENCE_VERTICES or total_triangles > MAX_REFERENCE_TRIANGLES:
            raise ValueError("FreeCAD per-solid aggregate exceeds mesh limits")
    evidence = {
        "freecadSolidCount": reference["solidCount"],
        "freecadFaceCount": reference.get("faceCount"),
        "referenceSolidMeshVertexCount": total_vertices,
        "referenceSolidMeshTriangleCount": total_triangles,
        "compoundReferenceIntegrity": surface.mesh_integrity(compound_points, compound_triangles),
        "referenceSolidIntegrity": [],
        "referenceSolidIdentityScope": "ordinal in this pinned FreeCAD import; not a STEP entity ID",
        "referenceVolumeScope": "sum of signed per-solid mesh volumes; not geometric-union volume",
    }
    points, triangles = [], []
    for solid in solids:
        row = {"solidIndex": solid["solidIndex"], "vertexCount": len(solid["verticesMm"]),
               "triangleCount": len(solid["triangles"]), "faceCount": solid.get("faceCount")}
        evidence["referenceSolidIntegrity"].append(row)
        try:
            if (solid.get("valid") is not True or type(solid.get("faceCount")) is not int
                    or solid["faceCount"] <= 0):
                raise ValueError("solid B-rep validity or face count is invalid")
            checked_reference_mesh(solid["verticesMm"], solid["triangles"])
            integrity = surface.mesh_integrity(solid["verticesMm"], solid["triangles"])
            row.update(integrity)
            row["outwardWinding"] = math.isfinite(integrity["signedVolumeMm3"]) and integrity["signedVolumeMm3"] > 0
            row["passed"] = integrity["closed"] and integrity["consistentlyOriented"] and row["outwardWinding"]
            offset = len(points)
            points.extend(solid["verticesMm"])
            triangles.extend([[offset + index for index in triangle] for triangle in solid["triangles"]])
        except ValueError as error:
            row.update({"passed": False, "error": str(error)[:600]})
    evidence["allReferenceSolidsClosedAndConsistentlyOriented"] = all(row["passed"] for row in evidence["referenceSolidIntegrity"])
    evidence["referenceSolidVolumeSumComplete"] = all("signedVolumeMm3" in row for row in evidence["referenceSolidIntegrity"])
    evidence["referenceSolidVolumeSumMm3"] = (
        sum(row["signedVolumeMm3"] for row in evidence["referenceSolidIntegrity"])
        if evidence["referenceSolidVolumeSumComplete"] else None
    )
    return points, triangles, evidence


def combine_occurrence_mesh(mesh):
    occurrences = mesh.get("occurrences")
    if not isinstance(occurrences, list) or not occurrences:
        raise ValueError("STEP candidate has no positioned occurrences")
    points = []
    triangles = []
    shape_count = 0
    for occurrence in occurrences:
        shapes = occurrence.get("shapes")
        if not isinstance(shapes, list):
            raise ValueError("STEP occurrence has no shape inventory")
        for shape in shapes:
            shape_points = shape.get("verticesMm")
            shape_triangles = shape.get("triangles")
            degenerate_count, degenerate_faces = degenerate_face_summary(shape)
            if degenerate_count:
                raise ValueError(
                    f"STEP tessellator emitted {degenerate_count} degenerate triangles "
                    f"on source face IDs {degenerate_faces}"
                )
            surface.checked_mesh(shape_points, shape_triangles)
            offset = len(points)
            points.extend(shape_points)
            triangles.extend([[offset + index for index in triangle] for triangle in shape_triangles])
            shape_count += 1
            if len(points) > surface.MAX_VERTICES or len(triangles) > surface.MAX_TRIANGLES:
                raise ValueError("combined STEP candidate exceeds mesh limits")
    surface.checked_mesh(points, triangles)
    return points, triangles, len(occurrences), shape_count


def degenerate_face_summary(shape):
    points = shape.get("verticesMm")
    triangles = shape.get("triangles")
    faces = shape.get("sourceTriangleFaceIds")
    if not isinstance(points, list) or not isinstance(triangles, list) or not isinstance(faces, list):
        raise ValueError("STEP shape is missing bounded mesh or face provenance arrays")
    counts = {}
    for ordinal, triangle in enumerate(triangles):
        if (
            not isinstance(triangle, list)
            or len(triangle) != 3
            or any(type(index) is not int or index < 0 or index >= len(points) for index in triangle)
        ):
            raise ValueError("STEP shape contains an invalid triangle index")
        a, b, c = (points[index] for index in triangle)
        if not all(surface.finite_point(point) for point in (a, b, c)):
            raise ValueError("STEP shape contains a non-finite vertex")
        ab, ac, bc = surface.sub(b, a), surface.sub(c, a), surface.sub(c, b)
        normal = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ]
        largest_edge_squared = max(
            surface.dot(ab, ab), surface.dot(ac, ac), surface.dot(bc, bc)
        )
        if surface.dot(normal, normal) <= largest_edge_squared * largest_edge_squared * 1e-24:
            face_id = faces[ordinal] if ordinal < len(faces) else "missing"
            counts[face_id] = counts.get(face_id, 0) + 1
    return sum(counts.values()), sorted(counts.items(), key=lambda pair: (-pair[1], str(pair[0])))[:16]


def run(binary, source_root, freecadcmd, manifest_path, output):
    if output.exists():
        raise FileExistsError(f"refusing to overwrite evidence folder: {output}")
    manifest_bytes = surface.bounded_read(manifest_path, 64 * 1024)
    manifest = json.loads(manifest_bytes)
    if (
        manifest.get("schema") != "tokito.esp8266-step-reference-manifest.v1"
        or manifest.get("datasetCommit") != "5283bc059e30e51f4d3a81a674425936fc213214"
    ):
        raise ValueError("source manifest is not the pinned ESP8266 dataset revision")
    rows = manifest.get("models")
    if not isinstance(rows, list) or [row.get("file") for row in rows] != ["0.step", "14.step", "16.step"]:
        raise ValueError("unexpected pinned STEP model set or ordering")
    binary_bytes = surface.bounded_read(binary, surface.MAX_BINARY_BYTES)
    output.mkdir(parents=True)
    results = []
    macro = ROOT / "references" / "generate_multi_reference.FCMacro"
    tool_paths = [Path(__file__), Path(surface.__file__), macro]
    tool_hashes = {path.name: hashlib.sha256(surface.bounded_read(path, 128 * 1024)).hexdigest() for path in tool_paths}
    config_root = output / "configs"
    config_root.mkdir()
    for row in rows:
        filename = row["file"]
        source_sha = row.get("sourceSha256")
        reference_evidence = {}
        try:
            filename = row["file"]
            source = source_root / filename
            source_bytes = surface.bounded_read(source, surface.MAX_SOURCE_BYTES)
            source_sha = hashlib.sha256(source_bytes).hexdigest()
            if source_sha != row.get("sourceSha256"):
                raise ValueError(f"source SHA mismatch for {filename}")
            reference_path = output / "references" / (Path(filename).stem + ".freecad.json")
            config_path = config_root / (Path(filename).stem + ".json")
            config_path.write_text(json.dumps({
                "manifest": str(manifest_path),
                "model": filename,
                "source": str(source),
                "output": str(reference_path),
            }, separators=(",", ":")), encoding="utf-8")
            # Explicit FreeCAD options isolate load/save settings and logs from
            # the tester's normal profile (and its potentially full drive).
            freecad_config = config_root / (Path(filename).stem + "-freecad")
            freecad_config.mkdir()
            return_code, stdout, stderr = surface.capture_bounded(
                [
                    str(freecadcmd),
                    "--user-cfg", str(freecad_config / "user.cfg"),
                    "--system-cfg", str(freecad_config / "system.cfg"),
                    "--log-file", str(freecad_config / "FreeCAD.log"),
                    str(macro), "--pass", str(config_path),
                ],
                timeout_seconds=300,
                stdout_limit=MAX_STDERR_BYTES,
                stderr_limit=MAX_STDERR_BYTES,
            )
            if return_code != 0 or stderr:
                raise ValueError(f"FreeCAD reference generation failed for {filename}: {stderr.decode('utf-8', errors='replace')[-2000:]}")
            reference_bytes = surface.bounded_read(reference_path, MAX_REFERENCE_JSON_BYTES)
            reference = json.loads(reference_bytes)
            if (
                reference.get("schema") != "tokito.freecad-reference-mesh.v2"
                or reference.get("inputSha256") != source_sha
                or reference.get("modelFile") != filename
                or reference.get("datasetCommit") != manifest["datasetCommit"]
                or reference.get("freecadVersion") != manifest["freecadVersion"]
                or reference.get("deflectionMm") != manifest["tessellationDeflectionMm"]
                or reference.get("solidCount") != row["solidCount"]
                or reference.get("faceCount") != row["faceCount"]
                or reference.get("valid") is not True
            ):
                raise ValueError(f"generated FreeCAD reference identity mismatch for {filename}")
            reference_points, reference_triangles, reference_evidence = checked_solid_reference(reference, row["solidCount"])
            reference_evidence["referenceSha256"] = hashlib.sha256(reference_bytes).hexdigest()
            if not reference_evidence["allReferenceSolidsClosedAndConsistentlyOriented"]:
                raise ValueError(f"{filename}: one or more FreeCAD solid meshes failed closure/orientation/outward-winding checks")
            code, stdout, stderr = surface.capture_bounded(
                [str(binary), "--candidate-mesh-data", str(source)],
                timeout_seconds=300,
                stdout_limit=MAX_CAPTURE_BYTES,
                stderr_limit=MAX_STDERR_BYTES,
            )
            if code != 2 or stderr:
                raise ValueError(f"STEP evaluator admission behavior changed for {filename}")
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
                or world.get("readerScaleAgreesWithIndependentUnits") is not True
                or mesh.get("lengthScaleModelUnitsToMm") is None
                or mesh.get("readerWarnings") != []
                or mesh.get("geometryQualified") is not False
                or mesh.get("pcbPlacementQualified") is not False
            ):
                raise ValueError(f"STEP candidate changed units, warnings, or non-admission for {filename}")
            candidate_points, candidate_triangles, occurrence_count, shape_count = combine_occurrence_mesh(mesh)
            candidate_integrity = surface.mesh_integrity(candidate_points, candidate_triangles)
            if not candidate_integrity["closed"] or not candidate_integrity["consistentlyOriented"]:
                raise ValueError(f"{filename}: reader mesh is open or inconsistently oriented")
            reference_volume = reference_evidence["referenceSolidVolumeSumMm3"]
            volume_scale = max(
                1e-12,
                abs(reference_volume) * VOLUME_RELATIVE_TOLERANCE,
            )
            if (
                candidate_integrity["signedVolumeMm3"] * reference_volume <= 0
                or abs(candidate_integrity["signedVolumeMm3"] - reference_volume) > volume_scale
            ):
                raise ValueError(f"{filename}: reader/FreeCAD signed-volume mismatch")
            work = [0]
            forward = directed_bvh_sample_distance(candidate_points, candidate_triangles, reference_points, reference_triangles, work)
            reverse = directed_bvh_sample_distance(reference_points, reference_triangles, candidate_points, candidate_triangles, work)
            maximum = max(forward[0], reverse[0])
            if maximum > SAMPLE_TOLERANCE_MM:
                raise ValueError(f"{filename}: bidirectional surface distance {maximum:.9g} mm exceeds {SAMPLE_TOLERANCE_MM} mm")
            results.append({
                **reference_evidence,
                "file": filename,
                "sourceSha256": source_sha,
                "status": "passed",
                "referenceSha256": hashlib.sha256(reference_bytes).hexdigest(),
                "freecadSolidCount": reference["solidCount"],
                "freecadFaceCount": reference["faceCount"],
                "readerOccurrenceCount": occurrence_count,
                "readerShapeCount": shape_count,
                "readerTriangleCount": len(candidate_triangles),
                "referenceTriangleCount": len(reference_triangles),
                "readerVolumeMm3": candidate_integrity["signedVolumeMm3"],
                "referenceVolumeMm3": reference_volume,
                "volumeAbsoluteToleranceMm3": volume_scale,
                "sampleCountReaderToFreecad": forward[2],
                "sampleCountFreecadToReader": reverse[2],
                "readerToFreecadMaxSampleDistanceMm": forward[0],
                "freecadToReaderMaxSampleDistanceMm": reverse[0],
                "triangleDistanceEvaluations": work[0],
                "readerBvhNodeCount": reverse[3],
                "referenceBvhNodeCount": forward[3],
                "withinSampleTolerance": True,
                "surfaceQualified": False,
                "componentAssociationVerified": False,
                "pcbPlacementVerified": False,
            })
        except Exception as error:
            results.append({
                **reference_evidence,
                "file": filename,
                "sourceSha256": source_sha,
                "status": "failed",
                "error": f"{type(error).__name__}: {str(error)[:1200]}",
                "stepAdmitted": False,
                "surfaceQualified": False,
                "componentAssociationVerified": False,
                "pcbPlacementVerified": False,
            })
    passed = len(results) == len(rows) and all(row.get("status") == "passed" for row in results)
    receipt = {
        "schema": "tokito.step-multi-surface-e2e.v2",
        "passed": passed,
        "datasetCommit": manifest["datasetCommit"],
        "manifestSha256": hashlib.sha256(manifest_bytes).hexdigest(),
        "evaluatorSha256": hashlib.sha256(binary_bytes).hexdigest(),
        "referenceMacroSha256": tool_hashes[macro.name],
        "comparisonToolSha256": tool_hashes,
        "referenceConfigurationScope": "fresh per-model user/system settings and logs inside this run; normal FreeCAD profile not loaded",
        "freecadVersion": manifest["freecadVersion"],
        "tessellationDeflectionMm": manifest["tessellationDeflectionMm"],
        "maximumBidirectionalSampleDistanceMm": SAMPLE_TOLERANCE_MM,
        "surfaceSampling": "three vertices and centroid for every triangle up to the sample cap; above it, evenly spaced source-triangle ordinals are selected deterministically",
        "maximumTriangleDistanceEvaluations": MAX_TRIANGLE_DISTANCE_EVALUATIONS,
        "models": results,
        "stepAdmitted": False,
        "geometryQualified": False,
        "assemblyPoseQualified": False,
        "scope": "three source-hash-pinned multi-solid mesh comparisons with bidirectional triangle-surface samples, closed-edge/orientation and signed-volume checks; no face-surface proof, component mapping, board placement, or production STEP admission",
    }
    if any(hashlib.sha256(surface.bounded_read(path, 128 * 1024)).hexdigest() != tool_hashes[path.name] for path in tool_paths):
        receipt["passed"] = False
        receipt["toolInputsChangedDuringRun"] = True
        passed = False
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    if not passed:
        print("FAIL: " + "; ".join(
            f"{row['file']}: {row.get('error', 'unknown failure')}"
            for row in results if row.get("status") != "passed"
        ) + "; report retained with production STEP admission false")
        raise SystemExit(2)
    print("PASS: " + ", ".join(
        f"{row['file']} {max(row['readerToFreecadMaxSampleDistanceMm'], row['freecadToReaderMaxSampleDistanceMm']):.6g} mm"
        for row in results
    ) + "; production STEP admission remains false")


def self_test_bvh():
    points = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]]
    triangles = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]
    index = TriangleBvh(points, triangles)
    for point in ([0.2, 0.2, 0], [0.1, 0.1, 0.1], [2, 2, 2], [0.4, 0.2, 0.4]):
        actual = index.nearest_distance(point, [0])
        expected = min(surface.point_triangle_distance(point, *(points[i] for i in triangle)) for triangle in triangles)
        if not math.isclose(actual, expected, rel_tol=0.0, abs_tol=1e-12):
            raise AssertionError("BVH nearest-surface distance differs from brute force")
    count, faces = degenerate_face_summary({
        "verticesMm": [[0, 0, 0], [1, 0, 0], [2, 0, 0]],
        "triangles": [[0, 1, 2]],
        "sourceTriangleFaceIds": [42],
    })
    if count != 1 or faces != [(42, 1)]:
        raise AssertionError("degenerate STEP triangle/source-face diagnostic regression")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--models-root", type=Path, default=DEFAULT_SOURCE_ROOT)
    parser.add_argument("--freecadcmd", type=Path, default=DEFAULT_FREECADCMD)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    self_test_bvh()
    run(
        args.binary.resolve(strict=True),
        args.models_root.resolve(strict=True),
        args.freecadcmd.resolve(strict=True),
        args.manifest.resolve(strict=True),
        args.output.resolve(),
    )
