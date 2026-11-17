#!/usr/bin/env python3
"""Verify a compressed Git source archive against its Git tree object ID."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import tarfile

MAX_FILES = 250000
MAX_BYTES = 8 * 1024 * 1024 * 1024


def git_blob(stream, size):
    h = hashlib.sha1(b'blob ' + str(size).encode() + b'\0')
    remaining = size
    while remaining:
        data = stream.read(min(1024 * 1024, remaining))
        if not data: raise ValueError('Truncated archive member')
        h.update(data); remaining -= len(data)
    return h.digest()


def compute_tree(archive, strip=0):
    if strip not in (0, 1) or archive.is_symlink() or not archive.is_file():
        raise ValueError('Invalid source archive or prefix setting')
    root = {}; total = 0; count = 0; prefix = None
    with tarfile.open(archive, 'r|gz') as tar:
        for member in tar:
            parts = member.name.rstrip('/').split('/')
            if (not parts or any(p in ('', '.', '..') for p in parts) or member.name.startswith('/')
                    or any('\\' in p or '\x00' in p for p in parts)):
                raise ValueError('Unsafe kernel source archive path')
            if strip:
                if prefix is None: prefix = parts[0]
                if parts[0] != prefix: raise ValueError('Multiple source archive prefixes')
                parts = parts[1:]
                if not parts:
                    if not member.isdir(): raise ValueError('Invalid source archive root')
                    continue
            if member.isdir(): continue
            count += 1; total += member.size
            if count > MAX_FILES or total > MAX_BYTES or member.size > 256 * 1024 * 1024:
                raise ValueError('Source archive exceeds verification limits')
            if member.isfile():
                stream = tar.extractfile(member)
                if stream is None: raise ValueError('Unreadable source archive member')
                obj = git_blob(stream, member.size)
                mode = b'100755' if member.mode & 0o111 else b'100644'
            elif member.issym():
                data = member.linkname.encode('utf-8', 'surrogateescape')
                obj = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).digest()
                mode = b'120000'
            else:
                raise ValueError('Unsupported source archive member type')
            directory = root
            for part in parts[:-1]:
                current = directory.setdefault(part, {})
                if not isinstance(current, dict): raise ValueError('Conflicting source archive path')
                directory = current
            if parts[-1] in directory: raise ValueError('Duplicate source archive path')
            directory[parts[-1]] = (mode, obj)
    if not count: raise ValueError('Empty source archive')
    def tree(node):
        body = bytearray()
        for name, value in sorted(node.items(), key=lambda pair: pair[0].encode('utf-8', 'surrogateescape') + (b'/' if isinstance(pair[1], dict) else b'')):
            if isinstance(value, dict): mode, obj = b'40000', tree(value)
            else: mode, obj = value
            body.extend(mode + b' ' + name.encode('utf-8', 'surrogateescape') + b'\0' + obj)
        return hashlib.sha1(b'tree ' + str(len(body)).encode() + b'\0' + body).digest()
    return {'git_tree': tree(root).hex(), 'files': count, 'uncompressed_bytes': total}


def verify(archive, expected_tree, strip=0):
    if not re.fullmatch(r'[0-9a-f]{40}', expected_tree): raise ValueError('Invalid Git tree ID')
    result = compute_tree(archive, strip)
    if result['git_tree'] != expected_tree: raise ValueError('Source archive does not match the expected Git tree')
    with archive.open('rb') as f: result['sha256'] = hashlib.file_digest(f, 'sha256').hexdigest()
    result['bytes'] = archive.stat().st_size
    return result


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive',type=Path); parser.add_argument('--tree',required=True)
    parser.add_argument('--strip',type=int,choices=(0,1),default=0)
    args=parser.parse_args()
    print(json.dumps(verify(args.archive,args.tree,args.strip),sort_keys=True))
