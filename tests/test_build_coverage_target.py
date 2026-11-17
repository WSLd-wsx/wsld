import importlib.util
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from test_kernel_package import create

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/build-coverage-target.py'
spec = importlib.util.spec_from_file_location('build_coverage_target', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class CoverageTargetTests(unittest.TestCase):
    def test_rc_target_reports_missing_package_mapping(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, 'RC target discovered.*package mapping'):
                module.build('linux-msft-wsl-6.19.0-rc2', 'a' * 40, Path(temporary))

    def test_crc_candidate_without_vermagic_runs_builder(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tag = 'linux-msft-wsl-6.18.54.1'
            (root / 'target-inputs').mkdir()
            (root / 'package.nupkg').write_bytes(b'input')
            (root / 'target-inputs' / (tag + '.json')).write_text(json.dumps({
                'version': '6.18.54.1-1', 'sha256': hashlib.sha256(b'input').hexdigest(), 'package': 'package.nupkg'}))
            generation_spec = importlib.util.spec_from_file_location('generation_fixture', module.ROOT / 'scripts/kernel_build_generation.py')
            generation_module = importlib.util.module_from_spec(generation_spec)
            generation_spec.loader.exec_module(generation_module)
            generation, _ = generation_module.generation(module.ROOT)
            candidate = root / 'candidate'
            create(candidate, 'linux-msft-wsl-6.18.58.1', generation=generation, schema=3)
            stage_spec = importlib.util.spec_from_file_location('stage_fixture', module.ROOT / 'scripts/kernel-staging-snapshots.py')
            stage = importlib.util.module_from_spec(stage_spec)
            stage_spec.loader.exec_module(stage)
            stage.merge_candidate(root, candidate)

            real_spec = importlib.util.spec_from_file_location
            class ScreenLoader:
                def create_module(self, spec):
                    return None
                def exec_module(self, target):
                    target.crc_prescreen = lambda *args: {'candidates': ['a' * 64]}

            def fixture_spec(name, path):
                if name == 'screen_kernel':
                    return importlib.util.spec_from_loader(name, ScreenLoader())
                return real_spec(name, path)

            with patch.object(module.importlib.util, 'spec_from_file_location', side_effect=fixture_spec), \
                 patch.object(module.shutil, 'which', return_value='/usr/sbin/modprobe'), \
                 patch.object(module.subprocess, 'run', return_value=subprocess.CompletedProcess([], 7)) as run:
                with self.assertRaisesRegex(ValueError, 'exit code 7'):
                    module.build(tag, 'b' * 40, root)
                self.assertEqual(run.call_count, 1)
                self.assertIn('build-target-kernel-modules.sh', run.call_args.args[0][1])

    def test_missing_vermagic_requires_build_and_unsafe_evidence_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence = root / 'proof.json'
            self.assertFalse(module.has_vermagic_evidence(evidence))
            evidence.write_text('{}')
            self.assertTrue(module.has_vermagic_evidence(evidence))
            evidence.unlink()
            evidence.symlink_to(root / 'missing')
            with self.assertRaisesRegex(ValueError, 'Unsafe'):
                module.has_vermagic_evidence(evidence)

    def test_existing_same_generation_rejects_unverified_kernel_before_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tag = 'linux-msft-wsl-6.18.54.1'
            (root / 'target-inputs').mkdir()
            (root / 'package.nupkg').write_bytes(b'input')
            value = {'version': '6.18.54.1-1', 'sha256': 'a' * 64, 'package': 'package.nupkg'}
            (root / 'target-inputs' / (tag + '.json')).write_text(json.dumps(value))
            generation_spec = importlib.util.spec_from_file_location('generation_test', module.ROOT / 'scripts/kernel_build_generation.py')
            generation_module = importlib.util.module_from_spec(generation_spec)
            generation_spec.loader.exec_module(generation_module)
            generation, _ = generation_module.generation(module.ROOT)
            candidate = root / 'candidate'
            create(candidate, 'linux-msft-wsl-6.18.58.1', generation=generation, schema=3)
            stage_spec = importlib.util.spec_from_file_location('staging_test', module.ROOT / 'scripts/kernel-staging-snapshots.py')
            stage = importlib.util.module_from_spec(stage_spec)
            stage_spec.loader.exec_module(stage)
            stage.merge_candidate(root, candidate)
            with patch.object(module.subprocess, 'run') as run:
                with self.assertRaisesRegex(ValueError, 'digest mismatch'):
                    module.build(tag, 'b' * 40, root)
                run.assert_not_called()

    def test_failed_build_retains_attempt_and_never_writes_success(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tag = 'linux-msft-wsl-6.18.54.1'
            (root / 'target-inputs').mkdir()
            (root / 'package.nupkg').write_bytes(b'input')
            config = {'version': '6.18.54.1-1', 'sha256': hashlib.sha256(b'input').hexdigest(), 'package': 'package.nupkg'}
            (root / 'target-inputs' / (tag + '.json')).write_text(json.dumps(config))
            with patch.object(module.subprocess, 'run', return_value=subprocess.CompletedProcess([], 7)):
                with self.assertRaisesRegex(ValueError, 'exit code 7'):
                    module.build(tag, 'b' * 40, root)
            attempts = list((root / 'diagnostic-builds' / tag).iterdir())
            self.assertEqual(len(attempts), 1)
            self.assertFalse((attempts[0] / 'diagnostic-result.json').exists())

    def test_target_input_cannot_escape_or_change_version(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'target-inputs').mkdir()
            tag = 'linux-msft-wsl-6.18.54.1'
            path = root / 'target-inputs' / (tag + '.json')
            path.write_text(json.dumps({'version': '6.18.54.1-1', 'sha256': 'a' * 64, 'package': '../outside'}))
            with self.assertRaisesRegex(ValueError, 'Unsafe'):
                module.target_input(root, tag)
            path.write_text(json.dumps({'version': '6.18.58.1-1', 'sha256': 'a' * 64, 'package': 'image'}))
            with self.assertRaisesRegex(ValueError, 'version'):
                module.target_input(root, tag)


if __name__ == '__main__':
    unittest.main()
