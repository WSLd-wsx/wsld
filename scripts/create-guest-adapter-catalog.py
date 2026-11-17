#!/usr/bin/env python3
# Original WSX code, BUSL-1.1. See LICENSING.md.
"""Create a build-time catalog from explicitly supplied adapter bundles.

The caller is responsible for reviewing the input sources and protecting the
catalog as a product build input. This script does not sign or publish it.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import stat

spec = importlib.util.spec_from_file_location('package_guest_adapter', Path(__file__).with_name('package-guest-adapter.py'))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result: raise ValueError('Duplicate manifest key')
        result[key] = value
    return result


def read_file(path, limit):
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or not 0 < info.st_size <= limit:
        raise ValueError('Unsafe or oversized adapter file: ' + str(path))
    data = path.read_bytes()
    if len(data) != info.st_size: raise ValueError('Adapter file changed while reading')
    return data


def catalog(bundles):
    entries = []
    seen = set()
    for implementation, directory in sorted(bundles):
        if implementation not in ('hyprland', 'plasma') or implementation in seen:
            raise ValueError('Invalid or duplicate adapter implementation')
        seen.add(implementation)
        directory = Path(directory)
        info = directory.lstat()
        if not stat.S_ISDIR(info.st_mode): raise ValueError('Unsafe adapter directory')
        raw = read_file(directory/'manifest.json', 65536)
        manifest = json.loads(raw, object_pairs_hook=unique)
        if (not isinstance(manifest, dict) or set(manifest) != {'schema_version', 'kind', 'protocol_major', 'implementation', 'files'}
                or type(manifest['schema_version']) is not int or manifest['schema_version'] != 1
                or manifest['kind'] != 'wsx-guest-adapter' or type(manifest['protocol_major']) is not int
                or manifest['protocol_major'] != 2 or manifest['implementation'] != implementation
                or not isinstance(manifest['files'], dict) or set(manifest['files']) != set(package.FILES)):
            raise ValueError('Invalid adapter manifest')
        total = 0
        for name, record in manifest['files'].items():
            if not isinstance(record, dict) or set(record) != {'bytes', 'sha256'} or type(record['bytes']) is not int:
                raise ValueError('Invalid adapter file record')
            data = read_file(directory/name, package.MAX_FILE)
            total += len(data)
            if len(data) != record['bytes'] or hashlib.sha256(data).hexdigest() != record['sha256']:
                raise ValueError('Adapter file does not match manifest: ' + name)
        if total > package.MAX_TOTAL: raise ValueError('Adapter bundle exceeds limit')
        entries.append({'implementation': implementation, 'manifest_sha256': hashlib.sha256(raw).hexdigest()})
    if not entries: raise ValueError('At least one reviewed adapter bundle is required')
    return {'schema_version': 1, 'adapters': entries}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--hyprland', type=Path)
    parser.add_argument('--plasma', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    bundles = [(name, path) for name, path in (('hyprland', args.hyprland), ('plasma', args.plasma)) if path]
    data = (json.dumps(catalog(bundles), sort_keys=True, separators=(',', ':')) + '\n').encode()
    with args.output.open('xb') as output: output.write(data)
    print('Catalog SHA-256:', hashlib.sha256(data).hexdigest())
    print('Review and secure this build input; no publisher signature is created.')


if __name__ == '__main__': main()
