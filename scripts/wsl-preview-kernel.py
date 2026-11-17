#!/usr/bin/env python3
"""Identify the kernel dependency of Microsoft's newest WSL prerelease."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import xml.etree.ElementTree as ET
from urllib.request import Request, urlopen

RELEASES = 'https://api.github.com/repos/microsoft/WSL/releases?per_page=100'
REPOSITORY = 'https://github.com/microsoft/WSL.git'
TAG = re.compile(r'[0-9]+(?:\.[0-9]+){2,3}\Z')
KERNEL = re.compile(r'([0-9]+(?:\.[0-9]+){3})-([0-9]+)\Z')


def latest_preview(releases):
    if not isinstance(releases, list):
        raise ValueError('Invalid WSL release inventory')
    candidates = []
    for item in releases:
        if not isinstance(item, dict) or not isinstance(item.get('draft'), bool) or not isinstance(item.get('prerelease'), bool):
            raise ValueError('Invalid WSL release record')
        name = item.get('tag_name')
        if item['draft'] or not item['prerelease'] or not isinstance(name, str) or not TAG.fullmatch(name):
            continue
        candidates.append(name)
    if not candidates:
        raise ValueError('No versioned public WSL prerelease found')
    return max(candidates, key=lambda value: tuple(int(x) for x in value.split('.')))


def tag_commit(refs, tag):
    if not TAG.fullmatch(tag):
        raise ValueError('Invalid WSL release tag')
    found = {}
    for line in refs.splitlines():
        commit, ref = line.split('\t', 1)
        if not re.fullmatch(r'[0-9a-f]{40}', commit):
            raise ValueError('Invalid WSL tag commit')
        if ref in ('refs/tags/' + tag, 'refs/tags/' + tag + '^{}'):
            if ref in found and found[ref] != commit:
                raise ValueError('Conflicting WSL release tag')
            found[ref] = commit
    if 'refs/tags/' + tag not in found:
        raise ValueError('WSL prerelease tag is missing')
    return found.get('refs/tags/' + tag + '^{}', found['refs/tags/' + tag])


def kernel_dependency(data):
    if len(data) > 1024 * 1024 or b'<!DOCTYPE' in data.upper() or b'<!ENTITY' in data.upper():
        raise ValueError('Unsafe WSL package manifest')
    root = ET.fromstring(data)
    if root.tag != 'packages':
        raise ValueError('Invalid WSL package manifest')
    matches = [entry.get('version') for entry in root if entry.tag == 'package' and entry.get('id') == 'Microsoft.WSL.Kernel']
    if len(matches) != 1 or not isinstance(matches[0], str) or not KERNEL.fullmatch(matches[0]):
        raise ValueError('Missing or ambiguous WSL kernel package dependency')
    return matches[0]


def fetch(url, limit):
    request = Request(url, headers={'User-Agent': 'WSX-kernel-builder', 'Accept': 'application/vnd.github+json'})
    with urlopen(request, timeout=60) as response:
        data = response.read(limit + 1)
    if len(data) > limit:
        raise ValueError('Upstream response exceeds limit')
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    releases = json.loads(fetch(RELEASES, 8 * 1024 * 1024))
    tag = latest_preview(releases)
    refs = subprocess.check_output(['git', 'ls-remote', '--tags', REPOSITORY, 'refs/tags/' + tag, 'refs/tags/' + tag + '^{}'], text=True, timeout=120)
    commit = tag_commit(refs, tag)
    url = 'https://raw.githubusercontent.com/microsoft/WSL/' + commit + '/packages.config'
    data = fetch(url, 1024 * 1024)
    package = kernel_dependency(data)
    result = {'schema_version': 1, 'wsl_preview_tag': tag, 'wsl_source_commit': commit,
              'package_manifest_url': url, 'package_manifest_sha256': hashlib.sha256(data).hexdigest(),
              'kernel_package_version': package, 'kernel_source_verified': False,
              'module_built': False, 'runtime_admitted': False,
              'scope': 'Preview dependency discovery only; binary/source correspondence and module build are not established'}
    with args.output.open('x', encoding='utf-8') as output:
        json.dump(result, output, indent=2)
        output.write('\n')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
