#!/usr/bin/env python3
"""Combine complete rebuilt targets without replacing conflicting assets."""
import argparse
import hashlib
from pathlib import Path
import re
import shutil

VERSION = re.compile(r'[0-9]{1,9}(?:\.[0-9]{1,9}){3}-[0-9]{1,9}\Z')


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').digest()


def merge(root, targets, output, run_id):
    if not re.fullmatch(r'[0-9]+-[0-9]+', run_id):
        raise ValueError('Invalid run identity')
    versions = []
    for line in targets.read_text(encoding='utf-8').splitlines():
        parts = line.split('\t')
        if len(parts) != 2 or not VERSION.fullmatch(parts[1]) or parts[1] in versions:
            raise ValueError('Invalid or duplicate target')
        versions.append(parts[1])
    if not versions or root.is_symlink() or not root.is_dir() or output.exists():
        raise ValueError('Missing targets or unsafe output')
    actual = {p.name for p in root.iterdir()}
    if actual != set(versions):
        raise ValueError('Rebuilt target directories differ from discovery')
    output.mkdir()
    for version in versions:
        directory = root / version
        if directory.is_symlink() or not directory.is_dir():
            raise ValueError('Unsafe target directory')
        required = {f'modules_{version}.tar.gz', f'modules_{version}-{run_id}.json',
                    f'source-materials-{version}-{run_id}.json', f'SHA256SUMS-{version}-{run_id}'}
        files = list(directory.iterdir())
        if not required <= {p.name for p in files}:
            raise ValueError('Incomplete rebuilt target: ' + version)
        for source in files:
            if source.is_symlink() or not source.is_file() or not re.fullmatch(r'[A-Za-z0-9._-]+', source.name):
                raise ValueError('Unsafe rebuild asset')
            dest = output / source.name
            if dest.exists():
                if digest(dest) != digest(source):
                    raise ValueError('Conflicting rebuilt asset: ' + source.name)
            else:
                shutil.copyfile(source, dest)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('root', type=Path)
    p.add_argument('targets', type=Path)
    p.add_argument('output', type=Path)
    p.add_argument('run_id')
    a = p.parse_args()
    merge(a.root, a.targets, a.output, a.run_id)
