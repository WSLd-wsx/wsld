import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/export-kernel-build-inputs.py'
sys.path.insert(0, str(SCRIPT.parent))
spec = importlib.util.spec_from_file_location('export_kernel_inputs_test', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class InputExportTests(unittest.TestCase):
    def test_failed_verification_leaves_no_output_or_temporary_archive(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'source.c').write_bytes(b'example')
            inputs = {'source.c': hashlib.sha256(b'example').hexdigest()}
            canonical = json.dumps({'schema_version': 1, 'inputs': inputs}, sort_keys=True, separators=(',', ':')).encode()
            identity = hashlib.sha256(canonical).hexdigest()
            with patch.object(module.generation, 'generation', return_value=(identity, inputs)):
                with patch.object(module, 'verify', side_effect=ValueError('verification failed')):
                    with self.assertRaisesRegex(ValueError, 'verification failed'):
                        module.export(root, root / 'output.tar.gz')
            self.assertEqual({path.name for path in root.iterdir()}, {'source.c'})

    def test_output_created_during_verification_is_never_replaced(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'source.c').write_bytes(b'example')
            inputs = {'source.c': hashlib.sha256(b'example').hexdigest()}
            canonical = json.dumps({'schema_version': 1, 'inputs': inputs}, sort_keys=True, separators=(',', ':')).encode()
            identity = hashlib.sha256(canonical).hexdigest()
            output = root / 'output.tar.gz'

            def concurrent_output(archive, expected):
                output.write_bytes(b'other process')

            with patch.object(module.generation, 'generation', return_value=(identity, inputs)):
                with patch.object(module, 'verify', side_effect=concurrent_output):
                    with self.assertRaises(FileExistsError):
                        module.export(root, output)
            self.assertEqual(output.read_bytes(), b'other process')
            self.assertEqual({path.name for path in root.iterdir()}, {'source.c', 'output.tar.gz'})

    def test_deterministic_archive_preserves_source_and_refuses_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'source.c').write_bytes(b'example source')
            digest = hashlib.sha256(b'example source').hexdigest()
            inputs = {'source.c': digest}
            canonical = json.dumps({'schema_version': 1, 'inputs': inputs}, sort_keys=True, separators=(',', ':')).encode()
            identity = hashlib.sha256(canonical).hexdigest()
            with patch.object(module.generation, 'generation', return_value=(identity, inputs)):
                first = module.export(root, root / 'one.tar.gz')
                second = module.export(root, root / 'two.tar.gz')
                self.assertEqual(first['sha256'], second['sha256'])
                self.assertFalse(first['corresponding_source_complete'])
                self.assertEqual(module.verify(root / 'one.tar.gz', identity)['generation'], identity)
                with self.assertRaisesRegex(ValueError, 'manifest'):
                    module.verify(root / 'one.tar.gz', 'f' * 64)
                with tarfile.open(root / 'one.tar.gz', 'r:gz') as archive:
                    self.assertEqual(archive.extractfile('inputs/source.c').read(), b'example source')
                (root / 'source.c').write_bytes(b'changed')
                with self.assertRaisesRegex(ValueError, 'changed while archiving'):
                    module.export(root, root / 'bad.tar.gz')
                self.assertFalse((root / 'bad.tar.gz').exists())


if __name__ == '__main__':
    unittest.main()
