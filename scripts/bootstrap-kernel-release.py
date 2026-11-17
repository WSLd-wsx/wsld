#!/usr/bin/env python3
"""Restore and validate a kernel snapshot from a dedicated release index."""
import argparse
import importlib.util
import json
from pathlib import Path
import tempfile

HERE = Path(__file__).resolve().parent


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, HERE / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


download = load('release_bootstrap_download', 'kernel-release-download.py')
bootstrap = load('release_bootstrap_snapshot', 'bootstrap-kernel-snapshot.py')
staging = load('release_bootstrap_staging', 'kernel-staging-snapshots.py')


def restore(root, opener=None):
    root = root.resolve(strict=True)
    index = download.fetch_index(opener)
    attempts = root / 'release-bootstrap'
    if attempts.is_symlink():
        raise ValueError('Unsafe release bootstrap directory')
    attempts.mkdir(exist_ok=True)
    attempt = Path(tempfile.mkdtemp(prefix='attempt-', dir=attempts))
    evidence = attempt / index['evidence']['asset']
    download.fetch_asset(index['evidence'], evidence, opener)
    snapshot = attempt / 'snapshot'
    bootstrap.extract_evidence(evidence, snapshot)
    input_archive = attempt / index['build_inputs']['asset']
    download.fetch_asset(index['build_inputs'], input_archive, opener)
    bootstrap.inputs_export.verify(input_archive, index['generation'])
    for group, item in sorted(index['groups'].items()):
        archive = attempt / item['asset']
        download.fetch_asset(item, archive, opener)
        bootstrap.registry.extract_group(archive, snapshot / 'groups' / group, group)
    checked = staging.package.validate(snapshot)
    expected = {tag: {'kernel_build_id': target['kernel_build_id'],
                      'kernel_image_sha256': target['kernel_image_sha256'], 'group': target['group']}
                for tag, target in checked['targets'].items()}
    if (checked['generation'] != index['generation'] or expected != index['targets']
            or set(checked['groups']) != set(index['groups'])):
        raise ValueError('Release index differs from restored snapshot')
    staged = staging.merge_candidate(root, snapshot, initial_only=True)
    result = {'schema_version': 1, 'generation': checked['generation'], 'groups': len(checked['groups']),
              'staging_sha256': staged['sha256'], 'corresponding_source_complete': False,
              'publisher_verified': False, 'runtime_admitted': False, 'release_ready': False}
    (attempt / 'bootstrap.json').write_text(json.dumps(result, sort_keys=True) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--coverage', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(restore(args.coverage), sort_keys=True))


if __name__ == '__main__':
    main()
