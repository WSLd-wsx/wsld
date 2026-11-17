#!/usr/bin/env python3
"""Build a diagnostic release index keyed by required kernel symbol CRCs."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shutil
import tempfile

HERE = Path(__file__).resolve().parent


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, HERE / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


release = load('crc_release_assets', 'export-kernel-release-assets.py')
crc = load('crc_module_inspection', 'compare-kernel-module-crcs.py')
HEX = re.compile(r'[0-9a-f]{64}\Z')


def requirements(directory, modprobe):
    imports, providers = {}, {}
    for name in crc.MODULES:
        path = directory / name
        imports[name] = crc.entries(modprobe, '--show-modversions', path)
        if not imports[name] or 'module_layout' not in imports[name]:
            raise ValueError('Missing versioned imports or module_layout: ' + name)
        for symbol, value in crc.entries(modprobe, '--show-exports', path).items():
            if symbol in providers:
                raise ValueError('Duplicate module export: ' + symbol)
            providers[symbol] = value
    needed = {}
    for name, symbols in imports.items():
        for symbol, value in symbols.items():
            if symbol in providers:
                if providers[symbol] != value:
                    raise ValueError('Module export CRC mismatch: ' + symbol)
                continue
            if symbol in needed and needed[symbol] != value:
                raise ValueError('Conflicting kernel import CRC: ' + symbol)
            needed[symbol] = value
    return dict(sorted(needed.items()))


def fingerprint(symbols):
    return hashlib.sha256(json.dumps(symbols, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def build(legacy_index, assets_directory, modprobe):
    legacy = release.validate(legacy_index)
    entries = {}
    with tempfile.TemporaryDirectory(prefix='wsx-crc-index-') as temporary:
        root = Path(temporary)
        for group, asset in sorted(legacy['groups'].items()):
            archive = assets_directory / asset['asset']
            if (archive.is_symlink() or not archive.is_file() or archive.stat().st_size != asset['bytes']
                    or release.groups.package.digest(archive) != asset['sha256']):
                raise ValueError('Module archive does not match the release index')
            directory = root / group
            release.groups.registry.extract_group(archive, directory, group)
            symbols = requirements(directory, modprobe)
            identity = fingerprint(symbols)
            if identity in entries:
                raise ValueError('Multiple module archives have the same kernel CRC requirements')
            entries[identity] = {'symbols': symbols, 'asset': asset}
    return {'schema_version': 2, 'generation': legacy['generation'], 'entries': entries,
            'build_inputs': legacy['build_inputs'], 'evidence': legacy['evidence'],
            'release_ready': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--index', type=Path, required=True)
    parser.add_argument('--assets', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists() or args.output.is_symlink():
        raise ValueError('Output index already exists')
    modprobe = shutil.which('modprobe')
    if not modprobe:
        raise ValueError('modprobe is required')
    result = build(json.loads(args.index.read_text()), args.assets, modprobe)
    args.output.write_text(json.dumps(result, sort_keys=True, separators=(',', ':')) + '\n')
    print(json.dumps({'entries': len(result['entries']), 'release_ready': False}))


if __name__ == '__main__':
    main()
