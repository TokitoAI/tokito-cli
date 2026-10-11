"""Independent checks on actual source-mesh replacement and PCB-origin corrections.

Does not certify the source surfaces, fitted population or phone registration.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess
import tempfile
import copy


def container(path):
    raw = path.read_bytes()
    magic, version, length = struct.unpack_from('<4sII', raw)
    assert (magic, version, length) == (b'glTF', 2, len(raw))
    size, kind = struct.unpack_from('<I4s', raw, 12)
    assert kind == b'JSON'
    doc = json.loads(raw[20:20+size])
    offset = 20+size
    count, kind = struct.unpack_from('<I4s', raw, offset)
    assert kind == b'BIN\0' and count == len(raw)-offset-8
    return raw, doc, raw[offset+8:]


def run(baseline, output, spec_path, pcb, binary=None):
    old_bytes, old, old_bin = container(baseline / 'active.digital-twin.glb')
    new_bytes, new, data = container(output / 'active.digital-twin.glb')
    spec = json.loads(spec_path.read_text())
    report = json.loads((output / 'prepare-report.json').read_text())
    assert hashlib.sha256(old_bytes).hexdigest() == spec['baselineGlbSha256'] == report['baselineGlbSha256']
    assert hashlib.sha256(new_bytes).hexdigest() == report['outputGlbSha256']
    assert data.startswith(old_bin)
    old_map = json.loads((baseline / 'component-map.json').read_text())
    new_map = json.loads((output / 'component-map.json').read_text())
    assert [(r['component_id'], r['reference']) for r in old_map['components']] == [
        (r['component_id'], r['reference']) for r in new_map['components']]
    population = spec.get('populationOverrides', [])
    assert new_map['component_count'] == 391 and new_map['renderable_count'] == 354-len(population)
    assert len(report['replacements']) == 3
    assert hashlib.sha256(pcb.read_bytes()).hexdigest() == 'c0d1389ce0710539104a3f2c4ca5f382c6b5fa2cce0edde315b4d73360f75b26'
    pcb_text = pcb.read_text()
    expected_origins = {'U8': (22.208007, -24.881002), 'J10': (3.054883, -36.619002), 'J11': (3.054883, -21.506002)}
    changed = set()
    comparisons = []
    for model, result in zip(spec['models'], report['replacements']):
        reference = model['reference']
        assert result['reference'] == reference
        source_bytes = (spec_path.parent / model['meshReport']).read_bytes()
        assert hashlib.sha256(source_bytes).hexdigest() == model['meshReportSha256']
        source = json.loads(source_bytes)
        assert source['sourceSha256'] == model['stepSha256']
        index = next(i for i, n in enumerate(old['nodes']) if n.get('name') == 'ref:'+reference)
        changed.add(index)
        assert new['nodes'][index]['translation'] == [0, 0, 0]
        placement = model['placementOverride']
        parent = placement['nodeIndex']
        changed.add(parent)
        assert old['nodes'][parent]['translation'] == placement['expectedTranslationMetres']
        assert new['nodes'][parent]['translation'] == placement['translationMetres']
        x, y = expected_origins[reference]
        assert abs(new['nodes'][parent]['translation'][0]*1000-x) < 1e-12
        assert abs(new['nodes'][parent]['translation'][2]*1000-y) < 1e-12
        assert f'(at {x:.6f} {y:.6f} -90)' in pcb_text
        assert new['nodes'][parent].get('rotation') == old['nodes'][parent].get('rotation')
        for key in ('translation',):
            before, after = dict(old['nodes'][parent]), dict(new['nodes'][parent])
            before.pop(key); after.pop(key)
            assert before == after
        primitives = new['meshes'][new['nodes'][index]['mesh']]['primitives']
        assert len(primitives) == 1
        primitive = primitives[0]

        def floats(name, columns):
            a = new['accessors'][primitive['attributes'][name]]
            v = new['bufferViews'][a['bufferView']]
            assert a['componentType'] == 5126 and a['type'] == 'VEC'+str(columns)
            return list(struct.iter_unpack('<'+'f'*columns, data[v['byteOffset']:v['byteOffset']+a['count']*columns*4]))

        p, normals, colors = floats('POSITION',3), floats('NORMAL',3), floats('COLOR_0',4)
        assert len(p) == len(normals) == len(colors) == result['triangleCount']*3
        source_nonzero, source_zero = [], []
        for oi, occurrence in enumerate(source['candidateMeshData']['occurrences']):
            for si, shape in enumerate(occurrence['shapes']):
                vs = shape['verticesMm']
                for ti, tri in enumerate(shape['triangles']):
                    a,b,c = [vs[i] for i in tri]
                    u,v = [b[i]-a[i] for i in range(3)], [c[i]-a[i] for i in range(3)]
                    n = [u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]]
                    if n == [0,0,0]:
                        source_zero.append((oi,si,ti))
                    else:
                        source_nonzero.append(([a,b,c],shape['sourceTriangleRgba'][ti],n))
        assert len(source_zero) == result['omittedExactZeroAreaTriangleCount']
        assert {(r['occurrenceIndex'],r['shapeIndex'],r['triangleIndex']) for r in result['omittedExactZeroAreaTriangles']} == set(source_zero)
        assert len(source_nonzero) == result['triangleCount']
        for ti, (original, rgba, normal) in enumerate(source_nonzero):
            length = math.hypot(*normal)
            expected_normal = [normal[0]/length,normal[2]/length,-normal[1]/length]
            for vi, v in enumerate(original):
                expected = [v[0]*.001,v[2]*.001,-v[1]*.001]
                at = ti*3+vi
                assert all(abs(a-b) <= max(abs(b)*2**-23,1e-15) for a,b in zip(p[at],expected))
                assert all(abs(a-b) < 2e-7 for a,b in zip(normals[at],expected_normal))
                assert all(abs(a-b) < 6e-8 for a,b in zip(colors[at],rgba))
            a,b,c = p[ti*3:ti*3+3]
            u,v = [b[i]-a[i] for i in range(3)], [c[i]-a[i] for i in range(3)]
            cross = [u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]]
            assert math.hypot(*cross) > 0, 'triangle collapsed during f32 export'
            assert sum(a*b for a,b in zip(cross,expected_normal)) > 0, 'export winding mismatch'
        comparisons.append({'reference':reference,'triangles':len(source_nonzero),'exactZeroOmitted':len(source_zero),
                            'sourceCoordinatesColoursNormalsCompared':True,'pcbPadOriginXYCompared':True})
    for override in population:
        ref = override['reference']
        source_bytes = (spec_path.parent / override['evidenceFile']).read_bytes()
        assert hashlib.sha256(source_bytes).hexdigest() == override['evidenceSha256']
        evidence = json.loads(source_bytes)
        rows = [r for r in evidence['components'] if r['refdes'] == ref]
        assert len(rows) == 1 and rows[0]['bom_item_id'] == override['bomItemId']
        assert rows[0]['dnp_explicit_in_bom'] is True and rows[0]['bom_columns']['ASSY_OPT'] == 'DNP'
        index = next(i for i,n in enumerate(old['nodes']) if n.get('name') == 'ref:'+ref)
        changed.add(index)
        assert 'mesh' not in new['nodes'][index]
        assert new['nodes'][index]['name'] == 'population-unfitted:'+ref
        row = next(r for r in new_map['components'] if r['reference'] == ref)
        assert row['glb_selector'] is None and row['needs_3d_body'] is False
        assert row['assembly_state'] == 'not_fitted' and row['category'] == 'unpopulated_footprint'
        for i,n in enumerate(new['nodes']):
            assert not (n.get('mesh') == old['nodes'][index]['mesh'] and n.get('name','').startswith('ref:'))
    assert report.get('populationOverrideCount', 0) == len(population)
    assert len(new['nodes']) == len(old['nodes'])
    for i in range(len(old['nodes'])):
        if i not in changed:
            assert new['nodes'][i] == old['nodes'][i]
    for flag in ('geometryQualified','poseVerified','physicalFitVerified','positionalGuidanceAllowed'):
        assert report[flag] is False
    negative_journeys = 0
    if binary is not None:
        with tempfile.TemporaryDirectory(dir=output.parent) as temp:
            scratch = Path(temp)
            def refused(candidate, extra_args=None, existing=False):
                nonlocal negative_journeys
                candidate_path = scratch/'spec.json'
                # Retain evidence paths in the same CLI-owned source folder.
                candidate_path = spec_path.parent/'negative-refinement-e2e.json'
                assert not candidate_path.exists()
                candidate_path.write_text(json.dumps(candidate))
                target = output if existing else scratch/('out'+str(negative_journeys))
                try:
                    result = subprocess.run([str(binary),str(baseline/'active.digital-twin.glb'),
                        str(baseline/'component-map.json'),str(baseline/'active.digital-twin.json'),
                        str(candidate_path),str(target),*(extra_args if extra_args is not None else ['--allow-partial'])],
                        capture_output=True,timeout=60)
                    assert result.returncode != 0
                    error = json.loads(result.stdout)
                    assert error['admitted'] is False and 'error' in error
                    if not existing: assert not target.exists()
                    negative_journeys += 1
                finally:
                    candidate_path.unlink()
            refused(spec, extra_args=[])
            refused(spec, existing=True)
            invalid = copy.deepcopy(spec); invalid['baselineGlbSha256']='0'*64; refused(invalid)
            invalid = copy.deepcopy(spec); invalid['models'][0]['meshReportSha256']='0'*64; refused(invalid)
            invalid = copy.deepcopy(spec); invalid['models'][0]['omitExactZeroAreaTriangles']=False; refused(invalid)
            if population:
                invalid = copy.deepcopy(spec); invalid['populationOverrides'][0]['evidenceSha256']='0'*64; refused(invalid)
                invalid = copy.deepcopy(spec); invalid['populationOverrides'][0]['reference']='U1'; refused(invalid)
    receipt = {'schema':'tokito.nxp-refinement-e2e.v1','passed':True,'outputGlbSha256':report['outputGlbSha256'],
               'comparisons':comparisons,'notFittedBodiesRemoved':[r['reference'] for r in population],
               'negativeJourneys':negative_journeys,'unchangedOtherNodes':True,'binaryPrefixPreserved':True,'physicalFitVerified':False}
    (output/'refinement-e2e.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(receipt))


if __name__ == '__main__':
    p=argparse.ArgumentParser()
    for name in ('baseline','output','spec','pcb'):p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--binary',type=Path)
    p.add_argument('--generate',action='store_true')
    p.add_argument('--cli-binary',type=Path)
    a=p.parse_args()
    if a.generate:
        assert a.binary and not a.output.exists()
        result=subprocess.run([str(a.binary),str(a.baseline/'active.digital-twin.glb'),
            str(a.baseline/'component-map.json'),str(a.baseline/'active.digital-twin.json'),
            str(a.spec),str(a.output),'--allow-partial'],capture_output=True,timeout=120)
        assert result.returncode == 0, result.stdout.decode(errors='replace')
    run(a.baseline,a.output,a.spec,a.pcb,a.binary)
    if a.cli_binary:
        common=[str(a.output/'active.digital-twin.glb'),'--component-map',str(a.output/'component-map.json'),
                '--manifest',str(a.output/'active.digital-twin.json')]
        result=subprocess.run([str(a.cli_binary),'twin','inspect',*common],capture_output=True,timeout=60)
        assert result.returncode == 0, result.stdout.decode(errors='replace')
        index=json.loads(result.stdout)
        assert index['componentCount'] == 391 and index['renderableCount'] == 351
        result=subprocess.run([str(a.cli_binary),'twin','prototype',*common,'--output',str(a.output/'prototype'),
                               '--allow-partial'],capture_output=True,timeout=120)
        assert result.returncode == 0, result.stdout.decode(errors='replace')
        receipt=json.loads((a.output/'refinement-e2e.json').read_text())
        receipt['shippingCliImportAndPrototypePassed']=True
        (a.output/'refinement-e2e.json').write_text(json.dumps(receipt,indent=2)+'\n')
