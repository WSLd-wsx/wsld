import importlib.util
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/wsl-preview-kernel.py'
spec = importlib.util.spec_from_file_location('wsl_preview_kernel', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PreviewTests(unittest.TestCase):
    def test_selects_newest_numeric_public_preview(self):
        rows = [{'tag_name': tag, 'draft': draft, 'prerelease': pre} for tag, draft, pre in [
            ('2.9.9', False, True), ('2.9.10', False, True), ('3.1.0', True, True),
            ('3.0.0', False, False), ('unsafe/4.0.0', False, True)]]
        self.assertEqual(module.latest_preview(rows), '2.9.10')
        with self.assertRaises(ValueError):
            module.latest_preview([{'tag_name': '3.0.0', 'draft': False, 'prerelease': False}])

    def test_peels_exact_tag(self):
        refs = 'a' * 40 + '\trefs/tags/3.0.4\n' + 'b' * 40 + '\trefs/tags/3.0.4^{}\n'
        self.assertEqual(module.tag_commit(refs, '3.0.4'), 'b' * 40)
        with self.assertRaises(ValueError):
            module.tag_commit(refs, '3.0.5')

    def test_kernel_dependency_requires_one_well_formed_entry(self):
        self.assertEqual(module.kernel_dependency(b'<packages><package id="Microsoft.WSL.Kernel" version="6.18.54.1-1"/></packages>'), '6.18.54.1-1')
        for data in (b'<packages/>', b'<!DOCTYPE x><packages/>',
                     b'<packages><package id="Microsoft.WSL.Kernel" version="bad"/></packages>',
                     b'<packages><package id="Microsoft.WSL.Kernel" version="6.18.54.1-1"/><package id="Microsoft.WSL.Kernel" version="6.18.54.1-2"/></packages>'):
            with self.subTest(data=data), self.assertRaises(ValueError):
                module.kernel_dependency(data)


if __name__ == '__main__':
    unittest.main()
