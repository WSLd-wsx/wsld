"""Admit and reconfigure only a fully retired, exact-reference Guest Owner.

Adapted from the prototype's normal rmmod/insmod restart contract. Never force
unload, touch the other five modules, replace a bound owner, or infer GPU drain.
Core supplies independently verified native retirement and previous receipts.
"""
import json
import os
from pathlib import Path
import runpy
import stat
import struct
import subprocess
import uuid

MODULES = [('drm_kms_helper', 'drm_kms_helper.ko'),
           ('virtio_dma_buf', 'virtio_dma_buf.ko'), ('virtio_gpu', 'virtio-gpu.ko'),
           ('virtio_msg', 'virtio_msg.ko'), ('virtio_msg_vsock', 'virtio_msg_vsock.ko'),
           ('wsx_virtio_msg_pci', 'wsx_virtio_msg_pci.ko')]
SYS = Path('/sys')
RUN = Path('/run')
PROC = Path('/proc')


def build_note(path):
    with path.open('rb') as stream:
        data = stream.read(32 * 1024 * 1024 + 1)
    if len(data) > 32 * 1024 * 1024 or data[:6] != b'\x7fELF\x02\x01':
        raise ValueError('Invalid reference module ELF')
    if struct.unpack_from('<H', data, 18)[0] != 62:
        raise ValueError('Expected x86-64 module')
    table = struct.unpack_from('<Q', data, 40)[0]
    size, count, names_index = struct.unpack_from('<HHH', data, 58)
    if size != 64 or not 0 < names_index < count or count > 4096:
        raise ValueError('Invalid module section table')
    entries = [struct.unpack_from('<IIQQQQIIQQ', data, table + i * size) for i in range(count)]
    def contents(e):
        offset, length = e[4:6]
        if offset + length > len(data):
            raise ValueError('Module section exceeds file')
        return data[offset:offset + length]
    names = contents(entries[names_index])
    found = []
    for e in entries:
        if e[0] >= len(names):
            raise ValueError('Module section name exceeds string table')
        name = names[e[0]:names.index(0, e[0])]
        if name == b'.note.gnu.build-id':
            note = contents(e)
            if len(note) < 20 or note[12:16] != b'GNU\0' or struct.unpack_from('<I', note, 8)[0] != 3:
                raise ValueError('Invalid module build note')
            found.append(note)
    if len(found) != 1:
        raise ValueError('Expected exactly one module build identity')
    return found[0]


def inspect_loaded(root, module_directory=None):
    loaded = {}
    module_directory = module_directory or root / 'guest'
    for name, file in MODULES:
        directory = SYS / 'module' / name
        if not directory.exists():
            continue
        if (directory / 'initstate').read_text().strip() != 'live':
            raise RuntimeError('Module is not live: ' + name)
        actual = (directory / 'notes/.note.gnu.build-id').read_bytes()
        if actual != build_note(module_directory / file):
            raise RuntimeError('Loaded module differs from the pinned reference: ' + name)
        loaded[name] = actual.hex()
    return loaded


