import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT/path)
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    return module
catalog = load('guest_catalog', 'scripts/create-guest-adapter-catalog.py')
package = load('guest_package', 'scripts/package-guest-adapter.py')


class GuestCatalogTests(unittest.TestCase):
    def test_catalog_binds_real_packaged_bytes_and_rejects_drift(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); source = root/'source'; source.mkdir()
            for name in package.FILES: (source/name).write_text('fixture ' + name)
            bundle = root/'bundle'
            expected = package.package(source, bundle, 'hyprland')
            self.assertEqual(catalog.catalog([('hyprland', bundle)])['adapters'][0]['manifest_sha256'], expected)
            with self.assertRaises(ValueError): catalog.catalog([('plasma', bundle)])
            with self.assertRaises(ValueError): catalog.catalog([('hyprland', bundle), ('hyprland', bundle)])
            (bundle/'desktop.py').write_text('changed')
            with self.assertRaises(ValueError): catalog.catalog([('hyprland', bundle)])


if __name__ == '__main__': unittest.main()
