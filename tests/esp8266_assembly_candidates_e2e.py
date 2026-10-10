"""Independent full-row candidate join check against materialized source evidence."""
import argparse
import hashlib
import json
import math
from decimal import Decimal
from pathlib import Path
import subprocess


def run(binary, source, output):
    pcb = source / 'NODEMCU_DEVKIT_V1.0.PcbDoc'
    extraction = json.loads((source / 'structured-source/extraction.json').read_text())
    digest = hashlib.sha256(pcb.read_bytes()).hexdigest()
    assert digest == extraction['source_sha256']
    process = subprocess.run([str(binary), str(pcb)], capture_output=True, timeout=45)
    assert process.returncode == 2 and not process.stderr
    report = json.loads(process.stdout)
    assert report['sourceSha256'] == digest and report['admitted'] is False
    streams = {s['path']: s for s in report['streams']}
    records = lambda path: streams[path]['framedRecordInspection']['records']
    components = records('/Components6/Data')
    models = records('/Models/Data')
    bodies = records('/ComponentBodies6/Data')
    assert [r['properties'] for r in components] == json.loads(
        (source / 'structured-source/components.json').read_text())
    association = report['candidateAssociations']
    assert association['sourceSha256'] == digest
    assert association['assemblyAdmitted'] is False and association['exportsProduced'] == []
    assert association['unresolvedComponentCandidates'] == []
    assert len(association['components']) == 34 and association['bodyRecords'] == 62
    assert association['modelRecords'] == 17 and association['uniqueModelIds'] == 13
    visited = []
    for original, group in zip(components, association['components'], strict=True):
        assert group['componentOrdinal'] == original['ordinal']
        assert group['componentIndexSemanticsIndependentlyVerified'] is False
        assert group['sourceProperties'] == original['properties']
        assert group['sourceIdentity']['uniqueId'] == original['properties']['UNIQUEID']
        assert group['sourceIdentity']['reference'] == original['properties']['SOURCEDESIGNATOR']
        assert group['sourceIdentity']['namespace'] == 'altium-pcb-uniqueid'
        assert group['selectableGeometryExported'] is False
        expected = [b for b in bodies if b['componentIndexCandidate'] == original['ordinal']]
        assert len(expected) == len(group['bodyCandidates'])
        for body, candidate in zip(expected, group['bodyCandidates'], strict=True):
            visited.append(candidate['bodyOrdinal'])
            assert candidate['bodyOrdinal'] == body['ordinal']
            for field in ('byteOffset', 'componentIndexCandidate', 'remainingPayloadBytes'):
                assert candidate[field] == body[field]
            assert candidate['sourceProperties'] == body['properties']
            pose = candidate['sourcePoseInputs']
            assert pose['numericInputsResolved'] is True
            assert pose['modelsTablePoseApplied'] is False and pose['poseVerified'] is False
            assert pose['worldTransform'] is None
            def metric(properties, key):
                text = properties[key].strip()
                assert text.endswith('mil')
                return float(Decimal(text[:-3]) * Decimal('0.0254'))
            def near(actual, expected):
                assert math.isfinite(actual) and math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-12)
            for axis, key in enumerate(('X', 'Y')):
                near(pose['component']['positionMm'][axis], metric(original['properties'], key))
                near(pose['body']['positionMm'][axis], metric(body['properties'], 'MODEL.2D.' + key))
            near(pose['body']['zOffsetMm'], metric(body['properties'], 'MODEL.3D.DZ'))
            near(pose['component']['rotationDegrees'], float(Decimal(original['properties']['ROTATION'])))
            near(pose['body']['rotation2dDegrees'], float(Decimal(body['properties']['MODEL.2D.ROTATION'])))
            for axis, key in enumerate(('ROTX', 'ROTY', 'ROTZ')):
                near(pose['body']['rotation3dDegrees'][axis], float(Decimal(body['properties']['MODEL.3D.' + key])))
            assert pose['component']['side'] == {'TOP': 'top', 'BOTTOM': 'bottom'}[original['properties']['LAYER']]
            assert pose['body']['projection'] == {'0': 'top', '1': 'bottom'}[body['properties']['BODYPROJECTION']]
            matches = [m for m in models if m['properties']['ID'] == body['properties']['MODELID']]
            assert [m['modelOrdinal'] for m in candidate['modelCandidates']] == [m['ordinal'] for m in matches]
            for original_model, model in zip(matches, candidate['modelCandidates'], strict=True):
                assert model['sourceProperties'] == original_model['properties']
                step = source / f"structured-source/embedded-models/{model['modelOrdinal']}.step"
                assert model['embeddedStepEvidence']['decompressedSha256'] == hashlib.sha256(step.read_bytes()).hexdigest()
            assert candidate['explicitModelIdResolved'] is True
            assert candidate['embeddedByteIdentityAgrees'] is True
            assert candidate['modelPlacementPropertiesAgree'] == all(m['properties'] == matches[0]['properties'] for m in matches)
            assert candidate['selectedModelOrdinal'] is None and candidate['worldTransform'] is None
            assert all(candidate[f] is False for f in ('componentAssociationVerified', 'geometryQualified', 'poseVerified'))
        if group['sourceIdentity']['reference'] in ('J1', 'J2'):
            assert len(group['bodyCandidates']) == 15
    assert sorted(visited) == list(range(62))
    output.mkdir(parents=True, exist_ok=False)
    artifact = output / 'candidate-associations.json'
    artifact.write_text(json.dumps(association, indent=2))
    (output / 'receipt.json').write_text(json.dumps({'passed': True, 'sourceSha256': digest,
        'candidateAssociationsSha256': hashlib.sha256(artifact.read_bytes()).hexdigest(),
        'componentRowsCompared': 34, 'bodyRowsCompared': 62, 'modelRowsCompared': 17,
        'sourceNumericPoseRowsCompared': 62, 'independentDecimalUnitComparisonPassed': True,
        'assemblyAdmitted': False, 'poseVerified': False, 'geometryQualified': False}, indent=2))
    print('PASS: 34 identity groups, 62 bodies, all ambiguous models retained; no pose/export admission')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    for name in ('binary', 'source', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    run(args.binary.resolve(strict=True), args.source.resolve(strict=True), args.output.resolve())
