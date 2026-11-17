#!/usr/bin/env python3
"""Fingerprint all reviewed module sources and build-time dependency inputs."""
import hashlib
import json
import os
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from kernel_runner_dependencies import PACKAGES

ROOT = Path(__file__).resolve().parents[1]
FILES = ('LICENSE', 'LICENSING.md', 'scripts/build-kernel-modules.py', 'scripts/build-wsl-stock-kernel.py',
         'scripts/kernel_build_generation.py',
         'scripts/kernel_symvers.py', 'scripts/prepare-wsl-module-tree.py',
         'scripts/extract-kernel-notes.py', 'scripts/extract-reference-kernel-package.py',
         'scripts/inspect-wsl-kernel-package.py',
         'scripts/kernel_target.py',
         'scripts/screen-kernel-target.py', 'scripts/select-kernel-module-group.py',
         'scripts/evaluate-kernel-module-group.py', 'scripts/compare-kernel-module-crcs.py',
         'scripts/kernel-module-load-requirements.py',
         'core/guest/kernel-exports.py', 'core/guest/desktop-control.py',
         'core/guest/desktop-display.py', 'core/guest/desktop-observe.py',
         'core/guest/drm-access.py', 'core/guest/kernel-symbol-audit.py',
         'core/guest/module-group.py', 'core/guest/module-reuse.py',
         'scripts/prepare-kernel-build-root.py', 'scripts/prepare-kernel-toolchain.py',
         'scripts/prepare-wsl-kernel-source.py', 'scripts/run-reference-kernel-build.sh',
         'scripts/prepare-target-kernel-source.py',
         'scripts/archive-target-kernel-source.py',
         'scripts/verify-kernel-source-archive.py',
         'scripts/export-kernel-build-evidence.py', 'scripts/review-kernel-source-materials.py',
         'scripts/publish-kernel-target.sh',
         'scripts/merge-kernel-rebuild-assets.py', 'scripts/publish-kernel-rebuild.sh',
         'scripts/build-target-kernel-modules.sh',
         'scripts/build-coverage-target.py',
         'scripts/kernel-release-coverage.py', 'scripts/bootstrap-kernel-snapshot.py',
         'scripts/kernel-package-registry.py',
         'scripts/assemble-kernel-snapshot.py', 'scripts/kernel-package.py',
         'scripts/export-kernel-build-inputs.py',
         'scripts/export-kernel-module-sources.py', 'scripts/export-kernel-release-assets.py',
         'scripts/export-kernel-groups.py', 'scripts/attach-kernel-source-evidence.py',
         'scripts/discover-kernel-series.py',
         'scripts/kernel_release_selection.py', 'scripts/select-kernel-release.py',
         'scripts/export-kernel-buildid-asset.py',
         'scripts/kernel-staging-snapshots.py',
         'scripts/capture-kernel-build-dependencies.py',
         'scripts/kernel_runner_dependencies.py',
         'scripts/fetch-target-kernel-package.py', 'scripts/wsl-kernel-feed.py',
         'scripts/finalize-kernel-build.py', 'scripts/runner-dns-fallback.sh',
         '.github/workflows/kernel-modules-build.yml')
TREES = ('kernel-modules',)


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def generation(root):
    paths = [root / item for item in FILES]
    for name in TREES:
        directory = root / name
        if directory.is_symlink() or not directory.is_dir():
            raise ValueError('Build input directory is missing or unsafe: ' + name)
        for path in directory.rglob('*'):
            if path.is_symlink() or not (path.is_file() or path.is_dir()):
                raise ValueError('Unsafe build input: ' + str(path))
            if path.is_file():
                paths.append(path)
    records = {}
    for path in sorted(paths):
        if path.is_symlink() or not path.is_file():
            raise ValueError('Build input is missing or unsafe: ' + str(path))
        records[path.relative_to(root).as_posix()] = digest(path)
    dependency_path = os.environ.get('WSX_KERNEL_RUNNER_DEPENDENCIES')
    if dependency_path:
        path = Path(dependency_path)
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 65536:
            raise ValueError('Runner dependency evidence is missing or unsafe')
        evidence = json.loads(path.read_text(encoding='utf-8'))
        expected = set(PACKAGES)
        packages = evidence.get('packages') if isinstance(evidence, dict) else None
        if (not isinstance(evidence, dict) or set(evidence) != {'schema_version', 'packages'}
                or evidence['schema_version'] != 1 or not isinstance(packages, dict)
                or set(packages) != expected
                or any(not isinstance(v, str) or not v or '\n' in v for v in packages.values())):
            raise ValueError('Invalid Runner dependency evidence')
        encoded_dependencies = json.dumps(evidence, sort_keys=True, separators=(',', ':')).encode()
        records['@runner-dependencies'] = hashlib.sha256(encoded_dependencies).hexdigest()
    encoded = json.dumps({'schema_version': 1, 'inputs': records}, sort_keys=True, separators=(',', ':')).encode()
    return hashlib.sha256(encoded).hexdigest(), records


if __name__ == '__main__':
    fingerprint, inputs = generation(ROOT)
    print(json.dumps({'schema_version': 1, 'generation': fingerprint, 'inputs': inputs}, sort_keys=True))
