import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/extract-reference-kernel-package.py'
spec = importlib.util.spec_from_file_location('extract_reference_kernel_package', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ExtractionTests(unittest.TestCase):
    def test_checks_package_and_member_bytes_before_writing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package, output = root / 'test.zip', root / 'output'
            with zipfile.ZipFile(package, 'w') as archive:
                archive.writestr('image', b'image')
                archive.writestr('config', b'config')
            files = {name: (name, hashlib.sha256(name.encode()).hexdigest(), 20)
                     for name in ('image', 'config')}
            package_hash = module.digest(package)
            with patch.object(module, 'FILES', files), patch.object(module, 'PACKAGE_SHA256', '0' * 64):
                with self.assertRaises(ValueError):
                    module.extract(package, output)
                self.assertFalse(output.exists())
            with patch.object(module, 'FILES', files), patch.object(module, 'PACKAGE_SHA256', package_hash):
                module.extract(package, output)
                self.assertEqual((output / 'image').read_bytes(), b'image')
                with self.assertRaises(ValueError):
                    module.extract(package, output)

    def test_duplicate_member_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / 'test.zip'
            with zipfile.ZipFile(package, 'w') as archive:
                archive.writestr('image', b'image')
                with self.assertWarns(UserWarning):
                    archive.writestr('image', b'image')
            files = {'image': ('image', hashlib.sha256(b'image').hexdigest(), 20)}
            with patch.object(module, 'FILES', files), patch.object(module, 'PACKAGE_SHA256', module.digest(package)):
                with self.assertRaises(ValueError):
                    module.extract(package, root / 'output')
                self.assertFalse((root / 'output').exists())


if __name__ == '__main__':
    unittest.main()
