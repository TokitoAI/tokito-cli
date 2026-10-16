"""Digest-bound negative regression for the pinned ESP8266 model-16 spline face."""
import argparse
import json
from pathlib import Path

from bounded_fingerprint import fingerprint
from surface_reference_e2e import capture_bounded

SOURCE_SHA256 = "7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f"
EXPECTED_COUNTS = {
    "coincident_positions_distinct_indices": 520,
    "relative_near_zero_exactly_noncollinear": 582,
}


def run(binary: Path, source: Path, output: Path) -> dict:
    if output.exists():
        raise ValueError("output must be new")
    output.mkdir(parents=True)
    tracked = [(binary, 64 * 1024 * 1024), (source, 32 * 1024 * 1024), (Path(__file__), 1024 * 1024)]
    before = {str(path): fingerprint(path, limit) for path, limit in tracked}
    receipt = {
        "schema": "tokito.step-model16-face-diagnostic-e2e.v1",
        "passed": False,
        "admitted": False,
        "exportsProduced": [],
        "inputSha256": before,
        "scope": "part-local reader-triangle observation; no repair, source-surface evaluation or admission",
    }
    try:
        if before[str(source)] != SOURCE_SHA256:
            raise ValueError("pinned model-16 source digest mismatch")
        code, stdout, stderr = capture_bounded(
            [str(binary), "--diagnose-face", "2875", str(source)],
            timeout_seconds=300,
            stdout_limit=8 * 1024 * 1024,
            stderr_limit=512 * 1024,
        )
        (output / "diagnostic.json").write_bytes(stdout)
        if code != 2 or stderr:
            raise ValueError("diagnostic subprocess must return refusal status 2 without stderr")
        report = json.loads(stdout)
        face = report.get("experimentalFaceDiagnostics", {})
        receipt["diagnosticSummary"] = {
            "faceId": face.get("faceId"),
            "selectedFaceTriangles": face.get("selectedFaceTriangles"),
            "degenerateOrNearZeroTriangles": face.get("degenerateOrNearZeroTriangles"),
            "classificationCounts": face.get("classificationCounts"),
            "coordinateFrame": face.get("coordinateFrame"),
            "readerReportedMetresPerModelUnit": face.get("readerReportedMetresPerModelUnit"),
        }
        if report.get("sourceSha256") != SOURCE_SHA256:
            raise ValueError("CLI report source digest mismatch")
        if report.get("admitted") is not False or report.get("exportsProduced") != []:
            raise ValueError("diagnostic unexpectedly admitted geometry or produced exports")
        if (
            face.get("faceId") != 2875
            or face.get("selectedFaceTriangles") != 2506
            or face.get("degenerateOrNearZeroTriangles") != 1102
            or face.get("classificationCounts") != EXPECTED_COUNTS
            or face.get("coordinateFrame") != "part_local_reader_model_units"
            or face.get("geometryQualified") is not False
            or face.get("admitted") is not False
            or face.get("uvCoordinatesAvailable") is not False
            or face.get("surfaceCauseEstablished") is not False
        ):
            raise ValueError("pinned face diagnostic did not match the expected bounded observation")
        if any(fingerprint(path, limit) != before[str(path)] for path, limit in tracked):
            raise ValueError("source, binary, or diagnostic test changed during execution")
        receipt["diagnosticSha256"] = fingerprint(output / "diagnostic.json", 8 * 1024 * 1024)
        receipt["passed"] = True
    except Exception as error:
        receipt["error"] = f"{type(error).__name__}: {error}"
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = run(args.binary, args.source, args.output)
    print(json.dumps({k: v for k, v in result.items() if k in ["passed", "error", "diagnosticSummary"]}))
    raise SystemExit(0 if result["passed"] else 2)
