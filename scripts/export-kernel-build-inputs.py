#!/usr/bin/env python3
"""Archive the recorded WSX module build inputs without asserting source completeness."""
import argparse
import gzip
import hashlib
import io
import importlib.util
import json
import os
from pathlib import Path
import re
import tarfile
import tempfile

spec = importlib.util.spec_from_file_location('kernel_build_generation_inputs', Path(__file__).with_name('kernel_build_generation.py'))
generation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generation)


def verify(archive, expected_generation):
    if (archive.is_symlink() or not archive.is_file() or archive.stat().st_size > 1024 * 1024 * 1024
            or not re.fullmatch(r'[0-9a-f]{64}', expected_generation)):
        raise ValueError('Invalid build input archive or generation')
    files, total = {}, 0
    with tarfile.open(archive, 'r:gz') as tar:
        for item in tar:
            path = Path(item.name)
            if (item.name in files or path.is_absolute() or '..' in path.parts or not item.isfile()
                    or item.size < 0 or item.size > 64 * 1024 * 1024):
                raise ValueError('Unsafe build input archive member')
            total += item.size
            if total > 128 * 1024 * 1024 or len(files) >= 10000:
                raise ValueError('Build input archive exceeds limits')
            stream = tar.extractfile(item)
            if stream is None:
                raise ValueError('Cannot read build input archive member')
            files[item.name] = stream.read()
    manifest = json.loads(files.get('manifest.json', b''))
    if (not isinstance(manifest, dict) or manifest.get('schema_version') != 1
            or manifest.get('generation') != expected_generation
            or manifest.get('corresponding_source_complete') is not False
            or manifest.get('release_ready') is not False or not isinstance(manifest.get('inputs'), dict)):
        raise ValueError('Invalid build input archive manifest')
    inputs = manifest['inputs']
    encoded = json.dumps({'schema_version': 1, 'inputs': inputs}, sort_keys=True, separators=(',', ':')).encode()
    if hashlib.sha256(encoded).hexdigest() != expected_generation:
        raise ValueError('Build input archive generation mismatch')
    expected = {'manifest.json'}
    for name, digest in inputs.items():
        if (not isinstance(name, str) or not name or not isinstance(digest, str)
                or not re.fullmatch(r'[0-9a-f]{64}', digest)):
            raise ValueError('Invalid build input identity')
        path = Path(name)
        if name != '@runner-dependencies' and (path.is_absolute() or '..' in path.parts or name.startswith('@')):
            raise ValueError('Unsafe build input identity')
        member = 'runner-dependencies.json' if name == '@runner-dependencies' else 'inputs/' + name
        if member not in files or hashlib.sha256(files[member]).hexdigest() != digest:
            raise ValueError('Build input archive digest mismatch: ' + name)
        expected.add(member)
    if set(files) != expected:
        raise ValueError('Unexpected build input archive member')
    return manifest


def export(root, output):
    root = root.resolve(strict=True)
    if output.exists() or output.is_symlink():
        raise ValueError('Build input archive already exists')
    ident, inputs = generation.generation(root)
    payloads = {}
    for name, expected in inputs.items():
        if name == '@runner-dependencies':
            evidence = Path(os.environ['WSX_KERNEL_RUNNER_DEPENDENCIES'])
            value = json.loads(evidence.read_text())
            data = json.dumps(value, sort_keys=True, separators=(',', ':')).encode()
            arcname = 'runner-dependencies.json'
        else:
            path = root / name
            if path.is_symlink() or not path.is_file():
                raise ValueError('Unsafe build input: ' + name)
            data = path.read_bytes()
            arcname = 'inputs/' + name
        if hashlib.sha256(data).hexdigest() != expected:
            raise ValueError('Build input changed while archiving: ' + name)
        payloads[arcname] = data
    manifest = {'schema_version': 1, 'generation': ident, 'inputs': inputs,
                'corresponding_source_complete': False, 'release_ready': False,
                'scope': 'Recorded WSX build inputs only; target kernel corresponding source and distribution review remain required'}
    payloads['manifest.json'] = (json.dumps(manifest, sort_keys=True, separators=(',', ':')) + '\n').encode()
    fd, temporary = tempfile.mkstemp(prefix='.' + output.name + '.', suffix='.tmp', dir=output.parent)
    temporary = Path(temporary)
    try:
        with os.fdopen(fd, 'wb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as tar:
                for name, data in sorted(payloads.items()):
                    info = tarfile.TarInfo(name)
                    info.size = len(data)
                    info.mode = 0o644
                    info.uid = info.gid = info.mtime = 0
                    tar.addfile(info, io.BytesIO(data))
        with tarfile.open(temporary, 'r:gz') as tar:
            members = tar.getmembers()
            if len(members) != len(payloads) or {m.name for m in members} != set(payloads):
                raise ValueError('Build input archive member mismatch')
            for member in members:
                stream = tar.extractfile(member)
                if not member.isfile() or stream is None or stream.read() != payloads[member.name]:
                    raise ValueError('Build input archive content mismatch: ' + member.name)
        if generation.generation(root)[0] != ident:
            raise ValueError('Build inputs changed while archiving')
        verify(temporary, ident)
        with temporary.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        size = temporary.stat().st_size
        # Linking within the same directory publishes atomically without replacing
        # an output that another process created during verification.
        os.link(temporary, output)
        return {'generation': ident, 'sha256': digest, 'bytes': size,
                'corresponding_source_complete': False, 'release_ready': False}
    finally:
        temporary.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(export(generation.ROOT, args.output), sort_keys=True))


if __name__ == '__main__':
    main()
