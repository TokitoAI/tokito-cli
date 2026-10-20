"""Build and inspect a GLB from the pinned ESP8266 model-14 repaired candidate.

This proves command wiring, the pinned single-face repair, STEP-occurrence
selection metadata, and GLB/candidate agreement. It is not an independent CAD
geometry oracle and does not qualify STEP solids or PCB component placement.
"""
import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

import diagnostic_glb_e2e as glb_e2e

SOURCE_SHA256 = "78db318947fa4d67e3d1f5ad9e7fbe3c44c9b0ffca621a8b8a0bb6dd93282b2b"
DATASET_COMMIT = "5283bc059e30e51f4d3a81a674425936fc213214"
TARGET_ITEM = 46427
TARGET_FACE = 51071
TARGET_REPRESENTATION = 58073
REPAIRED_FACE_TRIANGLES = 146
MAX_CAPTURE_BYTES = 128 * 1024 * 1024


def run(command, expected_code):
    result = subprocess.run(command, capture_output=True, timeout=300)
    if result.returncode != expected_code or result.stderr:
        raise ValueError(
            f"command exit/stderr mismatch: {result.returncode}: {result.stderr[:1024]!r}"
        )
    if len(result.stdout) > MAX_CAPTURE_BYTES:
        raise ValueError("command output exceeds the test capture limit")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ValueError(f"command did not emit JSON: {result.stdout[:1024]!r}") from error


