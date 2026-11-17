#!/usr/bin/env python3
"""Inventory official WSL kernel tags and fail closed on missing coverage."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from urllib.request import Request, urlopen

UPSTREAM = 'https://github.com/microsoft/WSL2-Linux-Kernel.git'
TAG = re.compile(r'linux-msft-wsl-([0-9]+(?:\.[0-9]+){2,3})(?:-rc([1-9][0-9]*))?\Z')
HEX = re.compile(r'[0-9a-f]{64}\Z')
MODULES = ('drm_kms_helper.ko', 'virtio_dma_buf.ko', 'virtio-gpu.ko',
           'virtio_msg.ko', 'virtio_msg_vsock.ko', 'wsx_virtio_msg_pci.ko')


def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def inventory(raw):
    direct, peeled = {}, {}
    for line in raw.splitlines():
        commit, ref = line.split('\t', 1)
        if not re.fullmatch(r'[0-9a-f]{40}', commit) or not ref.startswith('refs/tags/'):
            raise ValueError('Invalid upstream tag reference')
        name = ref.removeprefix('refs/tags/')
        target = peeled if name.endswith('^{}') else direct
        name = name.removesuffix('^{}')
        if TAG.fullmatch(name):
            if name in target and target[name] != commit:
                raise ValueError('Conflicting upstream tag references')
            target[name] = commit
    if not direct or any(name not in direct for name in peeled):
        raise ValueError('Empty or incomplete upstream tag inventory')
    def version_key(name):
        match = TAG.fullmatch(name)
        version = tuple(int(part) for part in match.group(1).split('.'))
        return (version + (0,) * (4 - len(version)), match.group(2) is None,
                int(match.group(2) or 0))
    names = sorted(direct, key=version_key, reverse=True)
    return {name: peeled.get(name, direct[name]) for name in names}


def published_inventory(tags, releases):
    selected = set()
    for release in releases:
        if not isinstance(release, dict) or not isinstance(release.get('draft'), bool) or not isinstance(release.get('prerelease'), bool):
            raise ValueError('Invalid upstream release metadata')
        name = release.get('tag_name')
        if release['draft'] or release['prerelease'] or not isinstance(name, str) or not TAG.fullmatch(name):
            continue
        if TAG.fullmatch(name).group(2):
            raise ValueError('RC-tagged kernel release cannot be treated as stable: ' + name)
        if name not in tags:
            raise ValueError('Published kernel release has no corresponding source tag: ' + name)
        selected.add(name)
    if not selected:
        raise ValueError('No published stable WSL kernel releases found')
    return {name: commit for name, commit in tags.items() if name in selected}


def latest_kernel_preview(tags, releases):
    # RC source tags can precede a GitHub prerelease record. Discover them
    # without claiming that a matching published binary or build is available.
    selected = {name for name in tags if TAG.fullmatch(name) and TAG.fullmatch(name).group(2)}
    for release in releases:
        if not isinstance(release, dict) or not isinstance(release.get('draft'), bool) or not isinstance(release.get('prerelease'), bool):
            raise ValueError('Invalid upstream release metadata')
        name = release.get('tag_name')
        if release['draft'] or not release['prerelease'] or not isinstance(name, str) or not TAG.fullmatch(name):
            continue
        if name not in tags:
            raise ValueError('Preview kernel release has no corresponding source tag: ' + name)
        selected.add(name)
    return next(((name, commit) for name, commit in tags.items() if name in selected), None)


def scan_inventory(tags, releases):
    channels = {}
    for release in releases:
        if not isinstance(release, dict) or not isinstance(release.get('draft'), bool) or not isinstance(release.get('prerelease'), bool):
            raise ValueError('Invalid upstream release metadata')
        name = release.get('tag_name')
        if release['draft'] or not isinstance(name, str) or not TAG.fullmatch(name):
            continue
        channel = release['prerelease']
        if name in channels and channels[name] != channel:
            raise ValueError('Conflicting upstream release channels: ' + name)
        channels[name] = channel
    stable = published_inventory(tags, releases)
    preview = latest_kernel_preview(tags, releases)
    selected = set(stable)
    if preview:
        selected.add(preview[0])
    return {name: commit for name, commit in tags.items() if name in selected}, preview


def fetch_releases():
    releases = []
    for page in range(1, 101):
        url = f'https://api.github.com/repos/microsoft/WSL2-Linux-Kernel/releases?per_page=100&page={page}'
        headers = {'Accept': 'application/vnd.github+json', 'User-Agent': 'WSX-kernel-coverage'}
        if os.environ.get('GITHUB_TOKEN'):
            headers['Authorization'] = 'Bearer ' + os.environ['GITHUB_TOKEN']
        with urlopen(Request(url, headers=headers), timeout=60) as response:
            body = response.read(8 * 1024 * 1024 + 1)
        if len(body) > 8 * 1024 * 1024:
            raise ValueError('Upstream release response exceeds size limit')
        batch = json.loads(body)
        if not isinstance(batch, list):
            raise ValueError('Invalid upstream release response')
        releases.extend(batch)
        if len(batch) < 100:
            return releases
    raise ValueError('Upstream release pagination limit exceeded')


def relative_file(root, value):
    path = Path(value)
    if path.is_absolute() or '..' in path.parts or not path.parts:
        raise ValueError('Unsafe evidence path')
    result = (root / path).resolve(strict=True)
    if not result.is_relative_to(root.resolve()) or not result.is_file():
        raise ValueError('Evidence file is outside the coverage directory')
    return result


def verified_kernel_image_hash(root, record):
    expected = record['kernel_image_sha256']
    if not isinstance(expected, str) or not HEX.fullmatch(expected):
        raise ValueError('Invalid kernel image digest')
    if 'kernel_image' in record:
        actual = sha(relative_file(root, record['kernel_image']))
    else:
        archive = relative_file(root, record['kernel_package'])
        package_hash = record['kernel_package_sha256']
        if not isinstance(package_hash, str) or not HEX.fullmatch(package_hash) or sha(archive) != package_hash:
            raise ValueError('Kernel package digest mismatch')
        with zipfile.ZipFile(archive) as package:
            matches = [item for item in package.infolist() if item.filename == 'build/native/bin/x64/kernel']
            if len(matches) != 1 or matches[0].is_dir() or not 0 < matches[0].file_size <= 64 * 1024 * 1024:
                raise ValueError('Missing, ambiguous or oversized kernel package image')
            with package.open(matches[0]) as stream:
                actual = hashlib.file_digest(stream, 'sha256').hexdigest()
    if actual != expected:
        raise ValueError('kernel image digest mismatch')
    return actual


def covered(root, tag, commit, generation=None):
    record_path = root / 'targets' / (tag + '.json')
    if not record_path.is_file():
        return False, 'missing target record'
    try:
        record = json.loads(record_path.read_text(encoding='utf-8'))
        if record['schema_version'] != 1 or record['tag'] != tag or record['source_commit'] != commit:
            raise ValueError('target identity differs from upstream')
        if generation is not None and record.get('build_generation') != generation:
            raise ValueError('build inputs changed; target must be rebuilt')
        audit = json.loads(relative_file(root, record['audit']).read_text(encoding='utf-8'))
        verified_kernel_image_hash(root, record)
        if (audit['symbol_crc_compatibility_admitted'] is not True
                or audit['kernel_image_sha256'] != record['kernel_image_sha256']
                or audit['kernel_build_id'] != record['kernel_build_id']):
            raise ValueError('kernel audit does not match the target')
        receipt_file = relative_file(root, record['receipt'])
        if not HEX.fullmatch(record['receipt_sha256']) or sha(receipt_file) != record['receipt_sha256']:
            raise ValueError('module receipt digest mismatch')
        receipt = json.loads(receipt_file.read_text(encoding='utf-8'))
        build_file = relative_file(root, record['build'])
        if not HEX.fullmatch(record['build_sha256']) or sha(build_file) != record['build_sha256']:
            raise ValueError('build evidence digest mismatch')
        build = json.loads(build_file.read_text(encoding='utf-8'))
        build_commit = record.get('build_source_commit', commit)
        if not isinstance(build_commit, str) or not re.fullmatch(r'[0-9a-f]{40}', build_commit):
            raise ValueError('Invalid module build source identity')
        if (build.get('schema_version') != 1 or build.get('kernel_source_commit') != build_commit
                or build.get('build_generation') != record.get('build_generation')):
            raise ValueError('build evidence target or generation mismatch')
        if build.get('runner_config_unreviewed') is True:
            raise ValueError('runner-generated configuration has not been admitted for release')
        module_dir = relative_file(root, record['module_anchor']).parent
        if set(audit['modules']) != set(MODULES):
            raise ValueError('audit does not cover the complete six-module set')
        for name in MODULES:
            entry = receipt['guest/' + name]
            if build.get('modules', {}).get(name) != entry:
                raise ValueError('build evidence module mismatch: ' + name)
            file = module_dir / name
            if not file.is_file() or file.is_symlink() or file.stat().st_size != entry['bytes']:
                raise ValueError('module size or type mismatch: ' + name)
            actual = sha(file)
            item = audit['modules'][name]
            if (actual != entry['sha256'] or actual != item['sha256'] or item['mismatches'] or item['unobserved']
                    or item.get('has_module_layout') is not True or not item.get('imports')):
                raise ValueError('module digest or CRC mismatch: ' + name)
        return True, record['receipt_sha256']
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError, zipfile.BadZipFile) as error:
        return False, str(error)


class DiagnosticSnapshotCache:
    def __init__(self, root):
        self.root = root
        self.temporary = tempfile.TemporaryDirectory(prefix='wsx-coverage-inspect-')
        self.pointer_bytes = None
        self.index = None
        self.snapshot = None
        self.counter = 0
        spec = importlib.util.spec_from_file_location('coverage_staging', Path(__file__).with_name('kernel-staging-snapshots.py'))
        self.staging = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.staging)

    def read(self):
        pointer = self.root / 'staging-snapshots/current.json'
        if pointer.is_symlink():
            raise ValueError('Unsafe staging snapshot pointer')
        if not pointer.exists():
            if pointer.parent.exists() or pointer.parent.is_symlink():
                raise ValueError('Staging snapshot pointer is missing or unsafe')
            self.pointer_bytes = self.index = self.snapshot = None
            return None, None
        data = pointer.read_bytes()
        if len(data) > 4096:
            raise ValueError('Staging snapshot pointer exceeds size limit')
        if data != self.pointer_bytes:
            self.counter += 1
            snapshot = Path(self.temporary.name) / str(self.counter)
            index = self.staging.extract_current(self.root, snapshot)
            previous = self.snapshot
            self.pointer_bytes, self.snapshot, self.index = data, snapshot, index
            if previous is not None:
                shutil.rmtree(previous)
        else:
            value = json.loads(data)
            archive = pointer.parent / (value['sha256'] + '.tar.gz')
            if archive.is_symlink() or not archive.is_file() or sha(archive) != value['sha256']:
                raise ValueError('Previous staging snapshot is missing or changed')
        return self.snapshot, self.index

    def close(self):
        self.temporary.cleanup()


def diagnostic_built(root, tag, commit, generation, cache=None):
    """Find an integrity-checked build without treating it as release coverage."""
    own_cache = cache is None
    cache = cache or DiagnosticSnapshotCache(root)
    try:
        snapshot, index = cache.read()
        if index is None or index['generation'] != generation or index['schema_version'] != 3:
            return False
        target = index['targets'].get(tag)
        if target is None or target['source_commit'] != commit or target['build_source_commit'] != commit:
            return False
        audit = json.loads((snapshot / 'audits' / (target['audit_sha256'] + '.json')).read_text())
        build = json.loads((snapshot / 'builds' / (target['build_sha256'] + '.json')).read_text())
        evidence = build.get('kernel_symvers_evidence')
        if (not isinstance(evidence, dict) or audit.get('schema_version') != 1
                or audit.get('crc_match') is not True or audit.get('runtime_admitted') is not False
                or audit.get('kernel_build_id') != target['kernel_build_id']
                or evidence.get('kernel_build_id') != target['kernel_build_id']
                or evidence.get('kernel_image_sha256') != target['kernel_image_sha256']
                or set(audit.get('modules', {})) != set(MODULES)):
            return False
        group = index['groups'][target['group']]
        for name in MODULES:
            item = audit['modules'][name]
            if (item.get('sha256') != group[name]['sha256'] or item.get('missing') != []
                    or item.get('mismatches') != [] or item.get('has_module_layout') is not True
                    or not item.get('import_count')):
                return False
        return True
    finally:
        if own_cache:
            cache.close()


def run_builder(builder, tag, commit, root):
    if not TAG.fullmatch(tag) or not re.fullmatch(r'[0-9a-f]{40}', commit):
        raise ValueError('Invalid builder target')
    logs = root / 'builder-logs'
    if logs.is_symlink():
        raise ValueError('Unsafe builder log directory')
    logs.mkdir(parents=True, exist_ok=True)
    pending = logs / (tag + '.pending')
    if pending.is_symlink() or (pending.exists() and not pending.is_file()):
        raise ValueError('Unsafe pending build marker')
    if not pending.exists():
        with pending.open('x', encoding='utf-8') as marker:
            marker.write('Build started; validation has not completed.\n')
    for attempt in range(1, 10001):
        log = logs / f'{tag}-{attempt:04d}.log'
        try:
            with log.open('xb') as output:
                process = subprocess.run([str(builder.resolve()), '--tag', tag, '--commit', commit,
                                          '--coverage', str(root.resolve())],
                                         stdin=subprocess.DEVNULL, stdout=output,
                                         stderr=subprocess.STDOUT, check=False)
            return process.returncode, log.relative_to(root).as_posix()
        except FileExistsError:
            continue
    raise ValueError('Builder log attempt limit exceeded')


def report_builder_failure(root, tag, log):
    failure_log = relative_file(root, log)
    with failure_log.open('rb') as stream:
        stream.seek(0, os.SEEK_END)
        size = stream.tell()
        stream.seek(max(0, size - 65536))
        tail = stream.read(65536).decode('utf-8', errors='replace')
    print(f'Build failed for {tag}; last {min(size, 65536)} log bytes follow:', file=sys.stderr)
    print(tail, file=sys.stderr)


def finish_builder_attempt(pending, exit_code, covered_result, diagnostic_result):
    """Clear the pending marker only after a successful, verified result."""
    if exit_code == 0 and (covered_result or diagnostic_result):
        pending.unlink()


def reusable_diagnostic(root, tag, commit, generation, cache, pending):
    if pending.exists() or pending.is_symlink():
        return False
    return diagnostic_built(root, tag, commit, generation, cache)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--coverage', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--admission', type=Path, required=True, help='Core-reviewed host-updates.json')
    parser.add_argument('--refs-file', type=Path, help='Offline inventory for diagnostics only; never a release gate')
    parser.add_argument('--builder', type=Path, help='Builder executable receiving --tag, --commit and --coverage')
    parser.add_argument('--bootstrap-registry', action='store_true',
                        help='Fetch and verify the existing registry snapshot before scanning')
    args = parser.parse_args()
    if args.report.exists() or args.report.is_symlink():
        raise ValueError('Coverage report already exists; previous evidence must be retained')
    generation_script = Path(__file__).with_name('kernel_build_generation.py')
    generation = json.loads(subprocess.check_output([sys.executable, str(generation_script)], text=True))['generation']
    admission = json.loads(args.admission.read_text(encoding='utf-8'))
    if admission.get('schema_version') != 1 or not isinstance(admission.get('updates'), list):
        raise ValueError('Invalid Core admission policy')
    admitted = {edge['replacement_receipt_sha256'] for edge in admission['updates']
                if edge.get('automatic_selection') is True and HEX.fullmatch(edge.get('replacement_receipt_sha256', ''))}
    admitted.add('442f6032d8645c413b7398e2654c669169e79e9b46e79220134bb881b51d0561')
    args.coverage.mkdir(parents=True, exist_ok=True)
    lock_path = args.coverage / '.coverage.lock'
    if lock_path.is_symlink():
        raise ValueError('Unsafe coverage lock')
    lock = lock_path.open('a')
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError as error:
        raise ValueError('Another coverage scan is active') from error
    bootstrap = None
    if args.bootstrap_registry:
        spec = importlib.util.spec_from_file_location('kernel_bootstrap', Path(__file__).with_name('bootstrap-kernel-snapshot.py'))
        bootstrap_module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(bootstrap_module)
        bootstrap = bootstrap_module.bootstrap_groups(args.coverage, os.environ.get('GITHUB_ACTOR', ''),
                                                      os.environ.get('GITHUB_TOKEN', ''))
    refs = args.refs_file.read_text(encoding='utf-8') if args.refs_file else subprocess.check_output(
        ['git', 'ls-remote', '--tags', UPSTREAM, 'refs/tags/linux-msft-wsl-*'], text=True, timeout=120)
    targets = inventory(refs)
    preview = None
    if args.refs_file is None:
        releases = fetch_releases()
        targets, preview = scan_inventory(targets, releases)
    results = []
    diagnostic_cache = DiagnosticSnapshotCache(args.coverage)
    for tag, commit in targets.items():
        ok, reason = covered(args.coverage, tag, commit, generation)
        pending = args.coverage / 'builder-logs' / (tag + '.pending')
        if pending.exists() or pending.is_symlink():
            ok, reason = False, 'previous builder attempt did not finish validation'
        build_log, build_exit_code = None, None
        diagnostic = not ok and reusable_diagnostic(args.coverage, tag, commit, generation, diagnostic_cache, pending)
        if not ok and args.builder and not diagnostic:
            print(json.dumps({'building_target': tag, 'source_commit': commit}, sort_keys=True), flush=True)
            build_exit_code, build_log = run_builder(args.builder, tag, commit, args.coverage)
            ok, reason = covered(args.coverage, tag, commit, generation)
            diagnostic = not ok and diagnostic_built(args.coverage, tag, commit, generation, diagnostic_cache)
            if build_exit_code != 0:
                ok, reason = False, f'builder exited {build_exit_code}; {reason}'
                report_builder_failure(args.coverage, tag, build_log)
            else:
                finish_builder_attempt(pending, build_exit_code, ok, diagnostic)
        if ok and reason not in admitted:
            ok, reason = False, 'module pair is not admitted by Core'
        target_result = {'tag': tag, 'source_commit': commit, 'covered': ok, 'detail': reason,
                         'diagnostic_built': diagnostic,
                         'channel': ('source_tag_only' if args.refs_file is not None else
                                     'kernel_prerelease' if preview and tag == preview[0] else 'stable'),
                         'builder_exit_code': build_exit_code, 'builder_log': build_log}
        results.append(target_result)
        print(json.dumps({'target_result': target_result}, sort_keys=True), flush=True)
    diagnostic_cache.close()
    final_generation = json.loads(subprocess.check_output([sys.executable, str(generation_script)], text=True))['generation']
    if final_generation != generation:
        raise ValueError('Build inputs changed during kernel coverage scan')
    missing = sum(not item['covered'] for item in results)
    # There is no supported, independently verified source-to-binary builder yet.
    # Passing local JSON checks alone must never authorize release packaging.
    report = {'schema_version': 1, 'upstream': UPSTREAM, 'live_inventory': args.refs_file is None,
              'build_generation': generation,
              'registry_bootstrap': bootstrap,
              'scope': ('Published stable official WSL kernel releases and latest kernel prerelease' if args.refs_file is None else 'Offline versioned source tag snapshot') +
                       '; binary/source provenance and hardware acceptance require separate review',
              'total': len(results), 'missing': missing, 'targets': results,
              'release_gate_passed': False,
              'release_blocker': 'Source-to-binary provenance and the supported module builder are not implemented'}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    with args.report.open('x', encoding='utf-8') as output:
        json.dump(report, output, indent=2)
        output.write('\n')
    print(json.dumps({'total': len(results), 'missing': missing, 'release_gate_passed': report['release_gate_passed']}))
    return 0 if report['release_gate_passed'] else 1


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f'Kernel coverage scan failed: {error}', file=sys.stderr)
        raise SystemExit(2)
