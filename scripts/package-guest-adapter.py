#!/usr/bin/env python3
# Original WSX code, BUSL-1.1. See LICENSING.md.
"""Package supplied, reviewed Guest adapter files; this does not establish trust."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat

FILES = ('protocol.py', 'desktop.py', 'display.py', 'retirement.py', 'device-view.py', 'wsx-guest.py')
MAX_FILE = 2 * 1024 * 1024
MAX_TOTAL = 8 * 1024 * 1024


def package(source, destination, implementation):
    source = Path(source)
    destination = Path(destination)
    if source.is_symlink() or not source.is_dir(): raise ValueError('Unsafe adapter source directory')
    if destination.exists() or destination.is_symlink(): raise ValueError('Destination already exists')
    files = {}
    payloads = {}
    total = 0
    for name in FILES:
        fd = os.open(source / name, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
        with os.fdopen(fd, 'rb') as stream:
            info = os.fstat(stream.fileno())
            if not stat.S_ISREG(info.st_mode) or not 0 < info.st_size <= MAX_FILE:
                raise ValueError('Invalid adapter source: ' + name)
            data = stream.read(MAX_FILE + 1)
        if len(data) != info.st_size: raise ValueError('Adapter source changed while reading: ' + name)
        total += len(data)
        if total > MAX_TOTAL: raise ValueError('Adapter bundle exceeds limit')
        payloads[name] = data
        files[name] = dict(sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))
    if implementation not in ('hyprland', 'plasma'): raise ValueError('Unsupported adapter implementation')
    manifest = dict(schema_version=1, kind='wsx-guest-adapter', protocol_major=2,
                    implementation=implementation, files=files)
    raw = (json.dumps(manifest, sort_keys=True, separators=(',', ':')) + '\n').encode()
    destination.mkdir(mode=0o700)
    for name, data in [('manifest.json', raw), *payloads.items()]:
        (destination/name).write_bytes(data)
    return hashlib.sha256(raw).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--implementation', choices=('hyprland','plasma'), required=True)
    args = parser.parse_args()
    print('Manifest SHA-256:', package(args.source, args.output, args.implementation))
    print('The digest requires an independently trusted catalog; activation is not implemented.')


if __name__ == '__main__': main()
