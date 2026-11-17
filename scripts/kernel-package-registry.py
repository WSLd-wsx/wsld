#!/usr/bin/env python3
"""Fetch a WSX kernel snapshot from GHCR without trusting registry content."""
import argparse
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
from urllib.error import HTTPError
from urllib.parse import parse_qsl, urlencode, urljoin, urlsplit, urlunsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener

REGISTRY = 'https://ghcr.io'
REPOSITORY = 'wsld-wsx/wsld-kernel-modules'
MANIFEST_TYPE = 'application/vnd.oci.image.manifest.v1+json'
LAYER_TYPE = 'application/vnd.wsx.kernel-modules.snapshot.v1.tar+gzip'
GROUP_LAYER_TYPE = 'application/vnd.wsx.kernel-modules.group.v1.tar+gzip'
INDEX_LAYER_TYPE = 'application/vnd.wsx.kernel-modules.index.v1+json'
METADATA_LAYER_TYPE = 'application/vnd.wsx.kernel-modules.evidence.v1.tar+gzip'
SHA = re.compile(r'sha256:([0-9a-f]{64})\Z')
MAX_MANIFEST = 1024 * 1024
MAX_ARCHIVE = 1024 * 1024 * 1024

spec = importlib.util.spec_from_file_location('kernel_package', Path(__file__).with_name('kernel-package.py'))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class SafeRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        old, new = urlsplit(req.full_url), urlsplit(newurl)
        if new.scheme != 'https':
            raise ValueError('Registry redirect must use HTTPS')
        redirected = super().redirect_request(req, fp, code, msg, headers, newurl)
        if redirected and (old.scheme, old.netloc) != (new.scheme, new.netloc):
            redirected.remove_header('Authorization')
            redirected.unredirected_hdrs.pop('Authorization', None)
        return redirected


def get_token(opener, actor, credential, push=False):
    query = urlencode({'service': 'ghcr.io', 'scope': f'repository:{REPOSITORY}:pull' + (',push' if push else '')})
    headers = {'Accept': 'application/json', 'User-Agent': 'WSX-kernel-packages'}
    if bool(actor) != bool(credential):
        raise ValueError('Both GitHub actor and token are required for authenticated access')
    if push and not actor:
        raise ValueError('Authenticated registry access is required for publication')
    if actor:
        basic = base64.b64encode(f'{actor}:{credential}'.encode()).decode()
        headers['Authorization'] = 'Basic ' + basic
    with opener.open(Request(REGISTRY + '/token?' + query, headers=headers), timeout=60) as response:
        raw = response.read(MAX_MANIFEST + 1)
    if len(raw) > MAX_MANIFEST:
        raise ValueError('Registry token response exceeds limit')
    value = json.loads(raw)
    if not isinstance(value, dict):
        raise ValueError('Invalid registry token response')
    token = value.get('token')
    if not isinstance(token, str) or not token or len(token) > 16384:
        raise ValueError('Registry did not provide a valid pull token')
    return token


def parse_manifest(raw, header_digest):
    if len(raw) > MAX_MANIFEST or not SHA.fullmatch(header_digest or ''):
        raise ValueError('Registry manifest or digest is invalid')
    if hashlib.sha256(raw).hexdigest() != SHA.fullmatch(header_digest).group(1):
        raise ValueError('Registry manifest digest mismatch')
    value = json.loads(raw)
    if (not isinstance(value, dict) or value.get('schemaVersion') != 2 or value.get('mediaType') != MANIFEST_TYPE
            or value.get('artifactType') != LAYER_TYPE or not isinstance(value.get('layers'), list)
            or len(value['layers']) != 1 or not isinstance(value['layers'][0], dict)):
        raise ValueError('Unsupported kernel package manifest')
    expected_config = {'mediaType': 'application/vnd.oci.empty.v1+json',
                       'digest': 'sha256:' + hashlib.sha256(b'{}').hexdigest(), 'size': 2}
    if value.get('config') != expected_config:
        raise ValueError('Unsupported kernel package config descriptor')
    layer = value['layers'][0]
    if (layer.get('mediaType') != LAYER_TYPE or not SHA.fullmatch(layer.get('digest', ''))
            or not isinstance(layer.get('size'), int) or isinstance(layer['size'], bool)
            or not 0 < layer['size'] <= MAX_ARCHIVE):
        raise ValueError('Invalid kernel package layer descriptor')
    return layer


