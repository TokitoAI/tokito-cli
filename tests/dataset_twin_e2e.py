#!/usr/bin/env python3
"""Audit the pinned NXP dataset twin's GLB selectors and provenance claims."""
import argparse
import hashlib
import json
import math
import shutil
from pathlib import Path

from glb_e2e import parse_glb, invoke

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_PROVENANCE_COMMIT = "ab1420fb08e813d4c761c6454fbba16cceaaa5dc"
EXPECTED_PROVENANCE_SHA256 = "3f9741e47ca5a4093e1694b4280c735249c0a8bd48084e1035aca684ebe97572"
EXTERNAL_SOURCE_COMMIT = "5283bc059e30e51f4d3a81a674425936fc213214"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bounded_output(value: Path, allowed_root: Path) -> Path:
    absolute = value.absolute()
    current = absolute
    while current != current.parent:
        if current.exists() and current.is_symlink():
            raise ValueError(f"linked output path is not allowed: {current}")
        current = current.parent
    resolved = absolute.resolve()
    root = allowed_root.resolve()
    if not resolved.is_relative_to(root):
        raise ValueError("dataset twin receipt must remain under the selected local output root")
    if resolved.exists():
        raise FileExistsError(f"refusing to overwrite dataset twin receipt directory: {resolved}")
    return resolved


def subtree_geometry(document: dict, root_index: int, claimed_nodes: set[int], claimed_meshes: set[int]) -> tuple[int, int]:
    nodes = document["nodes"]
    meshes = document["meshes"]
    stack = [root_index]
    local_nodes: set[int] = set()
    local_meshes: set[int] = set()
    while stack:
        index = stack.pop()
        if not isinstance(index, int) or index < 0 or index >= len(nodes):
            raise AssertionError(f"invalid node index in selected subtree: {index}")
        if index in local_nodes:
            raise AssertionError(f"node cycle or repeated child in selected subtree: {index}")
        if index in claimed_nodes:
            raise AssertionError(f"selected component subtrees share node {index}")
        local_nodes.add(index)
        node = nodes[index]
        mesh = node.get("mesh")
        if mesh is not None:
            if not isinstance(mesh, int) or mesh < 0 or mesh >= len(meshes):
                raise AssertionError(f"invalid mesh index in node {index}: {mesh}")
            if mesh in claimed_meshes or mesh in local_meshes:
                raise AssertionError(f"selected component subtrees share mesh {mesh}")
            local_meshes.add(mesh)
        stack.extend(node.get("children", []))
    if not local_meshes:
        raise AssertionError(f"selected node {root_index} contains no mesh geometry")
    claimed_nodes.update(local_nodes)
    claimed_meshes.update(local_meshes)
    return len(local_nodes), len(local_meshes)


