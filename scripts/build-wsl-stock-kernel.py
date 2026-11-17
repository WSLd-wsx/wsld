#!/usr/bin/env python3
"""Prepare the pinned reference kernel tree and export its module CRCs."""
import argparse
import hashlib
from pathlib import Path
import subprocess

CONFIG = '350bb895c0af6c4dbb90a596385baaa4cfb58dcc0c36c9be166edc9517bac760'
BUILT_CONFIG = '3b40bc0f4c90a824a133d72f7960be923ce5c43c01a52476ac74848c4e8ad5d5'
SYMVERS = '3a318755e6fd473e964b3ea4d7aa6e75395539ecb7fb6f87efa0b8a53d52399c'


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel-tree', type=Path, required=True)
    parser.add_argument('--jobs', type=int, default=4)
    args = parser.parse_args()
    if not 1 <= args.jobs <= 32:
        raise ValueError('Invalid job count')
    kernel = args.kernel_tree.resolve(strict=True)
    if digest(kernel / '.config') != CONFIG or (kernel / 'Module.symvers').exists():
        raise ValueError('Kernel tree is not a fresh reference source and configuration')
    prefix = '/opt/wsx-gcc/bin/x86_64-linux-'
    overrides = [f'{key}={prefix}{tool}' for key, tool in
                 [('CC', 'gcc'), ('LD', 'ld'), ('AR', 'ar'), ('NM', 'nm'),
                  ('OBJCOPY', 'objcopy'), ('OBJDUMP', 'objdump'), ('STRIP', 'strip')]]
    base = ['make', '-C', str(kernel), f'-j{args.jobs}', 'LOCALVERSION=', *overrides]
    subprocess.run(base + ['olddefconfig'], check=True)
    if digest(kernel / '.config') != BUILT_CONFIG:
        raise ValueError('Prepared kernel configuration differs from the tested configuration')
    subprocess.run(base + ['vmlinux'], check=True)
    source = kernel / 'vmlinux.symvers'
    if digest(source) != SYMVERS:
        raise ValueError('Built kernel symbol CRCs differ from the reviewed reference')
    subprocess.run(base + ['modules_prepare'], check=True)
    (kernel / 'Module.symvers').write_bytes(source.read_bytes())
    if digest(kernel / '.config') != BUILT_CONFIG or digest(kernel / 'Module.symvers') != SYMVERS:
        raise ValueError('Prepared tree changed the reviewed config or symbol CRCs')
    print('Pinned reference kernel build tree and Module.symvers prepared')


if __name__ == '__main__':
    main()
