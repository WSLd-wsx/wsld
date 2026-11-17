import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('audit', Path(__file__).parents[1] / 'guest' / 'kernel-symbol-audit.py')
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


class KernelAuditTests(unittest.TestCase):
    def test_installed_and_package_versions_are_not_conflated(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            installed = root / 'installed'
            candidate = root / 'candidate'
            installed.mkdir()
            candidate.mkdir()
            (installed / 'system.ko').touch()
            (candidate / 'a.ko').touch()
            (candidate / 'b.ko').touch()
            original = AUDIT.versions
            def fake(path, option='--show-modversions'):
                if path.name == 'system.ko':
                    return {'kernel_symbol': '0x1'}
                if path.name == 'a.ko' and option == '--show-exports':
                    return {'package_symbol': '0x2'}
                if path.name == 'b.ko' and option == '--show-modversions':
                    return {'kernel_symbol': '0x1', 'package_symbol': '0x2', 'unknown': '0x3'}
                return {}
            AUDIT.versions = fake
            try:
                result = AUDIT.audit(candidate, installed)['modules']['b.ko']
            finally:
                AUDIT.versions = original
            self.assertEqual(result['matched_installed_imports'], 1)
            self.assertEqual(result['matched_candidate_exports'], 1)
            self.assertEqual(result['unobserved'], ['unknown'])
            self.assertEqual(result['mismatches'], [])


if __name__ == '__main__':
    unittest.main()
