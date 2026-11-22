#!/usr/bin/env python3
"""Plan kernel packages referenced by released WSL 3.0.3 and newer."""
import base64
import importlib.util
import json
import re
import subprocess
import sys
from pathlib import Path
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('kernel_release_selection', ROOT / 'kernel_release_selection.py')
selection = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selection)
spec = importlib.util.spec_from_file_location('wsl_preview', ROOT / 'wsl-preview-kernel.py')
preview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preview)
WSL_VERSION = re.compile(r'([0-9]+)\.([0-9]+)\.([0-9]+)(?:\.([0-9]+))?(?:-(?:rc|preview)[0-9]+)?\Z', re.I)
SOURCE = re.compile(r'linux-msft-wsl-([0-9]+(?:\.[0-9]+){3})\Z')
KERNEL = re.compile(r'([0-9]+(?:\.[0-9]+){3})-([0-9]+)\Z')
SHA = re.compile(r'[0-9a-f]{40}\Z')


def selected_releases(releases):
    if not isinstance(releases, list): raise ValueError('Invalid WSL releases')
    selected = []
    for release in releases:
        if not isinstance(release, dict) or not isinstance(release.get('draft'), bool) or not isinstance(release.get('prerelease'), bool):
            raise ValueError('Invalid WSL release record')
        tag = release.get('tag_name')
        if release['draft'] or not isinstance(tag, str): continue
        match = WSL_VERSION.fullmatch(tag)
        if not match: continue
        version = tuple(int(x or 0) for x in match.groups())
        if version >= (3, 0, 3, 0): selected.append({'tag': tag, 'preview': release['prerelease'], 'version': version})
    if not selected: raise ValueError('No WSL releases at or above 3.0.3 found')
    if len({x['tag'] for x in selected}) != len(selected): raise ValueError('Duplicate WSL release tag')
    return sorted(selected, key=lambda x: (x['version'], not x['preview'], x['tag']), reverse=True)


def plan(releases, kernel_by_wsl, index, assets):
    if not isinstance(index, dict) or not isinstance(index.get('versions'), list) or not isinstance(assets, list):
        raise ValueError('Invalid kernel package or release inventory')
    published = {a['name'] for a in assets if isinstance(a, dict) and isinstance(a.get('name'), str)}
    found = {}
    for release in selected_releases(releases):
        tag = release['tag']
        version = kernel_by_wsl.get(tag)
        match = KERNEL.fullmatch(version) if isinstance(version, str) else None
        if not match: raise ValueError('Unsupported or missing exact WSL kernel dependency for ' + tag)
        if version not in index['versions']: raise ValueError('Exact WSL kernel package unavailable: ' + version)
        entry = found.setdefault(version, {'tag': 'linux-msft-wsl-' + match.group(1), 'version': version, 'wsl_releases': []})
        entry['wsl_releases'].append({'tag': tag, 'prerelease': release['preview']})
    for version, entry in found.items():
        archive = 'modules_' + version + '.tar.gz'
        if archive in published and not any(re.fullmatch(re.escape('modules_' + version) + r'-[0-9]+-[0-9]+\.json', name) for name in published):
            raise ValueError('Published kernel archive is missing matching metadata: ' + version)
        entry['missing'] = archive not in published
    return sorted(found.values(), key=lambda e: tuple(int(v) for v in re.split(r'[.-]', e['version'])), reverse=True)


def early_target(release, index, targets):
    """Pick the latest official stable source with a listed exact package."""
    if release.get('draft') is not False or release.get('prerelease') is not False:
        raise ValueError('Latest official kernel release is not stable')
    match = SOURCE.fullmatch(release.get('tag_name', ''))
    if not match:
        return None
    candidates = [version for version in index['versions']
                  if (item := KERNEL.fullmatch(version)) and item.group(1) == match.group(1)]
    version_key = lambda value: tuple(map(int, re.split(r'[.-]', value)))
    if not candidates:
        return None
    version = max(candidates, key=version_key)
    if version_key(version) < max(version_key(item['version']) for item in targets):
        return None
    referenced = any(item['version'] == version for item in targets)
    return {'tag': release['tag_name'], 'version': version, 'referenced': referenced}


