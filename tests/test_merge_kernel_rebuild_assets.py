import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('merge_rebuild', Path(__file__).resolve().parents[1] / 'scripts/merge-kernel-rebuild-assets.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class MergeRebuildTests(unittest.TestCase):
    def test_combines_targets_and_rejects_conflicting_shared_assets(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); build = root / 'build'; build.mkdir()
            versions = ('6.18.54.1-1', '6.18.58.1-1')
            targets = root / 'targets'
            targets.write_text(''.join('tag\t' + version + '\n' for version in versions))
            for version in versions:
                folder = build / version; folder.mkdir()
                for name in (f'modules_{version}.tar.gz', f'modules_{version}-12-1.json',
                             f'source-materials-{version}-12-1.json', f'SHA256SUMS-{version}-12-1', 'shared.tar.gz'):
                    (folder / name).write_bytes(b'fixture')
            module.merge(build, targets, root / 'out', '12-1')
            self.assertTrue((root / 'out/shared.tar.gz').exists())
            (build / versions[1] / 'shared.tar.gz').write_bytes(b'different')
            with self.assertRaisesRegex(ValueError, 'Conflicting'):
                module.merge(build, targets, root / 'out2', '12-1')
            (build / versions[1] / f'modules_{versions[1]}.tar.gz').unlink()
            with self.assertRaisesRegex(ValueError, 'Incomplete'):
                module.merge(build, targets, root / 'out3', '12-1')


if __name__ == '__main__':
    unittest.main()
