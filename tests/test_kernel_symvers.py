import importlib.util
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/kernel_symvers.py'
spec = importlib.util.spec_from_file_location('kernel_symvers_test', SCRIPT)
symvers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(symvers)


class SymversTests(unittest.TestCase):
    def test_preserves_crc_export_type_and_namespace(self):
        values = {'z': {'crc': 2, 'export': 'EXPORT_SYMBOL_GPL', 'namespace': 'EXAMPLE'},
                  'a': {'crc': 1, 'export': 'EXPORT_SYMBOL', 'namespace': ''}}
        self.assertEqual(symvers.format_symvers(values),
                         b'0x00000001\ta\tvmlinux\tEXPORT_SYMBOL\t\n'
                         b'0x00000002\tz\tvmlinux\tEXPORT_SYMBOL_GPL\tEXAMPLE\n')

    def test_rejects_empty_or_unsafe_symbol_metadata(self):
        with self.assertRaises(ValueError):
            symvers.format_symvers({})
        with self.assertRaises(ValueError):
            symvers.format_symvers({'bad\nname': {'crc': 1, 'export': 'EXPORT_SYMBOL', 'namespace': ''}})
        with self.assertRaises(ValueError):
            symvers.format_symvers({'x': {'crc': 1, 'export': 'UNKNOWN', 'namespace': ''}})


if __name__ == '__main__':
    unittest.main()
