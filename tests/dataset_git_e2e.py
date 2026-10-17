"""Independent Git/LFS oracle for the production Rust inventory reader."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zlib
from e2e_temp import e2e_temp_root

ROOT = Path(__file__).resolve().parents[1]


def git(root, *args):
    return subprocess.run(["git", "-C", str(root), *args], capture_output=True, check=True).stdout


def inventory(binary, root, revision, succeeds=True):
    env = dict(os.environ, GIT_DIR=str(root / "does-not-exist"), GIT_CONFIG_COUNT="1",
               GIT_CONFIG_KEY_0="core.worktree", GIT_CONFIG_VALUE_0=str(root / "outside"))
    result = subprocess.run([str(binary), "dataset", "inventory", str(root), "--revision", revision],
                            capture_output=True, text=True, encoding="utf-8", env=env)
    assert result.returncode == (0 if succeeds else 2), result.stderr
    return json.loads(result.stdout) if succeeds else result.stderr


def external_binding(binary):
    # Fixture is content identity only: format/geometry remain explicitly unqualified.
    with tempfile.TemporaryDirectory(prefix="external-binding-", dir=e2e_temp_root(ROOT / "target")) as temp:
        root = Path(temp)
        git(root, "init", "--quiet")
        for key, value in [("user.name", "Tokito fixture"), ("user.email", "fixture@example.invalid"), ("core.autocrlf", "false")]:
            git(root, "config", "--local", key, value)
        stage = root / "staged"
        stage.mkdir()
        payload = (ROOT / "tests/fixtures/assembly-e2e/single-resistor.kicad_pcb").read_bytes()
        artifact = stage / "candidate.cad"
        artifact.write_bytes(payload)
        claim = {"sha256": hashlib.sha256(payload).hexdigest(), "bytes": len(payload), "url": "https://example.invalid/candidate.cad"}
        descriptor = {"schema_version": "1.0", "model/body": dict(claim)}
        source = root / "provenance.json"
        request_path = root / "request.json"
        request = {"schema": "tokito.dataset-external-request.v1", "repository": "https://github.com/TokitoAI/tokito-app-dataset",
                   "revision": "", "assets": [{"boardId": "fixture", "role": "model", "path": "candidate.cad",
                                                  "provenancePath": "provenance.json", "claimPointer": "/model~1body"}]}
        sequence = 0
        def pin():
            source.write_text(json.dumps(descriptor), encoding="utf-8")
            git(root, "add", "--", "provenance.json")
            git(root, "commit", "--quiet", "--allow-empty", "-m", "Explicit reference fixture")
            request["revision"] = git(root, "rev-parse", "HEAD").decode().strip()
            (stage / "provenance.json").write_bytes(source.read_bytes())
        def bind(succeeds):
            nonlocal sequence
            sequence += 1
            output = root / f"receipt-{sequence}.json"
            request_path.write_text(json.dumps(request), encoding="utf-8")
            proc = subprocess.run([str(binary), "dataset", "bind-external", str(root), "--assets-root", str(stage),
                                   "--request", str(request_path), "--output", str(output)], capture_output=True, text=True, encoding="utf-8")
            assert proc.returncode == (0 if succeeds else 2), proc.stderr
            if not succeeds:
                assert not output.exists()
                return
            receipt = json.loads(proc.stdout)
            assert receipt["sourceBinding"]["commit"] == request["revision"]
            assert receipt["externalAssets"][0]["sha256"] == claim["sha256"]
            assert receipt["externalAssets"][0]["bytes"] == len(payload)
            for flag in ["publisherAuthenticated", "urlOriginVerified", "externalUrlFetched", "geometryQualified", "componentCrosswalkVerified", "physicalFitVerified", "appAdmissionAuthorized"]:
                assert receipt[flag] is False
            original = output.read_bytes()
            again = subprocess.run([str(binary), "dataset", "bind-external", str(root), "--assets-root", str(stage),
                                    "--request", str(request_path), "--output", str(output)], capture_output=True)
            assert again.returncode == 2 and output.read_bytes() == original
        pin(); bind(True)
        artifact.write_bytes(payload + b"altered"); bind(False)
        artifact.write_bytes(b"version https://git-lfs.github.com/spec/v1\noid sha256:" + b"0" * 64 + b"\nsize 1\n"); bind(False)
        artifact.unlink(); bind(False)
        artifact.write_bytes(payload)
        (stage / "provenance.json").write_text(json.dumps({**descriptor, "changed": True}), encoding="utf-8"); bind(False)
        (stage / "provenance.json").write_bytes(source.read_bytes())
        request["assets"][0]["claimPointer"] = "/model~2body"; bind(False)
        request["assets"][0]["claimPointer"] = "/absent"; bind(False)
        request["assets"][0]["claimPointer"] = "/model~1body"
        request["assets"].append(dict(request["assets"][0])); bind(False)
        request["assets"].pop()
        for bad in [{"bytes": 1.5}, {"bytes": 256 * 1024 * 1024 + 1}, {"sha256": "bad"}, {"url": "file:///outside"}]:
            descriptor["model/body"] = {**claim, **bad}
            pin(); bind(False)
        descriptor["model/body"] = {"repository": "https://example.invalid/models", "tree_sha": "0" * 40}
        pin(); bind(False)
        descriptor["model/body"] = dict(claim)
        descriptor["schema_version"] = "2.0"
        pin(); bind(False)
    print("PASS external binding E2E: source-pinned descriptor and selected payload identity; changed/missing/pointer/locator-only/unsupported claims refuse")


def run(binary):
    with tempfile.TemporaryDirectory(prefix="dataset-git-", dir=e2e_temp_root(ROOT / "target")) as temp:
        root = Path(temp)
        git(root, "init", "--quiet")
        for key, value in [("user.name", "Tokito CLI fixture"), ("user.email", "fixture@example.invalid"), ("core.autocrlf", "false")]:
            git(root, "config", "--local", key, value)
        native = root / "data/board-sources/fixture/native.kicad_pcb"
        native.parent.mkdir(parents=True)
        payload = (ROOT / "tests/fixtures/assembly-e2e/single-resistor.kicad_pcb").read_bytes()
        native.write_bytes(payload)
        pointer_path = native.parent / "lfs.kicad_pcb"
        digest = hashlib.sha256(payload).hexdigest()
        pointer = f"version https://git-lfs.github.com/spec/v1\noid sha256:{digest}\nsize {len(payload)}\n".encode()
        pointer_path.write_bytes(pointer)
        git(root, "add", "--", "data")
        git(root, "commit", "--quiet", "-m", "Pinned inventory fixture")
        revision = git(root, "rev-parse", "HEAD").decode().strip()
        tree = git(root, "rev-parse", "HEAD^{tree}").decode().strip()
        (root / "untracked-user.txt").write_text("preserve", encoding="utf-8")
        before = {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in (root / ".git").rglob("*") if p.is_file()}
        result = inventory(binary, root, revision)
        after = {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in (root / ".git").rglob("*") if p.is_file()}
        assert before == after
        assert result["commit"] == revision and result["tree"] == tree
        assert result["commitAndTreeHashesVerified"] is True
        assert result["publisherAuthenticated"] is False and result["workingTreeCoverageComplete"] is False
        assert result["trackedAssetCount"] == 2
        assert result["materializationCounts"] == {"materialized": 1, "pointer_only": 1}
        records = {a["path"]: a for a in result["assets"]}
        for path, data in [(native, payload), (pointer_path, pointer)]:
            relative = path.relative_to(root).as_posix()
            assert records[relative]["gitObjectId"] == hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
            assert records[relative]["gitBlobHashVerified"] is True
        assert records[pointer_path.relative_to(root).as_posix()]["lfs"] == {"sha256": digest, "bytes": len(payload)}
        pointer_path.write_bytes(payload)
        complete = inventory(binary, root, revision)
        assert complete["workingTreeCoverageComplete"] is True
        assert complete["materializationCounts"] == {"materialized": 2}
        # Separately staged payloads bind to the source pin, not their local names.
        staged = root / "candidate-assets"
        relative_paths = [p.relative_to(root).as_posix() for p in (native, pointer_path)]
        for relative in relative_paths:
            destination = staged / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(payload)
        request = {"schema": "tokito.dataset-source-request.v1",
                   "repository": "https://github.com/TokitoAI/tokito-app-dataset", "revision": revision,
                   "assets": [{"boardId": "fixture", "role": "pcb", "path": relative,
                               "provenance": "Real PCB fixture; identity only"} for relative in relative_paths]}
        request_path = root / "binding-request.json"
        request_path.write_text(json.dumps(request), encoding="utf-8")
        output_path = root / "binding.json"
        def bind(succeeds, output):
            proc = subprocess.run([str(binary), "dataset", "bind", str(root), "--assets-root", str(staged),
                                   "--request", str(request_path), "--output", str(output)],
                                  capture_output=True, text=True, encoding="utf-8")
            assert proc.returncode == (0 if succeeds else 2), proc.stderr
            if succeeds:
                return json.loads(proc.stdout)
            assert not output.exists()
        binding = bind(True, output_path)
        assert binding["commit"] == revision and binding["tree"] == tree
        assert binding["selectedPayloadsMatchSourceObjects"] and not binding["publisherAuthenticated"]
        assert not binding["geometryQualified"] and not binding["appAdmissionAuthorized"]
        assert {a["path"]: a["gitObjectId"] for a in binding["sourceAssets"]} == {
            path: records[path]["gitObjectId"] for path in relative_paths}
        assert binding["contentLock"]["assets"][0]["sha256"] == digest
        original_binding = output_path.read_bytes()
        proc = subprocess.run([str(binary), "dataset", "bind", str(root), "--assets-root", str(staged),
                               "--request", str(request_path), "--output", str(output_path)], capture_output=True)
        assert proc.returncode == 2 and output_path.read_bytes() == original_binding
        (staged / relative_paths[1]).write_bytes(payload + b"altered")
        bind(False, root / "altered-binding.json")
        (staged / relative_paths[1]).write_bytes(pointer)
        bind(False, root / "pointer-binding.json")
        (staged / relative_paths[1]).write_bytes(payload)
        request["assets"][0]["path"] = "untracked.kicad_pcb"
        (staged / "untracked.kicad_pcb").write_bytes(payload)
        request_path.write_text(json.dumps(request), encoding="utf-8")
        bind(False, root / "untracked-binding.json")
        pointer_path.write_bytes(payload + b"altered")
        assert inventory(binary, root, revision)["materializationCounts"]["content_mismatch"] == 1
        pointer_path.write_bytes(pointer)
        native.write_bytes(payload + b"\nnewer source")
        git(root, "add", "--", "data")
        git(root, "commit", "--quiet", "-m", "Newer source")
        assert git(root, "rev-parse", "HEAD").decode().strip() != revision
        assert inventory(binary, root, revision)["materializationCounts"]["content_mismatch"] == 1
        native.unlink()
        assert inventory(binary, root, revision)["materializationCounts"]["missing"] == 1
        native.write_bytes(payload)
        newer = git(root, "rev-parse", "HEAD").decode().strip()
        git(root, "replace", revision, newer)
        assert inventory(binary, root, revision)["tree"] == tree
        git(root, "replace", "-d", revision)
        config = root / ".git/config"
        config_bytes = config.read_bytes()
        config.write_bytes(config_bytes + b"\n#" + b"x" * (64 * 1024))
        assert "configuration exceeds" in inventory(binary, root, revision, succeeds=False)
        config.write_bytes(config_bytes)
        # A tracked path's linked parent must be refused before probing its target.
        saved = native.parent.with_name("fixture-saved")
        native.parent.rename(saved)
        env = dict(os.environ, TOKITO_INVENTORY_LINK=str(native.parent), TOKITO_INVENTORY_TARGET=str(saved))
        if os.name == "nt":
            subprocess.run(["pwsh", "-NoProfile", "-Command", "New-Item -ItemType Junction -Path $env:TOKITO_INVENTORY_LINK -Target $env:TOKITO_INVENTORY_TARGET | Out-Null"], env=env, check=True, capture_output=True)
        else:
            native.parent.symlink_to(saved, target_is_directory=True)
        try:
            blocked = inventory(binary, root, revision)
            assert blocked["materializationCounts"] == {"unavailable": 2}
            assert all("actualSha256" not in a for a in blocked["assets"])
        finally:
            if os.name == "nt":
                subprocess.run(["pwsh", "-NoProfile", "-Command", "Remove-Item -LiteralPath $env:TOKITO_INVENTORY_LINK -Force"], env=env, check=True, capture_output=True)
            else:
                native.parent.unlink()
            saved.rename(native.parent)
        alternates = root / ".git/objects/info/alternates"
        alternates.write_text("unsupported shared store", encoding="utf-8")
        assert "external/shared" in inventory(binary, root, revision, succeeds=False)
        alternates.unlink()
        info_dir = root / ".git/objects/info"
        saved_info = info_dir.with_name("info-saved")
        info_dir.rename(saved_info)
        env = dict(os.environ, TOKITO_INVENTORY_LINK=str(info_dir), TOKITO_INVENTORY_TARGET=str(saved_info))
        if os.name == "nt":
            subprocess.run(["pwsh", "-NoProfile", "-Command", "New-Item -ItemType Junction -Path $env:TOKITO_INVENTORY_LINK -Target $env:TOKITO_INVENTORY_TARGET | Out-Null"], env=env, check=True, capture_output=True)
        else:
            info_dir.symlink_to(saved_info, target_is_directory=True)
        try:
            assert "linked" in inventory(binary, root, revision, succeeds=False)
        finally:
            if os.name == "nt":
                subprocess.run(["pwsh", "-NoProfile", "-Command", "Remove-Item -LiteralPath $env:TOKITO_INVENTORY_LINK -Force"], env=env, check=True, capture_output=True)
            else:
                info_dir.unlink()
            saved_info.rename(info_dir)
        assert "full lowercase" in inventory(binary, root, "main", succeeds=False)
        assert "missing" in inventory(binary, root, "0" * 40, succeeds=False)
        blob_oid = records[native.relative_to(root).as_posix()]["gitObjectId"]
        stored = root / ".git/objects" / blob_oid[:2] / blob_oid[2:]
        assert stored.resolve().is_relative_to(root.resolve())
        stored.chmod(0o600)  # Git loose objects are read-only on Windows.
        original = stored.read_bytes()
        changed = bytearray(zlib.decompress(original))
        changed[-1] ^= 1
        stored.write_bytes(zlib.compress(changed))
        assert "hash mismatch" in inventory(binary, root, revision, succeeds=False)
        stored.write_bytes(original)
        # Exercise pack/delta decoding independently of the loose-object case.
        git(root, "gc", "--quiet")
        packed = inventory(binary, root, revision)
        assert packed["assets"] == result["assets"]
        assert packed["commit"] == revision and packed["tree"] == tree
        invalid = pointer_path.parent / "invalid-pointer.step"
        invalid.write_bytes(pointer.replace(b"size ", b"size 0"))
        git(root, "add", "--", "data")
        git(root, "commit", "--quiet", "-m", "Invalid pointer candidate")
        invalid_revision = git(root, "rev-parse", "HEAD").decode().strip()
        assert inventory(binary, root, invalid_revision)["materializationCounts"]["invalid_source_pointer"] == 1
        invalid.write_bytes(pointer.replace(b"oid ", b"ext-0-custom data\noid "))
        git(root, "add", "--", "data")
        git(root, "commit", "--quiet", "-m", "Extended pointer candidate")
        extended_revision = git(root, "rev-parse", "HEAD").decode().strip()
        assert inventory(binary, root, extended_revision)["materializationCounts"]["unsupported_source_pointer"] == 1
        assert (root / "untracked-user.txt").read_text() == "preserve"
    print("PASS Git inventory E2E: immutable commit/tree/blob oracle, LFS payloads, partial checkout, mutation/missing/corruption, packed objects and isolated environment")
    external_binding(binary)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    run(parser.parse_args().binary.resolve(strict=True))
