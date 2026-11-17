#!/usr/bin/env python3
"""Record candidate module metadata and internal load order without loading it."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess

MODULES = ('drm_kms_helper.ko', 'virtio_dma_buf.ko', 'virtio-gpu.ko',
           'virtio_msg.ko', 'virtio_msg_vsock.ko', 'wsx_virtio_msg_pci.ko')
NAME = re.compile(r'[A-Za-z0-9_]+\Z')


def metadata(program, path, field):
    result = subprocess.run([program, '-F', field, str(path)], capture_output=True,
                            text=True, timeout=30, check=True)
    if len(result.stdout) > 16384 or '\x00' in result.stdout:
        raise ValueError('Module metadata exceeds limits')
    return result.stdout.strip()


def inspect(directory, program):
    names = {name.removesuffix('.ko').replace('-', '_'): name for name in MODULES}
    records = {}
    for ident, name in names.items():
        path = directory / name
        if path.is_symlink() or not path.is_file():
            raise ValueError('Missing or unsafe module: ' + name)
        with path.open('rb') as source:
            digest = hashlib.file_digest(source, 'sha256').hexdigest()
        depends_text = metadata(program, path, 'depends')
        deps = [] if not depends_text else depends_text.split(',')
        if any(not NAME.fullmatch(dep) for dep in deps) or len(deps) != len(set(deps)):
            raise ValueError('Invalid module dependency metadata: ' + name)
        vermagic = metadata(program, path, 'vermagic')
        license_text = metadata(program, path, 'license')
        if not vermagic or not license_text or '\n' in vermagic:
            raise ValueError('Missing or invalid module loading metadata: ' + name)
        records[ident] = {'file': name, 'sha256': digest, 'depends': deps,
                          'vermagic': vermagic, 'license': license_text,
                          'signer': metadata(program, path, 'signer'),
                          'signature_id': metadata(program, path, 'sig_id'),
                          'import_namespaces': metadata(program, path, 'import_ns').splitlines()}
    remaining = set(records)
    order = []
    while remaining:
        ready = sorted(name for name in remaining if not (set(records[name]['depends']) & remaining))
        if not ready:
            raise ValueError('Module dependency cycle')
        order.extend(ready)
        remaining.difference_update(ready)
    external = sorted({dep for item in records.values() for dep in item['depends'] if dep not in records})
    return {'schema_version': 1, 'modules': records, 'internal_load_order': order,
            'external_dependencies': external, 'runtime_admitted': False,
            'scope': 'Metadata inventory only; kernel configuration, signature enforcement, symbol namespaces, CRCs and actual loading require separate checks'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--modules', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    program = shutil.which('modinfo')
    if not program:
        raise ValueError('modinfo is required')
    result = inspect(args.modules, program)
    with args.output.open('x', encoding='utf-8') as output:
        json.dump(result, output, indent=2)
        output.write('\n')
    print(json.dumps({'modules': len(result['modules']), 'external_dependencies': result['external_dependencies'],
                      'runtime_admitted': False}))


if __name__ == '__main__':
    main()
