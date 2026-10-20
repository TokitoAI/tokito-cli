"""Real source-bound canonical two-face patch and independent FreeCAD comparison."""
import argparse
import collections
import hashlib
import json
import math
from pathlib import Path

from surface_reference_e2e import bounded_read, capture_bounded

SOURCE_SHA256 = "7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f"
EXPECTED_EDGES = {3400: 791, 3478: 10828, 5409: 607, 9149: 2687, 7239: 9005, 4818: 4186}


def digest(path):
    return hashlib.sha256(bounded_read(path,128*1024*1024)).hexdigest()


def validate(r):
    if r.get("sourceSha256")!=SOURCE_SHA256 or r.get("status")!="sampled_targets_met_unqualified":
        raise ValueError("source pin or sampled patch status mismatch")
    for key in ("geometryQualified","admitted","completeSolidVerified","completeTrimCoverageVerified","certifiedDeviationBound","selfIntersectionVerified","sourceCurveSupportGapResolved"):
        if r.get(key) is not False:raise ValueError("patch qualification scope missing")
    if r.get("exportsProduced")!=[]:raise ValueError("patch must not export geometry")
    p=r["policy"]
    if (p["sourceUnchanged"] is not True or p["sourceUncertaintyUsedAsAllowance"] is not False
            or p["productionAcceptancePolicyApproved"] is not False
            or p["chordToleranceSourceUnits"]!=0.00001 or p["boundaryDisplacementAllowanceSourceUnits"]!=0.000005):
        raise ValueError("experimental displacement policy mismatch")
    vertices=r["globalVertices"]
    if len(vertices)>50000 or [v["id"] for v in vertices]!=list(range(len(vertices))):raise ValueError("global identity inventory invalid")
    if any(len(v["pointSourceUnits"])!=3 or any(not math.isfinite(x) for x in v["pointSourceUnits"]) for v in vertices):
        raise ValueError("nonfinite global coordinates")
    if {e["edgeCurveId"]:e["curveGeometryId"] for e in r["edges"]}!=EXPECTED_EDGES or len(r["edges"])!=6:
        raise ValueError("source edge inventory mismatch")
    edge_rows={e["edgeCurveId"]:e for e in r["edges"]}
    incidence=collections.defaultdict(list)
    boundary_ids=set()
    for row in r["edges"]:
        samples=row["samples"]
        if row["clampedNativeEndpointsVerified"] is not True or row["naturalEndpointResidualsSourceUnits"]!=[0.,0.]:
            raise ValueError("full curve endpoint correspondence missing")
        if len(samples)!=len(row["leafIntervals"])+1:raise ValueError("leaf inventory mismatch")
        parameters=[s["curveParameter"] for s in samples]
        if parameters!=sorted(set(parameters)) or [parameters[0],parameters[-1]]!=row["nativeCurveDomain"]:
            raise ValueError("source parameter interval invalid")
        if not all(k in parameters for k in row["sourceKnotBreaks"]):raise ValueError("source knot span lost")
        boundary_ids.update(s["globalVertexId"] for s in samples)
        for leaf,a,b in zip(row["leafIntervals"],samples,samples[1:]):
            if [leaf["startParameter"],leaf["endParameter"]]!=[a["curveParameter"],b["curveParameter"]] or not 0<=leaf["maximumQuarterChordErrorSourceUnits"]<=row["sampledChordTargetSourceUnits"]:
                raise ValueError("leaf interval target failed")
    if len(boundary_ids)!=r["canonicalBoundaryVertexCount"]:raise ValueError("canonical boundary inventory mismatch")
    if {f["faceId"] for f in r["faces"]}!={2875,10475} or len(r["faces"])!=2:raise ValueError("pair face inventory mismatch")
    total=0;used=set()
    for f in r["faces"]:
        if (f["geometryQualified"] is not False or f["trimMembershipVerified"] is not False
                or f["canonicalBoundaryUsedBeforeQualityEvaluation"] is not True or f["originalConstraintSegmentsPreserved"] is not True):
            raise ValueError("canonical mesh evaluation contract missing")
        ids=f["globalVertexIds"]
        if len(ids)!=f["vertexCount"] or len(ids)!=len(f["uvVertices"]) or len(set(ids))!=len(ids) or len(ids)>25000:
            raise ValueError("local face identity mapping invalid")
        for eid in (3478,9149):
            view=next(v for v in f["edgeViews"] if v["edgeCurveId"]==eid)
            natural=[s["globalVertexId"] for s in edge_rows[eid]["samples"]]
            reverse=(f["faceId"],eid) in ((2875,9149),(10475,3478))
            if view["globalVertexIds"]!=(list(reversed(natural)) if reverse else natural):
                raise ValueError("shared source seam identity view mismatch")
        q=f["quality"]
        if max(q["maximumSampledFixedBoundaryChordErrorSourceUnits"],q["maximumSampledInteriorChordErrorSourceUnits"])>p["chordToleranceSourceUnits"] or q["normalThresholdFailureCount"] or q["nonpositiveNormalCount"]:
            raise ValueError("actual canonical mesh sampled target failed")
        if f["maximumBoundaryDisplacementSourceUnits"]>p["boundaryDisplacementAllowanceSourceUnits"]:
            raise ValueError("explicit boundary allowance exceeded")
        if len(f["triangles"])!=f["triangleCount"] or len(f["triangles"])>50000:raise ValueError("triangle budget exceeded")
        local=set(ids)
        for t in f["triangles"]:
            if len(t)!=3 or len(set(t))!=3 or any(v not in local for v in t):raise ValueError("invalid global triangle map")
            used.update(t);total+=1
            for a,b in ((t[0],t[1]),(t[1],t[2]),(t[2],t[0])):incidence[tuple(sorted((a,b)))].append((f["faceId"],a<b))
    if used!=set(range(len(vertices))) or total>100000:raise ValueError("unused global point or triangle limit")
    if any(len(u)>2 or (len(u)==2 and u[0][1]==u[1][1]) for u in incidence.values()):raise ValueError("nonmanifold or inconsistent incidence")
    shared=0
    for eid in (3478,9149):
        for a,b in zip(edge_rows[eid]["samples"],edge_rows[eid]["samples"][1:]):
            u=incidence.get(tuple(sorted((a["globalVertexId"],b["globalVertexId"]))),[])
            if len(u)!=2 or u[0][0]==u[1][0] or u[0][1]==u[1][1]:raise ValueError("shared edge incidence mismatch")
            shared+=1
    topology=r["topology"]
    expected={"vertices":len(vertices),"edges":len(incidence),"triangles":total,"eulerCharacteristic":len(vertices)-len(incidence)+total}
    if any(topology[k]!=v for k,v in expected.items()) or expected["eulerCharacteristic"]!=0 or topology["openBoundaryCycles"]!=2 or shared!=64:
        raise ValueError("open annulus topology mismatch")
    w=r["work"]
    if (w["aggregateAcrossBothFaces"] is not True or w["surfaceCallLimit"]!=4_000_000
            or not 0<w["surfaceCallUnits"]<=4_000_000 or w["surfaceControlWorkLimit"]!=50_000_000
            or not 0<w["surfaceControlWork"]<=50_000_000 or w["curveProbeLimit"]!=32768
            or not 0<w["curveProbeCount"]<=32768 or not 0<w["curveControlWork"]<=20_000_000
            or not 0<=w["retainedCurveControlPoints"]<=w["curveControlStorageLimit"]):
        raise ValueError("aggregate patch resource budget invalid")
    return {**expected,"sharedOppositeSegments":shared,"canonicalBoundaryVertices":len(boundary_ids),"work":w}


