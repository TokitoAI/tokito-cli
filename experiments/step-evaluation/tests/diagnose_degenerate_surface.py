#!/usr/bin/env python3
"""Exploratory STEP triangle-removal diagnostic; never product admission."""

import argparse
import hashlib
import json
from pathlib import Path

import multi_surface_reference_e2e as multi
import surface_reference_e2e as surface


def filtered_mesh(mesh):
    if not isinstance(mesh, dict) or not isinstance(mesh.get("occurrences"), list):
        raise ValueError("candidate has no occurrence mesh data")
    points = []
    triangles = []
    removed = 0
    removed_faces = {}
    for occurrence in mesh["occurrences"]:
        for shape in occurrence["shapes"]:
            vertices = shape["verticesMm"]
            face_ids = shape["sourceTriangleFaceIds"]
            shape_triangles = shape["triangles"]
            if not isinstance(vertices, list) or not isinstance(shape_triangles, list) or not isinstance(face_ids, list):
                raise ValueError("candidate shape is missing vertices, triangles, or face provenance")
            offset = len(points)
            points.extend(vertices)
            for ordinal, triangle in enumerate(shape_triangles):
                if (
                    not isinstance(triangle, list)
                    or len(triangle) != 3
                    or any(type(index) is not int or index < 0 or index >= len(vertices) for index in triangle)
                ):
                    raise ValueError("candidate contains an invalid triangle index")
                a, b, c = (vertices[index] for index in triangle)
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
                    removed += 1
                    face = face_ids[ordinal] if ordinal < len(face_ids) else "missing"
                    removed_faces[str(face)] = removed_faces.get(str(face), 0) + 1
                    continue
                triangles.append([offset + index for index in triangle])
    surface.checked_mesh(points, triangles)
    return points, triangles, removed, removed_faces


