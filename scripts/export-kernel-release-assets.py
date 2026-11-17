#!/usr/bin/env python3
"""Export checked diagnostic kernel groups as content-addressed release assets."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shutil
import tarfile
import tempfile

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('kernel_group_release_export', HERE / 'export-kernel-groups.py')
groups = importlib.util.module_from_spec(spec)
spec.loader.exec_module(groups)
HEX = re.compile(r'[0-9a-f]{64}\Z')


def descriptor(path, prefix):
    sha = groups.package.digest(path)
    return {'asset': f'{prefix}-{sha}.tar.gz', 'sha256': sha, 'bytes': path.stat().st_size}


def validate(index):
    if (not isinstance(index, dict) or set(index) != {'schema_version', 'generation', 'groups', 'targets',
                                                       'evidence', 'build_inputs', 'release_ready'}
            or index['schema_version'] != 1 or not isinstance(index['generation'], str)
            or not HEX.fullmatch(index['generation']) or index['release_ready'] is not False
            or not isinstance(index['groups'], dict) or not index['groups']
            or not isinstance(index['targets'], dict) or not index['targets']):
        raise ValueError('Invalid kernel release index')
    def check(value, prefix, extra=()):
        if (not isinstance(value, dict) or set(value) != {'asset', 'sha256', 'bytes', *extra}
                or not isinstance(value['sha256'], str) or not HEX.fullmatch(value['sha256'])
                or value['asset'] != f"{prefix}-{value['sha256']}.tar.gz"
                or not isinstance(value['bytes'], int) or isinstance(value['bytes'], bool)
                or not 0 < value['bytes'] <= 1024 * 1024 * 1024):
            raise ValueError('Invalid kernel release asset descriptor')
    check(index['evidence'], 'evidence')
    check(index['build_inputs'], 'build-inputs', ('corresponding_source_complete',))
    if index['build_inputs']['corresponding_source_complete'] is not False:
        raise ValueError('Unreviewed corresponding-source claim')
    for group, item in index['groups'].items():
        if not isinstance(group, str) or not HEX.fullmatch(group):
            raise ValueError('Invalid kernel module group identity')
        check(item, 'modules')
    used = set()
    for tag, target in index['targets'].items():
        if (not groups.package.TAG.fullmatch(tag) or not isinstance(target, dict)
                or set(target) != {'kernel_build_id', 'kernel_image_sha256', 'group'}
                or not isinstance(target['kernel_build_id'], str) or not groups.package.BUILD_ID.fullmatch(target['kernel_build_id'])
                or not isinstance(target['kernel_image_sha256'], str) or not HEX.fullmatch(target['kernel_image_sha256'])
                or target['group'] not in index['groups']):
            raise ValueError('Invalid kernel release target')
        used.add(target['group'])
    if used != set(index['groups']):
        raise ValueError('Unreferenced kernel release group')
    return index


def verify(output, snapshot):
    index_path = output / 'index.json'
    if output.is_symlink() or index_path.is_symlink() or not index_path.is_file() or index_path.stat().st_size > 1024 * 1024:
        raise ValueError('Unsafe kernel release index')
    index = validate(json.loads(index_path.read_text()))
    source = groups.package.validate(snapshot)
    expected_targets = {tag: {'kernel_build_id': v['kernel_build_id'], 'kernel_image_sha256': v['kernel_image_sha256'],
                              'group': v['group']} for tag, v in source['targets'].items()}
    if (index['generation'] != source['generation'] or index['targets'] != expected_targets
            or set(index['groups']) != set(source['groups'])):
        raise ValueError('Kernel release index differs from checked snapshot')
    for item in [index['evidence'], index['build_inputs'], *index['groups'].values()]:
        path = output / item['asset']
        if (path.is_symlink() or not path.is_file() or path.stat().st_size != item['bytes']
                or groups.package.digest(path) != item['sha256']):
            raise ValueError('Kernel release asset digest mismatch: ' + item['asset'])
    groups.build_inputs_export.verify(output / index['build_inputs']['asset'], index['generation'])
    expected_evidence = {path.relative_to(snapshot).as_posix(): path for path in groups.evidence_paths(snapshot)}
    with tarfile.open(output / index['evidence']['asset'], 'r:gz') as archive:
        members = archive.getmembers()
        if len(members) != len(expected_evidence) or {member.name for member in members} != set(expected_evidence):
            raise ValueError('Kernel release evidence differs from snapshot')
        for member in members:
            stream = archive.extractfile(member)
            expected = expected_evidence[member.name]
            if (not member.isfile() or stream is None or member.size != expected.stat().st_size
                    or hashlib.file_digest(stream, 'sha256').hexdigest() != groups.package.digest(expected)):
                raise ValueError('Kernel release evidence differs from snapshot: ' + member.name)
    for group, item in index['groups'].items():
        groups.registry.verify_group_archive(output / item['asset'], group)
    return index


def export(snapshot, build_inputs, output):
    if output.exists() or output.is_symlink():
        raise ValueError('Kernel release asset output already exists')
    with tempfile.TemporaryDirectory(prefix='wsx-release-assets-') as temporary:
        intermediate = Path(temporary) / 'groups'
        checked = groups.export(snapshot, intermediate, build_inputs)
        output.mkdir(parents=True)
        def copy(source, prefix):
            value = descriptor(source, prefix)
            destination = output / value['asset']
            if destination.exists():
                if groups.package.digest(destination) != value['sha256']:
                    raise ValueError('Kernel release asset collision')
            else:
                shutil.copyfile(source, destination)
            return value
        module_groups = {ident: copy(intermediate / (ident + '.tar.gz'), 'modules')
                         for ident in checked['groups']}
        evidence = copy(intermediate / 'evidence.tar.gz', 'evidence')
        inputs = copy(intermediate / 'build-inputs.tar.gz', 'build-inputs')
        inputs['corresponding_source_complete'] = False
        result = {'schema_version': 1, 'generation': checked['generation'], 'groups': module_groups,
                  'targets': checked['targets'], 'evidence': evidence, 'build_inputs': inputs,
                  'release_ready': False}
        (output / 'index.json').write_text(json.dumps(result, sort_keys=True, separators=(',', ':')) + '\n')
        verify(output, snapshot)
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--snapshot', required=True, type=Path)
    parser.add_argument('--build-inputs', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    result = export(args.snapshot, args.build_inputs, args.output)
    print(json.dumps({'groups': len(result['groups']), 'targets': len(result['targets']),
                      'generation': result['generation'], 'release_ready': False}))


if __name__ == '__main__':
    main()
