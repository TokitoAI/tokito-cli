"""Run source-hash-pinned embedded-model intake against a real PCB board."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from collections import Counter
from pathlib import Path, PurePosixPath
from urllib.parse import unquote_to_bytes
import unicodedata

MAX_GLTF_DOCUMENT_BYTES = 64 * 1024 * 1024


def gltf_bundle_sha256(model_sha256: str, dependencies: list[dict]) -> str:
    identity = bytearray()

    def append_field(value: str) -> None:
        encoded = value.encode("utf-8")
        identity.extend(len(encoded).to_bytes(8, "big"))
        identity.extend(encoded)

    append_field(model_sha256)
    for dependency in sorted(dependencies, key=lambda row: row["relativeUriPath"]):
        append_field(dependency["relativeUriPath"])
        append_field(dependency["sha256"])
    return hashlib.sha256(identity).hexdigest()


def normalize_gltf_uri(uri: str) -> str:
    if (
        not uri
        or uri.startswith(("/", "\\"))
        or any(character in uri for character in '\\?#:<>' + '"|* ')
        or re.search(r"%(?![0-9A-Fa-f]{2})", uri)
    ):
        raise ValueError("glTF document has an unsafe external URI")
    try:
        decoded = unquote_to_bytes(uri).decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError("glTF document URI is not valid UTF-8") from error
    if (
        not decoded
        or decoded.startswith("/")
        or any(character in decoded for character in '\\?#:<>' + '"|*')
        or any(unicodedata.category(character) == "Cc" for character in decoded)
    ):
        raise ValueError("glTF document has an unsafe external URI")
    segments = []
    for segment in decoded.split("/"):
        if segment == ".":
            continue
        if not segment or segment == ".." or segment.endswith((" ", ".")):
            raise ValueError("glTF document has an unsafe external URI")
        stem = segment.split(".", 1)[0].rstrip(" ").upper()
        if stem in {"CON", "PRN", "AUX", "NUL", "CLOCK$", "CONIN$", "CONOUT$", "COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³"} or (
            len(stem) == 4 and stem[:3] in {"COM", "LPT"} and stem[3].isdigit()
        ):
            raise ValueError("glTF document has an unsafe external URI")
        segments.append(segment)
        if len(segments) > 64:
            raise ValueError("glTF document external URI exceeds the path-depth limit")
    if not segments:
        raise ValueError("glTF document has an empty external URI")
    return "/".join(segments)


def gltf_document_resources(path: Path) -> tuple[set[str], int, int]:
    if path.stat().st_size > MAX_GLTF_DOCUMENT_BYTES:
        raise ValueError("glTF document exceeds its verification byte limit")
    with path.open("rb") as stream:
        document_bytes = stream.read(MAX_GLTF_DOCUMENT_BYTES + 1)
    if len(document_bytes) > MAX_GLTF_DOCUMENT_BYTES:
        raise ValueError("glTF document exceeds its verification byte limit")
    document = json.loads(document_bytes.decode("utf-8"))
    if (
        not isinstance(document, dict)
        or not isinstance(document.get("asset"), dict)
        or document["asset"].get("version") != "2.0"
    ):
        raise ValueError("glTF document has an unsupported or malformed root")

    def has_extension_payload(value: object) -> bool:
        if isinstance(value, dict):
            for key, child in value.items():
                if key == "extensions" and (not isinstance(child, dict) or child):
                    return True
                if key in {"extensionsUsed", "extensionsRequired"} and (
                    not isinstance(child, list) or child
                ):
                    return True
                if has_extension_payload(child):
                    return True
        elif isinstance(value, list):
            return any(has_extension_payload(child) for child in value)
        return False

    if has_extension_payload(document):
        raise ValueError("glTF extensions are not included in the dependency closure")
    paths: set[str] = set()
    inline_count = 0
    reference_count = 0

    def record_uri(uri: object) -> None:
        nonlocal inline_count, reference_count
        if not isinstance(uri, str) or len(uri) > 4096:
            raise ValueError("glTF document resource URI is missing or invalid")
        reference_count += 1
        if reference_count > 4096:
            raise ValueError("glTF document exceeds the resource-reference limit")
        if uri[:5].lower() == "data:":
            inline_count += 1
            return
        paths.add(normalize_gltf_uri(uri))
        if len(paths) > 4096:
            raise ValueError("glTF document exceeds the unique-resource limit")

    buffers = document.get("buffers")
    if buffers is not None:
        if not isinstance(buffers, list):
            raise ValueError("glTF buffers field is malformed")
        for buffer in buffers:
            if not isinstance(buffer, dict):
                raise ValueError("glTF buffer row is malformed")
            record_uri(buffer.get("uri"))
    images = document.get("images")
    if images is not None:
        if not isinstance(images, list):
            raise ValueError("glTF images field is malformed")
        for image in images:
            if not isinstance(image, dict):
                raise ValueError("glTF image row is malformed")
            if "uri" in image and "bufferView" in image:
                raise ValueError("glTF image source is ambiguous")
            if "uri" in image:
                record_uri(image["uri"])
            elif "bufferView" not in image:
                raise ValueError("glTF image source is missing")
    return paths, inline_count, reference_count


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--board", required=True, type=Path)
    parser.add_argument("--expected-sha256", required=True)
    parser.add_argument("--expected-model-count", required=True, type=int)
    parser.add_argument("--expected-captured-bytes", required=True, type=int)
    parser.add_argument("--expected-format", required=True, choices=("step", "wrl", "glb", "gltf"))
    parser.add_argument("--extract-dir", type=Path)
    parser.add_argument("--require-unique-owners", action="store_true")
    parser.add_argument("--receipt", type=Path)
    args = parser.parse_args()

    binary = args.binary.resolve(strict=True)
    board = args.board.resolve(strict=True)
    if args.expected_model_count <= 0:
        raise SystemExit("expected model count must be positive")
    if args.expected_captured_bytes <= 0:
        raise SystemExit("expected captured-byte total must be positive")
    board_digest = hashlib.sha256(board.read_bytes()).hexdigest()
    if board_digest != args.expected_sha256.lower():
        raise SystemExit(
            f"board SHA-256 mismatch: expected {args.expected_sha256.lower()}, got {board_digest}"
        )

    command = [
        str(binary),
        "resolve",
        "models",
        str(board),
        "--project-root",
        str(board.parent),
    ]
    if args.extract_dir:
        extract_dir = args.extract_dir.resolve()
        if extract_dir.exists():
            raise SystemExit(f"model extraction destination already exists: {extract_dir}")
        command.extend(["--extract-dir", str(extract_dir)])
    command.append("--json")
    process = subprocess.run(
        command,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=300,
        check=False,
    )
    if process.returncode != 0:
        raise SystemExit(
            f"Tokito model resolution failed ({process.returncode}): {process.stderr.strip()}"
        )
    try:
        report = json.loads(process.stdout)
    except json.JSONDecodeError as error:
        raise SystemExit(f"Tokito returned invalid JSON: {error}") from error

    models = report.get("models")
    if not isinstance(models, list):
        raise SystemExit("Tokito report has no model list")
    if len(models) != args.expected_model_count:
        raise SystemExit(
            f"model count mismatch: expected {args.expected_model_count}, got {len(models)}"
        )
    failures = [
        model
        for model in models
        if model.get("status") != "resolved"
        or model.get("origin") != "embedded_board"
        or model.get("format") != args.expected_format
    ]
    if failures or report.get("resolvedAssets") != args.expected_model_count:
        examples = [
            {
                "source": item.get("source"),
                "status": item.get("status"),
                "format": item.get("format"),
                "origin": item.get("origin"),
            }
            for item in failures[:8]
        ]
        raise SystemExit(
            "not every pinned model resolved from the embedded board payload "
            f"as {args.expected_format}: {json.dumps(examples, sort_keys=True)}"
        )
    captured_bytes = report.get("capturedBytes")
    if captured_bytes != args.expected_captured_bytes:
        raise SystemExit(
            "captured-byte total mismatch: "
            f"expected {args.expected_captured_bytes}, got {captured_bytes}"
        )

    owners_ready = report.get("modelReferencesWithUniqueOwnerIdentity")
    owners_missing = report.get("modelReferencesWithoutUniqueOwnerIdentity")
    if (
        not isinstance(owners_ready, int)
        or not isinstance(owners_missing, int)
        or owners_ready < 0
        or owners_missing < 0
        or owners_ready + owners_missing != args.expected_model_count
    ):
        raise SystemExit("model-owner identity coverage is missing or inconsistent")
    if args.require_unique_owners and owners_ready != args.expected_model_count:
        raise SystemExit(
            "not every model reference has a unique owner UUID/reference pair: "
            f"expected {args.expected_model_count}, got {owners_ready}"
        )

    extraction = report.get("assetExport")
    if args.extract_dir:
        if not isinstance(extraction, dict):
            raise SystemExit("Tokito omitted the requested source-model asset export summary")
        if (
            extraction.get("schema") != "tokito.model-assets.v1"
            or extraction.get("sourceBoardSha256") != board_digest
            or extraction.get("partialExport") is not False
            or extraction.get("resolvedModelPayloads") != args.expected_model_count
            or extraction.get("unexportedModelReferences") != 0
            or extraction.get("payloadBytes") != args.expected_captured_bytes
            or extraction.get("geometryDecoded") is not False
            or extraction.get("componentSelectionReady") is not False
            or extraction.get("mobileImportReady") is not False
        ):
            raise SystemExit(f"source-model extraction summary is incomplete or overstated: {extraction}")
        extraction_root = args.extract_dir.resolve(strict=True)
        if args.extract_dir.is_symlink() or not extraction_root.is_dir():
            raise SystemExit("source-model extraction root is missing or linked")
        manifest_path = extraction_root / "manifest.json"
        try:
            manifest_bytes = manifest_path.read_bytes()
            manifest = json.loads(manifest_bytes.decode("utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise SystemExit(f"source-model extraction manifest is unreadable: {error}") from error
        except UnicodeDecodeError as error:
            raise SystemExit(f"source-model extraction manifest is not UTF-8: {error}") from error
        manifest_digest = hashlib.sha256(manifest_bytes).hexdigest()
        if extraction.get("manifestSha256") != manifest_digest:
            raise SystemExit("source-model extraction manifest SHA-256 does not match CLI summary")
        if (
            manifest.get("schema") != "tokito.model-assets.v1"
            or manifest.get("sourceBoardSha256") != board_digest
            or manifest.get("modelReferences") != args.expected_model_count
            or manifest.get("resolvedModelPayloads") != args.expected_model_count
            or manifest.get("unexportedModelReferences") != 0
            or manifest.get("payloadBytes") != args.expected_captured_bytes
            or manifest.get("geometryValidated") is not False
            or manifest.get("componentSelectionReady") is not False
            or manifest.get("mobileImportReady") is not False
        ):
            raise SystemExit("source-model extraction manifest does not match the pinned board intake")
        exported = manifest.get("models")
        if (
            not isinstance(exported, list)
            or len(exported) != args.expected_model_count
            or any(not isinstance(model, dict) for model in exported)
        ):
            raise SystemExit("source-model extraction manifest has an incomplete model identity list")
        seen_assets: dict[str, str] = {}
        unique_asset_bytes = 0
        reference_asset_bytes = 0
        gltf_dependency_files = 0
        gltf_dependency_payload_bytes = 0
        owner_uuids: set[str] = set()
        owner_references: set[str] = set()
        for model in exported:
            digest = model.get("sha256")
            model_format = model.get("format")
            extraction_row = model.get("extraction")
            owner = model.get("ownerComponent")
            if (
                not isinstance(digest, str)
                or len(digest) != 64
                or any(character not in "0123456789abcdef" for character in digest)
                or model_format not in ("step", "wrl", "glb", "gltf")
                or not isinstance(extraction_row, dict)
                or extraction_row.get("status")
                != ("exported_gltf_bundle" if model_format == "gltf" else "exported_source_payload")
                or extraction_row.get("sha256") != digest
                or extraction_row.get("geometryDecoded") is not False
                or extraction_row.get("placementApplied") is not False
                or (args.require_unique_owners and (not isinstance(owner, dict) or owner.get("selectableIdentityReady") is not True))
            ):
                raise SystemExit("source-model extraction lost per-reference source or owner identity")
            if args.require_unique_owners:
                component_uuid = owner.get("componentUuid")
                reference = owner.get("reference")
                if (
                    not isinstance(component_uuid, str)
                    or not component_uuid.strip()
                    or not isinstance(reference, str)
                    or not reference.strip()
                    or owner.get("identityIssues") != []
                ):
                    raise SystemExit("source-model extraction contains an unqualified component identity")
                if component_uuid in owner_uuids:
                    raise SystemExit("source-model extraction repeats a supposedly unique component UUID")
                if reference in owner_references:
                    raise SystemExit("source-model extraction repeats a supposedly unique component reference")
                owner_uuids.add(component_uuid)
                owner_references.add(reference)
            relative = extraction_row.get("assetPath")
            if model_format == "gltf":
                bundle_digest = extraction_row.get("gltfBundleSha256")
                if (
                    not isinstance(bundle_digest, str)
                    or len(bundle_digest) != 64
                    or any(character not in "0123456789abcdef" for character in bundle_digest)
                ):
                    raise SystemExit("source-model extraction has an invalid glTF bundle identity")
                bundle_report = model.get("gltfBundle")
                if not isinstance(bundle_report, dict) or bundle_report.get("status") != "complete":
                    raise SystemExit("source-model glTF dependency closure is not complete")
                expected_relative = f"assets/gltf/{bundle_digest}/scene.gltf"
            else:
                extension = {"step": "step", "wrl": "wrl", "glb": "glb"}[model_format]
                expected_relative = f"assets/{digest}.{extension}"
            if relative != expected_relative:
                raise SystemExit("source-model extraction manifest contains an unsafe asset path")
            declared_size = extraction_row.get("bytes")
            if type(declared_size) is not int or declared_size <= 0:
                raise SystemExit("source-model extraction row has an invalid byte count")
            payload_rows = [(relative, digest, declared_size, "source-model payload")]
            gltf_dependencies = []
            if model_format == "gltf":
                dependencies = extraction_row.get("dependencies")
                if not isinstance(dependencies, list):
                    raise SystemExit("glTF extraction is missing its dependency inventory")
                bundle_root = f"assets/gltf/{bundle_digest}/"
                dependency_paths: set[str] = set()
                for dependency in dependencies:
                    if not isinstance(dependency, dict):
                        raise SystemExit("glTF dependency inventory contains a malformed row")
                    uri_path = dependency.get("relativeUriPath")
                    dependency_path = dependency.get("assetPath")
                    dependency_digest = dependency.get("sha256")
                    dependency_size = dependency.get("bytes")
                    parsed_uri = PurePosixPath(uri_path) if isinstance(uri_path, str) else None
                    if (
                        parsed_uri is None
                        or not uri_path
                        or "\\" in uri_path
                        or parsed_uri.is_absolute()
                        or parsed_uri.as_posix() != uri_path
                        or any(part in ("", ".", "..") for part in uri_path.split("/"))
                        or dependency_path != bundle_root + uri_path
                        or not isinstance(dependency_digest, str)
                        or len(dependency_digest) != 64
                        or any(character not in "0123456789abcdef" for character in dependency_digest)
                        or type(dependency_size) is not int
                        or dependency_size <= 0
                        or uri_path in dependency_paths
                    ):
                        raise SystemExit("glTF dependency has an invalid path, digest, or byte count")
                    dependency_paths.add(uri_path)
                    gltf_dependency_files += 1
                    gltf_dependency_payload_bytes += dependency_size
                    payload_rows.append(
                        (dependency_path, dependency_digest, dependency_size, "glTF dependency")
                    )
                    gltf_dependencies.append(
                        {
                            "relativeUriPath": uri_path,
                            "sha256": dependency_digest,
                        }
                    )

                if gltf_bundle_sha256(digest, gltf_dependencies) != bundle_digest:
                    raise SystemExit("source-model glTF bundle identity does not match its contents")

            for payload_relative, payload_digest, payload_size, description in payload_rows:
                reference_asset_bytes += payload_size
                asset_path = extraction_root / payload_relative
                if asset_path.is_symlink() or not asset_path.is_file():
                    raise SystemExit(f"{description} is missing or linked: {asset_path}")
                if asset_path.stat().st_size != payload_size:
                    raise SystemExit(f"{description} size does not match its manifest row: {asset_path}")
                if payload_relative in seen_assets:
                    if seen_assets[payload_relative] != payload_digest:
                        raise SystemExit("two model rows disagree on one content-addressed asset")
                else:
                    hasher = hashlib.sha256()
                    with asset_path.open("rb") as payload_stream:
                        for chunk in iter(lambda: payload_stream.read(1024 * 1024), b""):
                            hasher.update(chunk)
                    if hasher.hexdigest() != payload_digest:
                        raise SystemExit(f"{description} SHA-256 mismatch: {asset_path}")
                    seen_assets[payload_relative] = payload_digest
                    unique_asset_bytes += payload_size
            if model_format == "gltf":
                scene_path = extraction_root / relative
                try:
                    document_resources, inline_count, reference_count = gltf_document_resources(scene_path)
                except (OSError, UnicodeDecodeError, json.JSONDecodeError, ValueError) as error:
                    raise SystemExit(f"source-model glTF document is invalid: {error}") from error
                declared_resources = {row["relativeUriPath"] for row in gltf_dependencies}
                if document_resources != declared_resources:
                    raise SystemExit("glTF scene resource URIs do not match the dependency inventory")
                if (
                    bundle_report.get("resourceReferences") != reference_count
                    or bundle_report.get("uniqueExternalResources") != len(document_resources)
                    or bundle_report.get("resolvedExternalResources") != len(document_resources)
                    or bundle_report.get("inlineDataResources") != inline_count
                ):
                    raise SystemExit("glTF dependency summary does not match the scene document")
        if (
            manifest.get("uniqueAssetFiles") != len(seen_assets)
            or extraction.get("uniqueAssetFiles") != len(seen_assets)
        ):
            raise SystemExit("source-model export unique-file count does not match its model records")
        if (
            manifest.get("uniqueAssetBytes") != unique_asset_bytes
            or extraction.get("uniqueAssetBytes") != unique_asset_bytes
        ):
            raise SystemExit("source-model export unique-byte count does not match its files")
        if (
            type(manifest.get("gltfDependencyFiles")) is not int
            or manifest.get("gltfDependencyFiles") != gltf_dependency_files
            or type(extraction.get("gltfDependencyFiles")) is not int
            or extraction.get("gltfDependencyFiles") != gltf_dependency_files
            or type(manifest.get("gltfDependencyPayloadBytes")) is not int
            or manifest.get("gltfDependencyPayloadBytes") != gltf_dependency_payload_bytes
            or type(extraction.get("gltfDependencyPayloadBytes")) is not int
            or extraction.get("gltfDependencyPayloadBytes") != gltf_dependency_payload_bytes
        ):
            raise SystemExit("source-model glTF dependency aggregate totals do not match its rows")
        if (
            reference_asset_bytes != args.expected_captured_bytes
            or manifest.get("payloadBytes") != reference_asset_bytes
            or extraction.get("payloadBytes") != reference_asset_bytes
            or (
                args.require_unique_owners
                and (
                    len(owner_uuids) != args.expected_model_count
                    or len(owner_references) != args.expected_model_count
                )
            )
        ):
            raise SystemExit("source-model export per-reference bytes or owner coverage do not match intake")
        actual_files: set[str] = set()
        actual_directories: set[str] = set()
        for path in extraction_root.rglob("*"):
            if path.is_symlink():
                raise SystemExit(f"source-model export contains a linked path: {path}")
            relative_path = path.relative_to(extraction_root).as_posix()
            if path.is_dir():
                actual_directories.add(relative_path)
            elif path.is_file():
                actual_files.add(relative_path)
            else:
                raise SystemExit(f"source-model export contains a non-file entry: {path}")
        expected_files = set(seen_assets) | {"manifest.json"}
        expected_directories = {"assets"}
        for relative_path in seen_assets:
            parent = PurePosixPath(relative_path).parent
            while parent.as_posix() != ".":
                expected_directories.add(parent.as_posix())
                parent = parent.parent
        if actual_files != expected_files or actual_directories != expected_directories:
            raise SystemExit("source-model export contains missing or unreferenced files")

    formats = Counter(model["format"] for model in models)
    receipt = {
        "schemaVersion": 1,
        "passed": True,
        "boardSha256": board_digest,
        "modelReferences": len(models),
        "resolvedAssets": report["resolvedAssets"],
        "capturedBytes": captured_bytes,
        "embeddedOrigins": len(models),
        "modelReferencesWithUniqueOwnerIdentity": owners_ready,
        "modelReferencesWithoutUniqueOwnerIdentity": owners_missing,
        "formats": dict(sorted(formats.items())),
        "allResolved": True,
        "geometryValidated": report.get("geometryValidated"),
        "componentSelectionReady": report.get("componentSelectionReady"),
        "sourceModelAssetExport": extraction,
    }
    if args.receipt:
        args.receipt.parent.mkdir(parents=True, exist_ok=True)
        temporary = args.receipt.with_suffix(args.receipt.suffix + ".tmp")
        temporary.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        temporary.replace(args.receipt)
    print(json.dumps(receipt, sort_keys=True))


if __name__ == "__main__":
    main()
