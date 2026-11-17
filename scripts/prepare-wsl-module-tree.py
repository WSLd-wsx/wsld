#!/usr/bin/env python3
"""Prepare the pinned reference tree using exports from the actual kernel."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

import kernel_symvers
import kernel_target

CONFIG = '350bb895c0af6c4dbb90a596385baaa4cfb58dcc0c36c9be166edc9517bac760'
BUILT_CONFIG = '3b40bc0f4c90a824a133d72f7960be923ce5c43c01a52476ac74848c4e8ad5d5'


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def config_values(path):
    values = {}
    for line in path.read_text().splitlines():
        match = re.fullmatch(r'(CONFIG_[A-Za-z0-9_]+)=(.*)', line)
        unset = re.fullmatch(r'# (CONFIG_[A-Za-z0-9_]+) is not set', line)
        if match:
            values[match[1]] = match[2]
        elif unset:
            values[unset[1]] = 'n'
    return values


def verify_target_config(target, prepared):
    # Kconfig probes host tool capabilities; these do not describe target ABI.
    ignored = {'CONFIG_CC_VERSION_TEXT', 'CONFIG_CC_CAN_LINK', 'CONFIG_RUSTC_VERSION',
               'CONFIG_RUSTC_LLVM_VERSION', 'CONFIG_PAHOLE_VERSION'}
    changed = sorted(name for name in target.keys() | prepared.keys()
                     if name not in ignored and not name.startswith('CONFIG_RUSTC_HAS_')
                     and target.get(name, 'n') != prepared.get(name, 'n'))
    if changed:
        raise ValueError('Runner configuration differs from target kernel: ' + ', '.join(changed[:40]))


def prepare(kernel, image, notes, jobs, prefix='/opt/wsx-gcc/bin/x86_64-linux-', allow_runner_config=False, target_inspection=None):
    if not 1 <= jobs <= 32:
        raise ValueError('Invalid job count')
    target = kernel_target.identity(kernel, image, target_inspection) if target_inspection else None
    if target and not allow_runner_config:
        raise ValueError('Non-reference target requires explicit diagnostic runner configuration')
    expected_config = target['config_sha256'] if target else CONFIG
    if digest(kernel / '.config') != expected_config or (kernel / 'Module.symvers').exists():
        raise ValueError('Kernel tree is not a fresh reference source and configuration')
    target_config = config_values(kernel / '.config')
    symvers, evidence = kernel_symvers.derive(image, notes)
    compiler = subprocess.check_output([prefix + 'gcc', '-dumpfullversion'], text=True).strip()
    linker = subprocess.check_output([prefix + 'ld', '--version'], text=True).splitlines()[0]
    if compiler != '13.2.0' or not linker.endswith(' 2.41'):
        raise ValueError('Compiler or linker does not match the reviewed toolchain')
    overrides = [f'{key}={prefix}{tool}' for key, tool in
                 [('CC', 'gcc'), ('LD', 'ld'), ('AR', 'ar'), ('NM', 'nm'),
                  ('OBJCOPY', 'objcopy'), ('OBJDUMP', 'objdump'), ('STRIP', 'strip')]]
    base = ['make', '-C', str(kernel), f'-j{jobs}', 'LOCALVERSION=', *overrides]
    subprocess.run(base + ['olddefconfig'], check=True)
    if allow_runner_config:
        verify_target_config(target_config, config_values(kernel / '.config'))
    if digest(kernel / '.config') != BUILT_CONFIG and not allow_runner_config:
        raise ValueError('Prepared kernel configuration differs from the tested configuration')
    if allow_runner_config:
        print('Runner-generated configuration is diagnostic only; release admission is not established', flush=True)
    subprocess.run(base + ['modules_prepare'], check=True)
    expected_release = target['release'] if target else '6.18.58.1-microsoft-standard-WSL2'
    if (kernel / 'include/config/kernel.release').read_text().strip() != expected_release:
        raise ValueError('Prepared kernel release differs from the pinned reference')
    with (kernel / 'Module.symvers').open('xb') as output:
        output.write(symvers)
    with (kernel / 'wsx-symvers-evidence.json').open('x') as output:
        json.dump(evidence, output, indent=2)
        output.write('\n')
    if target:
        target['prepared_config_sha256'] = digest(kernel / '.config')
        target['kernel_notes_sha256'] = kernel_target.digest(notes)
        with (kernel / 'wsx-target-prepared.json').open('x') as output:
            json.dump(target, output, indent=2)
            output.write('\n')
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel-tree', required=True, type=Path)
    parser.add_argument('--kernel-image', required=True, type=Path)
    parser.add_argument('--kernel-notes', required=True, type=Path)
    parser.add_argument('--jobs', type=int, default=4)
    parser.add_argument('--compiler-prefix', default='/opt/wsx-gcc/bin/x86_64-linux-')
    parser.add_argument('--allow-runner-config', action='store_true')
    parser.add_argument('--target-inspection', type=Path)
    args = parser.parse_args()
    print(json.dumps(prepare(args.kernel_tree.resolve(), args.kernel_image, args.kernel_notes, args.jobs, args.compiler_prefix, args.allow_runner_config, args.target_inspection)))


if __name__ == '__main__':
    main()
