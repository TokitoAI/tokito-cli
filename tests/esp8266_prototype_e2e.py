"""Real Altium/STEP candidate assembly subprocess and GLB structural acceptance.

Does not certify surface fidelity, component association, or physical placement.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess


def run(binary, pcb, meshes, output, cli_binary=None):
    source_run = subprocess.run([str(binary), str(pcb)], capture_output=True, timeout=45)
    assert source_run.returncode == 2
    source = json.loads(source_run.stdout)
    assert source['sourceSha256'] == hashlib.sha256(pcb.read_bytes()).hexdigest()
    refused = subprocess.run([str(binary), 'export-prototype', str(pcb), str(meshes), str(output)],
                             capture_output=True, timeout=45)
    assert refused.returncode == 2 and not output.exists()
    result = subprocess.run([str(binary), 'export-prototype', str(pcb), str(meshes), str(output),
                             '--allow-partial'], capture_output=True, timeout=120)
    assert result.returncode == 0, result.stdout.decode(errors='replace')
    report = json.loads(result.stdout)
    assert report['componentCount'] == 34 and report['bodyCount'] == 62
    assert report['assemblyAdmitted'] is False and report['poseVerified'] is False
    assert report['geometryQualified'] is False and report['positionalGuidanceAllowed'] is False
    assert report['modelsTablePoseApplied'] is False
    glb = (output / 'active.digital-twin.glb').read_bytes()
    magic, version, length = struct.unpack_from('<4sII', glb)
    assert (magic, version, length) == (b'glTF', 2, len(glb))
    json_length, json_kind = struct.unpack_from('<I4s', glb, 12)
    assert json_kind == b'JSON'
    document = json.loads(glb[20:20 + json_length])
    bin_offset = 20 + json_length
    bin_length, bin_kind = struct.unpack_from('<I4s', glb, bin_offset)
    assert bin_kind == b'BIN\0'
    data = glb[bin_offset + 8:]
    assert len(data) == bin_length == document['buffers'][0]['byteLength']
    mapping = json.loads((output / 'component-map.json').read_text())
    manifest = json.loads((output / 'active.digital-twin.json').read_text())
    digest = hashlib.sha256(glb).hexdigest()
    assert manifest['glbSha256'] == 'sha256:' + digest == 'sha256:' + report['glbSha256']
    assert manifest['physicalGeometryVerified'] is False and manifest['releaseEligible'] is False
    assert manifest['boardRevision'] == mapping['board_revision']
    assert len(document['nodes']) == 35 and len(document['meshes']) == 35
    names = [n['name'] for n in document['nodes']]
    assert len(set(names)) == len(names)
    assert len(mapping['components']) == 34
    omitted_by_ref = {c['reference']:c['degenerate_triangles_omitted']
                      for c in mapping['components'] if c['degenerate_triangles_omitted'] > 0}
    assert len(omitted_by_ref) == 6
    assert sum(omitted_by_ref.values()) == 1292
    assert omitted_by_ref['U2'] == 1277
    expected = {c['sourceIdentity']['uniqueId']: c for c in source['candidateAssociations']['components']}
    assert mapping['sourceEvidence'] == {'format':'altium_pcb','sha256':source['sourceSha256']}
    body_ordinals = []
    total = 0
    for component in mapping['components']:
        original = expected[component['component_id']]
        assert component['reference'] == original['sourceIdentity']['reference']
        claims = component['sourceDesignClaims']
        assert claims['status'] == 'unverified_source_design_claim'
        assert claims['sourceFormat'] == 'altium_pcb'
        assert claims['sourceSha256'] == source['sourceSha256']
        assert claims['sourceIdentity'] == original['sourceIdentity']
        assert claims['properties'] == {key:original['sourceProperties'].get(key)
                                        for key in ('SOURCEDESCRIPTION','PATTERN','SOURCELIBREFERENCE','COMMENT')}
        node = document['nodes'][component['glb_node_index']]
        assert node['name'] == component['glb_selector'] == 'ref:' + component['reference']
        assert node['extras']['componentId'] == component['component_id']
        primitive = document['meshes'][node['mesh']]['primitives'][0]
        position = document['accessors'][primitive['attributes']['POSITION']]
        normal = document['accessors'][primitive['attributes']['NORMAL']]
        color = document['accessors'][primitive['attributes']['COLOR_0']]
        assert position['count'] == normal['count'] == color['count'] > 0
        assert position['count'] == component['triangle_count'] * 3
        total += component['triangle_count']
        for accessor, columns in ((position, 3), (normal, 3), (color, 4)):
            view = document['bufferViews'][accessor['bufferView']]
            values = struct.unpack_from('<' + 'f' * accessor['count'] * columns, data, view['byteOffset'])
            assert all(math.isfinite(v) for v in values)
            if accessor is normal:
                assert all(abs(sum(v * v for v in values[i:i + 3]) - 1) < 2e-6
                           for i in range(0, len(values), 3))
            if accessor is color:
                assert all(0 <= v <= 1 for v in values)
            if accessor is position:
                for axis in range(3):
                    assert min(values[axis::3]) == position['min'][axis]
                    assert max(values[axis::3]) == position['max'][axis]
        assert component['body_count'] == len(original['bodyCandidates'])
        for body in component['sourceBodies']:
            body_ordinals.append(body['bodyOrdinal'])
            candidate = next(b for b in original['bodyCandidates'] if b['bodyOrdinal'] == body['bodyOrdinal'])
            model = next(m for m in candidate['modelCandidates'] if m['modelOrdinal'] == body['modelOrdinal'])
            assert body['stepSha256'] == model['embeddedStepEvidence']['decompressedSha256']
            assert body['sourcePoseInputs'] == candidate['sourcePoseInputs']
            assert body['meshReportSha256'] == hashlib.sha256(
                (meshes / f"{body['modelOrdinal']}.mesh.json").read_bytes()).hexdigest()
    assert sorted(body_ordinals) == list(range(62))
    assert total == report['componentTriangles']
    assert all(math.isfinite(v) for a in document['accessors'] for k in ('min', 'max') for v in a.get(k, []))
    again = subprocess.run([str(binary), 'export-prototype', str(pcb), str(meshes), str(output),
                            '--allow-partial'], capture_output=True, timeout=45)
    assert again.returncode == 2 and 'output_directory_exists' in again.stdout.decode()
    cli_inspection = None
    if cli_binary is not None:
        common = [str(output / 'active.digital-twin.glb'), '--component-map',
                  str(output / 'component-map.json'), '--manifest', str(output / 'active.digital-twin.json')]
        inspect = subprocess.run([str(cli_binary), 'twin', 'inspect', *common], capture_output=True, timeout=60)
        assert inspect.returncode == 0, inspect.stdout.decode(errors='replace')
        cli_inspection = json.loads(inspect.stdout)
        assert cli_inspection['directRenderableSelectors'] == 34
        inspected_by_ref = {c['reference']:c for c in cli_inspection['components']}
        assert len(inspected_by_ref) == 34
        for original in expected.values():
            inspected = inspected_by_ref[original['sourceIdentity']['reference']]
            assert inspected['componentClassClaim'] is None
            assert inspected['componentClassHint']['verified'] is False
            assert inspected['sourceDesignClaims']['sourceSha256'] == source['sourceSha256']
            assert inspected['sourceDesignClaims']['sourceIdentity'] == original['sourceIdentity']
            assert inspected['sourceDesignClaims']['properties']['SOURCEDESCRIPTION'] == original['sourceProperties'].get('SOURCEDESCRIPTION')
        assert inspected_by_ref['C4']['componentClassHint']['value'] == 'capacitor'
        assert inspected_by_ref['C4']['componentClassHint']['basis'] == 'unverified_source_text'
        assert inspected_by_ref['U2']['degenerateTrianglesOmitted'] == 1277
        prototype = output / 'prototype'
        prepare = subprocess.run([str(cli_binary), 'twin', 'prototype', *common, '--output', str(prototype),
                                  '--allow-partial'], capture_output=True, timeout=120)
        assert prepare.returncode == 0, prepare.stdout.decode(errors='replace')
        assert (prototype / 'index.html').stat().st_size > len(glb)
        for reference in ('U1', 'U2', 'J1', 'J2', 'D1', 'R1', 'S1'):
            selected = subprocess.run([str(cli_binary), 'twin', 'select', *common, '--reference', reference],
                                      capture_output=True, timeout=60)
            assert selected.returncode == 0, selected.stdout.decode(errors='replace')
    receipt = dict(schema='tokito.esp8266-prototype-e2e.v1', passed=True,
                   sourceSha256=source['sourceSha256'], glbSha256=digest,
                   components=34, bodies=62, componentTriangles=total,
                   qualification=False, mainCliInspection=cli_inspection,
                   checks=['partial consent', 'no overwrite', 'actual source identities',
                           'source design claims bound to source SHA and component identity',
                           'source descriptions remain unverified and never become classes',
                                                'STEP hash joins', 'all body coverage', 'selectable direct meshes',
                                                'finite positions', 'unit normals', 'source colors', 'exact bounds'])
    (output / 'e2e-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--pcb', type=Path, required=True)
    parser.add_argument('--meshes', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cli-binary', type=Path)
    args = parser.parse_args()
    run(args.binary.resolve(), args.pcb.resolve(), args.meshes.resolve(), args.output.resolve(),
        args.cli_binary.resolve() if args.cli_binary else None)
