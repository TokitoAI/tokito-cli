"""Real source-file subprocess checks; Decimal oracle is independent of Rust parsing.

Checks source inventories only: does not certify/render holes or register a photo.
"""
import argparse
from decimal import Decimal
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile

FILES = {
    'RoundHoles-Plated': ('e8e4c5a926c7b079cbbac1081e1ef9e0c62317dff1e361a14f5c4961a6f422f6', 275, 0),
    'RoundHoles-NonPlated': ('aa0a5b4d3cf4e88b9d8b1ce9d1f10a1dc613762ae2e1762b71a229f2e8116f8f', 4, 0),
    'SlotHoles-Plated': ('0a78fdcfba2ebe783076f1285f8b329918dfa175a9df4d9969305e5886aa6c71', 0, 2),
}


def oracle(text):
    assert ';FILE_FORMAT=2:5' in text and 'INCH,LZ' in text
    tools, holes, slots = {}, [], []
    position = [None, None]
    selected = None

    def point(command):
        for axis, number in re.findall(r'([XY])(-?\d+)', command):
            sign = -1 if number.startswith('-') else 1
            digits = number.lstrip('-').ljust(7, '0')
            position['XY'.index(axis)] = Decimal(digits) / Decimal(100000) * Decimal('25.4') * sign
        assert all(value is not None for value in position)
        return [float(value) for value in position]

    for line_number, line in enumerate(text.splitlines(), 1):
        tool = re.fullmatch(r'T(\d+)F00S00C([\d.]+)', line)
        if tool:
            tools[int(tool[1])] = float(Decimal(tool[2]) * Decimal('25.4'))
        elif re.fullmatch(r'T\d+', line):
            selected = int(line[1:])
        elif line.startswith(('X', 'Y')):
            row = {'tool': selected, 'diameterMm': tools[selected], 'sourceLine': line_number}
            if 'G85' in line:
                start, end = line.split('G85')
                row.update(startMm=point(start), endMm=point(end))
                slots.append(row)
            else:
                row['positionMm'] = point(line)
                holes.append(row)
    return tools, holes, slots


def invoke(binary, path):
    result = subprocess.run([str(binary), 'inspect-drill', str(path)], capture_output=True, timeout=15)
    assert result.returncode == 2, result.stderr.decode(errors='replace')
    return json.loads(result.stdout)


def run(binary, source, output):
    receipts = []
    for suffix, (digest, hole_count, slot_count) in FILES.items():
        path = source / ('NODEMCU_DEVKIT_V1.0-' + suffix + '.TXT')
        data = path.read_bytes()
        assert hashlib.sha256(data).hexdigest() == digest, 'source fixture changed: ' + str(path)
        report = invoke(binary, path)
        assert report['sourceSha256'] == digest
        assert report['schema'] == 'tokito.excellon-source-inventory.v1'
        assert report['lengthUnit'] == 'mm' and report['sourceDigits'] == [2, 5]
        assert report['sourceUnitScaleToMm'] == 25.4 and report['leadingZerosRetained'] is True
        for flag in ('admitted', 'geometryQualified', 'sourceBoardFrameVerified', 'physicalFitVerified'):
            assert report[flag] is False
        tools, holes, slots = oracle(data.decode('ascii'))
        assert len(report['holes']) == hole_count == len(holes)
        assert len(report['slots']) == slot_count == len(slots)
        assert report['platingClaim'] == ('NON_PLATED' if suffix.endswith('NonPlated') else 'PLATED')
        assert {row['tool'] for row in report['tools']} == set(tools)
        assert len(report['tools']) == len(tools)
        for group, expected in (('holes', holes), ('slots', slots)):
            for actual, reference in zip(report[group], expected):
                assert actual.keys() == reference.keys()
                for key, value in reference.items():
                    if isinstance(value, list):
                        assert len(actual[key]) == len(value) == 2
                        assert all(abs(a - b) < 1e-12 for a, b in zip(actual[key], value))
                    elif key == 'diameterMm':
                        assert abs(actual[key] - value) < 1e-12
                    else:
                        assert actual[key] == value
        for row in report['tools']:
            assert abs(row['diameterMm'] - tools[row['tool']]) < 1e-12
        receipts.append({'file': path.name, 'sha256': digest, 'holes': hole_count, 'slots': slot_count,
                         'allSourceCoordinatesCompared': True})
    original = (source / 'NODEMCU_DEVKIT_V1.0-SlotHoles-Plated.TXT').read_text()
    with tempfile.TemporaryDirectory() as temp:
        invalid = Path(temp) / 'invalid.TXT'
        for content in (original.replace(';FILE_FORMAT=2:5\n', ''),
                        original.replace('G90', 'G91'), original.replace('T03\n', 'T99\n'),
                        original.replace('M30', ''), original + 'X1Y2\n'):
            invalid.write_text(content)
            refused = invoke(binary, invalid)
            assert refused['admitted'] is False and 'error' in refused
    receipt = {'schema': 'tokito.drill-source-e2e.v1', 'passed': True,
               'binarySha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
               'files': receipts, 'negativeJourneys': 5, 'holesRendered': False,
               'physicalFitVerified': False, 'sourceBoardFrameVerified': False}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    run(args.binary, args.source, args.output)
