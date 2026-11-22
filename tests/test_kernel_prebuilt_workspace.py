import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

script = Path(__file__).resolve().parents[1] / 'scripts/build-coverage-target.py'
spec = importlib.util.spec_from_file_location('build_coverage_target', script)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PrebuiltWorkspaceTests(unittest.TestCase):
    def test_reuse_requires_same_package_bytes_source_and_generation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / 'build'
            (workspace / 'modules-build').mkdir(parents=True)
            (workspace / 'target-inspection').mkdir()
            tag, commit, generation = 'linux-msft-wsl-6.18.54.1', '1'*40, '2'*64
            package = {'version': '6.18.54.1-1', 'sha256': '3'*64}
            (workspace / 'modules-build/build-result.json').write_text(json.dumps({
                'kernel_source_commit': commit, 'build_generation': generation,
            }))
            (workspace / 'target-inspection/inspection.json').write_text(json.dumps({
                'package_version': package['version'], 'package_sha256': package['sha256'],
            }))
            candidate = root / 'cached'
            module.save_prebuilt(candidate, workspace, tag, commit, package, generation)
            destination = root / 'restored'
            self.assertTrue(module.restore_prebuilt(candidate, destination, tag, commit, package, generation))
            self.assertTrue((destination / 'modules-build/build-result.json').is_file())
            for altered in ((tag, '0'*40, package, generation),
                            (tag, commit, dict(package, sha256='0'*64), generation),
                            (tag, commit, package, '0'*64)):
                rejected = root / 'rejected'
                self.assertFalse(module.restore_prebuilt(candidate, rejected, *altered))
                self.assertFalse(rejected.exists())
