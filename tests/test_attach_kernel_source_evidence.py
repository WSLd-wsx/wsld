import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/attach-kernel-source-evidence.py'
spec = importlib.util.spec_from_file_location('attach_source_test', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SourceTests(unittest.TestCase):
    def test_attaches_checked_kernel_source_archive_and_rejects_changes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); assets = root/'assets'; workspace = root/'workspace'; assets.mkdir()
            (workspace/'modules-build').mkdir(parents=True); (workspace/'kernel-evidence').mkdir()
            config = workspace/'kernel-evidence/config'; config.write_bytes(b'config')
            generation='a'*64; commit='c'*40; tree='d'*40; revision='e'*40
            (assets/'index.json').write_text(json.dumps({'generation':generation,'groups':{'b'*64:{}},'build_inputs':{}}))
            (workspace/'modules-build/build-result.json').write_text(json.dumps({'kernel_source_commit':commit,
                'build_generation':generation,'kernel_config_sha256':hashlib.sha256(b'config').hexdigest()}))
            (workspace/'kernel-evidence/wsx-source-evidence.json').write_text(json.dumps({'source_commit':commit,
                'tag':'linux-msft-wsl-6.18.58.1','source_tag_verified':True,'source_tree':tree,
                'package_build_revision':revision,'package_build_tree':tree,'source_tree_matches_package_metadata':True}))
            archive=workspace/'kernel-source.tar.gz'; archive.write_bytes(b'archive')
            (workspace/'kernel-source-receipt.json').write_text(json.dumps({'schema_version':1,'source_commit':commit,
                'source_tree':tree,'package_build_revision':revision,'package_build_tree':tree,
                'sha256':hashlib.sha256(b'archive').hexdigest(),'bytes':7}))
            result=module.attach(assets,workspace,archive)
            self.assertEqual(result['kernel_source_archive']['sha256'],hashlib.sha256(b'archive').hexdigest())
            self.assertTrue(result['kernel_source_tree_evidence']['source_tree_matches_package_metadata'])
            self.assertFalse(result['source_correspondence_verified'])
            archive.write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError,'differs from build source'):
                module.attach(assets,workspace,archive)

    def test_records_upstream_commit_and_rejects_changed_config(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            assets, workspace = root / 'assets', root / 'workspace'
            assets.mkdir()
            (workspace / 'modules-build').mkdir(parents=True)
            (workspace / 'kernel-evidence').mkdir()
            config = workspace / 'kernel-evidence/config'
            config.write_bytes(b'config')
            generation = 'a' * 64
            (assets / 'index.json').write_text(json.dumps({'generation': generation, 'groups': {'b' * 64: {}},
                                                            'build_inputs': {'asset': 'inputs.tar.gz'}}))
            (workspace / 'modules-build/build-result.json').write_text(json.dumps({
                'kernel_source_commit': 'c' * 40, 'build_generation': generation,
                'kernel_config_sha256': hashlib.sha256(b'config').hexdigest()}))
            (workspace / 'kernel-evidence/wsx-source-evidence.json').write_text(json.dumps({
                'source_commit': 'c' * 40, 'tag': 'linux-msft-wsl-6.18.58.1', 'source_tag_verified': True}))
            result = module.attach(assets, workspace)
            self.assertEqual(result['kernel_source_commit'], 'c' * 40)
            self.assertEqual(result['kernel_source']['commit'], 'c' * 40)
            self.assertEqual(module.sha(assets / result['kernel_config']['asset']), result['kernel_config']['sha256'])
            self.assertFalse(any(assets.glob('kernel-source-*.tar.gz')))
            self.assertFalse(result['release_ready'])
            config.write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError, 'does not match'):
                module.attach(assets, workspace)


if __name__ == '__main__':
    unittest.main()
