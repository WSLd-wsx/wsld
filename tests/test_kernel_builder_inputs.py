from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class InputTests(unittest.TestCase):
    def test_build_package_lock_rejected_before_preparation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            lock = root / 'packages.json'
            lock.write_text('{"packages": []}')
            output = root / 'environment'
            result = subprocess.run([sys.executable, str(ROOT / 'scripts/prepare-kernel-build-root.py'),
                                     '--lock', str(lock), '--output', str(output)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())

    def test_source_archive_rejected_before_tree_creation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / 'bad.tar.gz'
            archive.write_bytes(b'not the pinned source')
            output = root / 'kernel'
            result = subprocess.run([sys.executable, str(ROOT / 'scripts/prepare-wsl-kernel-source.py'),
                                     '--archive', str(archive), '--output', str(output)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())

    def test_compiler_archive_rejected_before_toolchain_creation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / 'bad.tar.xz'
            archive.write_bytes(b'not the pinned compiler')
            result = subprocess.run([sys.executable, str(ROOT / 'scripts/prepare-kernel-toolchain.py'),
                                     '--archive', str(archive), '--root', str(root)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / 'opt/wsx-gcc').exists())


if __name__ == '__main__':
    unittest.main()
