import unittest
from pathlib import Path

import yaml


class CommunitySourceTests(unittest.TestCase):
    def test_latest_main_is_resolved_once_and_reused(self):
        path = Path(__file__).resolve().parents[1] / '.github/workflows/community-release.yml'
        workflow = yaml.safe_load(path.read_text(encoding='utf-8'))
        self.assertNotIn('wsld_commit', workflow[True]['workflow_dispatch']['inputs'])
        image = workflow['jobs']['service-image']
        source = next(step for step in image['steps'] if step.get('id') == 'source')
        self.assertIn('repos/WSLd-wsx/wsld/commits/main', source['run'])
        self.assertIn('^[0-9a-f]{40}$', source['run'])
        image_checkout = next(step for step in image['steps'] if 'actions/checkout@' in step.get('uses', ''))
        self.assertEqual(image_checkout['with']['ref'], '${{ steps.source.outputs.sha }}')
        self.assertEqual(image['outputs']['source_commit'], '${{ steps.source.outputs.sha }}')
        build = workflow['jobs']['build']
        self.assertEqual(build['needs'], 'service-image')
        steps = build['steps']
        shared_commit = '${{ needs.service-image.outputs.source_commit }}'
        checkout = next(step for step in steps if step.get('name') == 'Checkout pinned public Core source')
        self.assertEqual(checkout['with']['ref'], shared_commit)
        referenced = [step for step in steps if step.get('env', {}).get('SOURCE_COMMIT')]
        self.assertEqual(len(referenced), 4)
        for step in referenced:
            self.assertEqual(step['env']['SOURCE_COMMIT'], shared_commit)

    def test_host_and_draft_inputs_are_automatic(self):
        path = Path(__file__).resolve().parents[1] / '.github/workflows/community-release.yml'
        workflow = yaml.safe_load(path.read_text(encoding='utf-8'))
        self.assertEqual(set(workflow[True]['workflow_dispatch']['inputs']), {'release'})
        steps = workflow['jobs']['build']['steps']
        runtime = next(step for step in steps if step.get('id') == 'runtime')
        self.assertIn('gh release view --repo $env:RUNTIME_REPOSITORY', runtime['run'])
        self.assertIn('select(.private == true)', runtime['run'])
        self.assertIn('wsx-runtime.zip', runtime['run'])
        self.assertIn('Get-FileHash', runtime['run'])
        build = next(step for step in steps if step.get('name') == 'Build and package community edition')
        for key, output in (('RUNTIME_TAG', 'tag'), ('RUNTIME_ASSET', 'asset'), ('RUNTIME_SHA256', 'sha256')):
            self.assertEqual(build['env'][key], '${{ steps.runtime.outputs.' + output + ' }}')
        draft = next(step for step in steps if step.get('name') == 'Create draft release for review')
        self.assertIn('$env:RELEASE_TAG = "v$env:COMMUNITY_VERSION"', draft['run'])
        self.assertIn("$hostInfo.full_name -cne $env:RUNTIME_REPOSITORY", draft['run'])
        for step in (runtime, draft):
            self.assertEqual(step['env']['RUNTIME_REPOSITORY'], '${{ secrets.WSX_RUNTIME_REPOSITORY }}')

    def test_delivery_verifies_mpl_sources_against_build_inputs(self):
        path = Path(__file__).resolve().parents[1] / '.github/workflows/community-release.yml'
        workflow = yaml.safe_load(path.read_text(encoding='utf-8'))
        steps = workflow['jobs']['build']['steps']
        build = next(step for step in steps if step.get('name') == 'Build and package community edition')
        self.assertIn('community-mpl-sources.py verify', build['run'])
        for value in ('community/Cargo.lock', 'mpl-dependency-tree.txt', 'wsx-community-windows-x64.zip', 'wsx-community-materials.zip'):
            self.assertIn(value, build['run'])

    def test_executable_and_release_share_the_selected_version(self):
        path = Path(__file__).resolve().parents[1] / '.github/workflows/community-release.yml'
        steps = yaml.safe_load(path.read_text(encoding='utf-8'))['jobs']['build']['steps']
        build = next(step for step in steps if step.get('name') == 'Build and package community edition')
        self.assertEqual(build['env']['COMMUNITY_VERSION'], '${{ steps.version.outputs.number }}')
        self.assertIn('$env:TAURI_CONFIG = @{version = $env:COMMUNITY_VERSION}', build['run'])
        self.assertIn('$versionInfo.ProductVersion -cne $env:COMMUNITY_VERSION', build['run'])
        for name in ('Save test release for review', 'Create draft release for review'):
            step = next(step for step in steps if step.get('name') == name)
            self.assertEqual(step['env']['COMMUNITY_VERSION'], build['env']['COMMUNITY_VERSION'])
            self.assertIn('"v$env:COMMUNITY_VERSION"', step['run'])
            self.assertIn('--verify-tag', step['run'])
            self.assertIn('--title "WSX Community Edition $env:COMMUNITY_VERSION"', step['run'])

    def test_release_target_accepts_either_repository_visibility(self):
        path = Path(__file__).resolve().parents[1] / '.github/workflows/community-release.yml'
        steps = yaml.safe_load(path.read_text(encoding='utf-8'))['jobs']['build']['steps']
        for name in ('Save test release for review', 'Create draft release for review'):
            step = next(step for step in steps if step.get('name') == name)
            self.assertIn("$info.full_name -cne 'WSLd-wsx/wsld'", step['run'])
            self.assertNotIn('$info.private', step['run'])


if __name__ == '__main__': unittest.main()
