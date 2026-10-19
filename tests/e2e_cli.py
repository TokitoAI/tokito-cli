#!/usr/bin/env python3
"""Exercise the built CLI through actual processes and parse its artifacts; standard library only."""
import argparse
import csv
import hashlib
import io
import json
import math
from pathlib import Path
import subprocess
import struct
import tempfile
import xml.etree.ElementTree as ET

FIXTURES = [
    ("sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb", 20, 29, 0, 0),
    ("raspberry-pi-pico-community/RPI-PICO-R3a-PUBLIC - Official.kicad_pcb", 57, 232, 546, 111),
    ("arduino-uno-r3-community/Arduino UNO.kicad_pcb", 56, 265, 831, 75),
    ("nxp-mcxa-esc/MR-ESC-MCXA-AM32.kicad_pcb", 97, 353, 491, 128),
]
NS = "{http://www.w3.org/2000/svg}"

def invoke(binary, *args, succeeds=True):
    result = subprocess.run([str(binary), *map(str, args)], capture_output=True, text=True, encoding="utf-8")
    if succeeds and result.returncode != 0:
        raise AssertionError(f"CLI failed: {args}: {result.stderr}")
    if not succeeds and result.returncode == 0:
        raise AssertionError(f"CLI unexpectedly accepted: {args}")
    return result

def svg_check(path, side, footprints):
    root = ET.parse(path).getroot()
    assert root.tag == NS + "svg"
    assert root.attrib["data-side"] == side
    assert root.attrib["data-completeness"] == "partial"
    assert root.attrib["data-units"] == "mm"
    assert root.attrib["width"].endswith("mm")
    frame = list(map(float, root.attrib["viewBox"].split()))
    assert len(frame) == 4 and frame[2] > 0 and frame[3] > 0
    assert abs(float(root.attrib["width"][:-2]) - frame[2]) < 1e-6
    assert abs(float(root.attrib["height"][:-2]) - frame[3]) < 1e-6
    group = root.find(NS + "g")
    assert group is not None
    assert ("transform" in group.attrib) == (side == "bottom")
    component_ids = {p["uuid"] for p in footprints}
    selected_components = [element.attrib["data-component-id"] for element in root.iter()
                           if "data-component-id" in element.attrib]
    assert selected_components and set(selected_components) <= component_ids
    return root


def test_svg_hide_dnp_fab(binary, source, output):
    inspection = json.loads(invoke(binary, "inspect", source, "--json").stdout)
    dnp_ids = {footprint["uuid"] for footprint in inspection["footprints"] if footprint["dnp"]}
    assert len(dnp_ids) == 2 and all(dnp_ids)

    default_path = output / "uno-fab-default.svg"
    hidden_path = output / "uno-fab-hide-dnp.svg"
    invoke(binary, "export", "svg", source, "--layers", "F.Fab", "--output", default_path)
    invoke(binary, "export", "svg", source, "--layers", "F.Fab", "--output", hidden_path,
           "--hide-DNP-footprints-on-fab-layers")
    default_root = ET.parse(default_path).getroot()
    hidden_root = ET.parse(hidden_path).getroot()

    def owned(root):
        by_component = {}
        for element in root.iter():
            component_id = element.attrib.get("data-component-id")
            if component_id:
                by_component.setdefault(component_id, []).append(
                    ET.tostring(element, encoding="unicode")
                )
        return {key: sorted(value) for key, value in by_component.items()}

    before = owned(default_root)
    after = owned(hidden_root)
    assert dnp_ids <= before.keys(), "default SVG should retain both DNP footprint drawings"
    assert not (dnp_ids & after.keys()), "opt-in DNP Fab export must omit DNP-owned elements"
    for component_id in before.keys() - dnp_ids:
        assert before[component_id] == after[component_id], "non-DNP Fab output changed"
    dnp_values = [element.text for element in default_root.iter(NS + "text")
                  if element.attrib.get("data-component-id") in dnp_ids]
    assert dnp_values.count("1M") == 2
    hidden_values = [element.text for element in hidden_root.iter(NS + "text")
                     if element.attrib.get("data-component-id") in dnp_ids]
    assert hidden_values == []

    silk_default = output / "uno-silkscreen-default.svg"
    silk_with_option = output / "uno-silkscreen-hide-dnp-option.svg"
    invoke(binary, "export", "svg", source, "--layers", "F.SilkS", "--output", silk_default)
    invoke(binary, "export", "svg", source, "--layers", "F.SilkS", "--output", silk_with_option,
           "--hide-DNP-footprints-on-fab-layers")
    assert silk_default.read_bytes() == silk_with_option.read_bytes(), "DNP Fab option affected a non-Fab layer"

    missing_uuid_source = output / "uno-dnp-without-uuid.kicad_pcb"
    source_text = source.read_text(encoding="utf-8")
    first_dnp_uuid = next(footprint["uuid"] for footprint in inspection["footprints"] if footprint["dnp"])
    source_text = source_text.replace(f'(uuid "{first_dnp_uuid}")', "", 1)
    missing_uuid_source.write_text(source_text, encoding="utf-8")
    refused_output = output / "uno-dnp-without-uuid.svg"
    refusal = invoke(binary, "export", "svg", missing_uuid_source, "--layers", "F.Fab",
                     "--output", refused_output, "--hide-DNP-footprints-on-fab-layers", succeeds=False)
    assert "no source UUID" in refusal.stderr and not refused_output.exists()

    populated_uuid = next(footprint["uuid"] for footprint in inspection["footprints"]
                          if not footprint["dnp"] and footprint["uuid"])
    ambiguous_source = output / "uno-dnp-ambiguous-uuid.kicad_pcb"
    ambiguous_text = source.read_text(encoding="utf-8").replace(
        f'(uuid "{populated_uuid}")', f'(uuid "{first_dnp_uuid}")', 1
    )
    ambiguous_source.write_text(ambiguous_text, encoding="utf-8")
    ambiguous_output = output / "uno-dnp-ambiguous-uuid.svg"
    ambiguous_refusal = invoke(
        binary, "export", "svg", ambiguous_source, "--layers", "F.Fab", "--output",
        ambiguous_output, "--hide-DNP-footprints-on-fab-layers", succeeds=False,
    )
    assert "footprint UUID is ambiguous" in ambiguous_refusal.stderr
    assert not ambiguous_output.exists(), "ambiguous DNP identity must never publish an SVG"

    duplicate_uuid_field_source = output / "uno-dnp-duplicate-uuid-field.kicad_pcb"
    duplicate_uuid_field_text = source.read_text(encoding="utf-8").replace(
        f'(uuid "{first_dnp_uuid}")',
        f'(uuid "{first_dnp_uuid}") (uuid "{populated_uuid}")',
        1,
    )
    assert duplicate_uuid_field_text != source.read_text(encoding="utf-8")
    duplicate_uuid_field_source.write_text(duplicate_uuid_field_text, encoding="utf-8")
    duplicate_uuid_field_output = output / "uno-dnp-duplicate-uuid-field.svg"
    duplicate_uuid_field = invoke(
        binary, "export", "svg", duplicate_uuid_field_source, "--layers", "F.Fab", "--output",
        duplicate_uuid_field_output, "--hide-DNP-footprints-on-fab-layers", succeeds=False,
    )
    assert "multiple source UUID fields" in duplicate_uuid_field.stderr
    assert not duplicate_uuid_field_output.exists(), "duplicate identity fields must never publish an SVG"

    malformed_uuid_field_source = output / "uno-dnp-malformed-uuid-field.kicad_pcb"
    original_source_text = source.read_text(encoding="utf-8")
    malformed_uuid_field_text = original_source_text.replace(
        f'(uuid "{first_dnp_uuid}")', f'(uuid "{first_dnp_uuid}" extra)', 1
    )
    assert malformed_uuid_field_text != original_source_text
    malformed_uuid_field_source.write_text(malformed_uuid_field_text, encoding="utf-8")
    malformed_uuid_field_output = output / "uno-dnp-malformed-uuid-field.svg"
    malformed_uuid_field = invoke(
        binary, "export", "svg", malformed_uuid_field_source, "--layers", "F.Fab", "--output",
        malformed_uuid_field_output, "--hide-DNP-footprints-on-fab-layers", succeeds=False,
    )
    assert "malformed source UUID field" in malformed_uuid_field.stderr
    assert not malformed_uuid_field_output.exists(), "malformed identity must never publish an SVG"

    duplicate_flag_output = output / "uno-dnp-duplicate-option.svg"
    duplicate_flag = invoke(binary, "export", "svg", source, "--layers", "F.Fab", "--output",
                            duplicate_flag_output, "--hide-DNP-footprints-on-fab-layers",
                            "--hide-DNP-footprints-on-fab-layers", succeeds=False)
    assert "unknown or incomplete SVG option" in duplicate_flag.stderr
    assert not duplicate_flag_output.exists()

