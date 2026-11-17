#!/usr/bin/env python3
"""Export diagnostic per-group archives and a compact kernel index."""
import argparse
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import tarfile

spec = importlib.util.spec_from_file_location('kernel_snapshot_export', Path(__file__).with_name('kernel-package.py'))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
spec = importlib.util.spec_from_file_location('kernel_registry_export', Path(__file__).with_name('kernel-package-registry.py'))
registry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(registry)
spec = importlib.util.spec_from_file_location('kernel_build_inputs_export', Path(__file__).with_name('export-kernel-build-inputs.py'))
build_inputs_export = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build_inputs_export)


def evidence_paths(snapshot):
    return [path for path in sorted(snapshot.rglob('*')) if path.is_file()
            and (path.relative_to(snapshot).as_posix() == 'index.json'
                 or path.relative_to(snapshot).parts[0] in ('audits', 'builds'))]


def verify_export(snapshot, output):
    source = package.validate(snapshot)
    index_file = output / 'index.json'
    if output.is_symlink() or index_file.is_symlink() or not index_file.is_file() or index_file.stat().st_size > 4 * 1024 * 1024:
        raise ValueError('Unsafe group index')
    index = json.loads(index_file.read_text())
    registry.validate_group_index(index)
    manifest_path = output / 'index.oci.json'
    expected_index_manifest, _ = registry.build_index_manifest(index_file)
    if manifest_path.is_symlink() or not manifest_path.is_file() or manifest_path.read_bytes() != expected_index_manifest:
        raise ValueError('Index OCI manifest differs from index bytes')
    expected_targets = {tag: {'kernel_build_id': item['kernel_build_id'],
                              'kernel_image_sha256': item['kernel_image_sha256'], 'group': item['group']}
                        for tag, item in source['targets'].items()}
    if (index.get('schema_version') not in (1, 2) or index.get('generation') != source['generation']
            or index.get('targets') != expected_targets or index.get('runtime_admitted') is not False
            or index.get('release_ready') is not False or not isinstance(index.get('groups'), dict)
            or set(index['groups']) != {item['group'] for item in expected_targets.values()}):
        raise ValueError('Group index differs from checked snapshot')
    evidence_path = output / 'evidence.tar.gz'
    evidence_manifest = output / 'evidence.oci.json'
    info = index['evidence']
    if (evidence_path.is_symlink() or not evidence_path.is_file() or evidence_manifest.is_symlink()
            or not evidence_manifest.is_file() or evidence_path.stat().st_size != info['bytes']
            or package.digest(evidence_path) != info['archive_sha256']
            or package.digest(evidence_manifest) != info['oci_manifest_sha256']
            or evidence_manifest.read_bytes() != registry.build_evidence_manifest(evidence_path)[0]):
        raise ValueError('Evidence archive or manifest differs from index')
    expected = {path.relative_to(snapshot).as_posix(): path for path in evidence_paths(snapshot)}
    inputs = output / 'build-inputs.tar.gz'
    if inputs.exists() or inputs.is_symlink():
        build_inputs_export.verify(inputs, source['generation'])
        if index['schema_version'] == 1:
            expected['build-inputs.tar.gz'] = inputs
    if index['schema_version'] == 2:
        info = index['build_inputs']
        manifest = output / 'build-inputs.oci.json'
        if (not inputs.is_file() or inputs.is_symlink() or manifest.is_symlink() or not manifest.is_file()
                or inputs.stat().st_size != info['bytes'] or package.digest(inputs) != info['archive_sha256']
                or package.digest(manifest) != info['oci_manifest_sha256']
                or manifest.read_bytes() != registry.build_evidence_manifest(inputs)[0]):
            raise ValueError('Build input archive or manifest differs from index')
    with tarfile.open(evidence_path, 'r:gz') as tar:
        members = tar.getmembers()
        if len(members) != len(expected) or {member.name for member in members} != set(expected):
            raise ValueError('Evidence archive members differ from snapshot')
        for member in members:
            stream = tar.extractfile(member)
            if (not member.isfile() or stream is None or member.size != expected[member.name].stat().st_size
                    or hashlib.file_digest(stream, 'sha256').hexdigest() != package.digest(expected[member.name])):
                raise ValueError('Evidence archive differs from snapshot: ' + member.name)
    for group, info in index['groups'].items():
        archive = output / (group + '.tar.gz')
        manifest_path = output / (group + '.oci.json')
        if (archive.is_symlink() or not archive.is_file() or manifest_path.is_symlink() or not manifest_path.is_file()
                or not isinstance(info, dict) or set(info) != {'archive_sha256', 'bytes', 'oci_manifest_sha256'}
                or archive.stat().st_size != info['bytes'] or package.digest(archive) != info['archive_sha256']
                or package.digest(manifest_path) != info['oci_manifest_sha256']):
            raise ValueError('Group archive or manifest differs from index: ' + group)
        expected_manifest, _ = registry.build_group_manifest(archive, group)
        if manifest_path.read_bytes() != expected_manifest:
            raise ValueError('Group manifest does not match archive: ' + group)
        with tarfile.open(archive, 'r:gz') as tar:
            members = tar.getmembers()
            if len(members) != len(package.MODULES) or {m.name for m in members} != set(package.MODULES):
                raise ValueError('Invalid group archive members: ' + group)
            for member in members:
                expected = source['groups'][group][member.name]
                stream = tar.extractfile(member)
                if (not member.isfile() or stream is None or member.size != expected['bytes']
                        or hashlib.file_digest(stream, 'sha256').hexdigest() != expected['sha256']):
                    raise ValueError('Group archive differs from snapshot: ' + member.name)
    return index


