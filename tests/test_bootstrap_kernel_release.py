import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from test_kernel_package import create

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/bootstrap-kernel-release.py'
spec = importlib.util.spec_from_file_location('wsx_release_bootstrap_test', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class Opener:
    def __init__(self, directory):
        self.directory = directory

    def open(self, request, timeout):
        return io.BytesIO((self.directory / request.full_url.rsplit('/', 1)[1]).read_bytes())


class BootstrapTests(unittest.TestCase):
    def test_restores_verified_release_and_refuses_tampered_group(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs = root / 'inputs'
            inputs.mkdir()
            (inputs / 'source').write_bytes(b'source')
            records = {'source': hashlib.sha256(b'source').hexdigest()}
            generation = hashlib.sha256(json.dumps({'schema_version': 1, 'inputs': records}, sort_keys=True,
                                                    separators=(',', ':')).encode()).hexdigest()
            archive = root / 'inputs.tar.gz'
            exporter = module.download.assets
            with patch.object(exporter.groups.build_inputs_export.generation, 'generation', return_value=(generation, records)):
                exporter.groups.build_inputs_export.export(inputs, archive)
            create(root / 'snapshot', 'linux-msft-wsl-6.18.58.1', schema=3, generation=generation)
            index = exporter.export(root / 'snapshot', archive, root / 'release')
            target = root / 'target'
            target.mkdir()
            result = module.restore(target, Opener(root / 'release'))
            self.assertEqual(result['generation'], generation)
            self.assertFalse(result['release_ready'])
            self.assertTrue((target / 'staging-snapshots/current.json').is_file())
            group = next(iter(index['groups'].values()))
            (root / 'release' / group['asset']).write_bytes(b'tampered')
            other = root / 'other'
            other.mkdir()
            with self.assertRaisesRegex(ValueError, 'digest mismatch'):
                module.restore(other, Opener(root / 'release'))
            self.assertFalse((other / 'staging-snapshots/current.json').exists())


if __name__ == '__main__':
    unittest.main()
