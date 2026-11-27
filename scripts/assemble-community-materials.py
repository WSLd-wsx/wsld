#!/usr/bin/env python3
"""Bind and collect delivered component notices and available kernel sources."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import zipfile


def hash_stream(stream):
    h = hashlib.sha256()
    while data := stream.read(1024 * 1024):
        h.update(data)
    return h.hexdigest()


def archive_hash(archive, name):
    with archive.open(name) as stream:
        return hash_stream(stream)


def file_hash(path):
    with path.open('rb') as stream:
        return hash_stream(stream)


def assemble(package, runtime, kernel, source, output, service_image_directory=None):
    if output.exists():
        raise ValueError('Materials output already exists')
    source = source.resolve(strict=True)
    with zipfile.ZipFile(package) as product, zipfile.ZipFile(runtime) as component, zipfile.ZipFile(kernel) as evidence:
        product_names = [x.filename for x in product.infolist() if not x.is_dir()]
        component_names = [x.filename for x in component.infolist() if not x.is_dir()]
        for names in (product_names, component_names):
            if len(names) != len(set(x.casefold() for x in names)):
                raise ValueError('Duplicate archive path')
        product_set = set(product_names)
        component_set = set(component_names)
        checks = {}
        for mode in ('zero-copy', 'staging-copy'):
            for name in ('component-manifest.json', 'wsx-native-host.exe', 'wsx-input-host.exe', 'libvirglrenderer-1.dll', 'libepoxy-0.dll'):
                relative = f'graphics/product/{mode}/{name}'
                checks[relative] = 'runtime/' + relative
        for mode in ('hyprland', 'plasma'):
            for name in ('manifest.json', 'wsx-guest'):
                relative = f'guest-adapters/{mode}/{name}'
                checks[relative] = 'runtime/' + relative
        for name in component_names:
            if name.startswith(('runtime/licenses/native-host/', 'runtime/licenses/guest-adapters/')):
                checks[name.removeprefix('runtime/')] = name
        for target, origin in checks.items():
            if target not in product_set or origin not in component_set or archive_hash(product, target) != archive_hash(component, origin):
                raise ValueError('Runtime component or material differs: ' + target)
        for prefix in ('licenses/native-host/', 'licenses/guest-adapters/'):
            actual = {name for name in product_names if name.startswith(prefix)}
            expected = {name for name in checks if name.startswith(prefix)}
            if not expected or actual != expected:
                raise ValueError('Runtime material set differs: ' + prefix)
        core_files = ('LICENSE', 'NOTICE', 'LICENSING.md', 'THIRD-PARTY-NOTICES.md', 'integration/LICENSE',
                      'integration/graphics-opaque-probe/APACHE-2.0.txt', 'integration/graphics-opaque-probe/README.md',
                      'core/data/UNICODE-LICENSE.txt', 'core/LICENSE', 'core/CHANGE-LICENSE-APACHE-2.0.txt')
        for name in core_files:
            path = source / name
            if not path.is_file() or path.is_symlink() or name not in product_set or file_hash(path) != archive_hash(product, name):
                raise ValueError('WSLD notice differs from pinned source: ' + name)
        source_licenses = source / 'licenses'
        for path in source_licenses.rglob('*'):
            if path.is_symlink():
                raise ValueError('Unsafe WSLD license source')
            if path.is_file():
                name = path.relative_to(source).as_posix()
                if name not in product_set or file_hash(path) != archive_hash(product, name):
                    raise ValueError('WSLD license differs from pinned source: ' + name)
        kernel_manifest = json.loads(evidence.read('evidence-manifest.json'))
        index_sha = archive_hash(product, 'kernel-modules/index.json')
        if kernel_manifest.get('kernel_index_sha256') != index_sha:
            raise ValueError('Kernel evidence differs from product index')
        material_names = [name for name in product_names if name in core_files or name.startswith(('licenses/', 'gui-third-party/'))]
        if 'kernel_source_downloads' in kernel_manifest:
            link_name = 'KERNEL-SOURCE-LINKS.md'
            if link_name not in product_set or archive_hash(product, link_name) != archive_hash(evidence, link_name):
                raise ValueError('Kernel source links differ between product and evidence')
            material_names.append(link_name)
        if not any(name.startswith('gui-third-party/') for name in material_names):
            raise ValueError('Missing GUI notices')
        manifest = {'schema_version': 1, 'package_sha256': file_hash(package), 'runtime_sha256': file_hash(runtime),
                    'kernel_evidence_sha256': file_hash(kernel), 'kernel_index_sha256': index_sha,
                    'materials': {name: archive_hash(product, name) for name in sorted(material_names)},
                    'runtime_files': {name: archive_hash(product, name) for name in sorted(checks)},
                    'kernel_modules': kernel_manifest['modules'],
                    'scope': 'Host and Guest product code is closed source. Collected notices and available kernel source evidence do not establish third-party source completeness or license approval.'}
        if service_image_directory:
            service_files = ('wsx-service-rootfs.tar', 'wsx-service-packages.tsv', 'wsx-service-sources.tar.gz')
            sums = {}
            for line in (service_image_directory / 'SHA256SUMS').read_text().splitlines():
                digest, name = line.split(maxsplit=1)
                if name not in service_files or name in sums or file_hash(service_image_directory / name) != digest:
                    raise ValueError('Service image input does not match its checksum manifest: ' + name)
                sums[name] = digest
            if set(sums) != set(service_files):
                raise ValueError('Incomplete service image checksum manifest')
            for name in service_files[:2]:
                if archive_hash(product, 'service/' + name) != sums[name]:
                    raise ValueError('Packaged service input differs: ' + name)
            manifest['service_image'] = sums
            material_names.append('service/wsx-service-packages.tsv')
        with zipfile.ZipFile(output, 'x', compression=zipfile.ZIP_DEFLATED, allowZip64=True) as result:
            result.writestr('manifest.json', json.dumps(manifest, sort_keys=True, indent=2) + '\n')
            for name in sorted(material_names):
                notice = zipfile.ZipInfo('notices/' + name, product.getinfo(name).date_time)
                notice.compress_type = zipfile.ZIP_DEFLATED
                with product.open(name) as src, result.open(notice, 'w', force_zip64=True) as dst:
                    while chunk := src.read(1024 * 1024): dst.write(chunk)
            result.write(kernel, 'kernel-source-evidence.zip')
            if service_image_directory:
                result.write(service_image_directory / 'wsx-service-sources.tar.gz',
                             'wsx-service-sources.tar.gz', compress_type=zipfile.ZIP_STORED)
                result.write(source / 'scripts/build-service-image.sh', 'build-service-image.sh')
    return manifest


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--kernel-evidence', type=Path, required=True)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--service-image-directory', type=Path)
    args = parser.parse_args()
    print(json.dumps(assemble(args.package, args.runtime, args.kernel_evidence, args.source, args.output, args.service_image_directory), sort_keys=True))
