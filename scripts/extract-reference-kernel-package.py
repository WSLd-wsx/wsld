#!/usr/bin/env python3
"""Extract the pinned reference image from an already downloaded WSL package."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

PACKAGE_SHA256 = '42bcccebdde523ac272c97595a8e8d494db6b3c815bbfb8f86a1b87e9a397ac4'
FILES = {
    'build/native/bin/x64/kernel': ('kernel', '4845ac8718804c91f3b5cbb9c09c856732612af4e1729faf402f727b0f3285b6', 32 * 1024 * 1024),
    'build/native/bin/x64/kernel_config': ('kernel_config', '350bb895c0af6c4dbb90a596385baaa4cfb58dcc0c36c9be166edc9517bac760', 1024 * 1024),
}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def extract(package, output):
    if digest(package) != PACKAGE_SHA256:
        raise ValueError('Reference kernel package digest mismatch')
    if output.exists() or output.is_symlink():
        raise ValueError('Output directory already exists')
    values = {}
    with zipfile.ZipFile(package) as archive:
        for name, (destination, expected, limit) in FILES.items():
            matches = [item for item in archive.infolist() if item.filename == name]
            if len(matches) != 1 or matches[0].is_dir() or matches[0].file_size > limit:
                raise ValueError('Missing, ambiguous or oversized reference kernel member')
            data = archive.read(matches[0])
            if hashlib.sha256(data).hexdigest() != expected:
                raise ValueError('Reference kernel member digest mismatch')
            values[destination] = data
    output.mkdir()
    for name, data in values.items():
        with (output / name).open('xb') as target:
            target.write(data)
    evidence = {'schema_version': 1, 'package_sha256': PACKAGE_SHA256,
                'members': {name: sha for name, sha, _ in FILES.values()},
                'scope': 'Pinned bytes only; source correspondence and publisher signature not proven'}
    with (output / 'evidence.json').open('x') as target:
        json.dump(evidence, target, indent=2)
        target.write('\n')
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(extract(args.package, args.output)))


if __name__ == '__main__':
    main()
