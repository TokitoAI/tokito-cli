"""Independent decimal-arithmetic checks of an explicit planar face receipt.

This checks a diagnostic, not STEP units, solid closure or model admission.
Usage: python check_planar_receipt.py RECEIPT SOURCE_STEP
"""
import hashlib
import json
import sys
from collections import Counter
from decimal import Decimal, localcontext
from pathlib import Path


def check(receipt, source):
    assert receipt["schema"] == "tokito.step-linear-planar-face.v1"
    assert "error" not in receipt
    assert receipt["sourceSha256"] == hashlib.sha256(source).hexdigest()
    assert receipt["admitted"] is False
    assert receipt["geometryQualified"] is False
    assert receipt["exportsProduced"] == []
    points = receipt["projectedVerticesSourceUnits"]
    triangles = receipt["triangles"]
    count = len(points)
    assert 3 <= count <= 512
    assert len(set(receipt["sourceVertexIds"])) == count
    assert len(triangles) == count - 2 == receipt["triangleCount"]
    assert receipt["boundarySegmentsPreserved"] == count
    with localcontext() as context:
        context.prec = 100
        points = [[Decimal.from_float(float(value)) for value in p] for p in points]
        polygon = sum(points[i][0]*points[(i+1)%count][1] - points[i][1]*points[(i+1)%count][0] for i in range(count))
        area = Decimal(0)
        directed = Counter()
        for triangle in triangles:
            assert len(set(triangle)) == 3
            assert all(isinstance(i,int) and 0 <= i < count for i in triangle)
            a,b,c = [points[i] for i in triangle]
            determinant = (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])
            assert determinant != 0
            assert (determinant > 0) == receipt["faceSameSense"]
            area += abs(determinant)
            for i in range(3):
                directed[triangle[i],triangle[(i+1)%3]] += 1
        assert abs(area-abs(polygon)) <= max(abs(polygon),Decimal(1))*Decimal("1e-12")
        boundary = {tuple(sorted((i,(i+1)%count))) for i in range(count)}
        edges = {tuple(sorted(edge)) for edge in directed}
        for a,b in edges:
            incidence = directed[a,b]+directed[b,a]
            if (a,b) in boundary:
                assert incidence == 1
            else:
                assert incidence == 2 and directed[a,b] == directed[b,a] == 1
        assert boundary <= edges
    return {"decimalAreaAndWinding":True,"boundaryAndInteriorEdges":True,
            "sourceHashVerified":True,"vertices":count,"triangles":len(triangles),
            "solidOrPlacementQualification":False}


if __name__ == "__main__":
    assert len(sys.argv) == 3, __doc__
    print(json.dumps(check(json.loads(Path(sys.argv[1]).read_text(encoding="utf-8-sig")),Path(sys.argv[2]).read_bytes())))
