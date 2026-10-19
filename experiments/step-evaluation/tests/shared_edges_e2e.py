"""Real pinned-source shared-edge support journey and independent native oracle."""
import argparse
import hashlib
import json
import math
import struct
from pathlib import Path

from surface_reference_e2e import bounded_read, capture_bounded

SOURCE_SHA256 = "7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f"


def digest(path, cap=128 * 1024 * 1024):
    return hashlib.sha256(bounded_read(path, cap)).hexdigest()


def validate(report):
    if report.get("sourceSha256") != SOURCE_SHA256:
        raise ValueError("source pin mismatch")
    for key in ("geometryQualified", "admitted", "sourceCurveSupportGapResolved", "completeTrimCoverageVerified", "completeSolidVerified"):
        if report.get(key) is not False:
            raise ValueError("research scope or false qualification missing")
    if report.get("exportsProduced") != []:
        raise ValueError("research must not export geometry")
    topology = report["topology"]
    if [topology.get(k) for k in ("selectedFaceId", "representationId", "ownerId")] != [2875, 5957, 6218]:
        raise ValueError("selected identity changed")
    sources = topology["selectedEdges"]
    rows = report["edges"]
    expected = {3400: 791, 3478: 10828, 5409: 607, 9149: 2687}
    if {e["edgeCurveId"]: e["curveGeometryId"] for e in sources} != expected or len(rows) != 4:
        raise ValueError("source edge identity inventory mismatch")
    faces = {f["faceId"]: f for f in topology["faces"]}
    if (topology["faceCount"], topology["sourceEdgeCurveCount"], topology["orientedEdgeUseCount"]) != (9, 18, 36):
        raise ValueError("complete source shell inventory changed")
    shell = topology["shellTopologyPreflight"]
    if (shell.get("loopCount") != 10 or shell.get("allLoopsCloseBySourceVertexIdentity") is not True
            or shell.get("allSourceEdgesHaveExactlyTwoOppositelyOrientedUses") is not True
            or shell.get("eachFaceHasExactlyOneFaceOuterBound") is not True
            or shell.get("sourceTopologicalClosedShellCandidateVerified") is not True
            or report.get("completeSolidVerified") is not False):
        raise ValueError("source topological preflight scope or complete-solid refusal changed")
    loop_uses = {}
    loop_count = 0
    for face in faces.values():
        bounds = face["bounds"]
        if sum(bound["boundRole"] == "outer" for bound in bounds) != 1:
            raise ValueError("each source face must expose exactly one outer bound")
        for bound in bounds:
            loop_count += 1
            uses = bound["uses"]
            if not uses or bound.get("closedBySourceVertexIdentity") is not True:
                raise ValueError("source edge loop did not close")
            for current, following in zip(uses, uses[1:]):
                if current["traversalEndVertexId"] != following["traversalStartVertexId"]:
                    raise ValueError("consecutive loop vertices do not match")
            if uses[-1]["traversalEndVertexId"] != uses[0]["traversalStartVertexId"]:
                raise ValueError("source loop wraparound vertex does not match")
            loop_uses[bound["loopId"]] = uses
    if loop_count != 10:
        raise ValueError("source bound inventory incomplete")
    all_edge_rows = topology["allEdgeIncidence"]
    if len(all_edge_rows) != 18 or len({edge["edgeCurveId"] for edge in all_edge_rows}) != 18:
        raise ValueError("global source edge inventory incomplete or duplicated")
    enumerated_uses = {}
    for edge in all_edge_rows:
        uses = edge["uses"]
        if (edge.get("useCount") != 2 or edge.get("exactlyTwoUses") is not True
                or edge.get("usesHaveOppositeOrientation") is not True or len(uses) != 2):
            raise ValueError("source edge does not have two opposite oriented uses")
        for use in uses:
            enumerated_uses[use["orientedEdgeId"]] = edge["edgeCurveId"]
    for uses in loop_uses.values():
        for use in uses:
            if enumerated_uses.get(use["orientedEdgeId"]) != use["edgeCurveId"]:
                raise ValueError("loop and global edge incidence disagree")
    if len(enumerated_uses) != 36:
        raise ValueError("global oriented edge uses are incomplete")
    if faces[8445]["surfaceId"] != 42 or faces[8445]["sourceSurfaceFlags"]["self_intersect"]["value"] is not True:
        raise ValueError("source self-intersection warning missing")
    if len(faces[8127]["bounds"]) != 2 or {b["boundRole"] for b in faces[8127]["bounds"]} != {"outer", "inner_or_unspecified"}:
        raise ValueError("neighbor inner bound lost")
    source_lookup = {e["edgeCurveId"]: e for e in sources}
    sample_count = 0
    for row in rows:
        source = source_lookup[row["edgeCurveId"]]
        if row["curveGeometryId"] != source["curveGeometryId"] or len(source["uses"]) != 2:
            raise ValueError("edge use identity mismatch")
        samples = row["samples"]
        n = row["canonicalSampleCount"]
        if n != len(samples) or n != 16 * (len(row["sourceKnotBreaks"]) - 1) + 1:
            raise ValueError("canonical knot-span inventory mismatch")
        if [s["canonicalSampleIndex"] for s in samples] != list(range(n)):
            raise ValueError("canonical sample identities changed")
        if [s["curveParameter"] for s in samples] != sorted(set(s["curveParameter"] for s in samples)):
            raise ValueError("canonical parameters not strictly increasing")
        breaks = row["sourceKnotBreaks"]
        expected_parameters = [a + (b - a) * (i / 16) for a, b in zip(breaks, breaks[1:]) for i in range(16)] + [breaks[-1]]
        if [s["curveParameter"] for s in samples] != expected_parameters:
            raise ValueError("canonical parameter grid does not preserve every source knot span")
        if [samples[0]["curveParameter"], samples[-1]["curveParameter"]] != row["nativeCurveDomain"]:
            raise ValueError("canonical domain endpoints mismatch")
        if row["canonicalInventoryHashEncoding"] != "f64le_parameter_xyz_per_canonical_sample_no_header":
            raise ValueError("canonical hash framing changed")
        encoded = b"".join(struct.pack("<dddd", s["curveParameter"], *s["sourceCurvePointSourceUnits"]) for s in samples)
        if hashlib.sha256(encoded).hexdigest() != row["canonicalInventorySha256"]:
            raise ValueError("canonical independent f64le inventory hash mismatch")
        for view, use in zip(row["useTraversalViews"], source["uses"], strict=True):
            forward = source["edgeSameSense"] == (use["orientation"] == use["boundOrientation"])
            indices = list(range(n)) if forward else list(reversed(range(n)))
            if (view["canonicalSampleIndices"] != indices or view["naturalParameterFollowsBoundTraversal"] != forward
                    or view["orientedEdgeId"] != use["orientedEdgeId"] or view["faceId"] != use["faceId"]):
                raise ValueError("oriented use view is not an identity-preserving canonical permutation")
        for sample in samples:
            p = sample["sourceCurvePointSourceUnits"]
            projections = sample["supportProjections"]
            if len(projections) != 2:
                raise ValueError("two incident projections required")
            for projection, use in zip(projections, source["uses"], strict=True):
                if projection["faceId"] != use["faceId"] or projection["surfaceId"] != use["surfaceId"] or projection["trimMembershipVerified"] is not False:
                    raise ValueError("projection scope mismatch")
                if projection["solver"].get("authorizesGeometryAdmission") is not False:
                    raise ValueError("projection solver must not authorize geometry")
                measured = math.dist(p, projection["supportPointSourceUnits"])
                if not math.isclose(measured, projection["curveToSupportResidualSourceUnits"], rel_tol=1e-9, abs_tol=1e-14):
                    raise ValueError("stored curve support residual inconsistent")
            gap = math.dist(*(v["supportPointSourceUnits"] for v in projections))
            if not math.isclose(gap, sample["supportToSupportGapSourceUnits"], rel_tol=1e-9, abs_tol=1e-14):
                raise ValueError("stored support gap inconsistent")
        available = row["numericalResidualThresholdAvailableCounts"]
        unavailable = row["numericalResidualThresholdUnavailableCounts"]
        failures = row["numericalResidualThresholdFailureCounts"]
        for i, use in enumerate(source["uses"]):
            expected_available = 0 if use["surfaceType"] == "PLANE" else n
            if available[i] != expected_available or unavailable[i] != n - expected_available or not 0 <= failures[i] <= available[i]:
                raise ValueError("missing plane N/A versus numerical threshold availability distinction")
        sample_count += n
    for edge_id in (3478, 9149):
        neighbors = [u for u in source_lookup[edge_id]["uses"] if u["faceId"] != 2875]
        if len(neighbors) != 1 or neighbors[0]["faceId"] != 10475:
            raise ValueError("two distinct edges lost shared neighbor")
    work = report["work"]
    for cap, expected_cap in [("canonicalCurveSampleLimit", 8192), ("curveControlPointLimit", 20_000_000),
                              ("surfaceEvaluationLimit", 4_000_000), ("surfaceControlWorkLimit", 50_000_000)]:
        if work[cap] != expected_cap:
            raise ValueError("declared work budget changed; review pinned acceptance limits")
    for actual, cap in [("canonicalCurveSamples", "canonicalCurveSampleLimit"), ("curveControlPointEvaluations", "curveControlPointLimit"),
                        ("surfaceEvaluationCalls", "surfaceEvaluationLimit"), ("budgetChargedSurfaceControlWork", "surfaceControlWorkLimit")]:
        if type(work[actual]) is not int or not 0 <= work[actual] <= work[cap]:
            raise ValueError("aggregate work cap exceeded")
    if sample_count != work["canonicalCurveSamples"] or sample_count != 100:
        raise ValueError("canonical sample work count mismatch")
    if (topology["maxWorkSteps"] != 100_000 or topology["workSteps"] > 100_000
            or not 1 <= work["surfaceCount"] <= 16 or not 0 <= work["coarseGridPointCount"] <= 8192
            or not 0 <= work["constructionAndSeamControlWorkEstimate"] <= work["budgetChargedSurfaceControlWork"]):
        raise ValueError("topology work cap exceeded")
    if (work["surfaceEvaluationCallsIncludePreparationEstimates"] is not True
            or not 0 <= work["actualExplicitNurbsEvaluationCalls"] <= work["surfaceEvaluationCalls"]
            or work["analyticPlaneProjectionCount"] != 33):
        raise ValueError("preparation accounting or separate analytic plane observations missing")
    return {"sourceEdgeCount": len(rows), "canonicalSamples": sample_count, "sourceShell":{"faces":len(faces),"bounds":loop_count,"edges":len(all_edge_rows),"orientedUses":len(enumerated_uses),"loopClosureAndEdgeIncidenceVerified":True}, "work": work,
            "sourceWarning": {"surfaceId": 42, "self_intersect": True}, "innerBoundFaceId": 8127}


