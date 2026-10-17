"""Real CLI journeys using synthetic identity conflicts, never CAD qualification."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
from e2e_temp import e2e_temp_root


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', required=True)
    options = parser.parse_args()
    binary = Path(options.binary).resolve(strict=True)
    root = Path(__file__).resolve().parents[1]
    parent = e2e_temp_root(root / 'target' / 'bom-e2e')
    with tempfile.TemporaryDirectory(dir=parent) as folder:
        folder = Path(folder)
        board = folder / 'synthetic-identity-only.kicad_pcb'
        board.write_text('''(kicad_pcb (version 20240108) (generator "bom-e2e")
          (general (thickness 1.6)) (layers (0 "F.Cu" signal) (31 "B.Cu" signal))
          (footprint "Test:R" (layer "F.Cu") (uuid "component-r1") (at 5 6 90)
            (property "Reference" "R1") (property "Value" "1k")))''', encoding='utf-8')
        bom = folder / 'synthetic-bom.csv'
        bom.write_text('footprint_index,value,footprint,reference\n0,"1k, test",Test:R,R1\n1,unknown,Test:R,R99\n', encoding='utf-8')
        command = [str(binary), 'bom', 'reconcile', str(board), '--bom', str(bom), '--format', 'esp32-csv', '--json']
        result = subprocess.run(command, capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert result.returncode == 0, result.stderr
        report = json.loads(result.stdout)
        assert report['boardRevisionMatchVerified'] is False
        assert report['schema'] == 'tokito.bom-reconciliation.v1'
        assert report['matchedIdentityCount'] == 1
        assert report['unmatchedBomReferenceCount'] == 1
        assert report['boardMissingFromBomCount'] == 0
        assert report['rowCount'] == 2
        assert all(report[key] is False for key in ('manufacturerPartMatchVerified', 'modelVerified', 'poseVerified', 'geometryVerified'))
        joins = {join['reference']: join for row in report['rows'] for join in row['referenceJoins']}
        assert joins['R1']['status'] == 'matched_identity'
        assert joins['R1']['boardUuid'] == 'component-r1'
        assert joins['R99']['status'] == 'unmatched_bom_reference'
        (folder / 'report.json').write_text(result.stdout, encoding='utf-8')
        bom.write_text('footprint_index,value,footprint,reference\n0,"unterminated,Test:R,R1\n', encoding='utf-8')
        malformed = subprocess.run(command, capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert malformed.returncode != 0, 'Malformed CSV unexpectedly accepted'
        unsupported = subprocess.run([str(binary), 'bom', 'reconcile', str(board), '--bom', str(bom), '--format', 'unknown'], capture_output=True, text=True, timeout=30)
        assert unsupported.returncode != 0
        bom.write_text('footprint_index,value,footprint,reference\n0,1k,Test:R,R1\n', encoding='utf-8-sig')
        encoded = subprocess.run(command, capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert encoded.returncode == 0, encoded.stderr
        assert json.loads(encoded.stdout)['matchedIdentityCount'] == 1
        board.write_text('(kicad_pcb (version 20240108) (generator "cap-test") ' +
                         '(footprint "Test:R" (layer "F.Cu") (uuid "repeated") (at 0 0) (property "Reference" "R1")) ' * 10001 + ')', encoding='utf-8')
        refused = subprocess.run(command, capture_output=True, text=True, encoding='utf-8', timeout=60)
        assert refused.returncode != 0, 'Cap refusal unexpectedly exited successfully'
        refusal = json.loads(refused.stdout)
        assert refusal['status'] == 'refused'
        assert refusal['geometryVerified'] is False
    print('BOM CLI E2E: identity report, UTF-8 BOM and malformed/unsupported/cap refusal passed')


if __name__ == '__main__':
    main()
