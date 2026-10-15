#!/usr/bin/env python3
"""Validate pinned KiCad SVG reference artifacts and cross-version comparison."""

import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
REF_105 = ROOT / "tests/reference/kicad-cli-10.0.5/svg"
REF_107 = ROOT / "tests/reference/kicad-cli-10.0.7/svg"
TITLE = re.compile(rb"<title>.*?</title>", re.DOTALL)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def inspect_svg(path: Path) -> dict:
    raw = path.read_bytes()
    normalized, count = TITLE.subn(b"<title>__normalized__</title>", raw, count=1)
    assert count == 1, f"expected one SVG title node: {path}"
    root = ET.fromstring(raw)
    assert root.tag.endswith("svg"), f"unexpected root: {path}"
    paths = [node for node in root.iter() if node.tag.endswith("path")]
    texts = [node for node in root.iter() if node.tag.endswith("text")]
    text_content = ["".join(node.itertext()) for node in texts]
    return {
        "sha256": sha256(raw),
        "normalized_sha256": sha256(normalized),
        "path_elements": len(paths),
        "text_elements": len(texts),
        "dnp_1M_label_count": text_content.count("1M"),
        "width_mm": float(root.attrib["width"].removesuffix("mm")),
        "height_mm": float(root.attrib["height"].removesuffix("mm")),
    }


def main() -> None:
    manifest_107 = json.loads((REF_107 / "manifest.json").read_text(encoding="utf-8"))
    manifest_105 = json.loads((REF_105 / "manifest.json").read_text(encoding="utf-8"))
    assert manifest_107["schema"] == "tokito.kicad-svg-reference.v1"
    assert manifest_105["schema"] == manifest_107["schema"]
    captures = manifest_107["executable_captures"]
    version_bytes = (REF_107 / captures["version_path"]).read_bytes()
    help_bytes = (REF_107 / captures["svg_help_path"]).read_bytes()
    assert sha256(version_bytes) == captures["version_sha256"]
    assert version_bytes.strip() == b"10.0.7"
    assert sha256(help_bytes) == captures["svg_help_sha256"]
    assert captures["documented_flag_present"].encode() in help_bytes
    source = ROOT / manifest_107["reference"]["source_path"]
    source_hash = sha256(source.read_bytes())
    assert source_hash == manifest_107["reference"]["source_sha256"]
    assert source_hash == manifest_105["reference"]["source_sha256"]

    for case_107 in manifest_107["outputs"]:
        case_105 = next(item for item in manifest_105["outputs"] if item["path"] == case_107["path"])
        result_107 = inspect_svg(REF_107 / case_107["path"])
        result_105 = inspect_svg(REF_105 / case_105["path"])
        assert result_107["sha256"] == case_107["sha256"]
        assert result_105["sha256"] == case_105["sha256"]
        for key in ("path_elements", "text_elements", "dnp_1M_label_count", "width_mm", "height_mm"):
            assert result_107[key] == case_107[key], f"KiCad 10.0.7 {key} mismatch: {case_107['path']}"
            assert result_105[key] == result_107[key], f"KiCad 10.0.5/10.0.7 {key} differ: {case_107['path']}"
        assert result_107["normalized_sha256"] == case_107["normalized_sha256"]
        assert result_107["normalized_sha256"] == case_107["normalized_sha256_10_0_5"]
        assert result_107["normalized_sha256"] == result_105["normalized_sha256"]

    print("KiCad 10.0.5/10.0.7 SVG reference evidence passed: pinned source/output hashes, SVG metrics, DNP label suppression, and normalized cross-version output equality.")


if __name__ == "__main__":
    main()