def glb_check(path, expected_selectors=("ref:R1",)):
    data = path.read_bytes()
    assert len(data) >= 28 and data[:4] == b"glTF"
    version, total_length = struct.unpack_from("<II", data, 4)
    assert version == 2 and total_length == len(data)
    json_length, json_type = struct.unpack_from("<II", data, 12)
    assert json_type == 0x4E4F534A and 20 + json_length + 8 <= len(data)
    document = json.loads(data[20:20 + json_length].decode("utf-8").rstrip(" \0"))
    bin_header = 20 + json_length
    bin_length, bin_type = struct.unpack_from("<II", data, bin_header)
    assert bin_type == 0x004E4942 and bin_header + 8 + bin_length == len(data)
    binary = data[bin_header + 8:]
    declared_length = document["buffers"][0]["byteLength"]
    assert declared_length <= len(binary) and len(binary) - declared_length <= 3

    accessors = document["accessors"]
    views = document["bufferViews"]
    normal_accessor_indices = set()
    for mesh in document["meshes"]:
        for primitive in mesh["primitives"]:
            normal_accessor_indices.add(primitive["attributes"]["NORMAL"])
    for index, accessor in enumerate(accessors):
        assert accessor["componentType"] == 5126 and accessor["type"] == "VEC3"
        assert accessor["count"] > 0
        view = views[accessor["bufferView"]]
        start = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
        byte_count = accessor["count"] * 12
        assert view["buffer"] == 0 and start + byte_count <= view.get("byteOffset", 0) + view["byteLength"]
        assert start + byte_count <= declared_length
        values = list(struct.iter_unpack("<3f", binary[start:start + byte_count]))
        assert all(math.isfinite(value) for point in values for value in point)
        if accessor.get("min") is not None:
            for axis in range(3):
                actual_min = min(point[axis] for point in values)
                actual_max = max(point[axis] for point in values)
                assert abs(actual_min - accessor["min"][axis]) <= 1e-6
                assert abs(actual_max - accessor["max"][axis]) <= 1e-6
        if index in normal_accessor_indices:
            assert all(abs(math.sqrt(sum(value * value for value in point)) - 1.0) < 1e-4
                       for point in values)

    nodes = document["nodes"]
    roots = document["scenes"][document["scene"]]["nodes"]
    selector_indices = [index for index, node in enumerate(nodes)
                        if node.get("name", "").startswith("ref:") and node.get("mesh") is not None]
    selectors = [nodes[index]["name"] for index in selector_indices]
    if not expected_selectors:
        assert not selector_indices and len(roots) == 1
        return document
    assert selectors == list(expected_selectors)
    renderables = [nodes[index] for index in selector_indices]
    assert all(nodes.index(node) in roots for node in renderables)
    # Mirror the App's documented exact `componentNodePrefix + refdes` filter:
    # the named renderable itself must carry the mesh for selection to work.
    selected = {expected_selectors[0]}
    visible = [node["name"] for node in renderables
               if not node["name"].startswith("ref:") or node["name"] in selected]
    assert visible == [expected_selectors[0]]
    for node in renderables:
        expected_uuid = f"assembly-{node['name'].split(':', 1)[1].lower()}"
        assert node["extras"]["tokito"]["componentUuid"] == expected_uuid
        assert node["extras"]["tokito"]["selectorContract"] == "componentNodePrefix+refdes"
        assert node["extras"]["tokito"]["selectable"] is True
        primitives = document["meshes"][node["mesh"]]["primitives"]
        assert primitives
        for primitive in primitives:
            part_metadata = primitive["extras"]["tokito"]
            assert part_metadata["componentUuid"] == expected_uuid
            assert isinstance(part_metadata["modelSourceOrdinal"], int)
            assert isinstance(part_metadata["partOrdinal"], int)
            assert len(part_metadata["sourceAssetSha256"]) == 64
            assert all(character in "0123456789abcdef" for character in part_metadata["sourceAssetSha256"])
            assert part_metadata["selectable"] is False
        component_position_indices = {primitive["attributes"]["POSITION"] for primitive in primitives}
        component_positions = [
            point
            for accessor_index in component_position_indices
            for point in struct.iter_unpack(
                "<3f",
                binary[
                    views[accessors[accessor_index]["bufferView"]].get("byteOffset", 0)
                    + accessors[accessor_index].get("byteOffset", 0):
                    views[accessors[accessor_index]["bufferView"]].get("byteOffset", 0)
                    + accessors[accessor_index].get("byteOffset", 0)
                    + accessors[accessor_index]["count"] * 12
                ],
            )
        ]
        assert component_positions
        for axis in range(3):
            assert max(point[axis] for point in component_positions) > min(point[axis] for point in component_positions)
    return document

def component_triangle_normals(path, selector):
    data = path.read_bytes()
    json_length, = struct.unpack_from("<I", data, 12)
    document = json.loads(data[20:20 + json_length].decode("utf-8").rstrip(" \0"))
    bin_header = 20 + json_length
    bin_length, = struct.unpack_from("<I", data, bin_header)
    binary = data[bin_header + 8:bin_header + 8 + bin_length]
    node = next(node for node in document["nodes"] if node.get("name") == selector)
    mesh = document["meshes"][node["mesh"]]
    triangles = []
    for primitive in mesh["primitives"]:
        accessor = document["accessors"][primitive["attributes"]["NORMAL"]]
        view = document["bufferViews"][accessor["bufferView"]]
        start = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
        values = list(struct.iter_unpack(
            "<3f", binary[start:start + accessor["count"] * 12]))
        assert len(values) % 3 == 0
        triangles.extend([values[index:index + 3] for index in range(0, len(values), 3)])
    return triangles


