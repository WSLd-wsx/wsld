import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/compare-kernel-module-crcs.py'
spec = importlib.util.spec_from_file_location('compare_kernel_module_crcs', SCRIPT)
matcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(matcher)


class CompareTests(unittest.TestCase):
    def test_records_module_symbol_provider(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in matcher.MODULES:
                (root / name).write_bytes(name.encode())
            def entries(_program, option, path):
                if option == '--show-exports':
                    return {'provided': 9} if path.name == 'virtio_msg.ko' else {}
                return {'provided': 9} if path.name == 'virtio_msg_vsock.ko' else {'kernel': 3}
            with patch.object(matcher, 'entries', entries):
                result = matcher.compare({'kernel_build_id': 'abc', 'symbols': {'kernel': 3}}, root, 'modprobe')
                self.assertTrue(result['crc_match'])
                self.assertEqual(result['modules']['virtio_msg_vsock.ko']['module_symbol_providers'], ['virtio_msg.ko'])

    def test_reuse_requires_every_import_and_crc(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in matcher.MODULES:
                (root / name).write_bytes(name.encode())
            def entries(_program, option, _path):
                return {'symbol': 123} if option == '--show-modversions' else {}
            with patch.object(matcher, 'entries', entries):
                kernel = {'kernel_build_id': 'abc', 'symbols': {'symbol': 123}}
                self.assertTrue(matcher.compare(kernel, root, 'modprobe')['crc_match'])
                kernel['symbols']['symbol'] = 124
                result = matcher.compare(kernel, root, 'modprobe')
                self.assertFalse(result['crc_match'])
                self.assertFalse(result['runtime_admitted'])
                kernel['symbols'] = {}
                self.assertFalse(matcher.compare(kernel, root, 'modprobe')['crc_match'])


if __name__ == '__main__':
    unittest.main()
