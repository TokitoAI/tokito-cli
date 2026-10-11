#!/usr/bin/env python3
"""Exercise source-bound project bundle creation and verification end to end."""

import argparse
import csv
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ET


def invoke(binary: Path, *arguments: object, succeeds: bool = True, env=None):
    result = subprocess.run(
        [str(binary), *map(str, arguments)],
        capture_output=True,
        text=True,
        encoding="utf-8",
        env=env,
        check=False,
    )
    if succeeds and result.returncode != 0:
        raise AssertionError(f"CLI failed ({result.returncode}): {arguments}\n{result.stderr}")
    if not succeeds and result.returncode == 0:
        raise AssertionError(f"CLI unexpectedly accepted: {arguments}")
    return result


def read_glb(path: Path):
    data = path.read_bytes()
    assert len(data) >= 28 and data[:4] == b"glTF"
    version = int.from_bytes(data[4:8], "little")
    declared = int.from_bytes(data[8:12], "little")
    assert version == 2 and declared == len(data)
    json_length = int.from_bytes(data[12:16], "little")
    assert int.from_bytes(data[16:20], "little") == 0x4E4F534A
    document = json.loads(data[20 : 20 + json_length].decode("utf-8").rstrip(" \0"))
    return data, document


def test_post_copy_cleanup(binary: Path, project: Path, scratch: Path):
    fault_output = scratch / "fault-after-copy"
    fault_env = dict(os.environ, TOKITO_BUNDLE_TEST_FAIL_AFTER_SOURCE_COPY="1")
    fault = invoke(
        binary, "project", "build", project, "--board", "single-resistor.kicad_pcb",
        "--output", fault_output, "--allow-partial", succeeds=False, env=fault_env,
    )
    assert "injected failure after source copy" in fault.stderr
    assert not fault_output.exists()
    assert not list(scratch.glob(".fault-after-copy.tokito-*.stage"))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--fault-hook-only", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    root = Path(__file__).resolve().parent
    fixture = root / "fixtures/assembly-e2e/single-resistor.kicad_pcb"
    model_root = root / "fixtures/open-source/kicad-resistor-models"

    with tempfile.TemporaryDirectory(prefix="tokito-project-bundle-") as temporary:
        scratch = Path(temporary)
        project = scratch / "project"
        project.mkdir()
        source = project / "single-resistor.kicad_pcb"
        shutil.copyfile(fixture, source)
        source_digest = hashlib.sha256(source.read_bytes()).hexdigest()
        bundle = scratch / "bundle"

        if args.fault_hook_only:
            test_post_copy_cleanup(binary, project, scratch)
            print("project bundle post-copy cleanup E2E passed")
            return

        result = invoke(
            binary,
            "project", "build", project,
            "--output", bundle,
            "--expect-sha256", source_digest,
            "--assemble-wrl",
            "--library-root", model_root,
            "--json",
        )
        build_receipt = json.loads(result.stdout)
        manifest = json.loads((bundle / "manifest.json").read_text(encoding="utf-8"))
        assert manifest["schema"] == "tokito.project-export-bundle.v1"
        assert manifest["source"]["sha256"] == source_digest
        assert manifest["source"]["relativePath"] == "single-resistor.kicad_pcb"
        assert manifest["mobileImportReady"] is False
        assert manifest["appDigitalTwinManifestIncluded"] is False
        assert manifest["arRegistrationVerified"] is False
        assert build_receipt["source"]["sha256"] == source_digest

        source_copy = bundle / "source/board.kicad_pcb"
        assert hashlib.sha256(source_copy.read_bytes()).hexdigest() == source_digest
        assert source_copy.read_bytes() == source.read_bytes()
        inspection = json.loads((bundle / "derived/inspection.json").read_text(encoding="utf-8"))
        assert inspection["counts"]["footprints"] == 1
        placements = list(csv.DictReader(io.StringIO(
            (bundle / "derived/placements.csv").read_text(encoding="utf-8")
        )))
        assert placements[0]["reference"] == "R1"
        assert placements[0]["uuid"] == "assembly-r1"
        for filename, side in (("board.top.svg", "top"), ("board.bottom.svg", "bottom")):
            svg = ET.parse(bundle / "derived" / filename).getroot()
            assert svg.attrib["data-side"] == side
            assert svg.attrib["data-completeness"] == "partial"
        glb, gltf = read_glb(bundle / "derived/board.glb")
        assert gltf["asset"]["version"] == "2.0"
        assert "ref:R1" in [node.get("name") for node in gltf["nodes"]]
        component_map = json.loads(
            (bundle / "derived/component-map.json").read_text(encoding="utf-8")
        )
        component = next(row for row in component_map["components"] if row["reference"] == "R1")
        assert component["uuid"] == "assembly-r1"
        assert component["selector"] == "ref:R1"
        assert component["renderable"] is True
        coverage = json.loads(
            (bundle / "derived/glb-coverage.json").read_text(encoding="utf-8")
        )
        assert coverage["coverage"]["componentModelsEmitted"] == 1

        verification = json.loads(
            invoke(binary, "project", "verify", bundle, "--json").stdout
        )
        assert verification["status"] == "verified"
        assert verification["sourceSha256"] == source_digest
        assert verification["artifactCount"] >= 8

        sentinel = bundle / "keep.txt"
        sentinel.write_text("keep", encoding="utf-8")
        invoke(binary, "project", "build", project, "--output", bundle,
               "--assemble-wrl", "--library-root", model_root, succeeds=False)
        assert sentinel.read_text(encoding="utf-8") == "keep"
        sentinel.unlink()

        empty_existing = scratch / "existing-empty-output"
        empty_existing.mkdir()
        invoke(binary, "project", "build", project, "--output", empty_existing,
               "--allow-partial", succeeds=False)
        assert empty_existing.is_dir() and list(empty_existing.iterdir()) == []

        invoke(binary, "project", "build", project, "--output", scratch / "wrong-hash",
               "--expect-sha256", "0" * 64, "--assemble-wrl", "--library-root", model_root,
               succeeds=False)
        assert not (scratch / "wrong-hash").exists()

        denied = scratch / "missing-consent"
        invoke(binary, "project", "build", project, "--output", denied,
               succeeds=False)
        assert not denied.exists()

        competing = scratch / "competing-builds"
        competing_args = [
            str(binary), "project", "build", str(project), "--board",
            "single-resistor.kicad_pcb", "--output", str(competing), "--allow-partial",
        ]
        first = subprocess.Popen(competing_args, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 text=True, encoding="utf-8")
        second = subprocess.Popen(competing_args, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                  text=True, encoding="utf-8")
        first_stdout, first_stderr = first.communicate()
        second_stdout, second_stderr = second.communicate()
        results = [(first.returncode, first_stdout, first_stderr),
                   (second.returncode, second_stdout, second_stderr)]
        assert sum(result[0] == 0 for result in results) == 1, results
        assert sum(result[0] != 0 for result in results) == 1, results
        assert json.loads(invoke(binary, "project", "verify", competing, "--json").stdout)["status"] == "verified"
        assert not list(scratch.glob(".competing-builds.tokito-*.stage"))
        partial = scratch / "explicit-partial"
        partial_receipt = json.loads(invoke(
            binary, "project", "build", project, "--output", partial,
            "--allow-partial", "--json",
        ).stdout)
        assert partial_receipt["partialExport"] is True
        partial_glb, partial_gltf = read_glb(partial / "derived/board.glb")
        assert "ref:R1" not in [node.get("name") for node in partial_gltf["nodes"]]
        assert source.read_bytes() == source_copy.read_bytes()

        nested = project / "output"
        invoke(binary, "project", "build", project, "--output", nested,
               "--assemble-wrl", "--library-root", model_root, succeeds=False)
        assert not nested.exists()

        (project / "second.kicad_pcb").write_bytes(source.read_bytes())
        ambiguous = scratch / "ambiguous"
        invoke(binary, "project", "build", project, "--output", ambiguous,
               "--allow-partial", succeeds=False)
        assert not ambiguous.exists()
        selected = scratch / "selected"
        invoke(binary, "project", "build", project, "--board", "single-resistor.kicad_pcb",
               "--output", selected, "--allow-partial")

        bad_outline_project = scratch / "bad-outline"
        bad_outline_project.mkdir()
        bad_outline = bad_outline_project / "bad.kicad_pcb"
        bad_outline.write_text(
            source.read_text(encoding="utf-8").replace("Edge.Cuts", "Dwgs.User"),
            encoding="utf-8",
        )
        rejected = scratch / "bad-outline-output"
        invoke(binary, "project", "build", bad_outline_project, "--output", rejected,
               "--allow-partial", succeeds=False)
        assert not rejected.exists()

        source_board = project / "second.kicad_pcb"
        source_board.unlink()
        artifact_path = bundle / "derived/placements.csv"
        original_artifact = artifact_path.read_bytes()
        artifact_path.write_bytes(original_artifact + b"tamper")
        invoke(binary, "project", "verify", bundle, succeeds=False)
        artifact_path.write_bytes(original_artifact)
        invoke(binary, "project", "verify", bundle)

    print(
        "project export bundle E2E passed: exact source pin, top/bottom SVG, component selector, "
        "GLB coverage, manifest verification, overwrite refusal, explicit partial consent, "
        "concurrent no-overwrite publication, ambiguous selection, nested "
        "output refusal, invalid outline refusal and tamper detection"
    )


if __name__ == "__main__":
    main()
