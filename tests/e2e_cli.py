#!/usr/bin/env python3
"""Exercise the built CLI through actual processes and parse its artifacts; standard library only."""
import argparse
import csv
import hashlib
import io
import json
from pathlib import Path
import subprocess
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

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    fixture_root = Path(__file__).resolve().parent / "fixtures" / "open-source"
    with tempfile.TemporaryDirectory(prefix="tokito-cli-e2e-") as output:
        output = Path(output)
        for index, (relative, footprints, pads, segments, vias) in enumerate(FIXTURES):
            source = fixture_root / relative
            inspection = json.loads(invoke(binary, "inspect", source, "--json").stdout)
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
            print(f"PASS {relative}: inspect, placements, XML top/bottom, identities, determinism, no overwrite, native package round trip, forgery rejection")
        invoke(binary, "export", "svg", fixture_root / FIXTURES[0][0], "--output", output / "bad.svg",
               "--layers", "not-a-board-layer", succeeds=False)
    print("PASS all CLI E2E journeys; no device/desktop CAD conformance claimed")

if __name__ == "__main__":
    main()
