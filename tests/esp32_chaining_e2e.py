"""Real ESP32 clone chaining export, preserving component refusal diagnostics."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import shutil
from glb_e2e import invoke, parse_glb

def run(binary, board, library, output, profile, reference_bom=None):
    output.mkdir(parents=True, exist_ok=False)
    digest = hashlib.sha256(board.read_bytes()).hexdigest()
    invoke(binary, 'export', 'glb', board, '--output', output/'strict.glb', '--allow-partial', succeeds=False)
    assert not (output/'strict.glb').exists()
    common = ('export', 'glb', board, '--outline-chaining-tolerance', '0.01',
              '--assemble-wrl', '--project-root', board.parent, '--library-root', library,
              '--model-var', f'KICAD9_3DMODEL_DIR={library}', '--json-report')
    invoke(binary, *common, '--output', output/'complete.glb', succeeds=False)
    assert not (output/'complete.glb').exists()
    glb = output/'partial.glb'
    report = json.loads(invoke(binary, *common, '--output', glb, '--allow-partial').stdout)
    document, payload = parse_glb(glb)
    meta = document['asset']['extras']['tokito']
    assert meta['source']['sha256'] == digest
    assert meta['tessellation']['outlineJoinToleranceMm'] == 0.01
    assert meta['tessellation']['maxJoinDisplacementMm'] == 0.005
    assert not meta['tessellation']['sourceGeometryEdited']
    assert report['mesh']['outlineChainingToleranceMm'] == 0.01
    statuses = Counter(m['status'] for m in report['coverage']['unresolvedModels'])
    emitted = report['coverage']['componentModelsEmitted']
    if profile == 'installed-step-only':
        assert emitted == 0 and statuses == {'missing':23,'outside_approved_roots':1}
    else:
        intake = json.loads((library/'intake.json').read_text())
        assert intake['sourceCommit'] == 'd5db5cea003fe2b4b1c9b145f5fcbd5fdb48f8ca'
        for asset in intake['assets']:
            assert asset['downloaded'] and hashlib.sha256((library/asset['path']).read_bytes()).hexdigest() == asset['sha256']
        assert emitted == 18 and statuses == {'mesh_rejected':4,'glb_precision_rejected':1,'outside_approved_roots':1}
        selectors = {n['name']: n for n in document['nodes'] if n.get('name','').startswith('ref:')}
        assert len(selectors) == 18
        for ref in [f'R{i}' for i in range(1,7)]+[f'C{i}' for i in range(1,7)]:
            assert f'ref:{ref}' in selectors
        # Independent source-colour comparison, separate from Rust's parser.
        import re
        source_rgb = {tuple(float(v) for v in rgb.split()) for rgb in re.findall(r'diffuseColor\s+([0-9.eE+\- ]+)',
            (library/'Resistor_SMD.3dshapes/R_0805_2012Metric.wrl').read_text())}
        resistor_mesh=document['meshes'][selectors['ref:R1']['mesh']]
        exported_rgb={tuple(document['materials'][p['material']]['pbrMetallicRoughness']['baseColorFactor'][:3]) for p in resistor_mesh['primitives']}
        assert exported_rgb == source_rgb and len(exported_rgb)==3
        assert meta['shellRepresentation'] == 'nominal_thickness_envelope' and not meta['physicalLayerGeometryComplete']
        if profile == 'kicad9-wrl':
            # Derive only the unreferenced invalid palette material; run the
            # ordinary Rust board exporter against an isolated copy of the
            # pinned library, preserving all other models and source files.
            central=output/'derived-module'
            central.mkdir()
            intake=json.loads((library/'intake.json').read_text())
            module_asset=next(x for x in intake['assets'] if x['path']=='RF_Module.3dshapes/ESP32-WROOM-32.wrl')
            original_module=library/module_asset['path']
            original_bytes=original_module.read_bytes()
            original_hash=hashlib.sha256(original_bytes).hexdigest()
            assert original_hash==module_asset['sha256']
            derived_module=central/'ESP32-WROOM-32.derived.wrl'
            derivation=json.loads(invoke(binary,'model','derive-wrl',original_module,
                '--output',derived_module,'--units','kicad-wrl',
                '--remove-unused-invalid-materials').stdout)
            assert derivation['sourceSha256']==original_hash
            assert [x['materialDef'] for x in derivation['removedPaletteShapes']]==['GLASS-19']
            assert derivation['derivedMeshParts']==751 and derivation['derivedTriangles']==7652
            # Reconstruct output solely from the reported deleted byte span.
            start,end=derivation['removedPaletteShapes'][0]['startByte'],derivation['removedPaletteShapes'][0]['endByteExclusive']
            assert original_bytes[:start]+original_bytes[end:]==derived_module.read_bytes()
            copied_library=central/'library'
            shutil.copytree(library,copied_library)
            (copied_library/module_asset['path']).write_bytes(derived_module.read_bytes())
            assert original_module.read_bytes()==original_bytes
            central_glb=central/'partial.glb'
            central_common=('export','glb',board,'--outline-chaining-tolerance','0.01',
                '--assemble-wrl','--project-root',board.parent,'--library-root',copied_library,
                '--model-var',f'KICAD9_3DMODEL_DIR={copied_library}','--json-report')
            central_report=json.loads(invoke(binary,*central_common,'--output',central_glb,'--allow-partial').stdout)
            central_doc,central_payload=parse_glb(central_glb)
            central_statuses=Counter(m['status'] for m in central_report['coverage']['unresolvedModels'])
            assert central_report['coverage']['componentModelsEmitted']==19
            assert central_statuses=={'mesh_rejected':3,'glb_precision_rejected':1,'outside_approved_roots':1}
            central_selectors={n['name']:n for n in central_doc['nodes'] if n.get('name','').startswith('ref:')}
            assert len(central_selectors)==19 and 'ref:U1' in central_selectors
            inventory=json.loads(invoke(binary,'inspect',board,'--json').stdout)
            u1=next(fp for fp in inventory['footprints'] if fp['reference']=='U1')
            module_node=central_selectors['ref:U1']
            assert module_node['extras']['tokito']['componentUuid']==u1['uuid']
            module_mesh=central_doc['meshes'][module_node['mesh']]
            module_triangles=sum(central_doc['accessors'][primitive['indices']]['count']//3
                if 'indices' in primitive else central_doc['accessors'][primitive['attributes']['POSITION']]['count']//3
                for primitive in module_mesh['primitives'])
            assert module_triangles==derivation['derivedTriangles']
            position_accessors=[central_doc['accessors'][primitive['attributes']['POSITION']]
                for primitive in module_mesh['primitives']]
            module_bounds_mm={
                'min':[min(accessor['min'][axis] for accessor in position_accessors)*1000 for axis in range(3)],
                'max':[max(accessor['max'][axis] for accessor in position_accessors)*1000 for axis in range(3)]}
            module_dimensions=sorted(module_bounds_mm['max'][axis]-module_bounds_mm['min'][axis] for axis in range(3))
            assert all(abs(actual-expected)<0.08 for actual,expected in zip(module_dimensions,[3.1,18.0,25.5])), module_dimensions
            (central/'derivation-report.json').write_text(json.dumps(derivation,indent=2))
            (central/'export-report.json').write_text(json.dumps(central_report,indent=2))
            (central/'receipt.json').write_text(json.dumps({'passed':True,'originalAssetSha256':original_hash,
                'derivedAssetSha256':derivation['derivedSha256'],'removedMaterial':['GLASS-19'],
                'modelParts':derivation['derivedMeshParts'],'modelTriangles':module_triangles,
                'selectableBoardModels':19,'componentUuid':u1['uuid'],'boardFootprintRotationDegrees':u1['rotation_deg'],
                'placedModelBoundsMm':module_bounds_mm,'placedDimensionsMm':module_dimensions,
                'physicalFitVerified':False,'photographedBoardMatchVerified':False},indent=2))
        if reference_bom is not None:
            inventory = json.loads(invoke(binary,'inspect',board,'--json').stdout)
            rows=[]
            for fp in inventory['footprints']:
                ref=fp['reference']
                cls='resistor' if ref.startswith('R') and not fp['reference_is_placeholder'] else 'capacitor' if ref.startswith('C') else 'unclassified_reference'
                cls={'U1':'rf_module','U2':'voltage_regulator','U3':'interface_ic','D1':'diode','D2':'led','J1':'connector','J2':'pin_header','J3':'pin_header','Q1':'transistor','Q2':'transistor','SW1':'switch','SW2':'switch'}.get(ref,cls)
                node=selectors.get('ref:'+ref)
                if node:
                    assert node['extras']['tokito']['componentUuid'] == fp['uuid']
                rows.append({'reference':ref,'component_id':fp['uuid'],'component_class':cls,
                    'needs_3d_body':fp['model_count']>0,'glb_selector':'ref:'+ref if node else None,
                    'model_quality':'source_library_candidate_physical_fit_unverified','identity_verified':False,
                    'source_board_position_mm':{'x':fp['x_mm'],'y':fp['y_mm']},'source_rotation_degrees':fp['rotation_deg']})
            identity='esp32-community-layout-reference'
            component_map={'schema_version':'1.0','asset_id':identity,'board_id':identity,'board_revision':digest[:12],
                'component_count':26,'renderable_count':18,'component_node_prefix':'ref:','components':rows,
                'physicalGeometryVerified':False,'release_eligible':False,'calibration_eligible':False}
            assert all(not set(n)&{'matrix','translation','rotation','scale'} for n in document['nodes'])
            bounds=[a for a in document['accessors'] if a.get('type')=='VEC3' and 'min' in a and 'max' in a]
            manifest={'schemaVersion':'1.0','assetId':identity,'boardIdentity':identity,'boardRevision':digest[:12],
                'assemblyContextRevision':'native-reference-library-d5db5cea','units':'METRE','axes':'RIGHT_HANDED_Y_UP',
                'componentNodePrefix':'ref:','glbBytes':glb.stat().st_size,'glbSha256':hashlib.sha256(glb.read_bytes()).hexdigest(),
                'boardOriginMetres':dict.fromkeys('xyz',0),'boundsMetres':{'minimum':dict(zip('xyz',[min(a['min'][i] for a in bounds) for i in range(3)])),
                'maximum':dict(zip('xyz',[max(a['max'][i] for a in bounds) for i in range(3)]))},'defaultOpacity':.42,
                'physicalGeometryVerified':False,'arRegistrationVerified':False,'releaseEligible':False}
            mp=output/'component-map.json';mp.write_text(json.dumps(component_map,indent=2))
            mf=output/'manifest.json';mf.write_text(json.dumps(manifest,indent=2))
            invoke(binary,'twin','prototype',glb,'--component-map',mp,'--manifest',mf,'--output',output/'prototype',
                '--allow-partial','--reference-bom',reference_bom)
            audit=json.loads((output/'prototype/reference-population-audit.json').read_text())
            counts={r['classClaim']:r for r in audit['classCoverageComparison']}
            assert counts['resistor']['renderedCandidateCount']==counts['capacitor']['renderedCandidateCount']==6
            assert not audit['sameLabelClassConflicts'] and not audit['completePopulationVerified']
            if profile == 'kicad9-wrl':
                central_rows=[dict(row) for row in rows]
                u1_row=next(row for row in central_rows if row['reference']=='U1')
                u1_row['glb_selector']='ref:U1'
                central_map={**component_map,'renderable_count':19,
                    'components':central_rows,'assemblyContextRevision':'derived-unused-material-GLASS-19-d5db5cea'}
                central_bounds=[a for a in central_doc['accessors'] if a.get('type')=='VEC3' and 'min' in a and 'max' in a]
                central_manifest={**manifest,
                    'assemblyContextRevision':'derived-unused-material-GLASS-19-d5db5cea',
                    'glbBytes':central_glb.stat().st_size,
                    'glbSha256':hashlib.sha256(central_glb.read_bytes()).hexdigest(),
                    'boundsMetres':{'minimum':dict(zip('xyz',[min(a['min'][i] for a in central_bounds) for i in range(3)])),
                    'maximum':dict(zip('xyz',[max(a['max'][i] for a in central_bounds) for i in range(3)]))}}
                central_map_path=central/'component-map.json'
                central_manifest_path=central/'manifest.json'
                central_map_path.write_text(json.dumps(central_map,indent=2))
                central_manifest_path.write_text(json.dumps(central_manifest,indent=2))
                invoke(binary,'twin','prototype',central_glb,'--component-map',central_map_path,
                    '--manifest',central_manifest_path,'--output',central/'prototype',
                    '--allow-partial','--reference-bom',reference_bom)
                selected=json.loads(invoke(binary,'twin','select',central/'prototype/active.digital-twin.glb',
                    '--component-map',central/'prototype/component-map.json',
                    '--manifest',central/'prototype/active.digital-twin.json','--reference','U1').stdout)
                assert selected['status']=='unique_candidate'
                assert selected['matches'][0]['modelAvailable'] is True
                assert selected['matches'][0]['selector']=='ref:U1'
                (central/'receipt.json').write_text(json.dumps({**json.loads((central/'receipt.json').read_text()),
                    'offlinePrototypeGenerated':True,'u1CliSelectorCheckPassed':True,
                    'prototypeGlbSha256':central_manifest['glbSha256']},indent=2))
    assert not meta['wholeBoardDigitalTwinComplete'] and meta['partialExport']
    assert hashlib.sha256(board.read_bytes()).hexdigest() == digest
    receipt = {'passed':True,'sourceSha256':digest,'sourceUnchanged':True,
               'strictRefusalPreserved':True,'fullAssemblyRefusalPreserved':True,
               'explicitChainingToleranceMm':0.01,'maxJoinDisplacementMm':0.005,
               'modelRefusalStatuses':dict(statuses),'componentModelsEmitted':emitted,'libraryProfile':profile,
               'fullBoardQualified':False,'glbSha256':hashlib.sha256(glb.read_bytes()).hexdigest(),
               'mesh':report['mesh']}
    (output/'receipt.json').write_text(json.dumps(receipt,indent=2))
    (output/'export-report.json').write_text(json.dumps(report,indent=2))
    print(f'PASS native ESP32: {emitted} selectable bodies; strict/full-assembly refusal preserved; unresolved {dict(statuses)}')

if __name__=='__main__':
    p=argparse.ArgumentParser()
    for name in ['binary','board','library','output']:p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--library-profile',choices=['installed-step-only','kicad9-wrl'],default='installed-step-only')
    p.add_argument('--reference-bom',type=Path)
    a=p.parse_args();run(a.binary.resolve(strict=True),a.board.resolve(strict=True),a.library.resolve(strict=True),a.output.resolve(),a.library_profile,a.reference_bom)
