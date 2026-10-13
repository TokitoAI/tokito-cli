#!/usr/bin/env python3
"""Exercise the CLI's explicit, typed footprint-definition flow."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
EXAMPLE = ROOT / "examples/footprints/r0603-reference.json"
REFERENCE = ROOT / "tests/fixtures/kicad-10.0.5/r0603-reference-geometry.json"
SOURCE_REFERENCE = ROOT / "tests/fixtures/kicad-10.0.5/R_0603_1608Metric.kicad_mod"


def parse_sexpr(text: str):
    """Small independent reader for the generated footprint's geometry tree."""
    tokens = re.findall(r'"(?:\\.|[^"\\])*"|[()]|[^()\s]+', text)
    stack = []
    root = None
    for token in tokens:
        if token == "(":
            node = []
            if stack:
                stack[-1].append(node)
            elif root is None:
                root = node
            else:
                raise AssertionError("more than one top-level S-expression")
            stack.append(node)
        elif token == ")":
            assert stack, "unexpected closing parenthesis"
            stack.pop()
        else:
            assert stack, "atom outside S-expression"
            stack[-1].append(json.loads(token) if token.startswith('"') else token)
    assert root is not None and not stack, "incomplete S-expression"
    return root


def child(node, name: str):
    found = [item for item in node[1:] if isinstance(item, list) and item and item[0] == name]
    assert len(found) == 1, f"expected exactly one {name} child"
    return found[0]


def parsed_geometry(text: str):
    root = parse_sexpr(text)
    assert root[0] == "footprint"
    pads = []
    graphics = []
    for item in root[1:]:
        if not isinstance(item, list) or not item:
            continue
        if item[0] == "pad":
            position = child(item, "at")
            size = child(item, "size")
            layers = child(item, "layers")
            ratio = [part[1] for part in item[1:] if isinstance(part, list) and part[:1] == ["roundrect_rratio"]]
            position_mm = [float(value) for value in position[1:]]
            if len(position_mm) == 2:
                # KiCad footprint files omit a zero pad angle.
                position_mm.append(0.0)
            pads.append({
                "number": item[1], "type": item[2], "shape": item[3],
                "atMm": position_mm,
                "sizeMm": [float(value) for value in size[1:]],
                "layers": layers[1:],
                "roundrectRatio": float(ratio[0]) if ratio else None,
            })
        elif item[0] in ("fp_line", "fp_rect"):
            start = child(item, "start")
            end = child(item, "end")
            stroke = child(item, "stroke")
            width = child(stroke, "width")
            layer = child(item, "layer")
            graphics.append({
                "kind": "line" if item[0] == "fp_line" else "rectangle",
                "startMm": [float(value) for value in start[1:]],
                "endMm": [float(value) for value in end[1:]],
                "widthMm": float(width[1]), "layer": layer[1],
            })
        elif item[0] == "fp_text":
            # Reference-property text is not part of the compared pad/graphic geometry.
            continue
        elif item[0].startswith("fp_"):
            raise AssertionError(f"unsupported generated graphic {item[0]}")
    return {"pads": pads, "graphics": graphics}