def export(snapshot, output, build_inputs=None):
    index = package.validate(snapshot)
    if output.exists() or output.is_symlink():
        raise ValueError('Output already exists')
    output.mkdir(parents=True)
    if build_inputs is not None:
        build_inputs_export.verify(build_inputs, index['generation'])
        shutil.copyfile(build_inputs, output / 'build-inputs.tar.gz')
        build_inputs_export.verify(output / 'build-inputs.tar.gz', index['generation'])
    evidence_archive = output / 'evidence.tar.gz'
    with evidence_archive.open('xb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as tar:
            for path in evidence_paths(snapshot):
                info = tar.gettarinfo(str(path), arcname=path.relative_to(snapshot).as_posix())
                info.uid = info.gid = 0
                info.uname = info.gname = ''
                info.mtime = 0
                info.mode = 0o644
                with path.open('rb') as source:
                    tar.addfile(info, source)
    evidence_manifest, _ = registry.build_evidence_manifest(evidence_archive)
    (output / 'evidence.oci.json').write_bytes(evidence_manifest)
    evidence = {'archive_sha256': package.digest(evidence_archive), 'bytes': evidence_archive.stat().st_size,
                'oci_manifest_sha256': hashlib.sha256(evidence_manifest).hexdigest()}
    groups = {}
    referenced = {target['group'] for target in index['targets'].values()}
    if referenced != set(index['groups']):
        raise ValueError('Snapshot contains unreferenced module groups')
    for ident in sorted(referenced):
        archive = output / (ident + '.tar.gz')
        with archive.open('xb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as tar:
                for name in sorted(package.MODULES):
                    path = snapshot / 'groups' / ident / name
                    info = tar.gettarinfo(str(path), arcname=name)
                    info.uid = info.gid = 0
                    info.uname = info.gname = ''
                    info.mtime = 0
                    info.mode = 0o644
                    with path.open('rb') as source:
                        tar.addfile(info, source)
        with tarfile.open(archive, 'r:gz') as tar:
            members = tar.getmembers()
            if len(members) != len(package.MODULES) or {m.name for m in members} != set(package.MODULES):
                raise ValueError('Module group archive contents differ from the snapshot')
            for member in members:
                expected = index['groups'][ident][member.name]
                source = tar.extractfile(member)
                if (not member.isfile() or source is None or member.size != expected['bytes']
                        or hashlib.file_digest(source, 'sha256').hexdigest() != expected['sha256']):
                    raise ValueError('Module group archive content changed: ' + member.name)
        manifest, _ = registry.build_group_manifest(archive, ident)
        (output / (ident + '.oci.json')).write_bytes(manifest)
        groups[ident] = {'archive_sha256': package.digest(archive), 'bytes': archive.stat().st_size,
                         'oci_manifest_sha256': hashlib.sha256(manifest).hexdigest()}
    targets = {tag: {'kernel_build_id': target['kernel_build_id'],
                     'kernel_image_sha256': target['kernel_image_sha256'], 'group': target['group']}
               for tag, target in sorted(index['targets'].items())}
    result = {'schema_version': 1, 'generation': index['generation'], 'groups': groups, 'targets': targets, 'evidence': evidence,
              'runtime_admitted': False, 'release_ready': False}
    if build_inputs is not None:
        inputs_path = output / 'build-inputs.tar.gz'
        inputs_manifest, _ = registry.build_evidence_manifest(inputs_path)
        (output / 'build-inputs.oci.json').write_bytes(inputs_manifest)
        result['schema_version'] = 2
        result['build_inputs'] = {'archive_sha256': package.digest(inputs_path), 'bytes': inputs_path.stat().st_size,
                                  'oci_manifest_sha256': hashlib.sha256(inputs_manifest).hexdigest(),
                                  'corresponding_source_complete': False}
    (output / 'index.json').write_text(json.dumps(result, sort_keys=True, separators=(',', ':')) + '\n')
    index_manifest, _ = registry.build_index_manifest(output / 'index.json')
    (output / 'index.oci.json').write_bytes(index_manifest)
    verify_export(snapshot, output)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--snapshot', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--build-inputs', type=Path, help='Checked WSX build-input archive for this generation')
    args = parser.parse_args()
    result = export(args.snapshot, args.output, args.build_inputs)
    print(json.dumps({'groups': len(result['groups']), 'targets': len(result['targets']),
                      'runtime_admitted': False, 'release_ready': False}))


if __name__ == '__main__':
    main()
