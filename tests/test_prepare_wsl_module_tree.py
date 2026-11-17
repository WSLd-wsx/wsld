import importlib.util
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
spec = importlib.util.spec_from_file_location('prepare_wsl_module_tree', ROOT / 'scripts/prepare-wsl-module-tree.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class TargetConfigTests(unittest.TestCase):
    def test_rejects_missing_target_btf_modules(self):
        target = {'CONFIG_DEBUG_INFO_BTF_MODULES': 'y', 'CONFIG_CC_VERSION_TEXT': 'gcc'}
        module.verify_target_config(target, {'CONFIG_DEBUG_INFO_BTF_MODULES': 'y', 'CONFIG_CC_VERSION_TEXT': 'cross-gcc'})
        with self.assertRaisesRegex(ValueError, 'CONFIG_DEBUG_INFO_BTF_MODULES'):
            module.verify_target_config(target, {'CONFIG_CC_VERSION_TEXT': 'cross-gcc'})


if __name__ == '__main__':
    unittest.main()
