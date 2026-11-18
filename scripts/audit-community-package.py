#!/usr/bin/env python3
"""Record package evidence without treating file presence as license approval."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile
import zipfile

HOST = ('linaro-apache-2.0.txt', 'linaro-attribution.txt', 'virglrenderer-copying.txt',
        'virglrenderer-attribution.txt', 'rutabaga-license.txt', 'openvmm-license.txt',
        'libepoxy-copying.txt', 'host-rust-dependencies.tar.gz')
GUEST = ('LICENSE-MAP.md', 'OUTPUT-LICENSES.md', 'WSX-MIT.txt', 'rust-dependencies/DEPENDENCIES.txt')
HEX = re.compile(r'[0-9a-f]{64}\Z')


def digest(stream):
    h = hashlib.sha256()
    while chunk := stream.read(1024 * 1024):
        h.update(chunk)
    return h.hexdigest()


def audit(package, output, kernel_evidence=None):
    if output.exists():
        raise ValueError('Audit output already exists')
    with package.open('rb') as f:
        package_sha = digest(f)
    with zipfile.ZipFile(package) as z:
        entries = [x for x in z.infolist() if not x.is_dir()]
        files = {x.filename: x for x in entries}
        if len(files) != len(entries) or len({x.casefold() for x in files}) != len(files):
            raise ValueError('Duplicate package paths')
        def read(name, limit=1024 * 1024):
            item = files.get(name)
            if item is None or item.file_size < 1 or item.file_size > limit:
                raise ValueError('Missing or invalid package evidence: ' + name)
            return z.read(item)
        for name in HOST:
            item = files.get('licenses/native-host/' + name)
            if item is None or item.file_size < 1:
                raise ValueError('Missing Native Host notice: ' + name)
        for name in GUEST:
            read('licenses/guest-adapters/' + name)
        host_archive = read('licenses/native-host/host-rust-dependencies.tar.gz', 256 * 1024 * 1024)
        with tarfile.open(fileobj=io.BytesIO(host_archive), mode='r:gz') as tar:
            members = tar.getmembers()
            names = [item.name for item in members]
            if len(names) != len(set(names)) or any(not item.isfile() or item.size < 1 for item in members):
                raise ValueError('Invalid Host dependency notice archive')
            manifest_file = tar.extractfile('DEPENDENCIES.json')
            if manifest_file is None:
                raise ValueError('Missing Host dependency inventory')
            manifest = json.load(manifest_file)
            packages = manifest.get('packages')
            if not isinstance(packages, list) or not packages:
                raise ValueError('Empty Host dependency inventory')
            for item in packages:
                if not isinstance(item, dict) or not item.get('name') or not item.get('version') or not item.get('license') or not isinstance(item.get('files'), list) or not item['files']:
                    raise ValueError('Incomplete Host dependency record')
                if any(not isinstance(name, str) or name not in names or name == 'DEPENDENCIES.json' for name in item['files']):
                    raise ValueError('Missing Host dependency notice')
        guest_lines = read('licenses/guest-adapters/rust-dependencies/DEPENDENCIES.txt').decode('utf-8-sig').splitlines()
        for line in guest_lines:
            match_guest = re.fullmatch(r'([\w-]+) ([\w.+-]+):\s*(\S.*)', line)
            if not match_guest:
                raise ValueError('Invalid Guest dependency inventory')
            prefix = 'licenses/guest-adapters/rust-dependencies/' + match_guest[1] + '-' + match_guest[2] + '/'
            if not any(name.startswith(prefix) and files[name].file_size > 0 and re.match(r'(?i)^(license|copying|unlicense)', name[len(prefix):]) for name in files):
                raise ValueError('Missing Guest dependency notice: ' + match_guest[1])
        if not guest_lines:
            raise ValueError('Empty Guest dependency inventory')
        report = read('gui-third-party/REVIEW-REQUIRED.txt').decode('utf-8-sig')
        match = re.search(r'^Packages without local notice text:\s*(\d+)\s*$', report, re.M)
        if not match:
            raise ValueError('Invalid GUI notice inventory')
        index_raw = read('kernel-modules/index.json')
        index = json.loads(index_raw)
        if index.get('schema_version') != 1 or not isinstance(index.get('modules'), list) or not index['modules']:
            raise ValueError('Invalid kernel module index')
        modules = []
        names = set()
        for entry in index['modules']:
            name = entry.get('archive')
            if not isinstance(name, str) or not re.fullmatch(r'modules_[A-Za-z0-9.-]+\.tar\.gz', name) or name in names:
                raise ValueError('Invalid kernel archive name')
            names.add(name)
            item = files.get('kernel-modules/' + name)
            if item is None or item.file_size != entry.get('bytes') or not isinstance(entry.get('sha256'), str) or not HEX.fullmatch(entry['sha256']):
                raise ValueError('Kernel archive metadata mismatch: ' + name)
            with z.open(item) as f:
                actual = digest(f)
            if actual != entry['sha256']:
                raise ValueError('Kernel archive digest mismatch: ' + name)
            modules.append({'kernel_version': entry.get('kernel_version'), 'archive': name, 'sha256': actual, 'bytes': item.file_size})
    evidence_sha = None
    if kernel_evidence is not None:
        with kernel_evidence.open('rb') as f:
            evidence_sha = digest(f)
        with zipfile.ZipFile(kernel_evidence) as evidence:
            evidence_manifest = json.loads(evidence.read('evidence-manifest.json'))
        if evidence_manifest.get('kernel_index_sha256') != hashlib.sha256(index_raw).hexdigest():
            raise ValueError('Kernel source evidence does not match package index')
    result = {'schema_version': 1, 'package_sha256': package_sha,
              'kernel_index_sha256': hashlib.sha256(index_raw).hexdigest(), 'kernel_modules': modules,
              'kernel_source_evidence_sha256': evidence_sha,
              'gui_missing_notice_count': int(match.group(1)),
              'host_dependency_count': len(packages), 'guest_dependency_count': len(guest_lines),
              'scope': 'Component owners are responsible for license and source completeness; this report checks package evidence and does not establish desktop acceptance.'}
    output.write_text(json.dumps(result, indent=2, sort_keys=True) + '\n', encoding='utf-8')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('package', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--kernel-evidence', type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.package, args.output, args.kernel_evidence), sort_keys=True))


if __name__ == '__main__':
    main()
