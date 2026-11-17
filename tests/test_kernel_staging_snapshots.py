import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from test_kernel_package import create

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/kernel-staging-snapshots.py'
spec = importlib.util.spec_from_file_location('kernel_staging_snapshots', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class StagingSnapshotTests(unittest.TestCase):
    def test_failed_pointer_replace_preserves_previous_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            create(root / 'one', 'linux-msft-wsl-6.18.58.1', schema=3)
            create(root / 'next', 'linux-msft-wsl-6.18.54.1', schema=3, generation='f' * 64)
            previous = module.merge_candidate(root, root / 'one')
            pointer = root / 'staging-snapshots/current.json'
            before = pointer.read_bytes()
            with patch.object(module.os, 'replace', side_effect=OSError('simulated interruption')):
                with self.assertRaisesRegex(OSError, 'simulated interruption'):
                    module.merge_candidate(root, root / 'next')
            self.assertEqual(pointer.read_bytes(), before)
            restored = module.extract_current(root, root / 'restored')
            self.assertEqual(restored['generation'], previous['generation'])
            self.assertIn('linux-msft-wsl-6.18.58.1', restored['targets'])

    def test_incomplete_staging_state_is_not_treated_as_empty(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            create(root / 'candidate', 'linux-msft-wsl-6.18.58.1', schema=3)
            store = root / 'staging-snapshots'
            store.mkdir()
            (store / 'incomplete.tar.gz').write_bytes(b'partial')
            with self.assertRaisesRegex(ValueError, 'incomplete prior state'):
                module.merge_candidate(root, root / 'candidate')
            self.assertFalse((store / 'current.json').exists())
            self.assertEqual((store / 'incomplete.tar.gz').read_bytes(), b'partial')

    def test_merge_generation_rollover_and_tamper_rejection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            create(root / 'one', 'linux-msft-wsl-6.18.58.1', schema=3)
            create(root / 'two', 'linux-msft-wsl-6.18.54.1', schema=3)
            first = module.merge_candidate(root, root / 'one')
            second = module.merge_candidate(root, root / 'two')
            store = root / 'staging-snapshots'
            merged = module.package.extract(store / (second['sha256'] + '.tar.gz'), root / 'merged')
            self.assertEqual(len(merged['targets']), 2)
            self.assertTrue((store / (first['sha256'] + '.tar.gz')).is_file())
            create(root / 'next', 'linux-msft-wsl-6.18.58.1', schema=3, generation='f' * 64)
            next_generation = module.merge_candidate(root, root / 'next')
            self.assertEqual(next_generation['generation'], 'f' * 64)
            self.assertTrue((store / (second['sha256'] + '.tar.gz')).is_file())
            current = store / (next_generation['sha256'] + '.tar.gz')
            current.write_bytes(b'tampered')
            before = (store / 'current.json').read_bytes()
            with self.assertRaisesRegex(ValueError, 'missing or changed'):
                module.merge_candidate(root, root / 'next')
            self.assertEqual((store / 'current.json').read_bytes(), before)
            self.assertFalse(json.loads(before).get('release_ready', False))


if __name__ == '__main__':
    unittest.main()
