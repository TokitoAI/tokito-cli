"""Independent Git/LFS oracle for the production Rust inventory reader."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zlib

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


def run(binary):
    with tempfile.TemporaryDirectory(prefix="dataset-git-", dir=ROOT / "target") as temp:
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


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    run(parser.parse_args().binary.resolve(strict=True))