def download_layer(opener, token, descriptor, output):
    if output.exists() or output.is_symlink():
        raise ValueError('Download output already exists')
    request = Request(f"{REGISTRY}/v2/{REPOSITORY}/blobs/{descriptor['digest']}",
                      headers={'Authorization': 'Bearer ' + token, 'User-Agent': 'WSX-kernel-packages'})
    h, total = hashlib.sha256(), 0
    with opener.open(request, timeout=120) as response, output.open('xb') as stream:
        while True:
            block = response.read(1024 * 1024)
            if not block:
                break
            total += len(block)
            if total > descriptor['size']:
                raise ValueError('Package layer exceeds declared size; partial retained')
            h.update(block)
            stream.write(block)
    if total != descriptor['size'] or 'sha256:' + h.hexdigest() != descriptor['digest']:
        raise ValueError('Package layer size or digest mismatch; partial retained')


def verify_group_archive(archive, group):
    if archive.is_symlink() or not archive.is_file() or not isinstance(group, str) or not re.fullmatch(r'[0-9a-f]{64}', group):
        raise ValueError('Invalid module group archive or identity')
    files = {}
    with tarfile.open(archive, 'r:gz') as tar:
        for item in tar:
            if (item.name not in package.MODULES or item.name in files or not item.isfile()
                    or not 0 < item.size <= package.MAX_FILE):
                raise ValueError('Invalid module group archive member')
            stream = tar.extractfile(item)
            if stream is None:
                raise ValueError('Cannot read module group archive member')
            files[item.name] = {'bytes': item.size, 'sha256': hashlib.file_digest(stream, 'sha256').hexdigest()}
    if set(files) != set(package.MODULES) or package.group_id(files) != group:
        raise ValueError('Module group content identity mismatch')
    return files


def extract_group(archive, destination, group):
    files = verify_group_archive(archive, group)
    if destination.exists() or destination.is_symlink():
        raise ValueError('Module group output already exists')
    destination.mkdir(parents=True)
    seen = set()
    with tarfile.open(archive, 'r:gz') as tar:
        for item in tar:
            if item.name not in files or item.name in seen or not item.isfile() or item.size != files[item.name]['bytes']:
                raise ValueError('Module group changed during extraction')
            seen.add(item.name)
            stream = tar.extractfile(item)
            if stream is None:
                raise ValueError('Cannot read module group during extraction')
            target = destination / item.name
            with target.open('xb') as output:
                h = hashlib.sha256()
                for block in iter(lambda: stream.read(1024 * 1024), b''):
                    h.update(block)
                    output.write(block)
            if h.hexdigest() != files[item.name]['sha256']:
                raise ValueError('Module group changed during extraction')
    if seen != set(files):
        raise ValueError('Module group changed during extraction')
    return {'group': group, 'files': files, 'runtime_admitted': False}


def pull_group(archive, manifest_digest, group, actor='', credential='', opener=None):
    if not SHA.fullmatch(manifest_digest or '') or not re.fullmatch(r'[0-9a-f]{64}', group or ''):
        raise ValueError('Invalid immutable module group identity')
    opener = opener or build_opener(SafeRedirect())
    token = get_token(opener, actor, credential)
    request = Request(f'{REGISTRY}/v2/{REPOSITORY}/manifests/{manifest_digest}', headers={
        'Accept': MANIFEST_TYPE, 'Authorization': 'Bearer ' + token, 'User-Agent': 'WSX-kernel-packages'})
    with opener.open(request, timeout=60) as response:
        raw = response.read(MAX_MANIFEST + 1)
        header = response.headers.get('Docker-Content-Digest')
    if len(raw) > MAX_MANIFEST or header != manifest_digest or 'sha256:' + hashlib.sha256(raw).hexdigest() != manifest_digest:
        raise ValueError('Module group manifest digest mismatch')
    value = json.loads(raw)
    expected_config = {'mediaType': 'application/vnd.oci.empty.v1+json',
                       'digest': 'sha256:' + hashlib.sha256(b'{}').hexdigest(), 'size': 2}
    layers = value.get('layers') if isinstance(value, dict) else None
    if (not isinstance(value, dict) or value.get('schemaVersion') != 2 or value.get('mediaType') != MANIFEST_TYPE
            or value.get('artifactType') != GROUP_LAYER_TYPE or value.get('config') != expected_config
            or not isinstance(value.get('annotations'), dict)
            or value['annotations'].get('io.wsx.kernel-module-group') != group
            or not isinstance(layers, list) or len(layers) != 1 or not isinstance(layers[0], dict)):
        raise ValueError('Unsupported module group manifest')
    layer = layers[0]
    if (layer.get('mediaType') != GROUP_LAYER_TYPE or not isinstance(layer.get('digest'), str)
            or not SHA.fullmatch(layer['digest']) or not isinstance(layer.get('size'), int)
            or isinstance(layer['size'], bool) or not 0 < layer['size'] <= MAX_ARCHIVE):
        raise ValueError('Invalid module group archive descriptor')
    download_layer(opener, token, layer, archive)
    verify_group_archive(archive, group)
    return {'manifest_digest': manifest_digest, 'archive_digest': layer['digest'], 'group': group,
            'runtime_admitted': False, 'release_ready': False}


