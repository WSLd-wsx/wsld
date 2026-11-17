import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class PublishKernelRebuildTests(unittest.TestCase):
    def test_candidate_is_verified_without_replacing_fixed_release(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); work = root / 'work'; work.mkdir()
            source = root / 'source'; version = '6.18.54.1-1'; directory = source / version
            directory.mkdir(parents=True)
            names = [f'modules_{version}.tar.gz', f'modules_{version}-12-1.json', f'source-materials-{version}-12-1.json']
            for name in names:
                (directory / name).write_bytes(b'fixture')
            checksums = subprocess.check_output(['sha256sum', *names], cwd=directory, text=True)
            (directory / f'SHA256SUMS-{version}-12-1').write_text(checksums)
            targets = root / 'targets'; targets.write_text(f'tag\t{version}\n')
            bin_dir = root / 'bin'; bin_dir.mkdir()
            gh = bin_dir / 'gh'
            gh.write_text('''#!/usr/bin/env python3
import os, pathlib, shutil, sys
args=sys.argv[1:]; root=pathlib.Path(os.environ['TEST_ROOT']); store=root/'store'
with (root/'calls').open('a') as f: f.write(' '.join(args)+'\\n')
if args[0]=='api': print('WSLd-wsx/wsld')
elif args[:2]==['release','create']: store.mkdir()
elif args[:2]==['release','upload']:
    for name in args[3:args.index('--repo')]: shutil.copyfile(name,store/pathlib.Path(name).name)
elif args[:2]==['release','download']:
    name=args[args.index('--pattern')+1]; dest=pathlib.Path(args[args.index('--dir')+1]); shutil.copyfile(store/name,dest/name)
    if os.environ.get('BREAK_READBACK')=='1': (dest/name).write_bytes(b'changed')
elif args[:2]==['release','view']: print('true')
''')
            gh.chmod(0o755)
            env = dict(os.environ, PATH=str(bin_dir) + os.pathsep + os.environ['PATH'], TEST_ROOT=str(root),
                       RUNNER_TEMP=str(work), GITHUB_REPOSITORY='WSLd-wsx/wsld', GITHUB_RUN_ID='12',
                       GITHUB_RUN_ATTEMPT='1', GITHUB_SHA='a'*40, GITHUB_STEP_SUMMARY=str(root/'summary'))
            command = ['bash', str(ROOT/'scripts/publish-kernel-rebuild.sh'), str(source), str(targets)]
            result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            calls = (root/'calls').read_text()
            self.assertLess(calls.index('release download'), calls.index('release edit'))
            self.assertNotIn('release delete', calls)
            self.assertNotIn('--tag wsx-kernel-modules', calls)
            for path in work.iterdir():
                import shutil
                shutil.rmtree(path)
            import shutil
            shutil.rmtree(root/'store'); (root/'calls').unlink()
            env['BREAK_READBACK'] = '1'
            result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn('release delete', (root/'calls').read_text())


if __name__ == '__main__': unittest.main()
