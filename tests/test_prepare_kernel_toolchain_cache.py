import importlib.util
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/prepare-kernel-toolchain.py'
spec = importlib.util.spec_from_file_location('toolchain_cache', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ToolchainCacheTests(unittest.TestCase):
    def test_manifest_detects_changed_file_and_rejects_escaping_symlink(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / 'toolchain'
            target.mkdir()
            (target / 'gcc').write_bytes(b'compiler')
            (target / 'cc').symlink_to('gcc')
            before = module.tree_manifest(target)
            self.assertEqual(before['cc'], {'symlink': 'gcc'})
            (target / 'gcc').write_bytes(b'changed')
            self.assertNotEqual(before, module.tree_manifest(target))
            (root / 'outside').write_bytes(b'outside')
            (target / 'escape').symlink_to('../outside')
            with self.assertRaisesRegex(ValueError, 'escapes'):
                module.tree_manifest(target)


if __name__ == '__main__':
    unittest.main()
