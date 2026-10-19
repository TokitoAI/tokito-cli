"""Pinned real STEP face, CLI refusals and independent FreeCAD face comparison."""
import argparse
import hashlib
import json
import math
from pathlib import Path

from surface_reference_e2e import bounded_read, capture_bounded

SOURCE_SHA256 = "7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f"
MAX_TRIANGLE_PAIR_TESTS = 2_000_000


def digest(path, cap=128 * 1024 * 1024):
    return hashlib.sha256(bounded_read(path, cap)).hexdigest()


def ring_area(points):
    origin = points[0]
    return 0.5 * sum(
        (points[i][0] - origin[0]) * (points[(i + 1) % len(points)][1] - origin[1])
        - (points[(i + 1) % len(points)][0] - origin[0]) * (points[i][1] - origin[1])
        for i in range(len(points))
    )


def triangle_interiors_overlap(points, triangle_a, triangle_b):
    scale = max(1.0, *(abs(value) for index in triangle_a + triangle_b for value in points[index]))
    tolerance = 128.0 * 2.220446049250313e-16 * scale * scale
    for triangle in (triangle_a, triangle_b):
        for i in range(3):
            p = points[triangle[i]]
            q = points[triangle[(i + 1) % 3]]
            axis = (p[1] - q[1], q[0] - p[0])
            projected_a = [points[index][0] * axis[0] + points[index][1] * axis[1] for index in triangle_a]
            projected_b = [points[index][0] * axis[0] + points[index][1] * axis[1] for index in triangle_b]
            overlap = min(max(projected_a), max(projected_b)) - max(min(projected_a), min(projected_b))
            if overlap <= tolerance:
                return False
    return True


