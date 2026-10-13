"""Optional independent oracle capture; never part of the shipping CLI/runtime."""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
from pathlib import Path

from kicad_glb_reference import read_glb_json


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kicad-cli", required=True, type=Path)
    parser.add_argument("--model-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    manifest = json.loads((repo / "tests/reference/kicad-cli-10.0.5/glb-no-dnp/manifest.json").read_text(encoding="utf-8"))
    board = repo / manifest["reference"]["source_path"]
    digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    assert digest(board) == manifest["reference"]["source_sha256"], "reference PCB changed"
    version = subprocess.run([str(args.kicad_cli), "--version"], check=True, capture_output=True, text=True).stdout.strip()
    assert version == manifest["reference"]["version"], f"unqualified oracle version: {version}"
    assert args.model_root.is_dir(), "local reference model library missing"
    args.output.mkdir(parents=True, exist_ok=False)
    names = {}
    results = {}
    for name, flags in [("default", []), ("no-dnp", ["--no-dnp"])]:
        run = args.output / name
        run.mkdir()
        copy = run / board.name
        shutil.copy2(board, copy)
        output = run / "board.glb"
        command = [str(args.kicad_cli), "pcb", "export", "glb", "--define-var", f"KICAD8_3DMODEL_DIR={args.model_root.resolve()}", *flags, "--output", str(output.resolve()), str(copy.resolve())]
        completed = subprocess.run(command, capture_output=True, text=True)
        if completed.returncode:
            raise RuntimeError(f"KiCad {name} failed: {completed.stderr}")
        doc = read_glb_json(output)
        names[name] = {node["name"] for node in doc.get("nodes", []) if isinstance(node.get("name"), str)}
        results[name] = {"sha256": digest(output), "bytes": output.stat().st_size, "node_count": len(doc.get("nodes", []))}
    removed = sorted(names["default"] - names["no-dnp"])
    added = sorted(names["no-dnp"] - names["default"])
    assert removed == manifest["removed_named_nodes"], f"unexpected removed nodes: {removed}"
    assert added == manifest["added_named_nodes"], f"unexpected added nodes: {added}"
    assert not names["no-dnp"].intersection(manifest["dnp_references"]), "retained DNP nodes"
    receipt = {"version": version, "executable_sha256": digest(args.kicad_cli), "source_sha256": digest(board), "captures": results, "removed_named_nodes": removed, "added_named_nodes": added, "selection_comparison_passed": True, "geometry_equivalence_claimed": False}
    (args.output / "verification.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print("PASS KiCad 10.0.5 independent GLB DNP selection comparison; geometry equivalence unverified")


if __name__ == "__main__":
    main()
