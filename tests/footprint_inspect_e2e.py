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
        assert report["structuralInspectionOnly"] is True
        assert report["semanticValidationPerformed"] is False
        assert report["geometryValidationPerformed"] is False
        assert report["manufacturingQualified"] is False
        assert source.read_bytes() == source_bytes, "inspection modified its input"

        summary = invoke(binary, "component", "footprint", "inspect", source)
        assert summary.returncode == 0, summary.stderr
        assert "structural fields only" in summary.stdout.lower()

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
