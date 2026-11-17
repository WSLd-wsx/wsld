#!/usr/bin/env python3
"""Validate, merge and archive untrusted kernel module package snapshots."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import shutil
import tarfile
import tempfile

MODULES = ('drm_kms_helper.ko', 'virtio_dma_buf.ko', 'virtio-gpu.ko',
           'virtio_msg.ko', 'virtio_msg_vsock.ko', 'wsx_virtio_msg_pci.ko')
TAG = re.compile(r'linux-msft-wsl-[0-9]+(?:\.[0-9]+){2,3}(?:-rc[1-9][0-9]*)?\Z')
SHA = re.compile(r'[0-9a-f]{64}\Z')
COMMIT = re.compile(r'[0-9a-f]{40}\Z')
BUILD_ID = re.compile(r'[0-9a-f]{8,128}\Z')
MAX_FILE = 64 * 1024 * 1024
MAX_TOTAL = 2 * 1024 * 1024 * 1024


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def group_id(files):
    encoded = json.dumps(files, sort_keys=True, separators=(',', ':')).encode()
    return hashlib.sha256(encoded).hexdigest()


def verify_build_inputs(build, expected_generation):
    inputs = build.get('build_inputs')
    if inputs is None:
        return  # Older diagnostic snapshots did not record the input manifest.
    if (not isinstance(inputs, dict) or not inputs
            or any(not isinstance(k, str) or not k or not isinstance(v, str) or not SHA.fullmatch(v)
                   for k, v in inputs.items())):
        raise ValueError('Invalid build input manifest')
    encoded = json.dumps({'schema_version': 1, 'inputs': inputs}, sort_keys=True, separators=(',', ':')).encode()
    if hashlib.sha256(encoded).hexdigest() != expected_generation:
        raise ValueError('Build input manifest does not match generation')
    dependencies = build.get('runner_dependencies')
    if '@runner-dependencies' in inputs:
        if not isinstance(dependencies, dict):
            raise ValueError('Runner dependency evidence is missing')
        encoded = json.dumps(dependencies, sort_keys=True, separators=(',', ':')).encode()
        if hashlib.sha256(encoded).hexdigest() != inputs['@runner-dependencies']:
            raise ValueError('Runner dependency evidence does not match generation')
    elif dependencies is not None:
        raise ValueError('Runner dependency evidence is not included in generation')


def validate(root):
    if root.is_symlink() or not root.is_dir():
        raise ValueError('Package root is unsafe')
    index_file = root / 'index.json'
    if index_file.is_symlink() or not index_file.is_file() or index_file.stat().st_size > 4 * 1024 * 1024:
        raise ValueError('Package index is missing or too large')
    index = json.loads(index_file.read_text(encoding='utf-8'))
    if (set(index) != {'schema_version', 'generation', 'groups', 'targets'}
            or index['schema_version'] not in (1, 2, 3) or not isinstance(index['generation'], str)
            or not SHA.fullmatch(index['generation'])):
        raise ValueError('Unsupported package index')
    groups, targets = index['groups'], index['targets']
    if not isinstance(groups, dict) or not isinstance(targets, dict) or len(groups) > 512 or len(targets) > 4096:
        raise ValueError('Invalid package index size')
    expected = {'index.json'}
    total = index_file.stat().st_size
    for ident, files in groups.items():
        if not SHA.fullmatch(ident) or not isinstance(files, dict) or set(files) != set(MODULES) or group_id(files) != ident:
            raise ValueError('Invalid module group identity')
        for name, item in files.items():
            if not isinstance(item, dict) or set(item) != {'sha256', 'bytes'}:
                raise ValueError('Invalid module metadata')
            size = item['bytes']
            if not isinstance(size, int) or isinstance(size, bool) or not 0 < size <= MAX_FILE or not SHA.fullmatch(item['sha256']):
                raise ValueError('Invalid module size or digest')
            rel = f'groups/{ident}/{name}'
            file = root / rel
            if file.is_symlink() or not file.is_file() or file.stat().st_size != size or digest(file) != item['sha256']:
                raise ValueError('Module content mismatch: ' + rel)
            expected.add(rel)
            total += size
    for tag, item in targets.items():
        fields = {'source_commit', 'group', 'kernel_build_id', 'kernel_image_sha256', 'audit_sha256'}
        if index['schema_version'] >= 2:
            fields.add('build_sha256')
        if index['schema_version'] == 3:
            fields.add('build_source_commit')
        if not TAG.fullmatch(tag) or not isinstance(item, dict) or set(item) != fields:
            raise ValueError('Invalid target metadata')
        if (not COMMIT.fullmatch(item['source_commit']) or item['group'] not in groups
                or not BUILD_ID.fullmatch(item['kernel_build_id']) or not SHA.fullmatch(item['kernel_image_sha256'])
                or not SHA.fullmatch(item['audit_sha256'])):
            raise ValueError('Invalid target identity')
        if index['schema_version'] == 3 and not COMMIT.fullmatch(item['build_source_commit']):
            raise ValueError('Invalid module build source identity')
        rel = f"audits/{item['audit_sha256']}.json"
        file = root / rel
        if file.is_symlink() or not file.is_file() or file.stat().st_size > 8 * 1024 * 1024 or digest(file) != item['audit_sha256']:
            raise ValueError('Audit content mismatch: ' + tag)
        # Audit bytes are retained evidence, not an authority to admit a target.
        expected.add(rel)
        total += file.stat().st_size
        if index['schema_version'] >= 2:
            if not SHA.fullmatch(item['build_sha256']):
                raise ValueError('Invalid build evidence digest')
            rel = f"builds/{item['build_sha256']}.json"
            file = root / rel
            if file.is_symlink() or not file.is_file() or file.stat().st_size > 8 * 1024 * 1024 or digest(file) != item['build_sha256']:
                raise ValueError('Build evidence content mismatch: ' + tag)
            build = json.loads(file.read_text(encoding='utf-8'))
            if (build.get('schema_version') != 1 or build.get('build_generation') != index['generation']
                    or build.get('kernel_source_commit') != item.get('build_source_commit', item['source_commit'])
                    or build.get('modules') != groups[item['group']]):
                raise ValueError('Build evidence does not match indexed target or module group')
            verify_build_inputs(build, index['generation'])
            expected.add(rel)
            total += file.stat().st_size
    actual = set()
    for path in root.rglob('*'):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError('Package contains an unsafe file type')
        if path.is_file():
            actual.add(path.relative_to(root).as_posix())
    if actual != expected or total > MAX_TOTAL:
        raise ValueError('Unexpected package files or size')
    return index


def extract(archive, destination):
    if destination.exists() or destination.is_symlink():
        raise ValueError('Package destination already exists')
    destination.mkdir(parents=True)
    total = 0
    with tarfile.open(archive, 'r:gz') as tar:
        seen = set()
        for item in tar:
            path = Path(item.name)
            if (item.name in seen or path.is_absolute() or '..' in path.parts or not path.parts
                    or not item.isfile() or item.size < 0 or item.size > MAX_FILE):
                raise ValueError('Unsafe package archive member')
            seen.add(item.name)
            total += item.size
            if total > MAX_TOTAL or len(seen) > 10000:
                raise ValueError('Package archive exceeds limits')
            target = destination / path
            target.parent.mkdir(parents=True, exist_ok=True)
            source = tar.extractfile(item)
            if source is None:
                raise ValueError('Cannot read package archive member')
            with target.open('xb') as out:
                shutil.copyfileobj(source, out, 1024 * 1024)
    return validate(destination)


def merge(base, update, destination):
    left, right = validate(base), validate(update)
    if destination.exists() or destination.is_symlink():
        raise ValueError('Merge destination already exists')
    if left['generation'] != right['generation']:
        raise ValueError('Build input generation changed; rebuild all targets instead of reusing old results')
    if left['schema_version'] != right['schema_version']:
        raise ValueError('Snapshot evidence formats differ; migrate explicitly before merging')
    for key in ('groups', 'targets'):
        for name in left[key].keys() & right[key].keys():
            if left[key][name] != right[key][name]:
                raise ValueError(f'Immutable package {key} conflict: {name}')
    destination.mkdir(parents=True)
    index = {'schema_version': left['schema_version'], 'generation': left['generation'], 'groups': left['groups'] | right['groups'],
             'targets': left['targets'] | right['targets']}
    for root in (base, update):
        for path in sorted(root.rglob('*')):
            if not path.is_file() or path.name == 'index.json' and path.parent == root:
                continue
            target = destination / path.relative_to(root)
            target.parent.mkdir(parents=True, exist_ok=True)
            if target.exists():
                if digest(target) != digest(path):
                    raise ValueError('Conflicting package content')
            else:
                shutil.copyfile(path, target)
    (destination / 'index.json').write_text(json.dumps(index, sort_keys=True, indent=2) + '\n')
    return validate(destination)


def archive(root, output):
    validate(root)
    if output.exists() or output.is_symlink():
        raise ValueError('Package archive already exists')
    with output.open('xb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, compresslevel=6, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as tar:
            for path in sorted(root.rglob('*')):
                if path.is_file():
                    info = tar.gettarinfo(str(path), arcname=path.relative_to(root).as_posix())
                    info.uid = info.gid = 0
                    info.uname = info.gname = ''
                    info.mtime = 0
                    info.mode = 0o644
                    with path.open('rb') as stream:
                        tar.addfile(info, stream)
    with tempfile.TemporaryDirectory() as temp:
        extract(output, Path(temp) / 'check')
    return digest(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    for action in ('validate', 'archive', 'extract', 'merge'):
        cmd = sub.add_parser(action)
        cmd.add_argument('input', type=Path)
        if action == 'merge':
            cmd.add_argument('update', type=Path)
        if action != 'validate':
            cmd.add_argument('output', type=Path)
    args = parser.parse_args()
    if args.action == 'validate':
        result = validate(args.input)
    elif args.action == 'archive':
        result = {'archive_sha256': archive(args.input, args.output)}
    elif args.action == 'extract':
        result = extract(args.input, args.output)
    else:
        result = merge(args.input, args.update, args.output)
    print(json.dumps({'verified': True, 'scope': 'Package bytes only; no runtime or release admission',
                      'groups': len(result['groups']) if 'groups' in result else None, **({'archive_sha256': result['archive_sha256']} if 'archive_sha256' in result else {})}))


if __name__ == '__main__':
    main()
