#!/usr/bin/env python3
"""Merge diagnostic snapshots locally without authorizing publication."""
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import re
import tempfile

spec = importlib.util.spec_from_file_location('kernel_snapshot_package', Path(__file__).with_name('kernel-package.py'))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
SHA = re.compile(r'[0-9a-f]{64}\Z')


def extract_current(root, destination):
    root = root.resolve(strict=True)
    store = root / 'staging-snapshots'
    pointer = store / 'current.json'
    if not store.exists():
        return None
    if store.is_symlink() or pointer.is_symlink() or not pointer.is_file():
        raise ValueError('Staging snapshot pointer is missing or unsafe')
    value = json.loads(pointer.read_text())
    if (not isinstance(value, dict) or set(value) != {'schema_version', 'generation', 'sha256'}
            or value['schema_version'] != 1 or not isinstance(value['generation'], str)
            or not SHA.fullmatch(value['generation']) or not isinstance(value['sha256'], str)
            or not SHA.fullmatch(value['sha256'])):
        raise ValueError('Invalid staging snapshot pointer')
    archive = store / (value['sha256'] + '.tar.gz')
    if archive.is_symlink() or not archive.is_file() or package.digest(archive) != value['sha256']:
        raise ValueError('Previous staging snapshot is missing or changed')
    index = package.extract(archive, destination)
    if index['generation'] != value['generation']:
        raise ValueError('Previous staging snapshot generation differs from pointer')
    return index


def merge_candidate(root, candidate, initial_only=False):
    root = root.resolve(strict=True)
    candidate_index = package.validate(candidate)
    store = root / 'staging-snapshots'
    if store.is_symlink():
        raise ValueError('Unsafe staging snapshot directory')
    store.mkdir(exist_ok=True)
    lock_path = store / '.lock'
    if lock_path.is_symlink():
        raise ValueError('Unsafe staging snapshot lock')
    with lock_path.open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        pointer = store / 'current.json'
        if pointer.is_symlink():
            raise ValueError('Unsafe staging snapshot pointer')
        if initial_only and pointer.exists():
            raise ValueError('Staging snapshot already initialized')
        if not pointer.exists() and any(path.name != '.lock' for path in store.iterdir()):
            raise ValueError('Staging snapshot has incomplete prior state; retain it for inspection')
        old = None
        if pointer.exists():
            value = json.loads(pointer.read_text())
            if (not isinstance(value, dict) or set(value) != {'schema_version', 'generation', 'sha256'}
                    or value['schema_version'] != 1 or not SHA.fullmatch(value['generation'])
                    or not SHA.fullmatch(value['sha256'])):
                raise ValueError('Invalid staging snapshot pointer')
            old = store / (value['sha256'] + '.tar.gz')
            if old.is_symlink() or not old.is_file() or package.digest(old) != value['sha256']:
                raise ValueError('Previous staging snapshot is missing or changed')
        with tempfile.TemporaryDirectory(prefix='.merge-', dir=store) as temporary:
            work = Path(temporary)
            merged = candidate
            if old is not None:
                prior = work / 'prior'
                prior_index = package.extract(old, prior)
                if prior_index['generation'] != value['generation']:
                    raise ValueError('Previous staging snapshot generation differs from pointer')
                if prior_index['generation'] == candidate_index['generation']:
                    merged = work / 'merged'
                    package.merge(prior, candidate, merged)
            archive = work / 'snapshot.tar.gz'
            digest = package.archive(merged, archive)
            with archive.open('rb') as stream:
                os.fsync(stream.fileno())
            destination = store / (digest + '.tar.gz')
            if destination.is_symlink():
                raise ValueError('Unsafe content-addressed staging snapshot')
            try:
                os.link(archive, destination)
            except FileExistsError:
                if not destination.is_file() or package.digest(destination) != digest:
                    raise ValueError('Existing staging snapshot differs from its identity')
            record = {'schema_version': 1, 'generation': candidate_index['generation'], 'sha256': digest}
            new_pointer = work / 'current.json'
            with new_pointer.open('x', encoding='utf-8') as stream:
                json.dump(record, stream, sort_keys=True)
                stream.write('\n')
                stream.flush()
                os.fsync(stream.fileno())
            directory = os.open(store, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(directory)
                os.replace(new_pointer, pointer)
                os.fsync(directory)
            finally:
                os.close(directory)
    return {**record, 'runtime_admitted': False, 'release_ready': False}
