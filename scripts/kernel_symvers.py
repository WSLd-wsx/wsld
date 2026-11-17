#!/usr/bin/env python3
"""Derive Module.symvers from a build-ID-matched WSL kernel image."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy

ROOT = Path(__file__).resolve().parents[1]
EXTRACTOR = ROOT / 'core/guest/kernel-exports.py'


def format_symvers(symbols):
    if not symbols:
        raise ValueError('Kernel has no versioned exports')
    lines = []
    for name, item in sorted(symbols.items()):
        if (not name or any(ch.isspace() for ch in name) or
                any(ch.isspace() for ch in item['namespace']) or
                not isinstance(item['crc'], int) or isinstance(item['crc'], bool) or not 0 <= item['crc'] <= 0xffffffff or
                item['export'] not in ('EXPORT_SYMBOL', 'EXPORT_SYMBOL_GPL')):
            raise ValueError('Unsafe kernel symbol metadata')
        lines.append(f"0x{item['crc']:08x}\t{name}\tvmlinux\t{item['export']}\t{item['namespace']}\n")
    return ''.join(lines).encode('ascii')


def derive(image_path, notes_path):
    extractor = runpy.run_path(str(EXTRACTOR))
    image = extractor['bounded'](image_path)
    notes = extractor['bounded'](notes_path)
    symbols, build_id, elf_hash = extractor['exports_detailed'](image, notes)
    data = format_symvers(symbols)
    evidence = {'schema_version': 1, 'kernel_build_id': build_id,
                'kernel_image_sha256': hashlib.sha256(image).hexdigest(), 'vmlinux_sha256': elf_hash,
                'export_count': len(symbols), 'symvers_sha256': hashlib.sha256(data).hexdigest(),
                'extractor_sha256': hashlib.sha256(EXTRACTOR.read_bytes()).hexdigest(),
                'scope': 'Build-ID-matched image exports only; source provenance, module loading and GPU behavior are not proved'}
    return data, evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel-image', required=True, type=Path)
    parser.add_argument('--kernel-notes', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--evidence', required=True, type=Path)
    args = parser.parse_args()
    if any(p.exists() or p.is_symlink() for p in (args.output, args.evidence)):
        raise ValueError('Output or evidence already exists')
    data, evidence = derive(args.kernel_image, args.kernel_notes)
    with args.output.open('xb') as out:
        out.write(data)
    with args.evidence.open('x') as out:
        json.dump(evidence, out, indent=2)
        out.write('\n')
    print(json.dumps(evidence))


if __name__ == '__main__':
    main()
