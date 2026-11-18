#!/usr/bin/env python3
"""Extract a reviewed runtime ZIP without trusting its paths or file types."""
import pathlib
import stat
import sys
import zipfile

MAX_BYTES = 2 * 1024 * 1024 * 1024
MAX_FILES = 20000


def extract(archive: pathlib.Path, destination: pathlib.Path) -> None:
    if destination.exists():
        raise ValueError("Runtime destination already exists")
    with zipfile.ZipFile(archive) as zipped:
        entries = zipped.infolist()
        if len(entries) > MAX_FILES or sum(entry.file_size for entry in entries) > MAX_BYTES:
            raise ValueError("Runtime archive exceeds extraction limits")
        seen = set()
        for entry in entries:
            name = entry.filename
            path = pathlib.PurePosixPath(name)
            if (not name or '\\' in name or ':' in name or name.startswith('/') or
                    any(part in ('', '.', '..') for part in name.rstrip('/').split('/'))):
                raise ValueError("Unsafe runtime archive path")
            normalized = '/'.join(path.parts).casefold()
            if normalized in seen:
                raise ValueError("Duplicate runtime archive path")
            seen.add(normalized)
            kind = (entry.external_attr >> 16) & 0o170000
            if kind and kind not in (stat.S_IFREG, stat.S_IFDIR):
                raise ValueError("Unsupported runtime archive file type")
            if entry.flag_bits & 1:
                raise ValueError("Encrypted runtime archive is unsupported")
        destination.mkdir(parents=True)
        for entry in entries:
            output = destination.joinpath(*pathlib.PurePosixPath(entry.filename).parts)
            if entry.is_dir():
                output.mkdir(parents=True, exist_ok=True)
            else:
                output.parent.mkdir(parents=True, exist_ok=True)
                with zipped.open(entry) as source, output.open('xb') as target:
                    while chunk := source.read(1024 * 1024):
                        target.write(chunk)
    for required in ('runtime-catalog.json', 'guest-adapter-catalog.json'):
        if not (destination / required).is_file():
            raise ValueError(f"Missing runtime bundle input: {required}")
    if not (destination / 'runtime').is_dir():
        raise ValueError('Missing runtime bundle directory')


if __name__ == '__main__':
    if len(sys.argv) != 3:
        raise SystemExit('Usage: extract-community-runtime.py ARCHIVE DESTINATION')
    extract(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]))
