#!/usr/bin/env python3
"""Stage verified version-addressed kernel archives and a bounded local index."""
import argparse
import hashlib
import json
import re
from pathlib import Path
import shutil
import tarfile

VERSION = re.compile(r'[0-9]{1,9}(?:\.[0-9]{1,9}){2,3}(?:-(?:(?:rc|RC)[1-9][0-9]{0,8}(?:-[0-9]+)?|[0-9]+))?')


def check_module_headers(archive: Path) -> None:
    with tarfile.open(archive, 'r:gz') as modules:
        for member in modules:
            if (member.type not in (tarfile.REGTYPE, tarfile.AREGTYPE)
                    or member.pax_headers or member.offset_data != member.offset + 512):
                raise ValueError('Unsupported runtime module tar header: ' + archive.name)


def stage(source: Path, destination: Path) -> None:
    if source.is_symlink() or not source.is_dir() or destination.exists() or destination.is_symlink():
        raise ValueError('Unsafe source or existing destination')
    entries = []
    for archive in sorted(source.glob('modules_*.tar.gz')):
        if archive.is_symlink() or not archive.is_file():
            raise ValueError('Unsafe module archive')
        match = re.fullmatch(r'modules_([A-Za-z0-9.-]{1,64})\.tar\.gz', archive.name)
        if not match:
            raise ValueError('Invalid module archive name')
        version = match.group(1)
        if not VERSION.fullmatch(version):
            raise ValueError('Invalid kernel version: ' + version)
        candidates = []
        for path in source.glob('modules_' + version + '-*.json'):
            m = re.fullmatch(r'modules_' + re.escape(version) + r'-(\d+)-(\d+)\.json', path.name)
            if m:
                if path.is_symlink() or not path.is_file() or path.stat().st_size > 65536:
                    raise ValueError('Unsafe module metadata')
                candidates.append(((int(m.group(1)), int(m.group(2))), path))
        if not candidates:
            raise ValueError('Missing module metadata: ' + version)
        metadata = json.loads(max(candidates)[1].read_bytes())
        size = archive.stat().st_size
        with archive.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        if not 0 < size <= 1024 * 1024 * 1024 or metadata.get('schema_version') != 1 or metadata.get('kernel_version') != version or metadata.get('archive') != archive.name or metadata.get('archive_bytes') != size or metadata.get('archive_sha256') != digest:
            raise ValueError('Module archive metadata mismatch: ' + version)
        check_module_headers(archive)
        entries.append((archive, {'kernel_version': version, 'archive': archive.name, 'sha256': digest, 'bytes': size}))
    if not entries or len(entries) > 1000:
        raise ValueError('No module archives or too many archives')
    destination.mkdir(parents=True)
    for archive, _ in entries:
        shutil.copyfile(archive, destination / archive.name)
    (destination / 'index.json').write_text(json.dumps({'schema_version': 1, 'modules': [entry for _, entry in entries]}, separators=(',', ':')) + '\n', encoding='utf-8')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    stage(args.source, args.destination)
