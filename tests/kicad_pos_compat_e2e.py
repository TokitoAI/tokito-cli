"""Compare Tokito POS formats against hash-pinned KiCad reference outputs.

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
from datetime import datetime
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

ASCII_REFERENCE_CASES = {
    "uno-mm-both.pos": ("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb", "--format ascii --units mm --side both", 54),
    "uno-in-both.pos": ("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb", "--format ascii --units in --side both", 54),
    "nxp-mm-front.pos": ("tests/fixtures/open-source/nxp-mcxa-esc/MR-ESC-MCXA-AM32.kicad_pcb", "--format ascii --units mm --side front", 51),
    "nxp-mm-back.pos": ("tests/fixtures/open-source/nxp-mcxa-esc/MR-ESC-MCXA-AM32.kicad_pcb", "--format ascii --units mm --side back", 16),
    "nxp-mm-bottom-negate.pos": ("tests/fixtures/open-source/nxp-mcxa-esc/MR-ESC-MCXA-AM32.kicad_pcb", "--format ascii --units mm --side both --bottom-negate-x", 67),
    "nxp-mm-exclude-dnp.pos": ("tests/fixtures/open-source/nxp-mcxa-esc/MR-ESC-MCXA-AM32.kicad_pcb", "--format ascii --units mm --side both --exclude-dnp", 65),
    "spark-mm-drill-origin.pos": ("tests/fixtures/open-source/sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb", "--format ascii --units mm --side both --use-drill-file-origin", 17),
    "spark-mm-exclude-fp-th.pos": ("tests/fixtures/open-source/sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb", "--format ascii --units mm --side both --exclude-fp-th", 13),
    "pico-mm-drill-origin-bottom-negate.pos": ("tests/fixtures/open-source/raspberry-pi-pico-community/RPI-PICO-R3a-PUBLIC - Official.kicad_pcb", "--format ascii --units mm --side both --use-drill-file-origin --bottom-negate-x", 57),
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


def normalized_ascii(
    data: bytes, *, expected_tool: str, expected_version: str
) -> tuple[bytes, int]:
    if data.startswith(b"\xef\xbb\xbf"):
        raise SystemExit("POS ASCII output unexpectedly contains a UTF-8 BOM")
    normalized = data.replace(b"\r\n", b"\n")
    if b"\r" in normalized:
        raise SystemExit("POS ASCII output contains a non-CRLF carriage return")
    if not normalized.endswith(b"\n") or normalized.endswith(b"\n\n"):
        raise SystemExit("POS ASCII output must end with exactly one line feed")
    lines = normalized.split(b"\n")
    if len(lines) < 7 or lines[-1] != b"":
        raise SystemExit("POS ASCII output is missing its header or body")
    timestamp = re.fullmatch(
        rb"### Footprint positions - created on (\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}) ###",
        lines[0],
    )
    if timestamp is None:
        raise SystemExit("POS ASCII output has an invalid generated timestamp header")
    try:
        datetime.fromisoformat(timestamp.group(1).decode("ascii"))
    except ValueError as error:
        raise SystemExit("POS ASCII output timestamp is not a valid ISO-8601 time") from error
    lines[0] = b"### Footprint positions - created on <timestamp> ###"

    expected_version_line = f"### Printed by {expected_tool} version {expected_version}".encode("ascii")
    if lines[1] != expected_version_line:
        raise SystemExit(f"POS ASCII output is missing the expected {expected_tool} version header")
    lines[1] = b"### Printed by <tool> version <version>"

    if not lines[4].startswith(b"# Ref") or lines[-2] != b"## End":
        raise SystemExit("POS ASCII output has an unexpected table header or terminator")
    rows = [line for line in lines[5:-2] if line.strip()]
    return b"\n".join(lines), len(rows)


def pico_ascii_rows_match(actual: bytes, expected: bytes) -> bool:
    actual_lines = actual.split(b"\n")
    expected_lines = expected.split(b"\n")
    if actual_lines[:5] != expected_lines[:5] or actual_lines[-1:] != expected_lines[-1:]:
        return False
    value_column = actual_lines[4].find(b"Val", 1)
    if value_column < 0 or expected_lines[4].find(b"Val", 1) != value_column:
        return False

    def separate(rows: list[bytes]) -> tuple[list[bytes], list[bytes]]:
        named = []
        unidentified = []
        for row in rows:
            if row[:value_column].strip():
                named.append(row)
            else:
                unidentified.append(row)
        return named, sorted(unidentified)

    return separate(actual_lines[5:-2]) == separate(expected_lines[5:-2])


def verify_ascii_normalizer_guards(reference_data: bytes) -> None:
    args = {"expected_tool": "KiCad", "expected_version": "10.0.5"}
    normalized_ascii(reference_data, **args)
    invalid_inputs = {
        "BOM": b"\xef\xbb\xbf" + reference_data,
        "missing final newline": reference_data.rstrip(b"\r\n"),
        "extra final newline": reference_data + b"\n",
        "incorrect KiCad version": reference_data.replace(
            b"### Printed by KiCad version 10.0.5",
            b"### Printed by KiCad version 10.0.6",
            1,
        ),
    }
    for label, candidate in invalid_inputs.items():
        try:
            normalized_ascii(candidate, **args)
        except SystemExit:
            continue
        raise SystemExit(f"POS ASCII normalizer incorrectly accepted {label}")

    canonical_reference, _ = normalized_ascii(reference_data, **args)
    changed_row = reference_data.replace(b"FID", b"FIX", 1)
    if changed_row == reference_data:
        lines = reference_data.splitlines(keepends=True)
        row_index = next(
            index for index, line in enumerate(lines) if index >= 5 and line.strip() and b"## End" not in line
        )
        line = lines[row_index]
        lines[row_index] = b"Z" + line[1:]
        changed_row = b"".join(lines)
    changed_normalized, _ = normalized_ascii(changed_row, **args)
    if changed_normalized == canonical_reference:
        raise SystemExit("POS ASCII normalizer unexpectedly discarded a row-content difference")


def run_ascii_compatibility(
    binary: Path,
    repo_root: Path,
    reference_root: Path | None,
    tokito_version: str,
) -> int:
    reference_root = reference_root or Path("tests/reference/kicad-cli-10.0.5/pos-ascii")
    if not reference_root.is_absolute():
        reference_root = repo_root / reference_root
    reference_root = reference_root.resolve()
    if not is_within(reference_root, repo_root):
        raise SystemExit("ASCII reference root must be inside the CLI repository")
    manifest_path = reference_root / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("schema") != "tokito.kicad-cli-pos-ascii-reference.v1":
        raise SystemExit("unsupported KiCad POS ASCII manifest schema")
    reference = manifest.get("reference", {})
    if reference.get("product") != "KiCad CLI" or reference.get("version") != "10.0.5":
        raise SystemExit("POS ASCII reference manifest is not pinned to KiCad CLI 10.0.5")
    if not re.fullmatch(r"[0-9a-f]{64}", reference.get("executableSha256", "")):
        raise SystemExit("POS ASCII reference executable SHA-256 is missing or malformed")

    source_paths = set()
    for source in manifest.get("sources", []):
        source_path = (repo_root / source["path"]).resolve()
        if not is_within(source_path, repo_root) or not source_path.is_file():
            raise SystemExit(f"invalid or missing ASCII source fixture: {source['path']}")
        if sha256(source_path) != source["sha256"]:
            raise SystemExit(f"ASCII source fixture hash mismatch: {source['path']}")
        source_paths.add(source["path"])

    outputs = manifest.get("outputs", [])
    if {entry.get("path") for entry in outputs} != set(ASCII_REFERENCE_CASES):
        raise SystemExit("POS ASCII outputs do not match the reviewed nine-case matrix")
    exact = 0
    pico_semantic = 0
    with tempfile.TemporaryDirectory(prefix="tokito-kicad-pos-ascii-") as temp:
        temp_root = Path(temp)
        for entry in outputs:
            name = entry["path"]
            if Path(name).name != name:
                raise SystemExit(f"unsafe ASCII reference output name: {name}")
            source_path, expected_options, expected_count = ASCII_REFERENCE_CASES[name]
            if (
                entry.get("sourcePath") != source_path
                or entry.get("options") != expected_options
                or entry.get("rows") != expected_count
                or source_path not in source_paths
            ):
                raise SystemExit(f"POS ASCII manifest source/options/count mismatch: {name}")
            reference_path = reference_root / name
            if not reference_path.is_file() or sha256(reference_path) != entry["sha256"]:
                raise SystemExit(f"missing or corrupt pinned ASCII output: {name}")
            reference_data = reference_path.read_bytes()
            expected, expected_rows = normalized_ascii(
                reference_data,
                expected_tool="KiCad",
                expected_version=reference["version"],
            )
            if name == "uno-mm-both.pos":
                verify_ascii_normalizer_guards(reference_data)
            if expected_rows != expected_count:
                raise SystemExit(f"POS ASCII reference row count mismatch: {name}")
            output_path = temp_root / name
            command = [
                str(binary),
                "pcb",
                "export",
                "pos",
                str((repo_root / source_path).resolve()),
                *shlex.split(expected_options),
                "--output",
                str(output_path),
            ]
            process = subprocess.run(command, capture_output=True, check=False)
            if process.returncode != 0:
                diagnostic = process.stderr.decode("utf-8", errors="replace")[:500]
                raise SystemExit(f"Tokito ASCII export failed for {name}: {diagnostic}")
            if not output_path.is_file():
                raise SystemExit(f"Tokito reported success without writing {name}")
            actual, actual_rows = normalized_ascii(
                output_path.read_bytes(),
                expected_tool="Tokito CLI",
                expected_version=tokito_version,
            )
            if actual_rows != expected_count:
                raise SystemExit(f"Tokito POS ASCII row count mismatch: {name}")
            if actual == expected:
                exact += 1
            elif name.startswith("pico-") and pico_ascii_rows_match(actual, expected):
                pico_semantic += 1
            else:
                raise SystemExit(f"KiCad 10.0.5 POS ASCII output mismatch: {name}")
    if exact + pico_semantic != 9 or pico_semantic != 1:
        raise SystemExit(
            f"unexpected ASCII comparison distribution: {exact} exact, {pico_semantic} Pico row-equivalent"
        )
    print(
        "PASS KiCad 10.0.5 POS ASCII E2E: 9 pinned cases; "
        f"{exact} exact, {pico_semantic} Pico row-equivalent; only timestamp/version headers and "
        "line endings normalized; Tokito ran without KiCad."
    )
    return 9


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
    parser.add_argument(
        "--ascii-reference-root",
        type=Path,
        default=None,
        help="directory containing the pinned KiCad 10.0.5 ASCII manifest and files",
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
    version_text = version.stdout.decode("utf-8", errors="replace").strip()
    version_match = re.fullmatch(r"tokito-cli ([A-Za-z0-9.+-]+)", version_text)
    if version.returncode != 0 or version_match is None:
        raise SystemExit("--binary is not a Tokito CLI executable")
    tokito_version = version_match.group(1)
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
    run_ascii_compatibility(binary, repo_root, args.ascii_reference_root, tokito_version)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
