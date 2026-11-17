#!/usr/bin/env python3
"""Prepare a versioned Microsoft WSL source tree from a reusable Git cache."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

UPSTREAM = 'https://github.com/microsoft/WSL2-Linux-Kernel.git'
TAG = re.compile(r'linux-msft-wsl-[0-9]+(?:\.[0-9]+){2,3}(?:-rc[1-9][0-9]*)?\Z')
COMMIT = re.compile(r'[0-9a-f]{40}\Z')
SHA = re.compile(r'[0-9a-f]{64}\Z')


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def resolve_tag(lines, tag):
    direct, peeled = None, None
    for line in lines.splitlines():
        commit, ref = line.split('\t', 1)
        if not COMMIT.fullmatch(commit):
            raise ValueError('Invalid upstream tag object')
        if ref == 'refs/tags/' + tag:
            if direct and direct != commit:
                raise ValueError('Conflicting upstream tag')
            direct = commit
        elif ref == 'refs/tags/' + tag + '^{}':
            if peeled and peeled != commit:
                raise ValueError('Conflicting peeled upstream tag')
            peeled = commit
        else:
            raise ValueError('Unexpected upstream reference')
    if not direct:
        raise ValueError('Upstream kernel tag not found')
    return peeled or direct


def prepare(tag, commit, config, config_hash, cache, output, build_revision=None):
    if not TAG.fullmatch(tag) or not COMMIT.fullmatch(commit) or not SHA.fullmatch(config_hash):
        raise ValueError('Invalid target identity')
    if build_revision is not None and not COMMIT.fullmatch(build_revision):
        raise ValueError('Invalid upstream build revision')
    if config.is_symlink() or not config.is_file() or config.stat().st_size > 2 * 1024 * 1024 or digest(config) != config_hash:
        raise ValueError('Target configuration digest mismatch')
    if output.exists() or output.is_symlink():
        raise ValueError('Target source output already exists')
    refs = subprocess.check_output(['git', 'ls-remote', '--tags', UPSTREAM,
                                    'refs/tags/' + tag, 'refs/tags/' + tag + '^{}'], text=True, timeout=120)
    if resolve_tag(refs, tag) != commit:
        raise ValueError('Microsoft source tag does not match target commit')
    if cache.is_symlink():
        raise ValueError('Unsafe Git cache')
    if not cache.exists():
        cache.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(['git', 'init', '--bare', str(cache)], check=True)
    if not cache.is_dir():
        raise ValueError('Invalid Git cache')
    git = ['git', '--git-dir', str(cache.resolve())]
    # FETCH_HEAD alone is not cloned: retain an immutable, content-addressed
    # reference so a shared checkout can actually reach the fetched object.
    cache_ref = 'refs/heads/wsx-' + commit
    subprocess.run(git + ['fetch', '--no-tags', '--depth=1', UPSTREAM,
                          commit + ':' + cache_ref], check=True, timeout=1800)
    actual = subprocess.check_output(git + ['rev-parse', '--verify', commit + '^{commit}'], text=True).strip()
    if actual != commit:
        raise ValueError('Fetched Git commit differs from target')
    tree = subprocess.check_output(git + ['rev-parse', '--verify', commit + '^{tree}'], text=True).strip()
    build_tree = None
    if build_revision is not None:
        subprocess.run(git + ['fetch', '--no-tags', '--depth=1', UPSTREAM,
                              build_revision + ':refs/heads/wsx-build-' + build_revision], check=True, timeout=1800)
        build_tree = subprocess.check_output(git + ['rev-parse', '--verify', build_revision + '^{tree}'], text=True).strip()
        if tree != build_tree:
            raise ValueError('Kernel source tree differs from upstream package build revision')
    output.parent.mkdir(parents=True, exist_ok=True)
    # Keep a local shared clone so subsequent stages can verify the source tree
    # against the immutable commit before using it. Objects remain in the cache.
    subprocess.run(['git', '-c', 'core.hooksPath=/dev/null', 'clone', '--shared', '--no-checkout',
                    '--', str(cache.resolve()), str(output)], check=True)
    subprocess.run(['git', '-C', str(output), '-c', 'core.hooksPath=/dev/null',
                    'checkout', '--detach', commit], check=True)
    checked = subprocess.check_output(['git', '-C', str(output), 'rev-parse', 'HEAD'], text=True).strip()
    if checked != commit:
        raise ValueError('Checked-out target source differs from verified commit')
    target_config = output / '.config'
    if target_config.exists() or target_config.is_symlink():
        raise ValueError('Source tree already contains a configuration; retained for inspection')
    with target_config.open('xb') as target:
        target.write(config.read_bytes())
    result = {'schema_version': 1, 'tag': tag, 'source_commit': commit,
              'config_sha256': config_hash, 'source_tag_verified': True,
              'binary_source_correspondence_verified': False, 'release_ready': False}
    if build_revision is not None:
        result.update({'source_tree': tree, 'package_build_revision': build_revision,
                       'package_build_tree': build_tree, 'source_tree_matches_package_metadata': True})
    with (output / 'wsx-source-evidence.json').open('x', encoding='utf-8') as evidence:
        json.dump(result, evidence, indent=2)
        evidence.write('\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--config', required=True, type=Path)
    parser.add_argument('--config-sha256', required=True)
    parser.add_argument('--cache', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--build-revision')
    args = parser.parse_args()
    print(json.dumps(prepare(args.tag, args.commit, args.config, args.config_sha256, args.cache, args.output, args.build_revision)))


if __name__ == '__main__':
    main()
