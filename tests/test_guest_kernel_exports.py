import runpy
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
exports = runpy.run_path(str(ROOT / 'core/guest/kernel-exports.py'))
crc_compatible = exports['crc_compatible']


class GuestKernelExportsTests(unittest.TestCase):
    def test_module_set_rejects_wrong_name_and_symlink(self):
        names = ['drm_kms_helper.ko', 'virtio_dma_buf.ko', 'virtio-gpu.ko',
                 'virtio_msg.ko', 'virtio_msg_vsock.ko', 'wsx_virtio_msg_pci.ko']
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for name in names:
                (directory / name).write_bytes(b'module')
            self.assertEqual(len(exports['checked_module_files'](directory)), 6)
            (directory / names[0]).rename(directory / 'wrong.ko')
            with self.assertRaisesRegex(ValueError, 'exact six'):
                exports['checked_module_files'](directory)
            (directory / 'wrong.ko').rename(directory / names[0])
            (directory / names[0]).unlink()
            (directory / names[0]).symlink_to(directory / names[1])
            with self.assertRaisesRegex(ValueError, 'exact six'):
                exports['checked_module_files'](directory)

    def test_requires_versioned_layout_and_matching_symbols_for_every_module(self):
        records = {str(i): {'imports': 1, 'has_module_layout': True,
                            'unobserved': [], 'mismatches': []} for i in range(6)}
        self.assertTrue(crc_compatible(records))
        for change in ({'imports': 0}, {'has_module_layout': False},
                       {'unobserved': ['module_layout']}, {'mismatches': ['module_layout']}):
            broken = {key: dict(value) for key, value in records.items()}
            broken['0'].update(change)
            self.assertFalse(crc_compatible(broken))
        del records['0']
        self.assertFalse(crc_compatible(records))


if __name__ == '__main__':
    unittest.main()
