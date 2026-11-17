#!/usr/bin/env python3
"""Export deterministic shared module source and essential build dependencies."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import tarfile

SCRIPTS = (
    'LICENSE', 'LICENSING.md',
    'scripts/build-kernel-modules.py', 'scripts/build-target-kernel-modules.sh',
    'scripts/prepare-wsl-module-tree.py', 'scripts/prepare-target-kernel-source.py',
    'scripts/prepare-kernel-toolchain.py', 'scripts/inspect-wsl-kernel-package.py',
    'scripts/kernel_target.py', 'scripts/kernel_symvers.py', 'scripts/extract-kernel-notes.py',
    'scripts/finalize-kernel-build.py', 'scripts/compare-kernel-module-crcs.py',
    'scripts/kernel_runner_dependencies.py', 'core/guest/kernel-exports.py',
    'core/guest/desktop-control.py', 'core/guest/desktop-display.py',
    'core/guest/desktop-observe.py', 'core/guest/drm-access.py',
    'core/guest/kernel-symbol-audit.py', 'core/guest/module-group.py',
    'core/guest/module-reuse.py',
    'scripts/archive-target-kernel-source.py', 'scripts/export-kernel-build-evidence.py',
    'scripts/verify-kernel-source-archive.py', 'scripts/review-kernel-source-materials.py',
)


def export(root, dependencies, output):
    root = root.resolve(strict=True)
    if output.exists() or output.is_symlink():
        raise ValueError('Source archive output already exists')
    tree = root / 'kernel-modules'
    if tree.is_symlink() or not tree.is_dir():
        raise ValueError('Missing module source tree')
    paths = [root / name for name in SCRIPTS]
    for path in tree.rglob('*'):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError('Unsafe module source tree entry')
        if path.is_file(): paths.append(path)
    payloads = {}
    for path in sorted(paths):
        if path.is_symlink() or not path.is_file(): raise ValueError('Missing or unsafe build dependency: ' + str(path))
        payloads['inputs/' + path.relative_to(root).as_posix()] = path.read_bytes()
    if dependencies.is_symlink() or not dependencies.is_file() or dependencies.stat().st_size > 65536:
        raise ValueError('Invalid runner dependency evidence')
    deps = json.loads(dependencies.read_text())
    payloads['runner-dependencies.json'] = json.dumps(deps, sort_keys=True, separators=(',', ':')).encode()
    manifest = {'schema_version': 1, 'files': {name: hashlib.sha256(data).hexdigest() for name, data in sorted(payloads.items())},
                'corresponding_source_complete': False,
                'scope': 'Module sources, selected build dependencies and integration scripts at build revision; target kernel source, actual installation scripts and full correspondence require separate verification'}
    payloads['manifest.json'] = (json.dumps(manifest, sort_keys=True, separators=(',', ':')) + '\n').encode()
    with output.open('xb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as gz:
        with tarfile.open(fileobj=gz, mode='w', format=tarfile.PAX_FORMAT) as tar:
            for name, data in sorted(payloads.items()):
                info = tarfile.TarInfo(name); info.size = len(data); info.mode = 0o644
                info.uid = info.gid = info.mtime = 0
                tar.addfile(info, io.BytesIO(data))
    with tarfile.open(output, 'r:gz') as tar:
        for name, data in payloads.items():
            stream = tar.extractfile(name)
            if stream is None or stream.read() != data: raise ValueError('Source archive verification failed')
    digest = hashlib.sha256(output.read_bytes()).hexdigest()
    return {'asset': 'module-sources-' + digest + '.tar.gz', 'sha256': digest,
            'bytes': output.stat().st_size, 'corresponding_source_complete': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--dependencies', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--descriptor', type=Path, required=True)
    args = parser.parse_args()
    result = export(args.root, args.dependencies, args.output)
    args.descriptor.write_text(json.dumps(result, sort_keys=True) + '\n')
    print(json.dumps(result, sort_keys=True))

if __name__ == '__main__': main()
