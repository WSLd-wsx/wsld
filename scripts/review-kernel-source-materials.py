#!/usr/bin/env python3
"""Check released module build evidence and record the scope of source review."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import tarfile

HERE = Path(__file__).resolve().parent


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


inputs_export = load('review_build_inputs', 'export-kernel-build-inputs.py')
tree_verify = load('review_source_tree', 'verify-kernel-source-archive.py')
module_sources = load('review_module_source_list', 'export-kernel-module-sources.py')


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def asset(root, desc):
    name = desc.get('asset') if isinstance(desc, dict) else None
    if not isinstance(name, str) or not re.fullmatch(r'[A-Za-z0-9._-]+', name):
        raise ValueError('Invalid asset name')
    path = root / name
    if path.is_symlink() or not path.is_file() or path.stat().st_size != desc.get('bytes') or sha(path) != desc.get('sha256'):
        raise ValueError('Asset hash or size mismatch: ' + name)
    return path


def archive_manifest(path):
    with tarfile.open(path, 'r:gz') as archive:
        members = archive.getmembers()
        names = [item.name for item in members]
        if len(names) != len(set(names)) or any(not item.isfile() for item in members):
            raise ValueError('Unsafe evidence archive')
        manifest = json.load(archive.extractfile('manifest.json'))
        files = manifest.get('files')
        if not isinstance(files, dict) or set(names) != set(files) | {'manifest.json'}:
            raise ValueError('Evidence archive differs from manifest')
        for name, expected in files.items():
            if hashlib.file_digest(archive.extractfile(name), 'sha256').hexdigest() != expected:
                raise ValueError('Evidence member differs from manifest: ' + name)
        return manifest


def report(root, source_path, metadata_path, review_path=None):
    source = json.loads(source_path.read_text())
    metadata = json.loads(metadata_path.read_text())
    module = asset(root, source['module_archive'])
    sources = asset(root, source['build_inputs'])
    build_inputs = asset(root, source['recorded_build_inputs'])
    kernel_source = asset(root, source['kernel_source_archive'])
    config = asset(root, source['kernel_config'])
    evidence = asset(root, source['build_evidence'])
    if (metadata.get('archive') != module.name or metadata.get('archive_sha256') != sha(module)
            or metadata.get('archive_bytes') != module.stat().st_size
            or metadata.get('build_generation') != source.get('build_generation')
            or metadata.get('kernel_build_id') != source.get('kernel_build_id')
            or sha(metadata_path) != source.get('module_metadata_sha256')):
        raise ValueError('Module metadata differs from source record')
    generation = source['build_generation']
    recorded = inputs_export.verify(build_inputs, generation)
    source_manifest = archive_manifest(sources)
    required_sources = {'inputs/' + name for name in recorded['inputs'] if name.startswith('kernel-modules/')}
    required_sources.update('inputs/' + name for name in module_sources.SCRIPTS)
    required_sources.add('runner-dependencies.json')
    if set(source_manifest['files']) != required_sources:
        raise ValueError('Module source archive is missing or has unexpected inputs')
    for name, digest in source_manifest['files'].items():
        key = '@runner-dependencies' if name == 'runner-dependencies.json' else name.removeprefix('inputs/')
        if recorded['inputs'].get(key) != digest:
            raise ValueError('Module source differs from recorded input: ' + name)
    evidence_manifest = archive_manifest(evidence)
    with tarfile.open(evidence, 'r:gz') as tar:
        build = json.load(tar.extractfile('modules-build/build-result.json'))
        audit = json.load(tar.extractfile('static-crc-audit.json'))
        tree_evidence = json.load(tar.extractfile('kernel-evidence/wsx-source-evidence.json'))
        receipt = json.load(tar.extractfile('kernel-source-receipt.json'))
        inspection = json.load(tar.extractfile('target-inspection/inspection.json'))
        captured_config = hashlib.sha256(tar.extractfile('kernel-evidence/config').read()).hexdigest()
    tree = source['kernel_source_tree_evidence']
    if (build.get('build_generation') != generation or build.get('build_inputs') != recorded['inputs']
            or build.get('kernel_source_commit') != source.get('kernel_source_commit')
            or build.get('modules') != source.get('modules') or evidence_manifest.get('modules') != build.get('modules')
            or evidence_manifest.get('build_generation') != generation
            or inspection.get('kernel_build_id') != source.get('kernel_build_id')
            or inspection.get('kernel_image_sha256') != metadata.get('kernel_image_sha256')
            or captured_config != sha(config) or build.get('kernel_config_sha256') != sha(config)
            or tree_evidence.get('source_tag_verified') is not True
            or tree_evidence.get('source_tree_matches_package_metadata') is not True
            or tree_evidence.get('source_commit') != source.get('kernel_source_commit')
            or tree_evidence.get('source_tree') != tree.get('source_tree')
            or tree.get('source_tree') != tree.get('package_build_tree')
            or receipt.get('source_commit') != source.get('kernel_source_commit')
            or receipt.get('package_build_revision') != tree.get('package_build_revision')
            or tree_evidence.get('package_build_revision') != tree.get('package_build_revision')
            or inspection.get('upstream_build_metadata', {}).get('git_revision') != tree.get('package_build_revision')
            or receipt.get('package_build_tree') != tree.get('package_build_tree')
            or receipt.get('sha256') != sha(kernel_source) or receipt.get('source_tree') != tree.get('source_tree')):
        raise ValueError('Build inputs, configuration or kernel source evidence differ')
    tree_verify.verify(kernel_source, tree['source_tree'])
    records = source['modules']
    if (audit.get('crc_match') is not True or audit.get('kernel_build_id') != source.get('kernel_build_id')
            or set(audit.get('modules', {})) != set(records)):
        raise ValueError('Static module CRC audit differs')
    with tarfile.open(module, 'r:gz') as tar:
        members = tar.getmembers()
        if len(members) != len(records) or {m.name for m in members} != set(records):
            raise ValueError('Module members differ from recorded build')
        for item in members:
            entry, crc = records[item.name], audit['modules'][item.name]
            if (not item.isfile() or item.size != entry['bytes']
                    or hashlib.file_digest(tar.extractfile(item), 'sha256').hexdigest() != entry['sha256']
                    or crc.get('sha256') != entry['sha256'] or crc.get('missing') != []
                    or crc.get('mismatches') != [] or crc.get('has_module_layout') is not True
                    or not crc.get('import_count')):
                raise ValueError('Module or CRC evidence differs: ' + item.name)
    binding = {'kernel_version': metadata['kernel_version'], 'module_sha256': sha(module),
               'module_sources_sha256': sha(sources), 'kernel_source_sha256': sha(kernel_source),
               'build_inputs_sha256': sha(build_inputs), 'build_evidence_sha256': sha(evidence),
               'kernel_config_sha256': sha(config)}
    complete = False
    review = None
    if review_path is not None:
        if review_path.is_symlink() or not review_path.is_file():
            raise ValueError('Unsafe source completeness review')
        review = json.loads(review_path.read_text())
        if (review.get('schema_version') != 1 or review.get('decision') != 'complete'
                or review.get('binding') != binding
                or not isinstance(review.get('reviewer'), str) or not review['reviewer'].strip()
                or not isinstance(review.get('review_reference'), str) or not review['review_reference'].strip()
                or review.get('scope') != 'corresponding-source-and-recipient-access'):
            raise ValueError('Source completeness review is not valid for these exact assets')
        complete = True
    result = {'schema_version': 1, 'binding': binding,
              'checks': {'asset_hashes': True, 'recorded_build_inputs': True, 'module_sources_match_inputs': True,
                         'kernel_source_git_tree': True, 'kernel_configuration': True,
                         'module_build_records_and_crc': True},
              'source_correspondence_verified': True, 'corresponding_source_complete': complete,
              'correspondence_scope': 'Retained build records match these module and source assets; no independent rebuild or Microsoft kernel binary provenance is asserted.',
              'completeness_scope': 'Whole delivery, including applicable notices and recipient source access; requires a separate asset-specific review.',
              'completeness_review': review}
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--assets', type=Path, required=True)
    p.add_argument('--source-materials', type=Path, required=True)
    p.add_argument('--metadata', type=Path, required=True)
    p.add_argument('--review', type=Path)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    result = report(a.assets, a.source_materials, a.metadata, a.review)
    with a.output.open('x') as stream:
        json.dump(result, stream, sort_keys=True, indent=2)
        stream.write('\n')
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
