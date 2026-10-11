"""Independent GLB substrate checks on an actual drilled source export."""
import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import struct
from esp8266_drill_inventory_e2e import FILES


def run(directory):
    raw = (directory / 'active.digital-twin.glb').read_bytes()
    length = struct.unpack_from('<I', raw, 12)[0]
    doc = json.loads(raw[20:20 + length])
    offset = 20 + length
    data = raw[offset + 8:]
    report = json.loads((directory / 'prepare-report.json').read_text())
    assert report['holesAndCutoutsIncluded'] is True
    evidence = report['drillEvidence']
    assert report['sourceSha256'] == '007d603df0effe46bc254c978def4d3225a9c5123803cd115ad99c087a26ea74'
    assert {row['sourceSha256'] for row in evidence['sourceInventories']} == {row[0] for row in FILES.values()}
    assert len(evidence['sourceInventories']) == 3
    for flag in ('assemblyAdmitted', 'geometryQualified', 'poseVerified', 'positionalGuidanceAllowed'):
        assert report[flag] is False
    assert evidence['openingCount'] == 281
    assert evidence['sourceBoardFrameVerified'] is False and evidence['physicalFitVerified'] is False
    assert report['componentCount'] == 34 and report['bodyCount'] == 62
    assert report['glbSha256'] == hashlib.sha256(raw).hexdigest()
    primitive = doc['meshes'][doc['nodes'][0]['mesh']]['primitives'][0]

    def attribute(name):
        accessor = doc['accessors'][primitive['attributes'][name]]
        view = doc['bufferViews'][accessor['bufferView']]
        assert accessor['componentType'] == 5126 and accessor['type'] == 'VEC3'
        return list(struct.iter_unpack('<fff', data[view['byteOffset']:view['byteOffset'] + accessor['count'] * 12]))

    points, normals = attribute('POSITION'), attribute('NORMAL')
    assert len(points) == len(normals) and len(points) % 3 == 0
    edges = Counter()
    directed = Counter()
    area = [0., 0.]
    surfaces = []
    for i in range(0, len(points), 3):
        triangle = points[i:i + 3]
        assert len(set(triangle)) == 3
        for a, b in zip(triangle, triangle[1:] + triangle[:1]):
            edges[tuple(sorted((a, b)))] += 1
            directed[(a, b)] += 1
        n = normals[i]
        assert all(math.isfinite(v) for p in triangle for v in p)
        assert abs(sum(v * v for v in n) - 1) < 2e-6
        if n[1] == 1 or n[1] == -1:
            # glTF metres/Y-up -> board XY millimetres.
            xy = [(p[0] * 1000, -p[2] * 1000) for p in triangle]
            signed = ((xy[1][0]-xy[0][0])*(xy[2][1]-xy[0][1])
                      -(xy[1][1]-xy[0][1])*(xy[2][0]-xy[0][0])) / 2
            assert (signed > 0) == (n[1] > 0)
            area[0 if n[1] > 0 else 1] += abs(signed)
            if n[1] > 0:
                surfaces.append(xy)
    assert all(count == 2 for count in edges.values()), 'substrate is not a closed edge manifold'
    assert all(directed[(a, b)] == directed[(b, a)] == 1 for a, b in edges), 'edge orientation mismatch'
    # f32 export error bound from coordinate magnitudes and edge lengths.
    coordinate_error = max(abs(v) for p in points for v in p) * 1000 * 2**-23
    edge_sum = sum(math.dist(a, b) for t in surfaces for a, b in zip(t, t[1:] + t[:1]))
    arithmetic_bound = coordinate_error * edge_sum * 2 + len(surfaces) * coordinate_error**2 * 4
    assert abs(area[0] - area[1]) <= arithmetic_bound
    assert abs(area[0] - evidence['polygonizedMaterialAreaMm2']) <= arithmetic_bound

    def contains(tri, p):
        cross = [((b[0]-a[0])*(p[1]-a[1])-(b[1]-a[1])*(p[0]-a[0]))
                 for a, b in zip(tri, tri[1:] + tri[:1])]
        return all(v >= 0 for v in cross) or all(v <= 0 for v in cross)

    centres = []
    holes = slots = 0
    for inventory in evidence['sourceInventories']:
        for row in inventory['holes']:
            centres.append(row['positionMm'])
            holes += 1
        for row in inventory['slots']:
            centres.extend([row['startMm'], row['endMm'],
                            [(a+b)/2 for a, b in zip(row['startMm'], row['endMm'])]])
            slots += 1
    assert (holes, slots) == (279, 2)
    assert all(not contains(t, p) for p in centres for t in surfaces), 'face covers a drill centre'
    receipt = {'schema': 'tokito.drilled-substrate-e2e.v1', 'passed': True,
               'glbSha256': report['glbSha256'], 'holes': holes, 'slots': slots,
               'substrateTriangles': len(points)//3, 'closedOrientedEdgeManifold': True,
               'topAreaMm2': area[0], 'bottomAreaMm2': area[1],
               'floatExportAreaBoundMm2': arithmetic_bound,
               'allOpeningCentresUncovered': True, 'physicalFitVerified': False}
    (directory / 'drilled-substrate-e2e.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--directory', type=Path, required=True)
    run(parser.parse_args().directory)
