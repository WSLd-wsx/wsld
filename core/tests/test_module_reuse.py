import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('module_reuse', Path(__file__).parents[1] / 'guest/module-reuse.py')
REUSE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REUSE)
IDENTITY = '11111111-2222-3333-4444-555555555555'


def elf(note):
    names = b'\0.shstrtab\0.note.gnu.build-id\0'
    data = bytearray(64 + 3 * 64)
    data[:6] = b'\x7fELF\x02\x01'
    struct.pack_into('<H', data, 18, 62)
    struct.pack_into('<Q', data, 40, 64)
    struct.pack_into('<HHH', data, 58, 64, 3, 1)
    for i, name, chunk in [(1, 1, names), (2, 11, note)]:
        struct.pack_into('<IIQQQQIIQQ', data, 64 + i * 64, name, 1, 0, 0, len(data), len(chunk), 0, 0, 4, 0)
        data.extend(chunk)
    return bytes(data)


class ModuleReuseTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.sys = self.root / 'sys'
        self.run = self.root / 'run'
        self.proc = self.root / 'proc'
        for path in ('bus/pci/devices', 'bus/pci/drivers/wsx-virtio-msg-pci'):
            (self.sys / path).mkdir(parents=True)
        (self.root / 'guest').mkdir()
        (self.run / 'wsx').mkdir(parents=True)
        self.proc.mkdir()
        self.note = struct.pack('<III', 4, 4, 3) + b'GNU\0test'
        for name, file in REUSE.MODULES:
            (self.root / 'guest' / file).write_bytes(elf(self.note))
            directory = self.sys / 'module' / name
            (directory / 'notes').mkdir(parents=True)
            (directory / 'notes/.note.gnu.build-id').write_bytes(self.note)
            (directory / 'initstate').write_text('live')
        self.owner = self.sys / 'module/wsx_virtio_msg_pci'
        (self.owner / 'refcnt').write_text('0')
        (self.owner / 'holders').mkdir()
        (self.owner / 'parameters').mkdir()
        for name, value in dict(host_instance=IDENTITY, pci_bdf='2222:00:00.0', service_port='23000', aperture_bar='0', aperture_size='1073741824', device_number='1').items():
            (self.owner / 'parameters' / name).write_text(value)
        self.previous = {'native_retirement_confirmed': True, 'native': {'instance': '{'+IDENTITY+'}'},
                         'prepared': {'instance': IDENTITY, 'device': '2222:00:00.0', 'access': {'nodes': ['/dev/dri/card1'], 'user': 'test'}},
                         'profile': {'port': 23000, 'aperture_size': 1073741824, 'device_number': 1}}
        for key, value in [('SYS', self.sys), ('RUN', self.run), ('PROC', self.proc)]:
            patcher = patch.object(REUSE, key, value)
            patcher.start()
            self.addCleanup(patcher.stop)

    def test_exact_retired_modules_require_explicit_reload_permission(self):
        result = REUSE.check(self.root, self.previous)
        self.assertEqual(len(result['loaded']), 6)
        self.assertTrue(result['reload_required'])
        with patch.object(REUSE.subprocess, 'run') as command:
            with self.assertRaisesRegex(RuntimeError, 'authorization'):
                REUSE.recycle(self.root, self.previous, False)
            command.assert_not_called()

    def test_mismatched_owner_or_loaded_bytes_never_reuse(self):
        (self.owner / 'parameters/host_instance').write_text('22222222-2222-3333-4444-555555555555')
        with self.assertRaisesRegex(RuntimeError, 'parameters differ'):
            REUSE.check(self.root, self.previous)
        (self.owner / 'notes/.note.gnu.build-id').write_bytes(b'other binary')
        with self.assertRaisesRegex(RuntimeError, 'differs from the pinned'):
            REUSE.check(self.root, self.previous)

    def test_busy_reference_and_bound_driver_refuse_removal(self):
        with patch.object(REUSE.subprocess, 'run') as command:
            (self.owner / 'refcnt').write_text('1')
            with self.assertRaisesRegex(RuntimeError, 'references'):
                REUSE.recycle(self.root, self.previous, True)
            (self.owner / 'refcnt').write_text('0')
            (self.sys / 'bus/pci/drivers/wsx-virtio-msg-pci/2222:00:00.0').symlink_to(self.owner)
            with self.assertRaisesRegex(RuntimeError, 'bound PCI owner'):
                REUSE.recycle(self.root, self.previous, True)
            command.assert_not_called()

    def test_surviving_session_and_foreign_descriptor_refuse_reuse(self):
        process = self.proc / '1234'
        process.mkdir()
        (process / 'environ').write_bytes(('WSX_DESKTOP_INSTANCE='+IDENTITY).encode()+b'\0')
        with self.assertRaisesRegex(RuntimeError, 'session process'):
            REUSE.check(self.root, self.previous)
        (process / 'environ').write_bytes(b'')
        (self.run / 'wsx/foreign.json').write_text('{}')
        with self.assertRaisesRegex(RuntimeError, 'Foreign Guest descriptor'):
            REUSE.check(self.root, self.previous)

    def test_kernel_busy_race_preserves_failure_and_uses_no_force(self):
        import subprocess
        with patch.object(REUSE.subprocess, 'run', side_effect=subprocess.CalledProcessError(1, ['rmmod'])) as command:
            with self.assertRaises(subprocess.CalledProcessError):
                REUSE.recycle(self.root, self.previous, True)
            self.assertEqual(command.call_args.args[0], ['/usr/bin/rmmod', 'wsx_virtio_msg_pci'])
            self.assertNotIn('timeout', command.call_args.kwargs)
            self.assertTrue(self.owner.exists())

    def test_successful_recycle_removes_only_the_empty_owner(self):
        import shutil
        def remove_owner(argv, **kwargs):
            self.assertEqual(argv, ['/usr/bin/rmmod', 'wsx_virtio_msg_pci'])
            shutil.rmtree(self.owner)
        with patch.object(REUSE.subprocess, 'run', side_effect=remove_owner):
            result = REUSE.recycle(self.root, self.previous, True)
        self.assertTrue(result['owner_module_removed'])
        self.assertFalse(result['other_modules_unloaded'])
        self.assertEqual(len(REUSE.inspect_loaded(self.root)), 5)

    def test_retirement_and_identity_are_required_before_reuse(self):
        with self.assertRaisesRegex(RuntimeError, 'no admitted previous lifetime'):
            REUSE.check(self.root, None)
        self.previous['native_retirement_confirmed'] = False
        with self.assertRaisesRegex(RuntimeError, 'Verified prior native retirement'):
            REUSE.check(self.root, self.previous)


if __name__ == '__main__':
    unittest.main()
