#!/usr/bin/env python3
"""Retain hash-checked kernel build, source and static CRC evidence."""
import argparse
import gzip
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('kernel_finalizer_evidence', HERE / 'finalize-kernel-build.py')
finalizer = importlib.util.module_from_spec(spec); spec.loader.exec_module(finalizer)
FILES = ('modules-build/build-result.json', 'modules-build/build.log', 'static-crc-audit.json',
         'kernel-evidence/config', 'kernel-evidence/Module.symvers',
         'kernel-evidence/wsx-source-evidence.json', 'kernel-evidence/wsx-target-prepared.json',
         'kernel-evidence/wsx-symvers-evidence.json', 'target-inspection/inspection.json',
         'target-inspection/kernel_config', 'target-inspection/kernel-exports.json',
         'kernel-source-receipt.json', 'cleanup-result.json')


def sha(data): return hashlib.sha256(data).hexdigest()


def export(workspace, output):
    if output.exists() or output.is_symlink():
        raise ValueError('Build evidence archive already exists')
    workspace = workspace.resolve(strict=True)
    payloads = {}
    for name in FILES:
        path = workspace / name
        if path.is_symlink() or not path.is_file() or not 0 < path.stat().st_size <= 64 * 1024 * 1024:
            raise ValueError('Missing or unsafe build evidence: ' + name)
        payloads[name] = path.read_bytes()
    result = json.loads(payloads['modules-build/build-result.json'])
    inspection = json.loads(payloads['target-inspection/inspection.json'])
    source = json.loads(payloads['kernel-evidence/wsx-source-evidence.json'])
    receipt = json.loads(payloads['kernel-source-receipt.json'])
    source_archive = workspace / 'kernel-source.tar.gz'
    inputs = result.get('build_inputs')
    symvers_evidence = result.get('kernel_symvers_evidence')
    input_generation = sha(json.dumps({'schema_version': 1, 'inputs': inputs}, sort_keys=True, separators=(',', ':')).encode()) if isinstance(inputs, dict) else None
    if (result.get('schema_version') != 1 or set(result.get('modules', {})) != finalizer.MODULES
            or input_generation != result.get('build_generation')
            or result.get('kernel_config_sha256') != sha(payloads['kernel-evidence/config'])
            or result.get('kernel_symvers_sha256') != sha(payloads['kernel-evidence/Module.symvers'])
            or inspection.get('kernel_config_sha256') != sha(payloads['target-inspection/kernel_config'])
            or not isinstance(symvers_evidence, dict)
            or symvers_evidence.get('kernel_build_id') != inspection.get('kernel_build_id')
            or symvers_evidence.get('kernel_image_sha256') != inspection.get('kernel_image_sha256')
            or result.get('kernel_source_commit') != source.get('source_commit')
            or source.get('source_commit') != receipt.get('source_commit')
            or source.get('source_tree') != receipt.get('source_tree')
            or source.get('source_tag_verified') is not True
            or source.get('source_tree_matches_package_metadata') is not True
            or source.get('package_build_revision') != inspection.get('upstream_build_metadata', {}).get('git_revision')
            or source.get('package_build_revision') != receipt.get('package_build_revision')
            or source.get('source_tree') != receipt.get('package_build_tree')
            or source_archive.is_symlink() or not source_archive.is_file()
            or source_archive.stat().st_size != receipt.get('bytes') or finalizer.sha(source_archive) != receipt.get('sha256')):
        raise ValueError('Build, kernel source and package evidence do not match')
    finalizer.verify_static_crc(workspace, result)
    manifest = {'schema_version': 1, 'files': {name: sha(data) for name, data in sorted(payloads.items())},
                'build_generation': result['build_generation'], 'kernel_source_commit': result['kernel_source_commit'],
                'kernel_source_tree': source['source_tree'], 'package_build_revision': source['package_build_revision'],
                'kernel_build_id': inspection['kernel_build_id'], 'kernel_image_sha256': inspection['kernel_image_sha256'],
                'modules': result['modules'], 'source_correspondence_verified': False,
                'scope': 'Recorded build and CRC evidence; not an independent reproducible-build or license-completeness attestation.'}
    payloads['manifest.json'] = (json.dumps(manifest, sort_keys=True, indent=2) + '\n').encode()
    with output.open('xb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as tar:
            for name, data in sorted(payloads.items()):
                info = tarfile.TarInfo(name); info.size = len(data); info.mode = 0o644
                info.uid = info.gid = info.mtime = 0
                tar.addfile(info, io.BytesIO(data))
    with tarfile.open(output, 'r:gz') as tar:
        members = tar.getmembers()
        if len(members) != len(payloads) or {m.name for m in members} != set(payloads):
            raise ValueError('Build evidence archive member mismatch')
        for member in members:
            stream = tar.extractfile(member)
            if not member.isfile() or stream is None or stream.read() != payloads[member.name]:
                raise ValueError('Build evidence archive content mismatch: ' + member.name)
    with output.open('rb') as f: archive_sha = hashlib.file_digest(f, 'sha256').hexdigest()
    return {'asset': 'build-evidence-' + archive_sha + '.tar.gz', 'sha256': archive_sha, 'bytes': output.stat().st_size}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workspace', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--descriptor', type=Path, required=True)
    args = parser.parse_args()
    if args.descriptor.exists(): raise ValueError('Build evidence descriptor already exists')
    descriptor = export(args.workspace, args.output)
    args.descriptor.write_text(json.dumps(descriptor, sort_keys=True) + '\n')
    print(json.dumps(descriptor, sort_keys=True))
