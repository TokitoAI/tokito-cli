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


def validate_mesh(report, metric=False, retriangulate=False, adaptive=False):
    p = report["physicalRefinementCandidate"]
    if p.get("admitted") is not False or p.get("geometryQualified") is not False or p.get("exportsProduced") != []:
        raise ValueError("research mesh must stay unqualified")
    expected_termination="interior_retriangulation_incomplete" if retriangulate else "mesh_budget_exhausted"
    if adaptive:
        q=p["quality"]
        met=(q["maximumSampledInteriorChordErrorSourceUnits"]<=p["chordToleranceSourceUnits"]
             and q["maximumSampledFixedBoundaryChordErrorSourceUnits"]<=p["chordToleranceSourceUnits"]
             and q["normalThresholdFailureCount"]==0)
        if (p.get("status")!=("sampled_targets_met_unqualified" if met else "incomplete")
            or p.get("termination")!=("interior_retriangulation_sampled_targets_met" if met else "interior_retriangulation_incomplete")):
            raise ValueError("adaptive status contradicts measured quality")
    elif p.get("status") != "incomplete" or p.get("termination") != expected_termination:
        raise ValueError("pinned incomplete outcome changed; re-evaluate fixture evidence")
    if retriangulate:
        flips=p.get("interiorRetriangulation",{})
        accepted=flips.get("acceptedFlips",[])
        if ((not accepted and not adaptive) or len(accepted)!=flips.get("acceptedFlipCount")
            or flips.get("verticesMoved")!=0 or flips.get("boundaryEdgesChanged")!=0
            or flips.get("attempts",129)>128 or flips.get("passes",4)>3
            or flips.get("sampleEvaluationCount",400001)>400000
            or p.get("interiorRetriangulationControlPointEvaluationCount",50000001)>50000000
            or (not adaptive and p.get("beforeInteriorRetriangulationTermination")!="mesh_budget_exhausted")
            or (not adaptive and p["quality"]["nonpositiveNormalCount"]>=p["beforeInteriorRetriangulationQuality"]["nonpositiveNormalCount"])):
            raise ValueError("interior flip preservation/budget/improvement evidence invalid")
        for flip in accepted:
            if (flip["newMinimumNormalAlignment"]<=0
                or flip["newMinimumNormalAlignment"]<=flip["oldMinimumNormalAlignment"]
                or flip["newMaximumSampledPairChordErrorSourceUnits"]>flip["oldMaximumSampledPairChordErrorSourceUnits"]):
                raise ValueError("accepted flip did not meet pair quality requirements")
    chart=p.get("initialChartMetric")
    if metric:
        if not isinstance(chart,dict) or chart.get("originalUvCoordinatesRetainedByIdentity") is not True:
            raise ValueError("metric chart source identity evidence missing")
        matrix=chart["normalizedCholeskyMatrix"]
        if not 0.02<matrix[0][0]<0.04 or matrix[1][0]!=0 or not 0.99<matrix[1][1]<=1.000001:
            raise ValueError("pinned metric scale changed")
        if not adaptive and not 0<p["quality"]["nonpositiveNormalCount"]<=10:
            raise ValueError("metric remaining orientation count changed")
    elif chart is not None:
        raise ValueError("baseline unexpectedly uses a metric chart")
    uv = p["uvVertices"]
    points = p["surfacePointsSourceUnits"]
    triangles = p["triangles"]
    count = p["boundaryVertexCount"]
    if len(uv) != len(points) or len(uv) != p["vertexCount"] or len(triangles) != p["triangleCount"]:
        raise ValueError("mesh inventory mismatch")
    expanded=report["adaptiveBoundaryCandidate"]["expandedInputSampleCount"] if adaptive else 100
    if count != expanded-4 or p.get("originalInputSampleCount") != expanded or p.get("removedExactDuplicateCount") != 4 or p.get("additionalBoundaryVertexCount") != 0:
        raise ValueError("source boundary inventory changed")
    mapping = p["inputSampleVertexIndices"]
    source = report["adaptiveBoundaryCandidate"]["samples"] if adaptive else report["trimCandidate"]["boundarySamples"]
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
    if not adaptive and (quality["maximumSampledFixedBoundaryChordErrorSourceUnits"] <= p["chordToleranceSourceUnits"] or quality["nonpositiveNormalCount"] <= 0):
        raise ValueError("pinned remaining quality blockers changed")
    return {"vertices": len(uv), "triangles": len(triangles), "edges": len(edges), "eulerCharacteristic": 1,
            "fixedBoundaryEdges": len(boundary), "nonconformingEdges": 0,
            "firstQuality": p["history"][0]["quality"], "lastQuality": quality,
            "interiorRetriangulation":p.get("interiorRetriangulation")}