def validate_report(report):
    if report.get("sourceSha256") != SOURCE_SHA256:
        raise ValueError("pinned source mismatch")
    if [report.get(k) for k in ("sourceFaceId", "sourceRepresentationId", "sourceOwnerId")] != [8127, 5957, 6218]:
        raise ValueError("source identity mismatch")
    for key in ("geometryQualified", "completeFaceGeometryCertified", "completeSolidVerified", "admitted"):
        if report.get(key) is not False:
            raise ValueError("diagnostic candidate must not claim qualification")
    if report.get("exportsProduced") != []:
        raise ValueError("research mode must not produce production exports")
    if report["sourceTopologicalShellPreflight"].get("sourceTopologicalClosedShellCandidateVerified") is not True:
        raise ValueError("whole-shell source topology preflight changed")
    rings = report.get("rings")
    if not isinstance(rings, list) or len(rings) != 2:
        raise ValueError("expected one source outer loop and one source inner loop")
    if ([r.get("sourceRole") for r in rings] != ["outer", "inner"]
            or [r.get("sourceEntityType") for r in rings] != ["FACE_OUTER_BOUND", "FACE_BOUND"]
            or [r.get("triangulationTreatment") for r in rings] != ["outer", "interior_hole"]):
        raise ValueError("source bound entity semantics were not preserved and applied")
    if report.get("validation", {}).get("exactlyOneSourceOuterBound") is not True:
        raise ValueError("face must have exactly one explicit source outer bound")
    all_points = []
    total_area = 0.0
    for ring in rings:
        ring_points = ring.get("projectedBoundarySourceUnits")
        provenance = ring.get("sampleProvenance")
        if not isinstance(ring_points, list) or len(ring_points) < 3 or len(provenance) != len(ring_points) or len(ring_points) != ring.get("sampleCount"):
            raise ValueError("invalid boundary sample provenance")
        independently_computed = ring_area(ring_points)
        if not math.isclose(independently_computed, ring["signedAreaSourceUnitsSquared"], rel_tol=1e-11, abs_tol=1e-12):
            raise ValueError("ring area does not independently recompute")
        if len(ring.get("sourceEdgeCurveIds", [])) != ring["sourceEdgeCount"] or len(set(ring["sourceEdgeCurveIds"])) != ring["sourceEdgeCount"]:
            raise ValueError("source edge identity lost from boundary ring")
        source_edges = ring.get("sourceEdges")
        if not isinstance(source_edges, list) or len(source_edges) != ring["sourceEdgeCount"]:
            raise ValueError("source edge sample inventory is incomplete")
        if [edge.get("sourceEdgeCurveId") for edge in source_edges] != ring["sourceEdgeCurveIds"]:
            raise ValueError("ordered source edge IDs differ from edge samples")
        for edge in source_edges:
            edge_points = edge.get("samplePointsSourceUnits")
            if not isinstance(edge_points, list) or len(edge_points) < 2 or any(
                not isinstance(p, list) or len(p) != 3 or any(not math.isfinite(float(v)) for v in p)
                for p in edge_points
            ):
                raise ValueError("source edge sample geometry is invalid")
        for row, xy in zip(provenance, ring_points, strict=True):
            xyz = row.get("pointSourceUnits")
            if not isinstance(xyz, list) or len(xyz) != 3 or any(not math.isfinite(float(v)) for v in xyz):
                raise ValueError("invalid source XYZ sample")
            if not isinstance(row.get("sourceEdgeCurveId"), int) or len(xy) != 2:
                raise ValueError("missing sample source identity")
            all_points.append(xyz)
        total_area += abs(independently_computed) * (1.0 if ring is rings[0] else -1.0)
    if len(all_points) != len(report["mesh"]["verticesSourceUnits"]):
        raise ValueError("mesh/sample vertex inventory mismatch")
    if report["mesh"]["verticesSourceUnits"] != all_points:
        raise ValueError("mesh vertices are not the exact source boundary inventory")
    if not math.isclose(total_area, report["sampledPolygonMaterialAreaSourceUnitsSquared"], rel_tol=1e-11, abs_tol=1e-12):
        raise ValueError("sampled material area does not independently recompute")
    mesh = report["mesh"]
    triangles = mesh.get("triangles")
    if not isinstance(triangles, list) or not triangles:
        raise ValueError("missing face triangulation")
    if len(triangles) > 32_768:
        raise ValueError("triangle inventory exceeds the bounded diagnostic limit")
    triangle_pair_tests = len(triangles) * (len(triangles) - 1) // 2
    if triangle_pair_tests > MAX_TRIANGLE_PAIR_TESTS:
        raise ValueError("independent triangle overlap check exceeds its pair-work budget")
    vertices = mesh.get("verticesSourceUnits")
    if not isinstance(vertices, list) or len(vertices) > 16_384:
        raise ValueError("mesh vertex inventory exceeds the bounded diagnostic limit")
    for tri in triangles:
        if len(tri) != 3 or len(set(tri)) != 3 or any(type(i) is not int or i < 0 or i >= len(vertices) for i in tri):
            raise ValueError("invalid triangle index before independent overlap checks")
    projected_vertices = [point for ring in rings for point in ring["projectedBoundarySourceUnits"]]
    if len(projected_vertices) != len(vertices):
        raise ValueError("projected and spatial mesh vertex inventories differ")
    use_counts = {}
    boundary = set()
    offset = 0
    for ring in rings:
        count = ring["sampleCount"]
        for i in range(count):
            a, b = offset + i, offset + (i + 1) % count
            boundary.add((min(a, b), max(a, b)))
        offset += count
    area = 0.0
    for left in range(len(triangles)):
        for right in range(left + 1, len(triangles)):
            if triangle_interiors_overlap(projected_vertices, triangles[left], triangles[right]):
                raise ValueError("triangle interiors overlap in independent 2D SAT check")
    for tri in triangles:
        if len(tri) != 3 or len(set(tri)) != 3 or any(type(i) is not int or i < 0 or i >= len(vertices) for i in tri):
            raise ValueError("invalid triangle index")
        a, b, c = [vertices[i] for i in tri]
        ab = [b[i] - a[i] for i in range(3)]
        ac = [c[i] - a[i] for i in range(3)]
        cross = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]]
        triangle_area = 0.5 * math.sqrt(sum(v * v for v in cross))
        if not math.isfinite(triangle_area) or triangle_area <= 0.0:
            raise ValueError("degenerate triangle")
        area += triangle_area
        for i in range(3):
            x, y = tri[i], tri[(i + 1) % 3]
            key = (min(x, y), max(x, y))
            use_counts[key] = use_counts.get(key, 0) + 1
    if any(use_counts.get(edge) != 1 for edge in boundary) or any(count != (1 if edge in boundary else 2) for edge, count in use_counts.items()):
        raise ValueError("mesh boundary/interior edge incidence invalid")
    if not math.isclose(area, mesh["triangleAreaSourceUnitsSquared"], rel_tol=1e-11, abs_tol=1e-12):
        raise ValueError("triangle area does not independently recompute")
    if not math.isclose(area, total_area, rel_tol=1e-11, abs_tol=1e-12):
        raise ValueError("triangles do not cover sampled material area")
    sampling = report["sampling"]
    if not (0.0 <= sampling["maximumObservedQuarterMidpointChordErrorSourceUnits"] <= sampling["requestedQuarterMidpointChordTargetSourceUnits"]):
        raise ValueError("sampled chord observations exceed declared target")
    if report.get("validation", {}).get("triangleInteriorsDoNotOverlap") is not True:
        raise ValueError("Rust diagnostic did not report the triangle non-overlap check")
    if report["validation"].get("trianglePairTests") != triangle_pair_tests:
        raise ValueError("triangle pair-test count differs from independent recomputation")
    return {"sourceFaces": 1, "sourceBounds": len(rings), "boundaryVertices": len(vertices), "triangles": len(triangles),
            "trianglePairTests": triangle_pair_tests,
            "sampledMaterialAreaSourceUnitsSquared": total_area, "independentTriangleAreaSourceUnitsSquared": area,
            "maximumObservedChordErrorSourceUnits": sampling["maximumObservedQuarterMidpointChordErrorSourceUnits"],
            "completeSolidVerified": False, "productionExported": False}


