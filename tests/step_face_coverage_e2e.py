"""Independent face-ID inventory comparison for three pinned official STEP assets.

This single-solid fixture comparison proves membership only, not tessellation,
surface trimming, solid validity, placement, units or production admission.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess

PIN = "a0244fe3442823dbb052ebc4820b4c2951e1742c"
EXPECTED_COUNTS = {
    "RF_Module.3dshapes/ESP32-WROOM-32.step": 751,
    "Connector_USB.3dshapes/USB_Micro-B_Molex_47346-0001.step": 484,
    "Button_Switch_SMD.3dshapes/SW_SPST_B3U-1000P.step": 81,
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", required=True, type=Path)
    parser.add_argument("--asset-root", required=True, type=Path)
    args = parser.parse_args()
    intake = json.loads((args.asset_root / "intake.json").read_text(encoding="utf-8-sig"))
    assert intake["sourceCommit"] == PIN
    assert {asset["path"] for asset in intake["assets"]} == set(EXPECTED_COUNTS)
    for asset in intake["assets"]:
        path = args.asset_root / asset["path"]
        assert path.is_file() and not path.is_symlink()
        assert path.stat().st_size <= 32 * 1024 * 1024
        raw = path.read_bytes()
        assert hashlib.sha256(raw).hexdigest() == asset["sourceSha256"]
        # These pinned text fixtures each have exactly one emitted solid. The
        # independent inventory scans all explicit advanced-face declarations,
        # rather than using the Rust topology traversal or its summary counts.
        face_ids = [int(value) for value in re.findall(rb"(?m)^#(\d+)\s*=\s*ADVANCED_FACE\s*\(", raw)]
        assert len(face_ids) == len(set(face_ids)) == EXPECTED_COUNTS[asset["path"]]
        expected_digest = hashlib.sha256(b"".join(struct.pack("<Q", value) for value in sorted(face_ids))).hexdigest()
        result = subprocess.run([str(args.probe.resolve()), str(path.resolve())], capture_output=True, text=True, encoding="utf-8", timeout=60, check=False)
        assert result.returncode == 2, result.stderr
        report = json.loads(result.stdout)
        assert report["sourceSha256"] == asset["sourceSha256"]
        assert report["admitted"] is False and report["exportsProduced"] == []
        assert report["sourceFaceCompletenessVerified"] is False
        coverage = report["sourceFaceCoverage"]
        assert coverage["shapesInspected"] == len(coverage["shapeDetails"]) == 1
        detail = coverage["shapeDetails"][0]
        assert detail["expectedSourceFaceIdsSha256"] == expected_digest
        assert detail["expectedSourceFaces"] == len(face_ids)
        assert detail["sourceFacePresenceMatches"] is True
        assert detail["missingSourceFaceCount"] == detail["unexpectedSourceFaceCount"] == 0
        assert detail["geometryQualified"] is False
        assert coverage["allSourceShapesCovered"] is False
        print(f"PASS {path.name}: {len(face_ids)} independently inventoried faces present; no geometry admission")


if __name__ == "__main__":
    main()