def validate_adaptive_boundary(report):
    a=report["adaptiveBoundaryCandidate"]
    p=report["physicalRefinementCandidate"]
    originals=report["trimCandidate"]["boundarySamples"]
    samples=a["samples"]
    mapping=a["originalSampleExpandedInputIndices"]
    if (a.get("geometryQualified") is not False or a.get("admitted") is not False or a.get("exportsProduced")!=[]
        or a.get("sharedAcrossFaces") is not False or a.get("certifiedCurveDeviationBound") is not False
        or a.get("projectionAdmissionToleranceChanged") is not False or a.get("sourceCurveToSupportSurfaceGapResolved") is not False
        or a["sourceEdges"]!=report["edges"] or len(mapping)!=len(originals)
        or a["expandedInputSampleCount"]!=len(samples) or not len(originals)<len(samples)<=4096
        or len(samples)!=len(originals)+a["addedSampleCount"]
        or a["curveProjectionEvaluationCount"]!=len(a["observations"])*3
        or a["curveProjectionEvaluationCount"]>16384 or a["maximumDepth"]>16
        or a["controlPointEvaluationCount"]>50000000 or a["curveControlPointEvaluationCount"]>20000000):
        raise ValueError("adaptive scope, inventory or budget mismatch")
    if any(type(i) is not int or not 0<=i<len(samples) for i in mapping) or mapping!=sorted(set(mapping)):
        raise ValueError("adaptive original sample mapping invalid")
    for i,k in enumerate(mapping):
        original=originals[i]; retained=samples[k]
        for key in ["loopIndex","curveParameter","candidateUv","sourcePointSourceUnits","surfacePointSourceUnits","projectionConverged","projectionSolver"]:
            if original[key]!=retained[key]:raise ValueError("original boundary observation changed")
        if retained["originalInputIndex"]!=i or p["sourceOriginalSampleVertexIndices"][i]!=p["inputSampleVertexIndices"][k]:
            raise ValueError("original to expanded to mesh mapping invalid")
    if [i for i,s in enumerate(samples) if s["originalInputIndex"] is not None]!=mapping:
        raise ValueError("extra original identity claimed")
    roots=[(samples[mapping[i]],samples[mapping[i+1]],0) for i in range(len(originals)-1)
           if originals[i]["loopIndex"]==originals[i+1]["loopIndex"]]
    stack=list(reversed(roots)); leaves=[]
    for observation in a["observations"]:
        if not stack:raise ValueError("extra adaptive observation")
        start,end,depth=stack.pop()
        if observation["start"]!=start or observation["end"]!=end or observation["depth"]!=depth:
            raise ValueError("adaptive recursion replay mismatch")
        probes=observation["probes"]
        if len(probes)!=3:raise ValueError("adaptive probe fractions missing")
        for probe,fraction in zip(probes,[0.25,0.5,0.75]):
            s=probe["sample"]
            solver=s.get("projectionSolver",{})
            if (not isinstance(solver.get("termination"),str)
                or solver.get("residualToleranceMet") is not s.get("projectionConverged")
                or not isinstance(solver.get("stationarityObservation"),str)
                or solver.get("authorizesGeometryAdmission") is not False):
                raise ValueError("adaptive projection solver observations missing")
            t=start["curveParameter"]*(1-fraction)+end["curveParameter"]*fraction
            if (probe["fraction"]!=fraction or s["loopIndex"]!=start["loopIndex"] or s["curveParameter"]!=t
                or s["originalInputIndex"] is not None):raise ValueError("adaptive probe identity mismatch")
        errors=[observation[k] for k in ["maximumSampledSourceCurveChordErrorSourceUnits","maximumSampledSupportChordErrorSourceUnits"]]
        if any(not math.isfinite(v) or v<0 for v in errors):raise ValueError("invalid adaptive chord observation")
        if observation["split"]!=(max(errors)>a["toleranceSourceUnits"]):raise ValueError("adaptive split decision mismatch")
        if observation["split"]:
            middle=probes[1]["sample"]
            stack.append((middle,end,depth+1));stack.append((start,middle,depth+1))
        else:leaves.append((start,end,depth,errors))
    if stack or len(leaves)!=len(a["intervals"]):raise ValueError("missing adaptive recursion or leaves")
    expected_pairs=[(i,i+1) for i in range(len(samples)-1) if samples[i]["loopIndex"]==samples[i+1]["loopIndex"]]
    if [(v["startInputIndex"],v["endInputIndex"]) for v in a["intervals"]]!=expected_pairs:
        raise ValueError("expanded boundary interval coverage mismatch")
    for interval,leaf in zip(a["intervals"],leaves):
        start,end,depth,errors=leaf
        if (samples[interval["startInputIndex"]]!=start or samples[interval["endInputIndex"]]!=end
            or interval["depth"]!=depth or errors!=[interval["maximumSampledSourceCurveChordErrorSourceUnits"],interval["maximumSampledSupportChordErrorSourceUnits"]]):
            raise ValueError("adaptive leaf sample or measurement changed")
    if p["quality"]["maximumSampledFixedBoundaryChordErrorSourceUnits"]>a["toleranceSourceUnits"]:
        raise ValueError("expanded support boundary misses sampled chord target")


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

        def cli(name, tolerance="0.00001", face="2875", mode=None):
            mode=mode or ("--research-adaptive-boundary-source-face" if args.adaptive_boundary else ("--research-retriangulated-source-face" if args.retriangulate else ("--research-metric-source-face" if args.metric else "--research-physical-source-face")))
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
        receipt["metricPreconditioning"]=args.metric or args.retriangulate or args.adaptive_boundary
        receipt["interiorRetriangulation"]=args.retriangulate or args.adaptive_boundary
        receipt["adaptiveSourceBoundary"]=args.adaptive_boundary
        receipt["mesh"] = validate_mesh(report,args.metric or args.retriangulate or args.adaptive_boundary,args.retriangulate or args.adaptive_boundary,args.adaptive_boundary)
        if args.adaptive_boundary:
            validate_adaptive_boundary(report)
            baseline=cli("metric-baseline",mode="--research-metric-source-face")["physicalRefinementCandidate"]
            if report["physicalRefinementCandidate"]["quality"]["maximumSampledFixedBoundaryChordErrorSourceUnits"]>=baseline["quality"]["maximumSampledFixedBoundaryChordErrorSourceUnits"]:
                raise ValueError("expanded boundary did not improve measured support chord error")
            receipt["baselineQuality"]=baseline["quality"]
            receipt["baselineDiagnosticSha256"]=digest(output/"metric-baseline.json",32*1024*1024)
            receipt["checks"].append({"name":"adaptive-native-curve-inventory-source-mapping-and-chord-targets","passed":True})
        if args.retriangulate:
            baseline=cli("metric-baseline",mode="--research-metric-source-face")["physicalRefinementCandidate"]
            final=report["physicalRefinementCandidate"]
            for key in ["uvVertices","surfacePointsSourceUnits","inputSampleVertexIndices","boundaryVertexCount","originalInputSampleCount"]:
                if baseline[key]!=final[key]:
                    raise ValueError("interior flips changed source/mesh vertex identity")
            replay=[list(t) for t in baseline["triangles"]]
            for flip in final["interiorRetriangulation"]["acceptedFlips"]:
                indices=flip["triangleIndices"]
                if len(indices)!=2 or any(replay[i]!=t for i,t in zip(indices,flip["oldTriangles"])):
                    raise ValueError("flip log does not replay against actual baseline")
                for i,t in zip(indices,flip["newTriangles"]):replay[i]=t
            if replay!=final["triangles"]:
                raise ValueError("replayed flips differ from final mesh")
            receipt["baselineDiagnosticSha256"]=digest(output/"metric-baseline.json",32*1024*1024)
            receipt["checks"].append({"name":"actual-metric-baseline-identity-and-flip-replay","passed":True})
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
        if (not args.adaptive_boundary and independent.get("analyticNonpositiveNormalCount", 0) <= 0) or independent.get("admitted") is not False:
            raise ValueError("independent remaining orientation blocker missing")
        if (args.retriangulate or args.adaptive_boundary) and len(independent.get("independentAcceptedFlipAudits",[]))!=report["physicalRefinementCandidate"]["interiorRetriangulation"]["acceptedFlipCount"]:
            raise ValueError("independent accepted-flip audit incomplete")
        quality = report["physicalRefinementCandidate"]["quality"]
        if (independent["analyticNonpositiveNormalCount"] != quality["nonpositiveNormalCount"]
                or not math.isclose(independent["minimumAnalyticNormalAlignment"], quality["minimumSampledNormalAlignment"], rel_tol=1e-6, abs_tol=1e-8)):
            raise ValueError("finite-difference and independent analytic orientation observations disagree")
        receipt["independentAudit"] = independent
        receipt["checks"].append({"name": "independent-surface-chord-normal-audit", "passed": True})
        if args.adaptive_boundary:
            boundary=independent.get("independentAdaptiveBoundaryAudit",{})
            if (boundary.get("retainedSamplesAudited")!=report["adaptiveBoundaryCandidate"]["expandedInputSampleCount"]
                or boundary.get("quarterProbesAudited")!=report["adaptiveBoundaryCandidate"]["curveProjectionEvaluationCount"]):
                raise ValueError("independent boundary audit incomplete")
            corrupted=json.loads(json.dumps(report))
            corrupted["adaptiveBoundaryCandidate"]["samples"][1]["sourcePointSourceUnits"][0]+=0.01
            path=output/"wrong-boundary.json"
            path.write_text(json.dumps(corrupted),encoding="utf-8")
            _,error=audit("wrong-boundary",{**config,"diagnostic":str(path),"output":str(output/"wrong-boundary-must-not-exist.json")})
            if "independent adaptive sample mismatch" not in error or (output/"wrong-boundary-must-not-exist.json").exists():
                raise ValueError("independent adaptive source tamper refusal failed")
            receipt["checks"].append({"name":"independent-adaptive-source-tamper-refusal","passed":True})
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
    modes=parser.add_mutually_exclusive_group()
    modes.add_argument("--metric", action="store_true", help="Exercise the isolated single-anchor metric-preconditioned mode")
    modes.add_argument("--retriangulate", action="store_true", help="Exercise bounded interior flips after metric refinement")
    modes.add_argument("--adaptive-boundary", action="store_true", help="Exercise source-parameter adaptive boundary sampling before metric refinement")
    raise SystemExit(run(parser.parse_args()))
