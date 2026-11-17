import importlib.util
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

from test_kernel_package import create

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/bootstrap-kernel-snapshot.py'
spec = importlib.util.spec_from_file_location('kernel_bootstrap_test', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class BootstrapTests(unittest.TestCase):
    def test_per_group_restore_rebuilds_verified_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            source=root/'source'
            input_root=root/'input-root';input_root.mkdir();(input_root/'file').write_bytes(b'source')
            inputs={'file':hashlib.sha256(b'source').hexdigest()}
            identity=hashlib.sha256(json.dumps({'schema_version':1,'inputs':inputs},sort_keys=True,separators=(',',':')).encode()).hexdigest()
            input_archive=root/'build-inputs.tar.gz'
            with patch.object(module.inputs_export.generation,'generation',return_value=(identity,inputs)):
                module.inputs_export.export(input_root,input_archive)
            index=create(source,'linux-msft-wsl-6.18.58.1',schema=3,generation=identity)
            exporter_spec=importlib.util.spec_from_file_location('exporter_test',SCRIPT.parent/'export-kernel-groups.py')
            exporter=importlib.util.module_from_spec(exporter_spec);exporter_spec.loader.exec_module(exporter)
            exported=root/'exported';export=exporter.export(source,exported,input_archive)
            reference='sha256:'+'a'*64
            def pull_index(path,_reference,*args,**kwargs):
                shutil.copyfile(exported/'index.json',path);return {}
            def pull_evidence(path,_reference,*args,**kwargs):
                key = 'build_inputs' if _reference == 'sha256:' + export['build_inputs']['oci_manifest_sha256'] else 'evidence'
                shutil.copyfile(exported/('build-inputs.tar.gz' if key == 'build_inputs' else 'evidence.tar.gz'),path)
                return {'archive_digest':'sha256:'+export[key]['archive_sha256']}
            def pull_group(path,_reference,group,*args,**kwargs):
                shutil.copyfile(exported/(group+'.tar.gz'),path)
                return {'archive_digest':'sha256:'+export['groups'][group]['archive_sha256']}
            with patch.object(module.registry,'latest_index_digest',return_value=reference), \
                 patch.object(module.registry,'pull_index',side_effect=pull_index), \
                 patch.object(module.registry,'pull_evidence',side_effect=pull_evidence), \
                 patch.object(module.registry,'pull_group',side_effect=pull_group):
                result=module.bootstrap_groups(root)
            self.assertEqual(result['generation'],index['generation'])
            self.assertEqual(result['groups'],1)
            self.assertFalse(result['release_ready'])
            self.assertTrue(result['build_inputs_verified'])
            self.assertTrue((root/'staging-snapshots/current.json').is_file())
            attempts=list((root/'registry-bootstrap').glob('groups-*'))
            self.assertEqual(len(attempts),1)
            self.assertTrue((attempts[0]/'bootstrap.json').is_file())
            self.assertTrue((attempts[0]/'evidence.tar.gz').is_file())
            self.assertTrue((attempts[0]/'build-inputs.tar.gz').is_file())
            self.assertFalse((attempts[0]/'snapshot').exists())
            self.assertFalse(any((attempts[0]/(group+'.tar.gz')).exists() for group in export['groups']))

    def test_failed_group_restore_retains_evidence_and_never_stages(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);source=root/'source'
            create(source,'linux-msft-wsl-6.18.58.1',schema=3)
            exporter_spec=importlib.util.spec_from_file_location('exporter_failure_test',SCRIPT.parent/'export-kernel-groups.py')
            exporter=importlib.util.module_from_spec(exporter_spec);exporter_spec.loader.exec_module(exporter)
            exported=root/'exported';export=exporter.export(source,exported)
            def pull_index(path,*args,**kwargs): shutil.copyfile(exported/'index.json',path)
            def pull_evidence(path,*args,**kwargs):
                shutil.copyfile(exported/'evidence.tar.gz',path)
                return {'archive_digest':'sha256:'+export['evidence']['archive_sha256']}
            def pull_group(path,*args,**kwargs):
                path.write_bytes(b'partial')
                raise ValueError('download failed')
            with patch.object(module.registry,'latest_index_digest',return_value='sha256:'+'a'*64), \
                 patch.object(module.registry,'pull_index',side_effect=pull_index), \
                 patch.object(module.registry,'pull_evidence',side_effect=pull_evidence), \
                 patch.object(module.registry,'pull_group',side_effect=pull_group):
                with self.assertRaisesRegex(ValueError,'download failed'): module.bootstrap_groups(root)
            attempt=next((root/'registry-bootstrap').glob('groups-*'))
            self.assertTrue((attempt/'evidence.tar.gz').exists())
            self.assertTrue((attempt/'snapshot/index.json').exists())
            self.assertFalse((root/'staging-snapshots/current.json').exists())

    def test_failure_never_initializes_empty_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(module.registry, 'pull', side_effect=ValueError('Registry HTTP 403')):
                with self.assertRaisesRegex(ValueError, '403'):
                    module.bootstrap(root)
            self.assertFalse((root / 'staging-snapshots/current.json').exists())

    def test_success_stages_snapshot_and_refuses_reinitialization(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'source'
            index = create(source, 'linux-msft-wsl-6.18.58.1', schema=3)
            source_archive = root / 'source.tar.gz'
            digest = module.staging.package.archive(source, source_archive)
            def pull(archive, destination, actor, credential):
                archive.write_bytes(source_archive.read_bytes())
                module.staging.package.extract(archive, destination)
                return {'manifest_digest': 'sha256:' + 'a' * 64, 'archive_digest': 'sha256:' + digest,
                        'generation': index['generation']}
            with patch.object(module.registry, 'pull', side_effect=pull):
                result = module.bootstrap(root)
                self.assertFalse(result['release_ready'])
                self.assertEqual(result['generation'], index['generation'])
                pointer = root / 'staging-snapshots/current.json'
                before = pointer.read_bytes()
                with self.assertRaisesRegex(ValueError, 'already initialized'):
                    module.bootstrap(root)
                self.assertEqual(pointer.read_bytes(), before)


if __name__ == '__main__':
    unittest.main()
