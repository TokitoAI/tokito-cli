#!/usr/bin/env python3
"""Run the component-envelope slice against the pinned local dataset snapshot."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from glb_e2e import invoke, parse_glb

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_DATASET_COMMIT = "257ea7e68ee2e700522bd1b19ad9cbf1c0c3980a"
EXPECTED_UUID = "a1cba6f8-17e5-4d81-9ffa-833453b7914e"
EXPECTED_BOM_SHA256 = "2fa8ea7d6edd0705a82007e60cf2c15de6360546af89ad30f867333f5d994156"
EXPECTED_BOARD_SHA256 = "c0d1389ce0710539104a3f2c4ca5f382c6b5fa2cce0edde315b4d73360f75b26"
PACKAGE_URL = "https://www.nxp.com/docs/en/package-information/SOT782-1.pdf"


def output_path(value: Path, allowed_root: Path) -> Path:
    target_root = allowed_root.resolve()
    absolute = value.absolute()
    cursor = absolute
    while cursor != cursor.parent:
        if cursor.exists() and cursor.is_symlink():
            raise ValueError(f"linked output path is not allowed: {cursor}")
        cursor = cursor.parent
    resolved = absolute.resolve()
    if not resolved.is_relative_to(target_root):
        raise ValueError("dataset E2E outputs must remain under the selected local output root")
    if resolved.exists():
        raise FileExistsError(f"refusing to reuse existing dataset E2E output: {resolved}")
    return resolved


def run(binary: Path, dataset_repo: Path, bom_report: Path, out_dir: Path, output_root: Path) -> None:
    dataset_repo = dataset_repo.resolve(strict=True)
    board = dataset_repo / "data/board-sources/nxp-frdm-mcxn236/generated-kicad/FRDM-MCXN236.imported.kicad_pcb"
    if not board.is_file():
        raise FileNotFoundError(f"expected imported FRDM-MCXN236 board is missing: {board}")
    head = subprocess.run(
        ["git", "-C", str(dataset_repo), "rev-parse", "HEAD"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    if head != EXPECTED_DATASET_COMMIT:
        raise AssertionError(f"dataset revision mismatch: expected {EXPECTED_DATASET_COMMIT}, got {head}")
    bom_path = bom_report.resolve(strict=True)
    bom_bytes = bom_path.read_bytes()
    bom_report_file_sha256 = hashlib.sha256(bom_bytes).hexdigest()
    bom = json.loads(bom_bytes.decode("utf-8"))
    try:
        bom_report_label = bom_path.relative_to(ROOT).as_posix()
    except ValueError:
        # Keep machine-specific paths out of a portable delivery receipt.
        bom_report_label = "external-report"
    assert bom["schema"] == "tokito.bom-reconciliation.v1"
    assert bom["bomSourceSha256"] == EXPECTED_BOM_SHA256
    assert bom["boardSourceSha256"] == EXPECTED_BOARD_SHA256
    matches = [row for row in bom["rows"] if "U11" in row.get("references", [])]
    assert len(matches) == 1
    row = matches[0]
    assert row["sourceFields"]["source_columns"]["MFG"] == "NXP Semiconductors"
    assert row["sourceFields"]["source_columns"]["MFG_PN"] == "TJA1057GTK/3Z"
    joins = [join for join in row["referenceJoins"] if join["reference"] == "U11"]
    assert len(joins) == 1 and joins[0]["status"] == "matched_identity"
    assert joins[0]["boardUuid"] == EXPECTED_UUID

    out_dir = output_path(out_dir, output_root)
    out_dir.mkdir(parents=True)
    template_path = out_dir / "model-assignments.template.json"
    invoke(binary, "model", "assignments", "template", board, "--output", template_path)
    manifest = json.loads(template_path.read_text(encoding="utf-8"))
    assert manifest["sourceBoardSha256"] == EXPECTED_BOARD_SHA256
    selected = [item for item in manifest["assignments"] if item.get("reference") == "U11"]
    assert len(selected) == 1 and selected[0]["componentUuid"] == EXPECTED_UUID
    selected_assignment = selected[0]

    spec = {
        "schema": "tokito.component-envelope.v1",
        "board": {"sha256": manifest["sourceBoardSha256"]},
        "component": {
            "reference": "U11",
            "uuid": EXPECTED_UUID,
            "manufacturer": "NXP Semiconductors",
            "mpn": "TJA1057GTK/3Z",
            "package": "SOT782-1 / HVSON8",
        },
        "evidence": {
            "datasetCommit": head,
            "bomSourceSha256": EXPECTED_BOM_SHA256,
            "manufacturerUrl": PACKAGE_URL,
        },
        "bodyMm": {"length": 3.0, "width": 3.0, "height": 0.85},
    }
    spec_path = out_dir / "tja1057gtk3-u11-envelope.json"
    spec_path.write_text(json.dumps(spec, indent=2) + "\n", encoding="utf-8")
    model_path = out_dir / "tja1057gtk3-u11-envelope.wrl"
    model_report = json.loads(invoke(
        binary,
        "component", "model", "envelope", spec_path,
        "--board", board, "--output", model_path, "--json",
    ).stdout)
    assert model_report["status"] == "candidate-envelope-only"
    assert model_report["sourceClaims"]["component"]["uuid"] == EXPECTED_UUID
    assert model_report["bodyMm"] == {"units": "mm", "length": 3.0, "width": 3.0, "height": 0.85}

    mesh_report = json.loads(invoke(
        binary, "model", "inspect", model_path, "--units", "millimetres", "--json",
    ).stdout)
    assert mesh_report["meshParts"] == 1 and mesh_report["triangles"] == 12
    bounds = mesh_report["localBoundsMm"]
    spans = [bounds["max"][i] - bounds["min"][i] for i in range(3)]
    assert sorted(round(span, 6) for span in spans) == [0.85, 3.0, 3.0]

    model_bytes = model_path.read_bytes()
    selected_assignment["evidence"] = (
        "NXP package nominal dimensions; candidate cuboid only; "
        "physical population, pin-one, placement and fit remain unverified"
    )
    selected_assignment["model"] = {
        "path": model_path.name,
        "sha256": hashlib.sha256(model_bytes).hexdigest(),
        "units": "millimetres",
        "scale": [1.0, 1.0, 1.0],
        "rotationDeg": [0.0, 0.0, 0.0],
        "offsetMm": [0.0, 0.0, 0.0],
    }
    manifest_path = out_dir / "model-assignments.u11.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    assignment_report = json.loads(invoke(
        binary, "model", "assignments", "verify", board,
        "--manifest", manifest_path, "--project-root", out_dir, "--json",
    ).stdout)
    assert assignment_report["sourceIdentityBound"] is True
    assert assignment_report["physicalQualificationVerified"] is False
    assert assignment_report["assignedModelCount"] == 1
    assert assignment_report["resolvedAssetCount"] == 1
    assert assignment_report["supportedGeometryCount"] == 1
    u11_assignment = [entry for entry in assignment_report["assignments"] if entry["reference"] == "U11"]
    assert len(u11_assignment) == 1 and u11_assignment[0]["componentUuid"] == EXPECTED_UUID
    assert u11_assignment[0]["modelPhysicalMatchVerified"] is False

    glb_path = out_dir / "frdm-mcxn236-board-shell-only.glb"
    glb_report = json.loads(invoke(
        binary,
        "export", "glb", board,
        "--model-assignments", manifest_path,
        "--project-root", out_dir,
        "--output", glb_path,
        "--allow-partial", "--json-report",
    ).stdout)
    assert glb_report["partialExport"] is True
    assert glb_report["placementVerified"] is False
    assert glb_report["coverage"]["componentModelsEmitted"] == 0
    u11_export = [entry for entry in glb_report["assignments"]["components"] if entry["reference"] == "U11"]
    assert len(u11_export) == 1
    assert u11_export[0]["status"] == "missing_stackup_or_outline"
    doc, _ = parse_glb(glb_path)
    assert [node.get("name") for node in doc["nodes"]] == ["Board shell"]

    result = {
        "schema": "tokito.component-dataset-e2e.v1",
        "datasetCommit": head,
        "boardFileSha256": manifest["sourceBoardSha256"],
        "bomReportPath": bom_report_label,
        "bomReportFileSha256": bom_report_file_sha256,
        "bomSourceSha256Claim": bom["bomSourceSha256"],
        "reference": "U11",
        "componentUuid": EXPECTED_UUID,
        "mpn": "TJA1057GTK/3Z",
        "bodyNominalMm": [3.0, 3.0, 0.85],
        "generated": {
            "spec": spec_path.name,
            "wrl": {"file": model_path.name, "sha256": hashlib.sha256(model_bytes).hexdigest()},
            "glb": {"file": glb_path.name, "sha256": hashlib.sha256(glb_path.read_bytes()).hexdigest()},
        },
        "qualification": {
            "status": "candidate-envelope-only",
            "nominalBoundsMatchSpec": True,
            "bomSourceIndependentlyRehashed": False,
            "physicalBoardRevisionVerified": False,
            "physicalPopulationVerified": False,
            "pinOneOrientationVerified": False,
            "placementVerified": False,
            "packageFitVerified": False,
            "footprintRegenerated": False,
            "partialGlbConsent": True,
            "datasetComponentEmittedInGlb": False,
        },
        "limitations": [
            "The imported footprint's existing custom pads are retained; no .kicad_mod is generated.",
            "The cuboid expresses the published nominal package envelope only; terminals, chamfers, pin-one detail and true surfaces are absent.",
            "The BOM report's source SHA is carried as evidence but the original BOM source bytes were not independently rehashed in this local checkout.",
            "The BOM join does not establish population or photographed-specimen revision correspondence.",
            "The imported PCB has no canonical stackup object, which is a confirmed placement blocker. The current exporter reports the combined missing_stackup_or_outline condition; this E2E does not establish that stackup is the only unmet placement precondition. Although the dataset's generated Gerber job records material stackup, no qualified adapter reconciles it to this PCB hash, so the output remains shell-only.",
        ],
        "assignedExport": {
            "componentModelsEmitted": glb_report["coverage"]["componentModelsEmitted"],
            "u11Status": u11_export[0]["status"],
            "glbContainsBoardShellOnly": True,
        },
    }
    result_path = out_dir / "e2e-receipt.json"
    result_path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    try:
        output_label = out_dir.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        output_label = "external-output"
    print(
        "PASS FRDM-MCXN236 U11 candidate/identity E2E "
        f"(GLB correctly withheld from U11): {output_label}"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--dataset-repo", type=Path, required=True)
    parser.add_argument("--bom-report", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, default=ROOT / "target")
    args = parser.parse_args()
    run(args.binary.resolve(strict=True), args.dataset_repo, args.bom_report, args.output_dir, args.output_root)
