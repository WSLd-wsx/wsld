#!/usr/bin/env python3
"""Retain verified build outputs and remove only per-target intermediates."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

MODULES = {'drm_kms_helper.ko', 'virtio_dma_buf.ko', 'virtio-gpu.ko',
           'virtio_msg.ko', 'virtio_msg_vsock.ko', 'wsx_virtio_msg_pci.ko'}


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def verify_static_crc(workspace, result):
    audit_file = workspace / 'static-crc-audit.json'
    if audit_file.is_symlink() or not audit_file.is_file():
        raise ValueError('Static CRC evidence is missing or unsafe; intermediates retained')
    audit = json.loads(audit_file.read_text())
    if (audit.get('schema_version') != 1 or audit.get('crc_match') is not True
            or set(audit.get('modules', {})) != MODULES):
        raise ValueError('Static CRC evidence is incomplete or failed; intermediates retained')
    exports = json.loads((workspace / 'target-inspection/kernel-exports.json').read_text())
    if audit.get('kernel_build_id') != exports.get('kernel_build_id') or not audit.get('kernel_build_id'):
        raise ValueError('Static CRC evidence identifies a different kernel')
    image_evidence = result.get('kernel_symvers_evidence')
    if (not isinstance(image_evidence, dict)
            or image_evidence.get('kernel_build_id') != exports.get('kernel_build_id')
            or not isinstance(exports.get('kernel_image_sha256'), str)
            or image_evidence.get('kernel_image_sha256') != exports['kernel_image_sha256']):
        raise ValueError('Static CRC evidence identifies a different kernel image')
    for name, entry in result['modules'].items():
        item = audit['modules'][name]
        module = workspace / 'modules-build/modules' / name
        if (module.is_symlink() or not module.is_file() or sha(module) != entry['sha256']
                or item.get('sha256') != entry['sha256'] or item.get('missing') != []
                or item.get('mismatches') != [] or not item.get('import_count')
                or item.get('has_module_layout') is not True):
            raise ValueError('Static CRC evidence does not match module: ' + name)


def finalize(workspace):
    if workspace.is_symlink() or not workspace.is_dir():
        raise ValueError('Unsafe build workspace')
    workspace = workspace.resolve(strict=True)
    cleanup_report = workspace / 'cleanup-result.json'
    if cleanup_report.exists() or cleanup_report.is_symlink():
        raise ValueError('Cleanup evidence already exists')
    build = workspace / 'modules-build'
    kernel = workspace / 'kernel'
    if build.is_symlink() or kernel.is_symlink() or not build.is_dir() or not kernel.is_dir():
        raise ValueError('Unsafe build or kernel directory')
    result = json.loads((build / 'build-result.json').read_text())
    if result.get('schema_version') != 1 or set(result.get('modules', {})) != MODULES:
        raise ValueError('Incomplete module build result')
    if result.get('module_loaded') is not False or result.get('release_ready') is not False:
        raise ValueError('Unexpected build result scope')
    build_log = build / 'build.log'
    if build_log.is_symlink() or not build_log.is_file():
        raise ValueError('Build log is missing or unsafe; intermediates retained')
    modules = build / 'modules'
    if modules.is_symlink() or not modules.is_dir():
        raise ValueError('Unsafe module output directory')
    for name, entry in result['modules'].items():
        path = modules / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size != entry['bytes'] or sha(path) != entry['sha256']:
            raise ValueError('Module output changed: ' + name)
    config, symvers = kernel / '.config', kernel / 'Module.symvers'
    if sha(config) != result['kernel_config_sha256'] or sha(symvers) != result['kernel_symvers_sha256']:
        raise ValueError('Kernel build evidence changed')
    derived_evidence = result.get('kernel_symvers_evidence')
    evidence_file = kernel / 'wsx-symvers-evidence.json'
    if derived_evidence is not None:
        if (evidence_file.is_symlink() or not evidence_file.is_file()
                or json.loads(evidence_file.read_text()) != derived_evidence
                or derived_evidence.get('symvers_sha256') != result['kernel_symvers_sha256']):
            raise ValueError('Kernel symbol extraction evidence changed')
    target_evidence = result.get('diagnostic_target')
    if target_evidence is not None:
        verify_static_crc(workspace, result)
        target_file = kernel / 'wsx-target-prepared.json'
        source_file = kernel / 'wsx-source-evidence.json'
        if (target_file.is_symlink() or source_file.is_symlink() or not target_file.is_file() or not source_file.is_file()):
            raise ValueError('Target source evidence is missing or unsafe')
        prepared = json.loads(target_file.read_text())
        source = json.loads(source_file.read_text())
        if (any(prepared.get(key) != value for key, value in target_evidence.items())
                or prepared.get('prepared_config_sha256') != result['kernel_config_sha256']
                or source.get('source_commit') != result['kernel_source_commit']):
            raise ValueError('Target source evidence changed')
    evidence = workspace / 'kernel-evidence'
    if evidence.exists() or evidence.is_symlink():
        raise ValueError('Kernel evidence directory already exists')
    disposable = [build / 'graphics-module', build / 'owner-module', kernel]
    extracted_image = None
    if target_evidence is not None:
        disposable.append(workspace / 'environment')
        extracted_image = workspace / 'target-inspection/kernel'
        if extracted_image.is_symlink():
            raise ValueError('Unsafe extracted kernel image')
        if extracted_image.exists():
            if not extracted_image.is_file() or sha(extracted_image) != derived_evidence.get('kernel_image_sha256'):
                raise ValueError('Extracted kernel image changed; intermediates retained')
        else:
            extracted_image = None
    for path in disposable:
        if path.is_symlink() or not path.is_dir():
            raise ValueError('Unsafe build intermediate directory')
    evidence.mkdir()
    shutil.copyfile(config, evidence / 'config')
    shutil.copyfile(symvers, evidence / 'Module.symvers')
    if derived_evidence is not None:
        shutil.copyfile(evidence_file, evidence / 'wsx-symvers-evidence.json')
    if target_evidence is not None:
        shutil.copyfile(target_file, evidence / 'wsx-target-prepared.json')
        shutil.copyfile(source_file, evidence / 'wsx-source-evidence.json')
    if sha(evidence / 'config') != result['kernel_config_sha256'] or sha(evidence / 'Module.symvers') != result['kernel_symvers_sha256']:
        raise ValueError('Copied kernel evidence changed; intermediates retained')
    for path in disposable:
        shutil.rmtree(path)
    if extracted_image is not None:
        extracted_image.unlink()
    report = {'schema_version': 1, 'removed': [path.relative_to(workspace).as_posix() for path in disposable],
              'retained': ['kernel-evidence', 'modules-build/modules',
                           'modules-build/build-result.json', 'modules-build/build.log'],
              'module_loaded': False, 'release_ready': False}
    if extracted_image is not None:
        report['removed'].append(extracted_image.relative_to(workspace).as_posix())
    if target_evidence is None:
        report['retained'].extend(['environment', 'downloads'])
    with cleanup_report.open('x', encoding='utf-8') as output:
        json.dump(report, output, indent=2)
        output.write('\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('workspace', type=Path)
    args = parser.parse_args()
    print(json.dumps(finalize(args.workspace)))


if __name__ == '__main__':
    main()
