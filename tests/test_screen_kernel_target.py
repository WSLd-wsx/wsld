import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/screen-kernel-target.py'
spec = importlib.util.spec_from_file_location('screen_kernel_target', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ScreeningTests(unittest.TestCase):
    def test_crc_prescreen_rejects_mismatch_and_preserves_candidates(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def inspect(_package, _digest, _version, dest):
                dest.mkdir()
                (dest / 'kernel-exports.json').write_text(json.dumps({'kernel_build_id': 'abcdef12'}))
                return {'kernel_build_id': 'abcdef12'}
            index = {'generation': 'current', 'groups': {'match': {}, 'mismatch': {}}}
            def compare(_exports, path, _program):
                return {'crc_match': path.name == 'match', 'modules': {
                    'module.ko': {'import_count': 1, 'has_module_layout': True}}}
            with patch.object(module.selector.package, 'validate', return_value=index), \
                 patch.object(module.inspector, 'inspect', side_effect=inspect), \
                 patch.object(module.crc_module, 'compare', side_effect=compare):
                result = module.crc_prescreen(root / 'package', 'hash', 'version', root / 'snapshot',
                                              'current', root / 'out', 'modprobe')
            self.assertEqual(result['candidates'], ['match'])
            self.assertFalse(result['runtime_admitted'])

    def test_decisions_keep_errors_separate_from_builds(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def inspect(_package, _digest, _version, dest):
                dest.mkdir()
                (dest / 'kernel-exports.json').write_text(json.dumps({'kernel_build_id': 'id'}))
                return {'kernel_build_id': 'id'}
            outcomes = [({'inspection_complete': False, 'static_candidates': []}, 'inspection_failed'),
                        ({'inspection_complete': True, 'static_candidates': ['candidate']}, 'candidate_requires_admission'),
                        ({'inspection_complete': True, 'static_candidates': []}, 'build_required')]
            with patch.object(module.selector.package, 'validate', return_value={'generation': 'current'}), patch.object(module.inspector, 'inspect', side_effect=inspect):
                for i, (selection, expected) in enumerate(outcomes):
                    with patch.object(module.selector, 'select', return_value=selection):
                        result = module.screen(root / 'package', 'hash', 'version', root / 'snapshot', 'current', 'magic', root / str(i), 'modprobe', 'modinfo')
                    self.assertEqual(result['decision'], expected)
                    self.assertFalse(result['release_ready'])
                with self.assertRaisesRegex(ValueError, 'generation'):
                    module.screen(root / 'package', 'hash', 'version', root / 'snapshot', 'old', 'magic', root / 'old', 'modprobe', 'modinfo')


if __name__ == '__main__':
    unittest.main()
