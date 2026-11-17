#!/usr/bin/env python3
"""Discover versioned kernel packages in Microsoft's public WSL feed."""
import argparse
import json
from pathlib import Path
import re
from urllib.request import Request, urlopen

BASE = 'https://pkgs.dev.azure.com/shine-oss/wsl/_packaging/WslDependencies/nuget/v3/flat2/microsoft.wsl.kernel'
VERSION = re.compile(r'([0-9]+(?:\.[0-9]+){3})-([0-9]+)\Z')
TAG = re.compile(r'linux-msft-wsl-([0-9]+(?:\.[0-9]+){3})\Z')


def candidates(index, tag):
    match = TAG.fullmatch(tag)
    if not match or not isinstance(index, dict) or not isinstance(index.get('versions'), list):
        raise ValueError('Invalid kernel tag or Microsoft feed index')
    seen, found = set(), []
    for version in index['versions']:
        if not isinstance(version, str):
            raise ValueError('Invalid Microsoft package version')
        item = VERSION.fullmatch(version)
        if not item:
            continue
        if version in seen:
            raise ValueError('Duplicate Microsoft package version')
        seen.add(version)
        if item.group(1) == match.group(1):
            found.append({'version': version, 'revision': int(item.group(2)),
                          'url': f'{BASE}/{version}/microsoft.wsl.kernel.{version}.nupkg'})
    return sorted(found, key=lambda value: value['revision'], reverse=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--index-file', type=Path, help='Offline test evidence only')
    args = parser.parse_args()
    if args.output.exists() or args.output.is_symlink():
        raise ValueError('Output already exists')
    if args.index_file:
        raw = args.index_file.read_bytes()
    else:
        request = Request(BASE + '/index.json', headers={'Accept': 'application/json', 'User-Agent': 'WSX-kernel-builder'})
        with urlopen(request, timeout=60) as response:
            raw = response.read(1024 * 1024 + 1)
    if len(raw) > 1024 * 1024:
        raise ValueError('Microsoft feed index exceeds size limit')
    found = candidates(json.loads(raw), args.tag)
    report = {'schema_version': 1, 'tag': args.tag, 'candidates': found,
              'offline': args.index_file is not None, 'source_verified': False,
              'scope': 'Official feed discovery only; package signatures, source correspondence and ABI require verification'}
    with args.output.open('x') as output:
        json.dump(report, output, indent=2)
        output.write('\n')
    print(json.dumps({'tag': args.tag, 'candidates': len(found), 'source_verified': False}))


if __name__ == '__main__':
    main()
