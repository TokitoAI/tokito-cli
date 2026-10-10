#!/usr/bin/env python3
"""Capture and inspect a pinned external KiCad GLB reference.

This is an optional local acceptance journey.  It intentionally is not part of
``scripts/verify-local.ps1``: it requires an explicitly supplied KiCad binary
and tests that binary's behavior, while the Tokito engine has no KiCad runtime
dependency.  The default expectations are pinned to the locally observed
KiCad 10.0.5 artifact, not to the proposed 10.0.7 baseline.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import shutil
import struct
import subprocess
import sys
import uuid
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BOARD = ROOT / "tests" / "fixtures" / "open-source" / "arduino-uno-r3-community" / "Arduino UNO.kicad_pcb"
MODEL = Path("C:/Program Files/KiCad/10.0/share/kicad/3dmodels/Package_DFN_QFN.3dshapes/QFN-32-1EP_5x5mm_P0.5mm_EP3.3x3.3mm.step")
BOARD_SHA256 = "1b078b77026f4e8fd5de7cc6c7b6fff3daf454014e5e0873b7a6d0cdd77958f9"
MODEL_SHA256 = "5fa713ea46b36980895812ed853577889b7cb06d275041a39b99bc0be305dc79"
EXPECTED_TRANSLATION = (0.1341501, 0.001595, 0.0970026)
EXPECTED_ROTATION = (0.0, math.sqrt(0.5), 0.0, math.sqrt(0.5))


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def close(actual: float, expected: float, label: str) -> None:
    if not math.isclose(actual, expected, rel_tol=0.0, abs_tol=1e-12):
        raise AssertionError(f"{label}: expected {expected!r}, found {actual!r}")


def read_glb_json(path: Path) -> dict:
    blob = path.read_bytes()
    if len(blob) < 20:
        raise AssertionError("GLB is shorter than its required header")
    magic, version, declared_length = struct.unpack_from("<4sII", blob, 0)
    if magic != b"glTF" or version != 2 or declared_length != len(blob):
        raise AssertionError("GLB header is not a complete glTF 2 binary")
    json_length, chunk_type = struct.unpack_from("<I4s", blob, 12)
    if chunk_type != b"JSON" or 20 + json_length > len(blob):
        raise AssertionError("GLB has no complete JSON chunk")
    return json.loads(blob[20 : 20 + json_length].decode("utf-8").rstrip(" \t\r\n\0"))


def freecad_smoke(freecad_cmd: Path, glb: Path) -> dict:
    """Ask FreeCAD to import the GLB and report non-null imported shapes."""
    version = subprocess.run(
        [str(freecad_cmd), "--version"], check=True, text=True, capture_output=True
    ).stdout.strip()
    code = (
        "import FreeCAD, Import, json; "
        f"result=Import.open({json.dumps(glb.as_posix())}); "
        "doc=FreeCAD.ActiveDocument; "
        "payload={'return_count':len(result), 'object_count':len(doc.Objects), "
        "'non_null_shape_count':sum(1 for item in doc.Objects if hasattr(item, 'Shape') and not item.Shape.isNull())}; "
        "print('TOKITO_FREECAD_SMOKE=' + json.dumps(payload, sort_keys=True))"
    )
    completed = subprocess.run(
        [str(freecad_cmd), "-c", code], check=True, text=True, capture_output=True
    )
    for line in reversed(completed.stdout.splitlines()):
        if line.startswith("TOKITO_FREECAD_SMOKE="):
            result = json.loads(line.removeprefix("TOKITO_FREECAD_SMOKE="))
            if result["non_null_shape_count"] <= 0:
                raise AssertionError("FreeCAD imported no non-null shapes")
            result["version"] = version
            result["executable_sha256"] = sha256(freecad_cmd)
            return result
    raise AssertionError("FreeCAD did not emit an import receipt")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kicad-cli", type=Path, required=True)
    parser.add_argument("--expected-version", default="10.0.5")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target" / "kicad-reference-10.0.5")
    parser.add_argument("--freecad-cmd", type=Path)
    args = parser.parse_args()

    if not BOARD.is_file() or sha256(BOARD) != BOARD_SHA256:
        raise AssertionError("pinned Arduino Uno reference board is missing or changed")
    if not MODEL.is_file() or sha256(MODEL) != MODEL_SHA256:
        raise AssertionError("pinned installed QFN model is missing or changed")
    executable = args.kicad_cli.resolve(strict=True)
    version = subprocess.run([str(executable), "--version"], check=True, text=True, capture_output=True).stdout.strip()
    if version != args.expected_version:
        raise AssertionError(f"expected KiCad {args.expected_version}, found {version}")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    run_dir = args.output_dir / f"run-{uuid.uuid4().hex}"
    run_dir.mkdir()
    board_copy = run_dir / BOARD.name
    shutil.copy2(BOARD, board_copy)
    output = run_dir / "arduino-uno-u3.glb"
    command = [
        str(executable), "pcb", "export", "glb",
        "--define-var", "KICAD8_3DMODEL_DIR=C:\\Program Files\\KiCad\\10.0\\share\\kicad\\3dmodels",
        "--component-filter", "U3", "--output", str(output), str(board_copy),
    ]
    completed = subprocess.run(command, check=True, text=True, capture_output=True)
    document = read_glb_json(output)

    nodes = document.get("nodes", [])
    matches = [node for node in nodes if node.get("name") == "U3"]
    if len(matches) != 1:
        raise AssertionError(f"expected exactly one U3 node, found {len(matches)}")
    node = matches[0]
    translation = node.get("translation")
    rotation = node.get("rotation")
    if not isinstance(translation, list) or len(translation) != 3:
        raise AssertionError("U3 node has no 3-axis translation")
    if not isinstance(rotation, list) or len(rotation) != 4:
        raise AssertionError("U3 node has no quaternion rotation")
    for index, expected in enumerate(EXPECTED_TRANSLATION):
        close(float(translation[index]), expected, f"translation[{index}]")
    for index, expected in enumerate(EXPECTED_ROTATION):
        close(float(rotation[index]), expected, f"rotation[{index}]")

    freecad = None
    if args.freecad_cmd is not None:
        freecad = freecad_smoke(args.freecad_cmd.resolve(strict=True), output)
    receipt = {
        "schema": "tokito.kicad-glb-reference.v1",
        "kicad_version": version,
        "kicad_executable_sha256": sha256(executable),
        "board_sha256": BOARD_SHA256,
        "model_sha256": MODEL_SHA256,
        "run_directory": str(run_dir),
        "glb_sha256": sha256(output),
        "glb_bytes": output.stat().st_size,
        "node": {"name": "U3", "translation_m": translation, "rotation_xyzw": rotation},
        "freecad_smoke": freecad,
        "limitations": [
            "This is an observed KiCad 10.0.5 GLB artifact, not KiCad 10.0.7 parity evidence.",
            "U3 has zero model-local offset and rotation; separate asymmetric local-transform coverage is still required.",
            "The Tokito GLB frame negates saved board Y, so its component frame must be compared through an explicit frame conversion.",
        ],
    }
    (run_dir / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(receipt, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
