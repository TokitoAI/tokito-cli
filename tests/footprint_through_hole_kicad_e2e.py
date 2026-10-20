#!/usr/bin/env python3
"""Pinned KiCad format readback for generated footprint candidates."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "examples/footprints/header-1x02-through-hole.json"
FOOTPRINT = "Header_1x02_P2.54mm"
NPTH_SPEC = ROOT / "examples/footprints/mounting-hole-3mm-npth.json"
NPTH_FOOTPRINT = "MountingHole_3mm"
OVAL_SPEC = ROOT / "examples/footprints/header-1x02-oval-offset.json"
OVAL_FOOTPRINT = "Header_1x02_Slotted_P2.54mm"
SOT23_SPEC = ROOT / "examples/footprints/sot-23-to236ab.json"
SOT23_FOOTPRINT = "SOT-23"
MIXED_SPEC = ROOT / "examples/footprints/mixed-smd-through-hole.json"
MIXED_FOOTPRINT = "Mixed_Header_Test"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def svg_path_count(path: Path) -> int:
    root = ET.parse(path).getroot()
    if root.tag.rsplit("}", 1)[-1] != "svg":
        raise SystemExit(f"KiCad readback output is not an SVG document: {path}")
    return sum(1 for element in root.iter() if element.tag.rsplit("}", 1)[-1] == "path")


def run(command: list[str], *, timeout: int = 120) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        command,
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit(
            f"command failed ({result.returncode}): {command[0]}\n"
            f"stdout: {result.stdout[-4000:]}\nstderr: {result.stderr[-4000:]}"
        )
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--kicad-cli", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--expected-version", default="10.0.7")
    args = parser.parse_args()

    binary = args.binary.resolve(strict=True)
    kicad_cli = args.kicad_cli.resolve(strict=True)
    output = args.output_dir.resolve()
    if output.exists():
        raise SystemExit(f"refusing existing readback directory: {output}")
    output.mkdir(parents=True)
    library = output / "Header.pretty"
    svg_dir = output / "svg"
    svg_dir.mkdir()

    version = run([str(kicad_cli), "--version"]).stdout.strip()
    if version != args.expected_version:
        raise SystemExit(
            f"expected KiCad CLI {args.expected_version}, got {version!r}"
        )

    tokito_result = run(
        [
            str(binary),
            "component",
            "footprint",
            "library",
            "create",
            str(SPEC),
            "--output",
            str(library),
            "--json",
        ]
    )
    report = json.loads(tokito_result.stdout)
    if (
        report.get("schema") != "tokito.footprint-library-create-report.v1"
        or report.get("status") != "explicit-through-hole-library-candidate"
        or report.get("registeredInFpLibTable") is not False
        or report.get("footprintGeneration", {}).get("padType") != "thru_hole"
        or report.get("footprintGeneration", {}).get("manufacturerLandPatternQualified") is not False
    ):
        raise SystemExit(f"Tokito did not report an explicit unqualified THT candidate: {report}")

    footprint_path = library / f"{FOOTPRINT}.kicad_mod"
    footprint_text = footprint_path.read_text(encoding="utf-8")
    if (
        '(attr through_hole)' not in footprint_text
        or footprint_text.count('(pad "') != 2
        or footprint_text.count(' thru_hole circle ') != 2
        or footprint_text.count('(drill 0.8)') != 2
        or footprint_text.count('(layers "*.Cu" "*.Mask")') != 2
    ):
        raise SystemExit("generated footprint does not preserve expected pad/drill fields")

    run(
        [
            str(kicad_cli),
            "fp",
            "export",
            "svg",
            "--output",
            str(svg_dir),
            "--layers",
            "F.Cu,F.Fab,F.SilkS",
            "--footprint",
            FOOTPRINT,
            str(library),
        ]
    )
    svg_files = sorted(svg_dir.glob("*.svg"))
    if len(svg_files) != 1 or svg_files[0].stat().st_size <= 0:
        raise SystemExit(f"expected one nonempty KiCad SVG readback, found {svg_files}")
    root = ET.parse(svg_files[0]).getroot()
    if root.tag.rsplit("}", 1)[-1] != "svg":
        raise SystemExit("KiCad readback output is not an SVG document")

    npth_library = output / "MountingHole.pretty"
    npth_svg_dir = output / "npth-svg"
    npth_svg_dir.mkdir()
    npth_result = run(
        [
            str(binary),
            "component",
            "footprint",
            "library",
            "create",
            str(NPTH_SPEC),
            "--output",
            str(npth_library),
            "--json",
        ]
    )
    npth_report = json.loads(npth_result.stdout)
    npth_footprint_path = npth_library / f"{NPTH_FOOTPRINT}.kicad_mod"
    npth_text = npth_footprint_path.read_text(encoding="utf-8")
    if (
        npth_report.get("status") != "explicit-non-plated-hole-library-candidate"
        or npth_report.get("registeredInFpLibTable") is not False
        or npth_report.get("footprintGeneration", {}).get("padType") != "np_thru_hole"
        or npth_report.get("footprintGeneration", {}).get("manufacturerLandPatternQualified") is not False
        or '(attr through_hole)' not in npth_text
        or npth_text.count('(pad "" np_thru_hole circle') != 1
        or npth_text.count('(size 3 3) (drill 3) (layers "*.Cu" "*.Mask")') != 1
    ):
        raise SystemExit(f"Tokito did not report the explicit unqualified NPTH candidate: {npth_report}")
    run(
        [
            str(kicad_cli),
            "fp",
            "export",
            "svg",
            "--output",
            str(npth_svg_dir),
            "--layers",
            "F.Cu,F.Fab,F.SilkS",
            "--footprint",
            NPTH_FOOTPRINT,
            str(npth_library),
        ]
    )
    npth_svg_files = sorted(npth_svg_dir.glob("*.svg"))
    if len(npth_svg_files) != 1 or npth_svg_files[0].stat().st_size <= 0:
        raise SystemExit(f"expected one nonempty KiCad NPTH SVG readback, found {npth_svg_files}")
    npth_svg_root = ET.parse(npth_svg_files[0]).getroot()
    if npth_svg_root.tag.rsplit("}", 1)[-1] != "svg":
        raise SystemExit("KiCad NPTH readback output is not an SVG document")

    oval_library = output / "HeaderSlotted.pretty"
    oval_svg_dir = output / "oval-svg"
    oval_svg_dir.mkdir()
    oval_result = run(
        [
            str(binary),
            "component",
            "footprint",
            "library",
            "create",
            str(OVAL_SPEC),
            "--output",
            str(oval_library),
            "--json",
        ]
    )
    oval_report = json.loads(oval_result.stdout)
    oval_path = oval_library / f"{OVAL_FOOTPRINT}.kicad_mod"
    oval_text = oval_path.read_text(encoding="utf-8")
    if (
        oval_report.get("status") != "explicit-through-hole-library-candidate"
        or oval_report.get("registeredInFpLibTable") is not False
        or oval_report.get("footprintGeneration", {}).get("padType") != "thru_hole"
        or oval_report.get("footprintGeneration", {}).get("manufacturerLandPatternQualified") is not False
        or oval_text.count('(drill oval 0.8 1.2 (offset 0.1 0))') != 2
    ):
        raise SystemExit(f"Tokito did not preserve explicit oval/offset drills: {oval_report}")
    run(
        [
            str(kicad_cli),
            "fp",
            "export",
            "svg",
            "--output",
            str(oval_svg_dir),
            "--layers",
            "F.Cu,F.Fab,F.SilkS",
            "--footprint",
            OVAL_FOOTPRINT,
            str(oval_library),
        ]
    )
    oval_svg_files = sorted(oval_svg_dir.glob("*.svg"))
    if len(oval_svg_files) != 1 or oval_svg_files[0].stat().st_size <= 0:
        raise SystemExit(f"expected one nonempty KiCad oval-drill SVG, found {oval_svg_files}")
    oval_svg_root = ET.parse(oval_svg_files[0]).getroot()
    if oval_svg_root.tag.rsplit("}", 1)[-1] != "svg":
        raise SystemExit("KiCad oval-drill readback output is not an SVG document")

    round_offset_spec = output / "round-offset-spec.json"
    round_offset_payload = json.loads(SPEC.read_text(encoding="utf-8"))
    for pad in round_offset_payload["pads"]:
        pad["drillOffsetMm"] = [0.1, 0]
    round_offset_spec.write_text(
        json.dumps(round_offset_payload, indent=2) + "\n", encoding="utf-8"
    )
    round_offset_library = output / "HeaderRoundOffset.pretty"
    round_offset_svg_dir = output / "round-offset-svg"
    round_offset_svg_dir.mkdir()
    round_offset_result = run(
        [
            str(binary),
            "component",
            "footprint",
            "library",
            "create",
            str(round_offset_spec),
            "--output",
            str(round_offset_library),
            "--json",
        ]
    )
    round_offset_report = json.loads(round_offset_result.stdout)
    round_offset_path = round_offset_library / f"{FOOTPRINT}.kicad_mod"
    round_offset_text = round_offset_path.read_text(encoding="utf-8")
    if (
        round_offset_report.get("status") != "explicit-through-hole-library-candidate"
        or round_offset_report.get("registeredInFpLibTable") is not False
        or round_offset_text.count("(drill 0.8 (offset 0.1 0))") != 2
    ):
        raise SystemExit(
            f"Tokito did not preserve explicit round/offset drills: {round_offset_report}"
        )
    run(
        [
            str(kicad_cli),
            "fp",
            "export",
            "svg",
            "--output",
            str(round_offset_svg_dir),
            "--layers",
            "F.Cu,F.Fab,F.SilkS",
            "--footprint",
            FOOTPRINT,
            str(round_offset_library),
        ]
    )
    round_offset_svg_files = sorted(round_offset_svg_dir.glob("*.svg"))
    if len(round_offset_svg_files) != 1 or round_offset_svg_files[0].stat().st_size <= 0:
        raise SystemExit(
            f"expected one nonempty KiCad round-offset SVG, found {round_offset_svg_files}"
        )
    round_offset_svg_root = ET.parse(round_offset_svg_files[0]).getroot()
    if round_offset_svg_root.tag.rsplit("}", 1)[-1] != "svg":
        raise SystemExit("KiCad round-offset readback output is not an SVG document")

    roundrect_spec = output / "roundrect-slot-spec.json"
    roundrect_payload = json.loads(SPEC.read_text(encoding="utf-8"))
    for pad in roundrect_payload["pads"]:
        pad["shape"] = "roundrect"
        pad["roundrectRatio"] = 0.25
        pad["sizeMm"] = [4, 2]
        pad.pop("drillMm", None)
        pad["drillOvalMm"] = [2.5, 0.6]
    roundrect_spec.write_text(
        json.dumps(roundrect_payload, indent=2) + "\n", encoding="utf-8"
    )
    roundrect_library = output / "HeaderRoundrectSlot.pretty"
    roundrect_svg_dir = output / "roundrect-svg"
    roundrect_svg_dir.mkdir()
    roundrect_result = run(
        [
            str(binary),
            "component",
            "footprint",
            "library",
            "create",
            str(roundrect_spec),
            "--output",
            str(roundrect_library),
            "--json",
        ]
    )
    roundrect_report = json.loads(roundrect_result.stdout)
    roundrect_path = roundrect_library / f"{FOOTPRINT}.kicad_mod"
    roundrect_text = roundrect_path.read_text(encoding="utf-8")
    if (
        roundrect_report.get("status") != "explicit-through-hole-library-candidate"
        or roundrect_report.get("registeredInFpLibTable") is not False
        or roundrect_text.count("(roundrect_rratio 0.25)") != 2
        or roundrect_text.count("(drill oval 2.5 0.6)") != 2
    ):
        raise SystemExit(
            f"Tokito did not preserve roundrect pads and contained oval drills: {roundrect_report}"
        )
    run(
        [
            str(kicad_cli),
            "fp",
            "export",
            "svg",
            "--output",
            str(roundrect_svg_dir),
            "--layers",
            "F.Cu,F.Fab,F.SilkS",
            "--footprint",
            FOOTPRINT,
            str(roundrect_library),
        ]
    )
    roundrect_svg_files = sorted(roundrect_svg_dir.glob("*.svg"))
    if len(roundrect_svg_files) != 1 or roundrect_svg_files[0].stat().st_size <= 0:
        raise SystemExit(
            f"expected one nonempty KiCad roundrect-slot SVG, found {roundrect_svg_files}"
        )
    roundrect_svg_root = ET.parse(roundrect_svg_files[0]).getroot()
    if roundrect_svg_root.tag.rsplit("}", 1)[-1] != "svg":
        raise SystemExit("KiCad roundrect-slot readback output is not an SVG document")

    sot23_library = output / "SOT23.pretty"
    sot23_svg_dir = output / "sot23-svg"
    sot23_svg_dir.mkdir()
    sot23_result = run(
        [
            str(binary),
            "component",
            "footprint",
            "library",
            "create",
            str(SOT23_SPEC),
            "--output",
            str(sot23_library),
            "--json",
        ]
    )
    sot23_report = json.loads(sot23_result.stdout)
    sot23_path = sot23_library / f"{SOT23_FOOTPRINT}.kicad_mod"
    sot23_text = sot23_path.read_text(encoding="utf-8")
    if (
        sot23_report.get("schema") != "tokito.footprint-library-create-report.v1"
        or sot23_report.get("status") != "explicit-smd-library-candidate"
        or sot23_report.get("registeredInFpLibTable") is not False
        or sot23_report.get("footprintGeneration", {}).get("schema")
        != "tokito.footprint-generation-report.v1"
        or sot23_report.get("footprintGeneration", {}).get("padCount") != 3
        or sot23_report.get("footprintGeneration", {}).get("manufacturerLandPatternQualified") is not False
        or sot23_text.count('(pad "') != 3
        or sot23_text.count("(fp_poly") != 2
        or "(fill yes)" not in sot23_text
        or "(fill no)" not in sot23_text
    ):
        raise SystemExit(
            f"Tokito did not preserve the explicit SOT-23 pad/polygon candidate: {sot23_report}"
        )
    run(
        [
            str(kicad_cli),
            "fp",
            "export",
            "svg",
            "--output",
            str(sot23_svg_dir),
            "--layers",
            "F.Cu,F.Fab,F.SilkS",
            "--footprint",
            SOT23_FOOTPRINT,
            str(sot23_library),
        ]
    )
    sot23_svg_files = sorted(sot23_svg_dir.glob("*.svg"))
    if len(sot23_svg_files) != 1 or sot23_svg_files[0].stat().st_size <= 0:
        raise SystemExit(
            f"expected one nonempty KiCad SOT-23 SVG readback, found {sot23_svg_files}"
        )
    sot23_svg_path_count = svg_path_count(sot23_svg_files[0])

    # Plot the same candidate with its polygon records removed. A higher path
    # count in the KiCad output confirms these filled/unfilled nodes affect the
    # rendered plot, rather than merely surviving Tokito serialization.
    sot23_without_polygons_spec = output / "sot23-without-polygons.json"
    sot23_without_polygons_payload = json.loads(SOT23_SPEC.read_text(encoding="utf-8"))
    sot23_without_polygons_payload["graphics"] = [
        graphic
        for graphic in sot23_without_polygons_payload["graphics"]
        if graphic["kind"] != "polygon"
    ]
    sot23_without_polygons_spec.write_text(
        json.dumps(sot23_without_polygons_payload, indent=2) + "\n", encoding="utf-8"
    )
    sot23_without_polygons_library = output / "SOT23WithoutPolygons.pretty"
    sot23_without_polygons_svg_dir = output / "sot23-without-polygons-svg"
    sot23_without_polygons_svg_dir.mkdir()
    run(
        [
            str(binary),
            "component",
            "footprint",
            "library",
            "create",
            str(sot23_without_polygons_spec),
            "--output",
            str(sot23_without_polygons_library),
            "--json",
        ]
    )
    run(
        [
            str(kicad_cli),
            "fp",
            "export",
            "svg",
            "--output",
            str(sot23_without_polygons_svg_dir),
            "--layers",
            "F.Cu,F.Fab,F.SilkS",
            "--footprint",
            SOT23_FOOTPRINT,
            str(sot23_without_polygons_library),
        ]
    )
    sot23_without_polygons_svg_files = sorted(sot23_without_polygons_svg_dir.glob("*.svg"))
    if len(sot23_without_polygons_svg_files) != 1 or sot23_svg_path_count <= svg_path_count(
        sot23_without_polygons_svg_files[0]
    ):
        raise SystemExit("KiCad plot does not show a path-count change when SOT-23 polygons are added")
    sot23_without_polygons_svg_path_count = svg_path_count(sot23_without_polygons_svg_files[0])

    mixed_library = output / "MixedHeader.pretty"
    mixed_svg_dir = output / "mixed-svg"
    mixed_svg_dir.mkdir()
    mixed_result = run(
        [
            str(binary),
            "component",
            "footprint",
            "library",
            "create",
            str(MIXED_SPEC),
            "--output",
            str(mixed_library),
            "--json",
        ]
    )
    mixed_report = json.loads(mixed_result.stdout)
    mixed_path = mixed_library / f"{MIXED_FOOTPRINT}.kicad_mod"
    mixed_text = mixed_path.read_text(encoding="utf-8")
    if (
        mixed_report.get("status") != "explicit-mixed-pad-library-candidate"
        or mixed_report.get("registeredInFpLibTable") is not False
        or mixed_report.get("footprintGeneration", {}).get("padType") != "mixed"
        or mixed_report.get("footprintGeneration", {}).get("padCount") != 3
        or "(attr through_hole)" not in mixed_text
        or mixed_text.count('(pad "') != 3
        or mixed_text.count(" smd rect ") != 1
        or mixed_text.count(" thru_hole circle ") != 1
        or mixed_text.count(" np_thru_hole circle ") != 1
    ):
        raise SystemExit(f"Tokito did not preserve mixed pad types: {mixed_report}")
    run(
        [
            str(kicad_cli),
            "fp",
            "export",
            "svg",
            "--output",
            str(mixed_svg_dir),
            "--layers",
            "F.Cu,F.Fab,F.SilkS",
            "--footprint",
            MIXED_FOOTPRINT,
            str(mixed_library),
        ]
    )
    mixed_svg_files = sorted(mixed_svg_dir.glob("*.svg"))
    if len(mixed_svg_files) != 1 or mixed_svg_files[0].stat().st_size <= 0:
        raise SystemExit(f"expected one nonempty KiCad mixed-pad SVG, found {mixed_svg_files}")
    mixed_svg_root = ET.parse(mixed_svg_files[0]).getroot()
    if mixed_svg_root.tag.rsplit("}", 1)[-1] != "svg":
        raise SystemExit("KiCad mixed-pad readback output is not an SVG document")

    receipt = {
        "schema": "tokito.footprint-through-hole-kicad-readback.v1",
        "passed": True,
        "kicadCliVersion": version,
        "kicadCliSha256": sha256(kicad_cli),
        "tokitoBinarySha256": sha256(binary),
        "specSha256": sha256(SPEC),
        "footprintSha256": sha256(footprint_path),
        "svgSha256": sha256(svg_files[0]),
        "svgBytes": svg_files[0].stat().st_size,
        "footprintSemanticsQualified": False,
        "manufacturerLandPatternQualified": False,
        "scope": "KiCad format/plot acceptance only; no independent pad-dimension or land-pattern qualification",
        "nonPlatedHole": {
            "reportStatus": npth_report["status"],
            "footprintSha256": sha256(npth_footprint_path),
            "svgSha256": sha256(npth_svg_files[0]),
            "svgBytes": npth_svg_files[0].stat().st_size,
            "footprintSemanticsQualified": False,
            "manufacturerLandPatternQualified": False,
        },
        "ovalOffsetThroughHole": {
            "reportStatus": oval_report["footprintGeneration"]["status"],
            "footprintSha256": sha256(oval_path),
            "svgSha256": sha256(oval_svg_files[0]),
            "svgBytes": oval_svg_files[0].stat().st_size,
            "footprintSemanticsQualified": False,
            "manufacturerLandPatternQualified": False,
        },
        "roundOffsetThroughHole": {
            "reportStatus": round_offset_report["footprintGeneration"]["status"],
            "footprintSha256": sha256(round_offset_path),
            "svgSha256": sha256(round_offset_svg_files[0]),
            "svgBytes": round_offset_svg_files[0].stat().st_size,
            "footprintSemanticsQualified": False,
            "manufacturerLandPatternQualified": False,
        },
        "roundrectOvalDrillThroughHole": {
            "reportStatus": roundrect_report["footprintGeneration"]["status"],
            "footprintSha256": sha256(roundrect_path),
            "svgSha256": sha256(roundrect_svg_files[0]),
            "svgBytes": roundrect_svg_files[0].stat().st_size,
            "footprintSemanticsQualified": False,
            "manufacturerLandPatternQualified": False,
        },
        "sot23FootprintPlotAcceptance": {
            "reportStatus": sot23_report["footprintGeneration"]["status"],
            "footprintSha256": sha256(sot23_path),
            "svgSha256": sha256(sot23_svg_files[0]),
            "svgBytes": sot23_svg_files[0].stat().st_size,
            "svgPathCountWithPolygons": sot23_svg_path_count,
            "svgPathCountWithoutPolygons": sot23_without_polygons_svg_path_count,
            "footprintSemanticsQualified": False,
            "manufacturerLandPatternQualified": False,
        },
        "mixedPadTechnologyFormatAcceptance": {
            "reportStatus": mixed_report["footprintGeneration"]["status"],
            "specSha256": sha256(MIXED_SPEC),
            "footprintSha256": sha256(mixed_path),
            "svgSha256": sha256(mixed_svg_files[0]),
            "svgBytes": mixed_svg_files[0].stat().st_size,
            "padTypes": ["smd", "thru_hole", "np_thru_hole"],
            "footprintSemanticsQualified": False,
            "manufacturerLandPatternQualified": False,
        },
    }
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(receipt, sort_keys=True))


if __name__ == "__main__":
    main()
