"""Regression tests for the source-pinned embedded-model intake verifier."""

from __future__ import annotations

import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).with_name("embedded_dataset_e2e.py")
SPEC = importlib.util.spec_from_file_location("embedded_dataset_e2e", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
embedded_dataset_e2e = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(embedded_dataset_e2e)


class EmbeddedDatasetE2ETest(unittest.TestCase):
    def test_bundle_digest_matches_pinned_independent_vector(self) -> None:
        scene_digest = "697ae69ac43d10e084e043358b1cffc1b08de7a7abe930d66b6d6d2d3a1849b8"
        dependency_digest = "d30ca7a7a32bf5772dc5eb2a2e7bd35737eff795ad74f2479b359716b59abdfa"
        self.assertEqual(
            embedded_dataset_e2e.gltf_bundle_sha256(
                scene_digest,
                [{"relativeUriPath": "buffers/mesh.bin", "sha256": dependency_digest}],
            ),
            "52ba79a3fa5107c11136a0f290795e7ba7db434406b8a19dc599a265a4f97e9b",
        )

    def invoke_fake_gltf_cli(self, root: Path, scenario: str = "valid") -> None:
        board = root / "board.kicad_pcb"
        board.write_bytes(b"pinned-board-source")
        board_digest = hashlib.sha256(board.read_bytes()).hexdigest()
        binary = root / "tokito-cli.exe"
        binary.write_bytes(b"test-binary-placeholder")
        output = root / "bundle"
        document_uri = "buffers/mesh.bin"
        scene = (
            b'{"asset":{"version":"2.0"},'
            b'"buffers":[{"uri":"buffers/mesh.bin","byteLength":4}]}'
        )
        dependency = b"mesh"
        scene_digest = hashlib.sha256(scene).hexdigest()
        dependency_digest = hashlib.sha256(dependency).hexdigest()
        if scenario == "unsafe_dependency":
            dependency_uri = "../escape.bin"
        else:
            dependency_uri = document_uri
        if scenario == "missing_declaration":
            dependency_rows = []
        else:
            declared_digest = "0" * 64 if scenario == "wrong_dependency_digest" else dependency_digest
            dependency_rows = [
                {
                    "relativeUriPath": dependency_uri,
                    "assetPath": f"assets/gltf/PENDING/{dependency_uri}",
                    "sha256": declared_digest,
                    "bytes": len(dependency),
                }
            ]
        bundle_digest = embedded_dataset_e2e.gltf_bundle_sha256(scene_digest, dependency_rows)
        if scenario == "wrong_bundle_digest":
            bundle_digest = "a" * 64
        for row in dependency_rows:
            row["assetPath"] = f"assets/gltf/{bundle_digest}/{row['relativeUriPath']}"

        reference_bytes = len(scene) + sum(row["bytes"] for row in dependency_rows)
        unique_bytes = reference_bytes
        gltf_bundle_report = {
            "status": "complete",
            "resourceReferences": 1,
            "uniqueExternalResources": 1,
            "resolvedExternalResources": 1,
            "inlineDataResources": 0,
        }
        manifest_model = {
            "format": "gltf",
            "sha256": scene_digest,
            "gltfBundle": gltf_bundle_report,
            "ownerComponent": {
                "componentUuid": "component-u1",
                "reference": "U1",
                "selectableIdentityReady": True,
                "identityIssues": [],
            },
            "extraction": {
                "status": "exported_gltf_bundle",
                "assetPath": f"assets/gltf/{bundle_digest}/scene.gltf",
                "sha256": scene_digest,
                "bytes": len(scene),
                "gltfBundleSha256": bundle_digest,
                "dependencies": dependency_rows,
                "geometryDecoded": False,
                "placementApplied": False,
            },
        }
        manifest = {
            "schema": "tokito.model-assets.v1",
            "sourceBoardSha256": board_digest,
            "modelReferences": 1,
            "resolvedModelPayloads": 1,
            "unexportedModelReferences": 0,
            "uniqueAssetFiles": 1 + len(dependency_rows),
            "uniqueAssetBytes": unique_bytes,
            "gltfDependencyFiles": len(dependency_rows),
            "gltfDependencyPayloadBytes": sum(row["bytes"] for row in dependency_rows),
            "payloadBytes": reference_bytes,
            "geometryValidated": False,
            "componentSelectionReady": False,
            "mobileImportReady": False,
            "models": [manifest_model],
        }
        if scenario == "wrong_manifest_dependency_file_count":
            manifest["gltfDependencyFiles"] += 1
        if scenario == "wrong_manifest_dependency_payload_bytes":
            manifest["gltfDependencyPayloadBytes"] += 1
        manifest_bytes = json.dumps(manifest, indent=2).encode("utf-8")
        manifest_digest = hashlib.sha256(manifest_bytes).hexdigest()
        model_report = {
            **manifest_model,
            "status": "resolved",
            "origin": "embedded_board",
        }
        report = {
            "models": [model_report],
            "resolvedAssets": 1,
            "capturedBytes": reference_bytes,
            "modelReferencesWithUniqueOwnerIdentity": 1,
            "modelReferencesWithoutUniqueOwnerIdentity": 0,
            "geometryValidated": False,
            "componentSelectionReady": False,
            "mobileImportReady": False,
            "assetExport": {
                "schema": "tokito.model-assets.v1",
                "sourceBoardSha256": board_digest,
                "partialExport": False,
                "resolvedModelPayloads": 1,
                "unexportedModelReferences": 0,
                "payloadBytes": reference_bytes,
                "geometryDecoded": False,
                "componentSelectionReady": False,
                "mobileImportReady": False,
                "manifestSha256": manifest_digest,
                "uniqueAssetFiles": 1 + len(dependency_rows),
                "uniqueAssetBytes": unique_bytes,
                "gltfDependencyFiles": len(dependency_rows)
                + (1 if scenario == "wrong_summary_dependency_file_count" else 0),
                "gltfDependencyPayloadBytes": sum(row["bytes"] for row in dependency_rows)
                + (1 if scenario == "wrong_summary_dependency_payload_bytes" else 0),
            },
        }

        def run_cli(command, **_kwargs):
            output_arg = Path(command[command.index("--extract-dir") + 1])
            nested = output_arg / "assets" / "gltf" / bundle_digest
            nested.mkdir(parents=True)
            (nested / "scene.gltf").write_bytes(scene)
            if scenario not in {"missing_dependency_file", "unsafe_dependency", "missing_declaration"}:
                dependency_target = nested / "buffers" / "mesh.bin"
                dependency_target.parent.mkdir(parents=True)
                dependency_target.write_bytes(dependency)
            if scenario == "unexpected_file":
                (nested / "unused.bin").write_bytes(b"extra")
            (output_arg / "manifest.json").write_bytes(manifest_bytes)
            return mock.Mock(returncode=0, stdout=json.dumps(report), stderr="")

        arguments = [
            "embedded_dataset_e2e.py",
            "--binary",
            str(binary),
            "--board",
            str(board),
            "--expected-sha256",
            board_digest,
            "--expected-model-count",
            "1",
            "--expected-captured-bytes",
            str(reference_bytes),
            "--expected-format",
            "gltf",
            "--extract-dir",
            str(output),
            "--require-unique-owners",
        ]
        with mock.patch.object(embedded_dataset_e2e.subprocess, "run", side_effect=run_cli):
            with mock.patch.object(sys, "argv", arguments):
                with contextlib.redirect_stdout(io.StringIO()):
                    embedded_dataset_e2e.main()

    def assert_scenario_rejected(self, scenario: str, message: str) -> None:
        with tempfile.TemporaryDirectory(prefix="tokito-embedded-gltf-e2e-") as temporary:
            with self.assertRaisesRegex(SystemExit, message):
                self.invoke_fake_gltf_cli(Path(temporary), scenario)

    def test_valid_gltf_bundle_with_nested_dependency_passes(self) -> None:
        with tempfile.TemporaryDirectory(prefix="tokito-embedded-gltf-e2e-") as temporary:
            self.invoke_fake_gltf_cli(Path(temporary))

    def test_gltf_dependency_traversal_is_rejected(self) -> None:
        self.assert_scenario_rejected("unsafe_dependency", "invalid path, digest, or byte count")

    def test_missing_dependency_file_is_rejected(self) -> None:
        self.assert_scenario_rejected("missing_dependency_file", "is missing or linked")

    def test_wrong_dependency_digest_is_rejected(self) -> None:
        self.assert_scenario_rejected("wrong_dependency_digest", "SHA-256 mismatch")

    def test_bundle_digest_must_bind_document_and_dependencies(self) -> None:
        self.assert_scenario_rejected("wrong_bundle_digest", "bundle identity does not match")

    def test_scene_resource_inventory_must_match_manifest(self) -> None:
        self.assert_scenario_rejected("missing_declaration", "scene resource URIs do not match")

    def test_unreferenced_nested_file_is_rejected(self) -> None:
        self.assert_scenario_rejected("unexpected_file", "contains missing or unreferenced files")

    def test_manifest_dependency_aggregate_must_match_rows(self) -> None:
        self.assert_scenario_rejected(
            "wrong_manifest_dependency_file_count", "dependency aggregate totals do not match"
        )

    def test_summary_dependency_aggregate_must_match_rows(self) -> None:
        self.assert_scenario_rejected(
            "wrong_summary_dependency_file_count", "dependency aggregate totals do not match"
        )

    def test_manifest_dependency_byte_total_must_match_rows(self) -> None:
        self.assert_scenario_rejected(
            "wrong_manifest_dependency_payload_bytes", "dependency aggregate totals do not match"
        )

    def test_summary_dependency_byte_total_must_match_rows(self) -> None:
        self.assert_scenario_rejected(
            "wrong_summary_dependency_payload_bytes", "dependency aggregate totals do not match"
        )


if __name__ == "__main__":
    unittest.main()