def invoke(binary: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(binary), *map(str, args)],
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    binary = parser.parse_args().binary.resolve()
    with tempfile.TemporaryDirectory(prefix="tokito-footprint-spec-") as scratch:
        root = Path(scratch)
        output = root / "R_0603_1608Metric.kicad_mod"
        result = invoke(
            binary,
            "component",
            "footprint",
            "generate",
            str(EXAMPLE),
            "--output",
            str(output),
            "--json",
        )
        assert result.returncode == 0, result.stderr
        report = json.loads(result.stdout)
        generated = output.read_bytes()
        assert report["schema"] == "tokito.footprint-generation-report.v1"
        assert report["status"] == "explicit-smd-geometry-candidate"
        assert report["footprintName"] == "R_0603_1608Metric"
        assert report["padCount"] == 2 and report["graphicCount"] == 4
        assert report["sourceEvidenceContentVerified"] is False
        assert report["manufacturerLandPatternQualified"] is False
        assert report["componentBodyGenerated"] is False
        assert report["outputSha256"] == hashlib.sha256(generated).hexdigest()
        text = generated.decode("utf-8")
        assert '(property "Reference" "REF**"' in text
        assert text.count('(pad "') == 2
        assert '(pad "1" smd roundrect (at -0.825 0 0) (size 0.8 0.95)' in text
        assert '(fp_rect' in text and '(roundrect_rratio 0.25)' in text
        reference = json.loads(REFERENCE.read_text(encoding="utf-8"))
        example = json.loads(EXAMPLE.read_text(encoding="utf-8"))
        source_bytes = SOURCE_REFERENCE.read_bytes()
        source_digest = hashlib.sha256(source_bytes).hexdigest()
        assert source_digest == reference["source"]["sha256"], "pinned footprint bytes do not match the recorded source digest"
        assert source_digest == example["sourceEvidence"][0]["sha256"]
        source_geometry = parsed_geometry(source_bytes.decode("utf-8"))
        assert source_geometry == reference["geometry"], "geometry transcription differs from pinned source file"
        assert parsed_geometry(text) == source_geometry, "generated geometry differs from pinned KiCad library source"
        digest_before = hashlib.sha256(generated).hexdigest()

        library = root / "R0603.pretty"
        library_result = invoke(
            binary,
            "component",
            "footprint",
            "library",
            "create",
            str(EXAMPLE),
            "--output",
            str(library),
            "--json",
        )
        assert library_result.returncode == 0, library_result.stderr
        library_report = json.loads(library_result.stdout)
        library_footprint = library / "R_0603_1608Metric.kicad_mod"
        assert library_report["schema"] == "tokito.footprint-library-create-report.v1"
        assert library_report["status"] == "explicit-smd-library-candidate"
        assert library_report["registeredInFpLibTable"] is False
        assert library_footprint.is_file()
        assert hashlib.sha256(library_footprint.read_bytes()).hexdigest() == digest_before
        assert sorted(path.name for path in library.iterdir()) == [library_footprint.name]
        library_before = library_footprint.read_bytes()

        lock_path = root / "R0603.lock.json"
        lock_create = invoke(
            binary,
            "component", "footprint", "library", "lock", str(library),
            "--output", str(lock_path), "--json",
        )
        assert lock_create.returncode == 0, lock_create.stderr
        lock_create_report = json.loads(lock_create.stdout)
        assert lock_create_report["schema"] == "tokito.footprint-library-lock-create.v1"
        assert lock_create_report["status"] == "content-locked"
        assert lock_create_report["footprintSemanticsQualified"] is False
        lock_bytes = lock_path.read_bytes()
        in_library_lock = library / "in-library.lock.json"
        rejected_in_library_lock = invoke(
            binary,
            "component", "footprint", "library", "lock", str(library),
            "--output", str(in_library_lock),
        )
        assert rejected_in_library_lock.returncode != 0
        assert "outside the inspected .pretty directory" in rejected_in_library_lock.stderr
        assert not in_library_lock.exists()
        lock = json.loads(lock_bytes)
        assert lock["schema"] == "tokito.footprint-library-lock.v1"
        assert lock["assets"] == [{
            "fileName": library_footprint.name,
            "bytes": len(library_before),
            "sha256": digest_before,
        }]
        lock_verify = invoke(
            binary,
            "component", "footprint", "library", "verify", str(library),
            "--lock", str(lock_path), "--json",
        )
        assert lock_verify.returncode == 0, lock_verify.stderr
        lock_verify_report = json.loads(lock_verify.stdout)
        assert lock_verify_report["matches"] is True
        assert lock_verify_report["status"] == "verified"
        assert lock_verify_report["expectedInventorySha256"] == lock_verify_report["actualInventorySha256"]
        for mutation, expected_error in [
            (lambda value: value.update({"unexpected": True}), "unknown field"),
            (lambda value: value["assets"][0].update({"fileName": "../escape.kicad_mod"}), "invalid footprint file name"),
            (lambda value: value["assets"][0].update({"bytes": 4 * 1024 * 1024 + 1}), "asset exceeds"),
        ]:
            corrupted_lock = json.loads(lock_bytes)
            mutation(corrupted_lock)
            corrupted_path = root / f"invalid-lock-{len(expected_error)}.json"
            corrupted_path.write_text(json.dumps(corrupted_lock), encoding="utf-8")
            rejected_lock = invoke(
                binary,
                "component", "footprint", "library", "verify", str(library),
                "--lock", str(corrupted_path),
            )
            assert rejected_lock.returncode != 0 and expected_error in rejected_lock.stderr
        duplicate_lock = invoke(
            binary,
            "component", "footprint", "library", "lock", str(library),
            "--output", str(lock_path),
        )
        assert duplicate_lock.returncode != 0 and "output already exists" in duplicate_lock.stderr
        assert lock_path.read_bytes() == lock_bytes

        (library / "Malformed.kicad_mod").write_text('(footprint "bad"', encoding="utf-8")
        (library / "Legacy.kicad_mod").write_text('(module "legacy")', encoding="utf-8")
        (library / "NoName.kicad_mod").write_text('(footprint "")', encoding="utf-8")
        inventory_args = (
            "component", "footprint", "library", "inspect", str(library), "--json"
        )
        inventory_result = invoke(binary, *inventory_args)
        assert inventory_result.returncode == 0, inventory_result.stderr
        inventory = json.loads(inventory_result.stdout)
        assert inventory["schema"] == "tokito.footprint-library-inventory.v1"
        assert inventory["status"] == "inventory-only"
        assert inventory["footprintFileCount"] == 4
        assets = {asset["fileName"]: asset for asset in inventory["assets"]}
        assert assets[library_footprint.name] == {
            "fileName": library_footprint.name,
            "bytes": len(library_before),
            "sha256": digest_before,
            "parseStatus": "parsed_footprint_root",
            "footprintName": "R_0603_1608Metric",
            "padCount": 2,
            "modelReferenceCount": 0,
        }
        assert assets["Malformed.kicad_mod"]["parseStatus"] == "invalid_sexpr"
        assert assets["Legacy.kicad_mod"]["parseStatus"] == "unsupported_root"
        assert assets["NoName.kicad_mod"]["parseStatus"] == "invalid_footprint_name"
        assert inventory["footprintSyntaxValidated"] is False
        assert inventory["geometryValidated"] is False
        assert inventory["libraryRegistrationChanged"] is False
        repeated_inventory = json.loads(invoke(binary, *inventory_args).stdout)
        assert repeated_inventory == inventory
        assert library_footprint.read_bytes() == library_before
        drifted_lock = invoke(
            binary,
            "component", "footprint", "library", "verify", str(library),
            "--lock", str(lock_path), "--json",
        )
        assert drifted_lock.returncode != 0
        drift_report = json.loads(drifted_lock.stdout)
        assert drift_report["matches"] is False and drift_report["status"] == "drifted"
        assert drift_report["addedFiles"] == ["Legacy.kicad_mod", "Malformed.kicad_mod", "NoName.kicad_mod"]
        assert drift_report["missingFiles"] == [] and drift_report["changedFiles"] == []

        changed_bytes = library_footprint.read_bytes() + b"\n; changed after lock\n"
        library_footprint.write_bytes(changed_bytes)
        changed_lock = invoke(
            binary,
            "component", "footprint", "library", "verify", str(library),
            "--lock", str(lock_path), "--json",
        )
        assert changed_lock.returncode != 0
        changed_report = json.loads(changed_lock.stdout)
        assert changed_report["changedFiles"] == [library_footprint.name]
        assert len(changed_report["addedFiles"]) == 3
        library_footprint.write_bytes(library_before)

        missing_copy = root / "missing-entry.pretty"
        missing_copy.mkdir()
        first_asset = missing_copy / "A.kicad_mod"
        second_asset = missing_copy / "B.kicad_mod"
        first_asset.write_bytes(b'(footprint "A")\n')
        second_asset.write_bytes(b'(footprint "B")\n')
        missing_lock = root / "missing-entry.lock.json"
        missing_lock_create = invoke(
            binary, "component", "footprint", "library", "lock", str(missing_copy),
            "--output", str(missing_lock),
        )
        assert missing_lock_create.returncode == 0, missing_lock_create.stderr
        second_asset.unlink()
        missing_lock_verify = invoke(
            binary, "component", "footprint", "library", "verify", str(missing_copy),
            "--lock", str(missing_lock), "--json",
        )
        assert missing_lock_verify.returncode != 0
        missing_report = json.loads(missing_lock_verify.stdout)
        assert missing_report["missingFiles"] == ["B.kicad_mod"]
        assert missing_report["addedFiles"] == []

        missing_library = root / "missing.pretty"
        missing_inventory = invoke(
            binary, "component", "footprint", "library", "inspect", str(missing_library), "--json"
        )
        assert missing_inventory.returncode != 0
        assert not missing_library.exists()

        overwrite_library = invoke(
            binary,
            "component",
            "footprint",
            "library",
            "create",
            str(EXAMPLE),
            "--output",
            str(library),
        )
        assert overwrite_library.returncode != 0 and "cannot create new" in overwrite_library.stderr
        assert hashlib.sha256(library_footprint.read_bytes()).hexdigest() == digest_before

        bad_library = root / "not-a-library"
        bad_library_result = invoke(
            binary,
            "component",
            "footprint",
            "library",
            "create",
            str(EXAMPLE),
            "--output",
            str(bad_library),
        )
        assert bad_library_result.returncode != 0 and ".pretty" in bad_library_result.stderr
        assert not bad_library.exists()

        overwrite = invoke(
            binary,
            "component",
            "footprint",
            "generate",
            str(EXAMPLE),
            "--output",
            str(output),
        )
        assert overwrite.returncode != 0 and "already exists" in overwrite.stderr
        assert hashlib.sha256(output.read_bytes()).hexdigest() == digest_before

        invalid_spec = root / "invalid.json"
        invalid_spec.write_text(
            EXAMPLE.read_text(encoding="utf-8").replace('"side": "front"', '"side": "unsupported"'),
            encoding="utf-8",
        )
        invalid_output = root / "invalid.kicad_mod"
        invalid = invoke(
            binary,
            "component",
            "footprint",
            "generate",
            str(invalid_spec),
            "--output",
            str(invalid_output),
        )
        assert invalid.returncode != 0
        assert not invalid_output.exists()

        back_spec = root / "back.json"
        back_json = EXAMPLE.read_text(encoding="utf-8").replace('"side": "front"', '"side": "back"')
        back_spec.write_text(back_json, encoding="utf-8")
        back_output = root / "back.kicad_mod"
        back_result = invoke(
            binary,
            "component",
            "footprint",
            "generate",
            str(back_spec),
            "--output",
            str(back_output),
            "--json",
        )
        assert back_result.returncode != 0 and "board-side flipping belongs to placement" in back_result.stderr
        assert not back_output.exists()

        audit_library = root / "audit.pretty"
        audit_library.mkdir()
        audit_sources = {
            "A.kicad_mod": '(footprint "Shared")\n',
            "B.kicad_mod": '(footprint "Shared")\n',
            "WrongFile.kicad_mod": '(footprint "InternalOther")\n',
            "Malformed.kicad_mod": '(footprint "broken"',
            "Legacy.kicad_mod": '(module "legacy")\n',
            "notes.txt": "ignored library note\n",
        }
        for name, source in audit_sources.items():
            (audit_library / name).write_text(source, encoding="utf-8")
        audit_hashes_before = {
            path.name: hashlib.sha256(path.read_bytes()).hexdigest()
            for path in audit_library.iterdir()
        }
        audit_args = (
            "component", "footprint", "library", "audit", str(audit_library), "--json"
        )
        audit_result = invoke(binary, *audit_args)
        assert audit_result.returncode == 0, audit_result.stderr
        audit_report = json.loads(audit_result.stdout)
        assert audit_report["schema"] == "tokito.footprint-library-audit.v1"
        assert audit_report["status"] == "observations-present"
        assert audit_report["footprintFileCount"] == 5
        assert audit_report["ignoredEntryCount"] == 1
        assert audit_report["ignoredEntries"] == ["notes.txt"]
        assert len(audit_report["inventorySha256"]) == 64
        assert audit_report["parsedFootprintCount"] == 3
        assert audit_report["semanticValidationPerformed"] is False
        assert audit_report["geometryValidationPerformed"] is False
        assert audit_report["libraryModified"] is False
        findings = audit_report["findings"]
        assert [item["code"] for item in findings] == [
            "asset_not_parsed", "asset_not_parsed",
            "duplicate_internal_name",
            "filename_name_mismatch", "filename_name_mismatch", "filename_name_mismatch",
        ]
        duplicate = next(item for item in findings if item["code"] == "duplicate_internal_name")
        assert duplicate["footprintName"] == "Shared"
        assert duplicate["fileNames"] == ["A.kicad_mod", "B.kicad_mod"]
        assert {item["parseStatus"] for item in findings if item["code"] == "asset_not_parsed"} == {
            "invalid_sexpr", "unsupported_root"
        }
        assert json.loads(invoke(binary, *audit_args).stdout) == audit_report
        audit_hashes_after = {
            path.name: hashlib.sha256(path.read_bytes()).hexdigest()
            for path in audit_library.iterdir()
        }
        assert audit_hashes_after == audit_hashes_before
    print("footprint spec E2E passed: explicit source, stable pads/graphics, unregistered .pretty package, deterministic inventory/identity locks and library audit, strict lock validation, drift/missing/added/changed detection, canonical front side, refusal and no overwrite")


if __name__ == "__main__":
    main()
