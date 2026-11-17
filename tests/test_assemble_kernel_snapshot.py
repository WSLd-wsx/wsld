import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/assemble-kernel-snapshot.py'
spec = importlib.util.spec_from_file_location('snapshot_assembler_test', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SnapshotTests(unittest.TestCase):
    def test_static_build_evidence_binds_modules_and_kernel_without_admission(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            build = root / 'build'
            (build / 'modules').mkdir(parents=True)
            entries, proofs = {}, {}
            for name in module.package.MODULES:
                file = build / 'modules' / name
                file.write_bytes(name.encode())
                digest = module.package.digest(file)
                entries[name] = {'sha256': digest, 'bytes': file.stat().st_size}
                proofs[name] = {'sha256': digest, 'missing': [], 'mismatches': [],
                                'has_module_layout': True, 'import_count': 1}
            generation = 'a' * 64
            result = {'schema_version': 1, 'build_generation': generation,
                      'kernel_source_commit': 'b' * 40,
                      'kernel_release': '6.18.54.1-microsoft-standard-WSL2',
                      'module_loaded': False, 'runtime_admitted': False, 'release_ready': False,
                      'kernel_symvers_evidence': {'kernel_build_id': 'c' * 40,
                                                 'kernel_image_sha256': 'd' * 64},
                      'modules': entries}
            (build / 'build-result.json').write_text(json.dumps(result))
            audit = {'schema_version': 1, 'kernel_build_id': 'c' * 40, 'crc_match': True,
                     'runtime_admitted': False, 'modules': proofs}
            audit_path = root / 'audit.json'
            audit_path.write_text(json.dumps(audit))
            with patch.object(module.generation_module, 'generation', return_value=(generation, {})):
                output = root / 'snapshot'
                report = module.assemble(build, audit_path, 'linux-msft-wsl-6.18.54.1', output)
                self.assertFalse(report['runtime_admitted'])
                self.assertFalse(report['release_ready'])
                module.package.validate(output)
                stage_spec = importlib.util.spec_from_file_location('snapshot_stage_test', SCRIPT.parent / 'kernel-staging-snapshots.py')
                stage = importlib.util.module_from_spec(stage_spec)
                stage_spec.loader.exec_module(stage)
                stage.merge_candidate(root, output)
                coverage_spec = importlib.util.spec_from_file_location('coverage_test', SCRIPT.parent / 'kernel-release-coverage.py')
                coverage = importlib.util.module_from_spec(coverage_spec)
                coverage_spec.loader.exec_module(coverage)
                self.assertTrue(coverage.diagnostic_built(root, 'linux-msft-wsl-6.18.54.1', 'b' * 40, generation))
                self.assertFalse(coverage.diagnostic_built(root, 'linux-msft-wsl-6.18.54.1', 'f' * 40, generation))
                self.assertFalse(coverage.covered(root, 'linux-msft-wsl-6.18.54.1', 'b' * 40, generation)[0])
                cache = coverage.DiagnosticSnapshotCache(root)
                try:
                    self.assertTrue(coverage.diagnostic_built(root, 'linux-msft-wsl-6.18.54.1', 'b' * 40, generation, cache))
                    self.assertTrue(coverage.diagnostic_built(root, 'linux-msft-wsl-6.18.54.1', 'b' * 40, generation, cache))
                    self.assertEqual(cache.counter, 1)
                    pointer = json.loads((root / 'staging-snapshots/current.json').read_text())
                    archive = root / 'staging-snapshots' / (pointer['sha256'] + '.tar.gz')
                    archive.write_bytes(b'tampered')
                    with self.assertRaisesRegex(ValueError, 'missing or changed'):
                        coverage.diagnostic_built(root, 'linux-msft-wsl-6.18.54.1', 'b' * 40, generation, cache)
                finally:
                    cache.close()
                audit['kernel_build_id'] = 'e' * 40
                audit_path.write_text(json.dumps(audit))
                with self.assertRaisesRegex(ValueError, 'CRC evidence'):
                    module.assemble(build, audit_path, 'linux-msft-wsl-6.18.54.1', root / 'bad')
                guest_proofs = {name: {'sha256': value['sha256'], 'unobserved': [],
                                       'mismatches': [], 'imports': 1, 'has_module_layout': True}
                                for name, value in entries.items()}
                guest_audit = {'schema_version': 1, 'kernel': result['kernel_release'],
                               'kernel_build_id': 'c' * 40, 'kernel_image_sha256': 'd' * 64,
                               'symbol_crc_compatibility_admitted': True, 'modules': guest_proofs}
                guest_proofs['virtio-gpu.ko']['has_module_layout'] = False
                audit_path.write_text(json.dumps(guest_audit))
                with self.assertRaisesRegex(ValueError, 'Module does not match'):
                    module.assemble(build, audit_path, 'linux-msft-wsl-6.18.54.1', root / 'bad-guest')


if __name__ == '__main__':
    unittest.main()
