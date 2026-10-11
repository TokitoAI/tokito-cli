"""Actual Ultimaker source-candidate adapter/prototype journey, pinned dataset profile."""
import argparse
import hashlib
import json
from pathlib import Path
from glb_e2e import invoke, parse_glb

def run(binary, source, output):
    output.mkdir(parents=True, exist_ok=False)
    common = (source/'ultimaker-source-selectable.glb', '--component-map', source/'component-map.json',
              '--geometry-index', source/'component-geometry-index.json', '--provenance', source/'provenance.json',
              '--coverage', source/'geometry-coverage.json', '--board-id', 'ultimainboard', '--board-revision', '2.1.4')
    adapted = output/'adapted'
    report = json.loads(invoke(binary, 'twin', 'adapt-source', *common, '--output', adapted).stdout)
    assert report['componentCount'] == 574 and report['renderableCount'] == 310
    assert set(report['missingPackageBodies']) == {'J34','U9','K1','L2'}
    assert len(report['unknownMechanicalSpacers']) == 4
    assert not report['releaseEligible'] and not report['physicalGeometryVerified'] and not report['coverageOriginAuthenticated']
    original_doc, original_bin = parse_glb(common[0])
    assert (adapted/'active.digital-twin.glb').read_bytes() == common[0].read_bytes()
    manifest = json.loads((adapted/'active.digital-twin.json').read_text())
    # Independent NumPy reader's active-world vertex extrema, source metre claim.
    expected = [[-.0995835014977193,-.002721398043632508,-.039500001817941666],
                [.08749999850988388,.01669999966472388,.039500001817941666]]
    for side, values in zip(('minimum','maximum'), expected):
        for axis, value in zip(('x','y','z'), values):
            assert abs(manifest['boundsMetres'][side][axis]-value) < 1e-9
    # Authored substrate node in this pinned source; not automatic largest-mesh guessing.
    plane_output = output/'substrate-bound'
    plane_report = json.loads(invoke(binary, 'twin', 'adapt-source', *common,
                                    '--output', plane_output, '--board-plane-node', '=>[0:1:1:133]').stdout)
    plane_manifest = json.loads((plane_output/'active.digital-twin.json').read_text())
    assert plane_report['boardPlaneBoundsDecoded'] and not plane_report['physicalGeometryVerified']
    assert plane_manifest['boardPlaneSourceNode'] == '=>[0:1:1:133]'
    assert abs(plane_manifest['boardPlaneBoundsMetres']['minimum']['x'] + .08749999850988388) < 1e-9
    assert plane_manifest['boardPlaneBoundsMetres']['minimum']['x'] > manifest['boundsMetres']['minimum']['x']
    assert plane_manifest['boundsMetres'] == manifest['boundsMetres']
    bad_plane = output/'missing-plane'
    invoke(binary, 'twin', 'adapt-source', *common, '--output', bad_plane,
           '--board-plane-node', 'not-present', succeeds=False)
    assert not bad_plane.exists()
    twin = (adapted/'active.digital-twin.glb','--component-map',adapted/'component-map.json',
            '--manifest',adapted/'active.digital-twin.json')
    inspected = json.loads(invoke(binary,'twin','inspect',*twin).stdout)
    assert inspected['renderableCount'] == 310 and inspected['missingRequiredModelCount'] == 8
    assert inspected['multiMeshComponentCount'] > 0 and not inspected['appNameFilterCompatible']
    multi = next(c for c in inspected['components'] if len(c['renderableNodeIndices']) > 1)
    result = json.loads(invoke(binary,'twin','select',*twin,'--reference',multi['reference']).stdout)
    assert result['status'] == 'unique_candidate'
    prototype = output/'prototype'
    invoke(binary,'twin','prototype',*twin,'--output',prototype,succeeds=False)
    assert not prototype.exists()
    invoke(binary,'twin','prototype',*twin,'--output',prototype,'--allow-partial')
    doc, payload = parse_glb(prototype/'active.digital-twin.glb')
    assert payload == original_bin
    assert len(doc['nodes']) == len(original_doc['nodes'])
    for a,b in zip(original_doc['nodes'],doc['nodes']):
        assert {k:v for k,v in a.items() if k!='name'} == {k:v for k,v in b.items() if k!='name'}
    assert doc['meshes'] == original_doc['meshes'] and doc['materials'] == original_doc['materials']
    html = (prototype/'index.html').read_text()
    assert 'https://' not in html and '<script src=' not in html
    saved = (prototype/'index.html').read_bytes()
    invoke(binary,'twin','prototype',*twin,'--output',prototype,'--allow-partial',succeeds=False)
    assert (prototype/'index.html').read_bytes() == saved
    # An edited coverage classification must fail when the declared summary is unchanged.
    coverage = json.loads((source/'geometry-coverage.json').read_text())
    coverage['references'][0]['category'] = 'invented_category'
    bad = output/'bad-coverage.json';bad.write_text(json.dumps(coverage))
    changed = list(common);changed[8] = bad
    invoke(binary,'twin','adapt-source',*changed,'--output',output/'bad-candidate',succeeds=False)
    assert not (output/'bad-candidate').exists()
    receipt = {'schema':'tokito.source-candidate-e2e.v1','passed':True,'componentCount':574,'selectableComponents':310,
               'multiMeshReference':multi['reference'],'missingRequiredBodies':8,'boundsIndependentlyCompared':True,
               'geometryPayloadAndTransformsUnchanged':True,'physicalGeometryVerified':False,
               'sourceSha256':hashlib.sha256(common[0].read_bytes()).hexdigest()}
    (output/'e2e-receipt.json').write_text(json.dumps(receipt,indent=2))
    print('PASS Ultimaker source candidate: 310 groups, exact geometry/transforms, 8 explicit missing/unknown bodies')

if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True)
    p.add_argument('--source-root',type=Path,required=True);p.add_argument('--output-dir',type=Path,required=True)
    a=p.parse_args();run(a.binary.resolve(strict=True),a.source_root.resolve(strict=True),a.output_dir.resolve())