def self_test_filter():
    mesh = {
        "occurrences": [{"shapes": [{
            "verticesMm": [
                [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0], [2.0, 0.0, 0.0],
            ],
            "triangles": [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3], [0, 1, 4]],
            "sourceTriangleFaceIds": [10, 11, 12, 13, 99],
        }]}],
    }
    points, triangles, removed, faces = filtered_mesh(mesh)
    integrity = surface.mesh_integrity(points, triangles)
    if (
        len(triangles) != 4
        or removed != 1
        or faces != {"99": 1}
        or not integrity["closed"]
        or not integrity["consistentlyOriented"]
    ):
        raise AssertionError("degenerate removal diagnostic regression")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--models-root", required=True, type=Path)
    parser.add_argument("--references-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.output.exists():
        raise FileExistsError(f"refusing to overwrite {args.output}")

    manifest_path = multi.DEFAULT_MANIFEST
    manifest_bytes = surface.bounded_read(manifest_path, 64 * 1024)
    manifest = json.loads(manifest_bytes)
    if (
        manifest.get("schema") != "tokito.esp8266-step-reference-manifest.v1"
        or manifest.get("datasetCommit") != "5283bc059e30e51f4d3a81a674425936fc213214"
        or [row.get("file") for row in manifest.get("models", [])] != ["0.step", "14.step", "16.step"]
    ):
        raise ValueError("source manifest is not the pinned dataset revision")
    binary_bytes = surface.bounded_read(args.binary, surface.MAX_BINARY_BYTES)
    rows = []
    multi.MAX_SURFACE_SAMPLES = 40_000  # bounded exploratory sampling only
    for item in manifest["models"]:
        reference_evidence = {}
        digest = item["sourceSha256"]
        try:
            source = args.models_root / item["file"]
            source_bytes = surface.bounded_read(source, surface.MAX_SOURCE_BYTES)
            digest = hashlib.sha256(source_bytes).hexdigest()
            if digest != item["sourceSha256"]:
                raise ValueError(f"source hash mismatch: {item['file']}")
            code, stdout, stderr = surface.capture_bounded(
                [str(args.binary), "--candidate-mesh-data", str(source)],
                timeout_seconds=300,
                stdout_limit=multi.MAX_CAPTURE_BYTES,
                stderr_limit=multi.MAX_STDERR_BYTES,
            )
            if code != 2 or stderr:
                raise ValueError(f"unexpected evaluator response for {item['file']}")
            candidate = json.loads(stdout)
            if (
                candidate.get("sourceSha256") != digest
                or candidate.get("admitted") is not False
                or candidate.get("exportsProduced") != []
            ):
                raise ValueError(f"candidate source identity or refusal changed for {item['file']}")
            mesh = candidate["candidateMeshData"]
            if (
                not isinstance(mesh, dict)
                or mesh.get("readerWarnings") != []
                or mesh.get("geometryQualified") is not False
                or mesh.get("pcbPlacementQualified") is not False
            ):
                raise ValueError(f"candidate warnings or non-admission changed for {item['file']}")
            points, triangles, removed, removed_faces = filtered_mesh(mesh)
            candidate_integrity = surface.mesh_integrity(points, triangles)
            reference_path = args.references_root / (Path(item["file"]).stem + ".freecad.json")
            reference_bytes = surface.bounded_read(reference_path, multi.MAX_REFERENCE_JSON_BYTES)
            reference = json.loads(reference_bytes)
            if (
                reference.get("schema") != "tokito.freecad-reference-mesh.v2"
                or reference.get("inputSha256") != digest
                or reference.get("datasetCommit") != manifest["datasetCommit"]
                or reference.get("modelFile") != item["file"]
                or reference.get("freecadVersion") != manifest["freecadVersion"]
                or reference.get("deflectionMm") != manifest["tessellationDeflectionMm"]
                or reference.get("solidCount") != item["solidCount"]
                or reference.get("faceCount") != item["faceCount"]
                or reference.get("valid") is not True
            ):
                raise ValueError(f"reference source identity mismatch: {item['file']}")
            reference_points, reference_triangles, reference_evidence = multi.checked_solid_reference(reference, item["solidCount"])
            reference_evidence["referenceSha256"] = hashlib.sha256(reference_bytes).hexdigest()
            if not reference_evidence["allReferenceSolidsClosedAndConsistentlyOriented"]:
                raise ValueError(f"per-solid reference integrity failed: {item['file']}")
            reference_volume = reference_evidence["referenceSolidVolumeSumMm3"]
            volume_tolerance = max(
                1e-12, abs(reference_volume) * multi.VOLUME_RELATIVE_TOLERANCE
            )
            work = [0]
            forward = multi.directed_bvh_sample_distance(
                points, triangles, reference_points, reference_triangles, work
            )
            reverse = multi.directed_bvh_sample_distance(
                reference_points, reference_triangles, points, triangles, work
            )
            rows.append({
                "status": "diagnostic_completed",
                **reference_evidence,
                "file": item["file"],
                "sourceSha256": digest,
                "referenceSha256": hashlib.sha256(reference_bytes).hexdigest(),
                "trianglesBefore": len(mesh["occurrences"][0]["shapes"][0]["triangles"])
                if len(mesh["occurrences"]) == 1 and len(mesh["occurrences"][0]["shapes"]) == 1
                else sum(len(shape["triangles"]) for occurrence in mesh["occurrences"] for shape in occurrence["shapes"]),
                "trianglesAfter": len(triangles),
                "degeneratesRemoved": removed,
                "removedSourceFaces": removed_faces,
                "candidateClosedAfterRemoval": candidate_integrity["closed"],
                "candidateConsistentlyOrientedAfterRemoval": candidate_integrity["consistentlyOriented"],
                "candidateVolumeMm3": candidate_integrity["signedVolumeMm3"],
                "referenceVolumeMm3": reference_volume,
                "volumeWithinPriorTolerance": abs(candidate_integrity["signedVolumeMm3"] - reference_volume) <= volume_tolerance,
                "readerToReferenceMaxSampleDistanceMm": forward[0],
                "referenceToReaderMaxSampleDistanceMm": reverse[0],
                "exploratorySampleCountPerDirection": min(len(triangles), 10_000) * 4,
                "triangleDistanceEvaluations": work[0],
                "productionStepAdmitted": False,
            })
        except Exception as error:
            rows.append({
                **reference_evidence,
                "file": item["file"],
                "sourceSha256": digest,
                "status": "failed",
                "error": f"{type(error).__name__}: {str(error)[:1200]}",
                "productionStepAdmitted": False,
            })

    receipt = {
        "schema": "tokito.step-degenerate-removal-diagnostic.v2",
        "datasetCommit": manifest["datasetCommit"],
        "manifestSha256": hashlib.sha256(manifest_bytes).hexdigest(),
        "evaluatorSha256": hashlib.sha256(binary_bytes).hexdigest(),
        "freecadVersion": manifest["freecadVersion"],
        "purpose": "diagnose whether dropping zero-area reader triangles preserves closure, orientation, volume and bounded sampled surface agreement",
        "productionStepAdmitted": False,
        "diagnosticCompleted": all(row.get("status") == "diagnostic_completed" for row in rows),
        "models": rows,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(receipt, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    print(json.dumps(rows, separators=(",", ":")))
    if not receipt["diagnosticCompleted"]:
        raise SystemExit(2)


if __name__ == "__main__":
    self_test_filter()
    main()
