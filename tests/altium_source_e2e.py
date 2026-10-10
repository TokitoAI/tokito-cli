"""Actual pinned ESP8266 source-record audit; no geometry or placement admission."""
import argparse
from collections import defaultdict
import hashlib
import json
from decimal import Decimal
from pathlib import Path
import subprocess

def run(binary, source, output):
    output.mkdir(parents=True, exist_ok=False)
    pcb = source / 'NODEMCU_DEVKIT_V1.0.PcbDoc'
    extracted = json.loads((source / 'structured-source/extraction.json').read_text())
    components = json.loads((source / 'structured-source/components.json').read_text())
    assert hashlib.sha256(pcb.read_bytes()).hexdigest() == extracted['source_sha256']
    process = subprocess.run([str(binary), str(pcb)], capture_output=True, timeout=45)
    assert process.returncode == 2 and not process.stderr
    report = json.loads(process.stdout)
    assert 'error' not in report and report['admitted'] is False and report['exportsProduced'] == []
    assert report['sourceSha256'] == extracted['source_sha256']
    streams = {s['path']: s for s in report['streams']}
    board = streams['/Board6/Data']
    assert board['declaredBytes'] == 83174
    assert board['sha256'] == '7d1f2d9e12ccbf710401c6b17de6ecf0c85981ecb0660189802db3112c7c4852'
    board_rows = board['framedRecordInspection']['records']
    assert len(board_rows) == 1 and board_rows[0]['payloadBytes'] == 83170
    sections = board_rows[0]['propertySections']
    assert len(sections) == 27
    # Independent pinned source observations: do not flatten layer/view sections.
    primary = sections[0]['properties']
    assert board_rows[0]['properties'] == primary
    assert 'V9_STACK_LAYER3_COPTHICK' not in primary
    stack = sections[1]['properties']
    assert stack['RECORD'] == 'Board'
    for key, expected in {
        'LAYER1COPTHICK':'1.4mil', 'LAYER1DIELHEIGHT':'12.6mil',
        'LAYER1PREV':'0', 'LAYER1NEXT':'32',
        'V9_STACK_LAYER3_COPTHICK':'1.4mil', 'V9_STACK_LAYER4_DIELHEIGHT':'12.6mil',
        'V9_STACK_LAYER5_COPTHICK':'1.4mil',
        'V9_STACK_LAYER2_DIELHEIGHT':'0.4mil', 'V9_STACK_LAYER6_DIELHEIGHT':'0.4mil',
    }.items():
        assert stack[key] == expected
    bottom = sections[7]['properties']
    assert bottom['LAYER32COPTHICK'] == '1.4mil'
    assert bottom['LAYER32PREV'] == '1' and bottom['LAYER32NEXT'] == '0'
    total = sum(Decimal(stack[key][:-3]) for key in (
        'V9_STACK_LAYER3_COPTHICK','V9_STACK_LAYER4_DIELHEIGHT','V9_STACK_LAYER5_COPTHICK')) * Decimal('0.0254')
    assert total == Decimal('0.39116')
    vertices = [(2579.9606,2163.3268),(2579.9606,3883.3268),(2669.9606,3973.3268),
        (3489.9606,3973.3268),(3579.9606,3883.3268),(3579.9606,2163.3268),
        (3489.9606,2073.3268),(2669.9606,2073.3268),(2579.9606,2163.3268)]
    for i, (x,y) in enumerate(vertices):
        assert float(Decimal(primary[f'VX{i}'][:-3])) == x
        assert float(Decimal(primary[f'VY{i}'][:-3])) == y
        assert primary[f'KIND{i}'] == str(i % 2)
        if i % 2:
            assert primary[f'R{i}'] == '90mil'
    assert primary['ORIGINX'] == '3079.9606mil' and primary['ORIGINY'] == '3523.3268mil'
    normalized = report['boardGeometryInputs']
    assert normalized['numericInputsResolved'] is True
    assert normalized['sourceSha256'] == report['sourceSha256']
    assert normalized['sourceStream']['sha256'] == board['sha256']
    def value(key):
        rows = [(s['sectionOrdinal'], s['properties'][key]) for s in sections if key in s['properties']]
        assert rows and all(v == rows[0][1] for _, v in rows)
        assert normalized['sourceProperties'][key]['sectionOrdinals'] == sorted({n for n, _ in rows})
        return rows[0][1]
    def mm(key):
        text = value(key)
        assert text.endswith('mil')
        return Decimal(text[:-3]) * Decimal('0.0254')
    def close(actual, expected):
        assert abs(Decimal(str(actual)) - expected) <= Decimal('1e-12')
    close(normalized['sourceOriginMm'][0], mm('ORIGINX'))
    close(normalized['sourceOriginMm'][1], mm('ORIGINY'))
    physical = normalized['stack']
    for field, key in (('topCopperMm','LAYER1COPTHICK'),('coreMm','LAYER1DIELHEIGHT'),('bottomCopperMm','LAYER32COPTHICK')):
        close(physical[field], mm(key))
    close(physical['copperAndCoreThicknessMm'], total)
    close(physical['maskInclusiveThicknessMm'], Decimal('0.41148'))
    assert len(normalized['contourRecords']) == 9
    for row in normalized['contourRecords']:
        i = row['ordinal']
        assert row['kind'] == {'0':'line','1':'arc'}[value(f'KIND{i}')]
        for field, keys in (('pointMm',(f'VX{i}',f'VY{i}')),('centerMm',(f'CX{i}',f'CY{i}'))):
            for actual, key in zip(row[field], keys, strict=True):
                close(actual, mm(key))
        close(row['radiusMm'], mm(f'R{i}'))
        close(row['startAngleDegrees'], Decimal(value(f'SA{i}')))
        close(row['endAngleDegrees'], Decimal(value(f'EA{i}')))
    assert normalized['sourceZOrigin'] is None
    assert all(normalized[k] is False for k in ('contourTopologyVerified','physicalThicknessVerified','worldPoseVerified','geometryQualified'))
    assert normalized['exportsProduced'] == []
    decoded = {}
    for name in ['/Models/Data', '/Components6/Data', '/ComponentBodies6/Data']:
        audit = streams[name]['framedRecordInspection']
        assert audit['profileFramingResolved'], audit
        decoded[name] = audit['records']
    model_records, component_records, body_records = (decoded[n] for n in ['/Models/Data', '/Components6/Data', '/ComponentBodies6/Data'])
    assert len(model_records) == 17 and len(component_records) == 34 and len(body_records) == 62
    assert [r['properties'] for r in component_records] == components
    model_hashes = {}
    for model in extracted['models']:
        ordinal = int(model['stream'].split('/')[1])
        step = source / 'structured-source/embedded-models' / f'{ordinal}.step'
        digest = hashlib.sha256(step.read_bytes()).hexdigest()
        assert digest == model['sha256']
        embedded = streams[f'/Models/{ordinal}']['embeddedStep']
        assert embedded['streamOrdinal'] == ordinal and embedded['completeZlibStream'] is True
        assert embedded['decompressedSha256'] == digest
        assert embedded['decompressedBytes'] == step.stat().st_size
        model_hashes[ordinal] = digest
    ids = defaultdict(list)
    for record in model_records:
        ids[record['properties']['ID']].append(record)
    links = []
    for body in body_records:
        index = body['componentIndexCandidate']
        assert index < len(component_records)
        candidates = ids[body['properties']['MODELID']]
        assert candidates
        hashes = {model_hashes[r['ordinal']] for r in candidates}
        assert len(hashes) == 1, 'ambiguous model ID points to different extracted STEP bytes'
        links.append({'bodyOrdinal': body['ordinal'], 'componentIndexCandidate': index,
                      'referenceCandidate': component_records[index]['properties']['SOURCEDESIGNATOR'],
                      'modelOrdinals': [r['ordinal'] for r in candidates], 'stepSha256': next(iter(hashes)),
                      'modelPropertiesAgree': all(r['properties'] == candidates[0]['properties'] for r in candidates),
                      'componentIndexSemanticsIndependentlyVerified': False, 'poseVerified': False})
    assert len({link['referenceCandidate'] for link in links}) == 34
    receipt = {'passed': True, 'sourceSha256': report['sourceSha256'],
               'boardStreamSha256': board['sha256'], 'boardPropertySections': 27,
               'sourceDeclaredCopperCoreThicknessMm': str(total),
               'sourceContourVertexRecordsCompared': 9, 'boardPhysicalFitVerified': False,
               'independentBoardNumericComparisonPassed': True,
               'componentsMatchPublishedExtraction': True, 'components': 34, 'bodyRecords': 62,
               'embeddedModelBytesMatchExtractedSTEP': True,
               'modelRecords': 17, 'uniqueModelIDs': len(ids),
               'duplicateIDGroups': [[r['ordinal'] for r in group] for group in ids.values() if len(group) > 1],
               'candidateLinks': links, 'componentModelLinksIndependentlyVerified': False,
               'geometryQualified': False, 'poseVerified': False, 'admitted': False}
    (output / 'source-evidence.json').write_text(json.dumps(report, indent=2))
    (output / 'receipt.json').write_text(json.dumps(receipt, indent=2))
    print('PASS 34 source components matched; 62 body candidates; 17 models / 13 IDs; duplicate pose metadata retained')

if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--source', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    run(a.binary.resolve(strict=True), a.source.resolve(strict=True), a.output.resolve())
