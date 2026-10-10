"""Real CLI assignment flow and independent GLB position/provenance assertions."""
import argparse
import hashlib
import json
from pathlib import Path
import tempfile
from glb_e2e import invoke, parse_glb, accessor_floats

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/fixtures/assembly-e2e/single-resistor.kicad_pcb"
MODEL = ROOT / "tests/fixtures/open-source/kicad-resistor-models/Resistor_SMD.3dshapes/R_0603_1608Metric.wrl"
TRIANGLE = b"#VRML V2.0 utf8\nShape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0,1 0 0,0 2 0] } coordIndex [0 1 2 -1] } }"


def run(binary):
    with tempfile.TemporaryDirectory(prefix="assignment-e2e-", dir=ROOT / "target") as directory:
        root = Path(directory)
        board = root / "board.kicad_pcb"
        text = FIXTURE.read_text(encoding="utf-8").rstrip()
        text = text[:-1] + '\n(footprint "test" (layer "B.Cu") (at 3 4 90) (uuid "back-r2") (property "Reference" "R2"))\n)'
        board.write_text(text, encoding="utf-8")
        triangle = root / "triangle.wrl"
        triangle.write_bytes(TRIANGLE)
        real_model = root / "resistor.wrl"
        real_model.write_bytes(MODEL.read_bytes())
        template_path = root / "template.json"
        invoke(binary, "model", "assignments", "template", board, "--output", template_path)
        manifest = json.loads(template_path.read_text())
        assert len(manifest["assignments"]) == 2
        assert all(a["model"] is None for a in manifest["assignments"])
        for row in manifest["assignments"]:
            row["evidence"] = "Synthetic pose oracle; unqualified engineering candidate"
            row["model"] = {"path": "triangle.wrl", "sha256": hashlib.sha256(TRIANGLE).hexdigest(),
                            "units": "millimetres", "scale": [1, 1, 1], "rotationDeg": [0, 0, 0], "offsetMm": [0, 0, 0]}
        path = root / "assigned.json"
        path.write_text(json.dumps(manifest), encoding="utf-8")
        verified = json.loads(invoke(binary, "model", "assignments", "verify", board, "--manifest", path, "--project-root", root, "--json").stdout)
        assert verified["supportedGeometryCount"] == 2 and verified["placementVerified"] is False
        out = root / "placed.glb"
        args = ("export", "glb", board, "--model-assignments", path, "--project-root", root, "--output", out, "--json-report")
        invoke(binary, *args, succeeds=False)
        assert not out.exists()
        report = json.loads(invoke(binary, *args, "--allow-partial").stdout)
        assert report["coverage"]["componentModelsEmitted"] == 2
        assert report["coverage"]["modelReferencesEmitted"] == 0
        assert report["coverage"]["modelReferencesUnresolved"] == 1
        assert report["partialExport"] and not report["placementVerified"]
        document, payload = parse_glb(out)
        # Hand calculated placement: board center (5,5), copper +/-0.8,
        # 0.05 mm exporter clearance, footprint +90 deg, back X180.
        expected = {"R1": [(0, .00085, 0), (0, .00085, .001), (-.002, .00085, 0)],
                    "R2": [(-.002, -.00085, .001), (0, -.00085, .001), (-.002, -.00085, .002)]}
        for node in document["nodes"][1:]:
            ref = node["extras"]["tokito"]["componentReference"]
            primitive = document["meshes"][node["mesh"]]["primitives"][0]
            provenance = primitive["extras"]["tokito"]
            assert "modelSourceOrdinal" not in provenance
            assert provenance["manifestSha256"] == hashlib.sha256(path.read_bytes()).hexdigest()
            assert provenance["sourceAssetSha256"] == hashlib.sha256(TRIANGLE).hexdigest()
            positions = accessor_floats(document, payload, primitive["attributes"]["POSITION"])
            points = list(zip(*[iter(positions)] * 3))
            assert all(any(max(abs(a-b) for a, b in zip(actual, wanted)) < 1e-7 for actual in points) for wanted in expected[ref]), (ref, points, expected[ref])
        # Real pinned body geometry, not a generated placeholder.
        manifest["assignments"][0]["model"].update(path="resistor.wrl", sha256=hashlib.sha256(real_model.read_bytes()).hexdigest(), units="kicad-wrl")
        manifest["assignments"][1]["population"] = "not-fitted"
        path.write_text(json.dumps(manifest), encoding="utf-8")
        real = root / "real-resistor.glb"
        receipt = json.loads(invoke(binary, "export", "glb", board, "--model-assignments", path, "--project-root", root, "--output", real, "--allow-partial", "--json-report").stdout)
        assert receipt["coverage"]["componentModelsEmitted"] == 1
        doc, _ = parse_glb(real)
        assert [n["name"] for n in doc["nodes"][1:]] == ["ref:R1"]
        manifest["assignments"][0]["model"]["sha256"] = "0" * 64
        path.write_text(json.dumps(manifest), encoding="utf-8")
        refused = root / "hash-conflict.glb"
        invoke(binary, "export", "glb", board, "--model-assignments", path, "--project-root", root, "--output", refused, "--allow-partial", succeeds=False)
        assert not refused.exists()
        manifest["sourceBoardSha256"] = "0" * 64
        path.write_text(json.dumps(manifest), encoding="utf-8")
        invoke(binary, "model", "assignments", "verify", board, "--manifest", path, "--project-root", root, succeeds=False)
        invoke(binary, "model", "assignments", "--help")
    print("assignment E2E passed: exact identities, real WRL body, independent top/back pose, coverage, provenance and refusal paths")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    run(parser.parse_args().binary.resolve())
