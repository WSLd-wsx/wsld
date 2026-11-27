import importlib.util
import pathlib
import unittest
from unittest.mock import patch

path = pathlib.Path(__file__).resolve().parents[1] / 'scripts/discover-kernel-series.py'
spec = importlib.util.spec_from_file_location('discover_kernel_series', path)
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)

class DiscoveryTests(unittest.TestCase):
    def test_feed_candidates_require_an_exact_stable_source_release(self):
        stable = {'tag_name': 'linux-msft-wsl-6.18.54.1', 'draft': False, 'prerelease': False}
        preview = dict(stable, tag_name='linux-msft-wsl-6.18.58.1', prerelease=True)
        draft = dict(stable, tag_name='linux-msft-wsl-6.18.60.1', draft=True)
        source_only = dict(stable, tag_name='linux-msft-wsl-6.18.66.1')
        feed = {'versions': [
            '6.18.54.1-2', '6.18.54.1-1', '6.18.54.1-10', '6.18.54.1-special',
            '6.18.58.1-1', '6.18.60.1-1', '6.18.64.1-1',
        ]}
        self.assertEqual(module.latest_stable_target(feed, [preview, stable, source_only, draft]),
                         {'tag': stable['tag_name'], 'version': '6.18.54.1-10'})
        self.assertIsNone(module.latest_stable_target(feed, [preview, source_only, draft]))

    def test_unreferenced_stable_enters_publication_and_is_retained(self):
        releases = [{'tag_name': '3.0.5', 'draft': False, 'prerelease': True}]
        kernels = {'3.0.5': '6.18.66.1-1'}
        feed = {'versions': ['6.18.64.1-1', '6.18.66.1-1']}
        latest = {'tag_name': 'linux-msft-wsl-6.18.64.1', 'draft': False, 'prerelease': False}
        assets = [{'name': 'modules_6.18.66.1-1.tar.gz'}, {'name': 'modules_6.18.66.1-1-3-1.json'}]
        wsl_target, stable_target = module.plan(releases, kernels, feed, assets, [latest])
        self.assertFalse(wsl_target['missing'])
        self.assertEqual(stable_target, {
            'tag': latest['tag_name'], 'version': '6.18.64.1-1', 'wsl_releases': [],
            'latest_stable': True, 'missing': True,
        })

        assets += [{'name': 'modules_6.18.64.1-1.tar.gz'}, {'name': 'modules_6.18.64.1-1-4-1.json'}]
        self.assertFalse(any(target['missing'] for target in module.plan(releases, kernels, feed, assets, [latest])))

        latest['tag_name'] = 'linux-msft-wsl-6.18.66.1'
        result = module.plan(releases, kernels, feed, assets, [latest])
        self.assertEqual([target['version'] for target in result], ['6.18.66.1-1', '6.18.64.1-1'])
        self.assertTrue(result[0]['latest_stable'])
        self.assertFalse(any(target['missing'] for target in result))

    def test_wsl_adoption_reuses_only_the_exact_published_package_revision(self):
        releases = [{'tag_name': '3.0.6', 'draft': False, 'prerelease': False}]
        stable = {'tag_name': 'linux-msft-wsl-6.18.64.1', 'draft': False, 'prerelease': False}
        assets = [{'name': 'modules_6.18.64.1-1.tar.gz'}, {'name': 'modules_6.18.64.1-1-4-1.json'}]
        feed = {'versions': ['6.18.64.1-1']}
        targets = module.plan(releases, {'3.0.6': '6.18.64.1-1'}, feed, assets, [stable])
        self.assertEqual([(item['version'], item['missing']) for item in targets], [('6.18.64.1-1', False)])
        self.assertEqual(targets[0]['wsl_releases'], [{'tag': '3.0.6', 'prerelease': False}])

        feed['versions'].append('6.18.64.1-2')
        targets = module.plan(releases, {'3.0.6': '6.18.64.1-2'}, feed, assets, [stable])
        self.assertEqual([(item['version'], item['missing']) for item in targets],
                         [('6.18.64.1-2', True), ('6.18.64.1-1', False)])

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
