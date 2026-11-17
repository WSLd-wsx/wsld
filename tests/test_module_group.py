import hashlib
import json
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
module = runpy.run_path(str(ROOT / 'core/guest/module-group.py'))


class ModuleGroupTests(unittest.TestCase):
    def test_digest_and_loaded_identity_are_required(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            files = {}
            for name in module['NAMES']:
                data = b'wsx_owner_configuration_abi=2\0' if name.startswith('wsx_') else name.encode()
                (root / name).write_bytes(data)
                files[name] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
            manifest = json.dumps({'schema_version': 1, 'owner_configuration_abi': 2, 'files': files}).encode()
            (root / 'module-group.json').write_bytes(manifest)
            with patch('runpy.run_path', return_value={'build_note': lambda _path: b'note'}):
                group = module['verify'](root, hashlib.sha256(manifest).hexdigest(), 'unused')
                sysroot = root / 'sys'
                for name in module['NAMES']:
                    directory = sysroot / 'module' / name.removesuffix('.ko').replace('-', '_')
                    (directory / 'notes').mkdir(parents=True)
                    (directory / 'initstate').write_text('live')
                    (directory / 'notes/.note.gnu.build-id').write_bytes(b'note')
                abi = sysroot / 'bus/pci/drivers/wsx-virtio-msg-pci/configuration_abi'
                abi.parent.mkdir(parents=True)
                abi.write_text('2')
                self.assertTrue(module['verify_loaded'](group, sysroot)['loaded_group_verified'])
                (sysroot / 'module/virtio_msg/notes/.note.gnu.build-id').write_bytes(b'other')
                with self.assertRaisesRegex(RuntimeError, 'differs'):
                    module['verify_loaded'](group, sysroot)
                (root / 'virtio_msg.ko').write_bytes(b'tampered')
                with self.assertRaises(ValueError):
                    module['verify'](root, hashlib.sha256(manifest).hexdigest(), 'unused')


if __name__ == '__main__':
    unittest.main()
