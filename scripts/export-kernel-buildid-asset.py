#!/usr/bin/env python3
"""Export one checked diagnostic module archive addressed by kernel Build ID."""
import argparse
import gzip
import importlib.util
import json
from pathlib import Path
import tarfile

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('buildid_asset_snapshot', HERE / 'kernel-package.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def export(snapshot, kernel_exports, output):
    if output.exists() or output.is_symlink():
        raise ValueError('Build ID asset output already exists')
    if kernel_exports.is_symlink() or not kernel_exports.is_file() or kernel_exports.stat().st_size > 64 * 1024 * 1024:
        raise ValueError('Unsafe kernel export evidence')
    index = package.validate(snapshot)
    evidence = json.loads(kernel_exports.read_text())
    identity = evidence.get('kernel_build_id')
    image_hash = evidence.get('kernel_image_sha256')
    if not isinstance(identity, str) or not package.BUILD_ID.fullmatch(identity) or not isinstance(image_hash, str) or not package.SHA.fullmatch(image_hash):
        raise ValueError('Invalid kernel Build ID evidence')
    matches = [entry for entry in index['targets'].values()
               if entry['kernel_build_id'] == identity and entry['kernel_image_sha256'] == image_hash]
    if not matches or len({entry['group'] for entry in matches}) != 1:
        raise ValueError('No unique checked snapshot for the target kernel image')
    target = matches[0]
    audit = json.loads((snapshot / 'audits' / (target['audit_sha256'] + '.json')).read_text())
    if (audit.get('crc_match') is not True or audit.get('runtime_admitted') is not False
            or audit.get('kernel_build_id') != identity or set(audit.get('modules', {})) != set(package.MODULES)):
        raise ValueError('Kernel module CRC evidence is incomplete')
    for name in package.MODULES:
        item = audit['modules'][name]
        if (item.get('sha256') != index['groups'][target['group']][name]['sha256']
                or item.get('missing') != [] or item.get('mismatches') != []
                or item.get('has_module_layout') is not True or not item.get('import_count')):
            raise ValueError('Invalid module CRC evidence: ' + name)
    output.mkdir(parents=True)
    archive = output / ('modules-' + identity + '.tar.gz')
    with archive.open('xb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w', format=tarfile.USTAR_FORMAT) as tar:
            for name in sorted(package.MODULES):
                path = snapshot / 'groups' / target['group'] / name
                info = tar.gettarinfo(str(path), arcname=name)
                info.uid = info.gid = 0
                info.uname = info.gname = ''
                info.mtime = int(info.mtime)
                info.mode = 0o644
                with path.open('rb') as source:
                    tar.addfile(info, source)
    result = {'schema_version': 1, 'kernel_build_id': identity, 'kernel_image_sha256': image_hash,
              'archive': archive.name, 'archive_sha256': package.digest(archive),
              'archive_bytes': archive.stat().st_size, 'build_generation': index['generation'],
              'runtime_admitted': False, 'release_ready': False}
    (output / ('modules-' + identity + '.json')).write_text(json.dumps(result, sort_keys=True, separators=(',', ':')) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--snapshot', type=Path, required=True)
    parser.add_argument('--kernel-exports', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(export(args.snapshot, args.kernel_exports, args.output), sort_keys=True))


if __name__ == '__main__':
    main()
