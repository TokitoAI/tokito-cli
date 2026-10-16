"""Actual subprocess checks for help, presentation, doctor and actionable input errors."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    binary = parser.parse_args().binary.resolve(strict=True)
    env = dict(os.environ)
    env.pop('NO_COLOR', None)

    def call(*args, code=0, overrides=None):
        result = subprocess.run([str(binary), *map(str, args)], cwd=ROOT, env={**env, **(overrides or {})},
                                text=True, encoding='utf-8', capture_output=True, timeout=60)
        assert result.returncode == code, (args, result.returncode, result.stdout, result.stderr)
        return result

    root_help = call('--help').stdout
    assert '\x1b' not in root_help
    assert all(word in root_help for word in ['START HERE:', 'WORKFLOW:', 'doctor', '--color', 'NO_COLOR'])
    for topic, children in [('export', ['svg', 'glb', 'pos', 'placements']),
                            ('model', ['inspect', 'derive-wrl', 'assignments']), ('component', ['footprint', 'envelope']),
                            ('twin', ['audit-population','prototype','adapt-source']), ('package', ['build','verify'])]:
        assert all(child in call(topic, '--help').stdout for child in children)
    assert 'SPEC.json' in call('component', 'footprint', 'generate', '--help').stdout
    assert 'remove-zero-area-faces' in call('model','derive-wrl','--help').stdout
    assert 'USAGE' in call('pcb','inspect','board with spaces.kicad_pcb','--help').stdout
    call('help','project','nonexistent-command',code=2)
    assert 'read' in call('help', 'doctor').stdout.lower() or 'without modifying' in call('help', 'doctor').stdout
    coloured = call('--color', 'always', '--help').stdout
    assert '\x1b[38;2;8;201;170m' in coloured and '\x1b[38;2;84;86;240m' in coloured
    assert '\x1b' not in call('--color', 'never', '--help').stdout
    assert '\x1b' not in call('--no-color', '--help').stdout
    assert '\x1b' not in call('--color', 'always', '--help', overrides={'NO_COLOR': '1'}).stdout
    assert '\x1b' not in call('--help', overrides={'TERM': 'dumb'}).stdout
    call('--color', 'purple', '--help', code=2)
    call('--no-color', '--color', 'always', '--help', code=2)
    report = json.loads(call('--color', 'always', 'doctor', '--json').stdout)
    assert report['scope'] == 'runtime_information_only' and report['status'] == 'passed'
    assert not report['geometryQualified'] and not report['appAdmissionAuthorized']

    with tempfile.TemporaryDirectory(prefix='cli-experience-', dir=ROOT/'target') as temporary:
        root = Path(temporary)
        project = root/'project'; project.mkdir()
        source = (ROOT/'tests/fixtures/assembly-e2e/single-resistor.kicad_pcb').read_bytes()
        board = project/'board.kicad_pcb'; board.write_bytes(source)
        report = json.loads(call('doctor', project, '--json').stdout)
        assert report['status'] == 'passed' and report['source']['sha256'] == hashlib.sha256(source).hexdigest()
        assert any(row['code'] == 'component_models' and row['status'] == 'not_checked' for row in report['checks'])
        assert 'outline' in call('--color', 'always', 'doctor', board).stdout
        second = project/'second.kicad_pcb'; second.write_bytes(source)
        refused = json.loads(call('doctor', project, '--json', code=2).stdout)
        assert refused['status'] == 'failed'
        assert 'multiple' in refused['checks'][-1]['message']
        call('doctor', project, '--board', board.name, '--json')
        call('doctor', project, '--board', '../outside.kicad_pcb', '--json', code=2)
        call('doctor', project, '--json', '--json', code=2)
        call('doctor', '--board', board.name, code=2)
        call('doctor', project, '--unknown', code=2)
        call('doctor', root/'missing.kicad_pcb', '--json', code=2)
        empty=root/'empty'; empty.mkdir()
        call('doctor', empty, '--json', code=2)
        invalid=root/'invalid.kicad_pcb'
        invalid.write_text('(kicad_pcb (version 20240108) (general (thickness 1.6)))', encoding='utf-8')
        invalid_report=json.loads(call('doctor', invalid, '--json', code=2).stdout)
        assert any(row['code']=='outline' and row['status']=='failed' for row in invalid_report['checks'])
        before={p.name:p.read_bytes() for p in project.iterdir()}
        call('doctor', project, '--board', board.name)
        assert before == {p.name:p.read_bytes() for p in project.iterdir()}

        for newline in ['\n', '\r\n']:
            pointer=root/'pointer.kicad_pcb'
            pointer.write_bytes(newline.join(['version https://git-lfs.github.com/spec/v1',
                'oid sha256:'+hashlib.sha256(source).hexdigest(), 'size '+str(len(source)), '']).encode())
            failed=call('inspect', pointer, code=2)
            assert 'Git LFS pointer' in failed.stderr and str(pointer) in failed.stderr and 'materialize' in failed.stderr
            failed=call('doctor', pointer, '--json', code=2)
            assert 'Git LFS pointer' in failed.stdout
            failed=call('project', 'inspect', root, '--board', pointer.name, '--json', code=2)
            assert 'Git LFS pointer' in failed.stdout+failed.stderr
        output=root/'must-not-exist.glb'
        call('export','glb',pointer,'--output',output,'--allow-partial',code=2)
        assert not output.exists()

        paths=[root/name for name in ['assembly.glb','map.json','index.json','provenance.json','coverage.json']]
        for missing in range(5):
            for i,path in enumerate(paths):
                if path.exists(): path.unlink()
                if i != missing: path.write_bytes(b'{}')
            output=root/f'adapted-{missing}'
            failed=call('twin','adapt-source',paths[0],'--component-map',paths[1],'--geometry-index',paths[2],
                '--provenance',paths[3],'--coverage',paths[4],'--board-id','demo','--board-revision','test',
                '--output',output,code=2)
            assert str(paths[missing]) in failed.stderr and 'same pinned candidate' in failed.stderr
            assert not output.exists()
    print('PASS CLI experience E2E: grouped help, Tokito colours, plain/JSON output, doctor scopes/refusals, LFS diagnostics, five missing candidate inputs, no source mutation or failed-output publication')


if __name__ == '__main__':
    main()
