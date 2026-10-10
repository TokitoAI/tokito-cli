#!/usr/bin/env python3
"""Exercise GLB export as a process and independently inspect its container/mesh."""
import argparse
from collections import defaultdict
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures/open-source"
PORCUPINE = FIXTURES / "porcupine-routed/porcupine_routed.kicad_pcb"
NFC = FIXTURES / "kicad-royalblue54l-nfc/RoyalBlue54L-NFC-Antenna.kicad_pcb"
JSON_CHUNK = 0x4E4F534A
BIN_CHUNK = 0x004E4942


def invoke(binary, *args, succeeds=True):
    result = subprocess.run([str(binary), *map(str, args)], capture_output=True,
                            text=True, encoding="utf-8")
    if succeeds and result.returncode != 0:
        raise AssertionError(f"CLI failed ({result.returncode}): {args}: {result.stderr}")
    if not succeeds and result.returncode == 0:
        raise AssertionError(f"CLI unexpectedly accepted: {args}")
    return result


def parse_glb(path):
    data = path.read_bytes()
    assert len(data) >= 28
    magic, version, declared_length = struct.unpack_from("<4sII", data, 0)
    assert magic == b"glTF" and version == 2 and declared_length == len(data)
    chunks = []
    offset = 12
    while offset < len(data):
        assert offset + 8 <= len(data)
        length, kind = struct.unpack_from("<II", data, offset)
        offset += 8
        assert length % 4 == 0 and offset + length <= len(data)
        chunks.append((kind, data[offset:offset + length]))
        offset += length
    assert offset == len(data)
    assert [kind for kind, _ in chunks] == [JSON_CHUNK, BIN_CHUNK]
    document = json.loads(chunks[0][1].decode("utf-8").rstrip(" \x00"))
    binary = chunks[1][1]
    assert len(document["buffers"]) == 1
    buffer = document["buffers"][0]
    assert "uri" not in buffer, "GLB must embed its only buffer"
    assert 0 < buffer["byteLength"] <= len(binary)
    return document, binary


def accessor_floats(document, binary, accessor_index):
    accessor = document["accessors"][accessor_index]
    assert accessor["componentType"] == 5126 and accessor["type"] == "VEC3"
    view = document["bufferViews"][accessor["bufferView"]]
    assert view.get("byteStride") is None
    start = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    count = accessor["count"] * 3
    end = start + count * 4
    assert end <= view.get("byteOffset", 0) + view["byteLength"] <= len(binary)
    return list(struct.unpack_from(f"<{count}f", binary, start))


