"""Export and decode a pinned STEP occurrence-only diagnostic GLB."""
import argparse
import hashlib
import json
import struct
import subprocess
from pathlib import Path

SOURCE_SHA256 = "7234dff19581f670dd0f65f1b3c5d76abb6054004ffea8627bf0ef505f66138f"
MAX_GLB_BYTES = 128 * 1024 * 1024
DEFAULT_COLOR = [0.56, 0.58, 0.62, 1.0]


def run(command, expected_code):
    result = subprocess.run(command, capture_output=True, timeout=180)
    if result.returncode != expected_code or result.stderr:
        raise ValueError(f"command exit/stderr mismatch: {result.returncode}: {result.stderr[:1024]!r}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ValueError(f"command did not emit JSON: {result.stdout[:1024]!r}") from error


def decode_glb(path):
    data = path.read_bytes()
    if len(data) < 20 or len(data) > MAX_GLB_BYTES:
        raise ValueError("GLB size outside the declared bound")
    magic, version, declared = struct.unpack_from("<4sII", data)
    if magic != b"glTF" or version != 2 or declared != len(data):
        raise ValueError("GLB header mismatch")
    json_length, json_type = struct.unpack_from("<II", data, 12)
    if json_type != 0x4E4F534A or 20 + json_length + 8 > len(data):
        raise ValueError("GLB JSON chunk framing mismatch")
    document = json.loads(data[20:20 + json_length].rstrip(b" \t\r\n\0"))
    bin_header = 20 + json_length
    bin_length, bin_type = struct.unpack_from("<II", data, bin_header)
    binary = data[bin_header + 8:bin_header + 8 + bin_length]
    if bin_type != 0x004E4942 or len(binary) != bin_length or bin_header + 8 + bin_length != len(data):
        raise ValueError("GLB binary chunk framing mismatch")
    return document, binary, data


def canonical_f32_color(color):
    return [struct.unpack("<f", struct.pack("<f", float(value)))[0] for value in color]


def audit(document, binary, candidate_mesh):
    if document["asset"]["version"] != "2.0":
        raise ValueError("glTF version mismatch")
    claim = document["asset"]["extras"]["tokito"]
    if (claim.get("format") != "tokito.step-unqualified-occurrence-glb.v1"
            or claim.get("admission", {}).get("admitted") is not False
            or claim.get("geometryQualified") is not False
            or claim.get("pcbPlacementQualified") is not False
            or claim.get("unitsIndependentlyPreflighted") is not True
            or claim.get("componentReferenceMappingAvailable") is not False):
        raise ValueError("GLB qualification boundary is missing")
    nodes = document["nodes"]
    if not nodes:
        raise ValueError("GLB contains no occurrence nodes")
    selectors = set()
    reached = set()
    nodes_by_path = {}

    def visit(index):
        if index < 0 or index >= len(nodes) or index in reached:
            raise ValueError("node graph is invalid or cyclic")
        reached.add(index)
        tokito = nodes[index].get("extras", {}).get("tokito", {})
        selector = tokito.get("selectorId")
        path = tuple(tokito.get("sourceOccurrencePath", []))
        if (tokito.get("selectorContract") != "step-occurrence-path-v1"
                or tokito.get("componentSelectable") is not False
                or not isinstance(selector, str) or selector in selectors):
            raise ValueError("occurrence selector boundary is invalid")
        expected_selector = "step-occurrence:" + "/".join(str(part) for part in path)
        if selector != expected_selector:
            raise ValueError("selector id does not encode its source occurrence path")
        selectors.add(selector)
        if not path or path in nodes_by_path:
            raise ValueError("duplicate or missing occurrence path")
        nodes_by_path[path] = nodes[index]
        for child in nodes[index].get("children", []):
            visit(child)

    for root in document["scenes"][document["scene"]]["nodes"]:
        visit(root)
    if len(reached) != len(nodes):
        raise ValueError("GLB has unreachable occurrence nodes")

    occurrences = {tuple(row["occurrencePath"]): row for row in candidate_mesh["occurrences"]}
    if set(nodes_by_path) != set(occurrences):
        raise ValueError("GLB occurrence inventory differs from the STEP candidate")
    for path, node in nodes_by_path.items():
        emitted = node["extras"]["tokito"]
        source = occurrences[path]
        for emitted_key, source_key in (
                ("sourceOccurrenceId", "sourceOccurrenceId"),
                ("sourcePlacedById", "sourcePlacedById"),
                ("sourcePartDefinitionId", "sourcePartDefinitionId"),
                ("sourceProductId", "sourceProductId")):
            if emitted.get(emitted_key) != source.get(source_key):
                raise ValueError(f"occurrence provenance changed for {emitted_key}")
    triangles = 0
    primitives = 0
    degenerate_triangles = 0
    mesh_nodes = {node["mesh"]: node for node in nodes if "mesh" in node}
    for mesh_index, mesh in enumerate(document["meshes"]):
        node = mesh_nodes.get(mesh_index)
        if node is None:
            raise ValueError("GLB mesh has no occurrence selector node")
        for primitive in mesh["primitives"]:
            primitives += 1
            if primitive.get("mode", 4) != 4:
                raise ValueError("non-triangle primitive")
            extras = primitive.get("extras", {}).get("tokito", {})
            ordinals = extras.get("sourceTriangleOrdinals")
            face_ids = extras.get("sourceTriangleFaceIds")
            degenerate_ordinals = extras.get("degenerateTriangleOrdinals")
            if not isinstance(ordinals, list) or not isinstance(face_ids, list) or len(ordinals) != len(face_ids):
                raise ValueError("source triangle provenance mismatch")
            if not isinstance(degenerate_ordinals, list) or any(ordinal not in ordinals for ordinal in degenerate_ordinals):
                raise ValueError("degenerate triangle diagnostics mismatch")
            degenerate_triangles += len(degenerate_ordinals)
            indices_accessor = document["accessors"][primitive["indices"]]
            if indices_accessor["componentType"] != 5125 or indices_accessor["type"] != "SCALAR":
                raise ValueError("unexpected index accessor type")
            view = document["bufferViews"][indices_accessor["bufferView"]]
            start = view.get("byteOffset", 0) + indices_accessor.get("byteOffset", 0)
            count = indices_accessor["count"]
            if count % 3 or view["byteLength"] < count * 4:
                raise ValueError("index accessor length mismatch")
            values = struct.unpack_from("<" + "I" * count, binary, start)
            position_accessor = document["accessors"][primitive["attributes"]["POSITION"]]
            if position_accessor["componentType"] != 5126 or position_accessor["type"] != "VEC3":
                raise ValueError("unexpected position accessor type")
            positions_view = document["bufferViews"][position_accessor["bufferView"]]
            position_start = positions_view.get("byteOffset", 0) + position_accessor.get("byteOffset", 0)
            vertex_count = position_accessor["count"]
            if (positions_view["byteLength"] < vertex_count * 12 or not values
                    or max(values) >= vertex_count or values != tuple(range(vertex_count))):
                raise ValueError("position/index accessor mismatch")
            position_values = struct.unpack_from("<" + "f" * (vertex_count * 3), binary, position_start)
            if any(not (float("-inf") < value < float("inf")) for value in position_values):
                raise ValueError("non-finite exported coordinate")
            path = tuple(node["extras"]["tokito"]["sourceOccurrencePath"])
            shape_index = extras["sourceShapeIndex"]
            shape = occurrences[path]["shapes"][shape_index]
            source_triangles = shape["triangles"]
            source_vertices = shape["verticesMm"]
            ordinals = extras["sourceTriangleOrdinals"]
            source_faces = shape["sourceTriangleFaceIds"]
            if extras.get("sourceRepresentationItemId") != shape.get("sourceItemId"):
                raise ValueError("source STEP representation-item identity changed")
            source_shape_color = canonical_f32_color(shape.get("sourceItemRgba") or DEFAULT_COLOR)
            source_triangle_colors = shape["sourceTriangleRgba"]
            exported_color = extras["materialRgba"]
            if document["materials"][primitive["material"]]["pbrMetallicRoughness"]["baseColorFactor"] != exported_color:
                raise ValueError("GLB PBR base color differs from the primitive source-color record")
            if len(ordinals) * 3 != vertex_count or count != vertex_count:
                raise ValueError("position vertices do not map to recorded triangle provenance")
            expected_normals = []
            for output_triangle, source_ordinal in enumerate(ordinals):
                if not isinstance(source_ordinal, int) or not 0 <= source_ordinal < len(source_triangles):
                    raise ValueError("source triangle ordinal is outside its source shape")
                source_color = (source_triangle_colors[source_ordinal]
                                if source_ordinal < len(source_triangle_colors)
                                and source_triangle_colors[source_ordinal] is not None
                                else source_shape_color)
                if source_triangle_colors and source_triangle_colors[source_ordinal] is not None:
                    source_color = canonical_f32_color(source_color)
                if source_color != exported_color:
                    raise ValueError("GLB material differs from the source candidate triangle color")
                source_triangle = source_triangles[source_ordinal]
                source_points = [source_vertices[vertex_index] for vertex_index in source_triangle]
                for corner, vertex_index in enumerate(source_triangle):
                    expected = tuple(struct.unpack("<f", struct.pack("<f", source_vertices[vertex_index][axis] * 0.001))[0] for axis in range(3))
                    actual = tuple(position_values[output_triangle * 9 + corner * 3:output_triangle * 9 + corner * 3 + 3])
                    if actual != expected:
                        raise ValueError("decoded GLB point differs from source candidate position")
                expected_face = source_faces[source_ordinal] if source_ordinal < len(source_faces) else 0
                if extras["sourceTriangleFaceIds"][output_triangle] != expected_face:
                    raise ValueError("source face provenance changed during GLB encoding")
                a = [source_points[1][axis] - source_points[0][axis] for axis in range(3)]
                b = [source_points[2][axis] - source_points[0][axis] for axis in range(3)]
                normal = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
                length = sum(value * value for value in normal) ** 0.5
                expected_normals.append([0.0, 1.0, 0.0] if length == 0 else [value / length for value in normal])
            normal_accessor = document["accessors"][primitive["attributes"]["NORMAL"]]
            normals_view = document["bufferViews"][normal_accessor["bufferView"]]
            normals_start = normals_view.get("byteOffset", 0) + normal_accessor.get("byteOffset", 0)
            normals = struct.unpack_from("<" + "f" * (normal_accessor["count"] * 3), binary, normals_start)
            if (normal_accessor["count"] != vertex_count
                    or any(not (float("-inf") < value < float("inf")) for value in normals)):
                raise ValueError("normal accessor mismatch")
            for triangle_index, expected_normal in enumerate(expected_normals):
                if any(abs(normals[triangle_index * 9 + corner * 3 + axis] - expected_normal[axis]) > 2e-5
                       for axis in range(3) for corner in range(3)):
                    raise ValueError("flat GLB normal disagrees with emitted triangle winding")
            material = document["materials"][primitive["material"]]
            if material["pbrMetallicRoughness"]["baseColorFactor"] != extras["materialRgba"]:
                raise ValueError("GLB PBR base color differs from the recorded source color")
            triangles += count // 3
    if not triangles or not primitives:
        raise ValueError("GLB has no exported triangles")
    return {"nodes": len(nodes), "selectors": len(selectors), "primitives": primitives, "triangles": triangles,
        "degenerateTriangles": degenerate_triangles}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    source = args.source.resolve(strict=True)
    output = args.output.resolve()
    if hashlib.sha256(source.read_bytes()).hexdigest() != SOURCE_SHA256:
        raise ValueError("wrong pinned STEP source")
    output.mkdir(parents=True, exist_ok=False)
    glb = output / "candidate.glb"
    report = run([str(binary), "--research-export-diagnostic-glb", str(source), str(glb)], 0)
    if report.get("commandCompleted") is not True or report.get("admitted") is not False:
        raise ValueError("diagnostic export report is not explicit")
    if report.get("sourceSha256") != SOURCE_SHA256:
        raise ValueError("source digest mismatch")
    document, bin_data, glb_data = decode_glb(glb)
    candidate_report = run([str(binary), "--candidate-mesh-data", str(source)], 2)
    candidate_mesh = candidate_report.get("candidateMeshData")
    if (not isinstance(candidate_mesh, dict)
            or candidate_mesh.get("geometryQualified") is not False
            or candidate_mesh.get("pcbPlacementQualified") is not False):
        raise ValueError("reader candidate mesh is not explicitly unqualified")
    counts = audit(document, bin_data, candidate_mesh)
    diagnostic = report["diagnosticGlb"]
    if (diagnostic.get("outputSha256") != hashlib.sha256(glb_data).hexdigest()
            or diagnostic.get("outputBytes") != len(glb_data)
            or diagnostic.get("occurrenceCount") != counts["nodes"]
            or diagnostic.get("primitiveCount") != counts["primitives"]
            or diagnostic.get("triangleCount") != counts["triangles"]
            or diagnostic.get("degenerateTriangleCount") != counts["degenerateTriangles"]
            or diagnostic.get("temporaryFileCleanupCompleted") is not True
            or diagnostic.get("componentReferenceMappingAvailable") is not False):
        raise ValueError("GLB receipt does not match decoded output")

    original_hash = hashlib.sha256(glb.read_bytes()).hexdigest()
    refusal = run([str(binary), "--research-export-diagnostic-glb", str(source), str(glb)], 2)
    if (not str(refusal.get("error", "")).startswith("diagnostic_glb_output_must_be_new")
            or hashlib.sha256(glb.read_bytes()).hexdigest() != original_hash):
        raise ValueError("existing output was not safely refused")

    malformed = output / "malformed.step"
    malformed.write_bytes(b"ISO-10303-21; MALFORMED")
    nonexistent = output / "must-not-exist.glb"
    failure = run([str(binary), "--research-export-diagnostic-glb", str(malformed), str(nonexistent)], 2)
    if nonexistent.exists() or not str(failure.get("error", "")).startswith("reader_error: STEP parse error:"):
        raise ValueError("malformed STEP input was not refused without an artifact")
    if list(output.glob(".tokito-step-export-*.tmp")):
        raise ValueError("temporary output file remains after success or refusal")
    result = {"passed": True, "sourceSha256": SOURCE_SHA256, "receipt": diagnostic,
        "decodedGlb": counts, "outputSha256": original_hash,
        "candidateEvaluatorSha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "e2eScriptSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "independentGeometryOracle": False,
        "checks": ["GLB container and accessor readback", "one stable selector per STEP occurrence",
            "source occurrence identity/path, face provenance and material colors retained",
            "unqualified/component-unmapped status retained", "existing output refusal without partial overwrite",
            "malformed STEP refusal without output", "temporary output cleanup"]}
    (output / "receipt.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
