"""Offline subprocess asset lookup; this is not component mesh qualification."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
from e2e_temp import e2e_temp_root


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--temp-root", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    repo = Path(__file__).resolve().parents[1]
    board = repo / "tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb"
    library = repo / "tests/fixtures/open-source/kicad-resistor-models"
    model = library / "Resistor_SMD.3dshapes/R_0603_1608Metric.wrl"
    expected_sha = "eadfec9e71c67b1dd3910c72b45d811a06f0ab5397d0609354b10e9a3a4c1fdd"
    assert hashlib.sha256(model.read_bytes()).hexdigest() == expected_sha
    parent = e2e_temp_root(args.temp_root if args.temp_root is not None else repo / "target/model-resolution-e2e")
    with tempfile.TemporaryDirectory(prefix="isolated-", dir=parent) as temp:
        work = Path(temp)

        def run(*args, fail=False):
            result = subprocess.run([str(binary), *map(str, args)], cwd=work,
                                    capture_output=True, text=True, timeout=30)
            assert (result.returncode != 0) if fail else (result.returncode == 0), result.stderr
            return result.stdout

        options = ["--project-root", board.parent, "--library-root", library,
                   "--model-var", f"KICAD8_3DMODEL_DIR={library}", "--json"]
        decoded = json.loads(run("model", "inspect", model, "--units", "kicad-wrl", "--json"))
        assert decoded["sourceSha256"] == expected_sha
        assert (decoded["rootShapes"], decoded["meshParts"], decoded["vertices"],
                decoded["triangles"], decoded["materialDefinitions"]) == (29, 26, 328, 276, 3)
        for actual, expected in zip(decoded["localBoundsMm"]["min"], [-0.8001, -0.39878, 0]):
            assert abs(actual - expected) < 1e-12
        for actual, expected in zip(decoded["localBoundsMm"]["max"], [0.8001, 0.39878, 0.44958]):
            assert abs(actual - expected) < 1e-12
        assert not decoded["boardTransformsVerified"] and not decoded["componentSelectionReady"]
        vrml_alias = work / "same-model.vrml"
        shutil.copyfile(model, vrml_alias)
        vrml_decoded = json.loads(run("model", "inspect", vrml_alias,
                                      "--units", "kicad-wrl", "--json"))
        assert vrml_decoded["sourceSha256"] == expected_sha
        assert (vrml_decoded["rootShapes"], vrml_decoded["meshParts"],
                vrml_decoded["triangles"]) == (29, 26, 276)
        run("model", "inspect", model, fail=True)
        run("model", "inspect", model, "--units", "infer", fail=True)
        text = run("resolve", "models", board, *options)
        report = json.loads(text)
        resolved = [m for m in report["models"] if m["status"] == "resolved"]
        assert report["resolvedAssets"] == len(resolved) == 6, report
        assert all(m["format"] == "wrl" and m["sha256"] == expected_sha
                   and m["byte_length"] == 14831 for m in resolved)
        assert len({tuple(m["source"]["source_path"]) for m in resolved}) == 6
        assert len({m["source"]["component_ordinal"] for m in resolved}) == 6
        assert report["capturedBytes"] == 6 * 14831
        assert not report["geometryValidated"] and not report["componentSelectionReady"]
        assert not report["mobileImportReady"]
        assert str(repo) not in text and str(repo).replace("\\", "/") not in text
        assert text == run("resolve", "models", board, *options)
        exact = json.loads(run("resolve", "models", board, *options,
                               "--max-model-bytes", 14831, "--max-batch-bytes", 6 * 14831))
        assert exact["resolvedAssets"] == 6 and exact["capturedBytes"] == 6 * 14831
        run("resolve", "models", board, *options, "--max-batch-bytes", 6 * 14831 - 1, fail=True)
        small = json.loads(run("resolve", "models", board, *options, "--max-model-bytes", 14830))
        assert small["resolvedAssets"] == 0
        assert len([m for m in small["models"] if m["status"] == "too_large"]) == 6
        run("resolve", "models", board, *options, "--max-batch-bytes", 0, fail=True)

        package = work / "package"
        run("package", "build", board, "--output", package)
        assert json.loads(run("resolve", "models", package, *options)) == report
        unmapped = json.loads(run("resolve", "models", board, "--project-root", board.parent, "--json"))
        assert unmapped["resolvedAssets"] == 0
        assert any(m["status"] == "unknown_variable" for m in unmapped["models"])
        run("resolve", "models", board, "--json", fail=True)
        run("resolve", "models", board, *options, "--json", fail=True)
        run("resolve", "models", board, "--project-root", board.parent,
            "--model-var", "KIPRJMOD=not-allowed", fail=True)

        malicious = work / "synthetic-path-only.kicad_pcb"
        refs = ["${KIPRJMOD}/../outside.wrl", "//server/private.wrl",
                "https://example.com/private.wrl", "${UNKNOWN}/missing.wrl"]
        source = '(kicad_pcb (version 20240108) (generator test) (general (thickness 1.6))'
        source += '(layers (0 "F.Cu" signal) (31 "B.Cu" signal))'
        source += '(footprint "test" (layer "F.Cu") (at 0 0) (uuid "test-only")'
        source += ''.join(f'(model "{p}")' for p in refs) + '))'
        malicious.write_text(source, encoding="utf-8")
        unsafe = json.loads(run("resolve", "models", malicious, "--project-root", work, "--json"))
        assert [m["status"] for m in unsafe["models"]] == ["invalid_path"] * 3 + ["unknown_variable"]
        malicious.write_text(source.replace(''.join(f'(model "{p}")' for p in refs),
                                             '(model "missing.wrl")' * 1025), encoding="utf-8")
        run("resolve", "models", malicious, "--project-root", work, "--json", fail=True)
    print("PASS: real UNO six pinned model assets, deterministic redacted report, package lookup, explicit mappings, path rejection and batch limit")


if __name__ == "__main__":
    main()
