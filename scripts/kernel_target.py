"""Bind diagnostic target source evidence to a specific inspected kernel image."""
import hashlib
import json
from pathlib import Path
import re
import subprocess

TAG = re.compile(r'linux-msft-wsl-([0-9]+(?:\.[0-9]+){3})\Z')
COMMIT = re.compile(r'[0-9a-f]{40}\Z')
SHA = re.compile(r'[0-9a-f]{64}\Z')


def digest(path):
    if path.is_symlink() or not path.is_file():
        raise ValueError('Unsafe target input: ' + str(path))
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def identity(kernel, image, inspection_file):
    source_file = kernel / 'wsx-source-evidence.json'
    digest(source_file)
    digest(inspection_file)
    source = json.loads(source_file.read_text())
    inspection = json.loads(inspection_file.read_text())
    tag = source.get('tag', '')
    match = TAG.fullmatch(tag) if isinstance(tag, str) else None
    commit = source.get('source_commit', '')
    config = source.get('config_sha256', '')
    image_hash = inspection.get('kernel_image_sha256', '')
    if (source.get('schema_version') != 1 or source.get('source_tag_verified') is not True
            or not match or not isinstance(commit, str) or not COMMIT.fullmatch(commit)
            or not isinstance(config, str) or not SHA.fullmatch(config)
            or inspection.get('schema_version') != 1 or not isinstance(image_hash, str)
            or not SHA.fullmatch(image_hash) or image_hash != digest(image)
            or config != inspection.get('kernel_config_sha256')
            or not str(inspection.get('package_version', '')).startswith(match.group(1) + '-')):
        raise ValueError('Target source, configuration and inspected kernel do not match')
    current = subprocess.check_output(['git', '-C', str(kernel), 'rev-parse', '--verify', 'HEAD^{commit}'],
                                      text=True, timeout=60).strip()
    changed = subprocess.check_output(['git', '--no-optional-locks', '-C', str(kernel),
                                       'status', '--porcelain', '--untracked-files=no'],
                                      text=True, timeout=120)
    if current != commit or changed:
        raise ValueError('Target source tree differs from the verified commit')
    return {'tag': tag, 'source_commit': commit, 'config_sha256': config,
            'kernel_image_sha256': image_hash,
            'release': match.group(1) + '-microsoft-standard-WSL2',
            'source_correspondence_verified': False}
