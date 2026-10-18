"""Real pinned-source physical refinement journey and independent CAD audit."""
import argparse
import collections
import hashlib
import json
import math
from pathlib import Path

from surface_reference_e2e import bounded_read, capture_bounded

SOURCE_SHA256 = "7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f"


def digest(path, maximum):
    return hashlib.sha256(bounded_read(path, maximum)).hexdigest()


def validate_mesh(report, metric=False):
    p = report["physicalRefinementCandidate"]
    if p.get("admitted") is not False or p.get("geometryQualified") is not False or p.get("exportsProduced") != []:
        raise ValueError("research mesh must stay unqualified")
    if p.get("status") != "incomplete" or p.get("termination") != "mesh_budget_exhausted":
        raise ValueError("pinned incomplete outcome changed; re-evaluate fixture evidence")
    chart=p.get("initialChartMetric")
    if metric:
        if not isinstance(chart,dict) or chart.get("originalUvCoordinatesRetainedByIdentity") is not True:
            raise ValueError("metric chart source identity evidence missing")
        matrix=chart["normalizedCholeskyMatrix"]
        if not 0.02<matrix[0][0]<0.04 or matrix[1][0]!=0 or not 0.99<matrix[1][1]<=1.000001:
            raise ValueError("pinned metric scale changed")
        if not 0<p["quality"]["nonpositiveNormalCount"]<=10:
            raise ValueError("metric remaining orientation count changed")
    elif chart is not None:
        raise ValueError("baseline unexpectedly uses a metric chart")
    uv = p["uvVertices"]
    points = p["surfacePointsSourceUnits"]
    triangles = p["triangles"]
    count = p["boundaryVertexCount"]
    if len(uv) != len(points) or len(uv) != p["vertexCount"] or len(triangles) != p["triangleCount"]:
        raise ValueError("mesh inventory mismatch")
    if count != 96 or p.get("originalInputSampleCount") != 100 or p.get("removedExactDuplicateCount") != 4 or p.get("additionalBoundaryVertexCount") != 0:
        raise ValueError("source boundary inventory changed")
    mapping = p["inputSampleVertexIndices"]
    source = report["trimCandidate"]["boundarySamples"]
    if len(mapping) != len(source) or any(uv[k] != row["candidateUv"] for row, k in zip(source, mapping)):
        raise ValueError("source boundary coordinate changed")
    boundary = {tuple(sorted((i, (i + 1) % count))) for i in range(count)}
    edges = collections.Counter()
    senses = collections.Counter()
    for t in triangles:
        if len(t) != 3 or len(set(t)) != 3 or any(type(k) is not int or not 0 <= k < len(uv) for k in t):
            raise ValueError("invalid triangle")
        a, b, c = [uv[k] for k in t]
        area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        if not math.isfinite(area) or area <= 0:
            raise ValueError("degenerate or reversed UV face")
        for x, y in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]:
            key = tuple(sorted((x, y)))
            edges[key] += 1
            senses[key] += 1 if x < y else -1
    if {e for e, uses in edges.items() if uses == 1} != boundary or any(uses != 1 if e in boundary else uses != 2 or senses[e] != 0 for e, uses in edges.items()):
        raise ValueError("nonconforming or altered constraints")
    if len(uv) - len(edges) + len(triangles) != 1:
        raise ValueError("Euler disk invariant failed")
    quality = p["quality"]
    if not 0 < quality["maximumSampledInteriorChordErrorSourceUnits"] < p["history"][0]["quality"]["maximumSampledInteriorChordErrorSourceUnits"]:
        raise ValueError("physical interior error did not decrease")
    if quality["maximumSampledFixedBoundaryChordErrorSourceUnits"] <= p["chordToleranceSourceUnits"] or quality["nonpositiveNormalCount"] <= 0:
        raise ValueError("pinned remaining quality blockers changed")
    return {"vertices": len(uv), "triangles": len(triangles), "edges": len(edges), "eulerCharacteristic": 1,
            "fixedBoundaryEdges": len(boundary), "nonconformingEdges": 0,
            "firstQuality": p["history"][0]["quality"], "lastQuality": quality}