def pull(archive, destination, actor='', credential='', opener=None):
    opener = opener or build_opener(SafeRedirect())
    token = get_token(opener, actor, credential)
    request = Request(f'{REGISTRY}/v2/{REPOSITORY}/manifests/latest', headers={
        'Accept': MANIFEST_TYPE, 'Authorization': 'Bearer ' + token,
        'User-Agent': 'WSX-kernel-packages'})
    try:
        with opener.open(request, timeout=60) as response:
            raw = response.read(MAX_MANIFEST + 1)
            manifest_digest = response.headers.get('Docker-Content-Digest')
    except HTTPError as error:
        if error.code == 404:
            raise ValueError('Latest package was not found; absence and access must be independently verified') from error
        raise ValueError(f'Registry manifest request failed with HTTP {error.code}') from error
    layer = parse_manifest(raw, manifest_digest)
    download_layer(opener, token, layer, archive)
    index = package.extract(archive, destination)
    return {'manifest_digest': manifest_digest, 'archive_digest': layer['digest'],
            'generation': index['generation'], 'groups': len(index['groups']), 'targets': len(index['targets']),
            'scope': 'Integrity only; publisher identity, CRC compatibility and release admission are not proved'}


def build_manifest(archive):
    size = archive.stat().st_size
    if not archive.is_file() or archive.is_symlink() or not 0 < size <= MAX_ARCHIVE:
        raise ValueError('Invalid snapshot archive')
    layer_digest = 'sha256:' + package.digest(archive)
    config = b'{}'
    manifest = {'schemaVersion': 2, 'mediaType': MANIFEST_TYPE, 'artifactType': LAYER_TYPE,
                'config': {'mediaType': 'application/vnd.oci.empty.v1+json',
                           'digest': 'sha256:' + hashlib.sha256(config).hexdigest(), 'size': len(config)},
                'layers': [{'mediaType': LAYER_TYPE, 'digest': layer_digest, 'size': size}],
                'annotations': {'org.opencontainers.image.source': 'https://github.com/WSLd-wsx/wsld'}}
    return json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode(), config


def build_group_manifest(archive, group):
    if not isinstance(group, str) or not re.fullmatch(r'[0-9a-f]{64}', group):
        raise ValueError('Invalid module group identity')
    if archive.is_symlink() or not archive.is_file() or not 0 < archive.stat().st_size <= MAX_ARCHIVE:
        raise ValueError('Invalid module group archive')
    config = b'{}'
    manifest = {'schemaVersion': 2, 'mediaType': MANIFEST_TYPE, 'artifactType': GROUP_LAYER_TYPE,
                'config': {'mediaType': 'application/vnd.oci.empty.v1+json',
                           'digest': 'sha256:' + hashlib.sha256(config).hexdigest(), 'size': len(config)},
                'layers': [{'mediaType': GROUP_LAYER_TYPE, 'digest': 'sha256:' + package.digest(archive),
                            'size': archive.stat().st_size}],
                'annotations': {'org.opencontainers.image.source': 'https://github.com/WSLd-wsx/wsld',
                                'io.wsx.kernel-module-group': group}}
    return json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode(), config


