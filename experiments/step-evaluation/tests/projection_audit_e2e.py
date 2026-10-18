"""Pinned development-only FreeCAD projection comparison and refusal tests."""
import argparse
import hashlib
import json
import math
from pathlib import Path

from surface_reference_e2e import bounded_read, capture_bounded

SOURCE_SHA256 = "7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f"
INDICES = [0, 1534, 1535, 1536]


def digest(path, maximum):
    return hashlib.sha256(bounded_read(path, maximum)).hexdigest()


def validate_report(report):
    if report.get("schema") != "tokito.freecad-source-projection-audit.v1":
        raise ValueError("unexpected audit schema")
    if report.get("sourceSha256") != SOURCE_SHA256 or report.get("sourceFaceId") != 2875:
        raise ValueError("wrong source or face identity")
    if report.get("geometryQualified") is not False or report.get("admitted") is not False or report.get("exportsProduced") != []:
        raise ValueError("research report must not admit or export geometry")
    if report.get("globalClosestPointProven") is not False:
        raise ValueError("numerical projection must not claim global proof")
    if report.get("sourceBoundarySignatureCount") != 100 or report.get("maximumBoundarySignatureDistanceMm", math.inf) > 1e-7:
        raise ValueError("independent boundary signature mismatch")
    rows = report.get("probes", [])
    if [row.get("qualityProbeIndex") for row in rows] != INDICES:
        raise ValueError("probe identity/order mismatch")
    for row in rows:
        if not 0 <= row["occtVsRustSurfaceEvaluatorDistanceMm"] < 1e-12:
            raise ValueError("independent surface evaluations disagree")
        rust = row["rustResidualMm"]
        support = row["occtNaturalSupportFaceDistanceMm"]
        if not all(math.isfinite(value) and value >= 0 for value in (rust, support)):
            raise ValueError("nonfinite or negative residual")
        if not math.isclose(rust, support, rel_tol=1e-6, abs_tol=1e-12):
            raise ValueError("Rust/support-surface projection disagreement")
    if rows[0]["rustProjectionConverged"] is not True or rows[0]["rustResidualMm"] > 1e-12:
        raise ValueError("on-surface positive control failed")
    failed = rows[2]
    if failed["rustProjectionConverged"] is not False or not 1e-7 < failed["rustResidualMm"] < 2e-6:
        raise ValueError("pinned nonzero residual control changed")
    if failed["occtTrimmedFaceDistanceMm"] > 1e-12 or failed["occtParameterRoundtripDistanceMm"] < 1e-7:
        raise ValueError("trimmed topology versus supporting-surface distinction missing")
    if not 30 < failed["metricAtRustUv"]["jacobianAnisotropy"] < 50:
        raise ValueError("pinned parameter-metric regression changed")


def run(args):
    source = args.source.resolve(strict=True)
    diagnostic = args.diagnostic.resolve(strict=True)
    freecad = args.freecad.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    macro = Path(__file__).resolve().parents[1] / "references/audit_model16_projection.FCMacro"
    receipt = {"schema": "tokito.projection-audit-e2e.v1", "passed": False,
               "geometryQualified": False, "admitted": False, "exportsProduced": [], "checks": []}
    try:
        if digest(source, 32 * 1024 * 1024) != SOURCE_SHA256:
            raise ValueError("pinned source mismatch")
        receipt.update({"sourceSha256": SOURCE_SHA256,
                        "diagnosticSha256": digest(diagnostic, 32 * 1024 * 1024),
                        "auditScriptSha256": digest(macro, 128 * 1024),
                        "freecadExecutableSha256": digest(freecad, 128 * 1024 * 1024)})
        config = {"source": str(source), "diagnostic": str(diagnostic),
                  "probeIndices": INDICES, "output": str(output / "audit.json")}

        def execute(name, cfg):
            path = output / (name + ".config.json")
            path.write_text(json.dumps(cfg), encoding="utf-8")
            code, stdout, stderr = capture_bounded(
                [str(freecad), str(macro), "--pass", str(path)], 120, 128 * 1024, 128 * 1024
            )
            (output / (name + ".stdout.txt")).write_bytes(stdout)
            (output / (name + ".stderr.txt")).write_bytes(stderr)
            return code, stderr.decode("utf-8", errors="strict")

        code, _ = execute("positive", config)
        if code != 0:
            raise ValueError("FreeCAD positive execution failed")
        report = json.loads(bounded_read(output / "audit.json", 128 * 1024))
        validate_report(report)
        if report["auditScriptSha256"] != receipt["auditScriptSha256"] or report["diagnosticSha256"] != receipt["diagnosticSha256"]:
            raise ValueError("audit input fingerprints changed")
        receipt["checks"].append({"name": "independent-four-probe-comparison", "passed": True})
        report_digest = digest(output / "audit.json", 128 * 1024)
        _, error = execute("no-overwrite", config)
        if "refusing to overwrite output" not in error or digest(output / "audit.json", 128 * 1024) != report_digest:
            raise ValueError("no-overwrite refusal failed")
        receipt["checks"].append({"name": "no-overwrite", "passed": True})
        bad_indices = {**config, "probeIndices": [0, 0], "output": str(output / "duplicate-must-not-exist.json")}
        _, error = execute("duplicate-indices", bad_indices)
        if "unique probe indices" not in error or Path(bad_indices["output"]).exists():
            raise ValueError("duplicate-index refusal failed")
        receipt["checks"].append({"name": "duplicate-indices", "passed": True})
        bad_diagnostic = json.loads(bounded_read(diagnostic, 32 * 1024 * 1024))
        bad_diagnostic["sourceSha256"] = "0" * 64
        bad_path = output / "wrong-source-diagnostic.json"
        bad_path.write_text(json.dumps(bad_diagnostic), encoding="utf-8")
        bad_source = {**config, "diagnostic": str(bad_path), "output": str(output / "mismatch-must-not-exist.json")}
        _, error = execute("source-mismatch", bad_source)
        if "pinned source digest mismatch" not in error or Path(bad_source["output"]).exists():
            raise ValueError("source-mismatch refusal failed")
        receipt["checks"].append({"name": "source-mismatch", "passed": True})
        if digest(source, 32 * 1024 * 1024) != SOURCE_SHA256 or digest(macro, 128 * 1024) != receipt["auditScriptSha256"]:
            raise ValueError("source or audit script changed during execution")
        receipt["reportSha256"] = report_digest
        receipt["passed"] = True
    except Exception as error:
        receipt["error"] = str(error)
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2, allow_nan=False), encoding="utf-8")
    print(json.dumps(receipt))
    return 0 if receipt["passed"] else 2


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--freecad", required=True, type=Path)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--diagnostic", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    raise SystemExit(run(parser.parse_args()))
