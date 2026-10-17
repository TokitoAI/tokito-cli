#!/usr/bin/env python3
"""Type-filter selection oracle and offline CLI E2E, not geometry equivalence.

Reference capture explicitly invokes a supplied KiCad development executable.
Normal --binary verification only runs Tokito and consumes the pinned capture.
The synthetic metadata fixture deliberately reuses one real resistor body for
all footprint types; it is not a physically qualified board design.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import re
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
WRL = ROOT / "tests/fixtures/open-source/kicad-resistor-models/Resistor_SMD.3dshapes/R_0603_1608Metric.wrl"
REFERENCE = ROOT / "tests/reference/kicad-cli-10.0.7/type-filter/manifest.json"
ATTRIBUTES = ["(attr smd)", "(attr through_hole)", "", "(attr board_only)",
              "(attr smd dnp)", "(attr dnp)", "", "(attr)", "(attr virtual)",
              "(attr allow_missing_courtyard allow_soldermask_bridges)",
              "(attr smd board_only exclude_from_bom exclude_from_pos_files)",
              "(attr through_hole dnp)"]
CASES = [
    ("default", []),
    ("no-unspecified", ["--no-unspecified"]),
    ("no-dnp", ["--no-dnp"]),
    ("both", ["--no-dnp", "--no-unspecified"]),
    ("pattern-type", ["--component-filter", "R3,R5,R6,R11", "--no-unspecified"]),
    ("pattern-both", ["--component-filter", "R3,R5,R6,R11", "--no-unspecified", "--no-dnp"]),
]


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fixture() -> bytes:
    text = '''(kicad_pcb (version 20240108) (generator "tokito_cli_reference_fixture")
  (general (thickness 1.6)) (paper "A4")
  (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (37 "F.SilkS" user "f.silkscreen") (44 "Edge.Cuts" user))
  (setup (stackup (layer "F.Cu" (type "copper") (thickness 0.035))
    (layer "dielectric 1" (type "core") (thickness 1.53))
    (layer "B.Cu" (type "copper") (thickness 0.035))))
  (gr_rect (start 0 0) (end 40 20) (stroke (width 0.05) (type default))
    (fill none) (layer "Edge.Cuts") (uuid "82baf57e-062a-4ec7-912a-000000000099"))
'''
    for ordinal, attributes in enumerate(ATTRIBUTES, 1):
        pad = '(pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu"))' if ordinal == 7 else ""
        text += f'''  (footprint "FilterFixture:R_0603" (layer "F.Cu")
    (at {2 + (ordinal - 1) % 6 * 6} {5 + (ordinal - 1) // 6 * 9})
    (uuid "82baf57e-062a-4ec7-912a-{ordinal:012d}") {attributes}
    (property "Reference" "R{ordinal}" (at 0 -1) (layer "F.SilkS")
      (effects (font (size 1 1) (thickness 0.15))))
    (property "Type" "smd") {pad}
    (model "${{KIPRJMOD}}/Resistor_SMD.3dshapes/R_0603_1608Metric.wrl"
      (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 0))))
'''
    return (text + ")\n").encode("utf-8")


def document(path: Path) -> dict:
    payload = path.read_bytes()
    magic, version, length = struct.unpack_from("<4sII", payload)
    assert magic == b"glTF" and version == 2 and length == len(payload)
    chunk_length, chunk_kind = struct.unpack_from("<I4s", payload, 12)
    assert chunk_kind == b"JSON"
    return json.loads(payload[20:20 + chunk_length])


def run(command: list[str], *, succeeds: bool = True) -> subprocess.CompletedProcess:
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                            encoding="utf-8", errors="replace", timeout=120, check=False)
    assert (result.returncode == 0) == succeeds, (result.returncode, result.stdout[-3000:], result.stderr[-3000:])
    return result


def prepare(root: Path) -> Path:
    model_dir = root / "Resistor_SMD.3dshapes"
    model_dir.mkdir()
    shutil.copyfile(WRL, model_dir / WRL.name)
    board = root / "footprint-types.kicad_pcb"
    board.write_bytes(fixture())
    return board


def normalized_log(text: str, root: Path) -> str:
    return text.replace(str(root), "<capture-root>").replace(root.as_posix(), "<capture-root>")


def capture(args: argparse.Namespace) -> None:
    assert args.output and args.kicad_cli and args.step_model
    output = args.output.resolve()
    assert not output.exists(), "refuse existing capture output"
    output.mkdir(parents=True)
    board = prepare(output)
    native = str(args.kicad_cli.resolve(strict=True))
    assert run([native, "--version"]).stdout.strip() == "10.0.7"
    step = args.step_model.resolve(strict=True)
    shutil.copyfile(step, output / "Resistor_SMD.3dshapes/R_0603_1608Metric.step")
    references = {f"R{n}" for n in range(1, 13)}
    receipt = {"schema": "tokito.kicad-footprint-type-filter-reference.v1",
               "capturedAtUtc": datetime.now(timezone.utc).isoformat(),
               "version": "10.0.7", "executableSha256": sha(Path(native).read_bytes()),
               "fixtureSha256": sha(fixture()), "wrlSha256": sha(WRL.read_bytes()),
               "substitutedStepSha256": sha(step.read_bytes()),
               "nativeCommand": ["kicad-cli", "pcb", "export", "glb", "<fixture>", "--subst-models", "--output", "<output>", "<case options>"],
               "logPathNormalization": "Capture output root is replaced with <capture-root>; output file digests refer to unchanged native bytes.",
               "limitation": "Synthetic metadata fixture with a real resistor asset, not a physical design. Native GLB uses explicit --subst-models and the recorded STEP companion; Tokito uses the pinned WRL. Named component selection is compared; geometry equivalence, STEP import, footprint fit and real-board qualification are not claimed.",
               "cases": []}
    for name, options in CASES:
        glb = output / f"{name}.glb"
        command = [native, "pcb", "export", "glb", str(board), "--subst-models", "--output", str(glb), *options]
        result = run(command)
        doc = document(glb)
        names = [node.get("name", "") for node in doc["nodes"]]
        selected = sorted(references.intersection(names))
        if name == "default":
            assert set(selected) == references, names
        receipt["cases"].append({"name": name, "options": options,
                                 "selectedReferences": selected,
                                 "glbSha256": sha(glb.read_bytes()), "glbBytes": glb.stat().st_size,
                                 "nativeNodeNames": names,
                                 "stdout": normalized_log(result.stdout, output),
                                 "stderr": normalized_log(result.stderr, output)})
    quoted_source = fixture().replace(b"(attr smd)", b'(attr "smd")', 1)
    quoted = output / "quoted-type.kicad_pcb"
    quoted.write_bytes(quoted_source)
    quoted_glb = output / "quoted-type.glb"
    negative = run([native, "pcb", "export", "glb", str(quoted), "--subst-models",
                    "--no-unspecified", "--output", str(quoted_glb)], succeeds=False)
    assert not quoted_glb.exists()
    receipt["quotedTypeRefusal"] = {
        "fixtureSha256": sha(quoted_source), "exitCode": negative.returncode,
        "stdout": normalized_log(negative.stdout, output),
        "stderr": normalized_log(negative.stderr, output),
    }
    if args.baseline_binary:
        baseline = args.baseline_binary.resolve(strict=True)
        baseline_output = output / "tokito-pre-change-default.glb"
        result = run([str(baseline), "export", "glb", str(board), "--assemble-wrl",
                      "--project-root", str(output), "--output", str(baseline_output), "--json-report"])
        receipt["tokitoDefaultBaseline"] = {
            "binarySha256": sha(baseline.read_bytes()),
            "glbSha256": sha(baseline_output.read_bytes()),
            "reportSha256": sha(result.stdout.encode("utf-8")),
            "scope": "Previously delivered Tokito executable, run now against this deterministic fixture; default output only.",
        }
    (output / "manifest.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(f"PASS KiCad 10.0.7 capture: {output / 'manifest.json'}")


def verify(binary: Path) -> None:
    reference = json.loads(REFERENCE.read_text(encoding="utf-8"))
    assert reference["fixtureSha256"] == sha(fixture())
    assert reference["wrlSha256"] == sha(WRL.read_bytes())
    assert reference["version"] == "10.0.7"
    assert reference["schema"] == "tokito.kicad-footprint-type-filter-reference.v1"
    assert reference["quotedTypeRefusal"]["exitCode"] == 3
    assert reference["quotedTypeRefusal"]["fixtureSha256"] == sha(fixture().replace(b"(attr smd)", b'(attr "smd")', 1))
    for row in [*reference["cases"], reference["quotedTypeRefusal"]]:
        assert not re.search(r"[A-Za-z]:[\\/]", row["stdout"] + row["stderr"]), "capture logs retain an absolute Windows path"
    assert [(row["name"], row["options"]) for row in reference["cases"]] == CASES
    with tempfile.TemporaryDirectory(prefix="tokito-native-type-filter-") as temporary:
        root = Path(temporary)
        board = prepare(root)
        for row in reference["cases"]:
            output = root / f"{row['name']}.glb"
            result = run([str(binary), "export", "glb", str(board), "--assemble-wrl",
                          "--project-root", str(root), "--output", str(output),
                          "--json-report", *row["options"]])
            report = json.loads(result.stdout)
            doc = document(output)
            selected = sorted(node["name"][4:] for node in doc["nodes"]
                              if node.get("name", "").startswith("ref:"))
            assert selected == row["selectedReferences"], (row["name"], selected, row["selectedReferences"])
            if row["name"] == "default":
                assert sha(output.read_bytes()) == reference["tokitoDefaultBaseline"]["glbSha256"]
                assert sha(result.stdout.encode("utf-8")) == reference["tokitoDefaultBaseline"]["reportSha256"]
            assert not report["partialExport"]
            assert report["coverage"]["selectedComponentCount"] == len(selected)
            assert report["coverage"]["filteredComponentCount"] == 12 - len(selected)
            if "--no-unspecified" in row["options"]:
                metadata = report["coverage"]["unspecifiedFilter"]
                assert metadata == doc["asset"]["extras"]["tokito"]["coverage"]["unspecifiedFilter"]
                assert metadata["enabled"] is True
                matched = {"R3", "R5", "R6", "R11"} if "--component-filter" in row["options"] else {f"R{n}" for n in range(1, 13)}
                excluded = matched.intersection({"R3", "R4", "R6", "R7", "R8", "R9", "R10"})
                overlap = excluded.intersection({"R5", "R6", "R12"}) if "--no-dnp" in row["options"] else set()
                assert metadata["componentsExcluded"] == len(excluded)
                assert metadata["modelReferencesExcluded"] == len(excluded)
                assert metadata["overlapWithDnpComponents"] == len(overlap)
            else:
                assert "unspecifiedFilter" not in report["coverage"]
        for name, extra in [("shell", []),
                            ("assignments", ["--model-assignments", str(root / "missing.json")]),
                            ("duplicate", ["--assemble-wrl", "--project-root", str(root), "--no-unspecified"])]:
            output = root / f"refuse-{name}.glb"
            run([str(binary), "export", "glb", str(board), "--output", str(output),
                 "--no-unspecified", *extra], succeeds=False)
            assert not output.exists()
        bad = root / "ambiguous.kicad_pcb"
        bad.write_bytes(fixture().replace(b"(attr smd)", b"(attr smd through_hole)", 1))
        bad_output = root / "ambiguous.glb"
        result = run([str(binary), "export", "glb", str(bad), "--output", str(bad_output),
                      "--assemble-wrl", "--project-root", str(root), "--no-unspecified"], succeeds=False)
        assert "conflicting footprint types" in result.stderr
        assert not bad_output.exists()
        quoted = root / "quoted.kicad_pcb"
        quoted.write_bytes(fixture().replace(b"(attr smd)", b'(attr "smd")', 1))
        quoted_output = root / "quoted.glb"
        result = run([str(binary), "export", "glb", str(quoted), "--output", str(quoted_output),
                      "--assemble-wrl", "--project-root", str(root), "--no-unspecified"], succeeds=False)
        assert "quoted footprint attribute tokens" in result.stderr
        assert not quoted_output.exists()
        assert board.read_bytes() == fixture()
    print("PASS type-filter E2E: six KiCad 10.0.7 selection cases, exact selectors, overlap coverage, metadata, malformed attributes and mode/duplicate refusal; geometry equivalence not claimed")


def main() -> None:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--binary", type=Path)
    mode.add_argument("--capture-reference", action="store_true")
    parser.add_argument("--kicad-cli", type=Path)
    parser.add_argument("--step-model", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--baseline-binary", type=Path)
    args = parser.parse_args()
    if args.capture_reference:
        if not (args.output and args.kicad_cli and args.step_model and args.baseline_binary):
            parser.error("capture requires --output, --kicad-cli, --step-model and --baseline-binary")
        capture(args)
    else:
        assert args.binary, "--binary required for offline verification"
        verify(args.binary.resolve(strict=True))


if __name__ == "__main__":
    main()
