"""Pinned ESP8266 model-14 repair versus a digest-pinned independent reference.

Uses existing strict mesh and bounded bidirectional surface comparison helpers.
Does not admit STEP or establish component/PCB identity. Keeps failed receipts.
"""
import argparse
import math
import json
from pathlib import Path

import multi_surface_reference_e2e as multi
import surface_reference_e2e as surface
from bounded_fingerprint import fingerprint

SOURCE_SHA = "78db318947fa4d67e3d1f5ad9e7fbe3c44c9b0ffca621a8b8a0bb6dd93282b2b"


def native_compare(binary, a, b, path):
    payload=json.dumps({"a":a,"b":b},separators=(",",":"),allow_nan=False).encode()
    if len(payload)>96*1024*1024:
        raise ValueError("native comparison transfer limit")
    with path.open("xb") as stream:
        stream.write(payload)
    code,stdout,stderr=surface.capture_bounded([str(binary),str(path)],timeout_seconds=300,stdout_limit=1024*1024,stderr_limit=512*1024)
    if code!=0 or stderr:
        raise ValueError(f"native distance refused: {stdout[:200]!r}")
    result=json.loads(stdout)
    if result.get("schema")!="tokito.independent-sampled-distance.v1" or result.get("admitted") is not False:
        raise ValueError("native comparison contract")
    for name,mesh in [("forward",a),("reverse",b)]:
        row=result[name]
        if row["sampleCount"]!=min(len(mesh["triangles"]),75_000)*4 or any(not math.isfinite(row[key]) or row[key]<0 for key in ["maximum","mean"]):
            raise ValueError("native sampling/measurement contract")
    if not 0<result["triangleDistanceEvaluations"]<=multi.MAX_TRIANGLE_DISTANCE_EVALUATIONS:
        raise ValueError("native work budget contract")
    return result


