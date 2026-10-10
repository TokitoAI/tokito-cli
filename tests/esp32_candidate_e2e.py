"""Pinned ESP32 inspection candidate: no physical calibration or CAD quality claim."""
import argparse
import hashlib
import json
from pathlib import Path
from glb_e2e import invoke, parse_glb

def run(binary, source, output, reference_bom=None):
    output.mkdir(parents=True, exist_ok=False)
    glb = source / 'esp32-package-candidate.glb'
    document, payload = parse_glb(glb)
    provenance = json.loads((source / 'provenance.json').read_text())
    original_map = (source / 'input/component-map.json').read_bytes()
    assert hashlib.sha256(glb.read_bytes()).hexdigest() == provenance['output_sha256']
    assert hashlib.sha256(original_map).hexdigest() == provenance['source_component_map_sha256']
    assert not provenance['release_eligible'] and not provenance['calibration_eligible']
    component_map = json.loads(original_map)
    selectors = {n['name']: n for n in document['nodes'] if n.get('name', '').startswith('ref:')}
    assert len(selectors) == 15 and len(component_map['components']) == 19
    # Legacy map claims exact geometry/measurement. Preserve its source bytes,
    # but explicitly downgrade those claims in the inspection handoff.
    for c in component_map['components']:
        c['source_claims_unverified'] = {'model_quality': c.get('model_quality'), 'provenance': c.get('provenance')}
        node = selectors.get(c.get('glb_selector'))
        c['model_quality'] = node['extras']['geometry_status'] if node else 'nominal_mounting_hole_not_calibration_anchor'
        c['provenance'] = 'Dataset PR16 candidate; dimensions, identity and physical fit unverified'
        c['identity_verified'] = False
    component_map['release_eligible'] = False
    component_map['calibration_eligible'] = False
    map_path = output / 'component-map.json'
    map_path.write_text(json.dumps(component_map, indent=2))
    # This pinned generator stores world-space metres directly in POSITION.
    # Reject transforms before using actual decoded vertex extrema.
    assert all(not set(n) & {'matrix', 'translation', 'rotation', 'scale', 'children'} for n in document['nodes'])
    vertices = []
    import struct
    for mesh in document['meshes']:
        for primitive in mesh['primitives']:
            accessor = document['accessors'][primitive['attributes']['POSITION']]
            assert accessor['componentType'] == 5126 and accessor['type'] == 'VEC3' and 'sparse' not in accessor
            view = document['bufferViews'][accessor['bufferView']]
            assert view['buffer'] == 0
            offset = view.get('byteOffset', 0) + accessor.get('byteOffset', 0)
            stride = view.get('byteStride', 12)
            vertices.extend(struct.unpack_from('<fff', payload, offset + i * stride) for i in range(accessor['count']))
    bounds = {'minimum': dict(zip('xyz', [min(v[i] for v in vertices) for i in range(3)])),
              'maximum': dict(zip('xyz', [max(v[i] for v in vertices) for i in range(3)]))}
    manifest = {'schemaVersion': '1.0', 'assetId': component_map['asset_id'],
                'boardIdentity': component_map['board_id'], 'boardRevision': component_map['board_revision'],
                'assemblyContextRevision': 'dataset-pr16-07c4519-provisional-inspection',
                'units': 'METRE', 'axes': 'RIGHT_HANDED_Y_UP', 'componentNodePrefix': 'ref:',
                'glbBytes': glb.stat().st_size, 'glbSha256': hashlib.sha256(glb.read_bytes()).hexdigest(),
                'boardOriginMetres': dict.fromkeys('xyz', 0), 'boundsMetres': bounds, 'defaultOpacity': 0.42,
                'physicalGeometryVerified': False, 'arRegistrationVerified': False, 'releaseEligible': False,
                'limitations': provenance['limitations']}
    manifest_path = output / 'active.digital-twin.json'
    manifest_path.write_text(json.dumps(manifest, indent=2))
    common = (glb, '--component-map', map_path, '--manifest', manifest_path)
    inspected = json.loads(invoke(binary, 'twin', 'inspect', *common).stdout)
    assert inspected['renderableCount'] == 15
    for c in component_map['components']:
        result = json.loads(invoke(binary, 'twin', 'select', *common, '--reference', c['reference']).stdout)
        if c['glb_selector']:
            assert result['status'] == 'unique_candidate', (c['reference'], result)
    prototype = output / 'prototype'
    extra = ('--reference-bom', reference_bom) if reference_bom is not None else ()
    invoke(binary, 'twin', 'prototype', *common, '--output', prototype, '--allow-partial', *extra)
    prepared, prepared_payload = parse_glb(prototype / 'active.digital-twin.glb')
    assert prepared_payload == payload and prepared['meshes'] == document['meshes']
    assert prepared['materials'] == document['materials']
    assert prepared['nodes'] == document['nodes']
    html = (prototype / 'index.html').read_text()
    assert 'https://' not in html and '<script src=' not in html
    if reference_bom is not None:
        audit = json.loads(invoke(binary, 'twin', 'audit-population', *common, '--reference-bom', reference_bom).stdout)
        counts = {r['classClaim']: r for r in audit['classCoverageComparison']}
        assert audit['referenceBOMComponents'] == 24
        assert counts['resistor']['referenceBOMCount'] == 6 and counts['resistor']['renderedCandidateCount'] == 0
        assert counts['capacitor']['referenceBOMCount'] == 6 and counts['capacitor']['renderedCandidateCount'] == 2
        assert not audit['completePopulationVerified'] and not audit['samePhysicalBoardVerified']
        assert {r['reference'] for r in audit['sameLabelClassConflicts']} == {'U1', 'U2'}
        (output / 'reference-population-audit.json').write_text(json.dumps(audit, indent=2))
        bundled = json.loads((prototype / 'reference-population-audit.json').read_text())
        index = json.loads((prototype / 'component-index.json').read_text())
        assert bundled == index['referencePopulationComparison']
        assert bundled['candidateSourceHashes'] == index['sourceSha256']
        assert bundled['classCoverageComparison'] == audit['classCoverageComparison']
        assert not bundled['completePopulationVerified']
        invoke(binary, 'twin', 'prototype', *common, '--output', output/'duplicate-options',
               '--reference-bom', reference_bom, '--reference-bom', reference_bom, succeeds=False)
        assert not (output/'duplicate-options').exists()
    receipt = {'passed': True, 'sourceCommit': '07c4519c964520e1a143b5f3c059b18dd108f037',
               'selectableComponents': 15, 'componentRecords': 19, 'geometryAndMaterialsPreserved': True,
               'glbSha256': provenance['output_sha256'], 'physicalFitVerified': False,
               'browserAcceptance': 'pending', 'boundsMetres': bounds}
    (output / 'e2e-receipt.json').write_text(json.dumps(receipt, indent=2))
    print('PASS ESP32 actual CLI: 15 selectable components; geometry preserved; physical fit unverified')

if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--source', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--reference-bom', type=Path)
    a = p.parse_args()
    run(a.binary.resolve(strict=True), a.source.resolve(strict=True), a.output.resolve(), a.reference_bom)
