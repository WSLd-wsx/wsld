# Original WSX code, BUSL-1.1. See LICENSING.md.
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('package_adapter', Path(__file__).resolve().parents[1] / 'scripts/package-guest-adapter.py')
package = importlib.util.module_from_spec(spec); spec.loader.exec_module(package)

class PackageTests(unittest.TestCase):
    def test_packages_reviewed_files_and_rejects_symlinks(self):
        with tempfile.TemporaryDirectory() as tmp:
            source=Path(tmp)/'source'; source.mkdir()
            for name in package.FILES: (source/name).write_text('fixture '+name)
            output=Path(tmp)/'output'
            digest=package.package(source,output,'hyprland')
            raw=(output/'manifest.json').read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(),digest)
            manifest=json.loads(raw)
            self.assertEqual(set(manifest['files']),set(package.FILES))
            self.assertEqual(manifest['protocol_major'],2)
            (source/'desktop.py').unlink(); (source/'desktop.py').symlink_to(source/'display.py')
            with self.assertRaises(OSError): package.package(source,Path(tmp)/'bad','hyprland')
            self.assertFalse((Path(tmp)/'bad').exists())

if __name__ == '__main__': unittest.main()
