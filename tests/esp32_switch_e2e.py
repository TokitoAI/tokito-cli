"""Source-preserving switch derivation and native selectable ESP32 export."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
from glb_e2e import invoke, parse_glb


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(binary, board, library, baseline, bom, output):
    if output.exists():
        raise FileExistsError("refusing to overwrite previous evidence")
    output.mkdir(parents=True)
    intake = json.loads((library / 'intake.json').read_text())
    assert intake['sourceCommit'] == 'd5db5cea003fe2b4b1c9b145f5fcbd5fdb48f8ca'
    for asset in intake['assets']:
        assert digest(library / asset['path']) == asset['sha256']
    prior = json.loads((baseline / 'receipt.json').read_text())
    assert prior['passed'] and prior['selectableBoardModels'] == 19
    baseline_doc, _ = parse_glb(baseline / 'partial.glb')
    assert baseline_doc['asset']['extras']['tokito']['source']['sha256'] == digest(board)
    assert digest(baseline / 'partial.glb') == prior['prototypeGlbSha256']
    module = baseline / 'ESP32-WROOM-32.derived.wrl'
    assert digest(module) == prior['derivedAssetSha256']
    isolated = output / 'library'
    shutil.copytree(library, isolated)
    shutil.copyfile(module, isolated / 'RF_Module.3dshapes/ESP32-WROOM-32.wrl')
    switch_path = 'Button_Switch_SMD.3dshapes/SW_SPST_B3U-1000P.wrl'
    original = library / switch_path
    source_bytes = original.read_bytes()
    derived = output / 'SW.derived.wrl'
    derivation = json.loads(invoke(binary, 'model', 'derive-wrl', original,
        '--output', derived, '--units', 'kicad-wrl', '--remove-unused-invalid-materials').stdout)
    assert derivation['sourceSha256'] == digest(original)
    assert not derivation['geometryModified'] and not derivation['usedMaterialDefinitionsModified']
    removed = derivation['removedPaletteShapes']
    assert len(removed) == 1 and removed[0]['materialDef'] == 'GLASS-19'
    assert removed[0]['useCount'] == 0
    assert source_bytes[:removed[0]['startByte']] + source_bytes[removed[0]['endByteExclusive']:] == derived.read_bytes()
    assert digest(derived) == derivation['derivedSha256']
    shutil.copyfile(derived, isolated / switch_path)
    common = ('export', 'glb', board, '--outline-chaining-tolerance', '0.01',
        '--assemble-wrl', '--project-root', board.parent, '--library-root', isolated,
        '--model-var', f'KICAD9_3DMODEL_DIR={isolated}', '--json-report')
    invoke(binary, *common, '--output', output / 'complete.glb', succeeds=False)
    assert not (output / 'complete.glb').exists()
    glb = output / 'partial.glb'
    report = json.loads(invoke(binary, *common, '--output', glb, '--allow-partial').stdout)
    doc, _ = parse_glb(glb)
    selectors = {n['name']: n for n in doc['nodes'] if n.get('name', '').startswith('ref:')}
    assert len(selectors) == report['coverage']['componentModelsEmitted'] == 21
    expected_triangles = derivation['derivedTriangles']
    components = json.loads((baseline / 'component-map.json').read_text())
    assert components['board_revision'] == digest(board)[:12]
    for ref in ('SW1', 'SW2'):
        row = next(c for c in components['components'] if c['reference'] == ref)
        node = selectors['ref:' + ref]
        assert node['extras']['tokito']['componentUuid'] == row['component_id']
        primitives = doc['meshes'][node['mesh']]['primitives']
        actual = sum(doc['accessors'][p['indices']]['count'] // 3 if 'indices' in p
                     else doc['accessors'][p['attributes']['POSITION']]['count'] // 3 for p in primitives)
        assert actual == expected_triangles
        row['glb_selector'] = 'ref:' + ref
    assert {m['componentReference'] for m in report['coverage']['unresolvedModels']} == {'J1', 'D2', 'U3'}
    assert not doc['asset']['extras']['tokito']['wholeBoardDigitalTwinComplete']
    components['renderable_count'] = 21
    revision = 'derived-unused-palette-module-switch-d5db5cea'
    components['assemblyContextRevision'] = revision
    manifest = json.loads((baseline / 'manifest.json').read_text())
    manifest.update(assemblyContextRevision=revision, glbBytes=glb.stat().st_size, glbSha256=digest(glb))
    positions = [a for a in doc['accessors'] if a.get('type') == 'VEC3' and 'min' in a and 'max' in a]
    manifest['boundsMetres'] = {
        'minimum': dict(zip('xyz', [min(a['min'][i] for a in positions) for i in range(3)])),
        'maximum': dict(zip('xyz', [max(a['max'][i] for a in positions) for i in range(3)]))}
    map_path, manifest_path = output / 'component-map.json', output / 'manifest.json'
    map_path.write_text(json.dumps(components, indent=2))
    manifest_path.write_text(json.dumps(manifest, indent=2))
    invoke(binary, 'twin', 'prototype', glb, '--component-map', map_path,
        '--manifest', manifest_path, '--output', output / 'prototype', '--allow-partial', '--reference-bom', bom)
    for ref in ('SW1', 'SW2'):
        selection = json.loads(invoke(binary, 'twin', 'select', output / 'prototype/active.digital-twin.glb',
            '--component-map', output / 'prototype/component-map.json',
            '--manifest', output / 'prototype/active.digital-twin.json', '--reference', ref).stdout)
        assert selection['status'] == 'unique_candidate' and selection['matches'][0]['modelAvailable']
    assert original.read_bytes() == source_bytes
    (output / 'derivation-report.json').write_text(json.dumps(derivation, indent=2))
    (output / 'export-report.json').write_text(json.dumps(report, indent=2))
    receipt = {'passed': True, 'sourceBoardSha256': digest(board), 'sourceLibraryCommit': intake['sourceCommit'],
        'sourceSwitchSha256': digest(original), 'derivedSwitchSha256': digest(derived),
        'componentModelsEmitted': 21, 'switchTrianglesEach': expected_triangles,
        'remainingModelReferences': ['J1', 'D2', 'U3'], 'fullAssemblyRefusalPreserved': True,
        'sourceGeometryUnchanged': True, 'physicalFitVerified': False, 'photographedBoardMatchVerified': False,
        'glbSha256': digest(glb)}
    (output / 'receipt.json').write_text(json.dumps(receipt, indent=2))
    print('PASS native ESP32 switches: 21 selectable bodies; J1/D2/U3 still refused')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    for name in ('binary', 'board', 'library', 'baseline', 'bom', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    run(*(getattr(args, name).resolve() for name in ('binary', 'board', 'library', 'baseline', 'bom', 'output')))
