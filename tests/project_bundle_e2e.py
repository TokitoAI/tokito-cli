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


def write_glb_document(path: Path, original: bytes, document):
    old_json_length = int.from_bytes(original[12:16], "little")
    binary_chunk = original[20 + old_json_length :]
    json_chunk = json.dumps(document, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    json_chunk += b" " * (-len(json_chunk) % 4)
    total_length = 12 + 8 + len(json_chunk) + len(binary_chunk)
    rebuilt = (
        original[:8]
        + total_length.to_bytes(4, "little")
        + len(json_chunk).to_bytes(4, "little")
        + (0x4E4F534A).to_bytes(4, "little")
        + json_chunk
        + binary_chunk
    )
    assert len(rebuilt) == total_length
    path.write_bytes(rebuilt)


def refresh_artifact_digest(bundle: Path, relative_path: str):
    manifest_path = bundle / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    artifact = next(row for row in manifest["artifacts"] if row["path"] == relative_path)
    contents = (bundle / relative_path).read_bytes()
    artifact["bytes"] = len(contents)
    artifact["sha256"] = hashlib.sha256(contents).hexdigest()
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def refresh_source_inspection(binary: Path, bundle: Path, source_text: str):
    source_path = bundle / "source/board.kicad_pcb"
    source_path.write_text(source_text, encoding="utf-8")
    inspection = json.loads(invoke(binary, "inspect", source_path, "--json").stdout)
    manifest_path = bundle / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    inspection["source"] = manifest["source"]["relativePath"]
    (bundle / "derived/inspection.json").write_text(
        json.dumps(inspection, indent=2) + "\n", encoding="utf-8"
    )
    refresh_artifact_digest(bundle, "derived/inspection.json")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    source_bytes = source_path.read_bytes()
    manifest["source"]["bytes"] = len(source_bytes)
    manifest["source"]["sha256"] = hashlib.sha256(source_bytes).hexdigest()
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def active_scene(document):
    scene_index = document.get("scene")
    if scene_index is None:
        assert len(document["scenes"]) == 1
        scene_index = 0
    return document["scenes"][scene_index]


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

        # Embedded inspection identities must be an exact projection of the
        # reimported source PCB, even when bundle hashes are recomputed.
        mismatched_inspection_bundle = scratch / "mismatched-inspection"
        shutil.copytree(bundle, mismatched_inspection_bundle)
        mismatched_inspection_path = mismatched_inspection_bundle / "derived/inspection.json"
        mismatched_inspection = json.loads(mismatched_inspection_path.read_text(encoding="utf-8"))
        mismatched_inspection["footprints"][0]["reference"] = "R9"
        mismatched_inspection_path.write_text(
            json.dumps(mismatched_inspection, indent=2) + "\n", encoding="utf-8"
        )
        refresh_artifact_digest(mismatched_inspection_bundle, "derived/inspection.json")
        mismatched_inspection_result = invoke(
            binary, "project", "verify", mismatched_inspection_bundle, succeeds=False
        )
        assert "embedded inspection does not match the reimported source PCB" in mismatched_inspection_result.stderr

        # A selector is rejected when its source reference is duplicated, even
        # when the inspection and every recorded bundle digest agree.
        duplicate_reference_bundle = scratch / "duplicate-source-reference"
        shutil.copytree(bundle, duplicate_reference_bundle)
        duplicate_reference_source = (duplicate_reference_bundle / "source/board.kicad_pcb").read_text(encoding="utf-8")
        duplicate_reference_footprint = (
            '  (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu") '
            '(at 7 5 0) (uuid "assembly-r2") (property "Reference" "R1"))'
        )
        refresh_source_inspection(
            binary,
            duplicate_reference_bundle,
            duplicate_reference_source.rstrip()[:-1] + "\n" + duplicate_reference_footprint + "\n)\n",
        )
        duplicate_reference_result = invoke(
            binary, "project", "verify", duplicate_reference_bundle, succeeds=False
        )
        assert "does not uniquely match a source footprint" in duplicate_reference_result.stderr

        # A unique selector ref is also insufficient if its source UUID is
        # duplicated by a different footprint.
        duplicate_uuid_bundle = scratch / "duplicate-source-uuid"
        shutil.copytree(bundle, duplicate_uuid_bundle)
        duplicate_uuid_source = (duplicate_uuid_bundle / "source/board.kicad_pcb").read_text(encoding="utf-8")
        duplicate_uuid_footprint = (
            '  (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu") '
            '(at 7 5 0) (uuid "assembly-r1") (property "Reference" "R2"))'
        )
        refresh_source_inspection(
            binary,
            duplicate_uuid_bundle,
            duplicate_uuid_source.rstrip()[:-1] + "\n" + duplicate_uuid_footprint + "\n)\n",
        )
        duplicate_uuid_result = invoke(
            binary, "project", "verify", duplicate_uuid_bundle, succeeds=False
        )
        assert "does not uniquely match a source footprint identity" in duplicate_uuid_result.stderr

        # A selector may own a deep ordinary-node subtree. The one-pass walk
        # must handle it without recursive traversal or repeated subtree scans.
        deep_bundle = scratch / "deep-selector-subtree"
        shutil.copytree(bundle, deep_bundle)
        deep_glb_path = deep_bundle / "derived/board.glb"
        deep_glb, deep_document = read_glb(deep_glb_path)
        selector_index = next(
            index for index, node in enumerate(deep_document["nodes"])
            if node.get("name") == "ref:R1"
        )
        assert deep_document["meshes"]
        chain_length = 4096
        first_chain_index = len(deep_document["nodes"])
        for offset in range(chain_length):
            node = {"children": [first_chain_index + offset + 1]} if offset + 1 < chain_length else {"mesh": 0}
            deep_document["nodes"].append(node)
        selector_node = deep_document["nodes"][selector_index]
        selector_node.setdefault("children", []).append(first_chain_index)
        write_glb_document(deep_glb_path, deep_glb, deep_document)
        refresh_artifact_digest(deep_bundle, "derived/board.glb")
        deep_verification = json.loads(
            invoke(binary, "project", "verify", deep_bundle, "--json").stdout
        )
        assert deep_verification["status"] == "verified"

        # A selector in the active scene must identify exactly one non-placeholder
        # source footprint. Orphan GLB names cannot silently disappear from the map.
        orphan_bundle = scratch / "orphan-selector"
        shutil.copytree(bundle, orphan_bundle)
        orphan_glb_path = orphan_bundle / "derived/board.glb"
        orphan_glb, orphan_document = read_glb(orphan_glb_path)
        orphan_index = len(orphan_document["nodes"])
        orphan_document["nodes"].append({"name": "ref:ORPHAN"})
        active_scene(orphan_document).setdefault("nodes", []).append(orphan_index)
        write_glb_document(orphan_glb_path, orphan_glb, orphan_document)
        refresh_artifact_digest(orphan_bundle, "derived/board.glb")
        orphan_result = invoke(
            binary, "project", "verify", orphan_bundle, succeeds=False
        )
        assert "does not uniquely match a source footprint" in orphan_result.stderr

        # Root and child reference arrays are bounded by the unique node count,
        # before malformed repeated indexes can expand the traversal stack.
        oversized_roots_bundle = scratch / "oversized-scene-roots"
        shutil.copytree(bundle, oversized_roots_bundle)
        oversized_roots_path = oversized_roots_bundle / "derived/board.glb"
        oversized_roots_glb, oversized_roots_document = read_glb(oversized_roots_path)
        oversized_roots_selector = next(
            index for index, node in enumerate(oversized_roots_document["nodes"])
            if node.get("name") == "ref:R1"
        )
        oversized_roots_document["scenes"][0]["nodes"] = [oversized_roots_selector] * max(
            len(oversized_roots_document["nodes"]) + 1, 100_000
        )
        write_glb_document(oversized_roots_path, oversized_roots_glb, oversized_roots_document)
        refresh_artifact_digest(oversized_roots_bundle, "derived/board.glb")
        oversized_roots_result = invoke(
            binary, "project", "verify", oversized_roots_bundle, succeeds=False
        )
        assert "root references exceed the node-count budget" in oversized_roots_result.stderr

        oversized_edges_bundle = scratch / "oversized-scene-edges"
        shutil.copytree(bundle, oversized_edges_bundle)
        oversized_edges_path = oversized_edges_bundle / "derived/board.glb"
        oversized_edges_glb, oversized_edges_document = read_glb(oversized_edges_path)
        oversized_edges_selector = next(
            index for index, node in enumerate(oversized_edges_document["nodes"])
            if node.get("name") == "ref:R1"
        )
        oversized_edges_document["scenes"][0]["nodes"] = [oversized_edges_selector]
        oversized_edges_document["nodes"][oversized_edges_selector]["children"] = [
            oversized_edges_selector
        ] * max(len(oversized_edges_document["nodes"]), 100_000)
        write_glb_document(oversized_edges_path, oversized_edges_glb, oversized_edges_document)
        refresh_artifact_digest(oversized_edges_bundle, "derived/board.glb")
        oversized_edges_result = invoke(
            binary, "project", "verify", oversized_edges_bundle, succeeds=False
        )
        assert "node references exceed the node-count budget" in oversized_edges_result.stderr

        # A long chain of distinct nested selectors is rejected on encountering
        # the first nested owner rather than expanding every selector subtree.
        nested_bundle = scratch / "nested-selectors"
        shutil.copytree(bundle, nested_bundle)
        nested_glb_path = nested_bundle / "derived/board.glb"
        nested_glb, nested_document = read_glb(nested_glb_path)
        nested_selector_index = next(
            index for index, node in enumerate(nested_document["nodes"])
            if node.get("name") == "ref:R1"
        )
        original_node_count = len(nested_document["nodes"])
        outer_selector_index = nested_selector_index
        for offset in range(2048):
            wrapper_index = len(nested_document["nodes"])
            nested_document["nodes"].append(
                {"name": f"ref:ADVERSARY{offset}", "children": [outer_selector_index]}
            )
            outer_selector_index = wrapper_index
        replaced_links = 0
        roots = active_scene(nested_document).setdefault("nodes", [])
        for index, root_index in enumerate(roots):
            if root_index == nested_selector_index:
                roots[index] = outer_selector_index
                replaced_links += 1
        for node in nested_document["nodes"][:original_node_count]:
            for index, child_index in enumerate(node.get("children", [])):
                if child_index == nested_selector_index:
                    node["children"][index] = outer_selector_index
                    replaced_links += 1
        assert replaced_links == 1
        write_glb_document(nested_glb_path, nested_glb, nested_document)
        refresh_artifact_digest(nested_bundle, "derived/board.glb")
        nested_result = invoke(
            binary, "project", "verify", nested_bundle, succeeds=False
        )
        assert "nested GLB component selectors are unsupported" in nested_result.stderr

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
        "output refusal, invalid outline refusal, deep selector traversal, orphan/nested selector "
        "rejection, bounded scene-reference handling and tamper detection"
    )


if __name__ == "__main__":
    main()
