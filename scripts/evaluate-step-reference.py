"""Off-device independent STEP evaluation; never a shipping CAD dependency.

Run with FreeCAD's Python interpreter. Mesh bounds are evaluated surface bounds,
not the potentially larger B-spline control-pole envelope of Shape.BoundBox.
The report does not certify assembly transforms, physical fit or AR identity.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path


def evaluated_bounds(points):
    if not points:
        raise ValueError("empty evaluated mesh")
    minimum, maximum = [math.inf] * 3, [-math.inf] * 3
    for point in points:
        values = (point.x, point.y, point.z)
        if not all(math.isfinite(v) for v in values):
            raise ValueError("nonfinite evaluated mesh")
        for axis, value in enumerate(values):
            minimum[axis] = min(minimum[axis], value)
            maximum[axis] = max(maximum[axis], value)
    return {"min": minimum, "max": maximum}


def run(source, output, deflection):
    import FreeCAD
    import Part

    if not math.isfinite(deflection) or not 0.001 <= deflection <= 0.1:
        raise ValueError("deflection must be finite and within 0.001..0.1 mm")
    source = source.resolve(strict=True)
    if output.exists():
        raise FileExistsError("refusing to overwrite independent reference")
    paths = sorted(source.glob("*.step"))
    if not paths or len(paths) > 256:
        raise ValueError("reference requires 1..256 STEP files")
    rows = []
    for path in paths:
        if path.is_symlink() or not 1 <= path.stat().st_size <= 32 * 1024 * 1024:
            raise ValueError("linked, empty or over-limit source")
        data = path.read_bytes()
        if data.startswith(b"version https://git-lfs.github.com/spec/v1"):
            raise ValueError("STEP source is an unmaterialized LFS pointer")
        shape = Part.read(str(path))
        if shape.isNull() or not shape.isValid():
            raise ValueError(f"independent B-rep import invalid: {path.name}")
        points, triangles = shape.tessellate(deflection)
        if len(points) > 2_000_000 or len(triangles) > 4_000_000:
            raise ValueError("evaluated reference exceeds mesh budget")
        bounds = evaluated_bounds(points)
        for triangle in triangles:
            if len(triangle) != 3 or any(i < 0 or i >= len(points) for i in triangle):
                raise ValueError("invalid evaluated triangle")
        raw = shape.BoundBox
        brep = {"min": [raw.XMin, raw.YMin, raw.ZMin],
                "max": [raw.XMax, raw.YMax, raw.ZMax]}
        if not all(math.isfinite(v) for side in brep.values() for v in side):
            raise ValueError("nonfinite B-rep bounds")
        delta = max(abs(a - b) for side in ("min", "max")
                    for a, b in zip(bounds[side], brep[side]))
        # Bind the bytes actually read by the independent importer. Concurrent
        # source replacement cannot be silently associated with the first hash.
        if path.read_bytes() != data:
            raise ValueError("source changed during independent evaluation")
        rows.append({"file": path.name, "sourceSha256": hashlib.sha256(data).hexdigest(),
                     "sourceBytes": len(data), "boundsMm": bounds,
                     "boundsMethod": "evaluated-triangle-vertices",
                     "brepBoundsMm": brep, "brepVersusEvaluatedDeltaMm": delta,
                     "vertices": len(points), "triangles": len(triangles),
                     "faces": len(shape.Faces), "solids": len(shape.Solids),
                     "independentBrepValid": True, "surfaceFidelityQualified": False,
                     "assemblyPoseQualified": False})
        print(f"Evaluated {path.name}: {len(triangles)} triangles; bounds-method delta {delta:.6g} mm",
              flush=True)
    report = {"schema": "tokito.independent-step-evaluation.v1",
              "evaluator": "FreeCAD/OCCT, off-device QA only",
              "evaluatorVersion": list(FreeCAD.Version()), "deflectionMm": deflection,
              "models": rows, "geometryQualified": False, "physicalFitVerified": False,
              "scope": "independent B-rep validity and evaluated mesh bounds, not surface or assembly acceptance"}
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", encoding="utf-8") as stream:
        json.dump(report, stream, indent=2, allow_nan=False)
        stream.write("\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--deflection-mm", type=float, default=0.01)
    args = parser.parse_args()
    run(args.source, args.output, args.deflection_mm)