def run(args):
    source = args.source.resolve(strict=True)
    binary = args.binary.resolve(strict=True)
    freecad = args.freecad.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    macro = Path(__file__).parents[1] / "references" / "audit_model16_physical_refinement.FCMacro"
    root = Path(__file__).parents[1]
    inputs = [source, binary, freecad, macro, Path(__file__), Path(__file__).with_name("surface_reference_e2e.py")]
    inputs += sorted((root / "src").rglob("*.rs")) + [root / "Cargo.toml", root / "Cargo.lock"]
    hashes = {str(p): digest(p, 128 * 1024 * 1024) for p in inputs}
    receipt = {"schema": "tokito.physical-refinement-e2e.v1", "passed": False, "checks": [],
               "inputSha256": hashes, "geometryQualified": False, "admitted": False, "exportsProduced": []}
    try:
        if hashes[str(source)] != SOURCE_SHA256:
            raise ValueError("wrong materialized source")

        def cli(name, tolerance="0.00001", face="2875"):
            mode="--research-metric-source-face" if args.metric else "--research-physical-source-face"
            code, stdout, stderr = capture_bounded([str(binary), mode, face,
                "--chord-tolerance", tolerance, "--source-representation", "5957", str(source)],
                300, 32 * 1024 * 1024, 128 * 1024)
            (output / (name + ".json")).write_bytes(stdout)
            if code != 2 or stderr:
                raise ValueError("research CLI must return structured exit2 without stderr")
            return json.loads(stdout)

        report = cli("diagnostic")
        if report.get("sourceSha256") != SOURCE_SHA256 or report.get("admitted") is not False:
            raise ValueError("CLI provenance/qualification mismatch")
        receipt["metricPreconditioning"]=args.metric
        receipt["mesh"] = validate_mesh(report,args.metric)
        receipt["diagnosticSha256"] = digest(output / "diagnostic.json", 32 * 1024 * 1024)
        receipt["checks"].append({"name": "real-source-physical-refinement-and-topology", "passed": True})
        for name, tolerance, face, error in [
            ("invalid-tolerance", "nan", "2875", "invalid_physical_chord_tolerance"),
            ("wrong-face", "0.00001", "2876", "physical_research_requires_pinned_model16_face_and_representation"),
        ]:
            bad = cli(name, tolerance, face)
            if bad.get("error") != error or "physicalRefinementCandidate" in bad:
                raise ValueError("expected clean CLI refusal")
            receipt["checks"].append({"name": name + "-refusal", "passed": True})
        config = {"source": str(source), "diagnostic": str(output / "diagnostic.json"), "output": str(output / "audit.json")}

        def audit(name, cfg):
            path = output / (name + ".config.json")
            path.write_text(json.dumps(cfg), encoding="utf-8")
            code, stdout, stderr = capture_bounded([str(freecad), str(macro), "--pass", str(path)], 180, 128 * 1024, 128 * 1024)
            (output / (name + ".stdout.txt")).write_bytes(stdout)
            (output / (name + ".stderr.txt")).write_bytes(stderr)
            return code, stderr.decode("utf-8")

        code, error = audit("independent", config)
        if code != 0 or not (output / "audit.json").exists():
            raise ValueError("independent audit failed: " + error[-1024:])
        independent = json.loads(bounded_read(output / "audit.json", 128 * 1024))
        if independent.get("diagnosticSha256") != receipt["diagnosticSha256"] or independent.get("maximumIndependentVertexGapMm", math.inf) > 1e-10:
            raise ValueError("independent surface/provenance mismatch")
        for native, oracle in [("maximumSampledInteriorChordErrorSourceUnits", "maximumSampledInteriorChordErrorMm"),
                               ("maximumSampledFixedBoundaryChordErrorSourceUnits", "maximumSampledFixedBoundaryChordErrorMm")]:
            if not math.isclose(report["physicalRefinementCandidate"]["quality"][native], independent[oracle], rel_tol=1e-6, abs_tol=1e-10):
                raise ValueError("independent chord measurements disagree")
        if independent.get("analyticNonpositiveNormalCount", 0) <= 0 or independent.get("admitted") is not False:
            raise ValueError("independent remaining orientation blocker missing")
        quality = report["physicalRefinementCandidate"]["quality"]
        if (independent["analyticNonpositiveNormalCount"] != quality["nonpositiveNormalCount"]
                or not math.isclose(independent["minimumAnalyticNormalAlignment"], quality["minimumSampledNormalAlignment"], rel_tol=1e-6, abs_tol=1e-8)):
            raise ValueError("finite-difference and independent analytic orientation observations disagree")
        receipt["independentAudit"] = independent
        receipt["checks"].append({"name": "independent-surface-chord-normal-audit", "passed": True})
        h = digest(output / "audit.json", 128 * 1024)
        _, error = audit("no-overwrite", config)
        if "refusing to overwrite output" not in error or digest(output / "audit.json", 128 * 1024) != h:
            raise ValueError("independent no-overwrite refusal failed")
        receipt["checks"].append({"name": "oracle-no-overwrite-refusal", "passed": True})
        bad = dict(report)
        bad["sourceSha256"] = "0" * 64
        bad_path = output / "wrong-source.json"
        bad_path.write_text(json.dumps(bad), encoding="utf-8")
        bad_config = {**config, "diagnostic": str(bad_path), "output": str(output / "must-not-exist.json")}
        _, error = audit("wrong-source", bad_config)
        if "pinned source mismatch" not in error or Path(bad_config["output"]).exists():
            raise ValueError("independent wrong-source refusal failed")
        receipt["checks"].append({"name": "oracle-source-mismatch-refusal", "passed": True})
        if any(digest(Path(p), 128 * 1024 * 1024) != h for p, h in hashes.items()):
            raise ValueError("audit input changed")
        receipt["passed"] = True
    except Exception as error:
        receipt["error"] = str(error)
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2, allow_nan=False), encoding="utf-8")
    print(json.dumps({"passed": receipt["passed"], "error": receipt.get("error"), "checks": receipt["checks"], "mesh": receipt.get("mesh"), "independent": receipt.get("independentAudit")}))
    return 0 if receipt["passed"] else 2


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--freecad", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--metric", action="store_true", help="Exercise the isolated single-anchor metric-preconditioned mode")
    raise SystemExit(run(parser.parse_args()))
