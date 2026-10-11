#!/usr/bin/env python3
"""Subprocess acceptance for read-only, bounded project intake."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


def invoke(binary: Path, root: Path, *extra: str):
    result = subprocess.run(
        [str(binary), "project", "inspect", str(root), *extra, "--json"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    return result, json.loads(result.stdout)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", required=True, type=Path)
    args = parser.parse_args()
    binary = args.bin.resolve()
    fixture = (
        Path(__file__).parent
        / "fixtures/open-source/sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb"
    )
    with tempfile.TemporaryDirectory(prefix="tokito-project-intake-") as scratch:
        root = Path(scratch)
        board_a = root / "A.kicad_pcb"
        schematic = root / "A.kicad_sch"
        shutil.copyfile(fixture, board_a)
        schematic.write_text("(kicad_sch (version 20250101))\n", encoding="utf-8")
        before = hashlib.sha256(board_a.read_bytes()).hexdigest()
        nested = root / "nested"
        nested.mkdir()
        shutil.copyfile(fixture, nested / "Hidden.kicad_pcb")

        result, report = invoke(binary, root)
        assert result.returncode == 0, result.stderr
        assert report["status"] == "accepted"
        assert [c["relativePath"] for c in report["boards"]] == ["A.kicad_pcb"]
        assert [c["relativePath"] for c in report["schematics"]] == ["A.kicad_sch"]
        assert report["selectedBoard"]["sha256"] == before
        assert report["selectedBoard"]["relativePath"] == "A.kicad_pcb"
        result, report = invoke(binary, root, "--expect-sha256", before)
        assert result.returncode == 0, result.stderr
        assert report["selectedBoard"]["sha256"] == before
        result, report = invoke(binary, root, "--expect-sha256", "0" * 64)
        assert result.returncode == 2
        assert report["selectedBoard"]["sha256"] == before
        assert any("does not match" in reason for reason in report["diagnostics"])

        board_b = root / "B.kicad_pcb"
        shutil.copyfile(fixture, board_b)
        result, report = invoke(binary, root)
        assert result.returncode == 2
        assert report["status"] == "refused"
        assert report["selectedBoard"] is None
        assert [c["relativePath"] for c in report["boards"]] == [
            "A.kicad_pcb",
            "B.kicad_pcb",
        ]
        assert any("multiple" in reason for reason in report["diagnostics"])

        result, report = invoke(binary, root, "--board", "B.kicad_pcb")
        assert result.returncode == 0, result.stderr
        assert report["selectedBoard"]["relativePath"] == "B.kicad_pcb"
        result, report = invoke(binary, root, "--board", "../A.kicad_pcb")
        assert result.returncode == 2
        assert report["selectedBoard"] is None

        board_a.unlink()
        board_b.unlink()
        result, report = invoke(binary, root)
        assert result.returncode == 2
        assert report["status"] == "refused"
        assert any("no top-level" in reason for reason in report["diagnostics"])
        malformed = root / "Malformed.kicad_pcb"
        malformed.write_text("(kicad_sch (version 20250101))\n", encoding="utf-8")
        result, report = invoke(binary, root)
        assert result.returncode == 2
        assert report["selectedBoard"] is None
        assert any("not a KiCad PCB" in reason for reason in report["diagnostics"])
        assert schematic.read_text(encoding="utf-8") == "(kicad_sch (version 20250101))\n"
        assert hashlib.sha256(fixture.read_bytes()).hexdigest() == before
    print("project intake E2E passed: selected SHA, schematic inventory, nonrecursive discovery, ambiguity, traversal, absent/invalid board, no input mutation")


if __name__ == "__main__":
    main()