def retired_owner(previous, allow_peers=False):
    if not previous or previous.get('native_retirement_confirmed') is not True:
        raise RuntimeError('Verified prior native retirement is required before module reuse')
    instance = uuid.UUID(previous['native']['instance'].strip('{}'))
    if not instance.int:
        raise ValueError('Invalid previous owner identity')
    prepared = previous['prepared']
    if prepared['instance'] != str(instance):
        raise RuntimeError('Previous native/Guest receipt mismatch')
    # Require absence through actual VMBus ancestry, not just a cached PCI name.
    for device in (SYS / 'bus/pci/devices').iterdir():
        for parent in device.resolve(strict=True).parents:
            subsystem = parent / 'subsystem'
            if subsystem.is_symlink() and subsystem.resolve().name == 'vmbus':
                if uuid.UUID((parent / 'device_id').read_text().strip().strip('{}')) == instance:
                    raise RuntimeError('Previous PCI owner still exists')
                break
    driver = SYS / 'bus/pci/drivers/wsx-virtio-msg-pci'
    if not allow_peers and driver.exists() and any(p.is_symlink() and p.name != 'module' for p in driver.iterdir()):
        raise RuntimeError('An existing bound PCI owner must not be changed')
    # Do not remove descriptors while a process can still refer to the session.
    marker = ('WSX_DESKTOP_INSTANCE=' + str(instance)).encode()
    for process in PROC.glob('[0-9]*'):
        try:
            if marker in (process / 'environ').read_bytes().split(b'\0'):
                raise RuntimeError('Previous Guest session process remains alive')
        except (FileNotFoundError, ProcessLookupError):
            continue
    if (RUN / 'seatd.sock').exists():
        raise RuntimeError('Existing seat socket requires diagnosis')
    allowed = RUN / 'wsx' / (instance.hex + '.json')
    for descriptor in (RUN / 'wsx').glob('*.json'):
        if descriptor != allowed:
            raise RuntimeError('Foreign Guest descriptor must not be changed')
        st = descriptor.lstat()
        if not stat.S_ISREG(st.st_mode) or st.st_uid != 0 or st.st_mode & 0o022:
            raise RuntimeError('Untrusted previous Guest descriptor')
        spec = json.loads(descriptor.read_text())
        if spec['instance_id'] != str(instance) or spec['drm_node'] not in prepared['access']['nodes'] or spec['user'] != prepared['access']['user']:
            raise RuntimeError('Previous Guest descriptor differs from the retained receipt')
    return instance, allowed


def check(root, previous, module_directory=None, manifest_digest=None):
    independent = root / 'module-group'
    if module_directory is None and independent.exists():
        # Diagnostic group admitted by the matching host build. This pin is
        # separate from the Host/renderer receipt and is not release admission.
        module_directory = independent
        if manifest_digest is None:
            manifest_digest = '2cba196fcc056740a90186eeebfbd5354d21c8115994e2c69229d3df9a519a72'
    if (module_directory is None) != (manifest_digest is None):
        raise ValueError('Both module group directory and trusted manifest digest are required')
    if module_directory is not None:
        group = runpy.run_path(str(root / 'module-group.py'))['verify'](
            module_directory, manifest_digest, root / 'module-reuse.py')
        if group['owner_configuration_abi'] != 2:
            raise RuntimeError('Unsupported module group Owner ABI')
    loaded = inspect_loaded(root, module_directory)
    if module_directory is not None:
        if loaded and len(loaded) != len(MODULES):
            raise RuntimeError('Independent module group is only partially loaded')
        if loaded:
            runpy.run_path(str(root / 'module-group.py'))['verify_loaded'](group, SYS)
    reference = ((module_directory or root / 'guest') / 'wsx_virtio_msg_pci.ko').read_bytes()
    multi_owner = b'wsx_owner_configuration_abi=2\0' in reference
    if multi_owner:
        owner = SYS / 'module/wsx_virtio_msg_pci'
        if owner.exists() and (SYS / 'bus/pci/drivers/wsx-virtio-msg-pci/configuration_abi').read_text().strip() != '2':
            raise RuntimeError('Loaded Owner does not expose configuration ABI 2')
        # Module identities were verified above. Bound peers belong to other
        # exact devices and must never be retired or unloaded by this distro.
        instance, descriptor = None, None
        if list((RUN / 'wsx').glob('*.json')):
            if not previous:
                raise RuntimeError('This Guest retains session descriptors; inspect its lifetime')
            instance, descriptor = retired_owner(previous, allow_peers=True)
        return dict(loaded=loaded, reload_required=False, multi_owner=True,
                    descriptor_archive_required=bool(descriptor and descriptor.exists()),
                    previous_instance=str(instance) if instance else None)

    if loaded and not previous:
        raise RuntimeError('Loaded reference modules have no admitted previous lifetime')
    if previous:
        instance, descriptor = retired_owner(previous)
    elif list((RUN / 'wsx').glob('*.json')):
        raise RuntimeError('Retained Guest descriptors require diagnosis')
    owner = SYS / 'module/wsx_virtio_msg_pci'
    if owner.exists():
        if int((owner / 'refcnt').read_text()) != 0 or list((owner / 'holders').iterdir()):
            raise RuntimeError('Retired Owner still has references or module holders')
        params = {p.name: p.read_text().strip() for p in (owner / 'parameters').iterdir()}
        profile = previous['profile']
        if (uuid.UUID(params['host_instance']) != instance or params['pci_bdf'] != previous['prepared']['device'] or
                any(int(params[k], 0) != profile[k] for k in ('aperture_size', 'device_number')) or
                int(params['service_port']) != profile['port'] or int(params['aperture_bar']) != 0):
            raise RuntimeError('Loaded Owner parameters differ from the retired lifetime')
    return dict(loaded=loaded, reload_required=owner.exists(),
                descriptor_archive_required=bool(previous and descriptor.exists()),
                previous_instance=str(instance) if previous else None)