def run(glb_path: Path, map_path: Path, manifest_path: Path, provenance_path: Path, output_path: Path, output_root: Path, binary: Path | None = None, dataset_repo: Path | None = None) -> None:
    for source in (glb_path, map_path, manifest_path, provenance_path):
        if not source.is_file():
            raise FileNotFoundError(source)
    provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
    if sha256(provenance_path) != EXPECTED_PROVENANCE_SHA256:
        raise AssertionError("dataset provenance bytes differ from the pinned PR #6 metadata")
    geometry = provenance.get("geometry_asset", {})
    if geometry.get("name") != glb_path.name:
        raise AssertionError("GLB filename does not match dataset provenance")
    if int(geometry.get("bytes", -1)) != glb_path.stat().st_size:
        raise AssertionError("GLB length does not match dataset provenance")
    if geometry.get("sha256", "").removeprefix("sha256:") != sha256(glb_path):
        raise AssertionError("GLB SHA-256 does not match dataset provenance")

    declared_files = {item["path"]: item for item in provenance.get("files", [])}
    for name, path in (("component-map.json", map_path), ("digital-twin-manifest.json", manifest_path)):
        record = declared_files.get(name)
        if not record:
            raise AssertionError(f"dataset provenance is missing {name}")
        if int(record["bytes"]) != path.stat().st_size or record["sha256"] != sha256(path):
            raise AssertionError(f"{name} bytes or SHA-256 do not match dataset provenance")

    component_map = json.loads(map_path.read_text(encoding="utf-8"))
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("boardIdentity") != component_map.get("board_id"):
        raise AssertionError("manifest and component map board identities differ")
    if manifest.get("boardRevision") != component_map.get("board_revision"):
        raise AssertionError("manifest and component map board revisions differ")
    if manifest.get("assetId") != component_map.get("asset_id"):
        raise AssertionError("manifest and component map asset identities differ")
    if manifest.get("units") != "METRE" or manifest.get("axes") != "RIGHT_HANDED_Y_UP":
        raise AssertionError("dataset twin frame is not the declared metre/right-handed/Y-up frame")
    if manifest.get("componentNodePrefix") != "ref:":
        raise AssertionError("dataset twin component selector prefix differs from the App handoff contract")
    if int(manifest.get("glbBytes", -1)) != glb_path.stat().st_size:
        raise AssertionError("GLB length does not match its digital-twin manifest")
    if manifest.get("glbSha256", "").removeprefix("sha256:") != sha256(glb_path):
        raise AssertionError("GLB SHA-256 does not match its digital-twin manifest")
    for bound in (manifest.get("boundsMetres", {}).get("minimum", {}), manifest.get("boundsMetres", {}).get("maximum", {})):
        if not all(isinstance(bound.get(axis), (int, float)) and math.isfinite(bound[axis]) for axis in ("x", "y", "z")):
            raise AssertionError("manifest contains non-finite or missing 3D bounds")

    components = component_map.get("components", [])
    if len(components) != int(component_map.get("component_count", -1)):
        raise AssertionError("component map declared count differs from its rows")
    references = [entry.get("reference") for entry in components]
    if any(not isinstance(reference, str) or not reference.strip() for reference in references):
        raise AssertionError("component map contains a blank reference designator")
    if len(set(references)) != len(references):
        raise AssertionError("component map contains duplicate reference designators")

    expected: dict[str, str] = {}
    for entry in components:
        reference = entry["reference"]
        component_id = f"{component_map['board_id']}:{component_map['board_revision']}:{reference}"
        if entry.get("component_id") != component_id:
            raise AssertionError(f"component ID is not revision-scoped for {reference}")
        selector = entry.get("glb_selector")
        if selector is not None:
            if selector != f"ref:{reference}" or selector in expected:
                raise AssertionError(f"invalid or duplicate GLB selector for {reference}")
            expected[selector] = reference
            if not entry.get("model_file"):
                raise AssertionError(f"selector {selector} has no source model name")
    if int(component_map.get("renderable_count", -1)) != len(expected):
        raise AssertionError("renderable count differs from independently counted selectors")

    document, _ = parse_glb(glb_path)
    if document.get("asset", {}).get("version") != "2.0":
        raise AssertionError("dataset asset is not glTF 2.0")
    nodes = document.get("nodes", [])
    by_name: dict[str, list[int]] = {}
    for index, node in enumerate(nodes):
        name = node.get("name")
        if isinstance(name, str) and name.startswith("ref:"):
            by_name.setdefault(name, []).append(index)
    if set(by_name) != set(expected):
        raise AssertionError(f"dataset GLB selector set mismatch: missing={sorted(set(expected)-set(by_name))[:8]}, extra={sorted(set(by_name)-set(expected))[:8]}")
    if any(len(indexes) != 1 for indexes in by_name.values()):
        raise AssertionError("dataset GLB contains duplicate component selector nodes")

    claimed_nodes: set[int] = set()
    claimed_meshes: set[int] = set()
    subtree_node_count = 0
    subtree_mesh_count = 0
    for selector, indexes in by_name.items():
        node_count, mesh_count = subtree_geometry(document, indexes[0], claimed_nodes, claimed_meshes)
        subtree_node_count += node_count
        subtree_mesh_count += mesh_count

    quality_counts: dict[str, int] = {}
    for entry in components:
        quality = entry.get("model_quality") or "no_model"
        quality_counts[quality] = quality_counts.get(quality, 0) + 1
    output_path = bounded_output(output_path, output_root)
    output_path.mkdir(parents=True)
    preparation_evidence = None
    external_evidence = None
    if dataset_repo is not None:
        if binary is None:
            raise ValueError("source-pinned external journey requires the Rust binary")
        stage = output_path / "external-candidate"
        source_relative = "data/board-sources/nxp-frdm-mcxn236/ar-reference/provenance.json"
        descriptor = stage / source_relative
        descriptor.parent.mkdir(parents=True)
        shutil.copyfile(provenance_path, descriptor)
        local_model = stage / glb_path.name
        shutil.copyfile(glb_path, local_model)
        request = {"schema": "tokito.dataset-external-request.v1", "repository": "https://github.com/TokitoAI/tokito-app-dataset",
                   "revision": EXTERNAL_SOURCE_COMMIT, "assets": [{"boardId": manifest["boardIdentity"], "role": "model",
                   "path": local_model.name, "provenancePath": source_relative, "claimPointer": "/geometry_asset"}]}
        request_path = output_path / "external-request.json"
        request_path.write_text(json.dumps(request, indent=2), encoding="utf-8")
        binding_path = output_path / "external-binding.json"
        binding = json.loads(invoke(binary, "dataset", "bind-external", dataset_repo, "--assets-root", stage,
                                    "--request", request_path, "--output", binding_path).stdout)
        assert binding["sourceBinding"]["commit"] == EXTERNAL_SOURCE_COMMIT
        assert binding["sourceBinding"]["sourceAssets"][0]["gitObjectId"] == "1ced5133dc8fc81e044e0ea3c35b67d4dd5dc7d6"
        assert binding["externalAssets"][0]["sha256"] == sha256(glb_path)
        assert binding["externalAssets"][0]["bytes"] == glb_path.stat().st_size
        assert binding["externalAssets"][0]["payloadMatchesSourceClaim"] is True
        assert not binding["geometryQualified"] and not binding["urlOriginVerified"]
        external_evidence = {"commit": EXTERNAL_SOURCE_COMMIT, "selectedPayloads": 1, "bindingSha256": sha256(binding_path),
                             "sourceDescriptorGitObjectVerifiedByCli": True, "geometryQualified": False}
    if binary is not None:
        common = (glb_path, "--component-map", map_path, "--manifest", manifest_path)
        source_index = json.loads(invoke(binary, "twin", "inspect", *common).stdout)
        assert source_index["renderableCount"] == 354
        assert source_index["directRenderableSelectors"] == 0
        assert not source_index["appNameFilterCompatible"]
        prepared = output_path / "prepared"
        result = json.loads(invoke(binary, "twin", "prepare", *common, "--output", prepared).stdout)
        assert result["renamedComponentSubtrees"] == 354
        assert result["status"] == "unregistered-reference-candidate"
        assert not result["positionalGuidanceAllowed"]
        prepared_glb = prepared / "active.digital-twin.glb"
        prepared_doc, prepared_payload = parse_glb(prepared_glb)
        _, original_payload = parse_glb(glb_path)
        assert prepared_payload == original_payload
        for before, after in zip(document["nodes"], prepared_doc["nodes"]):
            assert {k: v for k, v in before.items() if k != "name"} == {k: v for k, v in after.items() if k != "name"}
        renderables = {n["name"] for n in prepared_doc["nodes"] if "mesh" in n and n.get("name", "").startswith("ref:")}
        assert renderables == set(expected)
        prepared_args = (prepared_glb, "--component-map", prepared / "component-map.json",
                         "--manifest", prepared / "active.digital-twin.json")
        index = json.loads(invoke(binary, "twin", "inspect", *prepared_args).stdout)
        assert index["directRenderableSelectors"] == 354 and index["appNameFilterCompatible"]
        assert index["unmappedRenderableNodes"] == source_index["unmappedRenderableNodes"]
        assert len(index["unmappedRenderableNodes"]) == 1
        assert not index["completeComponentCoverageVerified"]
        selected = json.loads(invoke(binary, "twin", "select", *prepared_args, "--reference", "U11").stdout)
        assert selected["status"] == "unique_candidate" and selected["matches"][0]["selector"] == "ref:U11"
        diodes = json.loads(invoke(binary, "twin", "select", *prepared_args, "--class", "diode_or_led").stdout)
        expected_diodes = [row for row in components if row.get("component_class") == "diode_or_led"]
        assert len(diodes["matches"]) == len(expected_diodes) and diodes["status"] == "multiple_candidates"
        preparation_evidence = {"renderableSelectors": 354, "geometryPayloadUnchanged": True,
                                "nodeTransformsUnchanged": True, "preparedGlbSha256": sha256(prepared_glb),
                                "diodeOrLedCandidates": len(expected_diodes),
                                "unmappedRenderableNodes": index["unmappedRenderableNodes"],
                                "unmappedRenderablesEngineeringClassified": False,
                                "appImportOrDeviceTested": False}
    receipt = {
        "schema": "tokito.dataset-digital-twin-e2e.v1",
        "provenanceOriginallyInspectedAtCommit": EXPECTED_PROVENANCE_COMMIT,
        "datasetPullRequestHead": EXTERNAL_SOURCE_COMMIT if external_evidence else None,
        "boardIdentity": manifest["boardIdentity"],
        "boardRevision": manifest["boardRevision"],
        "assetId": manifest["assetId"],
        "sourceSha256": {
            "glb": sha256(glb_path),
            "componentMap": sha256(map_path),
            "manifest": sha256(manifest_path),
            "provenance": sha256(provenance_path),
        },
        "glbBytes": glb_path.stat().st_size,
        "componentCount": len(components),
        "renderableSelectors": len(expected),
        "selectorsFoundExactlyOnce": len(by_name),
        "selectedSubtreeNodes": subtree_node_count,
        "selectedSubtreeMeshes": subtree_mesh_count,
        "noCrossComponentNodeOrMeshSharing": True,
        "modelQualityClaimsFromDataset": quality_counts,
        "rustCliPreparation": preparation_evidence,
        "rustExternalSourceBinding": external_evidence,
        "limitations": [
            "This checks dataset self-contained hashes, names, selector coverage and mesh isolation; it does not qualify package accuracy or compare geometry to a physical specimen.",
            "The dataset does not bind this map to an imported PCB SHA-256 or footprint UUIDs; a production CLI assignment still needs a hash-bound crosswalk.",
            "The photographed specimen revision, fitted BOM and visual detection accuracy remain unverified in the dataset provenance.",
            "The source map labels 306 package models exact, 25 approximate and 23 dimension-accurate placeholders; those labels are preserved as source claims, not independently certified here.",
        ],
    }
    (output_path / "e2e-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(f"PASS pinned NXP dataset twin: {len(expected)} unique selectors, {subtree_mesh_count} isolated meshes")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--glb", type=Path, required=True)
    parser.add_argument("--component-map", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--provenance", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, default=ROOT / "target")
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--dataset-repo", type=Path)
    args = parser.parse_args()
    run(args.glb, args.component_map, args.manifest, args.provenance, args.output_dir, args.output_root, args.binary, args.dataset_repo)
