import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('selection', Path(__file__).resolve().parents[1] / 'scripts/kernel_release_selection.py')
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)


def release(run, attempt=1, draft=False, prerelease=False, complete=True):
    assets = [{'name':'modules_6.18.54.1-1.tar.gz'}]
    if complete: assets.append({'name':f'modules_6.18.54.1-1-{run}-{attempt}.json'})
    return {'tag_name':f'wsx-kernel-rebuild-{run}-{attempt}', 'draft':draft, 'prerelease':prerelease, 'assets':assets}


class KernelReleaseSelectionTests(unittest.TestCase):
    def test_selects_latest_numeric_run_and_attempt_not_drafts(self):
        current = release(10, 2); current['tag_name'] = 'wsx-kernel-10-2'
        selected = module.select([release(9, 3), release(10, 1), current, release(11, draft=True)])
        self.assertEqual(selected['tag_name'], 'wsx-kernel-10-2')

    def test_latest_incomplete_release_fails_instead_of_mixing_versions(self):
        with self.assertRaisesRegex(ValueError, 'missing matching metadata'):
            module.select([release(10), release(11, complete=False)])
        with self.assertRaisesRegex(ValueError, 'No published'):
            module.select([release(11, prerelease=True)])
        duplicate = release(10); duplicate['tag_name'] = 'wsx-kernel-10-1'
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            module.select([release(10), duplicate])


if __name__ == '__main__': unittest.main()
