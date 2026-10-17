"""Mandatory subprocess journeys for previously optional-only commands.

Authored minimal geometry is an integration fixture, not a qualified CAD model.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import tempfile

from glb_e2e import invoke, parse_glb
from twin_e2e import encode


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--fault-hooks', action='store_true')
    options = parser.parse_args()
    binary = options.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='tokito-command-completion-') as directory:
        root = Path(directory)
        source = root/'source.wrl'
        source.write_text('''#VRML V2.0 utf8
Shape { geometry IndexedFaceSet {
coord Coordinate { point [0 0 0, 1 0 0, 0 1 0, 2 0 0] }
coordIndex [0,1,2,-1, 0,1,3,-1] } }
''', encoding='utf-8')
        original = source.read_bytes()
        derived = root/'derived.wrl'
        flags = ('--units', 'millimetres', '--remove-zero-area-faces')
        report = json.loads(invoke(binary, 'model', 'derive-wrl', source, '--output', derived, *flags).stdout)
        assert report['derivedSha256'] == hashlib.sha256(derived.read_bytes()).hexdigest()
        assert len(report['removedZeroAreaFaces']) == 1 and report['derivedTriangles'] == 1
        assert source.read_bytes() == original
        invoke(binary, 'model', 'inspect', derived, '--units', 'millimetres')
        saved = derived.read_bytes()
        invoke(binary, 'model', 'derive-wrl', source, '--output', derived, *flags, succeeds=False)
        assert derived.read_bytes() == saved

        payload = struct.pack('<9f', 0, 0, 0, .01, 0, 0, 0, .02, .03)
        document = {'asset': {'version':'2.0'}, 'buffers':[{'byteLength':len(payload)}],
                    'bufferViews':[{'buffer':0,'byteLength':len(payload)}],
                    'accessors':[{'bufferView':0,'componentType':5126,'type':'VEC3','count':3,
                                  'min':[0,0,0],'max':[.01,.02,.03]}],
                    'meshes':[{'primitives':[{'attributes':{'POSITION':0}}]}],
                    'nodes':[{'name':'ref:Q7','mesh':0,'translation':[.1,.2,.3]}],
                    'scenes':[{'nodes':[0]}], 'scene':0}
        glb = root/'source.glb'; glb.write_bytes(encode(document, payload))
        mapping = root/'map.json'
        mapping.write_text(json.dumps({'components':[{'component_id':'fixture:r1:Q7','reference':'Q7',
                              'glb_selector':'ref:Q7','needs_3d_body':True}]}))
        digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
        index = root/'index.json'
        index.write_text(json.dumps({'release_eligible':False,'source_geometry_only':True,
            'photo_correspondence_verified':False,'output_sha256':digest(glb),
            'selectable_source_references':1,'references_without_selectable_geometry':['X1'],
            'components':{'Q7':{'glb_selector':'ref:Q7','node_indices':[0],'mesh_indices':[0]}}}))
        coverage = root/'coverage.json'
        category = 'test_contact; needs_source_pad_region_not_package_body'
        coverage.write_text(json.dumps({'schema_version':'1.0','production_eligible':False,
            'source_index_sha256':digest(index),'independently_selectable_source_bodies':1,
            'unmodeled_reference_classification':{category:1},'references':[{'reference':'X1','category':category}]}))
        provenance = root/'provenance.json'
        provenance.write_text(json.dumps({'schema_version':'1.0','release_eligible':False,
            'photo_registration_verified':False,'physical_flip_verified':False,
            'files':[{'path':p.name,'sha256':digest(p),'bytes':p.stat().st_size} for p in [glb,mapping,index]]}))
        output = root/'adapted'
        args = ('twin','adapt-source',glb,'--component-map',mapping,'--geometry-index',index,
                '--provenance',provenance,'--coverage',coverage,'--board-id','fixture',
                '--board-revision','r1','--output',output)
        adapted = json.loads(invoke(binary,*args).stdout)
        assert adapted['renderableCount'] == 1 and not adapted['physicalGeometryVerified']
        assert (output/'active.digital-twin.glb').read_bytes() == glb.read_bytes()
        actual, actual_payload = parse_glb(output/'active.digital-twin.glb')
        assert actual == document and actual_payload == payload
        invoke(binary,*args,succeeds=False)
        if options.fault_hooks:
            input_hashes = {p: digest(p) for p in [glb,mapping,index,coverage,provenance]}
            def fail_publication(arguments, destination, count):
                previous = os.environ.get('TOKITO_TWIN_TEST_FAIL_AFTER')
                os.environ['TOKITO_TWIN_TEST_FAIL_AFTER'] = str(count)
                try:
                    result = invoke(binary,*arguments,succeeds=False)
                    assert result.returncode == 2 and 'injected twin publication failure' in result.stderr
                finally:
                    if previous is None: os.environ.pop('TOKITO_TWIN_TEST_FAIL_AFTER',None)
                    else: os.environ['TOKITO_TWIN_TEST_FAIL_AFTER'] = previous
                assert not destination.exists()
            for count in [1, 2, 3]:
                destination = root/f'adapt-failure-{count}'
                fail_publication((*args[:-1],destination),destination,count)
            assert all(digest(p) == value for p,value in input_hashes.items())
        common = (output/'active.digital-twin.glb','--component-map',output/'component-map.json',
                  '--manifest',output/'active.digital-twin.json')
        selected = json.loads(invoke(binary,'twin','select',*common,'--reference','Q7').stdout)
        assert selected['status'] == 'unique_candidate'
        for command in ['prepare','prototype']:
            destination = root/command
            invoke(binary,'twin',command,*common,'--output',destination)
            assert (destination/'active.digital-twin.json').exists()
            if command == 'prototype': assert (destination/'index.html').exists()
            if options.fault_hooks:
                for count in [1, 2, 4 if command == 'prepare' else 5]:
                    destination = root/f'{command}-failure-{count}'
                    fail_publication(('twin',command,*common,'--output',destination),destination,count)
        bom = root/'reference.csv'
        bom.write_text('footprint_index,value,footprint,reference\n0,transistor,Test:Q,Q7\n1,10k,Test:R,R1\n')
        audit = json.loads(invoke(binary,'twin','audit-population',*common,'--reference-bom',bom).stdout)
        assert audit['referenceBOMComponents'] == 2
        assert audit['referenceLabelsAbsentFromCandidate'] == ['R1']
        assert not audit['completePopulationVerified'] and not audit['samePhysicalBoardVerified']
        bad = root/'bad.csv'; bad.write_text('unsupported\n')
        invoke(binary,'twin','audit-population',*common,'--reference-bom',bad,succeeds=False)
    print('PASS base command completion: explicit WRL derivation, source adaptation, exact selection, BOM population audit and refusals; synthetic geometry remains unqualified')


if __name__ == '__main__':
    main()