def target_shape(candidate_mesh):
    matches = [
        (occurrence, shape_index, shape)
        for occurrence in candidate_mesh["occurrences"]
        for shape_index, shape in enumerate(occurrence["shapes"])
        if shape.get("sourceItemId") == TARGET_ITEM
    ]
    if len(matches) != 1:
        raise ValueError("pinned repair did not resolve to exactly one occurrence shape")
    occurrence, shape_index, shape = matches[0]
    faces = shape.get("sourceTriangleFaceIds", [])
    if faces.count(TARGET_FACE) != REPAIRED_FACE_TRIANGLES:
        raise ValueError("candidate repaired-face provenance/count mismatch")
    return occurrence, shape_index, shape


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    source = args.source.resolve(strict=True)
    output = args.output.resolve()
    if hashlib.sha256(source.read_bytes()).hexdigest() != SOURCE_SHA256:
        raise ValueError("wrong pinned ESP8266 model-14 STEP source")
    output.mkdir(parents=True, exist_ok=False)
    glb_path = output / "esp8266-model-14-repaired-diagnostic.glb"

    report = run(
        [
            str(binary),
            "--research-export-model14-repaired-diagnostic-glb",
            str(source),
            str(glb_path),
        ],
        0,
    )
    if (report.get("commandCompleted") is not True
            or report.get("admitted") is not False
            or report.get("productionExport") is not False
            or report.get("sourceSha256") != SOURCE_SHA256
            or report.get("sourceDatasetCommit") != DATASET_COMMIT
            or report.get("independentGeometryComparisonPerformedByCommand") is not False):
        raise ValueError("export report omitted source pin or non-admission boundary")

    repair = report.get("experimentalPlanarRepair")
    if (not isinstance(repair, dict)
            or repair.get("schema") != "tokito.step-planar-repair.v1"
            or repair.get("faceId") != TARGET_FACE
            or repair.get("representationId") != TARGET_REPRESENTATION
            or repair.get("itemId") != TARGET_ITEM
            or repair.get("oldFaceTriangles") != 146
            or repair.get("newFaceTriangles") != REPAIRED_FACE_TRIANGLES
            or repair.get("reusedBoundaryVertices") != 148
            or repair.get("positionsChanged") is not False
            or repair.get("occurrenceTransformsChanged") is not False
            or repair.get("unrelatedTrianglesChanged") is not False
            or repair.get("geometryQualified") is not False
            or repair.get("admitted") is not False):
        raise ValueError("repair report differs from the pinned repair contract")

    occurrence_world = report.get("occurrenceWorld", {})
    if (occurrence_world.get("independentRepresentationUnitsResolved") is not True
            or occurrence_world.get("readerScaleAgreesWithIndependentUnits") is not True
            or occurrence_world.get("transformCompositionResolved") is not True):
        raise ValueError("independent unit and occurrence-transform preflight was not retained")

    mesh_report = run(
        [
            str(binary),
            "--repair-planar-face", str(TARGET_FACE),
            "--source-tolerance", "0.000001",
            "--source-representation", str(TARGET_REPRESENTATION),
            str(source),
        ],
        2,
    )
    candidate_mesh = mesh_report.get("candidateMeshData")
    if (mesh_report.get("sourceSha256") != SOURCE_SHA256
            or mesh_report.get("admitted") is not False
            or mesh_report.get("exportsProduced") != []
            or not isinstance(candidate_mesh, dict)
            or candidate_mesh.get("geometryQualified") is not False
            or candidate_mesh.get("surfaceCompletenessVerified") is not False
            or candidate_mesh.get("pcbPlacementQualified") is not False):
        raise ValueError("native repaired candidate is missing or was promoted")
    occurrence, shape_index, shape = target_shape(candidate_mesh)

    document, binary_chunk, glb_bytes = glb_e2e.decode_glb(glb_path)
    counts = glb_e2e.audit(document, binary_chunk, candidate_mesh)
    asset_claim = document["asset"]["extras"]["tokito"]
    embedded_repair = asset_claim.get("experimentalPlanarRepair", {})
    if (asset_claim.get("sourceSha256") != SOURCE_SHA256
            or asset_claim.get("geometryQualified") is not False
            or asset_claim.get("surfaceCompletenessVerified") is not False
            or asset_claim.get("completeSolidVerified") is not False
            or asset_claim.get("pcbPlacementQualified") is not False
            or asset_claim.get("componentReferenceMappingAvailable") is not False
            or asset_claim.get("componentUuidMappingAvailable") is not False
            or asset_claim.get("admission", {}).get("admitted") is not False
            or embedded_repair.get("faceId") != TARGET_FACE
            or embedded_repair.get("itemId") != TARGET_ITEM
            or embedded_repair.get("newFaceTriangles") != REPAIRED_FACE_TRIANGLES):
        raise ValueError("GLB asset extras lost repair provenance or qualification boundary")

    occurrence_path = tuple(occurrence["occurrencePath"])
    matching_nodes = [
        node for node in document["nodes"]
        if tuple(node.get("extras", {}).get("tokito", {}).get("sourceOccurrencePath", []))
        == occurrence_path
    ]
    if len(matching_nodes) != 1 or "mesh" not in matching_nodes[0]:
        raise ValueError("target repaired STEP occurrence is not represented by one selectable mesh node")
    node_claim = matching_nodes[0]["extras"]["tokito"]
    if (node_claim.get("occurrenceSelectable") is not True
            or node_claim.get("componentSelectable") is not False
            or node_claim.get("componentReference") is not None
            or node_claim.get("componentUuid") is not None):
        raise ValueError("STEP occurrence selector was incorrectly claimed as a PCB component selector")
    glb_face_count = 0
    mesh = document["meshes"][matching_nodes[0]["mesh"]]
    for primitive in mesh["primitives"]:
        primitive_claim = primitive.get("extras", {}).get("tokito", {})
        if primitive_claim.get("sourceShapeIndex") == shape_index:
            glb_face_count += primitive_claim.get("sourceTriangleFaceIds", []).count(TARGET_FACE)
    if glb_face_count != REPAIRED_FACE_TRIANGLES:
        raise ValueError("GLB did not preserve all repaired target-face triangles")

    diagnostic = report.get("diagnosticGlb", {})
    if (diagnostic.get("outputSha256") != hashlib.sha256(glb_bytes).hexdigest()
            or diagnostic.get("outputBytes") != len(glb_bytes)
            or diagnostic.get("occurrenceCount") != counts["nodes"]
            or diagnostic.get("primitiveCount") != counts["primitives"]
            or diagnostic.get("triangleCount") != counts["triangles"]
            or diagnostic.get("experimentalPlanarRepairIncluded") is not True
            or diagnostic.get("temporaryFileCleanupCompleted") is not True
            or diagnostic.get("geometryQualified") is not False
            or diagnostic.get("pcbPlacementQualified") is not False
            or diagnostic.get("componentReferenceMappingAvailable") is not False):
        raise ValueError("GLB receipt differs from decoded artifact")

    original_glb_hash = hashlib.sha256(glb_bytes).hexdigest()
    refusal = run(
        [str(binary), "--research-export-model14-repaired-diagnostic-glb", str(source), str(glb_path)],
        2,
    )
    if (not str(refusal.get("error", "")).startswith("diagnostic_glb_output_must_be_new")
            or hashlib.sha256(glb_path.read_bytes()).hexdigest() != original_glb_hash):
        raise ValueError("existing GLB was overwritten or partially changed")

    wrong_source = output / "wrong-source.step"
    wrong_source.write_bytes(source.read_bytes() + b"\n")
    refused_path = output / "wrong-source.glb"
    wrong_source_report = run(
        [str(binary), "--research-export-model14-repaired-diagnostic-glb", str(wrong_source), str(refused_path)],
        2,
    )
    if (wrong_source_report.get("error") != "pinned_model14_step_source_sha256_mismatch"
            or refused_path.exists()):
        raise ValueError("non-pinned source was not refused before export")

    result = {
        "schema": "tokito.step-model14-repaired-diagnostic-glb-e2e.v1",
        "passed": True,
        "sourceSha256": SOURCE_SHA256,
        "datasetCommit": DATASET_COMMIT,
        "repair": repair,
        "decodedGlb": counts,
        "targetOccurrenceSelector": node_claim["selectorId"],
        "targetItemId": TARGET_ITEM,
        "targetFaceTriangleCount": glb_face_count,
        "outputSha256": original_glb_hash,
        "inputHashes": {
            "sourceSha256": SOURCE_SHA256,
            "evaluatorSha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "e2eSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "glbDecoderSha256": hashlib.sha256(Path(glb_e2e.__file__).read_bytes()).hexdigest(),
        },
        "independentGeometryOracleRunByThisTest": False,
        "qualification": {
            "geometryQualified": False,
            "completeSolidVerified": False,
            "pcbPlacementQualified": False,
            "componentReferenceMappingAvailable": False,
            "componentUuidMappingAvailable": False,
            "admitted": False,
            "productionExport": False,
        },
        "checks": [
            "fixed dataset source commit and SHA-256 gate",
            "pinned planar repair and source-face provenance retained in GLB",
            "GLB decoding, occurrence selector, positions, normals and source-candidate agreement",
            "explicit unqualified/component-unmapped status",
            "existing-output refusal without overwrite",
            "wrong-source refusal without output",
        ],
    }
    (output / "receipt.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result))


if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    main()
