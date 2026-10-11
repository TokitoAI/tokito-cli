"""Real positioned STEP candidate data; no engineering or PCB admission."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess


def validate_bounds(bounds):
    for key in ('min', 'max'):
        values = bounds[key]
        assert isinstance(values, list) and len(values) == 3
        assert all(type(value) in (int, float) and math.isfinite(value) for value in values)
    assert all(a <= b for a, b in zip(bounds['min'], bounds['max'], strict=True))


def run(binary, models, reference, output):
    # A non-first NaN must not disappear inside Python's maximum calculation.
    for invalid in (float('nan'), float('inf'), True, '1'):
        try:
            validate_bounds({'min': [0, invalid, 0], 'max': [1, 1, 1]})
        except AssertionError:
            pass
        else:
            raise AssertionError('invalid reference bound accepted')
    output.mkdir(parents=True, exist_ok=False)
    references = {r['file']: r for r in json.loads(reference.read_text())['models']}
    rows = []
    for ordinal in range(17):
        source = models / f'{ordinal}.step'
        process = subprocess.run([str(binary), '--candidate-mesh-data', str(source)],
                                 capture_output=True, timeout=90)
        assert process.returncode == 2 and not process.stderr
        assert len(process.stdout) <= 160 * 1024 * 1024
        report = json.loads(process.stdout)
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        assert report['sourceSha256'] == digest
        assert report['admitted'] is False and report['exportsProduced'] == []
        mesh, world = report['candidateMeshData'], report['assemblyOccurrenceWorld']
        assert 'error' not in mesh and mesh['sourceSha256'] == digest
        assert mesh['lengthUnit'] == 'mm' and world['independentRepresentationUnitsResolved']
        assert mesh['geometryQualified'] is False and mesh['pcbPlacementQualified'] is False
        assert mesh['surfaceCompletenessVerified'] is False
        assert mesh['readerWarningsTotal'] == len(mesh['readerWarnings']) == report['warningsTotal']
        minimum, maximum = [math.inf] * 3, [-math.inf] * 3
        vertices = triangles = 0
        paths = set()
        for occurrence in mesh['occurrences']:
            path = tuple(occurrence['occurrencePath'])
            assert path not in paths
            paths.add(path)
            for shape in occurrence['shapes']:
                vertices += len(shape['verticesMm'])
                triangles += len(shape['triangles'])
                assert shape['triangleFaceProvenanceCount'] == len(shape['sourceTriangleFaceIds'])
                assert shape['triangleFaceProvenanceAligned'] == (
                    len(shape['sourceTriangleFaceIds']) == len(shape['triangles']))
                for point in shape['verticesMm']:
                    assert len(point) == 3 and all(math.isfinite(x) for x in point)
                    for axis, value in enumerate(point):
                        minimum[axis] = min(minimum[axis], value)
                        maximum[axis] = max(maximum[axis], value)
                assert all(len(t) == 3 and all(0 <= i < len(shape['verticesMm']) for i in t)
                           for t in shape['triangles'])
        assert len(paths) == mesh['occurrenceCount'] == world['occurrenceCount']
        assert vertices == mesh['positionedVertexCount'] == world['occurrenceVertices']
        assert triangles == mesh['positionedTriangleCount'] == world['occurrenceTriangles']
        assert minimum == world['worldBoundsMm']['min'] and maximum == world['worldBoundsMm']['max']
        external = references[source.name]
        assert external['sourceSha256'] == digest
        validate_bounds(external['boundsMm'])
        delta = max(abs(a - b) for bounds, expected in (
            (minimum, external['boundsMm']['min']), (maximum, external['boundsMm']['max']))
                    for a, b in zip(bounds, expected, strict=True))
        # Preserve the three previously defined reference-bound regression gates.
        # Remaining model comparisons are diagnostic, never qualified by this test.
        if ordinal in (0, 14, 16):
            assert delta <= 0.05, (ordinal, delta)
        artifact = output / f'{ordinal}.mesh.json'
        artifact.write_bytes(process.stdout)
        rows.append({'model': source.name, 'sourceSha256': digest,
                     'meshArtifactSha256': hashlib.sha256(process.stdout).hexdigest(),
                     'vertices': vertices, 'triangles': triangles, 'occurrences': len(paths),
                     'boundsMm': {'min': minimum, 'max': maximum},
                     'externalBoundsMaximumDeltaMm': delta,
                     'withinDiagnosticTolerance005Mm': delta <= 0.05,
                     'readerWarnings': mesh['readerWarningsTotal'], 'geometryQualified': False})
        print(f'PASS candidate {source.name}: {vertices} vertices / {triangles} triangles; '
              f'external bounds delta {delta:.6g} mm', flush=True)
    receipt = {'passed': True, 'modelCount': 17, 'models': rows,
               'externalReferenceSha256': hashlib.sha256(reference.read_bytes()).hexdigest(),
               'geometryQualified': False, 'boardPlacementVerified': False,
               'assembledBoardGlbProduced': False,
               'scope': 'candidate output consistency and three bounded reference regressions; '
                        'other reference comparisons are diagnostic, not geometry acceptance'}
    (output / 'receipt.json').write_text(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    for name in ('binary', 'models', 'reference', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    run(args.binary.resolve(strict=True), args.models.resolve(strict=True),
        args.reference.resolve(strict=True), args.output.resolve())
