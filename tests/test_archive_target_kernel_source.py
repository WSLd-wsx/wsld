import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/archive-target-kernel-source.py'
spec = importlib.util.spec_from_file_location('archive_target_kernel_source', SCRIPT)
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)


class ArchiveKernelSourceTests(unittest.TestCase):
    def fixture(self, root):
        kernel = root / 'kernel'; kernel.mkdir()
        subprocess.run(['git', 'init', '-q', str(kernel)], check=True)
        def git(*args): return subprocess.check_output(['git', '-C', str(kernel), *args], text=True).strip()
        git('config', 'user.name', 'Test'); git('config', 'user.email', 'test@example.invalid')
        (kernel/'source.c').write_text('kernel source\n'); git('add', 'source.c'); git('commit', '-qm', 'source')
        commit = git('rev-parse', 'HEAD'); tree = git('rev-parse', 'HEAD^{tree}')
        (kernel/'wsx-source-evidence.json').write_text(json.dumps({'source_commit': commit, 'source_tree': tree,
            'package_build_revision': commit, 'package_build_tree': tree, 'source_tag_verified': True,
            'source_tree_matches_package_metadata': True}))
        return kernel

    def test_archives_only_clean_commit_and_records_hash(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); kernel=self.fixture(root)
            (kernel/'.config').write_text('generated configuration')
            result=module.archive(kernel,root/'source.tar.gz',root/'receipt.json')
            self.assertEqual(result['sha256'],hashlib.sha256((root/'source.tar.gz').read_bytes()).hexdigest())
            with tarfile.open(root/'source.tar.gz') as tar:
                self.assertEqual(tar.getnames(),['source.c'])
                self.assertEqual(tar.extractfile('source.c').read(),b'kernel source\n')

    def test_rejects_changed_tracked_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); kernel=self.fixture(root); (kernel/'source.c').write_text('changed')
            with self.assertRaisesRegex(ValueError,'not clean'):
                module.archive(kernel,root/'source.tar.gz',root/'receipt.json')
            self.assertFalse((root/'source.tar.gz').exists())

    def test_rejects_unmatched_package_tree(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); kernel=self.fixture(root)
            path=kernel/'wsx-source-evidence.json'; evidence=json.loads(path.read_text()); evidence['package_build_tree']='f'*40
            path.write_text(json.dumps(evidence))
            with self.assertRaisesRegex(ValueError,'not verified'):
                module.archive(kernel,root/'source.tar.gz',root/'receipt.json')


if __name__ == '__main__': unittest.main()
