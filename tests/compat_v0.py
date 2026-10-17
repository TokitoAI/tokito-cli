#!/usr/bin/env python3
"""Verify old-writer package, inspect, CSV and SVG bytes; never regenerate goldens.

Python standard library only. All temporary packages and outputs stay under the
CLI checkout's target/compat-v0 directory. The bow-tie is a synthetic test case,
not a usable physical board or manufacturing/AR evidence.
"""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ET
from e2e_temp import assert_no_reparse_components


REPOSITORY = Path(__file__).resolve().parent.parent
GOLDEN_ROOT = REPOSITORY / "tests" / "fixtures" / "compat-v0"
SOURCE_ROOT = REPOSITORY / "tests" / "fixtures" / "open-source"
ARCHIVED_FILES = (
    "manifest.json",
    "derived/inspection.json",
    "derived/placements.csv",
    "derived/svg-top.svg",
    "derived/svg-bottom.svg",
    "source/board.kicad_pcb",
)
PACKAGE_FILES = ARCHIVED_FILES[:3]

# Byte snapshots include filenames and lengths, not only manifest-declared
# digests. Changing a golden and updating its manifest must still fail this gate.
CASES = (
    (
        "sparkfun-default-kicad",
        "SparkFun_Basic_Board_Example.kicad_pcb",
        "f6bfa8dd2a1f24cadca002b617584cd80ba109f6b58f3bf1d630e8ff328aab7d",
    ),
    (
        "raspberry-pi-pico-community",
        "RPI-PICO-R3a-PUBLIC - Official.kicad_pcb",
        "fdab7cbf96a1c8b8d52a10acae7f16f6c85310be3a2bc25b00ab037b6e36efa8",
    ),
    (
        "arduino-uno-r3-community",
        "Arduino UNO.kicad_pcb",
        "6197f0a8bf039c600e5386da7cf5abe6bad5550e5b4b4a2c1c21bc8f198a84bc",
    ),
    (
        "nxp-mcxa-esc",
        "MR-ESC-MCXA-AM32.kicad_pcb",
        "a10b81f7e8eed26ba9d44556a7efb10ea16e86192a34709bdd2511dccb8f4033",
    ),
    (
        "synthetic-bow-tie",
        None,
        "9b7b60702ecc91404da97a285cb940cd2c14e23cacda746c09b70b7f7faeee04",
    ),
)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def snapshot_digest(directory, includes_source, includes_svg):
    digest = hashlib.sha256()
    expected = set(PACKAGE_FILES)
    if includes_svg:
        expected.update(ARCHIVED_FILES[3:5])
    if includes_source:
        expected.add(ARCHIVED_FILES[-1])
    actual = {path.relative_to(directory).as_posix() for path in directory.rglob("*") if path.is_file()}
    if actual != expected:
        raise AssertionError(f"Archived file set changed in {directory.name}: {actual ^ expected}")
    for relative in ARCHIVED_FILES:
        if relative not in expected:
            continue
        data = (directory / relative).read_bytes()
        digest.update(relative.encode("utf-8") + b"\0" + str(len(data)).encode("ascii") + b"\0" + data)
    return digest.hexdigest()


def invoke(binary, *arguments, succeeds=True):
    result = subprocess.run(
        [str(binary), *map(str, arguments)], capture_output=True, timeout=60, check=False
    )
    if (result.returncode == 0) != succeeds:
        stderr = result.stderr.decode("utf-8", errors="replace")
        raise AssertionError(f"Unexpected CLI exit {result.returncode} for {arguments}: {stderr}")
    return result


def check_digest(path, expected, label):
    actual = "sha256:" + sha256(path.read_bytes())
    if actual != expected:
        raise AssertionError(f"{label} bytes changed: expected {expected}, got {actual}")