def validate_board_shell(path, report):
    document, binary = parse_glb(path)
    assert document["asset"]["version"] == "2.0"
    extras = document["asset"]["extras"]["tokito"]
    assert extras["format"] == "tokito.board-shell.glb.v0"
    assert extras["scope"] == report["scope"] == "board_shell_only"
    assert extras["partialExport"] is True and report["partialExport"] is True
    assert extras["wholeBoardDigitalTwinComplete"] is False
    frame = extras["coordinateFrame"]
    assert frame["units"] == "m" and frame["handedness"] == "right-handed"
    assert frame["upAxis"] == "Y" and frame["boardPlane"] == "XZ"
    assert frame["sourceUnits"] == "mm" and frame["sourceCoordinateFrame"] == "KiCad-board-Y-down"
    assert len(document["meshes"]) == 1 and len(document["nodes"]) == 1
    primitive = document["meshes"][0]["primitives"][0]
    assert primitive["mode"] == 4 and "indices" not in primitive
    assert set(primitive["attributes"]) == {"POSITION", "NORMAL"}
    positions = accessor_floats(document, binary, primitive["attributes"]["POSITION"])
    normals = accessor_floats(document, binary, primitive["attributes"]["NORMAL"])
    assert len(positions) == len(normals) and len(positions) % 9 == 0
    assert all(math.isfinite(value) for value in positions + normals)
    vertex_count = len(positions) // 3
    assert vertex_count == document["accessors"][primitive["attributes"]["POSITION"]]["count"]
    accessor = document["accessors"][primitive["attributes"]["POSITION"]]
    actual_min = [min(positions[axis::3]) for axis in range(3)]
    actual_max = [max(positions[axis::3]) for axis in range(3)]
    assert all(abs(a - b) <= 2e-7 for a, b in zip(actual_min, accessor["min"]))
    assert all(abs(a - b) <= 2e-7 for a, b in zip(actual_max, accessor["max"]))
    half_thickness = report["mesh"]["thicknessMm"] / 2000.0
    assert abs(actual_min[1] + half_thickness) <= 2e-7
    assert abs(actual_max[1] - half_thickness) <= 2e-7
    assert abs(frame["boardTopYMetres"] - half_thickness) <= 1e-12
    assert abs(frame["boardBottomYMetres"] + half_thickness) <= 1e-12
    assert actual_min[0] < actual_max[0] and actual_min[2] < actual_max[2]

    top_area_m2 = 0.0
    directed_edges = defaultdict(lambda: [0, 0])
    for offset in range(0, len(positions), 9):
        p = [positions[offset + i:offset + i + 3] for i in (0, 3, 6)]
        n = normals[offset:offset + 3]
        norm_length = math.sqrt(sum(value * value for value in n))
        assert abs(norm_length - 1.0) <= 2e-6
        a = [p[1][i] - p[0][i] for i in range(3)]
        b = [p[2][i] - p[0][i] for i in range(3)]
        cross = [a[1] * b[2] - a[2] * b[1],
                 a[2] * b[0] - a[0] * b[2],
                 a[0] * b[1] - a[1] * b[0]]
        dot = sum(cross[i] * n[i] for i in range(3))
        area = math.sqrt(sum(value * value for value in cross)) / 2.0
        assert area > 0 and dot > 0
        if n[1] > 0.99:
            top_area_m2 += area
        for start, finish in ((p[0], p[1]), (p[1], p[2]), (p[2], p[0])):
            a = tuple(0.0 if value == 0.0 else value for value in start)
            b = tuple(0.0 if value == 0.0 else value for value in finish)
            assert a != b, "f32 quantization collapsed a mesh edge"
            low, high = sorted((a, b))
            directed_edges[(low, high)][0 if a == low else 1] += 1
    assert directed_edges, "shell has no edges"
    assert all(counts == [1, 1] for counts in directed_edges.values()), (
        "shell must be closed with oppositely wound paired edges"
    )
    assert vertex_count // 3 == report["mesh"]["triangles"]
    assert abs(top_area_m2 * 1_000_000 - report["mesh"]["triangulatedOutlineAreaMm2"]) <= max(
        1e-5, report["mesh"]["triangulatedOutlineAreaMm2"] * 1e-6)
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    binary = parser.parse_args().binary.resolve(strict=True)
    assert PORCUPINE.is_file() and NFC.is_file()
    with tempfile.TemporaryDirectory(prefix="tokito-glb-e2e-") as temp:
        output = Path(temp)
        refused = output / "default-refused.glb"
        refusal = invoke(binary, "export", "glb", PORCUPINE, "--output", refused, succeeds=False)
        assert "allow_partial=true" in refusal.stderr and not refused.exists()

        board_glb = output / "porcupine.glb"
        exported = invoke(binary, "export", "glb", PORCUPINE, "--output", board_glb,
                          "--allow-partial", "--json-report")
        report = json.loads(exported.stdout)
        assert report["shellGeometryComplete"] is True
        assert report["wholeBoardDigitalTwinComplete"] is False
        assert report["coverage"]["componentModelsEmitted"] == 0
        assert report["coverage"]["componentModelsUnresolved"] == report["coverage"]["componentCount"] > 0
        assert report["coverage"]["modelReferencesEmitted"] == 0
        assert report["coverage"]["modelReferencesUnresolved"] == report["coverage"]["modelReferences"]
        coverage = report["coverage"]
        unresolved = coverage["unresolvedComponents"]
        assert coverage["unresolvedComponentsTotal"] == coverage["componentCount"]
        assert coverage["unresolvedComponentsTruncated"] is False
        assert len(unresolved) == coverage["unresolvedComponentsTotal"]
        assert len({component["uuid"] for component in unresolved}) == len(unresolved)
        assert all(component["uuid"] and component["reason"] for component in unresolved)
        assert all(isinstance(component["modelPaths"], list) for component in unresolved)
        assert any(component["reference"] for component in unresolved)
        first_hash = validate_board_shell(board_glb, report)

        repeated = output / "repeat.glb"
        repeated_report = json.loads(invoke(binary, "export", "glb", PORCUPINE,
                                             "--output", repeated, "--allow-partial",
                                             "--json-report").stdout)
        assert validate_board_shell(repeated, repeated_report) == first_hash

        before = board_glb.read_bytes()
        invoke(binary, "export", "glb", PORCUPINE, "--output", board_glb,
               "--allow-partial", succeeds=False)
        assert board_glb.read_bytes() == before, "existing output must not be overwritten"

        package = output / "porcupine-package"
        invoke(binary, "package", "build", PORCUPINE, "--output", package)
        package_glb = output / "package.glb"
        package_report = json.loads(invoke(binary, "export", "glb", package,
                                            "--output", package_glb, "--allow-partial",
                                            "--json-report").stdout)
        assert validate_board_shell(package_glb, package_report) == first_hash

        invalid = output / "invalid-outline.glb"
        invoke(binary, "export", "glb", NFC, "--output", invalid,
               "--allow-partial", succeeds=False)
        assert not invalid.exists()
    print("PASS GLB E2E: explicit partial gate, unresolved identity report, independent GLB/mesh checks, determinism, package input, no overwrite, invalid outline refusal")
    print("PASS GLB scope remains board-shell-only; component geometry and device/AR acceptance are not claimed")


if __name__ == "__main__":
    main()
