"""Compare Tokito POS CSV against hash-pinned KiCad 10.0.7 outputs.

The reference executable is needed only when capturing evidence. This test runs
Tokito alone and keeps the product path independent of KiCad installation.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import io
import json
import re
import shlex
import subprocess
import tempfile
from pathlib import Path

REFERENCE_CASES = {
    "uno-mm-both.csv": ("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb", "--format csv --units mm", 54),
    "uno-mm-smd.csv": ("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb", "--format csv --units mm --smd-only", 44),
    "uno-mm-exclude-fp-th.csv": ("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb", "--format csv --units mm --exclude-fp-th", 43),
    "uno-mm-exclude-dnp.csv": ("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb", "--format csv --units mm --exclude-dnp", 52),
    "uno-in-both.csv": ("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb", "--format csv", 54),
    "nxp-mm-both.csv": ("tests/fixtures/open-source/nxp-mcxa-esc/MR-ESC-MCXA-AM32.kicad_pcb", "--format csv --units mm", 67),
    "nxp-mm-front.csv": ("tests/fixtures/open-source/nxp-mcxa-esc/MR-ESC-MCXA-AM32.kicad_pcb", "--format csv --units mm --side front", 51),
    "nxp-mm-back.csv": ("tests/fixtures/open-source/nxp-mcxa-esc/MR-ESC-MCXA-AM32.kicad_pcb", "--format csv --units mm --side back", 16),
    "nxp-mm-bottom-negate.csv": ("tests/fixtures/open-source/nxp-mcxa-esc/MR-ESC-MCXA-AM32.kicad_pcb", "--format csv --units mm --bottom-negate-x", 67),
    "spark-mm-both.csv": ("tests/fixtures/open-source/sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb", "--format csv --units mm", 17),
    "spark-mm-exclude-fp-th.csv": ("tests/fixtures/open-source/sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb", "--format csv --units mm --exclude-fp-th", 13),
    "spark-mm-drill-origin.csv": ("tests/fixtures/open-source/sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb", "--format csv --units mm --use-drill-file-origin", 17),
    "pico-mm-drill-origin.csv": ("tests/fixtures/open-source/raspberry-pi-pico-community/RPI-PICO-R3a-PUBLIC - Official.kicad_pcb", "--format csv --units mm --use-drill-file-origin", 57),
    "pico-mm-drill-origin-bottom-negate.csv": ("tests/fixtures/open-source/raspberry-pi-pico-community/RPI-PICO-R3a-PUBLIC - Official.kicad_pcb", "--format csv --units mm --use-drill-file-origin --bottom-negate-x", 57),
}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def is_within(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def csv_rows(data: bytes) -> list[dict[str, str]]:
    text = data.decode("utf-8-sig").replace("\r\n", "\n")
    return list(csv.DictReader(io.StringIO(text, newline="")))


def pico_rows_match(actual: bytes, expected: bytes) -> bool:
    actual_rows = csv_rows(actual)
    expected_rows = csv_rows(expected)
    actual_named = [row for row in actual_rows if row["Ref"]]
    expected_named = [row for row in expected_rows if row["Ref"]]
    actual_unidentified = sorted(tuple(sorted(row.items())) for row in actual_rows if not row["Ref"])
    expected_unidentified = sorted(tuple(sorted(row.items())) for row in expected_rows if not row["Ref"])
    return actual_named == expected_named and actual_unidentified == expected_unidentified


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path, help="built Tokito CLI executable")
    parser.add_argument(
        "--reference-root",
        type=Path,
        default=None,
        help="directory containing the pinned reference manifest and CSV files",
    )
    args = parser.parse_args()
    repo_root = Path(__file__).resolve().parents[1]
    binary = args.binary.resolve()
    reference_root = (args.reference_root or Path("tests/reference/kicad-cli-10.0.7/pos"))
    if not reference_root.is_absolute():
        reference_root = repo_root / reference_root
    reference_root = reference_root.resolve()
    if not binary.is_file():
        raise SystemExit(f"Tokito CLI executable not found: {binary}")
    version = subprocess.run([str(binary), "--version"], capture_output=True, check=False)
    if version.returncode != 0 or not version.stdout.decode("utf-8", errors="replace").strip().startswith("tokito-cli "):
        raise SystemExit("--binary is not a Tokito CLI executable")
    if not is_within(reference_root, repo_root):
        raise SystemExit("reference root must be inside the CLI repository")

    manifest_path = reference_root / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("schema") != "tokito.kicad-cli-reference.v1":
        raise SystemExit("unsupported KiCad reference manifest schema")
    reference = manifest.get("reference", {})
    if reference.get("product") != "KiCad CLI" or reference.get("version") != "10.0.7":
        raise SystemExit("reference manifest is not pinned to KiCad CLI 10.0.7")
    executable_digest = reference.get("executableSha256", "")
    if not re.fullmatch(r"[0-9a-f]{64}", executable_digest):
        raise SystemExit("reference executable SHA-256 is missing or malformed")

    for source in manifest.get("sources", []):
        source_path = (repo_root / source["path"]).resolve()
        if not is_within(source_path, repo_root) or not source_path.is_file():
            raise SystemExit(f"invalid or missing source fixture: {source['path']}")
        if sha256(source_path) != source["sha256"]:
            raise SystemExit(f"source fixture hash mismatch: {source['path']}")

    outputs = manifest.get("outputs", [])
    if {entry.get("path") for entry in outputs} != set(REFERENCE_CASES):
        raise SystemExit("reference outputs do not match the reviewed 14-case matrix")
    source_by_path = {source["path"]: source for source in manifest.get("sources", [])}
    byte_exact = 0
    pico_semantic = 0
    with tempfile.TemporaryDirectory(prefix="tokito-kicad-10.0.7-pos-") as temp:
        temp_root = Path(temp)
        for entry in outputs:
            ref_name = entry["path"]
            if Path(ref_name).name != ref_name:
                raise SystemExit(f"unsafe reference output name: {ref_name}")
            source_path, expected_options, expected_count = REFERENCE_CASES[ref_name]
            if entry.get("sourcePath") != source_path or entry.get("options") != expected_options:
                raise SystemExit(f"manifest source/options mismatch: {ref_name}")
            if entry.get("rows") != expected_count or source_path not in source_by_path:
                raise SystemExit(f"manifest source/count mismatch: {ref_name}")
            reference_path = reference_root / ref_name
            if not reference_path.is_file() or sha256(reference_path) != entry["sha256"]:
                raise SystemExit(f"missing or corrupt pinned output: {ref_name}")
            expected = reference_path.read_bytes()
            expected_rows = csv_rows(expected)
            if len(expected_rows) != entry["rows"]:
                raise SystemExit(f"manifest row count mismatch: {ref_name}")
            output_path = temp_root / ref_name
            command = [
                str(binary),
                "pcb",
                "export",
                "pos",
                str((repo_root / source_path).resolve()),
                *shlex.split(entry["options"]),
                "--output",
                str(output_path),
            ]
            process = subprocess.run(command, capture_output=True, check=False)
            if process.returncode != 0:
                diagnostic = process.stderr.decode("utf-8", errors="replace")[:500]
                raise SystemExit(f"Tokito export failed for {ref_name}: {diagnostic}")
            if not output_path.is_file():
                raise SystemExit(f"Tokito reported success without writing {ref_name}")
            actual = output_path.read_bytes()
            actual_rows = csv_rows(actual)
            if len(actual_rows) != entry["rows"]:
                raise SystemExit(f"Tokito row count mismatch for {ref_name}")
            actual_normalized = actual.replace(b"\r\n", b"\n")
            expected_normalized = expected.replace(b"\r\n", b"\n")
            if actual_normalized == expected_normalized:
                byte_exact += 1
            elif ref_name.startswith("pico-") and pico_rows_match(actual, expected):
                pico_semantic += 1
            else:
                raise SystemExit(f"KiCad 10.0.7 POS output mismatch: {ref_name}")

    if byte_exact + pico_semantic != 14 or pico_semantic != 2:
        raise SystemExit(
            f"unexpected comparison distribution: {byte_exact} byte-exact, {pico_semantic} Pico semantic"
        )
    print(
        "PASS KiCad 10.0.7 POS E2E: 14 pinned cases; "
        f"{byte_exact} byte-exact, {pico_semantic} Pico row-equivalent cases; Tokito ran without KiCad."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
