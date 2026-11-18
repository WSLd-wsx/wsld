#!/usr/bin/env python3
"""Bind released kernel module metadata to its available source evidence."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import tarfile
import zipfile

HEX = re.compile(r'[0-9a-f]{64}\Z')
COMMIT = re.compile(r'[0-9a-f]{40}\Z')


def sha(path):
    with path.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def collect(index_path, assets, output):
    if output.exists():
        raise ValueError('Evidence output already exists')
    index = json.loads(index_path.read_text(encoding='utf-8'))
    if index.get('schema_version') != 1 or not isinstance(index.get('modules'), list) or not index['modules']:
        raise ValueError('Invalid kernel index')
    selected = {}
    records = []
    for entry in index['modules']:
        version = entry.get('kernel_version')
        if not isinstance(version, str) or not re.fullmatch(r'[0-9]{1,9}(?:\.[0-9]{1,9}){2,3}(?:-(?:(?:rc|RC)[1-9][0-9]{0,8}(?:-[0-9]+)?|[0-9]+))?', version):
            raise ValueError('Invalid kernel version')
        candidates = []
        for path in assets.glob('modules_' + version + '-*.json'):
            match = re.fullmatch(r'modules_' + re.escape(version) + r'-(\d+)-(\d+)\.json', path.name)
            if match:
                candidates.append(((int(match.group(1)), int(match.group(2))), path))
        if not candidates:
            raise ValueError('Missing kernel metadata: ' + version)
        order, metadata_path = max(candidates)
        metadata = json.loads(metadata_path.read_text(encoding='utf-8'))
        if (metadata.get('schema_version') != 1 or metadata.get('kernel_version') != version
                or metadata.get('archive') != entry.get('archive') or metadata.get('archive_bytes') != entry.get('bytes')
                or metadata.get('archive_sha256') != entry.get('sha256')):
            raise ValueError('Kernel metadata differs from staged index: ' + version)
        source_path = assets / f'source-materials-{version}-{order[0]}-{order[1]}.json'
        if not source_path.is_file():
            raise ValueError('Missing kernel source evidence: ' + version)
        source = json.loads(source_path.read_text(encoding='utf-8'))
        kernel_source = source.get('kernel_source', {})
        if (source.get('schema_version') != 1 or source.get('module_metadata_sha256') != sha(metadata_path)
                or source.get('build_generation') != metadata.get('build_generation')
                or source.get('kernel_build_id') != metadata.get('kernel_build_id')
                or kernel_source.get('repository') != 'https://github.com/microsoft/WSL2-Linux-Kernel'
                or not isinstance(kernel_source.get('commit'), str) or not COMMIT.fullmatch(kernel_source['commit'])
                or kernel_source['commit'] != source.get('kernel_source_commit')):
            raise ValueError('Kernel source evidence does not match metadata: ' + version)
        selected[metadata_path.name] = metadata_path
        selected[source_path.name] = source_path
        for key, prefix, suffix in (('build_inputs', 'module-sources-', '.tar.gz'), ('kernel_config', 'kernel-config-', '.txt')):
            descriptor = source.get(key, {})
            value = descriptor.get('sha256')
            if not isinstance(value, str) or not HEX.fullmatch(value) or descriptor.get('asset') != prefix + value + suffix:
                raise ValueError('Invalid source asset descriptor: ' + version)
            path = assets / descriptor['asset']
            if not path.is_file() or path.is_symlink() or path.stat().st_size != descriptor.get('bytes') or sha(path) != value:
                raise ValueError('Kernel source asset mismatch: ' + descriptor['asset'])
            selected[path.name] = path
        extended = (('kernel_source_archive', 'kernel-source-'), ('recorded_build_inputs', 'build-inputs-'),
                    ('snapshot_evidence', 'evidence-'), ('build_evidence', 'build-evidence-'))
        present = [key in source for key, _ in extended]
        if any(present) and not all(present):
            raise ValueError('Incomplete kernel build evidence set: ' + version)
        if all(present):
            for key, prefix in extended:
                descriptor = source[key]
                value = descriptor.get('sha256') if isinstance(descriptor, dict) else None
                if not isinstance(value, str) or not HEX.fullmatch(value) or descriptor.get('asset') != prefix + value + '.tar.gz':
                    raise ValueError('Invalid kernel build evidence descriptor: ' + version)
                path = assets / descriptor['asset']
                if not path.is_file() or path.is_symlink() or path.stat().st_size != descriptor.get('bytes') or sha(path) != value:
                    raise ValueError('Kernel build evidence asset mismatch: ' + descriptor['asset'])
                selected[path.name] = path
            module_archive = source.get('module_archive')
            module_records = source.get('modules')
            if (not isinstance(module_archive, dict) or module_archive.get('asset') != entry.get('archive')
                    or module_archive.get('sha256') != entry.get('sha256') or module_archive.get('bytes') != entry.get('bytes')
                    or not isinstance(module_records, dict) or not module_records):
                raise ValueError('Kernel module build record differs from staged archive: ' + version)
            archive_path = assets / module_archive['asset']
            if archive_path.is_symlink() or not archive_path.is_file() or sha(archive_path) != module_archive['sha256']:
                raise ValueError('Kernel module archive differs from source evidence: ' + version)
            with tarfile.open(archive_path, 'r:gz') as tar:
                members = tar.getmembers()
                if len(members) != len(module_records) or {x.name for x in members} != set(module_records):
                    raise ValueError('Kernel module files differ from build record: ' + version)
                for member in members:
                    stream = tar.extractfile(member)
                    expected = module_records[member.name]
                    if (not member.isfile() or stream is None or member.size != expected.get('bytes')
                            or hashlib.file_digest(stream, 'sha256').hexdigest() != expected.get('sha256')):
                        raise ValueError('Kernel module file differs from build record: ' + member.name)
        complete = source['build_inputs'].get('corresponding_source_complete') is True
        if 'source_review' in source:
            descriptor = source['source_review']
            expected_name = f'source-review-{version}-{order[0]}-{order[1]}.json'
            if (not all(present) or not isinstance(descriptor, dict)
                    or descriptor.get('asset') != expected_name or not isinstance(descriptor.get('sha256'), str)
                    or not HEX.fullmatch(descriptor['sha256'])):
                raise ValueError('Invalid kernel source review descriptor: ' + version)
            path = assets / expected_name
            if (path.is_symlink() or not path.is_file() or path.stat().st_size != descriptor.get('bytes')
                    or sha(path) != descriptor['sha256']):
                raise ValueError('Kernel source review asset mismatch: ' + version)
            review = json.loads(path.read_text(encoding='utf-8'))
            binding = review.get('binding', {})
            expected_binding = {'kernel_version': version, 'module_sha256': entry['sha256'],
                'module_sources_sha256': source['build_inputs']['sha256'],
                'kernel_source_sha256': source['kernel_source_archive']['sha256'],
                'build_inputs_sha256': source['recorded_build_inputs']['sha256'],
                'build_evidence_sha256': source['build_evidence']['sha256'],
                'kernel_config_sha256': source['kernel_config']['sha256']}
            if (review.get('schema_version') != 1 or binding != expected_binding
                    or not isinstance(review.get('source_correspondence_verified'), bool)
                    or not isinstance(review.get('corresponding_source_complete'), bool)
                    or review['source_correspondence_verified'] != source.get('source_correspondence_verified')
                    or review['corresponding_source_complete'] != source.get('corresponding_source_complete')):
                raise ValueError('Kernel source review differs from selected assets: ' + version)
            selected[path.name] = path
            complete = review['corresponding_source_complete']
        records.append({'kernel_version': version, 'module_sha256': entry['sha256'],
                        'metadata_sha256': sha(metadata_path), 'source_evidence_sha256': sha(source_path),
                        'source_correspondence_verified': source.get('source_correspondence_verified') is True,
                        'corresponding_source_complete': complete,
                        'kernel_source': kernel_source, 'extended_build_evidence': all(present)})
    manifest = {'schema_version': 1, 'kernel_index_sha256': sha(index_path), 'modules': records,
                'scope': 'Component source evidence and asset-bound review reports when available; this collector does not independently determine completeness.'}
    with zipfile.ZipFile(output, 'x', compression=zipfile.ZIP_DEFLATED) as z:
        z.writestr('evidence-manifest.json', json.dumps(manifest, sort_keys=True, indent=2) + '\n')
        for name, path in sorted(selected.items()):
            z.write(path, 'assets/' + name)
    return manifest


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('index', type=Path)
    parser.add_argument('assets', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    print(json.dumps(collect(args.index, args.assets, args.output), sort_keys=True))
