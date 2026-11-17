#!/usr/bin/env python3
"""Attach target kernel configuration and exact upstream source identity."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def attach(assets, workspace, source_archive=None):
    index_file = assets / 'index.json'
    result_file = workspace / 'modules-build/build-result.json'
    config = workspace / 'kernel-evidence/config'
    source = workspace / 'kernel-evidence/wsx-source-evidence.json'
    for path in (index_file, result_file, source, config):
        if path.is_symlink() or not path.is_file():
            raise ValueError('Missing or unsafe source evidence: ' + str(path))
    index, result = json.loads(index_file.read_text()), json.loads(result_file.read_text())
    commit = result.get('kernel_source_commit')
    source_record = json.loads(source.read_text())
    if (not isinstance(commit, str) or not re.fullmatch(r'[0-9a-f]{40}', commit)
            or index.get('generation') != result.get('build_generation')
            or sha(config) != result.get('kernel_config_sha256')
            or source_record.get('source_commit') != commit
            or source_record.get('source_tag_verified') is not True
            or not isinstance(source_record.get('tag'), str)
            or not re.fullmatch(r'linux-msft-wsl-[0-9]+(?:\.[0-9]+){2,3}(?:-rc[1-9][0-9]*)?', source_record['tag'])):
        raise ValueError('Kernel source evidence does not match build receipt')
    config_digest = sha(config)
    config_asset = assets / ('kernel-config-' + config_digest + '.txt')
    if config_asset.is_symlink():
        raise ValueError('Unsafe kernel config asset')
    if not config_asset.exists():
        shutil.copyfile(config, config_asset)
    if sha(config_asset) != config_digest:
        raise ValueError('Copied kernel config changed')
    manifest = {'schema_version': 1, 'build_generation': index['generation'],
                'index_sha256': sha(index_file), 'kernel_source_commit': commit,
                'kernel_source': {'repository': 'https://github.com/microsoft/WSL2-Linux-Kernel',
                                  'commit': commit, 'tag': source_record['tag']},
                'kernel_config': {'asset': config_asset.name, 'sha256': config_digest, 'bytes': config_asset.stat().st_size},
                'build_inputs': index['build_inputs'], 'groups': sorted(index['groups']),
                'source_correspondence_verified': False, 'release_ready': False}
    if source_archive is not None:
        receipt_path = workspace / 'kernel-source-receipt.json'
        if (receipt_path.is_symlink() or not receipt_path.is_file() or source_archive.is_symlink()
                or not source_archive.is_file() or not 0 < source_archive.stat().st_size <= 1024 * 1024 * 1024):
            raise ValueError('Kernel source archive or receipt is missing or unsafe')
        receipt = json.loads(receipt_path.read_text())
        if (receipt.get('schema_version') != 1 or receipt.get('source_commit') != commit
                or receipt.get('source_tree') != source_record.get('source_tree')
                or receipt.get('package_build_revision') != source_record.get('package_build_revision')
                or receipt.get('package_build_tree') != source_record.get('package_build_tree')
                or source_record.get('source_tree_matches_package_metadata') is not True
                or receipt.get('sha256') != sha(source_archive)
                or receipt.get('bytes') != source_archive.stat().st_size):
            raise ValueError('Kernel source archive differs from build source evidence')
        archive_asset = assets / ('kernel-source-' + receipt['sha256'] + '.tar.gz')
        if archive_asset.is_symlink():
            raise ValueError('Unsafe kernel source archive asset')
        if not archive_asset.exists():
            shutil.copyfile(source_archive, archive_asset)
        if archive_asset.stat().st_size != receipt['bytes'] or sha(archive_asset) != receipt['sha256']:
            raise ValueError('Copied kernel source archive changed')
        manifest['kernel_source_archive'] = {'asset': archive_asset.name, 'sha256': receipt['sha256'], 'bytes': receipt['bytes']}
        manifest['kernel_source_tree_evidence'] = {'source_tree': receipt['source_tree'],
            'package_build_revision': receipt['package_build_revision'], 'package_build_tree': receipt['package_build_tree'],
            'source_tree_matches_package_metadata': True}
    (assets / 'source-materials.json').write_text(json.dumps(manifest, sort_keys=True, indent=2) + '\n')
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--assets', required=True, type=Path)
    parser.add_argument('--workspace', required=True, type=Path)
    parser.add_argument('--source-archive', type=Path)
    args = parser.parse_args()
    print(json.dumps(attach(args.assets, args.workspace, args.source_archive), sort_keys=True))


if __name__ == '__main__':
    main()
