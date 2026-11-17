import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/kernel-module-load-requirements.py'
spec = importlib.util.spec_from_file_location('kernel_module_load_requirements', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class RequirementsTests(unittest.TestCase):
    def test_dependency_order_external_and_cycle(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in module.MODULES:
                (root / name).write_bytes(name.encode())
            dependencies = {'virtio-gpu.ko': 'drm_kms_helper,external_driver',
                            'wsx_virtio_msg_pci.ko': 'virtio_msg_vsock,virtio_msg',
                            'virtio_msg_vsock.ko': 'virtio_msg'}

            def fake_metadata(_program, path, field):
                if field == 'depends':
                    return dependencies.get(path.name, '')
                return {'vermagic': 'test SMP modversions', 'license': 'GPL'}.get(field, '')

            with patch.object(module, 'metadata', fake_metadata):
                result = module.inspect(root, 'modinfo')
                order = result['internal_load_order']
                self.assertLess(order.index('virtio_msg'), order.index('virtio_msg_vsock'))
                self.assertLess(order.index('virtio_msg_vsock'), order.index('wsx_virtio_msg_pci'))
                self.assertEqual(result['external_dependencies'], ['external_driver'])
                self.assertFalse(result['runtime_admitted'])
                dependencies['virtio_msg.ko'] = 'wsx_virtio_msg_pci'
                with self.assertRaisesRegex(ValueError, 'cycle'):
                    module.inspect(root, 'modinfo')
                dependencies['virtio_msg.ko'] = '../bad'
                with self.assertRaisesRegex(ValueError, 'dependency'):
                    module.inspect(root, 'modinfo')


if __name__ == '__main__':
    unittest.main()
