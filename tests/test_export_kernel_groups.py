import importlib.util
import hashlib
import io
import json
from pathlib import Path
from types import SimpleNamespace
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from test_kernel_package import create, package

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/export-kernel-groups.py'
spec = importlib.util.spec_from_file_location('export_kernel_groups', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ExportTests(unittest.TestCase):
    def test_two_kernels_share_one_deterministic_group_archive(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            input_root=root/'inputs'; input_root.mkdir(); (input_root/'source').write_bytes(b'source')
            inputs={'source':hashlib.sha256(b'source').hexdigest()}
            identity=hashlib.sha256(json.dumps({'schema_version':1,'inputs':inputs},sort_keys=True,separators=(',',':')).encode()).hexdigest()
            input_archive=root/'build-inputs.tar.gz'
            with patch.object(module.build_inputs_export.generation,'generation',return_value=(identity,inputs)):
                module.build_inputs_export.export(input_root,input_archive)
            create(root / 'one', 'linux-msft-wsl-6.18.58.1', schema=3, generation=identity)
            create(root / 'two', 'linux-msft-wsl-6.18.54.1', schema=3, generation=identity)
            package.merge(root / 'one', root / 'two', root / 'merged')
            first = module.export(root / 'merged', root / 'out1', input_archive)
            second = module.export(root / 'merged', root / 'out2', input_archive)
            self.assertEqual(first, second)
            self.assertEqual(len(first['groups']), 1)
            self.assertEqual(len(first['targets']), 2)
            self.assertEqual(first['schema_version'], 2)
            self.assertEqual(first['build_inputs']['archive_sha256'], package.digest(input_archive))
            self.assertFalse(first['build_inputs']['corresponding_source_complete'])
            group = next(iter(first['groups']))
            self.assertTrue(all(target['group'] == group for target in first['targets'].values()))
            self.assertEqual(package.digest(root / 'out1' / (group + '.tar.gz')),
                             first['groups'][group]['archive_sha256'])
            self.assertEqual(package.digest(root / 'out1' / (group + '.oci.json')),
                             first['groups'][group]['oci_manifest_sha256'])
            with tarfile.open(root / 'out1' / (group + '.tar.gz'), 'r:gz') as archive:
                self.assertEqual(set(archive.getnames()), set(package.MODULES))
            self.assertFalse(first['release_ready'])

            manifest = (root / 'out1' / (group + '.oci.json')).read_bytes()
            archive_data = (root / 'out1' / (group + '.tar.gz')).read_bytes()
            reference = 'sha256:' + hashlib.sha256(manifest).hexdigest()
            class Response(io.BytesIO):
                def __init__(self, data, headers=None):
                    super().__init__(data)
                    self.headers = headers or {}
            class Opener:
                def open(self, request, timeout):
                    if '/token?' in request.full_url:
                        return Response(b'{"token":"test"}')
                    if '/manifests/' in request.full_url:
                        return Response(manifest, {'Docker-Content-Digest': reference})
                    if '/blobs/' in request.full_url:
                        return Response(archive_data)
                    raise AssertionError('Unexpected request')
            pulled = module.registry.pull_group(root / 'pulled.tar.gz', reference, group, opener=Opener())
            self.assertEqual(pulled['group'], group)
            self.assertFalse(pulled['runtime_admitted'])
            extracted = module.registry.extract_group(root / 'pulled.tar.gz', root / 'extracted', group)
            self.assertFalse(extracted['runtime_admitted'])
            self.assertEqual({path.name for path in (root / 'extracted').iterdir()}, set(package.MODULES))
            with self.assertRaisesRegex(ValueError, 'already exists'):
                module.registry.extract_group(root / 'pulled.tar.gz', root / 'extracted', group)
            with self.assertRaisesRegex(ValueError, 'identity mismatch'):
                module.registry.extract_group(root / 'pulled.tar.gz', root / 'bad-extract', 'f' * 64)
            self.assertFalse((root / 'bad-extract').exists())
            with self.assertRaisesRegex(ValueError, 'digest mismatch'):
                module.registry.pull_group(root / 'bad.tar.gz', 'sha256:' + '0' * 64, group, opener=Opener())
            index_manifest = (root / 'out1/index.oci.json').read_bytes()
            index_data = (root / 'out1/index.json').read_bytes()
            index_reference = 'sha256:' + hashlib.sha256(index_manifest).hexdigest()
            class IndexOpener:
                def open(self, request, timeout):
                    if '/token?' in request.full_url:
                        return Response(b'{"token":"test"}')
                    if '/manifests/' in request.full_url:
                        return Response(index_manifest, {'Docker-Content-Digest': index_reference})
                    if '/blobs/' in request.full_url:
                        return Response(index_data)
                    raise AssertionError('Unexpected request')
            checked = module.registry.pull_index(root / 'pulled-index.json', index_reference, opener=IndexOpener())
            self.assertEqual((checked['groups'], checked['targets']), (1, 2))
            self.assertFalse(checked['runtime_admitted'])
            report = root / 'release-report.json'
            report.write_text(json.dumps({'release_gate_passed': True}))
            prior = 'sha256:' + 'a' * 64
            with self.assertRaisesRegex(ValueError, 'Corresponding source delivery is incomplete'):
                module.registry.require_corresponding_source(module, root / 'out1', identity)
            with patch.object(module.registry.subprocess, 'run', return_value=SimpleNamespace(returncode=0)), \
                 patch.object(module.registry, 'verify_snapshot_against_gate'), \
                 patch.object(module.registry, 'require_corresponding_source'), \
                 patch.object(module.registry, 'latest_index_digest', return_value=prior), \
                 patch.object(module.registry, 'get_token', return_value='test'), \
                 patch.object(module.registry, 'upload_blob') as upload, \
                 patch.object(module.registry, 'put_manifest') as put, \
                 patch.object(module.registry, 'pull_group', return_value={'archive_digest': 'sha256:' + first['groups'][group]['archive_sha256']}), \
                 patch.object(module.registry, 'pull_evidence', side_effect=[{'archive_digest': 'sha256:' + first['evidence']['archive_sha256']}, {'archive_digest': 'sha256:' + first['build_inputs']['archive_sha256']}]), \
                 patch.object(module.registry, 'pull_index', return_value={'index_digest': 'sha256:' + hashlib.sha256(index_data).hexdigest()}):
                result = module.registry.publish_groups(root / 'merged', root / 'out1', root, report,
                                                        'actor', 'credential', opener=Opener(), expected_prior=prior)
            self.assertEqual(len(result['groups']), 1)
            self.assertTrue(result['index_published'])
            self.assertFalse(result['latest_updated'])
            self.assertEqual(upload.call_count, 5)
            self.assertEqual(put.call_count, 4)
            with patch.object(module.registry.subprocess, 'run', return_value=SimpleNamespace(returncode=0)), \
                 patch.object(module.registry, 'verify_snapshot_against_gate'), \
                 patch.object(module.registry, 'require_corresponding_source'), \
                 patch.object(module.registry, 'latest_index_digest', side_effect=[prior, 'sha256:' + 'b' * 64]), \
                 patch.object(module.registry, 'get_token', return_value='test'), \
                 patch.object(module.registry, 'upload_blob'), \
                 patch.object(module.registry, 'put_manifest') as conflict_put, \
                 patch.object(module.registry, 'pull_group', return_value={'archive_digest': 'sha256:' + first['groups'][group]['archive_sha256']}), \
                 patch.object(module.registry, 'pull_evidence', side_effect=[{'archive_digest': 'sha256:' + first['evidence']['archive_sha256']}, {'archive_digest': 'sha256:' + first['build_inputs']['archive_sha256']}]), \
                 patch.object(module.registry, 'pull_index', return_value={'index_digest': 'sha256:' + hashlib.sha256(index_data).hexdigest()}):
                with self.assertRaisesRegex(ValueError, 'changed since bootstrap'):
                    module.registry.publish_groups(root / 'merged', root / 'out1', root, report,
                                                   'actor', 'credential', opener=Opener(), expected_prior=prior)
            self.assertTrue(all(call.args[2].startswith('sha256:') for call in conflict_put.call_args_list))
            with patch.object(module.registry.subprocess, 'run', return_value=SimpleNamespace(returncode=0)), \
                 patch.object(module.registry, 'verify_snapshot_against_gate'), \
                 patch.object(module.registry, 'require_corresponding_source'), \
                 patch.object(module.registry, 'latest_index_digest', return_value=prior), \
                 patch.object(module.registry, 'get_token', return_value='test'), \
                 patch.object(module.registry, 'upload_blob') as upload, \
                 patch.object(module.registry, 'put_manifest') as put, \
                 patch.object(module.registry, 'pull_group', side_effect=ValueError('read-back failed')):
                with self.assertRaisesRegex(ValueError, 'read-back failed'):
                    module.registry.publish_groups(root / 'merged', root / 'out1', root, report,
                                                   'actor', 'credential', opener=Opener(), expected_prior=prior)
            self.assertEqual(upload.call_count, 2)
            self.assertEqual(put.call_count, 1)
            index_file = root / 'out1/index.json'
            index = json.loads(index_file.read_text())
            index['targets']['linux-msft-wsl-6.18.54.1']['group'] = 'f' * 64
            index_file.write_text(json.dumps(index))
            with self.assertRaisesRegex(ValueError, 'Invalid kernel target'):
                module.verify_export(root / 'merged', root / 'out1')
            archive = root / 'out2' / (group + '.tar.gz')
            archive.write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError, 'differs from index'):
                module.verify_export(root / 'merged', root / 'out2')
            inputs_file = root / 'out1' / 'build-inputs.tar.gz'
            inputs_file.write_bytes(b'tampered')
            with self.assertRaises((ValueError, tarfile.TarError, OSError)):
                module.build_inputs_export.verify(inputs_file, identity)


if __name__ == '__main__':
    unittest.main()
