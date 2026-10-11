#!/usr/bin/env python3
"""Exercise the actual CLI footprint generator and its refusal paths."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


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
    with tempfile.TemporaryDirectory(prefix="tokito-footprint-e2e-") as scratch:
        output = Path(scratch) / "R_0603_1608Metric.kicad_mod"
        result = invoke(binary, "component", "footprint", "r0603", "--output", str(output), "--json")
        assert result.returncode == 0, result.stderr
        report = json.loads(result.stdout)
        assert report["status"] == "library-geometry-candidate"
        assert report["padCount"] == 2
        assert report["manufacturerLandPatternQualified"] is False
        assert report["componentBodyGenerated"] is False
        data = output.read_text(encoding="utf-8")
        assert data.startswith('(footprint "R_0603_1608Metric"\n')
        assert data.count('(pad "') == 2
        assert '(pad "1" smd roundrect (at -0.825 0)' in data
        assert '(pad "2" smd roundrect (at 0.825 0)' in data
        before = hashlib.sha256(output.read_bytes()).hexdigest()
        overwrite = invoke(binary, "component", "footprint", "r0603", "--output", str(output))
        assert overwrite.returncode != 0
        assert hashlib.sha256(output.read_bytes()).hexdigest() == before
        unsupported = invoke(binary, "component", "footprint", "qfn", "--output", str(Path(scratch) / "qfn.kicad_mod"))
        assert unsupported.returncode != 0
        assert not (Path(scratch) / "qfn.kicad_mod").exists()
    print("footprint generator E2E passed: candidate report, two pads, no overwrite, unsupported family refusal")


if __name__ == "__main__":
    main()