def run(args):
    source = args.source.resolve(strict=True)
    binary = args.binary.resolve(strict=True)
    freecad = args.freecad.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).parents[1]
    macro = root / "references" / "audit_model16_planar_multibound.FCMacro"
    inputs = [source, binary, freecad, macro, Path(__file__), Path(__file__).with_name("surface_reference_e2e.py")]
    inputs += sorted((root / "src").rglob("*.rs")) + [root / "Cargo.toml", root / "Cargo.lock"]
    hashes = {str(p): digest(p) for p in inputs}
    receipt = {"schema": "tokito.planar-multibound-e2e.v1", "passed": False, "checks": [], "inputSha256": hashes,
               "geometryQualified": False, "completeSolidVerified": False, "admitted": False, "exportsProduced": []}
    try:
        if hashes[str(source)] != SOURCE_SHA256:
            raise ValueError("wrong materialized STEP source")
        command = [str(binary), "--research-planar-multibound-face", "8127", "--chord-tolerance", "0.00001",
                   "--source-representation", "5957", "--source-owner", "6218", str(source)]
        code, stdout, stderr = capture_bounded(command, 180, 16 * 1024 * 1024, 128 * 1024)
        diagnostic_path = output / "diagnostic.json"
        diagnostic_path.write_bytes(stdout)
        if code != 2 or stderr:
            raise ValueError("research CLI must return JSON candidate via exit 2 and no stderr")
        report = json.loads(stdout)
        receipt["inventory"] = validate_report(report)
        receipt["diagnosticSha256"] = digest(diagnostic_path)
        receipt["checks"].append({"name": "pinned-face-sampled-rings-mesh-incidence-and-area", "passed": True})
        for name, replacement, expected in [
            ("wrong-owner", "6219", "planar_multibound_research_requires_pinned_source_face_representation_owner"),
        ]:
            bad = command.copy()
            bad[bad.index("6218")] = replacement
            bad_code, bad_out, bad_err = capture_bounded(bad, 180, 1024 * 1024, 128 * 1024)
            error = json.loads(bad_out).get("error", "")
            if bad_code != 2 or bad_err or not error.startswith(expected):
                raise ValueError(name + " did not refuse")
            receipt["checks"].append({"name": name + "-refusal", "passed": True})
        wrong_source = output / "wrong-source.step"
        wrong_source.write_bytes(bounded_read(source, 32 * 1024 * 1024) + b"\n")
        bad = command.copy()
        bad[-1] = str(wrong_source)
        bad_code, bad_out, bad_err = capture_bounded(bad, 180, 1024 * 1024, 128 * 1024)
        if bad_code != 2 or bad_err or not json.loads(bad_out).get("error", "").startswith("planar_multibound_research_requires_pinned_source_face_representation_owner"):
            raise ValueError("wrong source pin did not refuse")
        receipt["checks"].append({"name": "wrong-source-refusal", "passed": True})
        config = {"source": str(source), "diagnostic": str(diagnostic_path), "output": str(output / "freecad-audit.json")}
        config_path = output / "freecad-config.json"
        config_path.write_text(json.dumps(config), encoding="utf-8")
        code, native_out, native_err = capture_bounded([str(freecad), str(macro), "--pass", str(config_path)], 240, 4 * 1024 * 1024, 128 * 1024)
        (output / "freecad.stdout.txt").write_bytes(native_out)
        (output / "freecad.stderr.txt").write_bytes(native_err)
        if code != 0 or not Path(config["output"]).is_file():
            raise ValueError("independent FreeCAD comparison failed: " + native_err.decode("utf-8", errors="replace")[-2048:])
        independent = json.loads(bounded_read(Path(config["output"]), 128 * 1024))
        if independent.get("sourceSha256") != SOURCE_SHA256 or independent.get("diagnosticSha256") != receipt["diagnosticSha256"]:
            raise ValueError("FreeCAD audit source/diagnostic binding mismatch")
        if independent.get("triangleCentroidsInsideTrimmedFace") != len(report["mesh"]["triangles"]):
            raise ValueError("FreeCAD did not confirm every triangle centroid inside source face")
        wire_bindings = independent.get("sourceFaceBinding", {}).get("ringToSourceWireBindings")
        if not isinstance(wire_bindings, list) or len(wire_bindings) != len(report["rings"]):
            raise ValueError("FreeCAD did not uniquely bind every reported ring to a source wire")
        freecad_wire_count = independent.get("sourceFaceBinding", {}).get("freecadWireCount")
        wire_indices = [item.get("freecadWireIndex") for item in wire_bindings]
        if (type(freecad_wire_count) is not int
                or any(type(index) is not int for index in wire_indices)
                or sorted(wire_indices) != list(range(freecad_wire_count))):
            raise ValueError("FreeCAD source-wire bindings are not a unique in-range assignment")
        if any(not math.isfinite(float(item.get("maximumBoundaryPointToSourceWireGapSourceUnits", math.inf))) or
               float(item["maximumBoundaryPointToSourceWireGapSourceUnits"]) > 1e-7 for item in wire_bindings):
            raise ValueError("a boundary sample is not on its independently matched source wire")
        for ring, binding in zip(report["rings"], wire_bindings, strict=True):
            edge_bindings = binding.get("sourceEdgeBindings")
            if not isinstance(edge_bindings, list) or len(edge_bindings) != ring["sourceEdgeCount"]:
                raise ValueError("FreeCAD did not bind every STEP edge identity to a source edge")
            if [row.get("sourceEdgeCurveId") for row in edge_bindings] != ring["sourceEdgeCurveIds"]:
                raise ValueError("FreeCAD source-edge mapping lost ordered STEP edge identities")
            edge_indices = [row.get("freecadEdgeIndex") for row in edge_bindings]
            if (any(type(index) is not int or index < 0 or index >= ring["sourceEdgeCount"] for index in edge_indices)
                    or sorted(edge_indices) != list(range(ring["sourceEdgeCount"]))):
                raise ValueError("STEP edges are not mapped injectively into the in-range imported wire edges")
            if any(not math.isfinite(float(row.get("maximumSampleToEdgeGapSourceUnits", math.inf))) or
                   float(row["maximumSampleToEdgeGapSourceUnits"]) > 1e-7 for row in edge_bindings):
                raise ValueError("a STEP edge sample does not match its independently bound FreeCAD edge")
            if any(not math.isfinite(float(row.get("maximumEndpointGapSourceUnits", math.inf))) or
                   float(row["maximumEndpointGapSourceUnits"]) > 1e-7 for row in edge_bindings):
                raise ValueError("a STEP edge endpoint does not match its independently bound FreeCAD edge")
        if not math.isfinite(float(independent.get("maximumSourceEdgeSampleToImportedEdgeGapSourceUnits", math.inf))) or independent["maximumSourceEdgeSampleToImportedEdgeGapSourceUnits"] > 1e-7:
            raise ValueError("FreeCAD edge correspondence gap exceeds the comparison limit")
        if independent.get("minimumTriangleToNativeFaceNormalAlignment", -1.0) < 0.999999:
            raise ValueError("FreeCAD did not confirm candidate triangle winding")
        if independent.get("geometryQualified") is not False or independent.get("completeSolidVerified") is not False or independent.get("admitted") is not False:
            raise ValueError("FreeCAD comparison must preserve unqualified scope")
        receipt["independentFreecadAudit"] = independent
        receipt["checks"].append({"name": "independent-freecad-source-edge-wire-and-trimmed-face-binding-area-centroid-test", "passed": True})
        audit_path = Path(config["output"])
        audit_hash = digest(audit_path)
        repeat_code, repeat_out, repeat_err = capture_bounded([str(freecad), str(macro), "--pass", str(config_path)], 240, 4 * 1024 * 1024, 128 * 1024)
        repeat_text = (repeat_out + repeat_err).decode("utf-8", errors="replace")
        # FreeCAD's launcher reports macro exceptions in its output but may
        # still return exit 0; the refusal text and unchanged output are the
        # authoritative checks for this repeated invocation.
        if "refusing to overwrite audit output" not in repeat_text or digest(audit_path) != audit_hash:
            raise ValueError("FreeCAD oracle did not safely refuse output overwrite")
        receipt["checks"].append({"name": "freecad-oracle-no-overwrite-refusal", "passed": True})
        if any(digest(Path(p)) != h for p, h in hashes.items()):
            raise ValueError("input changed during geometry E2E")
        receipt["passed"] = True
    except Exception as error:
        receipt["error"] = str(error)
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2, allow_nan=False), encoding="utf-8")
    print(json.dumps({"passed": receipt["passed"], "checks": receipt["checks"], "error": receipt.get("error"),
                      "inventory": receipt.get("inventory"), "freecad": receipt.get("independentFreecadAudit")}))
    return 0 if receipt["passed"] else 2


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--freecad", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    raise SystemExit(run(parser.parse_args()))
