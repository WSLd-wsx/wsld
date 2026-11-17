import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/finalize-kernel-build.py'
spec = importlib.util.spec_from_file_location('finalize_kernel_build', SCRIPT)
finalizer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(finalizer)


def fixture(root):
    kernel = root / 'kernel'
    kernel.mkdir()
    (kernel / '.config').write_bytes(b'config')
    (kernel / 'Module.symvers').write_bytes(b'symvers')
    build = root / 'modules-build'
    for name in ('modules', 'graphics-module', 'owner-module'):
        (build / name).mkdir(parents=True)
    (root / 'environment').mkdir()
    (root / 'downloads').mkdir()
    (build / 'build.log').write_text('build output')
    result = {'schema_version': 1, 'module_loaded': False, 'release_ready': False,
              'kernel_config_sha256': finalizer.sha(kernel / '.config'),
              'kernel_symvers_sha256': finalizer.sha(kernel / 'Module.symvers'), 'modules': {}}
    for name in finalizer.MODULES:
        path = build / 'modules' / name
        path.write_bytes(name.encode())
        result['modules'][name] = {'sha256': finalizer.sha(path), 'bytes': path.stat().st_size}
    (build / 'build-result.json').write_text(json.dumps(result))


class FinalizeTests(unittest.TestCase):
    def test_successful_diagnostic_removes_per_target_toolchain_copy(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fixture(root)
            result_file = root / 'modules-build/build-result.json'
            result = json.loads(result_file.read_text())
            result['diagnostic_target'] = {'tag': 'test'}
            result['kernel_source_commit'] = 'c' * 40
            result['kernel_symvers_evidence'] = {'kernel_build_id': 'abc', 'kernel_image_sha256': 'a' * 64,
                                                 'symvers_sha256': result['kernel_symvers_sha256']}
            result_file.write_text(json.dumps(result))
            (root / 'kernel/wsx-symvers-evidence.json').write_text(json.dumps(result['kernel_symvers_evidence']))
            (root / 'kernel/wsx-target-prepared.json').write_text(json.dumps(
                {'tag': 'test', 'prepared_config_sha256': result['kernel_config_sha256']}))
            (root / 'kernel/wsx-source-evidence.json').write_text(json.dumps({'source_commit': 'c' * 40}))
            (root / 'target-inspection').mkdir()
            image = root / 'target-inspection/kernel'
            image.write_bytes(b'kernel image')
            result['kernel_symvers_evidence']['kernel_image_sha256'] = finalizer.sha(image)
            result_file.write_text(json.dumps(result))
            (root / 'kernel/wsx-symvers-evidence.json').write_text(json.dumps(result['kernel_symvers_evidence']))
            (root / 'target-inspection/kernel-exports.json').write_text(json.dumps(
                {'kernel_build_id': 'abc', 'kernel_image_sha256': finalizer.sha(image)}))
            audit = {'schema_version': 1, 'kernel_build_id': 'abc', 'crc_match': True,
                     'modules': {name: {'sha256': item['sha256'], 'missing': [], 'mismatches': [],
                                        'has_module_layout': True, 'import_count': 1}
                                 for name, item in result['modules'].items()}}
            (root / 'static-crc-audit.json').write_text(json.dumps(audit))
            report = finalizer.finalize(root)
            self.assertIn('environment', report['removed'])
            self.assertFalse((root / 'environment').exists())
            self.assertFalse(image.exists())
            self.assertIn('target-inspection/kernel', report['removed'])
            self.assertTrue((root / 'modules-build/modules').is_dir())
            self.assertTrue((root / 'static-crc-audit.json').is_file())

    def test_changed_extracted_kernel_preserves_intermediates(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fixture(root)
            result_file = root / 'modules-build/build-result.json'
            result = json.loads(result_file.read_text())
            result['diagnostic_target'] = {'tag': 'test'}
            result['kernel_source_commit'] = 'c' * 40
            result['kernel_symvers_evidence'] = {'kernel_build_id': 'abc', 'kernel_image_sha256': 'a' * 64,
                                                 'symvers_sha256': result['kernel_symvers_sha256']}
            result_file.write_text(json.dumps(result))
            (root / 'kernel/wsx-symvers-evidence.json').write_text(json.dumps(result['kernel_symvers_evidence']))
            (root / 'kernel/wsx-target-prepared.json').write_text(json.dumps(
                {'tag': 'test', 'prepared_config_sha256': result['kernel_config_sha256']}))
            (root / 'kernel/wsx-source-evidence.json').write_text(json.dumps({'source_commit': 'c' * 40}))
            (root / 'target-inspection').mkdir()
            (root / 'target-inspection/kernel').write_bytes(b'changed kernel')
            (root / 'target-inspection/kernel-exports.json').write_text(json.dumps(
                {'kernel_build_id': 'abc', 'kernel_image_sha256': 'a' * 64}))
            audit = {'schema_version': 1, 'kernel_build_id': 'abc', 'crc_match': True,
                     'modules': {name: {'sha256': item['sha256'], 'missing': [], 'mismatches': [],
                                        'has_module_layout': True, 'import_count': 1}
                                 for name, item in result['modules'].items()}}
            (root / 'static-crc-audit.json').write_text(json.dumps(audit))
            with self.assertRaisesRegex(ValueError, 'kernel image changed'):
                finalizer.finalize(root)
            self.assertTrue((root / 'kernel').is_dir())
            self.assertTrue((root / 'target-inspection/kernel').is_file())
            self.assertFalse((root / 'cleanup-result.json').exists())

    def test_diagnostic_crc_failure_retains_kernel_tree(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fixture(root)
            result_file = root / 'modules-build/build-result.json'
            result = json.loads(result_file.read_text())
            result['diagnostic_target'] = {'tag': 'test'}
            result_file.write_text(json.dumps(result))
            with self.assertRaisesRegex(ValueError, 'CRC evidence is missing'):
                finalizer.finalize(root)
            (root / 'target-inspection').mkdir()
            result['kernel_symvers_evidence'] = {'kernel_build_id': 'abc', 'kernel_image_sha256': 'a' * 64,
                                                 'symvers_sha256': result['kernel_symvers_sha256']}
            result_file.write_text(json.dumps(result))
            (root / 'kernel/wsx-symvers-evidence.json').write_text(json.dumps(result['kernel_symvers_evidence']))
            (root / 'target-inspection/kernel-exports.json').write_text(json.dumps(
                {'kernel_build_id': 'abc', 'kernel_image_sha256': 'a' * 64}))
            audit = {'schema_version': 1, 'kernel_build_id': 'abc', 'crc_match': True,
                     'modules': {name: {'sha256': entry['sha256'], 'missing': [],
                                        'mismatches': [], 'import_count': 1, 'has_module_layout': True}
                                 for name, entry in result['modules'].items()}}
            (root / 'static-crc-audit.json').write_text(json.dumps(audit))
            finalizer.verify_static_crc(root, result)
            result['kernel_symvers_evidence']['kernel_image_sha256'] = 'b' * 64
            with self.assertRaisesRegex(ValueError, 'different kernel image'):
                finalizer.verify_static_crc(root, result)
            result['kernel_symvers_evidence']['kernel_image_sha256'] = 'a' * 64
            audit['modules']['virtio-gpu.ko']['mismatches'] = ['module_layout']
            (root / 'static-crc-audit.json').write_text(json.dumps(audit))
            with self.assertRaisesRegex(ValueError, 'does not match module'):
                finalizer.finalize(root)
            self.assertTrue((root / 'kernel').is_dir())
            self.assertTrue((root / 'modules-build/graphics-module').is_dir())

    def test_missing_log_or_existing_cleanup_report_retains_intermediates(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fixture(root)
            (root / 'modules-build/build.log').unlink()
            with self.assertRaisesRegex(ValueError, 'Build log'):
                finalizer.finalize(root)
            self.assertTrue((root / 'kernel').exists())
            (root / 'modules-build/build.log').write_text('build output')
            (root / 'cleanup-result.json').write_text('old evidence')
            with self.assertRaisesRegex(ValueError, 'already exists'):
                finalizer.finalize(root)
            self.assertEqual((root / 'cleanup-result.json').read_text(), 'old evidence')
            self.assertTrue((root / 'kernel').exists())

    def test_preserves_reusable_inputs_and_build_evidence(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fixture(root)
            finalizer.finalize(root)
            for path in ('environment', 'downloads', 'kernel-evidence/Module.symvers', 'modules-build/modules'):
                self.assertTrue((root / path).exists())
            for path in ('kernel', 'modules-build/graphics-module', 'modules-build/owner-module'):
                self.assertFalse((root / path).exists())

    def test_tampered_module_retains_all_intermediates(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fixture(root)
            (root / 'modules-build/modules/virtio-gpu.ko').write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError, 'Module output changed'):
                finalizer.finalize(root)
            self.assertTrue((root / 'kernel').exists())
            self.assertTrue((root / 'modules-build/graphics-module').exists())


if __name__ == '__main__':
    unittest.main()
