import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/build-kernel-crc-index.py'
spec = importlib.util.spec_from_file_location('crc_index_test', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class CrcIndexTests(unittest.TestCase):
    def test_fingerprint_uses_external_crcs_and_checks_internal_imports(self):
        def inspect(_tool, option, file):
            if option == '--show-exports':
                return {'internal': 9} if file.name == module.crc.MODULES[0] else {}
            return {'module_layout': 1, 'kernel_symbol': 2, 'internal': 9}
        with tempfile.TemporaryDirectory() as temporary, patch.object(module.crc, 'entries', side_effect=inspect):
            values = module.requirements(Path(temporary), 'modprobe')
        self.assertEqual(values, {'kernel_symbol': 2, 'module_layout': 1})
        self.assertEqual(module.fingerprint(values), module.fingerprint(dict(reversed(list(values.items())))))
        self.assertNotEqual(module.fingerprint(values), module.fingerprint({'kernel_symbol': 3, 'module_layout': 1}))

    def test_internal_crc_mismatch_fails_closed(self):
        def inspect(_tool, option, file):
            if option == '--show-exports':
                return {'internal': 9} if file.name == module.crc.MODULES[0] else {}
            return {'module_layout': 1, 'internal': 8}
        with tempfile.TemporaryDirectory() as temporary, patch.object(module.crc, 'entries', side_effect=inspect):
            with self.assertRaisesRegex(ValueError, 'Module export CRC mismatch'):
                module.requirements(Path(temporary), 'modprobe')


if __name__ == '__main__':
    unittest.main()
