"""Actual CLI selection/handoff and negative paths; synthetic hierarchy fixture."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import tempfile

from glb_e2e import invoke, parse_glb
from e2e_temp import e2e_temp_root

ROOT = Path(__file__).resolve().parents[1]


def encode(doc, payload):
    text = json.dumps(doc, separators=(",", ":")).encode()
    text += b" " * (-len(text) % 4)
    payload += b"\0" * (-len(payload) % 4)
    size = 12 + 8 + len(text) + 8 + len(payload)
    return struct.pack("<4sII", b"glTF", 2, size) + struct.pack("<I4s", len(text), b"JSON") + text + struct.pack("<I4s", len(payload), b"BIN\0") + payload


def run(binary):
    with tempfile.TemporaryDirectory(prefix="twin-e2e-", dir=e2e_temp_root(ROOT / "target")) as temp:
        root = Path(temp)
        source = root / "native.glb"
        invoke(binary, "export", "glb", ROOT / "tests/fixtures/assembly-e2e/single-resistor.kicad_pcb",
               "--assemble-wrl", "--project-root", ROOT / "tests/fixtures/assembly-e2e",
               "--library-root", ROOT / "tests/fixtures/open-source/kicad-resistor-models",
               "--output", source, "--allow-partial", "--json-report")
        doc, payload = parse_glb(source)
        mesh_node = next(i for i, n in enumerate(doc["nodes"]) if n.get("name") == "ref:R1")
        doc["nodes"][mesh_node]["name"] = "original-package-shape"
        parent = len(doc["nodes"])
        doc["nodes"].append({"name": "ref:R1", "children": [mesh_node], "translation": [.035797007, .001595, -.012689002000000001]})
        scene = doc["scenes"][doc.get("scene", 0)]
        scene["nodes"] = [parent if i == mesh_node else i for i in scene["nodes"]]
        glb = root / "nested.glb"
        glb.write_bytes(encode(doc, payload))
        manifest = {
            "schemaVersion": "1.0", "assetId": "fixture-twin", "boardIdentity": "fixture-board",
            "boardRevision": "aaaaaaaaaaaa", "assemblyContextRevision": "unreviewed-fixture-context",
            "units": "METRE", "axes": "RIGHT_HANDED_Y_UP", "componentNodePrefix": "ref:",
            "boardOriginMetres": {"x": 0, "y": 0, "z": 0},
            "boundsMetres": {"minimum": {"x": -.02, "y": -.02, "z": -.02},
                             "maximum": {"x": .02, "y": .02, "z": .02}},
            "defaultOpacity": .42,
        }
        manifest_path = root / "manifest.json"

        def bind():
            manifest["glbBytes"] = glb.stat().st_size
            manifest["glbSha256"] = hashlib.sha256(glb.read_bytes()).hexdigest()
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

        bind()
        mapping = {"board_id": "fixture-board", "board_revision": "aaaaaaaaaaaa",
                   "asset_id": "fixture-twin", "component_count": 3, "renderable_count": 1,
                   "sourceEvidence": {"format": "altium_pcb", "sha256": "a" * 64},
                   "components": [
                       {"reference": "R1", "component_class": "resistor", "glb_selector": "ref:R1", "needs_3d_body": True},
                       {"reference": "R2", "component_class": "resistor", "glb_selector": None, "needs_3d_body": True},
                       {"reference": "D1", "component_id": "fixture-D1", "component_class": None,
                        "glb_selector": None, "needs_3d_body": False,
                        "sourceDesignClaims": {"status": "unverified_source_design_claim",
                            "sourceFormat": "altium_pcb", "sourceSha256": "a" * 64,
                            "sourceIdentity": {"namespace": "altium-pcb-uniqueid",
                                "uniqueId": "fixture-D1", "reference": "D1"},
                            "properties": {"SOURCEDESCRIPTION": "Surface mount schottky diode SOD-323 package",
                                "PATTERN": "SMT_DIODE_SOD_323", "SOURCELIBREFERENCE": "SMT_DIODE_SOD_323",
                                "COMMENT": None}}}]}
        map_path = root / "map.json"
        map_path.write_text(json.dumps(mapping), encoding="utf-8")
        common = (glb, "--component-map", map_path, "--manifest", manifest_path)
        audit = json.loads(invoke(binary, "twin", "inspect", *common).stdout)
        assert audit["directRenderableSelectors"] == 0 and not audit["appNameFilterCompatible"]
        assert audit["missingRequiredModelCount"] == 1 and not audit["positionalGuidanceAllowed"]
        diode = next(component for component in audit["components"] if component["reference"] == "D1")
        assert diode["componentClassClaim"] is None
        assert diode["componentClassHint"]["value"] == "diode_or_led"
        assert diode["componentClassHint"]["basis"] == "unverified_source_text"
        assert diode["componentClassHint"]["sourceFields"] == ["SOURCEDESCRIPTION", "PATTERN", "SOURCELIBREFERENCE"]
        assert diode["componentClassHint"]["selectionEligible"] is False
        for option, value, status in [
            ("--reference", "R1", "unique_candidate"), ("--reference", "R2", "model_unavailable"),
            ("--reference", "absent", "no_match"), ("--class", "resistor", "multiple_candidates"),
            ("--class", "diode_or_led", "no_match")]:
            result = json.loads(invoke(binary, "twin", "select", *common, option, value).stdout)
            assert result["status"] == status and not result["positionalGuidanceAllowed"]
        output = root / "prepared"
        invoke(binary, "twin", "prepare", *common, "--output", output, succeeds=False)
        assert not output.exists()
        receipt = json.loads(invoke(binary, "twin", "prepare", *common, "--output", output, "--allow-partial").stdout)
        assert receipt["renamedComponentSubtrees"] == 1 and receipt["appNameFilterCompatible"]
        assert receipt["status"] == "unregistered-reference-candidate"
        new_glb = output / "active.digital-twin.glb"
        new_doc, new_payload = parse_glb(new_glb)
        assert new_payload == payload
        assert new_doc["nodes"][parent]["name"] == "placement:R1"
        assert new_doc["nodes"][mesh_node]["name"] == "ref:R1" and "mesh" in new_doc["nodes"][mesh_node]
        for a, b in zip(doc["nodes"], new_doc["nodes"]):
            assert {k: v for k, v in a.items() if k != "name"} == {k: v for k, v in b.items() if k != "name"}
        index = json.loads((output / "component-index.json").read_text())
        assert index["sourceSha256"]["glb"] == hashlib.sha256(new_glb.read_bytes()).hexdigest()
        assert index["sourceSha256"]["manifest"] == hashlib.sha256((output / "active.digital-twin.json").read_bytes()).hexdigest()
        assert index["sourceSha256"]["componentMap"] == hashlib.sha256((output / "component-map.json").read_bytes()).hexdigest()
        new_common = (new_glb, "--component-map", output / "component-map.json", "--manifest", output / "active.digital-twin.json")
        assert json.loads(invoke(binary, "twin", "inspect", *new_common).stdout)["directRenderableSelectors"] == 1
        before = new_glb.read_bytes()
        invoke(binary, "twin", "prepare", *common, "--output", output, "--allow-partial", succeeds=False)
        assert new_glb.read_bytes() == before
        # Prototype shares consent/no-overwrite gates and embeds the actual mesh bytes.
        prototype = root / "prototype"
        invoke(binary, "twin", "prototype", *common, "--output", prototype, succeeds=False)
        assert not prototype.exists()
        invoke(binary, "twin", "prototype", *common, "--output", prototype, "--allow-partial")
        import base64
        html = (prototype / "index.html").read_text(encoding="utf-8")
        assert base64.b64encode((prototype / "active.digital-twin.glb").read_bytes()).decode() in html
        assert "https://" not in html and "<script src=" not in html
        assert "Reference candidate" in html and 'id="view"' in html
        assert "source-text hint" in html and "unverified" in html
        profile = json.loads((prototype / "presentation-profile.json").read_text())
        assert profile["id"] == "tokito.ultimaker-studio.v1"
        assert profile["snapshotPixels"] == [3840, 2160]
        assert profile["defaultLighting"] == "ultimaker-reference"
        assert profile["geometryModified"] is False
        assert 'id="presentation"' in html and 'snapshot4k' in html
        import re
        embedded_profile = re.search(r'<script id="presentation" type="application/json">(.*?)</script>', html, re.S)
        assert embedded_profile and json.loads(embedded_profile.group(1)) == profile
        original_html = (prototype / "index.html").read_bytes()
        invoke(binary, "twin", "prototype", *common, "--output", prototype, "--allow-partial", succeeds=False)
        assert (prototype / "index.html").read_bytes() == original_html
        # Manifest byte binding, duplicate JSON keys and shared-mesh hierarchy must refuse.
        manifest["glbSha256"] = "0" * 64
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        invoke(binary, "twin", "inspect", *common, succeeds=False)
        bind()
        saved_bounds = json.loads(json.dumps(manifest["boundsMetres"]))
        manifest["boundsMetres"]["maximum"]["x"] = manifest["boundsMetres"]["minimum"]["x"]
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        invoke(binary, "twin", "inspect", *common, succeeds=False)
        manifest["boundsMetres"] = saved_bounds
        manifest["boardPlaneCornerPointsMetres"] = [{"x": 99, "y": 0, "z": 0}] * 4
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        invoke(binary, "twin", "inspect", *common, succeeds=False)
        del manifest["boardPlaneCornerPointsMetres"]
        manifest["boardPlaneBoundsMetres"] = {"minimum": {"x": 0, "y": 0, "z": 0}, "maximum": {"x": 0, "y": 0, "z": 0}}
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        invoke(binary, "twin", "inspect", *common, succeeds=False)
        del manifest["boardPlaneBoundsMetres"]
        bind()
        map_path.write_text('{"component_count":3,"component_count":3}', encoding="utf-8")
        invoke(binary, "twin", "inspect", *common, succeeds=False)
        map_path.write_text(json.dumps(mapping), encoding="utf-8")
        doc["nodes"].append({"name": "other-shape", "mesh": doc["nodes"][mesh_node]["mesh"]})
        scene["nodes"].append(len(doc["nodes"]) - 1)
        glb.write_bytes(encode(doc, payload))
        bind()
        invoke(binary, "twin", "inspect", *common, succeeds=False)
        invoke(binary, "twin", "--help")
        malformed = bytearray(glb.read_bytes())
        malformed[8:12] = struct.pack("<I", 0)
        glb.write_bytes(malformed)
        bind()
        refused = invoke(binary, "twin", "inspect", *common, succeeds=False)
        assert refused.returncode == 2
    print("twin E2E passed: renderable selector handoff, unchanged payload/transforms, exact/class query, partial consent, hashes and refusals")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    run(parser.parse_args().binary.resolve(strict=True))
