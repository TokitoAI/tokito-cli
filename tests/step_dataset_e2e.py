"""Run one hash-bound dataset STEP component through the real CLI and decode its GLB."""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import shutil
import struct
import tempfile

from glb_e2e import accessor_floats, invoke, parse_glb


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--board", type=Path, required=True)
    parser.add_argument("--mapping", type=Path, required=True)
    parser.add_argument("--extraction", type=Path, required=True)
    parser.add_argument("--step", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--reference", default="R41")
    args = parser.parse_args()

    binary = args.binary.resolve(strict=True)
    board = args.board.resolve(strict=True)
    mapping = json.loads(args.mapping.read_text(encoding="utf-8"))
    extraction = json.loads(args.extraction.read_text(encoding="utf-8"))
    step_digest = sha256(args.step)
    board_digest = sha256(board)
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=False)

    if mapping["source_pcb_sha256"] != board_digest:
        raise AssertionError("dataset mapping is not hash-bound to the supplied PCB")
    row = next(
        (candidate for candidate in mapping["rows"]
         if candidate["reference_designator"] == args.reference),
        None,
    )
    if row is None or not row.get("footprint_uuid") or not row.get("model"):
        raise AssertionError(f"dataset has no exact model row for {args.reference}")
    if row["model_mapping_status"] != "imported_model_reference_with_unverified_native_equivalence":
        raise AssertionError("expected explicit unverified native-equivalence status")
    extraction_model = next(
        (candidate for candidate in extraction["models"]
         if candidate["sha256"] == step_digest),
        None,
    )
    if extraction_model is None or extraction_model["bytes"] != args.step.stat().st_size:
        raise AssertionError("STEP bytes do not match any pinned dataset extraction record")

    with tempfile.TemporaryDirectory(prefix="tokito-dataset-step-") as temporary:
        work = Path(temporary)
        template_path = work / "template.json"
        invoke(binary, "model", "assignments", "template", board, "--output", template_path)
        manifest = json.loads(template_path.read_text(encoding="utf-8"))
        assignment = next(
            (candidate for candidate in manifest["assignments"]
             if candidate["reference"] == args.reference),
            None,
        )
        if assignment is None:
            raise AssertionError(f"CLI parser found no {args.reference} source component")
        if assignment["componentUuid"] != row["footprint_uuid"]:
            raise AssertionError("dataset mapping UUID does not match parsed PCB component")

        model = row["model"]
        model_root = work / "models"
        model_root.mkdir()
        model_name = "dataset-candidate.step"
        shutil.copyfile(args.step, model_root / model_name)
        assignment["population"] = "unknown"
        assignment["evidence"] = (
            "Explicit dataset candidate only: source mapping records the imported model reference, "
            "but native Altium equivalence and physical fit remain unverified."
        )
        assignment["model"] = {
            "path": model_name,
            "sha256": step_digest,
            "units": "step-declared",
            "scale": model["local_scale"],
            "rotationDeg": model["local_rotation_degrees_xyz"],
            "offsetMm": model["local_offset_mm"],
        }
        manifest_path = work / "assignments.json"
        manifest_path.write_text(json.dumps(manifest, separators=(",", ":")), encoding="utf-8")

        verification = json.loads(
            invoke(
                binary, "model", "assignments", "verify", board,
                "--manifest", manifest_path, "--project-root", model_root, "--json",
            ).stdout
        )
        verified = next(item for item in verification["assignments"] if item["reference"] == args.reference)
        if verified["status"] != "resolved_supported_step_candidate":
            raise AssertionError(f"dataset STEP candidate did not resolve: {verified}")
        if verification["physicalQualificationVerified"] is not False:
            raise AssertionError("STEP geometry must remain explicitly unqualified")

        glb_path = output_dir / f"{args.reference}-candidate.glb"
        report = json.loads(
            invoke(
                binary, "export", "glb", board,
                "--model-assignments", manifest_path,
                "--project-root", model_root,
                "--output", glb_path,
                "--allow-partial", "--json-report",
            ).stdout
        )
        detail = next(
            item for item in report["assignments"]["components"]
            if item["reference"] == args.reference
        )
        if detail["status"] != "emitted_claimed_candidate":
            raise AssertionError(f"candidate model was not emitted: {detail}")
        dropped_faces = detail.get("stepPrecisionDroppedTriangleCount", 0)
        if dropped_faces and not any(
            "sub-resolution surface gaps may remain" in diagnostic
            for diagnostic in detail.get("geometryDiagnostics", [])
        ):
            raise AssertionError("assignment report must explain any omitted STEP faces")
        if report["coverage"]["componentModelsEmitted"] != 1:
            raise AssertionError("partial candidate export should contain exactly one assigned component")
        if report["assignments"]["wholeBoardDigitalTwinComplete"] is not False:
            raise AssertionError("single-component partial export cannot claim whole-board completion")
        if report["assignments"]["physicalQualificationVerified"] is not False:
            raise AssertionError("dataset candidate must remain physically unqualified")

        document, payload = parse_glb(glb_path)
        selectors = [node for node in document["nodes"] if node.get("name") == f"ref:{args.reference}"]
        if len(selectors) != 1:
            raise AssertionError("GLB must expose exactly one selector for the assigned reference")
        selector = selectors[0]
        identity = selector["extras"]["tokito"]
        if identity["componentUuid"] != row["footprint_uuid"] or identity["selectable"] is not True:
            raise AssertionError("GLB selector identity or selection contract is wrong")
        if "mesh" not in selector or "translation" not in selector:
            raise AssertionError("STEP selector must carry a mesh and component-local node translation")
        translation = selector["translation"]
        if len(translation) != 3 or not all(math.isfinite(value) for value in translation):
            raise AssertionError("selector node translation is invalid")

        mesh = document["meshes"][selector["mesh"]]
        local_points: list[tuple[float, float, float]] = []
        triangle_count = 0
        for primitive in mesh["primitives"]:
            provenance = primitive["extras"]["tokito"]
            if provenance["sourceAssetSha256"] != step_digest:
                raise AssertionError("GLB primitive does not retain the exact STEP asset digest")
            if provenance.get("meshCoordinateFrame") != "component_local_glb_metres":
                raise AssertionError("STEP mesh is not stored in the expected component-local frame")
            positions = accessor_floats(document, payload, primitive["attributes"]["POSITION"])
            if not positions or len(positions) % 9:
                raise AssertionError("component primitive does not contain complete triangle positions")
            triangle_count += len(positions) // 9
            local_points.extend(
                tuple(positions[index + axis] for axis in range(3))
                for index in range(0, len(positions), 3)
            )
        if triangle_count <= 0 or not local_points:
            raise AssertionError("STEP candidate produced an empty mesh")
        if dropped_faces:
            for primitive in mesh["primitives"]:
                material_index = primitive.get("material")
                if material_index is None or document["materials"][material_index].get("doubleSided") is not True:
                    raise AssertionError("face-omitting STEP candidate must be rendered as an open, two-sided mesh")
        world_points = [
            tuple(f32(point[axis] + f32(translation[axis])) for axis in range(3))
            for point in local_points
        ]
        if not all(math.isfinite(value) for point in world_points for value in point):
            raise AssertionError("reconstructed world vertices contain a non-finite coordinate")
        for offset in range(0, len(world_points), 3):
            a, b, c = world_points[offset : offset + 3]
            ab = tuple(b[axis] - a[axis] for axis in range(3))
            ac = tuple(c[axis] - a[axis] for axis in range(3))
            normal = (
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            )
            if not any(component != 0.0 for component in normal):
                raise AssertionError(
                    "serialized GLB triangle collapses after float32 node translation"
                )
        minimum = [min(point[axis] for point in world_points) for axis in range(3)]
        maximum = [max(point[axis] for point in world_points) for axis in range(3)]
        if any(high <= low for low, high in zip(minimum, maximum)):
            raise AssertionError("reconstructed component bounds are degenerate")

        receipt = {
            "passed": True,
            "scope": "one explicit unqualified STEP component candidate",
            "reference": args.reference,
            "componentUuid": row["footprint_uuid"],
            "sourceBoardSha256": board_digest,
            "datasetMappingSha256": sha256(args.mapping),
            "sourceStepSha256": step_digest,
            "datasetExtractionSha256": extraction_model["sha256"],
            "candidateStatus": detail["status"],
            "triangleCount": triangle_count,
            "worldSpaceDegenerateTriangles": 0,
            "componentBoundsMetres": {"min": minimum, "max": maximum},
            "stepPrecisionDroppedTriangleCount": dropped_faces,
            "physicalQualificationVerified": False,
            "modelPhysicalMatchVerified": False,
            "placementVerified": False,
            "wholeBoardDigitalTwinComplete": False,
            "glbSha256": sha256(glb_path),
            "glbPath": str(glb_path),
        }
        receipt_path = output_dir / f"{args.reference}-candidate-e2e.json"
        receipt_path.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")

    print(
        f"PASS dataset STEP E2E: {args.reference} selector UUID={row['footprint_uuid']}; "
        f"{triangle_count} triangles; "
        f"{dropped_faces} sub-resolution faces recorded; "
        "candidate remains physically unqualified"
    )
    print(f"GLB: {glb_path}")
    print(f"Receipt: {receipt_path}")


if __name__ == "__main__":
    main()
