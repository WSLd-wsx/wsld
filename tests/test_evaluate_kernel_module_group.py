import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/evaluate-kernel-module-group.py'
spec = importlib.util.spec_from_file_location('evaluate_kernel_module_group', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class EvaluationTests(unittest.TestCase):
    def test_static_candidate_requires_matching_crc_metadata_and_dependencies(self):
        crc = {'crc_match': True, 'kernel_build_id': 'abc',
               'modules': {'a.ko': {'sha256': 'hash', 'import_count': 1, 'has_module_layout': True,
                                     'module_symbol_providers': []}}}
        info = {'external_dependencies': [], 'modules': {'a': {
            'file': 'a.ko', 'sha256': 'hash', 'vermagic': 'target SMP modversions', 'depends': []}}}
        with patch.object(module.crc, 'compare', return_value=crc), patch.object(module.requirements, 'inspect', return_value=info):
            evaluate = lambda: module.evaluate({}, Path('.'), 'target SMP modversions', 'modprobe', 'modinfo')
            self.assertTrue(evaluate()['static_candidate'])
            self.assertFalse(evaluate()['runtime_admitted'])
            info['external_dependencies'] = ['unknown']
            self.assertIn('external_module_dependencies_unverified', evaluate()['reasons'])
            info['external_dependencies'] = []
            crc['crc_match'] = False
            self.assertFalse(evaluate()['static_candidate'])
            crc['crc_match'] = True
            crc['modules']['a.ko']['module_symbol_providers'] = ['b.ko']
            self.assertIn('unreachable_symbol_provider:a:b.ko', evaluate()['reasons'])
            info['modules']['a']['depends'] = ['b']
            self.assertTrue(evaluate()['static_candidate'])
            crc['modules']['a.ko']['module_symbol_providers'] = []
            info['modules']['a']['vermagic'] = 'other SMP modversions'
            self.assertFalse(evaluate()['static_candidate'])
            self.assertTrue(module.evaluate({}, Path('.'), 'target SMP modversions', 'modprobe', 'modinfo', True)['static_candidate'])
            crc['modules']['a.ko']['has_module_layout'] = False
            self.assertFalse(module.evaluate({}, Path('.'), 'target SMP modversions', 'modprobe', 'modinfo', True)['static_candidate'])
            info['modules']['a']['vermagic'] = 'target SMP modversions'
            self.assertIn('missing_module_layout_crc:a', evaluate()['reasons'])
            crc['modules']['a.ko']['has_module_layout'] = True
            self.assertFalse(module.evaluate({}, Path('.'), 'target SMP modversions other', 'modprobe', 'modinfo', True)['static_candidate'])
            info['modules']['a']['sha256'] = 'changed'
            with self.assertRaisesRegex(ValueError, 'changed'):
                evaluate()


if __name__ == '__main__':
    unittest.main()
