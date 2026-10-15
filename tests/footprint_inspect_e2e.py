#!/usr/bin/env python3
"""Exercise bounded per-pad structural inspection against a pinned footprint."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / "tests/fixtures/kicad-10.0.5/R_0603_1608Metric.kicad_mod"


def parse_sexpr(text: str):
    tokens = re.findall(r'"(?:\\.|[^"\\])*"|[()]|[^()\s]+', text)
    stack = []
    root = None
    for token in tokens:
        if token == "(":
            node = []
            if stack:
                stack[-1].append(node)
            elif root is None:
                root = node
            else:
                raise AssertionError("multiple top-level forms")
            stack.append(node)
        elif token == ")":
            assert stack, "unexpected close"
            stack.pop()
        else:
            assert stack, "atom outside root"
            stack[-1].append(json.loads(token) if token.startswith('"') else token)
    assert root is not None and not stack, "incomplete source footprint"
    return root


def children(node, name: str):
    return [item for item in node[1:] if isinstance(item, list) and item[:1] == [name]]


def expected_pads(source: bytes):
    root = parse_sexpr(source.decode("utf-8"))
    assert root[0] == "footprint"
    pads = []
    for ordinal, pad in enumerate(children(root, "pad")):
        at = children(pad, "at")
        size = children(pad, "size")
        layers = children(pad, "layers")
        assert len(at) == len(size) == len(layers) == 1
        position = [float(value) for value in at[0][1:]]
        pads.append({
            "ordinal": ordinal,
            "number": pad[1],
            "padType": pad[2],
            "shape": pad[3],
            "atMm": {
                "x": position[0],
                "y": position[1],
                **({"rotationDeg": position[2]} if len(position) == 3 else {}),
            },
            "sizeMm": [float(value) for value in size[0][1:]],
            "roundrectRatio": next(
                (float(item[1]) for item in children(pad, "roundrect_rratio")), None
            ),
            "layers": layers[0][1:],
            "status": "reported_fields_complete",
            "uninterpretedFields": [],
            "uninterpretedFieldCount": 0,
            "uninterpretedFieldsTruncated": False,
            "diagnostics": [],
        })
    return root[1], pads


def invoke(binary: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(binary), *map(str, args)],
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    binary = parser.parse_args().binary.resolve()
    source_bytes = REFERENCE.read_bytes()
    expected_name, expected = expected_pads(source_bytes)
    with tempfile.TemporaryDirectory(prefix="tokito-footprint-inspect-") as scratch:
        root = Path(scratch)
        source = root / REFERENCE.name
        source.write_bytes(source_bytes)
        result = invoke(binary, "component", "footprint", "inspect", source, "--json")
        assert result.returncode == 0, result.stderr
        report = json.loads(result.stdout)
        assert report["schema"] == "tokito.footprint-geometry-inspection.v1"
        assert report["status"] == "reported_fields_complete"
        assert report["sourceFile"] == REFERENCE.name
        assert report["sourceBytes"] == len(source_bytes)
        assert report["sourceSha256"] == hashlib.sha256(source_bytes).hexdigest()
        assert report["footprintName"] == expected_name
        assert report["padCount"] == len(expected)
        assert report["padRecordCoverage"] == "selected-fields-only"
        assert report["pads"] == expected, "pad inventory differs from independent source parse"
        assert report["modelReferenceCount"] == 1
        assert report["modelTransformCoverage"] == "explicit-source-fields-only"
        assert report["modelReferences"] == [{
            "ordinal": 0,
            "path": "${KICAD10_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0603_1608Metric.step",
            "offsetXyz": [0.0, 0.0, 0.0],
            "scaleXyz": [1.0, 1.0, 1.0],
            "rotateXyz": [0.0, 0.0, 0.0],
            "status": "reported_explicit_fields",
            "diagnostics": [],
        }], "model-reference inventory differs from the pinned native footprint"
        assert report["structuralInspectionOnly"] is True
        assert report["semanticValidationPerformed"] is False
        assert report["geometryValidationPerformed"] is False
        assert report["manufacturingQualified"] is False
        assert source.read_bytes() == source_bytes, "inspection modified its input"

        summary = invoke(binary, "component", "footprint", "inspect", source)
        assert summary.returncode == 0, summary.stderr
        assert "model paths are not resolved" in summary.stdout.lower()
        assert "1 source-declared 3d model reference" in summary.stdout.lower()

        drill_source = root / "drilled-footprint.kicad_mod"
        drill_source.write_text(
            '(footprint "Drilled" '
            '(pad "1" thru_hole circle (at 0 0) (size 1.2 1.2) '
            '(drill 0.7 (offset 0.1 -0.2)) (layers "*.Cu" "*.Mask")) '
            '(pad "2" thru_hole oval (at 2 0 90) (size 1.4 2.0) '
            '(drill oval 0.6 1.0 (offset -0.15 0)) (layers "*.Cu" "*.Mask")) '
            '(pad "" np_thru_hole circle (at 4 0) (size 3 3) '
            '(drill 3) (layers "*.Cu" "*.Mask")))',
            encoding="utf-8",
        )
        drill_result = invoke(
            binary, "component", "footprint", "inspect", drill_source, "--json"
        )
        assert drill_result.returncode == 0, drill_result.stderr
        drill_report = json.loads(drill_result.stdout)
        assert drill_report["status"] == "reported_fields_complete"
        assert drill_report["pads"][0]["drill"] == {
            "shape": "round",
            "diameterMm": 0.7,
            "offsetMm": [0.1, -0.2],
        }
        assert drill_report["pads"][1]["drill"] == {
            "shape": "oval",
            "diameterMm": 0.6,
            "widthMm": 1.0,
            "offsetMm": [-0.15, 0.0],
        }
        assert drill_report["pads"][2]["number"] == ""
        assert drill_report["pads"][2]["drill"] == {
            "shape": "round",
            "diameterMm": 3.0,
        }
        assert drill_report["semanticValidationPerformed"] is False
        assert drill_report["manufacturingQualified"] is False

        model_source = root / "model-transform.kicad_mod"
        model_source.write_text(
            '(footprint "Model" (model "${KICAD9_3DMODEL_DIR}/part.wrl" '
            '(offset (xyz 0.1 -0.2 0.3)) (scale (xyz 1 1 1)) '
            '(rotate (xyz 0 0 90))))',
            encoding="utf-8",
        )
        model_result = invoke(binary, "component", "footprint", "inspect", model_source, "--json")
        assert model_result.returncode == 0, model_result.stderr
        model_report = json.loads(model_result.stdout)
        assert model_report["modelReferences"][0]["path"] == "${KICAD9_3DMODEL_DIR}/part.wrl"
        assert model_report["modelReferences"][0]["offsetXyz"] == [0.1, -0.2, 0.3]
        assert model_report["modelReferences"][0]["scaleXyz"] == [1.0, 1.0, 1.0]
        assert model_report["modelReferences"][0]["rotateXyz"] == [0.0, 0.0, 90.0]
        assert model_report["modelReferences"][0]["status"] == "reported_explicit_fields"

        malformed_model = root / "malformed-model.kicad_mod"
        malformed_model.write_text(
            '(footprint "BadModel" (model "part.step" '
            '(offset (xyz 0 0)) (offset (xyz 1 2 3))))',
            encoding="utf-8",
        )
        malformed_result = invoke(
            binary, "component", "footprint", "inspect", malformed_model, "--json"
        )
        assert malformed_result.returncode == 0, malformed_result.stderr
        malformed_report = json.loads(malformed_result.stdout)
        assert malformed_report["status"] == "partial_reported_fields"
        assert malformed_report["modelReferences"][0]["offsetXyz"] is None
        assert "invalid_model_offset" in malformed_report["modelReferences"][0]["diagnostics"]

        extra_model_content = root / "extra-model-content.kicad_mod"
        extra_model_content.write_text(
            '(footprint "ExtraModelContent" '
            '(model "part.step" (offset (xyz 0 0 0)) (vendor_field yes) extra))',
            encoding="utf-8",
        )
        extra_result = invoke(
            binary, "component", "footprint", "inspect", extra_model_content, "--json"
        )
        assert extra_result.returncode == 0, extra_result.stderr
        extra_report = json.loads(extra_result.stdout)
        assert extra_report["status"] == "partial_reported_fields"
        assert "uninterpreted_model_content" in extra_report["modelReferences"][0]["diagnostics"]

        malformed_pad = root / "malformed-pad.kicad_mod"
        malformed_pad.write_text(
            '(footprint "BadPad" (pad "A" smd polygonal (at 1 2 90) '
            '(size 0 1) (layers "F.Cu")))', encoding="utf-8"
        )
        partial = invoke(binary, "component", "footprint", "inspect", malformed_pad, "--json")
        assert partial.returncode == 0, partial.stderr
        partial_report = json.loads(partial.stdout)
        assert partial_report["status"] == "partial_reported_fields"
        assert partial_report["pads"][0]["shape"] == "polygonal"
        assert partial_report["pads"][0]["atMm"]["rotationDeg"] == 90.0
        assert "unsupported_pad_shape" in partial_report["pads"][0]["diagnostics"]
        assert "non_positive_pad_size" in partial_report["pads"][0]["diagnostics"]

        missing_oval_width = root / "missing-oval-width.kicad_mod"
        missing_oval_width.write_text(
            '(footprint "MissingOvalWidth" '
            '(pad "1" thru_hole oval (at 0 0) (size 1.6 2.0) '
            '(drill oval 0.6) (layers "*.Cu" "*.Mask")))',
            encoding="utf-8",
        )
        missing_width_result = invoke(
            binary, "component", "footprint", "inspect", missing_oval_width, "--json"
        )
        assert missing_width_result.returncode == 0, missing_width_result.stderr
        missing_width_report = json.loads(missing_width_result.stdout)
        assert missing_width_report["status"] == "partial_reported_fields"
        assert missing_width_report["pads"][0]["status"] == "partial_reported_fields"
        assert "widthMm" not in missing_width_report["pads"][0]["drill"]
        assert "missing_pad_drill_width" in missing_width_report["pads"][0]["diagnostics"]

        for name, payload, error in [
            ("broken.kicad_mod", b'(footprint "Broken"', "unclosed list"),
            ("wrong-root.kicad_mod", b'(module "legacy")', "unsupported S-expression root"),
            ("oversized.kicad_mod", b" " * (4 * 1024 * 1024 + 1), "size limit"),
        ]:
            path = root / name
            path.write_bytes(payload)
            refused = invoke(binary, "component", "footprint", "inspect", path, "--json")
            assert refused.returncode != 0, f"accepted invalid input {name}"
            assert error in refused.stderr, (name, refused.stderr)

        wrong_extension = root / "not-a-footprint.txt"
        wrong_extension.write_bytes(source_bytes)
        refused = invoke(binary, "component", "footprint", "inspect", wrong_extension)
        assert refused.returncode != 0 and ".kicad_mod" in refused.stderr

        help_result = invoke(binary, "help", "component", "footprint", "inspect")
        assert help_result.returncode == 0, help_result.stderr
        assert "structural inspection only" in help_result.stdout.lower()


if __name__ == "__main__":
    main()
