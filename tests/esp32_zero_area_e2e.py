"""Independent exact-decimal comparison and actual USB/LED native board export."""
import argparse
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import re
import shutil
from glb_e2e import invoke, parse_glb


def sha(data):
    return hashlib.sha256(data).hexdigest()


def remove_spans(data, spans):
    result, previous = bytearray(), 0
    for start, end in spans:
        assert previous <= start < end <= len(data)
        result.extend(data[previous:start])
        previous = end
    result.extend(data[previous:])
    return bytes(result)


def exact_faces(data):
    # Independent source-specific flat-root scanner, not a general VRML parser.
    text = data.decode('utf-8')
    shapes = re.split(r'\bShape\s*\{', text)[1:]
    zeros = []
    for ordinal, shape in enumerate(shapes):
        points = re.search(r'\bpoint\s*\[([^\]]*)\]', shape)
        indices = re.search(r'\bcoordIndex\s*\[([^\]]*)\]', shape)
        if not points or not indices:
            continue
        numbers = [Fraction(t) for t in re.split(r'[\s,]+', points[1].strip()) if t]
        vertices = [numbers[i:i+3] for i in range(0, len(numbers), 3)]
        faces = [int(t) for t in re.split(r'[\s,]+', indices[1].strip()) if t]
        assert len(faces) % 4 == 0
        for face in range(len(faces)//4):
            ids = faces[face*4:face*4+3]
            assert faces[face*4+3] == -1
            a,b,c = [vertices[i] for i in ids]
            u,v = [b[i]-a[i] for i in range(3)], [c[i]-a[i] for i in range(3)]
            if all(u[i]*v[j]-u[j]*v[i] == 0 for i,j in ((1,2),(2,0),(0,1))):
                zeros.append((ordinal,face,ids))
    return zeros


def run(args):
    output, baseline, library = args.output, args.baseline, args.library
    output.mkdir(parents=True, exist_ok=False)
    baseline_doc,_ = parse_glb(baseline/'partial.glb')
    assert baseline_doc['asset']['extras']['tokito']['source']['sha256'] == sha(args.board.read_bytes())
    assert len([n for n in baseline_doc['nodes'] if n.get('name','').startswith('ref:')]) == 21
    isolated = output/'library'
    shutil.copytree(baseline/'library', isolated)
    intake = json.loads((library/'intake.json').read_text())
    assert intake['sourceCommit'] == 'd5db5cea003fe2b4b1c9b145f5fcbd5fdb48f8ca'
    reports = {}
    for ref,path,count,parts,palette in (
        ('J1','Connector_USB.3dshapes/USB_Micro-B_Molex_47346-0001.wrl',7,483,True),
        ('D2','LED_SMD.3dshapes/LED_0805_2012Metric.wrl',2,50,False)):
        source = library/path
        original = source.read_bytes()
        assert sha(original) == next(a['sha256'] for a in intake['assets'] if a['path']==path)
        flags = ('--remove-unused-invalid-materials', '--remove-zero-area-faces') if palette else ('--remove-zero-area-faces',)
        derived = output/(ref+'.derived.wrl')
        report = json.loads(invoke(args.binary,'model','derive-wrl',source,'--output',derived,'--units','kicad-wrl',*flags).stdout)
        face_source = original
        if palette:
            removed = report['paletteDerivation']['removedPaletteShapes']
            assert len(removed)==1 and removed[0]['materialDef']=='GLASS-19'
            face_source = remove_spans(original,[(r['startByte'],r['endByteExclusive']) for r in removed])
        assert sha(face_source)==report['faceSourceSha256']
        actual = [(r['rootShapeOrdinalInFaceSource'],r['faceOrdinal'],r['indices']) for r in report['removedZeroAreaFaces']]
        assert actual == exact_faces(face_source) and len(actual)==count
        assert remove_spans(face_source,report['removedByteSpans']) == derived.read_bytes()
        assert sha(derived.read_bytes())==report['derivedSha256']
        assert not exact_faces(derived.read_bytes())
        assert report['derivedMeshParts']==parts
        assert report['derivedTriangles']==(4840 if ref=='J1' else 358)
        shutil.copyfile(derived,isolated/path)
        assert source.read_bytes()==original
        reports[ref]=report
    common=('export','glb',args.board,'--outline-chaining-tolerance','0.01','--assemble-wrl',
        '--project-root',args.board.parent,'--library-root',isolated,'--model-var',f'KICAD9_3DMODEL_DIR={isolated}','--json-report')
    invoke(args.binary,*common,'--output',output/'complete.glb',succeeds=False)
    assert not (output/'complete.glb').exists()
    glb=output/'partial.glb'
    export=json.loads(invoke(args.binary,*common,'--output',glb,'--allow-partial').stdout)
    (output/'export-report.json').write_text(json.dumps(export,indent=2))
    doc,_=parse_glb(glb)
    selectors={n['name']:n for n in doc['nodes'] if n.get('name','').startswith('ref:')}
    assert len(selectors)==export['coverage']['componentModelsEmitted']==23, export['coverage']
    assert {r['componentReference'] for r in export['coverage']['unresolvedModels']}=={'U3'}
    component_map=json.loads((baseline/'component-map.json').read_text())
    for ref in ('J1','D2'):
        row=next(r for r in component_map['components'] if r['reference']==ref)
        node=selectors['ref:'+ref]
        assert node['extras']['tokito']['componentUuid']==row['component_id']
        primitives=doc['meshes'][node['mesh']]['primitives']
        triangles=sum(doc['accessors'][p['indices']]['count']//3 if 'indices' in p else doc['accessors'][p['attributes']['POSITION']]['count']//3 for p in primitives)
        assert triangles==reports[ref]['derivedTriangles']
        row['glb_selector']='ref:'+ref
    revision='explicit-exact-zero-face-derivation-d5db5cea'
    component_map.update(renderable_count=23,assemblyContextRevision=revision)
    manifest=json.loads((baseline/'manifest.json').read_text())
    positions=[a for a in doc['accessors'] if a.get('type')=='VEC3' and 'min' in a and 'max' in a]
    manifest.update(assemblyContextRevision=revision,glbBytes=glb.stat().st_size,glbSha256=sha(glb.read_bytes()),
        boundsMetres={'minimum':dict(zip('xyz',[min(a['min'][i] for a in positions) for i in range(3)])),
                     'maximum':dict(zip('xyz',[max(a['max'][i] for a in positions) for i in range(3)]))})
    (output/'component-map.json').write_text(json.dumps(component_map,indent=2))
    (output/'manifest.json').write_text(json.dumps(manifest,indent=2))
    invoke(args.binary,'twin','prototype',glb,'--component-map',output/'component-map.json',
        '--manifest',output/'manifest.json','--output',output/'prototype','--allow-partial','--reference-bom',args.bom)
    (output/'derivation-reports.json').write_text(json.dumps(reports,indent=2))
    receipt={'passed':True,'sourceBoardSha256':sha(args.board.read_bytes()),'componentModelsEmitted':23,
        'independentExactDecimalFaceComparisonPassed':True,'fullAssemblyRefusalPreserved':True,
        'missingModels':['U3'],'glbSha256':sha(glb.read_bytes()),'physicalFitVerified':False,'photographedBoardMatchVerified':False}
    (output/'receipt.json').write_text(json.dumps(receipt,indent=2))
    print('PASS ESP32: USB/LED exact-zero-face derivation;23 selectable bodies;U3 remains missing')


if __name__=='__main__':
    p=argparse.ArgumentParser()
    for name in ('binary','board','library','baseline','bom','output'):
        p.add_argument('--'+name,type=Path,required=True)
    a=p.parse_args()
    for name in vars(a): setattr(a,name,getattr(a,name).resolve())
    run(a)
