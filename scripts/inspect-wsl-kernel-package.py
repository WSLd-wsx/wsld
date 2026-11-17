#!/usr/bin/env python3
"""Inspect a pinned Microsoft WSL kernel package for diagnostic builds."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SHA = re.compile(r'[0-9a-f]{64}\Z')
VERSION = re.compile(r'[0-9]+(?:\.[0-9]+){3}-[0-9]+\Z')
MEMBERS = {'Microsoft.WSL.Kernel.nuspec': 65536,
           'build/native/bin/x64/kernel': 64 * 1024 * 1024,
           'build/native/bin/x64/kernel_config': 2 * 1024 * 1024,
           'build/native/bin/kernel_build_metadata.json': 65536}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def inspect(archive, expected_sha256, expected_version, destination):
    if not SHA.fullmatch(expected_sha256) or not VERSION.fullmatch(expected_version):
        raise ValueError('Invalid expected package identity')
    if archive.is_symlink() or not archive.is_file() or archive.stat().st_size > 512 * 1024 * 1024:
        raise ValueError('Unsafe kernel package')
    with archive.open('rb') as stream:
        actual = hashlib.file_digest(stream, 'sha256').hexdigest()
    if actual != expected_sha256:
        raise ValueError('Kernel package digest mismatch')
    if destination.exists() or destination.is_symlink():
        raise ValueError('Output directory already exists')
    contents = {}
    with zipfile.ZipFile(archive) as package:
        for name, limit in MEMBERS.items():
            matches = [entry for entry in package.infolist() if entry.filename == name]
            if len(matches) != 1 or matches[0].is_dir() or matches[0].file_size > limit:
                raise ValueError('Missing, ambiguous or oversized package member: ' + name)
            contents[name] = package.read(matches[0])
    xml = contents['Microsoft.WSL.Kernel.nuspec']
    if b'<!DOCTYPE' in xml.upper() or b'<!ENTITY' in xml.upper():
        raise ValueError('Unsafe package manifest')
    root = ET.fromstring(xml)
    metadata = next((item for item in root if item.tag.endswith('}metadata') or item.tag == 'metadata'), None)
    entries = [] if metadata is None else [(item.tag.rsplit('}', 1)[-1], item.text) for item in metadata]
    ids = [value for key, value in entries if key == 'id']
    versions = [value for key, value in entries if key == 'version']
    if ids != ['Microsoft.WSL.Kernel'] or versions != [expected_version]:
        raise ValueError('Kernel package metadata does not match requested identity')
    build_metadata = json.loads(contents['build/native/bin/kernel_build_metadata.json'])
    if not isinstance(build_metadata, dict):
        raise ValueError('Invalid kernel build metadata')
    kernel = contents['build/native/bin/x64/kernel']
    config = contents['build/native/bin/x64/kernel_config']
    extractor = runpy.run_path(str(ROOT / 'core/guest/kernel-exports.py'))
    notes = extractor['embedded_notes'](kernel)
    exports, build_id, elf_sha = extractor['exports'](kernel, notes)
    destination.mkdir(parents=True)
    (destination / 'kernel').write_bytes(kernel)
    (destination / 'kernel_config').write_bytes(config)
    (destination / 'kernel_notes').write_bytes(notes)
    (destination / 'kernel-exports.json').write_text(json.dumps({
        'kernel_build_id': build_id, 'kernel_image_sha256': sha(kernel),
        'vmlinux_sha256': elf_sha, 'symbols': exports,
        'scope': 'Image-internal notes; publisher and running-kernel identity are unverified'}, indent=2) + '\n')
    report = {'schema_version': 1, 'package_version': expected_version,
              'package_sha256': actual, 'kernel_image_sha256': sha(kernel),
              'kernel_config_sha256': sha(config), 'kernel_build_id': build_id,
              'upstream_build_metadata': build_metadata, 'publisher_verified': False,
              'source_correspondence_verified': False, 'runtime_admitted': False}
    (destination / 'inspection.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', required=True, type=Path)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(inspect(args.package, args.sha256, args.version, args.output)))


if __name__ == '__main__':
    main()
