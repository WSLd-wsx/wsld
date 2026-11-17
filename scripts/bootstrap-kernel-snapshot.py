#!/usr/bin/env python3
"""Fetch and stage the existing registry snapshot without publishing."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import tarfile
import tempfile


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


registry = load('kernel_registry_bootstrap', 'kernel-package-registry.py')
staging = load('kernel_staging_bootstrap', 'kernel-staging-snapshots.py')
inputs_export = load('kernel_inputs_bootstrap', 'export-kernel-build-inputs.py')


def bootstrap(root, actor='', credential=''):
    root = root.resolve(strict=True)
    attempts = root / 'registry-bootstrap'
    if attempts.is_symlink():
        raise ValueError('Unsafe registry bootstrap directory')
    attempts.mkdir(exist_ok=True)
    attempt = Path(tempfile.mkdtemp(prefix='attempt-', dir=attempts))
    fetched = registry.pull(attempt / 'download.tar.gz', attempt / 'snapshot', actor, credential)
    staged = staging.merge_candidate(root, attempt / 'snapshot', initial_only=True)
    evidence = {'schema_version': 1, 'manifest_digest': fetched['manifest_digest'],
                'archive_digest': fetched['archive_digest'], 'generation': fetched['generation'],
                'staging_sha256': staged['sha256'], 'publisher_verified': False,
                'runtime_admitted': False, 'release_ready': False}
    with (attempt / 'bootstrap.json').open('x', encoding='utf-8') as stream:
        json.dump(evidence, stream, indent=2)
        stream.write('\n')
    return evidence


def extract_evidence(archive, destination):
    if destination.exists() or destination.is_symlink():
        raise ValueError('Evidence output already exists')
    destination.mkdir()
    seen, total = set(), 0
    with tarfile.open(archive, 'r:gz') as tar:
        for member in tar:
            name = member.name
            limit = 128 * 1024 * 1024 if name == 'build-inputs.tar.gz' else 8 * 1024 * 1024
            if (name in seen or not member.isfile() or member.size < 0 or member.size > limit
                    or not (name in ('index.json', 'build-inputs.tar.gz') or re.fullmatch(r'(audits|builds)/[0-9a-f]{64}\.json', name))):
                raise ValueError('Unsafe kernel evidence member')
            seen.add(name)
            total += member.size
            if len(seen) > 10000 or total > 1024 * 1024 * 1024:
                raise ValueError('Kernel evidence archive exceeds limits')
            target = destination.parent / name if name == 'build-inputs.tar.gz' else destination / name
            target.parent.mkdir(parents=True, exist_ok=True)
            source = tar.extractfile(member)
            if source is None:
                raise ValueError('Cannot read kernel evidence member')
            with target.open('xb') as output:
                shutil.copyfileobj(source, output, 1024 * 1024)
    if 'index.json' not in seen:
        raise ValueError('Kernel evidence snapshot index is missing')


def bootstrap_groups(root, actor='', credential='', opener=None):
    root = root.resolve(strict=True)
    attempts = root / 'registry-bootstrap'
    if attempts.is_symlink():
        raise ValueError('Unsafe registry bootstrap directory')
    attempts.mkdir(exist_ok=True)
    attempt = Path(tempfile.mkdtemp(prefix='groups-', dir=attempts))
    reference = registry.latest_index_digest(actor, credential, opener=opener)
    index_path = attempt / 'group-index.json'
    registry.pull_index(index_path, reference, actor, credential, opener=opener)
    group_index = registry.validate_group_index(json.loads(index_path.read_text()))
    evidence = group_index['evidence']
    evidence_path = attempt / 'evidence.tar.gz'
    result = registry.pull_evidence(evidence_path, 'sha256:' + evidence['oci_manifest_sha256'], actor, credential, opener=opener)
    if (result['archive_digest'] != 'sha256:' + evidence['archive_sha256']
            or evidence_path.stat().st_size != evidence['bytes']):
        raise ValueError('Kernel evidence does not match the index')
    snapshot = attempt / 'snapshot'
    extract_evidence(evidence_path, snapshot)
    for group, info in sorted(group_index['groups'].items()):
        archive = attempt / (group + '.tar.gz')
        result = registry.pull_group(archive, 'sha256:' + info['oci_manifest_sha256'], group, actor, credential, opener=opener)
        if result['archive_digest'] != 'sha256:' + info['archive_sha256'] or archive.stat().st_size != info['bytes']:
            raise ValueError('Kernel module group does not match the index')
        registry.extract_group(archive, snapshot / 'groups' / group, group)
    checked = staging.package.validate(snapshot)
    expected_targets = {tag: {'kernel_build_id': item['kernel_build_id'],
                              'kernel_image_sha256': item['kernel_image_sha256'], 'group': item['group']}
                        for tag, item in checked['targets'].items()}
    if (checked['generation'] != group_index['generation'] or expected_targets != group_index['targets']
            or set(checked['groups']) != set(group_index['groups'])):
        raise ValueError('Kernel group index and restored evidence differ')
    source_inputs = attempt / 'build-inputs.tar.gz'
    if group_index['schema_version'] == 2:
        descriptor = group_index['build_inputs']
        pulled = registry.pull_evidence(source_inputs, 'sha256:' + descriptor['oci_manifest_sha256'],
                                        actor, credential, opener=opener)
        if (pulled['archive_digest'] != 'sha256:' + descriptor['archive_sha256']
                or source_inputs.stat().st_size != descriptor['bytes']):
            raise ValueError('Kernel build inputs do not match the index')
    if source_inputs.exists() or source_inputs.is_symlink():
        inputs_export.verify(source_inputs, checked['generation'])
    staged = staging.merge_candidate(root, snapshot, initial_only=True)
    evidence = {'schema_version': 1, 'manifest_digest': reference, 'generation': checked['generation'],
                'staging_sha256': staged['sha256'], 'groups': len(checked['groups']),
                'build_inputs_verified': source_inputs.is_file(),
                'publisher_verified': False, 'runtime_admitted': False, 'release_ready': False}
    with (attempt / 'bootstrap.json').open('x', encoding='utf-8') as output:
        json.dump(evidence, output, indent=2)
        output.write('\n')
    if snapshot.is_symlink() or not snapshot.is_dir():
        raise ValueError('Unsafe restored snapshot; cleanup refused')
    for group in group_index['groups']:
        archive = attempt / (group + '.tar.gz')
        if archive.is_symlink() or not archive.is_file():
            raise ValueError('Unsafe restored group archive; cleanup refused')
    shutil.rmtree(snapshot)
    for group in group_index['groups']:
        (attempt / (group + '.tar.gz')).unlink()
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--coverage', type=Path, required=True)
    args = parser.parse_args()
    result = bootstrap(args.coverage, os.environ.get('GITHUB_ACTOR', ''), os.environ.get('GITHUB_TOKEN', ''))
    print(json.dumps(result))


if __name__ == '__main__':
    main()
