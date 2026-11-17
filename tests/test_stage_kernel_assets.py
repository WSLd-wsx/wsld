import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('stage_kernel_assets', Path(__file__).resolve().parents[1] / 'scripts/stage-kernel-assets.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class StageKernelAssetsTests(unittest.TestCase):
    def test_accepts_packaged_release_candidate_version(self):
        self.assertIsNotNone(module.VERSION.fullmatch('6.18.60.0-rc2-1'))

    def test_rejects_invalid_kernel_version_before_staging(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'source'
            source.mkdir()
            (source / 'modules_not-a-version.tar.gz').write_bytes(b'fixture')
            with self.assertRaisesRegex(ValueError, 'Invalid kernel version'):
                module.stage(source, root / 'result')
            self.assertFalse((root / 'result').exists())

    def test_stages_verified_archive_and_rejects_mutation(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'source'
            source.mkdir()
            archive = source / 'modules_6.18.58.1-1.tar.gz'
            archive.write_bytes(b'verified fixture')
            metadata = {'schema_version': 1, 'kernel_version': '6.18.58.1-1', 'archive': archive.name,
                        'archive_bytes': archive.stat().st_size, 'archive_sha256': hashlib.sha256(archive.read_bytes()).hexdigest()}
            (source / 'modules_6.18.58.1-1-22-1.json').write_text(json.dumps(metadata))
            module.stage(source, root / 'result')
            index = json.loads((root / 'result/index.json').read_bytes())
            self.assertEqual(index['modules'][0]['sha256'], metadata['archive_sha256'])
            archive.write_bytes(b'modified fixture')
            with self.assertRaisesRegex(ValueError, 'metadata mismatch'):
                module.stage(source, root / 'rejected')
            self.assertFalse((root / 'rejected').exists())


if __name__ == '__main__':
    unittest.main()
