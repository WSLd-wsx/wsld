#!/usr/bin/env python3
"""Compare candidate module imports with a recorded kernel export table."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

MODULES = ('drm_kms_helper.ko', 'virtio_dma_buf.ko', 'virtio-gpu.ko',
           'virtio_msg.ko', 'virtio_msg_vsock.ko', 'wsx_virtio_msg_pci.ko')


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def entries(program, option, file):
    result = subprocess.run([program, option, str(file)], text=True, capture_output=True, timeout=30)
    if result.returncode:
        if option == '--show-exports' and 'No data available' in result.stderr:
            return {}
        raise ValueError(f'Module inspection failed for {file.name}: {result.stderr.strip()}')
    values = {}
    for line in result.stdout.splitlines():
        crc, name = line.split()
        if name in values or not crc.startswith('0x'):
            raise ValueError('Invalid or duplicate module symbol')
        values[name] = int(crc, 16)
    return values


def compare(kernel, directory, program):
    if not isinstance(kernel.get('symbols'), dict) or not kernel.get('kernel_build_id'):
        raise ValueError('Kernel export evidence is incomplete')
    available = {}
    for name, crc in kernel['symbols'].items():
        value = int(crc, 16) if isinstance(crc, str) else crc
        if not isinstance(value, int) or isinstance(value, bool) or not 0 <= value <= 0xffffffff:
            raise ValueError('Invalid kernel symbol CRC')
        available[name] = value
    modules, providers = {}, {}
    for name in MODULES:
        path = directory / name
        if path.is_symlink() or not path.is_file():
            raise ValueError(f'Missing or unsafe module: {name}')
        modules[name] = {'sha256': digest(path), 'imports': entries(program, '--show-modversions', path)}
        for symbol, crc in entries(program, '--show-exports', path).items():
            if symbol in available:
                raise ValueError(f'Duplicate kernel or module export: {symbol}')
            available[symbol] = crc
            providers[symbol] = name
    for name, record in modules.items():
        imports = record.pop('imports')
        record['import_count'] = len(imports)
        record['has_module_layout'] = 'module_layout' in imports
        record['module_symbol_providers'] = sorted({providers[symbol] for symbol in imports if symbol in providers})
        record['missing'] = sorted(set(imports) - set(available))
        record['mismatches'] = sorted(name for name, crc in imports.items() if name in available and available[name] != crc)
    matched = all(not record['missing'] and not record['mismatches'] for record in modules.values())
    return {'schema_version': 1, 'kernel_build_id': kernel['kernel_build_id'], 'modules': modules,
            'crc_match': matched, 'runtime_admitted': False,
            'scope': 'Static symbol CRC comparison only; kernel image provenance, loader, protocol and GPU behavior are not proved'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel-exports', required=True, type=Path)
    parser.add_argument('--modules', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    program = shutil.which('modprobe')
    if not program:
        raise ValueError('modprobe is required to inspect module symbol versions')
    kernel = json.loads(args.kernel_exports.read_text())
    result = compare(kernel, args.modules, program)
    if args.output.exists():
        raise ValueError('Output evidence already exists')
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'crc_match': result['crc_match'], 'runtime_admitted': False}))
    raise SystemExit(0 if result['crc_match'] else 1)


if __name__ == '__main__':
    main()