def run(binary, source, reference, reference_sha, output, distance_binary=None):
    if output.exists():
        raise ValueError("output must be new")
    output.mkdir(parents=True)
    receipt = {"schema":"tokito.step-planar-repair-reference.v1", "passed":False,
               "geometryQualified":False,"admitted":False,"componentAssociationVerified":False,
               "pcbPlacementVerified":False,"scope":"pinned model-14 experimental repair and sampled reference comparison"}
    tracked = [(binary,64*1024*1024),(source,32*1024*1024),
        (reference,multi.MAX_REFERENCE_JSON_BYTES),(Path(__file__),1024*1024),
        (Path(multi.__file__),1024*1024),(Path(surface.__file__),1024*1024),
        (Path(__file__).with_name("bounded_fingerprint.py"),1024*1024)]
    try:
        if distance_binary is not None:
            tracked.extend([(distance_binary,64*1024*1024),(Path(__file__).parents[1]/"examples/surface_distance.rs",1024*1024)])
        hashes = {str(p):fingerprint(p,limit) for p,limit in tracked}
        receipt["inputHashes"] = hashes
        if hashes[str(source)] != SOURCE_SHA or hashes[str(reference)] != reference_sha:
            raise ValueError("source or pinned reference digest mismatch")
        ref = json.loads(surface.bounded_read(reference,multi.MAX_REFERENCE_JSON_BYTES))
        if ref.get("inputSha256") != SOURCE_SHA or ref.get("modelFile") != "14.step":
            raise ValueError("reference source identity mismatch")
        points,triangles,evidence = multi.checked_solid_reference(ref,41)
        receipt["referenceEvidence"] = evidence
        if not evidence["allReferenceSolidsClosedAndConsistentlyOriented"]:
            raise ValueError("reference solids failed integrity")
        outputs = []
        for name,flags in [("baseline",["--candidate-mesh-data"]),
                           ("repaired",["--repair-planar-face","51071","--source-tolerance","0.000001","--source-representation","58073"])]:
            code,stdout,stderr=surface.capture_bounded([str(binary),*flags,str(source)],timeout_seconds=300,
                stdout_limit=multi.MAX_CAPTURE_BYTES,stderr_limit=multi.MAX_STDERR_BYTES)
            (output/f"{name}.json").write_bytes(stdout)
            if code!=2 or stderr:
                raise ValueError(f"{name} diagnostic process contract changed")
            report=json.loads(stdout)
            if report.get("sourceSha256")!=SOURCE_SHA or report.get("admitted") is not False or report.get("exportsProduced")!=[]:
                raise ValueError(f"{name} source/non-admission changed: {report.get('error')}")
            if report.get("warningsTotal")!=0:
                raise ValueError(f"{name} reader warnings require review")
            mesh=report.get("candidateMeshData",{})
            world=report.get("assemblyOccurrenceWorld",{})
            if mesh.get("lengthUnit")!="mm" or world.get("independentRepresentationUnitsResolved") is not True or world.get("readerScaleAgreesWithIndependentUnits") is not True:
                raise ValueError(f"{name} independently checked unit/frame conversion missing")
            outputs.append(report)
        baseline,repaired=outputs
        old=baseline["candidateMeshData"]["occurrences"]
        new=repaired["candidateMeshData"]["occurrences"]
        if len(old)!=len(new):
            raise ValueError("occurrence inventory changed")
        changed=0
        for a,b in zip(old,new):
            if {k:v for k,v in a.items() if k!="shapes"}!={k:v for k,v in b.items() if k!="shapes"} or len(a["shapes"])!=len(b["shapes"]):
                raise ValueError("occurrence identity changed")
            for x,y in zip(a["shapes"],b["shapes"]):
                if x==y:
                    continue
                changed+=1
                if x["sourceItemId"]!=46427 or y["sourceItemId"]!=46427 or x["verticesMm"]!=y["verticesMm"]:
                    raise ValueError("unrelated geometry or positions changed")
                keep=lambda shape:[(f,t) for f,t in zip(shape["sourceTriangleFaceIds"],shape["triangles"]) if f!=51071]
                if keep(x)!=keep(y):
                    raise ValueError("non-target face triangles changed")
                allowed={"triangles","sourceTriangleFaceIds"}
                if {k:v for k,v in x.items() if k not in allowed}!={k:v for k,v in y.items() if k not in allowed}:
                    raise ValueError("target metadata changed")
        if changed!=1:
            raise ValueError("expected exactly one changed shape occurrence")
        receipt["unrelatedOccurrencesAndGeometryUnchanged"]=True
        mesh=repaired["candidateMeshData"]
        candidate_points,candidate_triangles,occurrences,shapes=multi.combine_occurrence_mesh(mesh)
        integrities=[]
        for occurrence in mesh["occurrences"]:
            for shape in occurrence["shapes"]:
                integrity=surface.mesh_integrity(shape["verticesMm"],shape["triangles"])
                if not integrity["closed"] or not integrity["consistentlyOriented"] or integrity["signedVolumeMm3"]<=0:
                    raise ValueError("a repaired candidate shape failed integrity")
                integrities.append(integrity)
        integrity=surface.mesh_integrity(candidate_points,candidate_triangles)
        receipt["candidateIntegrity"]=integrity
        receipt["candidateShapeIntegrities"]=integrities
        volume=evidence["referenceSolidVolumeSumMm3"]
        volume_limit=max(1e-12,abs(volume)*multi.VOLUME_RELATIVE_TOLERANCE)
        if not integrity["closed"] or not integrity["consistentlyOriented"] or abs(integrity["signedVolumeMm3"]-volume)>volume_limit:
            raise ValueError("candidate whole-mesh closure/winding/volume mismatch")
        if distance_binary is None:
            raise ValueError("explicit native distance binary required")
        tiny={"points":[[0,0,0],[1,0,0],[0,1,0]],"triangles":[[0,1,2]]}
        offset={"points":[[2,3,4],[3,3,4],[2,4,4]],"triangles":[[0,1,2]]}
        crosscheck=native_compare(distance_binary,tiny,offset,output/"distance-crosscheck.json")
        for name,a,b in [("forward",tiny,offset),("reverse",offset,tiny)]:
            python=multi.directed_bvh_sample_distance(a["points"],a["triangles"],b["points"],b["triangles"],[0])
            if not math.isclose(crosscheck[name]["maximum"],python[0],rel_tol=1e-12,abs_tol=1e-12) or not math.isclose(crosscheck[name]["mean"],python[1],rel_tol=1e-12,abs_tol=1e-12) or crosscheck[name]["sampleCount"]!=python[2]:
                raise ValueError("native/Python analytic-fixture mismatch")
        native=native_compare(distance_binary,{"points":candidate_points,"triangles":candidate_triangles},{"points":points,"triangles":triangles},output/"distance-input.json")
        receipt["nativeDistanceEvidence"]=native
        receipt["nativePythonCrosscheck"]=crosscheck
        forward=(native["forward"]["maximum"],native["forward"]["mean"],native["forward"]["sampleCount"])
        reverse=(native["reverse"]["maximum"],native["reverse"]["mean"],native["reverse"]["sampleCount"])
        work=[native["triangleDistanceEvaluations"]]
        receipt.update({"candidateShapeCount":shapes,"candidateOccurrenceCount":occurrences,
            "maximumBidirectionalSampleDistanceMm":max(forward[0],reverse[0]),
            "readerToReferenceMaxDistanceMm":forward[0],"referenceToReaderMaxDistanceMm":reverse[0],
            "sampleCountReaderToReference":forward[2],"sampleCountReferenceToReader":reverse[2],
            "sampleToleranceMm":multi.SAMPLE_TOLERANCE_MM,"volumeAbsoluteToleranceMm3":volume_limit,
            "triangleDistanceEvaluations":work[0],"repair":repaired["experimentalPlanarRepair"]})
        if max(forward[0],reverse[0])>multi.SAMPLE_TOLERANCE_MM:
            raise ValueError("bidirectional sampled surface distance failed")
        if any(fingerprint(p,limit)!=hashes[str(p)] for p,limit in tracked):
            raise ValueError("input changed during comparison")
        receipt["passed"]=True
    except Exception as error:
        receipt["error"]=f"{type(error).__name__}: {error}"
    (output/"receipt.json").write_text(json.dumps(receipt,indent=2)+"\n",encoding="utf-8")
    return receipt


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    for flag in ["binary","source","reference","output"]:
        parser.add_argument(f"--{flag}",type=Path,required=True)
    parser.add_argument("--reference-sha256",required=True)
    parser.add_argument("--distance-binary",type=Path,required=True)
    args=parser.parse_args()
    result=run(args.binary,args.source,args.reference,args.reference_sha256,args.output,args.distance_binary)
    print(json.dumps({k:v for k,v in result.items() if k in ["passed","error","maximumBidirectionalSampleDistanceMm","candidateShapeCount"]}))
    raise SystemExit(0 if result["passed"] else 2)
