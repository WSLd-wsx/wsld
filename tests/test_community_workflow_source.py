import unittest
from pathlib import Path

import yaml


class CommunitySourceTests(unittest.TestCase):
    def test_latest_main_is_resolved_once_and_reused(self):
        path = Path(__file__).resolve().parents[1] / '.github/workflows/community-release.yml'
        workflow = yaml.safe_load(path.read_text(encoding='utf-8'))
        self.assertNotIn('wsld_commit', workflow[True]['workflow_dispatch']['inputs'])
        steps = workflow['jobs']['build']['steps']
        source = next(step for step in steps if step.get('id') == 'source')
        self.assertIn('repos/WSLd-wsx/wsld/commits/main', source['run'])
        self.assertIn('^[0-9a-f]{40}$', source['run'])
        checkout = next(step for step in steps if step['name'] == 'Checkout pinned public Core source')
        self.assertEqual(checkout['with']['ref'], '${{ steps.source.outputs.sha }}')
        referenced = [step for step in steps if step.get('env', {}).get('SOURCE_COMMIT')]
        self.assertEqual(len(referenced), 4)
        for step in referenced:
            self.assertEqual(step['env']['SOURCE_COMMIT'], '${{ steps.source.outputs.sha }}')

    def test_host_and_draft_inputs_are_automatic(self):
        path = Path(__file__).resolve().parents[1] / '.github/workflows/community-release.yml'
        workflow = yaml.safe_load(path.read_text(encoding='utf-8'))
        self.assertEqual(set(workflow[True]['workflow_dispatch']['inputs']), {'release'})
        steps = workflow['jobs']['build']['steps']
        runtime = next(step for step in steps if step.get('id') == 'runtime')
        self.assertIn('gh release view --repo WSXdesk/wsx-host', runtime['run'])
        self.assertIn('wsx-runtime.zip', runtime['run'])
        self.assertIn('Get-FileHash', runtime['run'])
        build = next(step for step in steps if step['name'] == 'Build and package community edition')
        for key, output in (('RUNTIME_TAG', 'tag'), ('RUNTIME_ASSET', 'asset'), ('RUNTIME_SHA256', 'sha256')):
            self.assertEqual(build['env'][key], '${{ steps.runtime.outputs.' + output + ' }}')
        draft = next(step for step in steps if step['name'] == 'Create draft release for review')
        self.assertIn('community-draft-$env:GITHUB_RUN_ID-$env:GITHUB_RUN_ATTEMPT', draft['run'])

    def test_delivery_verifies_mpl_sources_against_build_inputs(self):
        path = Path(__file__).resolve().parents[1] / '.github/workflows/community-release.yml'
        workflow = yaml.safe_load(path.read_text(encoding='utf-8'))
        steps = workflow['jobs']['build']['steps']
        build = next(step for step in steps if step['name'] == 'Build and package community edition')
        self.assertIn('community-mpl-sources.py verify', build['run'])
        for value in ('community/Cargo.lock', 'mpl-dependency-tree.txt', 'wsx-community-windows-x64.zip', 'wsx-community-materials.zip'):
            self.assertIn(value, build['run'])

    def test_release_titles_use_community_edition(self):
        path = Path(__file__).resolve().parents[1] / '.github/workflows/community-release.yml'
        steps = yaml.safe_load(path.read_text(encoding='utf-8'))['jobs']['build']['steps']
        for name in ('Save test release for review', 'Create draft release for review'):
            step = next(step for step in steps if step['name'] == name)
            self.assertIn('--title "WSX Community Edition $env:GITHUB_RUN_ID"', step['run'])

    def test_release_target_accepts_either_repository_visibility(self):
        path = Path(__file__).resolve().parents[1] / '.github/workflows/community-release.yml'
        steps = yaml.safe_load(path.read_text(encoding='utf-8'))['jobs']['build']['steps']
        for name in ('Save test release for review', 'Create draft release for review'):
            step = next(step for step in steps if step['name'] == name)
            self.assertIn("$info.full_name -cne 'WSLd-wsx/wsld'", step['run'])
            self.assertNotIn('$info.private', step['run'])


if __name__ == '__main__': unittest.main()
