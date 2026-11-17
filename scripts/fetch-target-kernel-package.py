#!/usr/bin/env python3
"""Fetch an exact-version WSL kernel package for diagnostic target builds."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
from urllib.request import Request, urlopen

spec = importlib.util.spec_from_file_location('wsl_kernel_feed', Path(__file__).with_name('wsl-kernel-feed.py'))
feed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(feed)
MAX_INDEX = 1024 * 1024
MAX_PACKAGE = 512 * 1024 * 1024


def fetch(tag, root, opener=urlopen, expected_version=None):
    if not feed.TAG.fullmatch(tag):
        raise ValueError('Unsupported kernel tag for Microsoft package lookup')
    root = root.resolve(strict=True)
    inputs = root / 'target-inputs'
    packages = root / 'kernel-packages'
    if inputs.is_symlink() or packages.is_symlink():
        raise ValueError('Unsafe package cache directory')
    inputs.mkdir(exist_ok=True)
    packages.mkdir(exist_ok=True)
    target_input = inputs / (tag + '.json')
    if target_input.exists() or target_input.is_symlink():
        raise ValueError('Target package input already exists; verify before reuse')
    headers = {'User-Agent': 'WSX-kernel-builder', 'Accept': 'application/json'}
    with opener(Request(feed.BASE + '/index.json', headers=headers), timeout=60) as response:
        raw = response.read(MAX_INDEX + 1)
    if len(raw) > MAX_INDEX:
        raise ValueError('Kernel package feed index exceeds size limit')
    candidates = feed.candidates(json.loads(raw), tag)
    if not candidates:
        raise ValueError('Microsoft feed has no exact matching kernel package')
    if expected_version is not None:
        candidates = [item for item in candidates if item['version'] == expected_version]
        if not candidates: raise ValueError('Exact requested kernel package is unavailable')
    candidate = candidates[0]
    target = packages / (candidate['version'] + '.nupkg')
    if target.is_symlink() or (target.exists() and not target.is_file()):
        raise ValueError('Unsafe cached kernel package')
    descriptor, name = tempfile.mkstemp(prefix=candidate['version'] + '-', suffix='.partial', dir=packages)
    partial = Path(name)
    digest, total = hashlib.sha256(), 0
    try:
        with os.fdopen(descriptor, 'wb') as output:
            with opener(Request(candidate['url'], headers={'User-Agent': 'WSX-kernel-builder'}), timeout=120) as response:
                declared = response.headers.get('Content-Length')
                if declared is not None and (not declared.isdecimal() or int(declared) > MAX_PACKAGE):
                    raise ValueError('Invalid kernel package content length')
                while True:
                    block = response.read(1024 * 1024)
                    if not block:
                        break
                    total += len(block)
                    if total > MAX_PACKAGE:
                        raise ValueError('Kernel package exceeds size limit')
                    digest.update(block)
                    output.write(block)
                if not total or (declared is not None and total != int(declared)):
                    raise ValueError('Kernel package download is incomplete')
            output.flush()
            os.fsync(output.fileno())
        try:
            os.link(partial, target)
        except FileExistsError:
            if target.is_symlink() or not target.is_file() or target.stat().st_size != total:
                raise ValueError('Existing kernel package differs from the completed download; partial retained')
            with target.open('rb') as existing:
                existing_hash = hashlib.file_digest(existing, 'sha256').hexdigest()
            if existing_hash != digest.hexdigest():
                raise ValueError('Existing kernel package differs from the completed download; partial retained')
        partial.unlink()
    except BaseException:
        # Retain partial downloads as diagnostic evidence; never treat them as a cache hit.
        raise
    value = {'version': candidate['version'], 'sha256': digest.hexdigest(),
             'package': target.relative_to(root).as_posix()}
    with target_input.open('x', encoding='utf-8') as output:
        json.dump(value, output, indent=2)
        output.write('\n')
    evidence = {'schema_version': 1, 'tag': tag, 'package_version': candidate['version'],
                'package_sha256': digest.hexdigest(), 'bytes': total,
                'feed_index_sha256': hashlib.sha256(raw).hexdigest(),
                'publisher_verified': False, 'source_correspondence_verified': False,
                'release_ready': False}
    with (inputs / (tag + '.download.json')).open('x', encoding='utf-8') as output:
        json.dump(evidence, output, indent=2)
        output.write('\n')
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--coverage', required=True, type=Path)
    parser.add_argument('--version', help='Exact WSL kernel package version')
    args = parser.parse_args()
    print(json.dumps(fetch(args.tag, args.coverage, expected_version=args.version)))


if __name__ == '__main__':
    main()
