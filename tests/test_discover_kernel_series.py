import importlib.util
import pathlib
import unittest
from unittest.mock import patch

path = pathlib.Path(__file__).resolve().parents[1] / 'scripts/discover-kernel-series.py'
spec = importlib.util.spec_from_file_location('discover_kernel_series', path)
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)

class DiscoveryTests(unittest.TestCase):
    def test_only_latest_stable_source_with_an_exact_feed_package_is_prebuilt(self):
        release = {'tag_name': 'linux-msft-wsl-6.18.54.1', 'draft': False, 'prerelease': False}
        feed = {'versions': ['6.18.40.1-1', '6.18.54.1-1', '6.18.54.1-2']}
        targets = [{'version': '6.18.40.1-1'}]
        self.assertEqual(module.early_target(release, feed, targets),
                         {'tag': release['tag_name'], 'version': '6.18.54.1-2', 'referenced': False})
        self.assertTrue(module.early_target(release, feed, targets + [{'version': '6.18.54.1-2'}])['referenced'])
        self.assertIsNone(module.early_target(release, {'versions': ['6.18.40.1-1']}, targets))
        self.assertIsNone(module.early_target(release, feed, [{'version': '6.18.58.1-1'}]))
        self.assertIsNone(module.early_target(dict(release, tag_name=release['tag_name']+'-rc1'), feed, targets))
        with self.assertRaisesRegex(ValueError, 'not stable'):
            module.early_target(dict(release, prerelease=True), feed, targets)

    def test_wsl_versions_include_preview_deduplicate_and_skip_existing(self):
        releases = [{'tag_name':'3.0.2','draft':False,'prerelease':True},
                    {'tag_name':'3.0.3','draft':False,'prerelease':False},
                    {'tag_name':'3.0.4','draft':False,'prerelease':True},
                    {'tag_name':'3.0.5','draft':False,'prerelease':True},
                    {'tag_name':'3.0.6','draft':True,'prerelease':False}]
        kernels = {'3.0.3':'6.18.54.1-1','3.0.4':'6.18.54.1-1','3.0.5':'6.18.58.1-1'}
        feed = {'versions':['6.18.54.1-1','6.18.58.1-1','6.18.58.1-2']}
        assets = [{'name':'modules_6.18.54.1-1.tar.gz'}, {'name':'modules_6.18.54.1-1-3-1.json'}]
        result = module.plan(releases, kernels, feed, assets)
        self.assertEqual([r['version'] for r in result], ['6.18.58.1-1','6.18.54.1-1'])
        self.assertEqual([r['missing'] for r in result], [True,False])
        self.assertEqual(len(result[1]['wsl_releases']), 2)
    def test_missing_exact_package_is_not_replaced_by_newer_revision(self):
        releases = [{'tag_name':'3.0.3','draft':False,'prerelease':False}]
        with self.assertRaisesRegex(ValueError, 'Exact WSL kernel package unavailable'):
            module.plan(releases, {'3.0.3':'6.18.54.1-1'}, {'versions':['6.18.54.1-2']}, [])
    def test_first_publication_checks_repository_identity_for_public_and_private(self):
        repo = 'WSLd-wsx/wsld'
        calls = [dict(full_name=repo, private=True), []]
        with patch.object(module, 'gh_json', side_effect=calls):
            self.assertEqual(module.published_release_assets(repo), [])
        with patch.object(module, 'gh_json', side_effect=[dict(full_name=repo, private=False), []]):
            self.assertEqual(module.published_release_assets(repo), [])
        with patch.object(module, 'gh_json', return_value=dict(full_name='other/repo', private=False)), self.assertRaisesRegex(ValueError, 'identity'):
            module.published_release_assets(repo)
        with patch.object(module, 'gh_json', side_effect=[dict(full_name=repo, private=True), RuntimeError('403')]), self.assertRaisesRegex(RuntimeError, '403'):
            module.published_release_assets(repo)
    def test_existing_release_assets_are_reused(self):
        repo = 'WSLd-wsx/wsld'
        assets = [{'name':'modules_6.18.54.1-1.tar.gz'}, {'name':'modules_6.18.54.1-1-30-1.json'}]
        releases = [{'tag_name':'wsx-kernel-rebuild-30-1','assets':assets,'draft':False,'prerelease':False},
                    {'tag_name':'wsx-kernel-rebuild-40-1','assets':[],'draft':True,'prerelease':False},
                    {'tag_name':'wsx-kernel-modules','assets':[],'draft':False,'prerelease':False}]
        with patch.object(module, 'gh_json', side_effect=[dict(full_name=repo, private=True), releases]):
            self.assertEqual(module.published_release_assets(repo), assets)

    def test_older_releases_and_drafts_are_excluded(self):
        releases = [{'tag_name':'3.0.2','draft':False,'prerelease':True}, {'tag_name':'3.0.3','draft':True,'prerelease':False}]
        with self.assertRaisesRegex(ValueError, 'No WSL releases'):
            module.plan(releases, {}, {'versions':[]}, [])

if __name__ == '__main__': unittest.main()
