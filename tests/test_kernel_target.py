import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/kernel_target.py'
spec = importlib.util.spec_from_file_location('kernel_target', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class KernelTargetTests(unittest.TestCase):
    def test_real_git_checkout_detects_tracked_source_changes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            (root / 'source.c').write_text('int value = 1;\n')
            subprocess.run(['git', '-C', str(root), 'add', 'source.c'], check=True)
            subprocess.run(['git', '-C', str(root), '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                            'commit', '-qm', 'fixture'], check=True)
            commit = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
            image = root / 'image'
            image.write_bytes(b'kernel')
            source = {'schema_version': 1, 'source_tag_verified': True, 'tag': 'linux-msft-wsl-6.18.54.1',
                      'source_commit': commit, 'config_sha256': 'b' * 64}
            inspection = {'schema_version': 1, 'package_version': '6.18.54.1-1',
                          'kernel_image_sha256': hashlib.sha256(b'kernel').hexdigest(), 'kernel_config_sha256': 'b' * 64}
            (root / 'wsx-source-evidence.json').write_text(json.dumps(source))
            info = root / 'inspection.json'
            info.write_text(json.dumps(inspection))
            self.assertEqual(module.identity(root, image, info)['source_commit'], commit)
            (root / 'source.c').write_text('int value = 2;\n')
            with self.assertRaisesRegex(ValueError, 'source tree differs'):
                module.identity(root, image, info)

    def test_source_config_and_image_must_match(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / 'image'
            image.write_bytes(b'kernel')
            image_hash = hashlib.sha256(b'kernel').hexdigest()
            source = {'schema_version': 1, 'source_tag_verified': True, 'tag': 'linux-msft-wsl-6.18.54.1',
                      'source_commit': 'a' * 40, 'config_sha256': 'b' * 64}
            inspection = {'schema_version': 1, 'package_version': '6.18.54.1-1',
                          'kernel_image_sha256': image_hash, 'kernel_config_sha256': 'b' * 64}
            (root / 'wsx-source-evidence.json').write_text(json.dumps(source))
            info = root / 'inspection.json'
            info.write_text(json.dumps(inspection))
            with patch.object(module.subprocess, 'check_output', side_effect=['a' * 40 + '\n', '']):
                result = module.identity(root, image, info)
            self.assertEqual(result['release'], '6.18.54.1-microsoft-standard-WSL2')
            self.assertFalse(result['source_correspondence_verified'])
            with patch.object(module.subprocess, 'check_output', side_effect=['a' * 40 + '\n', ' M include/header.h\n']):
                with self.assertRaisesRegex(ValueError, 'source tree differs'):
                    module.identity(root, image, info)
            inspection['kernel_config_sha256'] = 'c' * 64
            info.write_text(json.dumps(inspection))
            with self.assertRaisesRegex(ValueError, 'do not match'):
                module.identity(root, image, info)
            inspection['kernel_config_sha256'] = 'b' * 64
            info.write_text(json.dumps(inspection))
            image.write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError, 'do not match'):
                module.identity(root, image, info)


if __name__ == '__main__':
    unittest.main()
