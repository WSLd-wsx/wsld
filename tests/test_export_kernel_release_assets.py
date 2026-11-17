import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from test_kernel_package import create, package

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/export-kernel-release-assets.py'
spec = importlib.util.spec_from_file_location('wsx_release_export_test', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ReleaseAssetTests(unittest.TestCase):
    def test_shared_input_archive_and_content_checked_module_group(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            input_root = root / 'inputs'
            input_root.mkdir()
            (input_root / 'source').write_bytes(b'source')
            inputs = {'source': hashlib.sha256(b'source').hexdigest()}
            identity = hashlib.sha256(json.dumps({'schema_version': 1, 'inputs': inputs}, sort_keys=True,
                                                  separators=(',', ':')).encode()).hexdigest()
            input_archive = root / 'inputs.tar.gz'
            with patch.object(module.groups.build_inputs_export.generation, 'generation', return_value=(identity, inputs)):
                module.groups.build_inputs_export.export(input_root, input_archive)
            create(root / 'one', 'linux-msft-wsl-6.18.58.1', schema=3, generation=identity)
            create(root / 'two', 'linux-msft-wsl-6.18.54.1', schema=3, generation=identity)
            package.merge(root / 'one', root / 'two', root / 'snapshot')
            result = module.export(root / 'snapshot', input_archive, root / 'release')
            self.assertEqual(len(result['targets']), 2)
            self.assertEqual(len(result['groups']), 1)
            self.assertFalse(result['build_inputs']['corresponding_source_complete'])
            self.assertFalse(result['release_ready'])
            self.assertEqual(module.verify(root / 'release', root / 'snapshot'), result)
            group = next(iter(result['groups'].values()))
            (root / 'release' / group['asset']).write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError, 'digest mismatch'):
                module.verify(root / 'release', root / 'snapshot')

    def test_rejects_unsafe_asset_names(self):
        with self.assertRaisesRegex(ValueError, 'Invalid kernel release index'):
            module.validate({'schema_version': 1})


if __name__ == '__main__':
    unittest.main()
