#!/usr/bin/env python3
"""Prepare a review-only pairing from two independently pinned receipts."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

BASE = 'b3dd55513250a38bc7ca199820cdabc4341e4782c6af53acab567779551c5590'
MODULES = '1853cabe03ffd4ac29eb2e77544f8be5dcaf8aeea4683dd6d2476ad93760a28e'
NAMES = ('drm_kms_helper.ko', 'virtio_dma_buf.ko', 'virtio-gpu.ko',
         'virtio_msg.ko', 'virtio_msg_vsock.ko', 'wsx_virtio_msg_pci.ko')


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def verify(root, name, entry):
    path = root / name
    if not path.is_file() or path.is_symlink() or path.stat().st_size != entry['bytes'] or digest(path) != entry['sha256']:
        raise ValueError(f'Component verification failed: {name}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', type=Path)
    parser.add_argument('modules', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    baseline_receipt = args.baseline / 'verified-files.json'
    module_receipt = args.modules / 'diagnostic-manifest.json'
    if digest(baseline_receipt) != BASE or digest(module_receipt) != MODULES:
        raise ValueError('Input receipt digest mismatch')
    baseline = json.loads(baseline_receipt.read_text())
    module_manifest = json.loads(module_receipt.read_text())
    if module_manifest['guest_module_build_kernel'] != '6.18.58.1-microsoft-standard-WSL2':
        raise ValueError('Unexpected module build kernel')
    for name, entry in baseline.items():
        if name.startswith('/') or any(part in ('', '.', '..') for part in name.split('/')) or '\\' in name or ':' in name:
            raise ValueError('Unsafe component path')
        verify(args.baseline, name, entry)
    for name in NAMES:
        verify(args.modules, 'guest/' + name, module_manifest['files']['guest/' + name])
    args.output.mkdir(parents=True, exist_ok=False)
    (args.output / 'guest').mkdir()
    result = dict(baseline)
    for name in baseline:
        source = args.modules if name in {'guest/' + n for n in NAMES} else args.baseline
        shutil.copyfile(source / name, args.output / name)
        if source == args.modules:
            result[name] = module_manifest['files'][name]
        verify(args.output, name, result[name])
    (args.output / 'verified-files.json').write_text(json.dumps(result, indent=2) + '\n')
    evidence = {'schema_version': 1, 'baseline_receipt_sha256': BASE,
                'module_receipt_sha256': MODULES, 'candidate_receipt_sha256': digest(args.output / 'verified-files.json'),
                'module_build_kernel': module_manifest['guest_module_build_kernel'],
                'runtime_admitted': False, 'hardware_tested': False,
                'scope': 'Review-only combined files; live CRC, pairing and desktop acceptance are required.'}
    (args.output / 'candidate-evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print(json.dumps(evidence, sort_keys=True))


if __name__ == '__main__':
    main()
