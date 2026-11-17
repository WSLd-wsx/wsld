#!/usr/bin/env python3
"""Install the pinned kernel.org compiler into a new isolated build root."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

URL = 'https://mirrors.edge.kernel.org/pub/tools/crosstool/files/bin/x86_64/13.2.0/x86_64-gcc-13.2.0-nolibc-x86_64-linux.tar.xz'
SHA256 = '2f1975496a15200f2aaef4e0998c530bbcba6ca237285be76f030087da563fb6'


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def tree_manifest(target):
    entries = {}
    for path in sorted(target.rglob('*')):
        name = path.relative_to(target).as_posix()
        if path.is_symlink():
            resolved = path.resolve(strict=True)
            if not resolved.is_relative_to(target.resolve()):
                raise ValueError('Compiler symlink escapes cache')
            entries[name] = {'symlink': os.readlink(path)}
        elif path.is_file():
            entries[name] = {'sha256': digest(path), 'mode': path.stat().st_mode & 0o777}
        elif not path.is_dir():
            raise ValueError('Unsafe compiler cache file')
    return entries


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--reuse', action='store_true', help='Verify and reuse a previously extracted compiler')
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    target = root / 'opt/wsx-gcc'
    if (target.exists() or target.is_symlink()) and not args.reuse:
        raise ValueError('Compiler target already exists')
    if not args.archive.exists():
        args.archive.parent.mkdir(parents=True, exist_ok=True)
        partial = args.archive.with_name(args.archive.name + '.partial')
        if partial.exists():
            raise ValueError('Compiler partial download already exists')
        subprocess.run(['curl', '-fsSL', '--retry', '3', '--max-time', '900', URL, '-o', str(partial)], check=True)
        if digest(partial) != SHA256:
            raise ValueError('Compiler download hash mismatch; partial retained')
        partial.rename(args.archive)
    if not args.archive.is_file() or args.archive.is_symlink() or digest(args.archive) != SHA256:
        raise ValueError('Compiler archive hash mismatch')
    manifest = root / 'wsx-gcc-manifest.json'
    if target.exists() or target.is_symlink():
        if (target.is_symlink() or not target.is_dir() or manifest.is_symlink() or not manifest.is_file()
                or json.loads(manifest.read_text()) != {'archive_sha256': SHA256, 'files': tree_manifest(target)}):
            raise ValueError('Compiler cache differs from verified extraction')
        print(f'Reused pinned GCC 13.2.0: {target}')
        return
    if manifest.exists() or manifest.is_symlink():
        raise ValueError('Incomplete compiler cache; retained for inspection')
    target.mkdir(parents=True, exist_ok=False)
    subprocess.run(['tar', '-xJf', str(args.archive.resolve()), '--no-same-owner', '--strip-components=2', '-C', str(target)], check=True)
    if not (target / 'bin/x86_64-linux-gcc').is_file():
        raise ValueError('Pinned compiler archive did not contain the expected compiler')
    if args.reuse:
        with manifest.open('x', encoding='utf-8') as stream:
            json.dump({'archive_sha256': SHA256, 'files': tree_manifest(target)}, stream, sort_keys=True)
            stream.write('\n')
    print(f'Prepared pinned GCC 13.2.0: {target}')


if __name__ == '__main__':
    main()