def reconstruct(name, filename, expected_snapshot, output):
    golden = GOLDEN_ROOT / name
    if snapshot_digest(golden, filename is None, filename is not None) != expected_snapshot:
        raise AssertionError(f"Historical golden bytes changed for {name}; do not regenerate them")
    manifest = json.loads((golden / "manifest.json").read_bytes())
    assert manifest["schemaVersion"] == "0.1.0"
    assert manifest["source"]["path"] == "source/board.kicad_pcb"
    assert manifest["artifacts"]["inspection"]["path"] == "derived/inspection.json"
    assert manifest["artifacts"]["placements"]["path"] == "derived/placements.csv"
    assert manifest["producer"] == {"name": "tokito-cli", "version": "0.1.0"}
    for artifact in ("inspection", "placements"):
        record = manifest["artifacts"][artifact]
        check_digest(golden / record["path"], record["sha256"], f"{name} archived {artifact}")
    source = golden / "source/board.kicad_pcb" if filename is None else SOURCE_ROOT / name / filename
    check_digest(source, manifest["source"]["sha256"], f"{name} original PCB source")
    assert source.stat().st_size == manifest["source"]["bytes"], name
    package = output / name
    package.mkdir()
    (package / "derived").mkdir()
    (package / "source").mkdir()
    for relative in PACKAGE_FILES:
        shutil.copyfile(golden / relative, package / relative)
    shutil.copyfile(source, package / "source/board.kicad_pcb")
    return package, golden, manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output-root", type=Path)
    arguments = parser.parse_args()
    binary = arguments.binary.resolve(strict=True)
    output_root = arguments.output_root or (REPOSITORY / "target" / "compat-v0")
    output_root = output_root.expanduser()
    if not output_root.is_absolute():
        raise ValueError("Compatibility output root must be an absolute path")
    assert_no_reparse_components(output_root)
    output_root.mkdir(parents=True, exist_ok=True)
    assert_no_reparse_components(output_root)
    output_root = output_root.resolve(strict=True)
    repo_root = REPOSITORY.resolve(strict=True)
    if arguments.output_root is None and not output_root.is_relative_to(repo_root):
        raise AssertionError("Compatibility output directory resolves outside the CLI checkout")
    with tempfile.TemporaryDirectory(prefix="historical-packages-", dir=output_root) as temporary:
        output = Path(temporary)
        for name, filename, snapshot in CASES:
            package, golden, manifest = reconstruct(name, filename, snapshot, output)
            verification = json.loads(invoke(binary, "package", "verify", package, "--json").stdout)
            assert verification["verified"] is True, name
            assert verification["schemaVersion"] == "0.1.0", name
            assert verification["sourceSha256"] == manifest["source"]["sha256"].removeprefix("sha256:"), name
            assert verification["mobileImportReady"] is False, name
            assert verification["publisherAuthenticated"] is False, name
            inspection = invoke(binary, "inspect", package, "--json").stdout
            assert inspection == (golden / "derived/inspection.json").read_bytes(), f"{name}: archived inspection changed"
            placements = output / f"{name}-placements.csv"
            invoke(binary, "export", "placements", package, "--output", placements)
            assert placements.read_bytes() == (golden / "derived/placements.csv").read_bytes(), f"{name}: archived placements changed"
            for side in ("top", "bottom"):
                svg = output / f"{name}-{side}.svg"
                if filename is None:
                    failure = invoke(binary, "export", "svg", package, "--side", side, "--output", svg, succeeds=False)
                    assert not svg.exists(), "Invalid bow-tie must not create an SVG"
                    assert failure.stderr, "Invalid topology must provide a diagnostic"
                else:
                    invoke(binary, "export", "svg", package, "--side", side, "--output", svg)
                    root = ET.parse(svg).getroot()
                    assert root.tag == "{http://www.w3.org/2000/svg}svg", name
                    assert root.attrib["data-side"] == side, name
                    assert root.attrib["data-completeness"] == "partial", name
                    assert root.attrib["data-units"] == "mm", name
                    assert svg.read_bytes() == (golden / f"derived/svg-{side}.svg").read_bytes(), f"{name}: archived {side} SVG bytes changed"
            # Tampering happens only to the temporary reconstruction, never a golden.
            inspection_path = package / "derived/inspection.json"
            inspection_path.write_bytes(inspection_path.read_bytes() + b"\n")
            invoke(binary, "package", "verify", package, succeeds=False)
            assert snapshot_digest(golden, filename is None, filename is not None) == snapshot
            print(f"PASS historical {name}: original hashes, exact inspect/CSV/top/bottom SVG bytes, package verification and tamper rejection")
    print("PASS historical v0.1 compatibility: four real-source packages and one synthetic invalid-topology package")


if __name__ == "__main__":
    main()
