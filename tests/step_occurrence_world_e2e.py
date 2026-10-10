#!/usr/bin/env python3
"""Reconcile bounded STEP world-occurrence bounds to pinned FreeCAD reports.

This checks the Rust reader's product-occurrence transforms and one-time unit
normalization against externally evaluated reference bounds. It does not
qualify surface fidelity, Altium association, PCB placement, or physical fit.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MODELS = ROOT / "target/dataset-intake/esp8266-pr6-5283bc0-source-candidate/data/board-sources/esp8266-nodemcu-reference/structured-source/embedded-models"
DEFAULT_REFERENCE = ROOT / "target/github-delivery/esp8266-step-preflight/freecad-reference.json"
DEFAULT_SOURCE_RECEIPT = ROOT / "target/github-delivery/esp8266-step-preflight/receipt.json"
EXPECTED_COMMIT = "5283bc059e30e51f4d3a81a674425936fc213214"
MODELS = ("0.step", "14.step", "16.step")
MAX_STEP_BYTES = 32 * 1024 * 1024


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def compare_bounds(actual, expected, tolerance):
    differences = []
    for side in ("min", "max"):
        for axis in range(3):
            left, right = actual[side][axis], expected[side][axis]
            if not isinstance(left, (int, float)) or not math.isfinite(left):
                raise AssertionError(f"non-finite reader {side}[{axis}]")
            if not isinstance(right, (int, float)) or not math.isfinite(right):
                raise AssertionError(f"non-finite reference {side}[{axis}]")
            difference = abs(left - right)
            differences.append(difference)
            if difference > tolerance:
                raise AssertionError(
                    f"world bound mismatch {side}[{axis}]: reader={left}, "
                    f"FreeCAD={right}, delta={difference} mm > {tolerance} mm"
                )
    return differences


def run(binary: Path, models_root: Path, reference_path: Path, source_receipt_path: Path, output: Path, tolerance: float):
    if not math.isfinite(tolerance) or tolerance <= 0:
        raise ValueError("bounds tolerance must be finite and positive")
    if output.exists():
        raise FileExistsError(f"refusing to overwrite evidence folder: {output}")
    output.mkdir(parents=True)
    reference = json.loads(reference_path.read_text(encoding="utf-8"))
    source_receipt = json.loads(source_receipt_path.read_text(encoding="utf-8"))
    if source_receipt.get("sourceCommit") != EXPECTED_COMMIT or source_receipt.get("passed") is False:
        raise AssertionError("source report is not pinned to the expected dataset commit")
    source_models = {row["model"]: row for row in source_receipt.get("models", [])}
    reference_models = {row["file"]: row for row in reference.get("models", [])}
    results = []
    for filename in MODELS:
        source = models_root / filename
        if source.is_symlink() or not source.is_file() or source.stat().st_size > MAX_STEP_BYTES:
            raise AssertionError(f"missing, linked or over-limit STEP input: {source}")
        source_hash = sha256(source)
        expected = reference_models.get(filename)
        source_row = source_models.get(filename)
        reported_hash = source_row.get("sourceSha256", source_row.get("sha256")) if source_row else None
        if (
            not expected
            or expected.get("sourceSha256") != source_hash
            or not source_row
            or reported_hash != source_hash
        ):
            raise AssertionError(f"FreeCAD reference does not bind source bytes for {filename}")
        completed = subprocess.run(
            [str(binary), str(source)],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="strict",
            timeout=300,
            check=False,
        )
        if completed.returncode != 2:
            raise AssertionError(f"expected experimental refusal exit code 2 for {filename}, got {completed.returncode}: {completed.stderr[-1000:]}")
        try:
            report = json.loads(completed.stdout)
        except json.JSONDecodeError as error:
            raise AssertionError(f"invalid STEP probe JSON for {filename}: {error}") from error
        occurrence = report.get("assemblyOccurrenceWorld", {})
        if occurrence.get("available") is False or not occurrence.get("transformCompositionResolved"):
            raise AssertionError(f"occurrence-world audit unavailable for {filename}: {occurrence}")
        if occurrence.get("independentRepresentationUnitsResolved") is not True:
            raise AssertionError(f"independent unit audit missing for {filename}")
        if occurrence.get("readerScaleAgreesWithIndependentUnits") is not True:
            raise AssertionError(f"reader/unit preflight disagreement for {filename}")
        differences = compare_bounds(
            occurrence["worldBoundsMm"], expected["boundsMm"], tolerance
        )
        if report.get("admitted") is not False or report.get("exportsProduced") != []:
            raise AssertionError(f"experimental STEP probe unexpectedly admitted/exported {filename}")
        for field in (
            "surfaceFidelityQualified",
            "altiumComponentAssociationQualified",
            "pcbPlacementQualified",
            "boundsQualifiedAgainstExternalReference",
        ):
            if occurrence.get(field) is not False:
                raise AssertionError(f"unsafe qualification field {field} for {filename}")
        row = {
            "file": filename,
            "sourceSha256": source_hash,
            "probeExitCode": completed.returncode,
            "readerOccurrenceCount": occurrence["occurrenceCount"],
            "readerOccurrenceTriangles": occurrence["occurrenceTriangles"],
            "readerWorldBoundsMm": occurrence["worldBoundsMm"],
            "freecadWorldBoundsMm": expected["boundsMm"],
            "maxAbsoluteBoundDeltaMm": max(differences),
            "withinTolerance": True,
            "physicalGeometryQualified": False,
            "assemblyPoseQualified": False,
        }
        results.append(row)
        (output / f"{Path(filename).stem}-occurrence-world.json").write_text(
            json.dumps(report, indent=2) + "\n", encoding="utf-8"
        )
    receipt = {
        "passed": True,
        "datasetSourceCommit": EXPECTED_COMMIT,
        "freecadVersion": reference.get("freecadVersion"),
        "freecadReferenceSha256": sha256(reference_path),
        "sourceProbeReceiptSha256": sha256(source_receipt_path),
        "boundsToleranceMm": tolerance,
        "models": results,
        "scope": "independent numeric world-bounds comparison; no surface, Altium placement, photographed-board or AR qualification",
        "geometryQualified": False,
        "assemblyPoseQualified": False,
    }
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print("PASS STEP occurrence-world comparison: " + ", ".join(
        f"{row['file']} max delta {row['maxAbsoluteBoundDeltaMm']:.6g} mm"
        for row in results
    ))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--models-root", type=Path, default=DEFAULT_MODELS)
    parser.add_argument("--reference", type=Path, default=DEFAULT_REFERENCE)
    parser.add_argument("--source-receipt", type=Path, default=DEFAULT_SOURCE_RECEIPT)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--bounds-tolerance-mm", type=float, default=0.05)
    args = parser.parse_args()
    run(
        args.binary.resolve(strict=True),
        args.models_root.resolve(strict=True),
        args.reference.resolve(strict=True),
        args.source_receipt.resolve(strict=True),
        args.output.resolve(),
        args.bounds_tolerance_mm,
    )
