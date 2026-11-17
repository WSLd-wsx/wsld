"""Verify an independently pinned six-module group before Guest admission."""
import hashlib
import json
from pathlib import Path
import re
import runpy

NAMES = ('drm_kms_helper.ko', 'virtio_dma_buf.ko', 'virtio-gpu.ko',
         'virtio_msg.ko', 'virtio_msg_vsock.ko', 'wsx_virtio_msg_pci.ko')
SHA = re.compile(r'[0-9a-f]{64}\Z')


def verify(directory, manifest_digest, reuse_script):
    if not isinstance(manifest_digest, str) or not SHA.fullmatch(manifest_digest):
        raise ValueError('A trusted module group manifest digest is required')
    directory = Path(directory)
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError('Unsafe module group directory')
    manifest = directory / 'module-group.json'
    if manifest.is_symlink() or not manifest.is_file() or manifest.stat().st_size > 65536:
        raise ValueError('Unsafe module group manifest')
    data = manifest.read_bytes()
    if hashlib.sha256(data).hexdigest() != manifest_digest:
        raise ValueError('Module group manifest digest mismatch')
    record = json.loads(data)
    if (record.get('schema_version') != 1 or record.get('owner_configuration_abi') != 2
            or not isinstance(record.get('files'), dict) or set(record['files']) != set(NAMES)):
        raise ValueError('Unsupported module group or Owner ABI')
    build_note = runpy.run_path(str(reuse_script))['build_note']
    result = {}
    for name in NAMES:
        path, entry = directory / name, record['files'][name]
        if path.is_symlink() or not path.is_file() or not isinstance(entry, dict):
            raise ValueError('Unsafe module file: ' + name)
        if path.stat().st_size != entry.get('bytes') or not SHA.fullmatch(entry.get('sha256', '')):
            raise ValueError('Module metadata mismatch: ' + name)
        with path.open('rb') as stream:
            actual = hashlib.file_digest(stream, 'sha256').hexdigest()
        if actual != entry['sha256']:
            raise ValueError('Module digest mismatch: ' + name)
        if name == 'wsx_virtio_msg_pci.ko' and b'wsx_owner_configuration_abi=2\0' not in path.read_bytes():
            raise ValueError('Module does not declare Owner configuration ABI 2')
        result[name] = {'sha256': actual, 'build_note': build_note(path).hex()}
    return {'manifest_sha256': manifest_digest, 'owner_configuration_abi': 2,
            'files': result, 'runtime_admitted': False,
            'scope': 'Module file identity only; CRC, loaded identity, component compatibility and lifecycle require separate checks'}


def verify_loaded(group, sys_root=Path('/sys')):
    files = group.get('files', {})
    if set(files) != set(NAMES) or group.get('owner_configuration_abi') != 2:
        raise ValueError('Incomplete verified module group')
    for filename in NAMES:
        name = filename.removesuffix('.ko').replace('-', '_')
        module = sys_root / 'module' / name
        if (module / 'initstate').read_text().strip() != 'live':
            raise RuntimeError('Module is not live: ' + name)
        if (module / 'notes/.note.gnu.build-id').read_bytes().hex() != files[filename]['build_note']:
            raise RuntimeError('Loaded module differs from the verified module group: ' + name)
    owner_abi = sys_root / 'bus/pci/drivers/wsx-virtio-msg-pci/configuration_abi'
    if owner_abi.read_text().strip() != '2':
        raise RuntimeError('Loaded Owner configuration ABI is not 2')
    return {'loaded_group_verified': True, 'owner_configuration_abi': 2,
            'runtime_admitted': False}