def run(args):
    source=args.source.resolve(strict=True);binary=args.binary.resolve(strict=True);freecad=args.freecad.resolve(strict=True)
    output=args.output.resolve();output.mkdir(parents=True,exist_ok=False)
    root=Path(__file__).parents[1];macro=root/"references"/"audit_model16_shared_patch.FCMacro"
    inputs=[source,binary,freecad,macro,Path(__file__),Path(__file__).with_name("surface_reference_e2e.py")]
    inputs+=sorted((root/"src").rglob("*.rs"))+[root/"Cargo.toml",root/"Cargo.lock"]
    hashes={str(p):digest(p) for p in inputs}
    receipt={"schema":"tokito.shared-patch-e2e.v1","passed":False,"checks":[],"inputSha256":hashes,"geometryQualified":False,"admitted":False,"exportsProduced":[]}
    try:
        if hashes[str(source)]!=SOURCE_SHA256:raise ValueError("wrong source fixture")
        def cli(name,path=source,owner="6218",allowance="0.000005",usage=False):
            command=[str(binary),"--research-shared-patch","--chord-tolerance","0.00001","--boundary-allowance",allowance,"--source-representation","5957","--source-owner",owner,str(path)]
            if usage:command.pop()
            code,stdout,stderr=capture_bounded(command,360,32*1024*1024,128*1024)
            (output/(name+".json")).write_bytes(stdout)
            if code!=2 or stderr:raise ValueError("expected structured unqualified exit2 without stderr")
            return json.loads(stdout)
        report=cli("diagnostic");receipt["mesh"]=validate(report);receipt["diagnosticSha256"]=digest(output/"diagnostic.json")
        receipt["checks"].append({"name":"actual-source-canonical-boundary-pair-mesh-and-opposite-shared-incidence","passed":True})
        changed=output/"changed-source.step";changed.write_bytes(bounded_read(source,32*1024*1024)+b"\n")
        for name,bad,expected in [
            ("wrong-owner",cli("wrong-owner",owner="6219"),"shared_patch_requires_pinned_source_representation_owner"),
            ("wrong-source",cli("wrong-source",path=changed),"shared_patch_requires_pinned_source_representation_owner"),
            ("wrong-usage",cli("wrong-usage",usage=True),"usage:"),
            ("excess-allowance",cli("excess-allowance",allowance="0.000006"),"patch_boundary_allowance_exceeds_half_chord_target"),
            ("invalid-allowance",cli("invalid-allowance",allowance="nan"),"invalid_shared_patch_tolerance"),
            ("insufficient-allowance",cli("insufficient-allowance",allowance="0.0000001"),"patch_boundary_displacement_exceeds_explicit_allowance"),
        ]:
            if not bad.get("error","").startswith(expected) or bad.get("admitted") is not False or "globalVertices" in bad:
                raise ValueError("clean "+name+" refusal missing")
            receipt["checks"].append({"name":name+"-refusal","passed":True})
        config={"source":str(source),"diagnostic":str(output/"diagnostic.json"),"output":str(output/"audit.json")}
        def audit(name,cfg):
            path=output/(name+".config.json");path.write_text(json.dumps(cfg),encoding="utf-8")
            code,stdout,stderr=capture_bounded([str(freecad),str(macro),"--pass",str(path)],360,128*1024,128*1024)
            (output/(name+".stdout.txt")).write_bytes(stdout);(output/(name+".stderr.txt")).write_bytes(stderr)
            return code,stderr.decode("utf-8",errors="replace")
        code,error=audit("independent",config)
        if code!=0 or not (output/"audit.json").is_file():raise ValueError("independent native audit failed: "+error[-3000:])
        independent=json.loads(bounded_read(output/"audit.json",128*1024))
        if (independent["diagnosticSha256"]!=receipt["diagnosticSha256"] or independent["vertices"]!=receipt["mesh"]["vertices"]
                or independent["triangles"]!=receipt["mesh"]["triangles"] or independent["geometryQualified"] is not False
                or independent["sourceCurveCount"]!=6 or independent["sharedOppositeSegments"]!=64
                or {e["edgeCurveId"] for e in independent["nativeTopologicalEdgeIntervals"]}!=set(EXPECTED_EDGES)
                or sum(f["trimCentroidsSampled"] for f in independent["faces"])!=512):
            raise ValueError("independent native audit inventory mismatch")
        receipt["independentAudit"]=independent
        receipt["checks"].append({"name":"independent-six-curves-quarter-chords-all-vertices-triangles-normals-and-sampled-trims","passed":True})
        h=digest(output/"audit.json");_,error=audit("no-overwrite",config)
        if "refusing to overwrite output" not in error or digest(output/"audit.json")!=h:raise ValueError("oracle no-overwrite failure")
        receipt["checks"].append({"name":"oracle-no-overwrite-refusal","passed":True})
        for name,expected in [("source-pin","pinned source mismatch"),("point","independent interior surface point or UV mapping mismatch"),
                              ("seam-ids","canonical shared seam IDs or ordered edge views changed"),("uv-mapping","independent interior surface point or UV mapping mismatch"),
                              ("native-edge-domain","independent native topological edge interval mismatch")]:
            corrupt=json.loads(json.dumps(report))
            if name=="source-pin":corrupt["sourceSha256"]="0"*64
            elif name=="point":
                target=next(v for v in corrupt["globalVertices"] if "sourceFaceId" in v["origin"]);target["pointSourceUnits"][0]+=0.01
            elif name=="seam-ids":
                view=next(v for v in corrupt["faces"][0]["edgeViews"] if v["edgeCurveId"]==3478);view["globalVertexIds"][1]=view["globalVertexIds"][2]
            elif name=="native-edge-domain":
                corrupt["edges"][0]["nativeCurveDomain"][0]+=0.01
            else:
                face=corrupt["faces"][0];boundary=set(face["boundaryGlobalVertexIds"])
                local=next(i for i,gid in enumerate(face["globalVertexIds"]) if gid not in boundary);face["uvVertices"][local][1]+=0.01
            path=output/(name+"-corrupt.json");path.write_text(json.dumps(corrupt),encoding="utf-8")
            forbidden=output/(name+"-must-not-exist.json")
            _,error=audit(name,{**config,"diagnostic":str(path),"output":str(forbidden)})
            if expected not in error or forbidden.exists():raise ValueError("oracle "+name+" mutation refusal missing: "+error[-1500:])
            receipt["checks"].append({"name":"oracle-"+name+"-refusal","passed":True})
        if any(digest(Path(p))!=h for p,h in hashes.items()):raise ValueError("input changed during E2E")
        receipt["passed"]=True
    except Exception as error:receipt["error"]=str(error)
    (output/"receipt.json").write_text(json.dumps(receipt,indent=2,allow_nan=False),encoding="utf-8")
    print(json.dumps({"passed":receipt["passed"],"error":receipt.get("error"),"checks":receipt["checks"],"independent":receipt.get("independentAudit")}))
    return 0 if receipt["passed"] else 2


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ("binary","freecad","source","output"):parser.add_argument("--"+name,type=Path,required=True)
    raise SystemExit(run(parser.parse_args()))