def component_position_bounds(document, selector):
    node = next(node for node in document["nodes"]
                if node.get("name") == selector and node.get("mesh") is not None)
    mesh = document["meshes"][node["mesh"]]
    return [
        (
            document["accessors"][primitive["attributes"]["POSITION"]]["min"],
            document["accessors"][primitive["attributes"]["POSITION"]]["max"],
        )
        for primitive in mesh["primitives"]
    ]

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    test_root = Path(__file__).resolve().parent
    repo_root = test_root.parent
    fixture_root = test_root / "fixtures" / "open-source"
    reference_root = test_root / "reference" / "kicad-cli-10.0.5" / "pos"
    svg_reference_root = test_root / "reference" / "kicad-cli-10.0.5" / "svg"
    reference_manifest = json.loads((reference_root / "manifest.json").read_text(encoding="utf-8"))
    assert reference_manifest["reference"]["version"] == "10.0.5"
    for source in reference_manifest["sources"]:
        assert hashlib.sha256((repo_root / source["path"]).read_bytes()).hexdigest() == source["sha256"]
    for reference in reference_manifest["outputs"]:
        assert hashlib.sha256((reference_root / reference["path"]).read_bytes()).hexdigest() == reference["sha256"]
    svg_reference_manifest = json.loads((svg_reference_root / "manifest.json").read_text(encoding="utf-8"))
    assert svg_reference_manifest["reference"]["version"] == "10.0.5"
    svg_reference_source = repo_root / svg_reference_manifest["reference"]["source_path"]
    assert hashlib.sha256(svg_reference_source.read_bytes()).hexdigest() == svg_reference_manifest["reference"]["source_sha256"]
    for reference in svg_reference_manifest["outputs"]:
        reference_path = svg_reference_root / reference["path"]
        assert hashlib.sha256(reference_path.read_bytes()).hexdigest() == reference["sha256"]
        reference_root_svg = ET.parse(reference_path).getroot()
        assert sum(element.text == "1M" for element in reference_root_svg.iter(NS + "text")) == reference["text_1M_count"]
    svg_reference_text_counts = [entry["text_1M_count"] for entry in svg_reference_manifest["outputs"]]
    assert svg_reference_text_counts == [2, 0], "KiCad reference must hide both DNP Fab Value labels only when opted in"
    assert invoke(binary, "--version").stdout.strip() == "tokito-cli 0.1.0"
    assert invoke(binary, "-V").stdout.strip() == "tokito-cli 0.1.0"
    assert invoke(binary, "version").stdout.strip() == "tokito-cli 0.1.0"
    assert "validated board shell" in invoke(binary, "help", "export", "glb").stdout
    assert "--assemble-wrl" in invoke(binary, "help", "pcb", "export", "glb").stdout
    assert "--assemble-wrl" in invoke(binary, "pcb", "export", "glb", "--help").stdout
    assert "--assemble-wrl" in invoke(binary, "pcb", "export", "glb", "-h").stdout
    assert "Print the Tokito CLI version" in invoke(binary, "version", "--help").stdout
    assert "unknown command help topic" in invoke(binary, "help", "sch", succeeds=False).stderr
    root_help = invoke(binary, "--help").stdout
    pcb_help = invoke(binary, "pcb", "--help").stdout
    assert "COMMAND GROUPS:" in root_help and "tokito-cli pcb" in root_help
    assert "--version|-V" in root_help and "-h and --help" in root_help
    assert "Existing physical-board operations." in pcb_help
    pos_help = invoke(binary, "pcb", "export", "pos", "--help").stdout
    assert "--format csv|ascii" in pos_help
    assert "CSV or ASCII" in pos_help
    assert "Gerber and variants remain unsupported" in pos_help
    assert "--use-drill-file-origin" in pos_help
    assert "CSV is compared with pinned KiCad 10.0.5 and 10.0.7 cases" in pos_help
    assert "ASCII is compared with pinned KiCad 10.0.5 cases" in pos_help
    svg_help = invoke(binary, "help", "export", "svg").stdout
    assert "--hide-DNP-footprints-on-fab-layers" in svg_help
    with tempfile.TemporaryDirectory(prefix="tokito-cli-e2e-") as output:
        output = Path(output)
        uno = fixture_root / "arduino-uno-r3-community" / "Arduino UNO.kicad_pcb"
        nxp = fixture_root / "nxp-mcxa-esc" / "MR-ESC-MCXA-AM32.kicad_pcb"
        sparkfun = fixture_root / "sparkfun-default-kicad" / "SparkFun_Basic_Board_Example.kicad_pcb"
        test_svg_hide_dnp_fab(binary, uno, output)
        pos_cases = [
            (uno, "uno-mm-both.csv", ["--units", "mm"], 54),
            (uno, "uno-mm-smd.csv", ["--units", "mm", "--smd-only"], 44),
            (uno, "uno-mm-exclude-fp-th.csv", ["--units", "mm", "--exclude-fp-th"], 43),
            (uno, "uno-mm-exclude-dnp.csv", ["--units", "mm", "--exclude-dnp"], 52),
            (uno, "uno-in-both.csv", [], 54),
            (nxp, "nxp-mm-both.csv", ["--units", "mm"], 67),
            (nxp, "nxp-mm-front.csv", ["--units", "mm", "--side", "front"], 51),
            (nxp, "nxp-mm-back.csv", ["--units", "mm", "--side", "back"], 16),
            (nxp, "nxp-mm-bottom-negate.csv", ["--units", "mm", "--bottom-negate-x"], 67),
            (sparkfun, "spark-mm-both.csv", ["--units", "mm"], 17),
            (sparkfun, "spark-mm-exclude-fp-th.csv", ["--units", "mm", "--exclude-fp-th"], 13),
            (sparkfun, "spark-mm-drill-origin.csv", ["--units", "mm", "--use-drill-file-origin"], 17),
            (fixture_root / "raspberry-pi-pico-community" / "RPI-PICO-R3a-PUBLIC - Official.kicad_pcb", "pico-mm-drill-origin.csv", ["--units", "mm", "--use-drill-file-origin"], 57),
            (fixture_root / "raspberry-pi-pico-community" / "RPI-PICO-R3a-PUBLIC - Official.kicad_pcb", "pico-mm-drill-origin-bottom-negate.csv", ["--units", "mm", "--use-drill-file-origin", "--bottom-negate-x"], 57),
        ]
        for index, (source, reference_name, options, expected_rows) in enumerate(pos_cases):
            pos_path = output / f"pos-{index}.csv"
            invoke(binary, "pcb", "export", "pos", source, "--format", "csv", "--output", pos_path, *options)
            actual = pos_path.read_text(encoding="utf-8")
            reference = (reference_root / reference_name).read_text(encoding="utf-8").replace("\r\n", "\n")
            if reference_name.startswith("pico-"):
                actual_rows = list(csv.DictReader(io.StringIO(actual)))
                reference_rows = list(csv.DictReader(io.StringIO(reference)))
                assert [row for row in actual_rows if row["Ref"]] == [row for row in reference_rows if row["Ref"]], reference_name
                actual_empty_ref_rows = sorted(tuple(row.values()) for row in actual_rows if not row["Ref"])
                reference_empty_ref_rows = sorted(tuple(row.values()) for row in reference_rows if not row["Ref"])
                assert actual_empty_ref_rows == reference_empty_ref_rows, reference_name
            else:
                assert actual == reference, f"KiCad 10.0.5 POS mismatch: {reference_name}"
            assert len(list(csv.DictReader(io.StringIO(actual)))) == expected_rows
        uno_drill_fallback = output / "uno-drill-fallback.csv"
        invoke(
            binary, "pcb", "export", "pos", uno, "--format", "csv", "--units", "mm",
            "--output", uno_drill_fallback, "--use-drill-file-origin",
        )
        assert uno_drill_fallback.read_text(encoding="utf-8").replace("\r\n", "\n") == (
            reference_root / "uno-mm-both.csv"
        ).read_text(encoding="utf-8").replace("\r\n", "\n")

        sparkfun_text = sparkfun.read_text(encoding="utf-8")
        origin_declaration = "(aux_axis_origin 115.57 124.46)"
        assert origin_declaration in sparkfun_text
        malformed_origins = {
            "missing-coordinate": sparkfun_text.replace(origin_declaration, "(aux_axis_origin 115.57)", 1),
            "nonfinite-coordinate": sparkfun_text.replace(origin_declaration, "(aux_axis_origin nan 124.46)", 1),
            "duplicate": sparkfun_text.replace(
                origin_declaration,
                origin_declaration + "\n\t\t(aux_axis_origin 1 2)",
                1,
            ),
        }
        misplaced_origin = sparkfun_text
        misplaced_origin = misplaced_origin[:misplaced_origin.rfind(")")] + "\n\t(aux_axis_origin 1 2)\n" + misplaced_origin[misplaced_origin.rfind(")"):]
        malformed_origins["misplaced"] = misplaced_origin
        for name, malformed_source in malformed_origins.items():
            malformed_board = output / f"{name}-origin.kicad_pcb"
            malformed_board.write_text(malformed_source, encoding="utf-8")
            malformed_pos = output / f"{name}-origin.csv"
            malformed_result = invoke(
                binary, "pcb", "export", "pos", malformed_board, "--format", "csv", "--units", "mm",
                "--output", malformed_pos, "--use-drill-file-origin", succeeds=False,
            )
            assert "auxiliary" in malformed_result.stderr, f"{name}: {malformed_result.stderr}"
            assert not malformed_pos.exists(), f"refused origin wrote output: {name}"
        malformed_source = uno.read_text(encoding="utf-8")
        footprint_start = malformed_source.find("(footprint ")
        assert footprint_start >= 0
        layer_token = '(layer "F.Cu")'
        layer_start = malformed_source.find(layer_token, footprint_start)
        assert layer_start >= 0
        malformed_board = output / "unsupported-layer.kicad_pcb"
        malformed_board.write_text(
            malformed_source[:layer_start] + '(layer "In1.Cu")' + malformed_source[layer_start + len(layer_token):],
            encoding="utf-8",
        )
        malformed_pos = output / "unsupported-layer.csv"
        malformed_result = invoke(
            binary, "pcb", "export", "pos", malformed_board, "--format", "csv", "--output", malformed_pos,
            succeeds=False,
        )
        assert "unsupported copper side" in malformed_result.stderr and not malformed_pos.exists()
        missing_format_output = output / "missing-format.csv"
        missing_format = invoke(binary, "export", "pos", uno, "--output", missing_format_output, succeeds=False)
        assert "--format csv or ascii is required" in missing_format.stderr and not missing_format_output.exists()
        uno_ascii = output / "uno-mm-both.pos"
        ascii_result = invoke(
            binary, "pcb", "export", "pos", uno, "--format", "ascii", "--units", "mm",
            "--side", "both", "--output", uno_ascii,
        )
        ascii_text = uno_ascii.read_text(encoding="utf-8")
        assert "Wrote 54 ascii position rows" in ascii_result.stdout
        assert "### Printed by Tokito CLI version " in ascii_text
        assert "## Unit = mm, Angle = deg." in ascii_text
        assert ascii_text.endswith("## End\n")
        wrong_ascii_extension = output / "uno-ascii.csv"
        wrong_extension_result = invoke(
            binary, "export", "pos", uno, "--format", "ascii", "--output", wrong_ascii_extension,
            succeeds=False,
        )
        assert ".pos extension" in wrong_extension_result.stderr and not wrong_ascii_extension.exists()
        unknown_format_result = invoke(
            binary, "export", "pos", uno, "--format", "gerber", "--output", output / "unsupported.pos",
            succeeds=False,
        )
        assert "supports CSV and ASCII only" in unknown_format_result.stderr
        gerber_flag_result = invoke(
            binary, "export", "pos", uno, "--format", "ascii", "--output", output / "gerber-flag.pos",
            "--gerber-board-edge", succeeds=False,
        )
        assert "--gerber-board-edge applies only to Gerber output" in gerber_flag_result.stderr
        no_overwrite = output / "pos-no-overwrite.csv"
        no_overwrite.write_bytes(b"keep this file")
        invoke(binary, "export", "pos", uno, "--format", "csv", "--output", no_overwrite, succeeds=False)
        assert no_overwrite.read_bytes() == b"keep this file"
        no_overwrite_ascii = output / "pos-no-overwrite.pos"
        no_overwrite_ascii.write_bytes(b"keep this ASCII file")
        invoke(binary, "export", "pos", uno, "--format", "ascii", "--output", no_overwrite_ascii, succeeds=False)
        assert no_overwrite_ascii.read_bytes() == b"keep this ASCII file"
        no_overwrite_drill = output / "pos-no-overwrite-drill.csv"
        no_overwrite_drill.write_bytes(b"keep this drill-origin output")
        invoke(
            binary, "pcb", "export", "pos", sparkfun, "--format", "csv", "--units", "mm",
            "--output", no_overwrite_drill, "--use-drill-file-origin", succeeds=False,
        )
        assert no_overwrite_drill.read_bytes() == b"keep this drill-origin output"

        for index, (relative, footprints, pads, segments, vias) in enumerate(FIXTURES):
            source = fixture_root / relative
            inspection = json.loads(invoke(binary, "inspect", source, "--json").stdout)
            pcb_inspection = json.loads(invoke(binary, "pcb", "inspect", source, "--json").stdout)
            assert pcb_inspection == inspection
            counts = inspection["counts"]
            assert [counts[k] for k in ("footprints", "pads", "track_segments", "vias")] == [footprints, pads, segments, vias]
            assert inspection["geometry_complete"] is False
            csv_path = output / f"{index}.csv"
            invoke(binary, "export", "placements", source, "--output", csv_path)
            assert len(list(csv.DictReader(io.StringIO(csv_path.read_text(encoding="utf-8"))))) == footprints
            for side in ("top", "bottom"):
                svg = output / f"{index}-{side}.svg"
                invoke(binary, "export", "svg", source, "--side", side, "--output", svg)
                root = svg_check(svg, side, inspection["footprints"])
                repeated = output / f"{index}-{side}-repeat.svg"
                invoke(binary, "export", "svg", source, "--side", side, "--output", repeated)
                assert svg.read_bytes() == repeated.read_bytes(), "SVG must be deterministic"
                before = svg.read_bytes()
                invoke(binary, "export", "svg", source, "--side", side, "--output", svg, succeeds=False)
                assert svg.read_bytes() == before
                if index == 3 and side == "top":
                    assert "BA" in [text.text for text in root.iter(NS + "text")], "NXP custom silk field must render"
            package = output / f"package-{index}"
            invoke(binary, "package", "build", source, "--output", package)
            verified = json.loads(invoke(binary, "package", "verify", package, "--json").stdout)
            assert verified["verified"] and not verified["mobileImportReady"]
            assert not verified["publisherAuthenticated"]
            manifest = json.loads((package / "manifest.json").read_text(encoding="utf-8"))
            assert manifest["source"]["sha256"] == "sha256:" + hashlib.sha256(source.read_bytes()).hexdigest()
            native = json.loads(invoke(binary, "inspect", package, "--json").stdout)
            assert native["counts"] == counts and native["footprints"] == inspection["footprints"]
            if index == 3:
                packaged_pos = output / "nxp-package-pos.csv"
                invoke(binary, "pcb", "export", "pos", package, "--format", "csv", "--units", "mm", "--output", packaged_pos)
                assert packaged_pos.read_text(encoding="utf-8") == (reference_root / "nxp-mm-both.csv").read_text(encoding="utf-8").replace("\r\n", "\n")
            native_svg = output / f"{index}-package.svg"
            invoke(binary, "export", "svg", package, "--side", "top", "--output", native_svg)
            assert native_svg.read_bytes() == (output / f"{index}-top.svg").read_bytes()
            invoke(binary, "package", "build", source, "--output", package, succeeds=False)
            stored = package / "derived" / "inspection.json"
            forged = json.loads(stored.read_text(encoding="utf-8"))
            forged["counts"]["footprints"] = 999
            stored.write_text(json.dumps(forged), encoding="utf-8")
            manifest["artifacts"]["inspection"]["sha256"] = "sha256:" + hashlib.sha256(stored.read_bytes()).hexdigest()
            (package / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
            invoke(binary, "package", "verify", package, succeeds=False)
            print(f"PASS {relative}: inspect, raw placements, SVG, packages and geometry; position CSV references are checked separately")
        assembly_fixture = Path(__file__).resolve().parent / "fixtures" / "assembly-e2e" / "single-resistor.kicad_pcb"
        model_root = fixture_root / "kicad-resistor-models"
        assembly_glb = output / "single-resistor-assembly.glb"
        assembly_result = invoke(
            binary, "export", "glb", assembly_fixture,
            "--output", assembly_glb, "--assemble-wrl", "--project-root", model_root,
            "--json-report",
        )
        assembly_report = json.loads(assembly_result.stdout)
        assert assembly_report["scope"] == "board_shell_and_supported_vrml_models"
        assert not assembly_report["partialExport"]
        coverage = assembly_report["coverage"]
        assert coverage["componentModelsEmitted"] == 1 and coverage["modelReferencesEmitted"] == 1
        assert coverage["unresolvedModelsTotal"] == 0 and coverage["unresolvedModels"] == []
        assembly_document = glb_check(assembly_glb)
        assert "KHR_materials_specular" in assembly_document["extensionsUsed"]
        assert "KHR_materials_specular" not in assembly_document.get("extensionsRequired", [])
        authored_materials = [
            material for material in assembly_document["materials"]
            if material.get("extras", {}).get("tokito", {}).get("authoredMaterialPresent") is True
        ]
        assert len(authored_materials) == 3, "pinned resistor model should preserve all three authored VRML materials"
        for material in authored_materials:
            tokito = material["extras"]["tokito"]
            source_material = tokito["sourceVrmlMaterial"]
            exponent = source_material["shininess"] * 128.0
            expected_roughness = math.sqrt(2.0 / (exponent + 2.0))
            assert math.isclose(
                material["pbrMetallicRoughness"]["roughnessFactor"], expected_roughness,
                rel_tol=0.0, abs_tol=1e-12,
            ), "GLB roughness must preserve the bounded shininess projection"
            assert material["extensions"]["KHR_materials_specular"]["specularColorFactor"] == source_material["specularColor"]
            assert material["pbrMetallicRoughness"]["metallicFactor"] == 0.0
        assert assembly_document["asset"]["extras"]["tokito"]["coverage"]["componentModelsEmitted"] == 1
        emitted_normals = component_triangle_normals(assembly_glb, "ref:R1")
        assert any(
            len({tuple(round(axis, 6) for axis in normal) for normal in triangle}) > 1
            for triangle in emitted_normals
        ), "pinned KiCad resistor should retain source creaseAngle smoothing in the GLB"
        explicit_model_root = output / "explicit-normal-models"
        explicit_model_root.mkdir()
        (explicit_model_root / "explicit-normal.wrl").write_text(
            """#VRML V2.0 utf8
Shape { geometry IndexedFaceSet {
  coord DEF C Coordinate { point [0 0 0, 1 0 0, 0 1 0, 0 0 1] }
  coordIndex [0 1 2 -1]
  normalPerVertex TRUE
  normal DEF N Normal { vector [0 0 1, 0 1 0, 1 0 0, 0.577350269 0.577350269 0.577350269] }
  creaseAngle 0
} }
Shape { geometry IndexedFaceSet {
  coord USE C
  coordIndex [1 0 3 -1]
  normalPerVertex TRUE
  normal USE N
  creaseAngle 0
} }
""",
            encoding="utf-8",
        )
        explicit_board = output / "explicit-normal-board.kicad_pcb"
        explicit_board.write_text(
            assembly_fixture.read_text(encoding="utf-8").replace(
                "Resistor_SMD.3dshapes/R_0603_1608Metric.wrl", "explicit-normal.wrl"
            ),
            encoding="utf-8",
        )
        explicit_glb = output / "explicit-normal-assembly.glb"
        explicit_report = json.loads(invoke(
            binary, "export", "glb", explicit_board, "--output", explicit_glb,
            "--assemble-wrl", "--project-root", explicit_model_root, "--json-report",
        ).stdout)
        assert not explicit_report["partialExport"]
        explicit_document = glb_check(explicit_glb)
        explicit_normals = component_triangle_normals(explicit_glb, "ref:R1")
        assert len(explicit_normals) == 2, "Coordinate/Normal USE must expand both source shapes"
        assert any(
            len({tuple(round(axis, 6) for axis in normal) for normal in triangle}) > 1
            for triangle in explicit_normals
        ), "authored Coordinate/Normal DEF/USE values must remain distinct in GLB with creaseAngle 0"
        assert json.loads(invoke(
            binary, "export", "glb", assembly_fixture, "--output", output / "empty-filter.glb",
            "--assemble-wrl", "--project-root", model_root, "--component-filter", "",
            "--json-report",
        ).stdout)["coverage"]["componentFilter"] is None

        transformed_model_root = output / "transformed-vrml-models"
        transformed_model_root.mkdir()
        original_model_text = (model_root / "Resistor_SMD.3dshapes" / "R_0603_1608Metric.wrl").read_text(encoding="utf-8")
        _, original_model_body = original_model_text.split("\n", 1)
        (transformed_model_root / "translated.VRML").write_text(
            "#VRML V2.0 utf8\nTransform { translation 1 0 0 children [\n"
            + original_model_body
            + "\n] }\n",
            encoding="utf-8",
        )
        transformed_board = output / "transformed-vrml.kicad_pcb"
        transformed_board.write_text(
            assembly_fixture.read_text(encoding="utf-8").replace(
                "Resistor_SMD.3dshapes/R_0603_1608Metric.wrl", "translated.VRML"
            ),
            encoding="utf-8",
        )
        transformed_glb = output / "transformed-vrml.glb"
        transformed_report = json.loads(invoke(
            binary, "export", "glb", transformed_board, "--output", transformed_glb,
            "--assemble-wrl", "--project-root", transformed_model_root, "--json-report",
        ).stdout)
        assert not transformed_report["partialExport"]
        assert transformed_report["coverage"]["componentModelsEmitted"] == 1
        transformed_document = glb_check(transformed_glb)
        base_bounds = component_position_bounds(assembly_document, "ref:R1")
        moved_bounds = component_position_bounds(transformed_document, "ref:R1")
        assert len(base_bounds) == len(moved_bounds) > 0
        for (base_min, base_max), (moved_min, moved_max) in zip(base_bounds, moved_bounds):
            for axis, shift in enumerate((0.0, 0.0, 0.00254)):
                assert abs((moved_min[axis] - base_min[axis]) - shift) <= 1e-6
                assert abs((moved_max[axis] - base_max[axis]) - shift) <= 1e-6
        transformed_filtered_glb = output / "filtered-vrml-alias.glb"
        transformed_filtered = json.loads(invoke(
            binary, "export", "glb", transformed_board,
            "--output", transformed_filtered_glb, "--assemble-wrl",
            "--project-root", transformed_model_root, "--component-filter", "R1",
            "--json-report",
        ).stdout)
        assert not transformed_filtered["partialExport"]
        assert transformed_filtered["coverage"]["componentFilter"] == ["R1"]
        glb_check(transformed_filtered_glb, ("ref:R1",))

        oriented_model_root = output / "scale-oriented-vrml-models"
        oriented_model_root.mkdir()
        (oriented_model_root / "oriented-scale.wrl").write_text(
            "#VRML V2.0 utf8\nTransform { scale 1.5 0.75 1 "
            "scaleOrientation 0 0 1 0.7853981633974483 children [\n"
            + original_model_body
            + "\n] }\n",
            encoding="utf-8",
        )
        oriented_board = output / "scale-oriented-vrml.kicad_pcb"
        oriented_board.write_text(
            assembly_fixture.read_text(encoding="utf-8").replace(
                "Resistor_SMD.3dshapes/R_0603_1608Metric.wrl", "oriented-scale.wrl"
            ),
            encoding="utf-8",
        )
        oriented_glb = output / "scale-oriented-vrml.glb"
        oriented_report = json.loads(invoke(
            binary, "export", "glb", oriented_board, "--output", oriented_glb,
            "--assemble-wrl", "--project-root", oriented_model_root, "--json-report",
        ).stdout)
        assert not oriented_report["partialExport"]
        assert oriented_report["coverage"]["componentModelsEmitted"] == 1
        oriented_document = glb_check(oriented_glb, ("ref:R1",))
        assert len(component_position_bounds(oriented_document, "ref:R1")) > 0

        source_text = assembly_fixture.read_text(encoding="utf-8").rstrip()
        assert source_text.endswith(")")
        multi_source = output / "two-resistor-assembly.kicad_pcb"
        multi_source.write_text(
            source_text[:-1]
            + '\n  (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu") (at 7 7 0) (uuid "assembly-r2")\n'
            + '    (property "Reference" "R2")\n'
            + '    (model "Resistor_SMD.3dshapes/R_0603_1608Metric.wrl"))\n)',
            encoding="utf-8",
        )
        multi_glb = output / "two-resistor-assembly.glb"
        multi_report = json.loads(invoke(
            binary, "export", "glb", multi_source,
            "--output", multi_glb, "--assemble-wrl", "--project-root", model_root,
            "--json-report",
        ).stdout)
        assert multi_report["coverage"]["componentModelsEmitted"] == 2
        assert multi_report["coverage"]["modelReferencesEmitted"] == 2
        multi_document = glb_check(multi_glb, ("ref:R1", "ref:R2"))
        assert len(multi_document["scenes"][0]["nodes"]) == 3

        dnp_source = output / "two-resistor-dnp-assembly.kicad_pcb"
        dnp_source.write_text(
            source_text[:-1]
            + '\n  (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu") (at 7 7 0) (uuid "assembly-r2") (attr dnp)\n'
            + '    (property "Reference" "R2")\n'
            + '    (model "Resistor_SMD.3dshapes/R_0603_1608Metric.wrl"))\n)',
            encoding="utf-8",
        )
        dnp_default_glb = output / "two-resistor-dnp-default.glb"
        dnp_default_report = json.loads(invoke(
            binary, "export", "glb", dnp_source,
            "--output", dnp_default_glb, "--assemble-wrl", "--project-root", model_root,
            "--json-report",
        ).stdout)
        assert "dnpFilter" not in dnp_default_report["coverage"]
        dnp_default_document = glb_check(dnp_default_glb, ("ref:R1", "ref:R2"))
        assert len(dnp_default_document["scenes"][0]["nodes"]) == 3

        dnp_filtered_glb = output / "two-resistor-dnp-filtered.glb"
        dnp_filtered_report = json.loads(invoke(
            binary, "export", "glb", dnp_source,
            "--output", dnp_filtered_glb, "--assemble-wrl", "--project-root", model_root,
            "--no-dnp", "--json-report",
        ).stdout)
        dnp_coverage = dnp_filtered_report["coverage"]
        assert not dnp_filtered_report["partialExport"]
        assert dnp_coverage["componentCount"] == 2
        assert dnp_coverage["componentsIncluded"] == 1
        assert dnp_coverage["componentModelsEmitted"] == 1
        assert dnp_coverage["componentModelsUnresolved"] == 0
        assert dnp_coverage["modelReferences"] == 2
        assert dnp_coverage["modelReferencesEmitted"] == 1
        assert dnp_coverage["modelReferencesUnresolved"] == 0
        assert dnp_coverage["dnpFilter"] == {
            "enabled": True, "componentsExcluded": 1, "modelReferencesExcluded": 1,
        }
        dnp_filtered_document = glb_check(dnp_filtered_glb, ("ref:R1",))
        assert len(dnp_filtered_document["scenes"][0]["nodes"]) == 2
        assert dnp_filtered_document["asset"]["extras"]["tokito"]["coverage"]["dnpFilter"] == dnp_coverage["dnpFilter"]

        property_dnp_source = output / "two-resistor-property-dnp-assembly.kicad_pcb"
        property_dnp_source.write_text(
            dnp_source.read_text(encoding="utf-8").replace('(attr dnp)', '(property "DNP" "yes")'),
            encoding="utf-8",
        )
        property_dnp_glb = output / "two-resistor-property-dnp-preserved.glb"
        property_dnp_report = json.loads(invoke(
            binary, "export", "glb", property_dnp_source,
            "--output", property_dnp_glb, "--assemble-wrl", "--project-root", model_root,
            "--no-dnp", "--json-report",
        ).stdout)
        property_dnp_coverage = property_dnp_report["coverage"]
        assert property_dnp_coverage["componentsIncluded"] == 2
        assert property_dnp_coverage["componentModelsEmitted"] == 2
        assert property_dnp_coverage["dnpFilter"] == {
            "enabled": True, "componentsExcluded": 0, "modelReferencesExcluded": 0,
        }
        property_dnp_document = glb_check(property_dnp_glb, ("ref:R1", "ref:R2"))
        assert len(property_dnp_document["scenes"][0]["nodes"]) == 3

        shell_no_dnp_glb = output / "shell-no-dnp-unsupported.glb"
        no_dnp_shell_error = invoke(
            binary, "export", "glb", dnp_source,
            "--output", shell_no_dnp_glb, "--no-dnp", "--allow-partial", succeeds=False,
        )
        assert "require --assemble-wrl" in no_dnp_shell_error.stderr
        assert not shell_no_dnp_glb.exists()

        duplicate_attr_source = output / "two-resistor-duplicate-dnp-attributes.kicad_pcb"
        duplicate_attr_source.write_text(
            dnp_source.read_text(encoding="utf-8").replace(
                '(attr dnp)', '(attr smd) (attr dnp)', 1,
            ),
            encoding="utf-8",
        )
        duplicate_attr_glb = output / "two-resistor-duplicate-dnp-attributes.glb"
        duplicate_attr_report = json.loads(invoke(
            binary, "export", "glb", duplicate_attr_source,
            "--output", duplicate_attr_glb, "--assemble-wrl", "--project-root", model_root,
            "--no-dnp", "--json-report",
        ).stdout)
        assert duplicate_attr_report["coverage"]["dnpFilter"]["componentsExcluded"] == 1
        assert duplicate_attr_report["coverage"]["selectedComponentCount"] == 1
        glb_check(duplicate_attr_glb, ("ref:R1",))

        malformed_dnp_uuid_source = output / "two-resistor-dnp-without-uuid.kicad_pcb"
        malformed_dnp_uuid_source.write_text(
            dnp_source.read_text(encoding="utf-8").replace('(uuid "assembly-r2")', '', 1),
            encoding="utf-8",
        )
        malformed_dnp_uuid_glb = output / "two-resistor-dnp-without-uuid.glb"
        malformed_dnp_uuid_error = invoke(
            binary, "export", "glb", malformed_dnp_uuid_source,
            "--output", malformed_dnp_uuid_glb, "--assemble-wrl", "--project-root", model_root,
            "--no-dnp", "--allow-partial", succeeds=False,
        )
        assert "DNP footprint has a missing or ambiguous canonical UUID" in malformed_dnp_uuid_error.stderr
        assert not malformed_dnp_uuid_glb.exists()
        for pattern in ("R*", "R?"):
            wildcard_glb = output / f"wildcard-{pattern.replace('*', 'star').replace('?', 'any')}.glb"
            wildcard_report = json.loads(invoke(
                binary, "export", "glb", multi_source, "--output", wildcard_glb,
                "--assemble-wrl", "--project-root", model_root,
                "--component-filter", pattern, "--json-report",
            ).stdout)
            assert wildcard_report["coverage"]["componentFilter"] == [pattern]
            assert wildcard_report["coverage"]["componentModelsEmitted"] == 2
            assert not wildcard_report["partialExport"]
            glb_check(wildcard_glb, ("ref:R1", "ref:R2"))

        for unmatched_pattern in ("R404", "ZZ*", "r*"):
            no_match_glb = output / f"no-match-{unmatched_pattern.replace('*', 'star')}.glb"
            no_match_report = json.loads(invoke(
                binary, "export", "glb", multi_source, "--output", no_match_glb,
                "--assemble-wrl", "--project-root", model_root,
                "--component-filter", unmatched_pattern, "--json-report",
            ).stdout)
            assert no_match_report["coverage"]["componentFilter"] == [unmatched_pattern]
            assert no_match_report["coverage"]["componentCount"] == 2
            assert no_match_report["coverage"]["selectedComponentCount"] == 0
            assert no_match_report["coverage"]["filteredComponentCount"] == 2
            assert no_match_report["coverage"]["modelReferences"] == 2
            assert no_match_report["coverage"]["selectedModelReferences"] == 0
            assert no_match_report["coverage"]["filteredModelReferences"] == 2
            assert not no_match_report["partialExport"]
            glb_check(no_match_glb, expected_selectors=())

        for comma_patterns, expected_refs in [
            ("R1,R1", ("ref:R1",)),
            (" R1 ,", ("ref:R1",)),
            (",R2", ("ref:R2",)),
            ("R1,,R2", ("ref:R1", "ref:R2")),
            (",", ("ref:R1", "ref:R2")),
        ]:
            filtered_by_list_glb = output / f"comma-patterns-{len(comma_patterns)}-{ord(comma_patterns[0])}.glb"
            filtered_by_list = json.loads(invoke(
                binary, "export", "glb", multi_source,
                "--output", filtered_by_list_glb, "--assemble-wrl", "--project-root", model_root,
                "--component-filter", comma_patterns, "--json-report",
            ).stdout)
            if comma_patterns == ",":
                assert filtered_by_list["coverage"]["componentFilter"] is None
            assert not filtered_by_list["partialExport"]
            glb_check(filtered_by_list_glb, expected_refs)

        filtered_source = output / "filtered-two-resistor-assembly.kicad_pcb"
        step_asset_root = output / "step-model-assets"
        step_asset_root.mkdir()
        (step_asset_root / "selected-component.step").write_text(
            "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n",
            encoding="ascii",
        )
        filtered_source.write_text(
            source_text[:-1]
            + '\n  (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu") (at 7 7 0) (uuid "assembly-r2")\n'
            + '    (property "Reference" "R2")\n'
            + '    (model "selected-component.step"))\n)',
            encoding="utf-8",
        )
        filtered_glb = output / "filtered-r1.glb"
        filtered_report = json.loads(invoke(
            binary, "export", "glb", filtered_source, "--output", filtered_glb,
            "--assemble-wrl", "--project-root", model_root,
            "--library-root", step_asset_root, "--component-filter", "R1",
            "--json-report",
        ).stdout)
        filtered_coverage = filtered_report["coverage"]
        assert filtered_report["scope"] == "board_shell_and_selected_supported_vrml_models"
        assert not filtered_report["partialExport"]
        assert filtered_coverage["componentFilter"] == ["R1"]
        assert filtered_coverage["componentCount"] == 2
        assert filtered_coverage["selectedComponentCount"] == 1
        assert filtered_coverage["sourceComponentCount"] == 2
        assert filtered_coverage["filteredComponentCount"] == 1
        assert filtered_coverage["componentModelsEmitted"] == 1
        assert filtered_coverage["componentModelsUnresolved"] == 0
        assert filtered_coverage["modelReferences"] == 2
        assert filtered_coverage["selectedModelReferences"] == 1
        assert filtered_coverage["sourceModelReferences"] == 2
        assert filtered_coverage["filteredModelReferences"] == 1
        assert filtered_coverage["modelReferencesEmitted"] == 1
        assert filtered_coverage["modelReferencesUnresolved"] == 0
        filtered_document = glb_check(filtered_glb, ("ref:R1",))
        assert len(filtered_document["scenes"][0]["nodes"]) == 2

        selected_step_glb = output / "selected-step-model.glb"
        selected_step_refusal = invoke(
            binary, "export", "glb", filtered_source, "--output", selected_step_glb,
            "--assemble-wrl", "--project-root", model_root,
            "--library-root", step_asset_root, "--component-filter", "R2",
            "--json-report", succeeds=False,
        )
        assert "1 component(s) and 1 model reference(s)" in selected_step_refusal.stderr
        assert not selected_step_glb.exists()
        selected_step_report = json.loads(invoke(
            binary, "export", "glb", filtered_source, "--output", selected_step_glb,
            "--assemble-wrl", "--project-root", model_root,
            "--library-root", step_asset_root, "--component-filter", "R2",
            "--allow-partial", "--json-report",
        ).stdout)
        assert selected_step_report["partialExport"]
        assert selected_step_report["coverage"]["modelReferencesEmitted"] == 0
        assert selected_step_report["coverage"]["modelReferencesUnresolved"] == 1
        assert selected_step_report["coverage"]["unresolvedModels"][0]["status"] == "unsupported_asset_format"
        glb_check(selected_step_glb, expected_selectors=())

        dnp_source = output / "dnp-model-filter.kicad_pcb"
        dnp_source.write_text(
            filtered_source.read_text(encoding="utf-8")
            .replace('(property "Reference" "R2")', '(property "Reference" "R2")\n    (attr smd dnp)')
            .replace("selected-component.step", "missing-dnp-model.step"),
            encoding="utf-8",
        )
        dnp_filtered_glb = output / "dnp-filter.glb"
        dnp_filtered = json.loads(invoke(
            binary, "export", "glb", dnp_source, "--output", dnp_filtered_glb,
            "--assemble-wrl", "--project-root", model_root, "--library-root", step_asset_root,
            "--no-dnp", "--json-report",
        ).stdout)
        dnp_coverage = dnp_filtered["coverage"]
        assert not dnp_filtered["partialExport"]
        assert dnp_filtered["reportVersion"] == "1.2.0"
        assert dnp_coverage["noDnp"] is True
        assert dnp_coverage["componentFilter"] is None
        assert dnp_coverage["componentCount"] == 2
        assert dnp_coverage["selectedComponentCount"] == 1
        assert dnp_coverage["sourceComponentCount"] == 2
        assert dnp_coverage["filteredComponentCount"] == 1
        assert dnp_coverage["excludedDnpComponentCount"] == 1
        assert dnp_coverage["sourceModelReferences"] == 2
        assert dnp_coverage["filteredModelReferences"] == 1
        assert dnp_coverage["modelReferencesUnresolved"] == 0
        glb_check(dnp_filtered_glb, expected_selectors=("ref:R1",))

        custom_dnp_property_source = output / "custom-dnp-property-is-not-native.kicad_pcb"
        custom_dnp_property_source.write_text(
            filtered_source.read_text(encoding="utf-8")
            .replace('(property "Reference" "R2")', '(property "Reference" "R2")\n    (property "DNP" "yes")')
            .replace("selected-component.step", "custom-property-model.step"),
            encoding="utf-8",
        )
        custom_dnp_property_glb = output / "custom-dnp-property-is-not-native.glb"
        custom_dnp_property_report = json.loads(invoke(
            binary, "export", "glb", custom_dnp_property_source,
            "--output", custom_dnp_property_glb, "--assemble-wrl",
            "--project-root", model_root, "--library-root", step_asset_root,
            "--no-dnp", "--allow-partial", "--json-report",
        ).stdout)
        assert custom_dnp_property_report["coverage"]["excludedDnpComponentCount"] == 0
        assert custom_dnp_property_report["coverage"]["componentCount"] == 2
        assert custom_dnp_property_report["partialExport"]
        glb_check(custom_dnp_property_glb, expected_selectors=("ref:R1",))

        dnp_and_ref_filter_glb = output / "dnp-and-ref-filter.glb"
        dnp_and_ref_filter = json.loads(invoke(
            binary, "export", "glb", dnp_source, "--output", dnp_and_ref_filter_glb,
            "--assemble-wrl", "--project-root", model_root, "--library-root", step_asset_root,
            "--component-filter", "R*", "--no-dnp", "--json-report",
        ).stdout)
        assert dnp_and_ref_filter["coverage"]["componentFilter"] == ["R*"]
        assert dnp_and_ref_filter["coverage"]["excludedDnpComponentCount"] == 1
        assert not dnp_and_ref_filter["partialExport"]
        glb_check(dnp_and_ref_filter_glb, expected_selectors=("ref:R1",))

        unfiltered_dnp_glb = output / "unfiltered-dnp.glb"
        unfiltered_dnp = json.loads(invoke(
            binary, "export", "glb", dnp_source, "--output", unfiltered_dnp_glb,
            "--assemble-wrl", "--project-root", model_root, "--library-root", step_asset_root,
            "--allow-partial", "--json-report",
        ).stdout)
        assert unfiltered_dnp["partialExport"]
        assert unfiltered_dnp["coverage"]["noDnp"] is False
        assert unfiltered_dnp["coverage"]["componentCount"] == 2
        assert unfiltered_dnp["coverage"]["componentModelsUnresolved"] == 1
        assert unfiltered_dnp["coverage"]["unresolvedModels"][0]["status"] == "missing"

        oversized_pattern = "R" + ("?" * 1024)
        rejected_glb = output / "oversized-component-filter-pattern.glb"
        rejected = invoke(
            binary, "export", "glb", filtered_source, "--output", rejected_glb,
            "--assemble-wrl", "--project-root", model_root,
            "--component-filter", oversized_pattern, succeeds=False,
        )
        assert rejected.returncode != 0 and "pattern exceeds 1024 characters" in rejected.stderr
        assert not rejected_glb.exists()

        duplicate_flag_glb = output / "duplicate-component-filter-option.glb"
        duplicate_flag = invoke(
            binary, "export", "glb", filtered_source, "--output", duplicate_flag_glb,
            "--assemble-wrl", "--project-root", model_root,
            "--component-filter", "R1", "--component-filter", "R2", succeeds=False,
        )
        assert "unknown, duplicate, or incomplete GLB option" in duplicate_flag.stderr
        assert not duplicate_flag_glb.exists()

        shell_filter_glb = output / "shell-component-filter.glb"
        shell_filter = invoke(
            binary, "export", "glb", filtered_source, "--output", shell_filter_glb,
            "--component-filter", "R1", succeeds=False,
        )
        assert "require --assemble-wrl" in shell_filter.stderr
        assert not shell_filter_glb.exists()

        assignment_filter_glb = output / "assignment-component-filter.glb"
        assignment_filter = invoke(
            binary, "export", "glb", filtered_source, "--output", assignment_filter_glb,
            "--model-assignments", output / "missing-assignments.json",
            "--project-root", model_root, "--component-filter", "R1", succeeds=False,
        )
        assert "require --assemble-wrl" in assignment_filter.stderr
        assert not assignment_filter_glb.exists()

        duplicate_reference_source = output / "duplicate-source-reference.kicad_pcb"
        duplicate_reference_source.write_text(
            source_text[:-1]
            + '\n  (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu") (at 7 7 0) (uuid "assembly-r1-duplicate")\n'
            + '    (property "Reference" "R1")\n'
            + '    (model "Resistor_SMD.3dshapes/R_0603_1608Metric.wrl"))\n)',
            encoding="utf-8",
        )
        ambiguous_glb = output / "ambiguous-component-reference.glb"
        ambiguous = invoke(
            binary, "export", "glb", duplicate_reference_source,
            "--output", ambiguous_glb, "--assemble-wrl", "--project-root", model_root,
            "--component-filter", "R1", succeeds=False,
        )
        assert "assembled GLB has" in ambiguous.stderr
        assert not ambiguous_glb.exists()
        duplicate_partial_glb = output / "duplicate-component-reference-partial.glb"
        duplicate_partial = json.loads(invoke(
            binary, "export", "glb", duplicate_reference_source,
            "--output", duplicate_partial_glb, "--assemble-wrl", "--project-root", model_root,
            "--component-filter", "R1", "--allow-partial", "--json-report",
        ).stdout)
        assert duplicate_partial["partialExport"]
        assert duplicate_partial["coverage"]["componentCount"] == 2
        assert duplicate_partial["coverage"]["selectedComponentCount"] == 2
        assert duplicate_partial["coverage"]["componentModelsUnresolved"] == 2

        unmodeled_source = output / "selected-component-without-model.kicad_pcb"
        two_resistor_text = filtered_source.read_text(encoding="utf-8").rstrip()
        unmodeled_source.write_text(
            two_resistor_text[:-1]
            + '\n  (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu") (at 9 9 0) (uuid "assembly-r3")\n'
            + '    (property "Reference" "R3"))\n)',
            encoding="utf-8",
        )
        unmodeled_glb = output / "selected-component-without-model.glb"
        unmodeled_refusal = invoke(
            binary, "export", "glb", unmodeled_source, "--output", unmodeled_glb,
            "--assemble-wrl", "--project-root", model_root,
            "--component-filter", "R3", "--json-report", succeeds=False,
        )
        assert "1 component(s) and 0 model reference(s)" in unmodeled_refusal.stderr
        assert not unmodeled_glb.exists()
        unmodeled_report = json.loads(invoke(
            binary, "export", "glb", unmodeled_source, "--output", unmodeled_glb,
            "--assemble-wrl", "--project-root", model_root,
            "--component-filter", "R3", "--allow-partial", "--json-report",
        ).stdout)
        unmodeled_coverage = unmodeled_report["coverage"]
        assert unmodeled_report["partialExport"]
        assert unmodeled_coverage["componentCount"] == 3
        assert unmodeled_coverage["selectedComponentCount"] == 1
        assert unmodeled_coverage["componentModelsUnresolved"] == 1
        assert unmodeled_coverage["modelReferences"] == 2
        assert unmodeled_coverage["selectedModelReferences"] == 0
        assert unmodeled_coverage["sourceModelReferences"] == 2
        assert unmodeled_coverage["filteredModelReferences"] == 2
        glb_check(unmodeled_glb, expected_selectors=())

        unsupported_side_source = output / "unsupported-side.kicad_pcb"
        unsupported_side_source.write_text(
            assembly_fixture.read_text(encoding="utf-8").replace(
                '(layer "F.Cu") (at 5 5 90)',
                '(layer "F.SilkS") (at 5 5 90)',
                1,
            ),
            encoding="utf-8",
        )
        unsupported_side_glb = output / "unsupported-side.glb"
        unsupported_side_report = json.loads(invoke(
            binary, "export", "glb", unsupported_side_source,
            "--output", unsupported_side_glb, "--assemble-wrl", "--project-root", model_root,
            "--allow-partial", "--json-report",
        ).stdout)
        side_coverage = unsupported_side_report["coverage"]
        assert side_coverage["unresolvedModelsTotal"] == 1
        assert side_coverage["unresolvedModels"][0]["status"] == "unsupported_component_side"
        assert side_coverage["componentModelsUnresolved"] == 1
        glb_check(unsupported_side_glb, expected_selectors=())

        diagnostic_limit = 2048
        mass_source = output / "diagnostic-cap.kicad_pcb"
        board_prefix = assembly_fixture.read_text(encoding="utf-8").split("  (footprint ", 1)[0]
        mass_footprints = [
            f'(footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu") (at 5 5 0) '
            f'(uuid "mass-{index}") (property "Reference" "R{index}") '
            '(model "missing-model.wrl"))'
            for index in range(diagnostic_limit + 2)
        ]
        mass_source.write_text(board_prefix + "\n".join(mass_footprints) + "\n)\n", encoding="utf-8")
        mass_glb = output / "diagnostic-cap.glb"
        mass_report = json.loads(invoke(
            binary, "export", "glb", mass_source,
            "--output", mass_glb, "--assemble-wrl", "--project-root", model_root,
            "--allow-partial", "--json-report",
        ).stdout)
        mass_coverage = mass_report["coverage"]
        assert mass_coverage["componentModelsUnresolved"] == diagnostic_limit + 2
        assert mass_coverage["modelReferencesUnresolved"] == diagnostic_limit + 2
        assert mass_coverage["unresolvedModelsTotal"] == diagnostic_limit + 2
        assert len(mass_coverage["unresolvedModels"]) == diagnostic_limit
        assert mass_coverage["unresolvedModelsTruncated"] is True
        assert mass_coverage["unresolvedComponentsTotal"] == diagnostic_limit + 2
        assert mass_coverage["unresolvedComponentsTruncated"] is True
        glb_check(mass_glb, expected_selectors=())

        missing_source = output / "missing-model.kicad_pcb"
        missing_source.write_text(
            assembly_fixture.read_text(encoding="utf-8").replace(
                "R_0603_1608Metric.wrl", "missing-model.wrl"
            ),
            encoding="utf-8",
        )
        refused_glb = output / "missing-model-refused.glb"
        invoke(binary, "export", "glb", missing_source, "--output", refused_glb,
               "--assemble-wrl", "--project-root", model_root, succeeds=False)
        assert not refused_glb.exists(), "incomplete assembly must refuse without explicit partial consent"
        partial_glb = output / "missing-model-partial.glb"
        partial_report = json.loads(invoke(
            binary, "export", "glb", missing_source, "--output", partial_glb,
            "--assemble-wrl", "--project-root", model_root, "--allow-partial", "--json-report",
        ).stdout)
        assert partial_report["partialExport"]
        missing_coverage = partial_report["coverage"]["unresolvedModels"]
        assert len(missing_coverage) == 1 and missing_coverage[0]["status"] == "missing"
        assert partial_report["coverage"]["unresolvedComponents"][0]["reason"] == "component_model_geometry_not_emitted_by_assembly_export"
        glb_check(partial_glb, expected_selectors=())
        invoke(binary, "export", "svg", fixture_root / FIXTURES[0][0], "--output", output / "bad.svg",
               "--layers", "not-a-board-layer", succeeds=False)
    print("PASS CLI E2E journeys, including DNP Fab filtering, 14 hash-pinned KiCad 10.0.5 POS CSV comparisons, wildcard/edge-case component and DNP filters, STEP-skip accounting, drill-origin handling, package input, no overwrite and parsed VRML assembly GLB with generated crease-angle and explicit-normal fixtures; no STEP export or device/desktop CAD conformance claimed")

if __name__ == "__main__":
    main()
