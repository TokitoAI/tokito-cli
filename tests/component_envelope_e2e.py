#!/usr/bin/env python3
"""Exercise evidence-bound component envelope generation and GLB assignment."""
import argparse
import hashlib
import json
from pathlib import Path
import tempfile

from glb_e2e import accessor_floats, invoke, parse_glb

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/fixtures/assembly-e2e/single-resistor.kicad_pcb"


def run(binary: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="component-envelope-e2e-", dir=ROOT / "target") as temp:
        root = Path(temp)
        board = root / "board.kicad_pcb"
        board.write_bytes(FIXTURE.read_bytes())

        template_path = root / "assignments.json"
        invoke(binary, "model", "assignments", "template", board, "--output", template_path)
        manifest = json.loads(template_path.read_text(encoding="utf-8"))
        assert len(manifest["assignments"]) == 1
        assignment = manifest["assignments"][0]
        assert assignment["reference"] == "R1"
        assert assignment["componentUuid"] == "assembly-r1"

        spec = {
            "schema": "tokito.component-envelope.v1",
            "board": {"sha256": manifest["sourceBoardSha256"]},
            "component": {
                "reference": assignment["reference"],
                "uuid": assignment["componentUuid"],
                "manufacturer": "Example Manufacturer",
                "mpn": "EXAMPLE-PACKAGE",
                "package": "Example-3x3",
            },
            "evidence": {
                "datasetCommit": "a" * 40,
                "bomSourceSha256": "b" * 64,
                "manufacturerUrl": "https://example.com/package",
            },
            "bodyMm": {"length": 3.0, "width": 3.0, "height": 0.85},
        }
        spec_path = root / "component.json"
        spec_path.write_text(json.dumps(spec), encoding="utf-8")

        model_path = root / "candidate.wrl"
        generated = invoke(
            binary,
            "component", "model", "envelope", spec_path,
            "--board", board, "--output", model_path, "--json",
        )
        report = json.loads(generated.stdout)
        assert report["status"] == "candidate-envelope-only"
        assert report["output"]["geometry"] == "rectangular-body-envelope"
        assert report["bodyMm"] == {"units": "mm", "length": 3.0, "width": 3.0, "height": 0.85}
        assert report["validation"]["packageFitVerified"] is False
        assert report["validation"]["physicalBoardRevisionVerified"] is False
        assert report["limitations"]["footprintGenerated"] is False
        assert report["limitations"]["terminalsIncluded"] is False

        inspected = json.loads(invoke(binary, "model", "inspect", model_path, "--units", "millimetres", "--json").stdout)
        assert inspected["meshParts"] == 1
        assert inspected["vertices"] == 8
        assert inspected["triangles"] == 12

        model_bytes = model_path.read_bytes()
        assignment["evidence"] = "Generated dimensioned envelope; not a qualified physical model"
        assignment["model"] = {
            "path": model_path.name,
            "sha256": hashlib.sha256(model_bytes).hexdigest(),
            "units": "millimetres",
            "scale": [1.0, 1.0, 1.0],
            "rotationDeg": [0.0, 0.0, 0.0],
            "offsetMm": [0.0, 0.0, 0.0],
        }
        manifest_path = root / "assigned.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        verified = json.loads(invoke(
            binary,
            "model", "assignments", "verify", board,
            "--manifest", manifest_path, "--project-root", root, "--json",
        ).stdout)
        assert verified["supportedGeometryCount"] == 1
        assert verified["physicalQualificationVerified"] is False
        assert verified["assignments"][0]["modelPhysicalMatchVerified"] is False

        glb_path = root / "candidate.glb"
        export_report = json.loads(invoke(
            binary,
            "export", "glb", board,
            "--model-assignments", manifest_path,
            "--project-root", root,
            "--output", glb_path,
            "--allow-partial", "--json-report",
        ).stdout)
        assert export_report["partialExport"] is True
        assert export_report["placementVerified"] is False
        assert export_report["coverage"]["componentModelsEmitted"] == 1
        document, payload = parse_glb(glb_path)
        nodes = [node for node in document["nodes"] if node.get("name") == "ref:R1"]
        assert len(nodes) == 1
        node = nodes[0]
        assert node["extras"]["tokito"]["componentReference"] == "R1"
        primitive = document["meshes"][node["mesh"]]["primitives"][0]
        assert primitive["extras"]["tokito"]["sourceAssetSha256"] == hashlib.sha256(model_bytes).hexdigest()
        positions = accessor_floats(document, payload, primitive["attributes"]["POSITION"])
        points = list(zip(*[iter(positions)] * 3))
        spans = sorted(max(p[a] for p in points) - min(p[a] for p in points) for a in range(3))
        expected = [0.00085, 0.003, 0.003]
        assert all(abs(actual - target) <= 2e-7 for actual, target in zip(spans, expected)), spans

        # A source-identity mismatch and a no-overwrite attempt must fail closed.
        wrong_identity = dict(spec)
        wrong_identity["component"] = dict(spec["component"], uuid="different-uuid")
        wrong_spec = root / "wrong-identity.json"
        wrong_spec.write_text(json.dumps(wrong_identity), encoding="utf-8")
        refused = root / "refused.wrl"
        invoke(binary, "component", "model", "envelope", wrong_spec,
               "--board", board, "--output", refused, succeeds=False)
        assert not refused.exists()
        before = model_path.read_bytes()
        invoke(binary, "component", "model", "envelope", spec_path,
               "--board", board, "--output", model_path, succeeds=False)
        assert model_path.read_bytes() == before

    print("component envelope E2E passed: exact PCB identity, bounded WRL, assigned partial GLB, dimensions and refusal paths")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    run(parser.parse_args().binary.resolve(strict=True))
