"""Actual offline CLI source locking, mutation and hostile-input refusals."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

from glb_e2e import invoke as process_invoke

ROOT = Path(__file__).resolve().parents[1]


def invoke(binary, *args, success=True):
    result = process_invoke(binary, *args, succeeds=success)
    assert result.returncode == (0 if success else 2), result.stderr
    return result.stdout


def run(binary):
    with tempfile.TemporaryDirectory(prefix="dataset-lock-", dir=ROOT / "target") as temp:
        root = Path(temp)
        snapshot = root / "snapshot"
        snapshot.mkdir()
        source = ROOT / "tests/fixtures/assembly-e2e/single-resistor.kicad_pcb"
        payload = source.read_bytes()
        (snapshot / "board.kicad_pcb").write_bytes(payload)
        (snapshot / "unlisted-user-file.txt").write_text("preserve me", encoding="utf-8")
        request = {"schema": "tokito.dataset-source-request.v1", "repository": "https://github.com/TokitoAI/tokito-app-dataset",
                   "revision": "257ea7e68ee2e700522bd1b19ad9cbf1c0c3980a", "assets": [
                       {"boardId": "fixture", "role": "pcb", "path": "board.kicad_pcb", "provenance": "test fixture, not dataset board"}]}
        spec = root / "request.json"
        lock = root / "source-lock.json"
        def save(value):
            spec.write_text(json.dumps(value), encoding="utf-8")
        save(request)
        result = json.loads(invoke(binary, "dataset", "lock", snapshot, "--request", spec, "--output", lock))
        assert result == json.loads(lock.read_text())
        assert len(result["assets"]) == 1
        assert result["assets"][0]["bytes"] == len(payload)
        assert result["assets"][0]["sha256"] == hashlib.sha256(payload).hexdigest()
        verified = json.loads(invoke(binary, "dataset", "verify", snapshot, "--lock", lock))
        assert verified["status"] == "passed"
        assert verified["inventory"] == {"fixture": {"pcb": 1}}
        assert verified["scope"] == "listed_assets_only"
        for claim in ["revisionAuthenticated", "physicalFitVerified", "componentCrosswalkVerified", "geometryQualified", "appAdmissionAuthorized"]:
            assert verified[claim] is False
        invoke(binary, "dataset", "lock", snapshot, "--request", spec, "--output", lock, success=False)
        second = root / "second.json"
        invoke(binary, "dataset", "lock", snapshot, "--request", spec, "--output", second)
        assert second.read_bytes() == lock.read_bytes()
        for replacement in [b"changed content", b"version https://git-lfs.github.com/spec/v1\noid sha256:" + b"0" * 64 + b"\nsize 123\n"]:
            (snapshot / "board.kicad_pcb").write_bytes(replacement)
            refused = json.loads(invoke(binary, "dataset", "verify", snapshot, "--lock", lock, success=False))
            assert refused["status"] == "refused" and refused["assets"][0]["status"] == "failed"
        invoke(binary, "dataset", "lock", snapshot, "--request", spec, "--output", root / "pointer.json", success=False)
        assert not (root / "pointer.json").exists()
        (snapshot / "board.kicad_pcb").unlink()
        assert json.loads(invoke(binary, "dataset", "verify", snapshot, "--lock", lock, success=False))["status"] == "refused"
        (snapshot / "board.kicad_pcb").write_bytes(payload)
        for path in ["../board.kicad_pcb", "/board.kicad_pcb", "C:/board.kicad_pcb", "folder\\board.kicad_pcb", "NUL.txt", "board.kicad_pcb."]:
            bad = json.loads(json.dumps(request))
            bad["assets"][0]["path"] = path
            save(bad)
            invoke(binary, "dataset", "lock", snapshot, "--request", spec, "--output", root / "bad.json", success=False)
        bad = json.loads(json.dumps(request))
        bad["assets"].append(dict(bad["assets"][0], path="BOARD.kicad_pcb"))
        save(bad)
        invoke(binary, "dataset", "lock", snapshot, "--request", spec, "--output", root / "bad.json", success=False)
        bad = dict(request, revision="main")
        save(bad)
        invoke(binary, "dataset", "lock", snapshot, "--request", spec, "--output", root / "bad.json", success=False)
        spec.write_text('{"schema":"x","schema":"y"}', encoding="utf-8")
        invoke(binary, "dataset", "lock", snapshot, "--request", spec, "--output", root / "bad.json", success=False)
        assert not (root / "bad.json").exists()
        assert (snapshot / "unlisted-user-file.txt").read_text() == "preserve me"
        assert "USAGE" in invoke(binary, "help", "dataset")
        # A valid request can expand beyond the lock parser limit when hashes
        # and pretty formatting are added. Refuse before creating any output.
        large = dict(request, assets=[])
        for index in range(1900):
            path = f"asset-{index}"
            (snapshot / path).write_bytes(b"")
            large["assets"].append({"boardId": "fixture", "role": "metadata", "path": path, "provenance": "x" * 2000})
        save(large)
        assert spec.stat().st_size < 4 * 1024 * 1024
        oversized = root / "oversized-lock.json"
        refused = process_invoke(binary, "dataset", "lock", snapshot, "--request", spec, "--output", oversized, succeeds=False)
        assert refused.returncode == 2 and "generated dataset lock exceeds" in refused.stderr
        assert not oversized.exists()
        linked = root / "linked-snapshot"
        env = dict(os.environ, TOKITO_DATASET_TEST_LINK=str(linked), TOKITO_DATASET_TEST_TARGET=str(snapshot))
        if os.name == "nt":
            subprocess.run(["pwsh", "-NoProfile", "-Command", "New-Item -ItemType Junction -Path $env:TOKITO_DATASET_TEST_LINK -Target $env:TOKITO_DATASET_TEST_TARGET | Out-Null"], env=env, check=True, capture_output=True)
        else:
            linked.symlink_to(snapshot, target_is_directory=True)
        try:
            invoke(binary, "dataset", "verify", linked, "--lock", lock, success=False)
        finally:
            if os.name == "nt":
                subprocess.run(["pwsh", "-NoProfile", "-Command", "Remove-Item -LiteralPath $env:TOKITO_DATASET_TEST_LINK -Force"], env=env, check=True, capture_output=True)
            else:
                linked.unlink()
    print("PASS dataset source-lock E2E: real PCB bytes, deterministic hashes, selective inventory, mutations, pointers, missing files, path and duplicate refusals")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    run(parser.parse_args().binary.resolve(strict=True))