def run(args):
    source = args.source.resolve(strict=True)
    binary = args.binary.resolve(strict=True)
    freecad = args.freecad.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).parents[1]
    macro = root / "references" / "audit_model16_shared_edges.FCMacro"
    inputs = [source, binary, freecad, macro, Path(__file__), Path(__file__).with_name("surface_reference_e2e.py")]
    inputs += sorted((root / "src").rglob("*.rs")) + [root / "Cargo.toml", root / "Cargo.lock"]
    hashes = {str(p): digest(p) for p in inputs}
    receipt = {"schema": "tokito.shared-edge-e2e.v1", "passed": False, "checks": [], "inputSha256": hashes,
               "geometryQualified": False, "admitted": False, "exportsProduced": []}
    try:
        if hashes[str(source)] != SOURCE_SHA256:
            raise ValueError("wrong materialized source")

        def cli(name, path=source, owner="6218", usage=False):
            command = [str(binary), "--research-shared-edges", "2875", "--source-representation", "5957", "--source-owner", owner, str(path)]
            if usage:
                command.pop()
            code, stdout, stderr = capture_bounded(command, 180, 16 * 1024 * 1024, 128 * 1024)
            (output / (name + ".json")).write_bytes(stdout)
            if code != 2 or stderr:
                raise ValueError("research CLI must return structured exit2 without stderr")
            return json.loads(stdout)

        report = cli("diagnostic")
        receipt["inventory"] = validate(report)
        receipt["diagnosticSha256"] = digest(output / "diagnostic.json")
        receipt["checks"].append({"name": "real-source-canonical-identity-views-flags-innerbounds-and-aggregate-budgets", "passed": True})
        wrong_source = output / "changed-source.step"
        wrong_source.write_bytes(bounded_read(source, 32 * 1024 * 1024) + b"\n")
        for name, bad in [("wrong-owner", cli("wrong-owner", owner="6219")),
                          ("wrong-source", cli("wrong-source", path=wrong_source)),
                          ("wrong-usage", cli("wrong-usage", usage=True))]:
            error = bad.get("error", "")
            expected = "usage:" if name == "wrong-usage" else "shared_edge_research_requires_pinned_source_face_representation_owner"
            if not error.startswith(expected) or bad.get("admitted") is not False or "edges" in bad:
                raise ValueError("expected explicit scoped CLI refusal")
            receipt["checks"].append({"name": name + "-refusal", "passed": True})
        config = {"source": str(source), "diagnostic": str(output / "diagnostic.json"), "output": str(output / "audit.json")}

        def audit(name, cfg):
            path = output / (name + ".config.json")
            path.write_text(json.dumps(cfg), encoding="utf-8")
            code, stdout, stderr = capture_bounded([str(freecad), str(macro), "--pass", str(path)], 180, 128 * 1024, 128 * 1024)
            (output / (name + ".stdout.txt")).write_bytes(stdout)
            (output / (name + ".stderr.txt")).write_bytes(stderr)
            return code, stderr.decode("utf-8", errors="replace")

        code, error = audit("independent", config)
        if code != 0 or not (output / "audit.json").is_file():
            raise ValueError("independent oracle failed: " + error[-2048:])
        independent = json.loads(bounded_read(output / "audit.json", 128 * 1024))
        if (independent.get("diagnosticSha256") != receipt["diagnosticSha256"] or independent.get("canonicalSamplesAudited") != 100
                or independent.get("maximumIndependentCurvePointGapMm", math.inf) > 1e-9
                or independent.get("maximumIndependentSupportPointGapMm", math.inf) > 1e-9
                or independent.get("admitted") is not False):
            raise ValueError("independent source or support evidence incomplete")
        receipt["independentAudit"] = independent
        receipt["checks"].append({"name": "independent-all-four-curve-binding-shared-native-adjacency-and-untrimmed-support-audit", "passed": True})
        original_audit_hash = digest(output / "audit.json")
        _, error = audit("no-overwrite", config)
        if "refusing to overwrite output" not in error or digest(output / "audit.json") != original_audit_hash:
            raise ValueError("oracle no-overwrite refusal failed")
        receipt["checks"].append({"name": "oracle-no-overwrite-refusal", "passed": True})
        for name, expected in [("source-pin", "pinned source mismatch"), ("support-point", "independent support point or residual mismatch"),
                               ("support-gap", "independent support gap mismatch")]:
            corrupt = json.loads(json.dumps(report))
            if name == "source-pin":
                corrupt["sourceSha256"] = "0" * 64
            elif name == "support-point":
                corrupt["edges"][0]["samples"][1]["supportProjections"][0]["supportPointSourceUnits"][0] += 0.01
            else:
                corrupt["edges"][0]["samples"][1]["supportToSupportGapSourceUnits"] += 0.01
            path = output / (name + "-corrupt.json")
            path.write_text(json.dumps(corrupt), encoding="utf-8")
            forbidden = output / (name + "-must-not-exist.json")
            _, error = audit(name, {**config, "diagnostic": str(path), "output": str(forbidden)})
            if expected not in error or forbidden.exists():
                raise ValueError("oracle " + name + " mutation was not rejected")
            receipt["checks"].append({"name": "oracle-" + name + "-refusal", "passed": True})
        if any(digest(Path(p)) != h for p, h in hashes.items()):
            raise ValueError("input changed during E2E")
        receipt["passed"] = True
    except Exception as error:
        receipt["error"] = str(error)
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2, allow_nan=False), encoding="utf-8")
    print(json.dumps({"passed": receipt["passed"], "checks": receipt["checks"], "error": receipt.get("error"),
                      "independent": receipt.get("independentAudit")}))
    return 0 if receipt["passed"] else 2


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "freecad", "source", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    raise SystemExit(run(parser.parse_args()))
