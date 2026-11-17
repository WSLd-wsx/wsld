import importlib.util
from pathlib import Path
import struct
import unittest
import gzip

SPEC = importlib.util.spec_from_file_location('kernel_exports', Path(__file__).parents[1] / 'guest/kernel-exports.py')
KERNEL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(KERNEL)


def fixture():
    notes = struct.pack('<III', 4, 4, 3) + b'GNU\0' + b'test'
    names = ['', '.shstrtab', '.notes', '__ksymtab', '__kcrctab', '__ksymtab_gpl', '__kcrctab_gpl', '__ksymtab_strings']
    name_data = b'\0'.join(n.encode() for n in names) + b'\0'
    # One relative-address export paired with a CRC; GPL tables may be empty.
    chunks = [b'', name_data, notes, struct.pack('<iii', 0, 0x2000 - 0x1004, 0), struct.pack('<I', 0x12345678), b'', b'', b'module_layout\0']
    data = bytearray(64 + len(names) * 64)
    data[:6] = b'\x7fELF\x02\x01'
    struct.pack_into('<H', data, 18, 62)
    struct.pack_into('<Q', data, 40, 64)
    struct.pack_into('<HHH', data, 58, 64, len(names), 1)
    for i, (name, chunk) in enumerate(zip(names, chunks)):
        address = 0x1000 if name == '__ksymtab' else 0x2000 if name == '__ksymtab_strings' else 0
        struct.pack_into('<IIQQQQIIQQ', data, 64 + i * 64, name_data.index(name.encode()+b'\0'), 1, 0, address, len(data), len(chunk), 0, 0, 4, 0)
        data.extend(chunk)
    return bytes(data), notes


class KernelExportTests(unittest.TestCase):
    def test_relative_export_crc_and_build_identity(self):
        image, notes = fixture()
        values, identity, _ = KERNEL.exports(image, notes)
        self.assertEqual(values, {'module_layout': 0x12345678})
        self.assertEqual(identity, b'test'.hex())
        wrapped = b'boot prefix' + gzip.compress(image) + b'trailer'
        self.assertEqual(KERNEL.exports(wrapped, notes)[0], values)

    def test_other_running_kernel_is_rejected(self):
        image, notes = fixture()
        with self.assertRaisesRegex(ValueError, 'differs'):
            KERNEL.exports(image, notes[:-4] + b'nope')

    def test_truncated_export_data_is_rejected(self):
        image, notes = fixture()
        with self.assertRaises(ValueError):
            KERNEL.exports(image[:-2], notes)

    def test_wrong_elf_architecture_and_compression_are_rejected(self):
        image, notes = fixture()
        wrong = bytearray(image)
        struct.pack_into('<H', wrong, 18, 183)
        with self.assertRaisesRegex(ValueError, 'x86-64'):
            KERNEL.exports(wrong, notes)
        with self.assertRaisesRegex(ValueError, 'compression'):
            KERNEL.elf_image(b'not an image')


if __name__ == '__main__':
    unittest.main()
