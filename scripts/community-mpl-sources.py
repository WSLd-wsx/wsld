#!/usr/bin/env python3
"""Collect and verify MPL source archives used by the community build."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import tarfile
import tomllib
import zipfile

CRATES_IO = 'registry+https://github.com/rust-lang/crates.io-index'
LINE = re.compile(r'^([\w-]+) v([\w.+-]+)(?: \([^\r\n]*\))?\|(.+?)(?: \(\*\))?$')
MPL = re.compile(r'(?<![A-Za-z0-9.-])MPL-2\.0(?:\+)?(?![A-Za-z0-9.-])')
HEX = re.compile(r'[0-9a-f]{64}\Z')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def selection(lock_bytes, tree_bytes):
    lock = tomllib.loads(lock_bytes.decode('utf-8'))
    packages = {}
    for item in lock.get('package', []):
        key = (item.get('name'), item.get('version'))
        packages.setdefault(key, []).append(item)
    selected = {}
    lines = tree_bytes.decode('utf-8-sig').splitlines()
    if not lines:
        raise ValueError('Empty Cargo dependency tree')
    for line in lines:
        if not line.strip():
            continue
        match = LINE.fullmatch(line.strip())
        if not match:
            raise ValueError('Unrecognized Cargo dependency tree entry: ' + line)
        name, version, license_expression = match.groups()
        if license_expression.strip().lower() in ('unknown', '[unknown]', '(unknown)'):
            raise ValueError('Dependency license is unknown: ' + name + ' ' + version)
        if not MPL.search(license_expression):
            continue
        matches = packages.get((name, version), [])
        if (len(matches) != 1 or matches[0].get('source') != CRATES_IO
                or not isinstance(matches[0].get('checksum'), str) or not HEX.fullmatch(matches[0]['checksum'])):
            raise ValueError('MPL dependency is not a uniquely locked crates.io archive: ' + name + ' ' + version)
        key = (name, version)
        record = {'name': name, 'version': version, 'license': license_expression,
                  'registry': CRATES_IO, 'crate_sha256': matches[0]['checksum']}
        if key in selected and selected[key] != record:
            raise ValueError('Conflicting MPL dependency metadata: ' + name)
        selected[key] = record
    return [selected[key] for key in sorted(selected)]


def valid_path(name):
    parts = name.split('/')
    return bool(name and not name.startswith('/') and '\\' not in name and ':' not in name
                and all(part not in ('', '.', '..') for part in parts))


def crate_files(data, prefix):
    result = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as tar:
        for member in tar:
            name = member.name.rstrip('/')
            if not valid_path(name) or not name.startswith(prefix + '/'):
                raise ValueError('Invalid crate archive path')
            if member.isdir():
                continue
            if not member.isfile() or member.size > 64 * 1024 * 1024:
                raise ValueError('Unsupported crate archive member')
            rel = name[len(prefix) + 1:]
            if rel in result:
                raise ValueError('Duplicate crate archive path')
            stream = tar.extractfile(member)
            if stream is None:
                raise ValueError('Unreadable crate archive member')
            result[rel] = sha(stream.read())
    if not result:
        raise ValueError('Empty crate archive')
    return result


def snapshot_tree(path):
    result = {}
    if path.is_symlink() or not path.is_dir():
        raise ValueError('Missing or unsafe Cargo registry source')
    for item in sorted(path.rglob('*')):
        if item.is_symlink() or not (item.is_file() or item.is_dir()):
            raise ValueError('Unsafe registry source entry')
        if item.is_file():
            rel = item.relative_to(path).as_posix()
            if rel in ('.cargo-ok', '.cargo-checksum.json'):
                continue
            if not valid_path(rel) or item.stat().st_size > 64 * 1024 * 1024:
                raise ValueError('Invalid registry source entry')
            result[rel] = item.read_bytes()
    return result


def make_snapshot(files):
    output = io.BytesIO()
    with zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED) as z:
        for name, data in sorted(files.items()):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, data)
    return output.getvalue()


def source_guide(records):
    lines = ['# MPL-2.0 dependency sources', '',
             'The source archives below are included with this distribution. Each original crate is checked against the community Cargo.lock checksum.',
             'Files ending in `.crate` are gzip-compressed tar archives; extract them using a standard tar tool.',
             'Where a source snapshot is listed, it contains the actual modified source used by the build and supplements the original crate.', '',
             'The [Cargo lock file](mpl-sources/Cargo.lock) and [target dependency graph](mpl-sources/dependency-tree.txt) are included for verification.', '',
             '| Package | Version | License expression | SHA-256 | Source |', '| --- | --- | --- | --- | --- |']
    for record in records:
        links = '[original](mpl-sources/' + record['archive'] + ')'
        if record.get('actual_source'):
            links += ' and [modified source](mpl-sources/' + record['actual_source'] + ') (`' + record['actual_source_sha256'] + '`)'
        lines.append('| {name} | {version} | {license} | `{crate_sha256}` | {links} |'.format(**record, links=links))
    return ('\n'.join(lines) + '\n').encode('utf-8')


def collect(lock_path, tree_path, registry_src, registry_cache, output):
    if output.exists():
        raise ValueError('MPL source destination already exists')
    lock_bytes, tree_bytes = lock_path.read_bytes(), tree_path.read_bytes()
    records = selection(lock_bytes, tree_bytes)
    payload = {'Cargo.lock': lock_bytes, 'dependency-tree.txt': tree_bytes}
    for record in records:
        ident = record['name'] + '-' + record['version']
        archive = (registry_cache / (ident + '.crate')).read_bytes()
        if sha(archive) != record['crate_sha256']:
            raise ValueError('Cargo.lock checksum mismatch: ' + ident)
        original = crate_files(archive, ident)
        actual = snapshot_tree(registry_src / ident)
        actual_hashes = {name: sha(data) for name, data in actual.items()}
        changes = sorted(name for name in set(original) | set(actual_hashes) if original.get(name) != actual_hashes.get(name))
        archive_name = 'crates/' + ident + '.crate'
        payload[archive_name] = archive
        record['archive'] = archive_name
        record['changes'] = changes
        if changes:
            snapshot = make_snapshot(actual)
            record['actual_source'] = 'modified/' + ident + '.zip'
            record['actual_source_sha256'] = sha(snapshot)
            payload[record['actual_source']] = snapshot
    manifest = {'schema_version': 1, 'target': 'x86_64-pc-windows-msvc', 'dependency_edges': 'normal,build',
                'cargo_lock_sha256': sha(lock_bytes), 'dependency_tree_sha256': sha(tree_bytes), 'packages': records}
    output.mkdir(parents=True)
    for name, data in payload.items():
        target = output / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n', encoding='utf-8', newline='\n')
    (output.parent / 'MPL-SOURCES.md').write_bytes(source_guide(records))
    return manifest


def verify(lock_path, tree_path, package, materials):
    lock_bytes, tree_bytes = lock_path.read_bytes(), tree_path.read_bytes()
    expected = selection(lock_bytes, tree_bytes)
    with zipfile.ZipFile(package) as p, zipfile.ZipFile(materials) as m:
        prefix = 'gui-third-party/'
        manifest = json.loads(p.read(prefix + 'mpl-sources/manifest.json'))
        if (manifest.get('schema_version') != 1 or manifest.get('target') != 'x86_64-pc-windows-msvc'
                or manifest.get('dependency_edges') != 'normal,build' or manifest.get('cargo_lock_sha256') != sha(lock_bytes)
                or manifest.get('dependency_tree_sha256') != sha(tree_bytes) or not isinstance(manifest.get('packages'), list)):
            raise ValueError('MPL manifest does not match lock and actual dependency tree')
        records = manifest['packages']
        if len(records) != len(expected):
            raise ValueError('MPL source set differs from dependency tree')
        if p.read(prefix + 'MPL-SOURCES.md') != source_guide(records):
            raise ValueError('MPL source guide differs from manifest')
        names = [prefix + 'MPL-SOURCES.md', prefix + 'mpl-sources/manifest.json',
                 prefix + 'mpl-sources/Cargo.lock', prefix + 'mpl-sources/dependency-tree.txt']
        if p.read(prefix + 'mpl-sources/Cargo.lock') != lock_bytes or p.read(prefix + 'mpl-sources/dependency-tree.txt') != tree_bytes:
            raise ValueError('Delivered MPL lock or dependency tree differs from build')
        for record, item in zip(records, expected):
            if any(record.get(key) != value for key, value in item.items()):
                raise ValueError('MPL dependency identity mismatch')
            ident = item['name'] + '-' + item['version']
            archive_name = 'crates/' + ident + '.crate'
            if record.get('archive') != archive_name:
                raise ValueError('Unexpected MPL archive path')
            path = prefix + 'mpl-sources/' + archive_name
            data = p.read(path)
            if sha(data) != item['crate_sha256']:
                raise ValueError('MPL archive checksum mismatch: ' + ident)
            original = crate_files(data, ident)
            names.append(path)
            changes = record.get('changes')
            if not isinstance(changes, list) or any(not isinstance(name, str) or not valid_path(name) for name in changes):
                raise ValueError('Invalid MPL change list')
            if changes:
                snapshot_name = 'modified/' + ident + '.zip'
                if record.get('actual_source') != snapshot_name:
                    raise ValueError('Missing modified MPL source snapshot')
                snapshot_path = prefix + 'mpl-sources/' + snapshot_name
                snapshot = p.read(snapshot_path)
                if sha(snapshot) != record.get('actual_source_sha256'):
                    raise ValueError('Modified MPL snapshot hash mismatch')
                with zipfile.ZipFile(io.BytesIO(snapshot)) as z:
                    entries = z.namelist()
                    if len(entries) != len(set(entries)) or any(not valid_path(name) for name in entries):
                        raise ValueError('Unsafe modified MPL source snapshot')
                    actual = {name: sha(z.read(name)) for name in entries}
                calculated = sorted(name for name in set(original) | set(actual) if original.get(name) != actual.get(name))
                if changes != calculated:
                    raise ValueError('Modified MPL source change list mismatch')
                names.append(snapshot_path)
            elif 'actual_source' in record or 'actual_source_sha256' in record:
                raise ValueError('Unexpected modified MPL source snapshot')
        for name in names:
            if p.read(name) != m.read('notices/' + name):
                raise ValueError('MPL material differs between package and material archive: ' + name)
    return len(expected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    collect_parser = sub.add_parser('collect')
    for name in ('lock', 'tree', 'registry-src', 'registry-cache', 'output'):
        collect_parser.add_argument('--' + name, type=Path, required=True)
    verify_parser = sub.add_parser('verify')
    for name in ('lock', 'tree', 'package', 'materials'):
        verify_parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'collect':
        print(json.dumps(collect(args.lock, args.tree, args.registry_src, args.registry_cache, args.output), sort_keys=True))
    else:
        print('Verified MPL dependency sources:', verify(args.lock, args.tree, args.package, args.materials))


if __name__ == '__main__':
    main()