def validate_group_index(value):
    base = {'schema_version', 'generation', 'groups', 'targets', 'evidence', 'runtime_admitted', 'release_ready'}
    if (not isinstance(value, dict) or value.get('schema_version') not in (1, 2)
            or set(value) != (base | ({'build_inputs'} if value.get('schema_version') == 2 else set()))
            or not isinstance(value['generation'], str)
            or not re.fullmatch(r'[0-9a-f]{64}', value['generation'])
            or value['runtime_admitted'] is not False or value['release_ready'] is not False
            or not isinstance(value['groups'], dict) or not 0 < len(value['groups']) <= 512
            or not isinstance(value['targets'], dict) or not 0 < len(value['targets']) <= 4096):
        raise ValueError('Invalid module group index')
    evidence = value['evidence']
    if (not isinstance(evidence, dict) or set(evidence) != {'archive_sha256', 'bytes', 'oci_manifest_sha256'}
            or not isinstance(evidence['archive_sha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', evidence['archive_sha256'])
            or not isinstance(evidence['oci_manifest_sha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', evidence['oci_manifest_sha256'])
            or not isinstance(evidence['bytes'], int) or isinstance(evidence['bytes'], bool)
            or not 0 < evidence['bytes'] <= MAX_ARCHIVE):
        raise ValueError('Invalid kernel evidence descriptor')
    if value['schema_version'] == 2:
        inputs = value['build_inputs']
        if (not isinstance(inputs, dict) or set(inputs) != {'archive_sha256', 'bytes', 'oci_manifest_sha256', 'corresponding_source_complete'}
                or inputs['corresponding_source_complete'] is not False
                or not isinstance(inputs['archive_sha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', inputs['archive_sha256'])
                or not isinstance(inputs['oci_manifest_sha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', inputs['oci_manifest_sha256'])
                or not isinstance(inputs['bytes'], int) or isinstance(inputs['bytes'], bool)
                or not 0 < inputs['bytes'] <= MAX_ARCHIVE):
            raise ValueError('Invalid build input archive descriptor')
    for group, info in value['groups'].items():
        if (not re.fullmatch(r'[0-9a-f]{64}', group) or not isinstance(info, dict)
                or set(info) != {'archive_sha256', 'bytes', 'oci_manifest_sha256'}
                or not isinstance(info['archive_sha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', info['archive_sha256'])
                or not isinstance(info['oci_manifest_sha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', info['oci_manifest_sha256'])
                or not isinstance(info['bytes'], int) or isinstance(info['bytes'], bool) or not 0 < info['bytes'] <= MAX_ARCHIVE):
            raise ValueError('Invalid module group index entry')
    used = set()
    for tag, target in value['targets'].items():
        if (not package.TAG.fullmatch(tag) or not isinstance(target, dict)
                or set(target) != {'kernel_build_id', 'kernel_image_sha256', 'group'}
                or not isinstance(target['kernel_build_id'], str) or not package.BUILD_ID.fullmatch(target['kernel_build_id'])
                or not isinstance(target['kernel_image_sha256'], str) or not package.SHA.fullmatch(target['kernel_image_sha256'])
                or not isinstance(target['group'], str) or target['group'] not in value['groups']):
            raise ValueError('Invalid kernel target index entry')
        used.add(target['group'])
    if used != set(value['groups']):
        raise ValueError('Unreferenced module group in index')
    return value


def build_index_manifest(index_path):
    if index_path.is_symlink() or not index_path.is_file() or not 0 < index_path.stat().st_size <= MAX_MANIFEST:
        raise ValueError('Invalid module group index file')
    validate_group_index(json.loads(index_path.read_text()))
    config = b'{}'
    manifest = {'schemaVersion': 2, 'mediaType': MANIFEST_TYPE, 'artifactType': INDEX_LAYER_TYPE,
                'config': {'mediaType': 'application/vnd.oci.empty.v1+json',
                           'digest': 'sha256:' + hashlib.sha256(config).hexdigest(), 'size': len(config)},
                'layers': [{'mediaType': INDEX_LAYER_TYPE, 'digest': 'sha256:' + package.digest(index_path),
                            'size': index_path.stat().st_size}],
                'annotations': {'org.opencontainers.image.source': 'https://github.com/WSLd-wsx/wsld'}}
    return json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode(), config


def build_evidence_manifest(archive):
    if archive.is_symlink() or not archive.is_file() or not 0 < archive.stat().st_size <= MAX_ARCHIVE:
        raise ValueError('Invalid kernel evidence archive')
    config = b'{}'
    manifest = {'schemaVersion': 2, 'mediaType': MANIFEST_TYPE, 'artifactType': METADATA_LAYER_TYPE,
                'config': {'mediaType': 'application/vnd.oci.empty.v1+json',
                           'digest': 'sha256:' + hashlib.sha256(config).hexdigest(), 'size': len(config)},
                'layers': [{'mediaType': METADATA_LAYER_TYPE, 'digest': 'sha256:' + package.digest(archive),
                            'size': archive.stat().st_size}],
                'annotations': {'org.opencontainers.image.source': 'https://github.com/WSLd-wsx/wsld'}}
    return json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode(), config


def pull_evidence(output, manifest_digest, actor='', credential='', opener=None):
    if not isinstance(manifest_digest, str) or not SHA.fullmatch(manifest_digest):
        raise ValueError('Invalid immutable evidence identity')
    opener = opener or build_opener(SafeRedirect())
    token = get_token(opener, actor, credential)
    request = Request(f'{REGISTRY}/v2/{REPOSITORY}/manifests/{manifest_digest}', headers={
        'Accept': MANIFEST_TYPE, 'Authorization': 'Bearer ' + token, 'User-Agent': 'WSX-kernel-packages'})
    with opener.open(request, timeout=60) as response:
        raw = response.read(MAX_MANIFEST + 1)
        header = response.headers.get('Docker-Content-Digest')
    if len(raw) > MAX_MANIFEST or header != manifest_digest or 'sha256:' + hashlib.sha256(raw).hexdigest() != manifest_digest:
        raise ValueError('Evidence manifest digest mismatch')
    value = json.loads(raw)
    expected_config = {'mediaType': 'application/vnd.oci.empty.v1+json',
                       'digest': 'sha256:' + hashlib.sha256(b'{}').hexdigest(), 'size': 2}
    layers = value.get('layers') if isinstance(value, dict) else None
    if (not isinstance(value, dict) or value.get('schemaVersion') != 2 or value.get('mediaType') != MANIFEST_TYPE
            or value.get('artifactType') != METADATA_LAYER_TYPE or value.get('config') != expected_config
            or not isinstance(layers, list) or len(layers) != 1 or not isinstance(layers[0], dict)):
        raise ValueError('Unsupported evidence manifest')
    layer = layers[0]
    if (layer.get('mediaType') != METADATA_LAYER_TYPE or not isinstance(layer.get('digest'), str)
            or not SHA.fullmatch(layer['digest']) or not isinstance(layer.get('size'), int)
            or isinstance(layer['size'], bool) or not 0 < layer['size'] <= MAX_ARCHIVE):
        raise ValueError('Invalid evidence descriptor')
    download_layer(opener, token, layer, output)
    return {'manifest_digest': manifest_digest, 'archive_digest': layer['digest'], 'release_ready': False}


def pull_index(output, manifest_digest, actor='', credential='', opener=None):
    if not isinstance(manifest_digest, str) or not SHA.fullmatch(manifest_digest):
        raise ValueError('Invalid immutable index identity')
    opener = opener or build_opener(SafeRedirect())
    token = get_token(opener, actor, credential)
    request = Request(f'{REGISTRY}/v2/{REPOSITORY}/manifests/{manifest_digest}', headers={
        'Accept': MANIFEST_TYPE, 'Authorization': 'Bearer ' + token, 'User-Agent': 'WSX-kernel-packages'})
    with opener.open(request, timeout=60) as response:
        raw = response.read(MAX_MANIFEST + 1)
        header = response.headers.get('Docker-Content-Digest')
    if len(raw) > MAX_MANIFEST or header != manifest_digest or 'sha256:' + hashlib.sha256(raw).hexdigest() != manifest_digest:
        raise ValueError('Index manifest digest mismatch')
    value = json.loads(raw)
    expected_config = {'mediaType': 'application/vnd.oci.empty.v1+json',
                       'digest': 'sha256:' + hashlib.sha256(b'{}').hexdigest(), 'size': 2}
    layers = value.get('layers') if isinstance(value, dict) else None
    if (not isinstance(value, dict) or value.get('schemaVersion') != 2 or value.get('mediaType') != MANIFEST_TYPE
            or value.get('artifactType') != INDEX_LAYER_TYPE or value.get('config') != expected_config
            or not isinstance(layers, list) or len(layers) != 1 or not isinstance(layers[0], dict)):
        raise ValueError('Unsupported index manifest')
    layer = layers[0]
    if (layer.get('mediaType') != INDEX_LAYER_TYPE or not isinstance(layer.get('digest'), str)
            or not SHA.fullmatch(layer['digest']) or not isinstance(layer.get('size'), int)
            or isinstance(layer['size'], bool) or not 0 < layer['size'] <= MAX_MANIFEST):
        raise ValueError('Invalid index descriptor')
    download_layer(opener, token, layer, output)
    index = validate_group_index(json.loads(output.read_text()))
    return {'manifest_digest': manifest_digest, 'index_digest': layer['digest'],
            'groups': len(index['groups']), 'targets': len(index['targets']),
            'runtime_admitted': False, 'release_ready': False}


def latest_index_digest(actor='', credential='', opener=None):
    opener = opener or build_opener(SafeRedirect())
    token = get_token(opener, actor, credential)
    request = Request(f'{REGISTRY}/v2/{REPOSITORY}/manifests/latest', headers={
        'Accept': MANIFEST_TYPE, 'Authorization': 'Bearer ' + token, 'User-Agent': 'WSX-kernel-packages'})
    try:
        with opener.open(request, timeout=60) as response:
            raw = response.read(MAX_MANIFEST + 1)
            digest = response.headers.get('Docker-Content-Digest')
    except HTTPError as error:
        raise ValueError(f'Cannot read latest kernel index (HTTP {error.code}); absence is not established') from error
    if (len(raw) > MAX_MANIFEST or not isinstance(digest, str) or not SHA.fullmatch(digest)
            or 'sha256:' + hashlib.sha256(raw).hexdigest() != digest):
        raise ValueError('Latest kernel index manifest digest mismatch')
    value = json.loads(raw)
    if (not isinstance(value, dict) or value.get('schemaVersion') != 2
            or value.get('mediaType') != MANIFEST_TYPE or value.get('artifactType') != INDEX_LAYER_TYPE):
        raise ValueError('Latest registry artifact is not a kernel group index')
    return digest


def upload_blob(opener, token, digest, size, data):
    headers = {'Authorization': 'Bearer ' + token, 'User-Agent': 'WSX-kernel-packages'}
    request = Request(f'{REGISTRY}/v2/{REPOSITORY}/blobs/uploads/', data=b'', headers=headers, method='POST')
    with opener.open(request, timeout=120) as response:
        if response.status != 202:
            raise ValueError('Registry did not create an upload session')
        location = response.headers.get('Location')
    if not location:
        raise ValueError('Registry upload location is missing')
    target = urlsplit(urljoin(REGISTRY, location))
    if target.scheme != 'https' or target.netloc != 'ghcr.io' or target.fragment:
        raise ValueError('Registry upload location is outside the trusted registry')
    query = parse_qsl(target.query, keep_blank_values=True)
    if any(key == 'digest' for key, _ in query):
        raise ValueError('Registry upload location already contains a digest')
    # Preserve opaque registry query bytes; only append the required digest.
    query_text = target.query + ('&' if target.query else '') + urlencode({'digest': digest})
    target = urlunsplit((target.scheme, target.netloc, target.path, query_text, ''))
    headers.update({'Content-Type': 'application/octet-stream', 'Content-Length': str(size)})
    with opener.open(Request(target, data=data, headers=headers, method='PUT'), timeout=600) as response:
        if response.status != 201 or response.headers.get('Docker-Content-Digest') != digest:
            raise ValueError('Registry did not confirm the uploaded blob digest')


def put_manifest(opener, token, reference, manifest):
    if not SHA.fullmatch(reference) or reference != 'sha256:' + hashlib.sha256(manifest).hexdigest():
        raise ValueError('Only the exact content-addressed manifest digest is permitted')
    headers = {'Authorization': 'Bearer ' + token, 'Content-Type': MANIFEST_TYPE,
               'User-Agent': 'WSX-kernel-packages'}
    request = Request(f'{REGISTRY}/v2/{REPOSITORY}/manifests/{reference}', data=manifest, headers=headers, method='PUT')
    with opener.open(request, timeout=120) as response:
        expected = 'sha256:' + hashlib.sha256(manifest).hexdigest()
        if response.status != 201 or response.headers.get('Docker-Content-Digest') != expected:
            raise ValueError('Registry did not confirm the uploaded manifest digest')
    return expected


def verify_snapshot_against_gate(index, coverage, gate_result):
    if index.get('schema_version') not in (2, 3):
        raise ValueError('Release snapshots must retain content-addressed build evidence')
    if index['generation'] != gate_result['build_generation']:
        raise ValueError('Snapshot build generation differs from the release gate')
    targets = gate_result['targets']
    if not targets or set(index['targets']) != {target['tag'] for target in targets}:
        raise ValueError('Snapshot target inventory differs from the release gate')
    for target in targets:
        tag = target['tag']
        entry = index['targets'][tag]
        if target.get('covered') is not True or entry['source_commit'] != target['source_commit']:
            raise ValueError('Snapshot target is not admitted by the release gate')
        record = json.loads((coverage / 'targets' / (tag + '.json')).read_text())
        if (record['build_generation'] != index['generation'] or record['source_commit'] != entry['source_commit']
                or record['kernel_build_id'] != entry['kernel_build_id']
                or record['kernel_image_sha256'] != entry['kernel_image_sha256']):
            raise ValueError('Snapshot target differs from its checked coverage evidence')
        if record.get('build_source_commit', record['source_commit']) != entry.get('build_source_commit', entry['source_commit']):
            raise ValueError('Snapshot build source differs from checked coverage evidence')
        audit_path = (coverage / record['audit']).resolve(strict=True)
        receipt_path = (coverage / record['receipt']).resolve(strict=True)
        build_path = (coverage / record['build']).resolve(strict=True)
        if not all(path.is_relative_to(coverage.resolve()) for path in (audit_path, receipt_path, build_path)):
            raise ValueError('Coverage evidence is outside the coverage directory')
        if (package.digest(audit_path) != entry['audit_sha256'] or package.digest(receipt_path) != target['detail']
                or package.digest(build_path) != entry['build_sha256']
                or record['build_sha256'] != entry['build_sha256']):
            raise ValueError('Snapshot evidence differs from the checked coverage receipt')
        receipt = json.loads(receipt_path.read_text())
        expected = {name: receipt['guest/' + name] for name in package.MODULES}
        if index['groups'][entry['group']] != expected:
            raise ValueError('Snapshot modules differ from the checked coverage receipt')


def check_prior_index(expected, actor, credential, opener):
    if not isinstance(expected, str) or not SHA.fullmatch(expected):
        raise ValueError('An immutable prior index digest is required')
    if latest_index_digest(actor, credential, opener=opener) != expected:
        raise ValueError('Registry latest changed since bootstrap; publication stopped')


def require_corresponding_source(exporter, exported, generation):
    archive = exported / 'build-inputs.tar.gz'
    if archive.is_symlink() or not archive.is_file():
        raise ValueError('Checked WSX build input archive is required before publishing')
    manifest = exporter.build_inputs_export.verify(archive, generation)
    if manifest.get('corresponding_source_complete') is not True:
        raise ValueError('Corresponding source delivery is incomplete; no group was published')


def publish_groups(snapshot, exported, coverage, report, actor, credential, opener=None, expected_prior=None):
    """Publish immutable groups after the coverage gate and verify registry read-back."""
    gate = Path(__file__).with_name('kernel-release-coverage.py')
    admission = Path(__file__).parents[1] / 'core/compatibility/host-updates.json'
    result = subprocess.run([sys.executable, str(gate), '--coverage', str(coverage),
                             '--admission', str(admission), '--report', str(report)], check=False)
    if result.returncode != 0:
        raise ValueError('Kernel release coverage gate failed; no group was published')
    gate_result = json.loads(report.read_text())
    if gate_result.get('release_gate_passed') is not True:
        raise ValueError('Kernel release coverage was not admitted')
    index = package.validate(snapshot)
    verify_snapshot_against_gate(index, coverage, gate_result)
    exporter_spec = importlib.util.spec_from_file_location('publish_export_check', Path(__file__).with_name('export-kernel-groups.py'))
    exporter = importlib.util.module_from_spec(exporter_spec)
    exporter_spec.loader.exec_module(exporter)
    exported_index = exporter.verify_export(snapshot, exported)
    require_corresponding_source(exporter, exported, index['generation'])
    groups = exported_index['groups']
    opener = opener or build_opener(SafeRedirect())
    check_prior_index(expected_prior, actor, credential, opener)
    token = get_token(opener, actor, credential, push=True)
    config = b'{}'
    upload_blob(opener, token, 'sha256:' + hashlib.sha256(config).hexdigest(), len(config), config)
    verified = []
    for group, info in sorted(groups.items()):
        archive = exported / (group + '.tar.gz')
        manifest = (exported / (group + '.oci.json')).read_bytes()
        reference = 'sha256:' + info['oci_manifest_sha256']
        with archive.open('rb') as stream:
            upload_blob(opener, token, 'sha256:' + info['archive_sha256'], info['bytes'], stream)
        put_manifest(opener, token, reference, manifest)
        with tempfile.TemporaryDirectory(prefix='wsx-group-readback-') as temporary:
            readback = Path(temporary) / 'group.tar.gz'
            checked = pull_group(readback, reference, group, actor, credential, opener=opener)
            if checked['archive_digest'] != 'sha256:' + info['archive_sha256']:
                raise ValueError('Registry group read-back differs from the checked archive')
        verified.append({'group': group, 'manifest_digest': reference})
    exporter.verify_export(snapshot, exported)
    evidence = exported_index['evidence']
    evidence_path = exported / 'evidence.tar.gz'
    evidence_reference = 'sha256:' + evidence['oci_manifest_sha256']
    with evidence_path.open('rb') as stream:
        upload_blob(opener, token, 'sha256:' + evidence['archive_sha256'], evidence['bytes'], stream)
    put_manifest(opener, token, evidence_reference, (exported / 'evidence.oci.json').read_bytes())
    with tempfile.TemporaryDirectory(prefix='wsx-evidence-readback-') as temporary:
        checked = pull_evidence(Path(temporary) / 'evidence.tar.gz', evidence_reference, actor, credential, opener=opener)
        if checked['archive_digest'] != 'sha256:' + evidence['archive_sha256']:
            raise ValueError('Registry evidence read-back differs from the checked archive')
    if exported_index['schema_version'] != 2:
        raise ValueError('Independent build input archive descriptor is required')
    inputs = exported_index['build_inputs']
    inputs_reference = 'sha256:' + inputs['oci_manifest_sha256']
    with (exported / 'build-inputs.tar.gz').open('rb') as stream:
        upload_blob(opener, token, 'sha256:' + inputs['archive_sha256'], inputs['bytes'], stream)
    put_manifest(opener, token, inputs_reference, (exported / 'build-inputs.oci.json').read_bytes())
    with tempfile.TemporaryDirectory(prefix='wsx-inputs-readback-') as temporary:
        checked = pull_evidence(Path(temporary) / 'inputs.tar.gz', inputs_reference, actor, credential, opener=opener)
        if checked['archive_digest'] != 'sha256:' + inputs['archive_sha256']:
            raise ValueError('Registry build input read-back differs from the checked archive')
    index_path = exported / 'index.json'
    index_manifest = (exported / 'index.oci.json').read_bytes()
    reference = 'sha256:' + hashlib.sha256(index_manifest).hexdigest()
    index_digest = 'sha256:' + package.digest(index_path)
    with index_path.open('rb') as stream:
        upload_blob(opener, token, index_digest, index_path.stat().st_size, stream)
    put_manifest(opener, token, reference, index_manifest)
    with tempfile.TemporaryDirectory(prefix='wsx-index-readback-') as temporary:
        checked = pull_index(Path(temporary) / 'index.json', reference, actor, credential, opener=opener)
        if checked['index_digest'] != index_digest:
            raise ValueError('Registry index read-back differs from the checked index')
    check_prior_index(expected_prior, actor, credential, opener)
    return {'groups': verified, 'index_published': True, 'index_manifest_digest': reference,
            'latest_updated': False}


def publish_immutable(archive, coverage, report, actor, credential, opener=None):
    # This legacy snapshot format has no corresponding-source delivery contract.
    # Use the group publisher once that contract has been implemented and verified.
    raise ValueError('Snapshot publication lacks corresponding source delivery; no package was published')


def _publish_immutable_unadmitted(archive, coverage, report, actor, credential, opener=None):
    # The coverage command reruns the live upstream inventory and cannot currently
    # pass until target source/binary provenance and the all-version builder exist.
    gate = Path(__file__).with_name('kernel-release-coverage.py')
    admission = Path(__file__).parents[1] / 'core/compatibility/host-updates.json'
    result = subprocess.run([sys.executable, str(gate), '--coverage', str(coverage),
                             '--admission', str(admission), '--report', str(report)], check=False)
    if result.returncode != 0:
        raise ValueError('Kernel release coverage gate failed; no package was published')
    gate_result = json.loads(report.read_text())
    if gate_result.get('release_gate_passed') is not True:
        raise ValueError('Kernel release coverage was not admitted')
    with tempfile.TemporaryDirectory() as temp:
        index = package.extract(archive, Path(temp) / 'snapshot')
    verify_snapshot_against_gate(index, coverage, gate_result)
    # A mutable latest pointer needs a separate concurrency/ownership policy.
    # Until that is proven, this entry only writes a content-addressed tag.
    manifest, config = build_manifest(archive)
    archive_digest = 'sha256:' + package.digest(archive)
    reference = 'sha256:' + hashlib.sha256(manifest).hexdigest()
    opener = opener or build_opener(SafeRedirect())
    token = get_token(opener, actor, credential, push=True)
    upload_blob(opener, token, 'sha256:' + hashlib.sha256(config).hexdigest(), len(config), config)
    with archive.open('rb') as stream:
        upload_blob(opener, token, archive_digest, archive.stat().st_size, stream)
    manifest_digest = put_manifest(opener, token, reference, manifest)
    return {'reference': reference, 'manifest_digest': manifest_digest, 'archive_digest': archive_digest,
            'latest_updated': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--destination', type=Path)
    parser.add_argument('--publish-immutable', action='store_true')
    parser.add_argument('--publish-groups', action='store_true')
    parser.add_argument('--exported', type=Path)
    parser.add_argument('--expected-prior-index')
    parser.add_argument('--coverage', type=Path)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    try:
        actor, credential = os.environ.get('GITHUB_ACTOR', ''), os.environ.get('GITHUB_TOKEN', '')
        if args.publish_immutable and args.publish_groups:
            raise ValueError('Choose one publication mode')
        if args.publish_groups:
            if args.destination is not None or args.coverage is None or args.report is None or args.exported is None or not args.expected_prior_index:
                raise ValueError('Group publication requires --exported, --coverage, --report and --expected-prior-index, without --destination')
            result = publish_groups(args.archive, args.exported, args.coverage, args.report, actor, credential, expected_prior=args.expected_prior_index)
        elif args.publish_immutable:
            if args.destination is not None or args.coverage is None or args.report is None or args.exported is not None or args.expected_prior_index is not None:
                raise ValueError('Publication requires --coverage and --report, without --destination')
            result = publish_immutable(args.archive, args.coverage, args.report, actor, credential)
        else:
            if args.destination is None or args.coverage is not None or args.report is not None or args.exported is not None or args.expected_prior_index is not None:
                raise ValueError('Download requires --destination and does not accept coverage options')
            result = pull(args.archive, args.destination, actor, credential)
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        action = 'publication' if args.publish_groups or args.publish_immutable else 'download'
        print(f'Kernel package {action} failed: {error}', file=sys.stderr)
        return 1
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
