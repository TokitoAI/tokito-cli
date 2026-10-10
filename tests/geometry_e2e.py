#!/usr/bin/env python3
"""Actual CLI topology checks and an independent, pinned Gerber outline oracle.

This is deliberately a narrow reader of the committed KiCad reference export,
not a production Gerber importer or a claim of complete CAD equivalence.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import subprocess
import tempfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures/open-source"
NS = "{http://www.w3.org/2000/svg}"
TAU = math.tau


def run(binary, *args, valid=True):
    result = subprocess.run([str(binary), *map(str, args)], capture_output=True,
                            text=True, encoding="utf-8")
    assert result.returncode == (0 if valid else 2), (args, result.returncode, result.stderr)
    return result.stdout


def circle(start, mid, end):
    # Independent calculation from three points in a local frame.
    ax, ay = mid[0] - start[0], mid[1] - start[1]
    bx, by = end[0] - start[0], end[1] - start[1]
    det = 2 * (ax * by - ay * bx)
    aa, bb = ax * ax + ay * ay, bx * bx + by * by
    return (start[0] + (aa * by - bb * ay) / det,
            start[1] + (ax * bb - bx * aa) / det)


def gerber(path, *, expected_entities=None, expected_arcs=None, expected_circles=0):
    content = path.read_text(encoding="utf-8")
    assert "%FSLAX46Y46*%" in content and "%MOMM*%" in content
    assert "%TF.SameCoordinates,Original*%" in content
    mode, current, entities, ended = "G01", None, [], False
    allowed_meta = ("%TF.", "%TA.", "%TD*%", "%TO.", "%FSLAX46Y46*%",
                    "%MOMM*%", "%LPD*%")
    for line in content.splitlines():
        if (not line or line.startswith("G04") or line.startswith(allowed_meta)
                or line.startswith("%ADD")):
            continue
        if line in ("G01*", "G02*", "G03*"):
            mode = line[:-1]
            continue
        if re.fullmatch(r"D\d+\*", line) or line == "G75*":
            continue
        if line == "M02*":
            ended = True
            continue
        assert not ended, "data after Gerber end"
        match = re.fullmatch(r"X(-?\d+)Y(-?\d+)(?:I(-?\d+)J(-?\d+))?D0([12])\*", line)
        assert match, f"Unreviewed reference command: {line}"
        x, y, i, j, operation = match.groups()
        # Gerber is Y-up; normalize to the saved board Y-down frame.
        end = (int(x) / 1e6, -int(y) / 1e6)
        if operation == "1":
            assert current is not None
            if mode == "G01":
                assert i is None
                entities.append({"kind": "line", "start": current, "end": end})
            else:
                assert i is not None and j is not None
                center = (current[0] + int(i) / 1e6, current[1] - int(j) / 1e6)
                if end == current:
                    radius = math.dist(current, center)
                    assert radius > 0, "full-circle Gerber arc has a zero radius"
                    entities.append({"kind": "circle", "center": center, "radiusMm": radius})
                else:
                    entities.append({"kind": "arc", "start": current, "end": end,
                                     "center": center, "positive": mode == "G02"})
        current = end
    assert ended
    if expected_entities is not None:
        assert len(entities) == expected_entities
    if expected_arcs is not None:
        assert sum(e["kind"] == "arc" for e in entities) == expected_arcs
    assert sum(e["kind"] == "circle" for e in entities) == expected_circles
    return entities


def compare_reference(report, reference, *, expected_valid, expected_entities,
                      expected_arcs, expected_circles=0):
    assert len(report["primitives"]) == len(reference) == expected_entities
    assert sum(p["shape"]["kind"] == "arc" for p in report["primitives"]) == expected_arcs
    assert sum(p["shape"]["kind"] == "circle" for p in report["primitives"]) == expected_circles
    pending = list(reference)
    max_endpoint, max_center = 0.0, 0.0
    for primitive in report["primitives"]:
        shape = primitive["shape"]
        if shape["kind"] == "circle":
            matches = []
            for index, entry in enumerate(pending):
                if entry["kind"] != "circle":
                    continue
                center_error = math.dist(shape["center"], entry["center"])
                radius_error = abs(shape["radiusMm"] - entry["radiusMm"])
                if center_error <= 5e-6 and radius_error <= 5e-6:
                    matches.append((index, center_error))
            assert len(matches) == 1, "Each sourced circle must match exactly one full-circle reference arc"
            index, center_error = matches[0]
            pending.pop(index)
            max_center = max(max_center, center_error)
            continue
        assert shape["kind"] in ("line", "arc")
        matches = []
        for index, entry in enumerate(pending):
            if entry["kind"] != shape["kind"]:
                continue
            forward = max(math.dist(shape["start"], entry["start"]), math.dist(shape["end"], entry["end"]))
            reverse = max(math.dist(shape["start"], entry["end"]), math.dist(shape["end"], entry["start"]))
            error = min(forward, reverse)
            if error <= 1e-6:
                matches.append((index, error))
        assert len(matches) == 1, "Each sourced primitive must match exactly one exported reference entity"
        index, error = matches[0]
        entry = pending.pop(index)
        max_endpoint = max(max_endpoint, error)
        if shape["kind"] == "arc":
            center = shape["circle"]["center"]
            assert math.dist(center, circle(shape["start"], shape["mid"], shape["end"])) <= 1e-9
            center_error = math.dist(center, entry["center"])
            max_center = max(max_center, center_error)
            assert center_error <= 5e-6, "Gerber IJ rounding tolerance exceeded"
            assert abs(shape["circle"]["radiusMm"] - math.dist(entry["center"], entry["start"])) <= 5e-6
            angle = lambda p: math.atan2(p[1] - entry["center"][1], p[0] - entry["center"][0])
            first, finish, middle = angle(entry["start"]), angle(entry["end"]), angle(shape["mid"])
            direction = 1 if entry["positive"] else -1
            span = (direction * (finish - first)) % TAU
            assert (direction * (middle - first)) % TAU <= span + 5e-5, "Midpoint/sweep differs from exported arc"
    assert not pending
    assert bool(report["valid"]) is expected_valid
    if expected_valid:
        assert report["bounds"] is not None
    else:
        assert report["bounds"] is None
    return {"entities": len(reference), "arcEntities": expected_arcs,
            "circleEntities": expected_circles,
            "maxEndpointErrorMm": max_endpoint, "maxCenterErrorMm": max_center,
            "endpointToleranceMm": 1e-6, "centerAndRadiusToleranceMm": 5e-6}


def compare_svg_reference(root, reference):
    """Check emitted source-frame paths against Gerber, not canonical geometry.

    Deliberately limited to this stored profile's line/circular-arc/circle
    objects; unsupported SVG path syntax fails instead of being skipped.
    """
    pending = list(reference)
    edges = [e for e in root.iter() if e.attrib.get("data-layer") == "Edge.Cuts"]
    assert len(edges) == len(reference)
    maximum = 0.0
    for element in edges:
        if element.tag == NS + "circle":
            center = (float(element.attrib["cx"]), float(element.attrib["cy"]))
            radius = float(element.attrib["r"])
            assert radius > 0 and all(map(math.isfinite, (*center, radius)))
            matches = [(i, max(math.dist(center, r["center"]), abs(radius-r["radiusMm"])))
                       for i, r in enumerate(pending) if r["kind"] == "circle"]
        else:
            assert element.tag == NS + "path"
            values = element.attrib["d"].split()
            assert values[0] == "M"
            start = (float(values[1]), float(values[2]))
            assert values[3] in ("L", "A")
            is_arc = values[3] == "A"
            assert len(values) == (11 if is_arc else 6)
            end = (float(values[-2]), float(values[-1]))
            assert all(map(math.isfinite, (*start, *end)))
            if is_arc:
                rx, ry, rotation = map(float, values[4:7])
                large, sweep = map(int, values[7:9])
                assert rx > 0 and rx == ry and rotation == 0
                assert large in (0, 1) and sweep in (0, 1)
                chord = math.dist(start, end)
                assert chord > 0 and chord <= 2*rx + 1e-9
                # SVG endpoint-to-centre conversion for an unrotated circle.
                coefficient = math.sqrt(max(0.0, rx*rx/(chord*chord)-0.25))
                if large == sweep:
                    coefficient = -coefficient
                center = ((start[0]+end[0])/2 - (end[1]-start[1])*coefficient,
                          (start[1]+end[1])/2 + (end[0]-start[0])*coefficient)
            matches = []
            for i, entry in enumerate(pending):
                if entry["kind"] != ("arc" if is_arc else "line"):
                    continue
                forward = max(math.dist(start, entry["start"]), math.dist(end, entry["end"]))
                reverse = max(math.dist(start, entry["end"]), math.dist(end, entry["start"]))
                error = min(forward, reverse)
                if is_arc:
                    error = max(error, math.dist(center, entry["center"]),
                                abs(rx-math.dist(entry["start"], entry["center"])))
                    expected_direction = entry["positive"] if forward <= reverse else not entry["positive"]
                    if bool(sweep) != expected_direction:
                        continue
                matches.append((i, error))
        matches = [(i, error) for i, error in matches if error <= 5e-6]
        assert len(matches) == 1, "Emitted SVG geometry/sweep must match one stored Gerber entity"
        index, error = matches[0]
        maximum = max(maximum, error)
        pending.pop(index)
    assert not pending
    return {"entities": len(edges), "maxSvgGeometryErrorMm": maximum,
            "toleranceMm": 5e-6, "coverage": "path/circle geometry, not raster rendering"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    binary = parser.parse_args().binary.resolve(strict=True)
    output_parent = ROOT / "target/geometry-e2e"
    output_parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=output_parent) as temporary:
        output = Path(temporary)
        nfc = FIXTURES / "kicad-royalblue54l-nfc/RoyalBlue54L-NFC-Antenna.kicad_pcb"
        reference = nfc.with_name("RoyalBlue54L-NFC-Antenna-EdgeCuts.gbr")
        assert hashlib.sha256(reference.read_bytes()).hexdigest() == "154cb7652150e636eb3ae1ed7e1975197fbe3e379822190eba89ae848d38d3f2"
        report = json.loads(run(binary, "validate", "outline", nfc, "--json", valid=False))
        assert not report["valid"] and report["units"] == "mm"
        assert any(d["code"] == "OpenContour" for d in report["diagnostics"])
        comparison = compare_reference(
            report, gerber(reference, expected_entities=22, expected_arcs=12),
            expected_valid=False, expected_entities=22, expected_arcs=12)
        rp2040 = FIXTURES / "rp2040-game-con/rp2040_game_con.kicad_pcb"
        rp2040_profile = rp2040.with_name("rp2040_game_con-Edge_Cuts.gm1")
        assert hashlib.sha256(rp2040.read_bytes()).hexdigest() == "359ab533c810a2e5ee4881d7578e6e1d33beb55c8003f3a4dad792b6f00d23bd"
        assert hashlib.sha256(rp2040_profile.read_bytes()).hexdigest() == "b823eb715f230855a4d86945391a17ccd822e00bce498a09381d1cac91808e7f"
        rp2040_report = json.loads(run(binary, "validate", "outline", rp2040, "--json"))
        assert rp2040_report["sourceSha256"] == "359ab533c810a2e5ee4881d7578e6e1d33beb55c8003f3a4dad792b6f00d23bd"
        assert len(rp2040_report["primitives"]) == 644
        assert sorted(c["role"] for c in rp2040_report["contours"]) == ["cutout"] * 4 + ["outer"]
        rp2040_comparison = compare_reference(
            rp2040_report, gerber(rp2040_profile, expected_entities=644, expected_arcs=0),
            expected_valid=True, expected_entities=644, expected_arcs=0)
        lumenpnp = FIXTURES / "lumenpnp-vac-interposer/vac-adapter_profile.kicad_pcb"
        lumenpnp_profile = lumenpnp.with_name("vac-adapter-Edge_Cuts.gbr")
        assert hashlib.sha256(lumenpnp.read_bytes()).hexdigest() == "dfa1ab6a4facd19542629b2a16c3296a3e6910fe1f624055dce15ce11bb8c174"
        assert hashlib.sha256(lumenpnp_profile.read_bytes()).hexdigest() == "698d1cad11241bf7f225e8fa9a631fbbc7f5a74b995151b5ad328e52ad2785e1"
        lumenpnp_report = json.loads(run(binary, "validate", "outline", lumenpnp, "--json"))
        assert lumenpnp_report["valid"] and len(lumenpnp_report["primitives"]) == 8
        assert sorted(c["role"] for c in lumenpnp_report["contours"]) == ["cutout", "cutout", "outer"]
        lumenpnp_ids = {p["source"]["uuid"] for p in lumenpnp_report["primitives"]}
        assert lumenpnp_ids == {
            "173757fd-c4d7-4804-add6-ce7c15e60c10",
            "41dfc15b-3765-4d10-88e8-72c043d00ea1",
            "59b706ba-da59-4f96-8360-f8e5bcc11efb",
            "9578fb6a-c825-470c-8a7a-229030955fe4",
            "cd4ef0b5-2dc4-4b63-94eb-71625dcda556",
            "d7d4f837-0db1-4f3e-b556-d65b7de7e620",
            "15cef0c4-75f7-4294-bc4e-e717462bc021",
            "fcb4c6b1-4e0b-49ae-a4be-e4853ff3b06b",
        }
        footprint_id = json.loads(run(binary, "inspect", lumenpnp, "--json"))["footprints"][0]["uuid"]
        assert footprint_id == "aa130053-a451-4f12-97f7-3d4d891a5f83"
        expected_fp_circles = {
            "15cef0c4-75f7-4294-bc4e-e717462bc021": ((121.0, 74.4), 0.5),
            "fcb4c6b1-4e0b-49ae-a4be-e4853ff3b06b": ((127.0, 80.4), 0.5),
        }
        fp_circles = [p for p in lumenpnp_report["primitives"]
                      if p["source"]["uuid"] in expected_fp_circles]
        assert len(fp_circles) == 2
        for primitive in fp_circles:
            source = primitive["source"]
            shape = primitive["shape"]
            center, radius = expected_fp_circles[source["uuid"]]
            assert source["kind"] == "fp_circle" and source["componentOrdinal"] is not None
            assert math.dist(shape["center"], center) <= 1e-9
            assert abs(shape["radiusMm"] - radius) <= 1e-9
        lumenpnp_comparison = compare_reference(
            lumenpnp_report,
            gerber(lumenpnp_profile, expected_entities=8, expected_arcs=2, expected_circles=2),
            expected_valid=True, expected_entities=8, expected_arcs=2, expected_circles=2)
        lumenpnp_svg = output / "lumenpnp-profile.svg"
        run(binary, "export", "svg", lumenpnp, "--output", lumenpnp_svg)
        lumenpnp_root = ET.parse(lumenpnp_svg).getroot()
        lumenpnp_edges = [e for e in lumenpnp_root.iter()
                          if e.attrib.get("data-layer") == "Edge.Cuts"]
        assert {e.attrib.get("data-id") for e in lumenpnp_edges} == lumenpnp_ids
        lumenpnp_svg_circles = {e.attrib["data-id"]: e for e in lumenpnp_edges
                                if e.tag == NS + "circle"}
        assert set(lumenpnp_svg_circles) == set(expected_fp_circles)
        for source_id, (center, radius) in expected_fp_circles.items():
            element = lumenpnp_svg_circles[source_id]
            assert element.attrib["data-component-id"] == footprint_id
            assert max(abs(float(element.attrib["cx"]) - center[0]),
                       abs(float(element.attrib["cy"]) - center[1]),
                       abs(float(element.attrib["r"]) - radius)) <= 5.1e-7
        original = FIXTURES / "lumenpnp-vac-interposer-original/vac-adapter.kicad_pcb"
        original_profile = original.with_name("vac-adapter-Edge_Cuts.gbr")
        assert hashlib.sha256(original.read_bytes()).hexdigest() == "2b21f974c74f9521b45717f17e7f6f67489f7a47316db0f9eb2697f234edcce3"
        assert hashlib.sha256(original_profile.read_bytes()).hexdigest() == "698d1cad11241bf7f225e8fa9a631fbbc7f5a74b995151b5ad328e52ad2785e1"
        original_report = json.loads(run(binary, "validate", "outline", original, "--json"))
        original_inventory = json.loads(run(binary, "inspect", original, "--json"))
        assert len(original_inventory["footprints"]) == 7
        original_reference = gerber(original_profile, expected_entities=8, expected_arcs=2, expected_circles=2)
        original_comparison = compare_reference(
            original_report, original_reference, expected_valid=True,
            expected_entities=8, expected_arcs=2, expected_circles=2)
        assert sorted(c["role"] for c in original_report["contours"]) == ["cutout", "cutout", "outer"]
        original_svg_comparisons = {}
        for side in ("top", "bottom"):
            original_svg = output / f"original-lumenpnp-{side}.svg"
            run(binary, "export", "svg", original, "--side", side, "--output", original_svg)
            original_root = ET.parse(original_svg).getroot()
            assert ("transform" in original_root.find(NS + "g").attrib) == (side == "bottom")
            original_svg_comparisons[side] = compare_svg_reference(original_root, original_reference)
            original_ids = {p["source"]["uuid"] for p in original_report["primitives"]}
            edges = [e for e in original_root.iter() if e.attrib.get("data-layer") == "Edge.Cuts"]
            assert {e.attrib.get("data-id") for e in edges} == original_ids
            circles = [e for e in edges if e.tag == NS + "circle"]
            assert {e.attrib.get("data-component-id") for e in circles} == {footprint_id}
            # Prove the external comparison detects an actual wrong radius or
            # sweep rather than merely checking the source endpoint inventory.
            for field, bad_value in ((4, "2.000000"), (8, "0")):
                mutated = ET.fromstring(ET.tostring(original_root))
                arc = next(e for e in mutated.iter()
                           if e.attrib.get("data-layer") == "Edge.Cuts"
                           and " A " in e.attrib.get("d", ""))
                values = arc.attrib["d"].split()
                values[field] = bad_value
                if field == 4:
                    values[5] = bad_value
                arc.attrib["d"] = " ".join(values)
                try:
                    compare_svg_reference(mutated, original_reference)
                except AssertionError:
                    pass
                else:
                    raise AssertionError("External SVG reference comparison accepted a wrong radius/sweep")
        refused = output / "refused.svg"
        run(binary, "export", "svg", nfc, "--output", refused, valid=False)
        assert not refused.exists()
        package = output / "nfc-inventory"
        run(binary, "package", "build", nfc, "--output", package)
        verified = json.loads(run(binary, "package", "verify", package, "--json"))
        assert verified["verified"] and not verified["mobileImportReady"]
        packaged = json.loads(run(binary, "validate", "outline", package, "--json", valid=False))
        assert packaged == report
        porcupine = FIXTURES / "porcupine-routed/porcupine_routed.kicad_pcb"
        board = json.loads(run(binary, "validate", "outline", porcupine, "--json"))
        inspection = json.loads(run(binary, "inspect", porcupine, "--json"))
        assert board["valid"] and len(board["primitives"]) == 37
        assert sorted(c["role"] for c in board["contours"]) == ["cutout"] * 5 + ["outer"]
        assert inspection["counts"]["track_arcs"] == 607
        component_ids = {f["uuid"] for f in inspection["footprints"]}
        outline_ids = {p["source"]["uuid"] for p in board["primitives"]}
        for side in ("top", "bottom"):
            svg = output / f"porcupine-{side}.svg"
            run(binary, "export", "svg", porcupine, "--side", side, "--output", svg)
            root = ET.parse(svg).getroot()
            elements = list(root.iter())
            assert root.attrib["data-side"] == side and root.attrib["data-completeness"] == "partial"
            assert ("transform" in root.find(NS + "g").attrib) == (side == "bottom")
            edge_elements = [e for e in elements if e.attrib.get("data-layer") == "Edge.Cuts"]
            assert len(edge_elements) == 37 and {e.attrib.get("data-id") for e in edge_elements} == outline_ids
            assert sum(e.tag == NS + "circle" for e in edge_elements) == 5
            bounds = board["bounds"]
            view_x, view_y, view_w, view_h = map(float, root.attrib["viewBox"].split())
            assert (view_x <= bounds["minX"] and view_y <= bounds["minY"]
                    and view_x + view_w >= bounds["maxX"]
                    and view_y + view_h >= bounds["maxY"]), "SVG viewport clips canonical physical outline bounds"
            by_id = {e.attrib["data-id"]: e for e in edge_elements if "data-id" in e.attrib}
            arcs = [p for p in board["primitives"] if p["shape"]["kind"] == "arc"]
            circles = [p for p in board["primitives"] if p["shape"]["kind"] == "circle"]
            assert len(arcs) == 17 and len(circles) == 5
            for primitive in arcs:
                shape = primitive["shape"]
                source_id = primitive["source"]["uuid"]
                assert source_id and source_id in by_id, "physical SVG arc identity missing"
                element = by_id[source_id]
                assert element.tag == NS + "path"
                match = re.fullmatch(r"M (-?[0-9.]+) (-?[0-9.]+) A ([0-9.]+) ([0-9.]+) 0 ([01]) ([01]) (-?[0-9.]+) (-?[0-9.]+)", element.attrib["d"])
                assert match, f"unexpected SVG arc path: {element.attrib['d']}"
                sx, sy, rx, ry, large_flag, sweep_flag, ex, ey = map(float, match.groups())
                assert max(abs(sx-shape["start"][0]), abs(sy-shape["start"][1]),
                           abs(ex-shape["end"][0]), abs(ey-shape["end"][1])) <= 5.1e-7
                assert abs(rx-shape["circle"]["radiusMm"]) <= 5.1e-7 and abs(ry-rx) <= 1e-12
                center = shape["circle"]["center"]
                assert math.dist(center, circle(shape["start"], shape["mid"], shape["end"])) <= 1e-9
                start_angle = math.atan2(shape["start"][1]-center[1], shape["start"][0]-center[0])
                mid_angle = math.atan2(shape["mid"][1]-center[1], shape["mid"][0]-center[0])
                end_angle = math.atan2(shape["end"][1]-center[1], shape["end"][0]-center[0])
                positive_end = (end_angle-start_angle) % TAU
                positive_mid = (mid_angle-start_angle) % TAU
                expected_sweep = 1 if positive_mid <= positive_end else 0
                sweep_angle = positive_end if expected_sweep else TAU-positive_end
                assert int(sweep_flag) == expected_sweep
                assert int(large_flag) == int(sweep_angle > math.pi)
                progress = ((mid_angle-start_angle) if expected_sweep else (start_angle-mid_angle)) % TAU
                assert progress <= sweep_angle + 1e-9, "SVG sweep excludes its source midpoint"
            for primitive in circles:
                shape = primitive["shape"]
                element = by_id[primitive["source"]["uuid"]]
                assert element.tag == NS + "circle"
                assert max(abs(float(element.attrib["cx"])-shape["center"][0]),
                           abs(float(element.attrib["cy"])-shape["center"][1]),
                           abs(float(element.attrib["r"])-shape["radiusMm"])) <= 5.1e-7
            selected = {e.attrib["data-component-id"] for e in elements if "data-component-id" in e.attrib}
            assert selected and selected <= component_ids
            repeated = output / f"repeat-{side}.svg"
            run(binary, "export", "svg", porcupine, "--side", side, "--output", repeated)
            assert svg.read_bytes() == repeated.read_bytes()
        print("PASS geometry E2E: real cutouts, routed arcs, SVG identities/views, invalid-source refusal and inventory packages")
        print("PASS pinned independent Edge.Cuts Gerber reference: " + json.dumps(comparison, sort_keys=True))
        print("PASS valid RP2040 board Gerber reference: " + json.dumps(rp2040_comparison, sort_keys=True))
        print("PASS reduced LumenPnP curved-profile Gerber reference and footprint cutouts: " + json.dumps(lumenpnp_comparison, sort_keys=True))
        print("PASS unchanged original LumenPnP PCB/stored Gerber and actual SVG geometry: " + json.dumps(
            {"canonical": original_comparison, "svg": original_svg_comparisons}, sort_keys=True))


if __name__ == "__main__":
    main()
