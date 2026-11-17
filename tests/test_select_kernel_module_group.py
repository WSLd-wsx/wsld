import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
import subprocess

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/select-kernel-module-group.py'
spec = importlib.util.spec_from_file_location('select_kernel_module_group', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SelectTests(unittest.TestCase):
    def test_failed_inspection_is_not_reported_as_complete(self):
        index = {'generation': 'current', 'groups': {'group-a': {}, 'group-b': {}}}
        def evaluate(_exports, path, *_args):
            if path.name == 'group-a':
                raise subprocess.CalledProcessError(1, 'modinfo')
            return {'static_candidate': True, 'reasons': []}
        with patch.object(module.package, 'validate', return_value=index), patch.object(module.screen, 'evaluate', side_effect=evaluate):
            result = module.select(Path('.'), 'current', {'kernel_build_id': 'id'}, 'magic', 'modprobe', 'modinfo')
            self.assertFalse(result['inspection_complete'])
            self.assertTrue(result['groups'][0]['inspection_failed'])
            self.assertEqual(result['static_candidates'], ['group-b'])
            self.assertFalse(result['runtime_admitted'])

    def test_generation_must_match_and_only_candidates_are_selected(self):
        index = {'generation': 'current', 'groups': {'group-a': {}, 'group-b': {}}}
        def evaluate(_exports, path, *_args):
            return {'static_candidate': path.name == 'group-b', 'reasons': []}
        with patch.object(module.package, 'validate', return_value=index), patch.object(module.screen, 'evaluate', side_effect=evaluate):
            with self.assertRaisesRegex(ValueError, 'generation'):
                module.select(Path('.'), 'old', {'kernel_build_id': 'id'}, 'magic', 'modprobe', 'modinfo')
            result = module.select(Path('.'), 'current', {'kernel_build_id': 'id'}, 'magic', 'modprobe', 'modinfo')
            self.assertEqual(result['static_candidates'], ['group-b'])
            self.assertFalse(result['runtime_admitted'])
            self.assertFalse(result['reviewed_crc_vermagic'])
            result = module.select(Path('.'), 'current', {'kernel_build_id': 'id'}, 'magic', 'modprobe', 'modinfo', True)
            self.assertTrue(result['reviewed_crc_vermagic'])
            self.assertTrue(module.screen.evaluate.call_args.args[-1])


if __name__ == '__main__':
    unittest.main()
