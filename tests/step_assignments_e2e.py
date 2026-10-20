"""Exercise exact PCB identity + declared-unit STEP import into selectable GLB."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import tempfile

from e2e_temp import e2e_temp_root
from glb_e2e import accessor_floats, invoke, parse_glb

ROOT = Path(__file__).resolve().parents[1]
BOARD_FIXTURE = ROOT / "tests/fixtures/assembly-e2e/single-resistor.kicad_pcb"
STEP_FIXTURE = ROOT / "tests/fixtures/step/declared-mm-triangle.step"
WRL_FIXTURE = ROOT / "tests/fixtures/open-source/kicad-resistor-models/Resistor_SMD.3dshapes/R_0603_1608Metric.wrl"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bounds(points: list[tuple[float, float, float]]) -> tuple[tuple[float, ...], tuple[float, ...]]:
    return tuple(min(point[axis] for point in points) for axis in range(3)), tuple(
        max(point[axis] for point in points) for axis in range(3)
    )


def close(actual: tuple[float, ...], expected: tuple[float, ...], tolerance: float = 1e-7) -> bool:
    return all(abs(a - e) <= tolerance for a, e in zip(actual, expected))


def main(binary: Path) -> None:
    with tempfile.TemporaryDirectory(
        prefix="step-assignment-e2e-", dir=e2e_temp_root(ROOT / "target")
    ) as temp:
        work = Path(temp)
        board = work / "board.kicad_pcb"
        text = BOARD_FIXTURE.read_text(encoding="utf-8").rstrip()
        text = text[:-1] + '\n(footprint "test" (layer "B.Cu") (at 3 4 90) (uuid "back-r2") (property "Reference" "R2"))\n)'
        board.write_text(text, encoding="utf-8")
        model_root = work / "models"
        model_root.mkdir()
        step_path = model_root / "plate.step"
        step_path.write_bytes(STEP_FIXTURE.read_bytes())

        template_path = work / "template.json"
        invoke(binary, "model", "assignments", "template", board, "--output", template_path)
        manifest = json.loads(template_path.read_text(encoding="utf-8"))
        assert [row["reference"] for row in manifest["assignments"]] == ["R1", "R2"]
        identities = {row["reference"]: row["componentUuid"] for row in manifest["assignments"]}
        for row in manifest["assignments"]:
            row["evidence"] = "Checked-in planar STEP fixture; assignment plumbing only, no physical board claim"
            row["model"] = {
                "path": "plate.step",
                "sha256": sha256(step_path),
                "units": "step-declared",
                "scale": [1, 1, 1],
                "rotationDeg": [0, 0, 0],
                "offsetMm": [0, 0, 0],
            }
        manifest_path = work / "assignments.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

        verified = json.loads(
            invoke(
                binary,
                "model", "assignments", "verify", board,
                "--manifest", manifest_path, "--project-root", model_root, "--json",
            ).stdout
        )
        assert verified["supportedGeometryCount"] == 2
        assert verified["assignments"][0]["modelUnitsClaim"] == "step-declared"
        assert verified["assignments"][0]["sourceUnitMetres"] == 0.001
        assert "unverified" in verified["assignments"][0]["geometryDiagnostics"][0]
        assert verified["physicalQualificationVerified"] is False
        assert verified["modelPhysicalMatchVerified"] is False

        output = work / "assigned.glb"
        args = (
            "export", "glb", board,
            "--model-assignments", manifest_path,
            "--project-root", model_root,
            "--output", output,
            "--json-report",
        )
        invoke(binary, *args, succeeds=False)
        assert not output.exists(), "STEP model must not export without explicit partial consent"
        report = json.loads(invoke(binary, *args, "--allow-partial").stdout)
        assert report["geometryVerified"] is False
        assert report["placementVerified"] is False
        assert report["assignments"]["physicalQualificationVerified"] is False
        assert report["assignments"]["wholeBoardDigitalTwinComplete"] is False
        assert report["coverage"]["componentModelsEmitted"] == 2
        assert report["scope"] == "board_shell_and_explicitly_assigned_models"

        document, payload = parse_glb(output)
        selectors = {
            node["name"]: node
            for node in document["nodes"]
            if node.get("name", "").startswith("ref:")
        }
        assert set(selectors) == {"ref:R1", "ref:R2"}
        for entry_index, reference in enumerate(("R1", "R2")):
            assert selectors[f"ref:{reference}"]["extras"]["tokito"]["componentUuid"] == identities[reference]
            assert selectors[f"ref:{reference}"]["extras"]["tokito"]["selectable"] is True
            translation = selectors[f"ref:{reference}"].get("translation", [0.0, 0.0, 0.0])
            primitive = document["meshes"][selectors[f"ref:{reference}"]["mesh"]]["primitives"][0]
            provenance = primitive["extras"]["tokito"]
            assert provenance["meshCoordinateFrame"] == "component_local_glb_metres"
            assert provenance["sourceAssetSha256"] == sha256(step_path)
            assert provenance["sourceAssetFormat"] == "step"
            assert provenance["manifestSha256"] == sha256(manifest_path)
            assert provenance["manifestEntryIndex"] == entry_index
            assert any("unverified" in item for item in provenance["geometryDiagnostics"])
            values = accessor_floats(document, payload, primitive["attributes"]["POSITION"])
            points = [
                tuple(values[index + axis] + translation[axis] for axis in range(3))
                for index in range(0, len(values), 3)
            ]
            actual_min, actual_max = bounds(points)
            if reference == "R1":
                expected_min = (-0.002, 0.00085, 0.0)
                expected_max = (0.0, 0.00085, 0.001)
            else:
                expected_min = (-0.002, -0.00085, 0.001)
                expected_max = (0.0, -0.00085, 0.002)
            assert close(actual_min, expected_min), (reference, actual_min, actual_max)
            assert close(actual_max, expected_max), (reference, actual_min, actual_max)
        # Independent bounds cover STEP's 1 x 2 mm planar surface, 90 degree
        # footprint rotations, board centre=(5,5), both copper sides, and lift.

        # Preserve the established WRL assignment report and GLB contract when
        # STEP-specific diagnostics are added to the same export path.
        wrl_path = model_root / "resistor.wrl"
        wrl_path.write_bytes(WRL_FIXTURE.read_bytes())
        for row in manifest["assignments"]:
            row["evidence"] = "Pinned open-source VRML parser regression fixture; not physical fit evidence"
            row["model"] = {
                "path": "resistor.wrl",
                "sha256": sha256(wrl_path),
                "units": "millimetres",
                "scale": [1, 1, 1],
                "rotationDeg": [0, 0, 0],
                "offsetMm": [0, 0, 0],
            }
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        wrl_output = work / "assigned-wrl.glb"
        wrl_args = (
            "export", "glb", board,
            "--model-assignments", manifest_path,
            "--project-root", model_root,
            "--output", wrl_output,
            "--json-report",
        )
        wrl_report = json.loads(invoke(binary, *wrl_args, "--allow-partial").stdout)
        assert wrl_report["scope"] == "board_shell_and_explicitly_assigned_wrl_models"
        assert "geometryDiagnostics" not in wrl_report["assignments"]["components"][0]
        assert wrl_report["assignments"]["components"][0]["evidenceClaim"] == manifest["assignments"][0]["evidence"]
        wrl_verify = json.loads(
            invoke(
                binary,
                "model", "assignments", "verify", board,
                "--manifest", manifest_path, "--project-root", model_root, "--json",
            ).stdout
        )
        for row in wrl_verify["assignments"]:
            assert "sourceUnitMetres" not in row
            assert "geometryDiagnostics" not in row
            assert row["evidenceClaim"] == manifest["assignments"][0]["evidence"]
        wrl_document, _ = parse_glb(wrl_output)
        wrl_selectors = {
            node["name"]: node
            for node in wrl_document["nodes"]
            if node.get("name", "").startswith("ref:")
        }
        assert set(wrl_selectors) == {"ref:R1", "ref:R2"}
        for selector in wrl_selectors.values():
            assert "translation" not in selector
            primitive = wrl_document["meshes"][selector["mesh"]]["primitives"][0]
            provenance = primitive["extras"]["tokito"]
            assert provenance["selectable"] is False
            assert "sourceAssetFormat" not in provenance
            assert "geometryDiagnostics" not in provenance

        # Exercise the parser's planar B-rep route with an OPEN_SHELL surface.
        # This proves only candidate face tessellation, not a closed solid.
        brep_path = model_root / "planar-open-brep.step"
        brep_path.write_text(planar_open_brep_step(), encoding="ascii")
        manifest["assignments"][0]["model"].update(
            path="planar-open-brep.step", sha256=sha256(brep_path), units="step-declared"
        )
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        brep_verified = json.loads(
            invoke(
                binary,
                "model", "assignments", "verify", board,
                "--manifest", manifest_path, "--project-root", model_root, "--json",
            ).stdout
        )
        assert brep_verified["supportedGeometryCount"] == 2, brep_verified
        assert brep_verified["assignments"][0]["assetFormat"] == "step"
        brep_output = work / "planar-open-brep.glb"
        brep_report = json.loads(
            invoke(
                binary,
                "export", "glb", board,
                "--model-assignments", manifest_path,
                "--project-root", model_root,
                "--output", brep_output,
                "--json-report", "--allow-partial",
            ).stdout
        )
        assert brep_report["coverage"]["componentModelsEmitted"] == 2
        brep_document, brep_payload = parse_glb(brep_output)
        brep_selector = next(
            node for node in brep_document["nodes"] if node.get("name") == "ref:R1"
        )
        brep_primitive = brep_document["meshes"][brep_selector["mesh"]]["primitives"][0]
        brep_positions = accessor_floats(
            brep_document, brep_payload, brep_primitive["attributes"]["POSITION"]
        )
        brep_points = list(zip(*[iter(brep_positions)] * 3))
        brep_min, brep_max = bounds(brep_points)
        brep_extent = tuple(high - low for low, high in zip(brep_min, brep_max))
        assert close(brep_extent, (0.001, 0.0, 0.001), tolerance=1e-6), brep_extent
        triangle_count = (
            brep_document["accessors"][brep_primitive["indices"]]["count"] // 3
            if "indices" in brep_primitive
            else len(brep_positions) // 9
        )
        assert triangle_count == 2, triangle_count
        assert brep_report["assignments"]["physicalQualificationVerified"] is False

        # A STEP whose representation context has no declared length unit must
        # be rejected rather than inheriting the reader's default millimetres.
        bad_step = model_root / "missing-unit.step"
        bad_bytes = STEP_FIXTURE.read_bytes().replace(
            b"GLOBAL_UNIT_ASSIGNED_CONTEXT((#1,#2))",
            b"GLOBAL_UNIT_ASSIGNED_CONTEXT((#2))",
        )
        assert bad_bytes != STEP_FIXTURE.read_bytes()
        bad_step.write_bytes(bad_bytes)
        manifest["assignments"][0]["model"].update(
            path="missing-unit.step", sha256=sha256(bad_step), units="step-declared"
        )
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        bad_report = json.loads(
            invoke(
                binary,
                "model", "assignments", "verify", board,
                "--manifest", manifest_path, "--project-root", model_root, "--json",
            ).stdout
        )
        assert bad_report["supportedGeometryCount"] == 1
        assert bad_report["assignments"][0]["status"] == "resolved_geometry_rejected"

    print(
        "PASS STEP assignment E2E: declared units, exact board UUID/ref selector, "
        "hand-calculated top/back placement, planar open-shell B-rep candidate, "
        "WRL report compatibility, explicit partial consent and missing-unit refusal"
    )


def planar_open_brep_step() -> str:
    """One planar, single-face OPEN_SHELL fixture; deliberately not a solid."""
    entities = [
        "#1=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));",
        "#2=(PLANE_ANGLE_UNIT() NAMED_UNIT(*) SI_UNIT($,.RADIAN.));",
        "#3=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT((#1,#2)) REPRESENTATION_CONTEXT('',''));",
    ]
    points = ((0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0))
    for index, point in enumerate(points):
        point_id = 10 + index
        vertex_id = 20 + index
        entities.append(
            f"#{point_id}=CARTESIAN_POINT('',({point[0]}. ,{point[1]}. ,{point[2]}.));"
            f"#{vertex_id}=VERTEX_POINT('',#{point_id});"
        )
    directions = ((1, 0, 0), (0, 1, 0), (-1, 0, 0), (0, -1, 0))
    for index, direction in enumerate(directions):
        direction_id = 30 + index
        vector_id = 40 + index
        line_id = 50 + index
        edge_id = 60 + index
        oriented_edge_id = 70 + index
        next_vertex = 20 + ((index + 1) % 4)
        current_vertex = 20 + index
        entities.append(
            f"#{direction_id}=DIRECTION('',({direction[0]}. ,{direction[1]}. ,{direction[2]}.));"
            f"#{vector_id}=VECTOR('',#{direction_id},1.);"
            f"#{line_id}=LINE('',#{10 + index},#{vector_id});"
            f"#{edge_id}=EDGE_CURVE('',#{current_vertex},#{next_vertex},#{line_id},.T.);"
            f"#{oriented_edge_id}=ORIENTED_EDGE('',*,*,#{edge_id},.T.);"
        )
    entities.extend(
        [
            "#80=EDGE_LOOP('',(#70,#71,#72,#73));",
            "#81=FACE_OUTER_BOUND('',#80,.T.);",
            "#90=CARTESIAN_POINT('',(0.,0.,0.));",
            "#91=DIRECTION('',(0.,0.,1.));",
            "#92=DIRECTION('',(1.,0.,0.));",
            "#93=AXIS2_PLACEMENT_3D('',#90,#91,#92);",
            "#94=PLANE('',#93);",
            "#95=ADVANCED_FACE('',(#81),#94,.T.);",
            "#96=OPEN_SHELL('',(#95));",
            "#97=MANIFOLD_SURFACE_SHAPE_REPRESENTATION('planar open face',(#96),#3);",
        ]
    )
    return (
        "ISO-10303-21;HEADER;FILE_DESCRIPTION(('Tokito planar B-rep E2E'),'2;1');"
        "FILE_NAME('planar-open-brep.step','2026-10-20T00:00:00',('Tokito CLI'),"
        "('Tokito CLI'),'','Tokito CLI','');FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));"
        "ENDSEC;DATA;" + "".join(entities) + "ENDSEC;END-ISO-10303-21;"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    main(parser.parse_args().binary.resolve())
