#!/usr/bin/env python3
"""Prepare the pinned WSL kernel source for an isolated reference build."""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
COMMIT = 'a989171929bed4ea681be8d77333845ce1c1732a'
ARCHIVE_SHA256 = 'eb3c8d6157de4ac843b35f79f6043cb82583325a635bd19428af3de5b0641ad7'
CONFIG_SHA256 = '350bb895c0af6c4dbb90a596385baaa4cfb58dcc0c36c9be166edc9517bac760'
URL = 'https://codeload.github.com/microsoft/WSL2-Linux-Kernel/tar.gz/' + COMMIT


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    if args.output.exists():
        raise ValueError('Kernel source output already exists')
    if not args.archive.exists():
        args.archive.parent.mkdir(parents=True, exist_ok=True)
        partial = args.archive.with_name(args.archive.name + '.partial')
        if partial.exists():
            raise ValueError('Kernel archive partial already exists')
        subprocess.run(['curl', '-fsSL', '--retry', '3', '--max-time', '1800', URL, '-o', str(partial)], check=True)
        if digest(partial) != ARCHIVE_SHA256:
            raise ValueError('Kernel source archive digest mismatch; partial retained')
        partial.rename(args.archive)
    if not args.archive.is_file() or args.archive.is_symlink() or digest(args.archive) != ARCHIVE_SHA256:
        raise ValueError('Kernel source archive digest mismatch')
    config = ROOT / 'kernel-modules/build-inputs/wsl-6.18.58.1.config'
    if digest(config) != CONFIG_SHA256:
        raise ValueError('Kernel configuration digest mismatch')
    args.output.mkdir(parents=True, exist_ok=False)
    subprocess.run(['tar', '-xzf', str(args.archive.resolve()), '--no-same-owner', '--strip-components=1', '-C', str(args.output)], check=True)
    shutil.copyfile(config, args.output / '.config')
    print(f'Prepared pinned WSL kernel {COMMIT}: {args.output}')


if __name__ == '__main__':
    main()