def published_releases(repository):
    releases = []
    for page in range(1, 101):
        batch = gh_json(f'repos/{repository}/releases?per_page=100&page={page}')
        if not isinstance(batch, list): raise ValueError('Invalid release inventory')
        releases.extend(batch)
        if len(batch) < 100: return releases
    raise ValueError('Release inventory exceeds page limit')


def gh_json(endpoint):
    return json.loads(subprocess.check_output(['gh', 'api', endpoint], text=True))


def published_release_assets(repository):
    """Find an existing release or safely plan the first publication."""
    metadata = gh_json(f'repos/{repository}')
    if repository != 'WSLd-wsx/wsld' or metadata.get('full_name') != repository:
        raise ValueError('Kernel repository identity mismatch')
    releases = []
    for page in range(1, 101):
        batch = gh_json(f'repos/{repository}/releases?per_page=100&page={page}')
        if not isinstance(batch, list): raise ValueError('Invalid target release inventory')
        releases.extend(batch)
        if len(batch) < 100: break
    else:
        raise ValueError('Target release inventory exceeds page limit')
    try:
        return selection.select(releases)['assets']
    except ValueError as error:
        if str(error) == 'No published kernel rebuild release': return []
        raise


def tag_commit(tag, repository='microsoft/WSL'):
    obj = gh_json(f'repos/{repository}/git/ref/tags/' + tag)['object']
    for _ in range(4):
        sha = obj.get('sha', '')
        if not SHA.fullmatch(sha): raise ValueError('Invalid WSL tag identity')
        if obj.get('type') == 'commit': return sha
        if obj.get('type') != 'tag': raise ValueError('Unsupported WSL tag object')
        obj = gh_json(f'repos/{repository}/git/tags/' + sha)['object']
    raise ValueError('Excessive nested WSL tag objects')


def wsl_kernel(tag):
    commit = tag_commit(tag)
    obj = gh_json('repos/microsoft/WSL/contents/packages.config?ref=' + commit)
    if obj.get('encoding') != 'base64' or not isinstance(obj.get('content'), str):
        raise ValueError('Invalid WSL package manifest encoding')
    data = base64.b64decode(obj['content'], validate=False)
    return preview.kernel_dependency(data)


def main():
    repository = sys.argv[1]
    releases = published_releases('microsoft/WSL')
    kernels = {item['tag']: wsl_kernel(item['tag']) for item in selected_releases(releases)}
    assets = published_release_assets(repository)
    spec = importlib.util.spec_from_file_location('feed', ROOT / 'wsl-kernel-feed.py')
    feed = importlib.util.module_from_spec(spec); spec.loader.exec_module(feed)
    with urlopen(Request(feed.BASE + '/index.json', headers={'User-Agent':'WSX-kernel-builder'}), timeout=60) as response:
        raw = response.read(1024 * 1024 + 1)
    if len(raw) > 1024 * 1024: raise ValueError('Microsoft kernel package index is too large')
    index = json.loads(raw)
    targets = plan(releases, kernels, index, assets)
    if len(sys.argv) == 3:
        try:
            latest = gh_json('repos/microsoft/WSL2-Linux-Kernel/releases/latest')
            candidate = early_target(latest, index, targets)
            if candidate:
                candidate['commit'] = tag_commit(candidate['tag'], 'microsoft/WSL2-Linux-Kernel')
        except (subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
            print(f'Kernel prebuild discovery unavailable: {error}', file=sys.stderr)
            candidate = None
        with Path(sys.argv[2]).open('x', encoding='utf-8') as output:
            json.dump(candidate, output, sort_keys=True)
    print(json.dumps(targets, sort_keys=True))

if __name__ == '__main__': main()
