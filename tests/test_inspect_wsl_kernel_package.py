import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/inspect-wsl-kernel-package.py'
spec = importlib.util.spec_from_file_location('inspect_wsl_kernel_package', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PackageInspectionTests(unittest.TestCase):
    def test_rejects_hash_and_version_then_records_non_admission(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / 'package.zip'
            with zipfile.ZipFile(archive, 'w') as package:
                package.writestr('Microsoft.WSL.Kernel.nuspec', '<package><metadata><id>Microsoft.WSL.Kernel</id><version>6.18.54.1-1</version></metadata></package>')
                package.writestr('build/native/bin/x64/kernel', b'kernel')
                package.writestr('build/native/bin/x64/kernel_config', b'config')
                package.writestr('build/native/bin/kernel_build_metadata.json', '{}')
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            with self.assertRaisesRegex(ValueError, 'digest mismatch'):
                module.inspect(archive, '0' * 64, '6.18.54.1-1', root / 'bad')
            with self.assertRaisesRegex(ValueError, 'metadata'):
                module.inspect(archive, digest, '6.18.58.1-1', root / 'bad')
            fake = {'embedded_notes': lambda _kernel: b'notes', 'exports': lambda _kernel, _notes: ({'x': 1}, 'id', 'elf')}
            with patch.object(module.runpy, 'run_path', return_value=fake):
                result = module.inspect(archive, digest, '6.18.54.1-1', root / 'ok')
            self.assertFalse(result['publisher_verified'])
            self.assertFalse(result['runtime_admitted'])
            self.assertTrue((root / 'ok/kernel-exports.json').is_file())


if __name__ == '__main__':
    unittest.main()
