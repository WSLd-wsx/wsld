import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/export-kernel-buildid-asset.py'
spec = importlib.util.spec_from_file_location('buildid_asset_test', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class BuildIdAssetTests(unittest.TestCase):
    def test_exact_buildid_names_archive_and_rejects_crc_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            snapshot = root / 'snapshot'
            (snapshot / 'audits').mkdir(parents=True)
            group = 'a' * 64
            (snapshot / 'groups' / group).mkdir(parents=True)
            records, audit_modules = {}, {}
            for name in module.package.MODULES:
                path = snapshot / 'groups' / group / name
                path.write_bytes(name.encode())
                digest = module.package.digest(path)
                records[name] = {'sha256': digest, 'bytes': path.stat().st_size}
                audit_modules[name] = {'sha256': digest, 'missing': [], 'mismatches': [],
                                       'has_module_layout': True, 'import_count': 1}
            audit = {'crc_match': True, 'runtime_admitted': False, 'kernel_build_id': 'b' * 40,
                     'modules': audit_modules}
            (snapshot / 'audits' / ('c' * 64 + '.json')).write_text(json.dumps(audit))
            target = {'group': group, 'kernel_build_id': 'b' * 40, 'kernel_image_sha256': 'd' * 64,
                      'audit_sha256': 'c' * 64}
            index = {'generation': 'e' * 64, 'targets': {'source-tag': target}, 'groups': {group: records}}
            evidence = root / 'exports.json'
            evidence.write_text(json.dumps({'kernel_build_id': 'b' * 40, 'kernel_image_sha256': 'd' * 64}))
            with patch.object(module.package, 'validate', return_value=index):
                result = module.export(snapshot, evidence, root / 'out')
            self.assertEqual(result['archive'], 'modules-' + 'b' * 40 + '.tar.gz')
            self.assertEqual(result['archive_sha256'], module.package.digest(root / 'out' / result['archive']))
            self.assertFalse(result['release_ready'])
            audit_modules[next(iter(audit_modules))]['mismatches'] = ['bad']
            (snapshot / 'audits' / ('c' * 64 + '.json')).write_text(json.dumps(audit))
            with patch.object(module.package, 'validate', return_value=index):
                with self.assertRaisesRegex(ValueError, 'Invalid module CRC evidence'):
                    module.export(snapshot, evidence, root / 'bad')
            self.assertFalse((root / 'bad').exists())


if __name__ == '__main__':
    unittest.main()
