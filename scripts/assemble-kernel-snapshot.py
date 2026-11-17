#!/usr/bin/env python3
"""Assemble a non-admitted snapshot from one build and its CRC evidence."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('kernel_package', Path(__file__).with_name('kernel-package.py'))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
spec = importlib.util.spec_from_file_location('kernel_build_generation', Path(__file__).with_name('kernel_build_generation.py'))
generation_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generation_module)


def assemble(build_dir, audit_path, tag, destination):
    if destination.exists() or destination.is_symlink():
        raise ValueError('Snapshot output already exists')
    if not package.TAG.fullmatch(tag):
        raise ValueError('Invalid kernel source tag')
    result_file = build_dir / 'build-result.json'
    if result_file.is_symlink() or not result_file.is_file() or audit_path.is_symlink() or not audit_path.is_file():
        raise ValueError('Unsafe build or audit evidence')
    result_bytes = result_file.read_bytes()
    if len(result_bytes) > 8 * 1024 * 1024:
        raise ValueError('Build evidence exceeds limit')
    result = json.loads(result_bytes)
    audit_bytes = audit_path.read_bytes()
    if len(audit_bytes) > 8 * 1024 * 1024:
        raise ValueError('Kernel audit exceeds limit')
    audit = json.loads(audit_bytes)
    current, _ = generation_module.generation(ROOT)
    release = tag.removeprefix('linux-msft-wsl-') + '-microsoft-standard-WSL2'
    if (result.get('schema_version') != 1 or result.get('build_generation') != current
            or result.get('kernel_release') != release or result.get('module_loaded') is not False
            or result.get('runtime_admitted') is not False or result.get('release_ready') is not False
            or not package.COMMIT.fullmatch(result.get('kernel_source_commit', ''))):
        raise ValueError('Build evidence does not match the current target or build generation')
    static = audit.get('crc_match') is True and audit.get('runtime_admitted') is False
    evidence = result.get('kernel_symvers_evidence') if static else audit
    if not isinstance(evidence, dict):
        raise ValueError('Kernel image evidence is missing')
    if (audit.get('schema_version') != 1
            or (not static and (audit.get('kernel') != release
                                or audit.get('symbol_crc_compatibility_admitted') is not True))
            or audit.get('kernel_build_id') != evidence.get('kernel_build_id')
            or not package.BUILD_ID.fullmatch(evidence.get('kernel_build_id', ''))
            or not package.SHA.fullmatch(evidence.get('kernel_image_sha256', ''))
            or set(audit.get('modules', {})) != set(package.MODULES)
            or set(result.get('modules', {})) != set(package.MODULES)):
        raise ValueError('Kernel CRC evidence is incomplete or targets a different kernel')
    files = {}
    for name in package.MODULES:
        file = build_dir / 'modules' / name
        entry, proof = result['modules'][name], audit['modules'][name]
        if (file.is_symlink() or not file.is_file() or file.stat().st_size != entry['bytes']
                or package.digest(file) != entry['sha256'] or proof['sha256'] != entry['sha256']
                or proof.get('mismatches') != []
                or (proof.get('missing') if static else proof.get('unobserved')) != []
                or proof.get('has_module_layout') is not True
                or not proof.get('import_count' if static else 'imports')):
            raise ValueError('Module does not match build and CRC evidence: ' + name)
        files[name] = {'bytes': entry['bytes'], 'sha256': entry['sha256']}
    ident = package.group_id(files)
    audit_sha = hashlib.sha256(audit_bytes).hexdigest()
    build_sha = hashlib.sha256(result_bytes).hexdigest()
    destination.mkdir(parents=True)
    group = destination / 'groups' / ident
    group.mkdir(parents=True)
    for name in package.MODULES:
        shutil.copyfile(build_dir / 'modules' / name, group / name)
    (destination / 'audits').mkdir()
    (destination / 'audits' / (audit_sha + '.json')).write_bytes(audit_bytes)
    (destination / 'builds').mkdir()
    (destination / 'builds' / (build_sha + '.json')).write_bytes(result_bytes)
    index = {'schema_version': 3, 'generation': current, 'groups': {ident: files}, 'targets': {tag: {
        'source_commit': result['kernel_source_commit'], 'group': ident,
        'build_source_commit': result['kernel_source_commit'],
        'kernel_build_id': evidence['kernel_build_id'], 'kernel_image_sha256': evidence['kernel_image_sha256'],
        'audit_sha256': audit_sha, 'build_sha256': build_sha}}}
    (destination / 'index.json').write_text(json.dumps(index, sort_keys=True, indent=2) + '\n')
    package.validate(destination)
    return {'generation': current, 'group': ident, 'target': tag,
            'runtime_admitted': False, 'release_ready': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', required=True, type=Path)
    parser.add_argument('--audit', required=True, type=Path)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(assemble(args.build, args.audit, args.tag, args.output)))


if __name__ == '__main__':
    main()