def archive_descriptor(descriptor, archive):
    # /run and /var may be different filesystems. Commit inside the archive
    # filesystem before removing the verified original; never overwrite history.
    fd = os.open(descriptor, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW)
    with os.fdopen(fd, 'rb') as stream:
        original = os.fstat(stream.fileno())
        if not stat.S_ISREG(original.st_mode) or original.st_size > 65536:
            raise RuntimeError('Invalid retired descriptor')
        data = stream.read(65537)
    temporary = archive / ('.new-' + uuid.uuid4().hex)
    target = archive / descriptor.name
    try:
        fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, 'wb') as stream:
            stream.write(data); stream.flush(); os.fsync(stream.fileno())
        try:
            os.link(temporary, target, follow_symlinks=False)
        except FileExistsError:
            if target.is_symlink() or target.read_bytes() != data:
                raise RuntimeError('Retired descriptor archive conflicts with existing history')
        directory = os.open(archive, os.O_RDONLY | os.O_DIRECTORY)
        try: os.fsync(directory)
        finally: os.close(directory)
        current = descriptor.lstat()
        if (current.st_dev, current.st_ino) != (original.st_dev, original.st_ino) or descriptor.read_bytes() != data:
            raise RuntimeError('Retired descriptor changed during archival')
        descriptor.unlink()
    finally:
        temporary.unlink(missing_ok=True)


def recycle(root, previous, allow_reload, manifest_digest=None):
    before = check(root, previous, manifest_digest=manifest_digest)
    if before['reload_required']:
        if allow_reload is not True:
            raise RuntimeError('Retired Owner reload requires explicit --reload-retired-owner authorization')
        # Same operation as the verified launcher. Kernel refcounts reject races;
        # never pass force flags and never touch the other five modules.
        # Do not kill an in-flight module operation on a Python timeout. Core's
        # bounded observation retains the caller and recycle intent on timeout.
        subprocess.run(['/usr/bin/rmmod', 'wsx_virtio_msg_pci'], check=True, capture_output=True, text=True)
        if (SYS / 'module/wsx_virtio_msg_pci').exists():
            raise RuntimeError('Retired Owner module still exists after normal removal')
    if previous:
        instance, descriptor = retired_owner(previous, allow_peers=before.get('multi_owner', False))
        if descriptor.exists():
            archive = Path('/var/lib/wsx/core-retired')
            archive.mkdir(mode=0o700, exist_ok=True)
            st = archive.lstat()
            if not stat.S_ISDIR(st.st_mode) or st.st_uid != 0 or st.st_mode & 0o022:
                raise RuntimeError('Unsafe retired descriptor archive')
            archive_descriptor(descriptor, archive)
    return dict(previous=before, owner_module_removed=before['reload_required'], other_modules_unloaded=False)
