#!/usr/bin/env python3
"""Extract SHA-256 pinned Ubuntu build packages without installing them."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def sha256(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def materialize(package, downloads):
    if Path(package['filename']).name != package['filename'] or package['filename'] in ('.', '..'):
        raise ValueError('Unsafe package filename')
    target = downloads / package['filename']
    if not target.exists() or sha256(target) != package['sha256']:
        partial = target.with_suffix(target.suffix + '.partial')
        url = package['url']
        if url.startswith('http://archive.ubuntu.com/'):
            url = 'https://' + url[len('http://'):]
        if not url.startswith('https://'):
            raise ValueError('Build package URL must use HTTPS')
        subprocess.run(['curl', '-fsSL', '--retry', '3', '--max-time', '300',
                        url, '-o', str(partial)], check=True)
        if partial.stat().st_size != package['size'] or sha256(partial) != package['sha256']:
            raise ValueError(f'Package size/hash mismatch: {package["filename"]}')
        partial.replace(target)
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--lock', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--package-cache', type=Path)
    args = parser.parse_args()
    if sha256(args.lock) != 'b825180dd79831af530902975e53007555af297a384bebe79b8ac34801beffe1':
        raise ValueError('Build package lock digest mismatch')
    packages = json.loads(args.lock.read_text())['packages']
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    downloads = output / 'packages'
    downloads.mkdir(exist_ok=True)
    if args.package_cache:
        for package in packages:
            if Path(package['filename']).name != package['filename']:
                raise ValueError('Unsafe package filename')
            source = args.package_cache / package['filename']
            if source.is_file() and not (downloads / package['filename']).exists():
                shutil.copyfile(source, downloads / package['filename'])
    root = output / 'rootfs'
    if root.exists():
        raise ValueError('Choose an output without an existing rootfs')
    lock = dict(schema=1, packages=packages, method='SHA-256 verified extraction; no maintainer scripts')
    (output / 'build-packages.lock.json').write_text(json.dumps(lock, indent=2) + '\n')
    with ThreadPoolExecutor(max_workers=6) as pool:
        archives = list(pool.map(lambda p: materialize(p, downloads), packages))
    root.mkdir()
    for name in ('bin', 'sbin', 'lib', 'lib64'):
        (root / 'usr' / name).mkdir(parents=True, exist_ok=True)
        (root / name).symlink_to('usr/' + name)
    for archive in archives:
        subprocess.run(['dpkg-deb', '--extract', str(archive), str(root)], check=True)
    # Maintainer scripts normally create these alternatives. Keep extraction
    # deterministic and create only the build tools selected in the lock.
    for name, target in {'cc': 'gcc', 'c++': 'g++', 'sh': 'bash', 'awk': 'gawk',
                         'lex': 'flex', 'yacc': 'bison.yacc', 'pkg-config': 'pkgconf'}.items():
        link = root / 'usr/bin' / name
        if not link.exists() and not link.is_symlink() and (link.parent / target).exists():
            link.symlink_to(target)
    # Build tools require temporary files; no devices or service setup is needed.
    (root / 'tmp').mkdir(exist_ok=True)
    (root / 'tmp').chmod(0o1777)
    (root / 'work').mkdir()
    (root / 'etc').mkdir(exist_ok=True)
    (root / 'etc/passwd').write_text('builder:x:0:0:Build user:/work:/bin/bash\n')
    (root / 'etc/group').write_text('builder:x:0:\n')
    print(f'Prepared {root} from {len(packages)} pinned packages', flush=True)


if __name__ == '__main__':
    main()
