import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/kernel_build_generation.py'
spec = importlib.util.spec_from_file_location('kernel_build_generation_test', SCRIPT)
generation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generation)


class GenerationTests(unittest.TestCase):
    def test_runner_dependency_change_changes_generation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'input').write_text('source')
            evidence = root / 'runner.json'
            value = {'schema_version': 1, 'packages': {name: '1.0' for name in generation.PACKAGES}}
            evidence.write_text(json.dumps(value))
            with patch.multiple(generation, FILES=('input',), TREES=()), \
                 patch.dict('os.environ', {'WSX_KERNEL_RUNNER_DEPENDENCIES': str(evidence)}):
                first = generation.generation(root)[0]
                value['packages']['pahole'] = '2.0'
                evidence.write_text(json.dumps(value))
                self.assertNotEqual(first, generation.generation(root)[0])
                del value['packages']['pahole']
                evidence.write_text(json.dumps(value))
                with self.assertRaisesRegex(ValueError, 'dependency evidence'):
                    generation.generation(root)

    def test_pipeline_assembly_inputs_are_part_of_generation(self):
        for path in ('scripts/assemble-kernel-snapshot.py', 'scripts/kernel-package.py',
                     'scripts/kernel-staging-snapshots.py', '.github/workflows/kernel-modules-build.yml',
                     'scripts/kernel-release-coverage.py', 'scripts/bootstrap-kernel-snapshot.py',
                     'scripts/kernel-package-registry.py'):
            self.assertIn(path, generation.FILES)
        current, inputs = generation.generation(generation.ROOT)
        self.assertEqual(len(current), 64)
        self.assertTrue(all(path in inputs for path in generation.FILES))

    def test_source_and_dependency_changes_each_change_generation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'sources').mkdir()
            (root / 'sources/module.c').write_text('source one')
            (root / 'toolchain.lock').write_text('toolchain one')
            with patch.multiple(generation, FILES=('toolchain.lock',), TREES=('sources',)):
                first = generation.generation(root)[0]
                self.assertEqual(first, generation.generation(root)[0])
                (root / 'sources/module.c').write_text('source two')
                second = generation.generation(root)[0]
                self.assertNotEqual(first, second)
                (root / 'toolchain.lock').write_text('toolchain two')
                self.assertNotEqual(second, generation.generation(root)[0])


if __name__ == '__main__':
    unittest.main()
