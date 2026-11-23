"""Core-owned candidate device bootstrap adapted from the verified r7 launcher.

Retains VMBus instance discovery and binding. Rootfs owns prepare/session/start.
Only explicitly authorized, fully retired Owner reload is supported; no image
patch, GPU-specific branch, force unload, or process termination.
"""
import hashlib
import json
import os
from pathlib import Path
import pwd
import runpy
import stat
import subprocess
import sys
import time
import uuid

MODULES = [('drm_kms_helper', 'drm_kms_helper.ko'),
           ('virtio_dma_buf', 'virtio_dma_buf.ko'), ('virtio_gpu', 'virtio-gpu.ko'),
           ('virtio_msg', 'virtio_msg.ko'), ('virtio_msg_vsock', 'virtio_msg_vsock.ko'),
           ('wsx_virtio_msg_pci', 'wsx_virtio_msg_pci.ko')]


def run(argv, **kwargs):
    # Core bounds observation without killing in-flight device/session operations.
    result = subprocess.run(argv, check=True, capture_output=True, text=True, **kwargs)
    if result.stdout:
        print(result.stdout, file=sys.stderr, end='')
    return result


def find_device(instance):
    # Preserved VMBus ancestry algorithm from the pinned prototype launcher.
    matches = []
    for candidate in Path('/sys/bus/pci/devices').iterdir():
        for parent in candidate.resolve(strict=True).parents:
            subsystem = parent / 'subsystem'
            if subsystem.is_symlink() and subsystem.resolve(strict=True).name == 'vmbus':
                if uuid.UUID((parent / 'device_id').read_text().strip().strip('{}')) == instance:
                    matches.append(candidate)
                break
    if len(matches) > 1:
        raise RuntimeError('Multiple PCI functions match the host instance')
    return matches[0] if matches else None


def trusted_bytes(path, limit=2 * 1024 * 1024):
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW), 'rb') as stream:
        st = os.fstat(stream.fileno())
        if not stat.S_ISREG(st.st_mode) or st.st_uid != 0 or st.st_mode & 0o022 or st.st_size > limit:
            raise ValueError('Untrusted Guest compatibility metadata')
        data = stream.read(limit + 1)
        if len(data) > limit:
            raise ValueError('Guest compatibility metadata exceeds limit')
        return data


def legacy_plasma_admission():
    # Compatibility adapter for the published ABI 1 images, not a Python import
    # of Guest implementation internals. New images use the preflight command.
    required = {'cx16', 'lahf_lm', 'pni', 'popcnt', 'sse4_1', 'sse4_2', 'ssse3',
                'avx', 'avx2', 'bmi1', 'bmi2', 'f16c', 'fma', 'abm', 'movbe', 'xsave'}
    flags = [set(line.split(':', 1)[1].split()) for line in Path('/proc/cpuinfo').read_text().splitlines()
             if line.startswith('flags') and ':' in line]
    if not flags or any(not required.issubset(f) for f in flags):
        raise ValueError('Private KWin requires x86-64-v3 on every Guest CPU')
    expected = json.loads(trusted_bytes('/usr/lib/wsx/plasma-compat.json', 65536))['packages']
    if set(expected) != {'kwin', 'plasma-workspace', 'qt6-base'}:
        raise ValueError('Incomplete Plasma package compatibility lock')
    result = subprocess.run(['/usr/bin/pacman', '-Q', *sorted(expected)], check=True,
                            capture_output=True, text=True, timeout=10)
    actual = dict(line.split(' ', 1) for line in result.stdout.splitlines())
    if actual != expected:
        raise ValueError('Installed Plasma packages differ from the rootfs compatibility lock')


def guest_call(operation, params, session_instance=None):
    request = dict(protocol='wsx-guest', major=1, minor=0, id=uuid.uuid4().hex,
                   operation=operation, params=params)
    if session_instance is None:
        command = ['/usr/bin/wsx-guest', 'host-call']
    else:
        identity = uuid.UUID(str(session_instance))
        if not identity.int: raise ValueError('Invalid session identity')
        command = ['/usr/bin/python3', '-I', '/usr/lib/wsx/bootstrap-v2.py',
                   'dispatch', 'host-call', '--instance', str(identity)]
    process = subprocess.run(command, input=json.dumps(request),
                             check=True, capture_output=True, text=True, timeout=30)
    if len(process.stdout) > 1024 * 1024:
        raise ValueError('Guest protocol response exceeds limit')
    reply = json.loads(process.stdout)
    if reply.get('protocol') != 'wsx-guest' or reply.get('major') != 1 or reply.get('id') != request['id']:
        raise ValueError('Guest protocol identity or version mismatch')
    if reply.get('ok') is not True:
        raise ValueError('Guest protocol rejected request: ' + json.dumps(reply.get('error')))
    return reply['result']


def protocol_preflight(contract, require_idle=False, requested_sharing_path=None, session_instance=None):
    def call(operation, params):
        if session_instance is None: return guest_call(operation, params)
        return guest_call(operation, params, session_instance)
    caps = call('capabilities', {})
    if caps.get('major') != 1 or caps.get('minor', -1) < contract['guest_protocol_minor']:
        raise ValueError('Guest protocol negotiation failed')
    available = caps.get('capabilities')
    if not isinstance(available, list) or not all(isinstance(cap, str) for cap in available):
        raise ValueError('Invalid Guest capability list')
    if not set(contract['required_capabilities']).issubset(available):
        raise ValueError('Guest lacks a required capability')
    params = dict(contract=contract, require_idle=require_idle)
    expected_path = contract['sharing_path']
    if requested_sharing_path is not None:
        if requested_sharing_path != 'linear-v1':
            raise ValueError('Unsupported optional Guest sharing path')
        if 'graphics.linear-preflight-v1' not in available:
            raise ValueError('Guest does not support explicit LINEAR preflight')
        params['requested_sharing_path'] = requested_sharing_path
        expected_path = requested_sharing_path
    result = call('desktop.preflight', params)
    if (result.get('compatible') is not True or result.get('guest_interface_abi') != 1 or
            result.get('guest_profile_sha256') != contract['guest_profile_sha256'] or
            result.get('sharing_path') != expected_path):
        raise ValueError('Guest preflight did not acknowledge the release contract')
    return result


def verify_components(distribution, compatibility):
    if not isinstance(compatibility, dict):
        raise ValueError('Missing admitted desktop compatibility plan')
    if compatibility['source'] == 'release-declaration':
        return protocol_preflight(compatibility['contract'])
    contract = compatibility['contract']
    consumer_name = {'wsx-omarchy': 'opaque-consumer', 'wsx-cachyos-plasma': 'kwin-opaque'}[distribution]
    runtime = Path('/usr/lib/wsx/runtime')
    manifests = {}
    for name in ('mesa', 'venus', consumer_name):
        directory = runtime / name
        raw = trusted_bytes(directory / 'manifest.json')
        manifest = json.loads(raw)
        versions = compatibility.get('legacy_versions') or {}
        if name in versions and manifest.get('version') != versions[name]:
            raise ValueError('Guest component differs from the admitted legacy closure: ' + name)
        if not manifest.get('files'):
            raise ValueError('Empty Guest component closure')
        for relative, expected in manifest['files'].items():
            path = (directory / relative).resolve(strict=True)
            if not path.is_relative_to(directory) or not path.is_file():
                raise RuntimeError('Guest component path escapes bundle')
            with path.open('rb') as stream:
                actual = hashlib.file_digest(stream, 'sha256').hexdigest()
            if actual != expected:
                raise RuntimeError('Guest component hash mismatch: ' + name + '/' + relative)
        manifests[name] = manifest
    mesa, venus, consumer = (manifests[n] for n in ('mesa', 'venus', consumer_name))
    if (not mesa.get('opaque_consumer') or not mesa.get('opaque_gbm_export') or
            mesa.get('presentation_staging_policy') or not venus.get('opaque_wsi') or
            not venus.get('opaque_import') or consumer.get('abi') != 2):
        raise RuntimeError('Guest components lack the declared opaque-v1 / ABI 2 capabilities')
    if contract['guest_admission'] == 'legacy-package-lock-v1' and compatibility['source'] == 'legacy-component-closure':
        if consumer_name == 'kwin-opaque':
            if not {'bin/kwin_wayland', 'lib/libkwin.so.6'}.issubset(consumer['files']):
                raise ValueError('Incomplete private KWin closure')
            legacy_plasma_admission()
    else:
        raise ValueError('Unsupported Guest admission contract')
    return {name: dict(version=value.get('version'), files_verified=len(value['files'])) for name, value in manifests.items()}


def preflight(root, kernel, previous=None, distribution='wsx-omarchy', compatibility=None,
              requested_sharing_path=None, module_manifest_sha256=None, session_instance=None):
    if os.geteuid() != 0:
        raise RuntimeError('Root control channel required')
    if requested_sharing_path is not None and (requested_sharing_path != 'linear-v1' or
            not compatibility or compatibility.get('source') != 'release-declaration'):
        raise ValueError('Optional Guest sharing requires a declared protocol and explicit LINEAR path')
    checker = runpy.run_path(str(root / 'module-reuse.py'))['check']
    modules = checker(root, previous) if module_manifest_sha256 is None else checker(root, previous, manifest_digest=module_manifest_sha256)
    if compatibility and compatibility['source'] == 'release-declaration':
        options = dict(require_idle=True, requested_sharing_path=requested_sharing_path)
        if session_instance is not None: options['session_instance'] = session_instance
        components = protocol_preflight(compatibility['contract'], **options)
        initialized = components['account']
        uid = initialized['uid']
        sharing_path = components['sharing_path']
    else:
        if Path('/run/seatd.sock').exists():
            raise RuntimeError('Existing Guest seat owner must not be replaced')
        initialized = json.loads(trusted_bytes('/var/lib/wsx/initialized.json', 65536))
        account = pwd.getpwnam(initialized['user'])
        if account.pw_uid == 0:
            raise RuntimeError('Root desktop forbidden')
        uid = account.pw_uid
        components = verify_components(distribution, compatibility)
        sharing_path = 'opaque-v1'
    module_directory = root / 'module-group' if (root / 'module-group').is_dir() else root / 'guest'
    if module_manifest_sha256 is not None:
        audit = screen_module_group(root, kernel, module_manifest_sha256)['kernel']
    else:
        audit = runpy.run_path(str(root / 'kernel-exports.py'))['audit'](kernel, module_directory)
    if not audit['symbol_crc_compatibility_admitted']:
        print(json.dumps(audit), file=sys.stderr)
        raise RuntimeError('Kernel symbol CRC admission failed')
    return dict(kernel=audit, initialized=initialized, uid=uid,
                manifests=components, sharing_path=sharing_path, existing_owner=False, modules=modules)


def guest_prepare_command(instance, protocol_major):
    """Use the exact session pin for v2; never accept a caller-supplied path."""
    if type(protocol_major) is not int or protocol_major not in (1, 2):
        raise ValueError('Unsupported Guest protocol major')
    if protocol_major == 1:
        return ['/usr/bin/wsx-guest', 'prepare']
    identity = uuid.UUID(str(instance))
    if not identity.int: raise ValueError('Invalid session identity')
    return ['/usr/bin/python3', '-I', '/usr/lib/wsx/bootstrap-v2.py', 'dispatch',
            'prepare', '--instance', str(identity)]


def prepare(root, profile, instance, previous=None, sharing_path='opaque-v1', module_manifest_sha256=None, guest_protocol_major=1, audio=None):
    instance = uuid.UUID(instance.strip('{}'))
    if not instance.int:
        raise ValueError('Nonzero host instance required')
    guest_command = guest_prepare_command(instance, guest_protocol_major)
    if sharing_path not in ('opaque-v1', 'linear-v1') or (
            sharing_path == 'linear-v1' and profile.get('guest_protocol') is not True):
        raise ValueError('Guest sharing path requires an admitted protocol')
    # Recheck actual identities after native startup; do not unload at this stage.
    checker = runpy.run_path(str(root / 'module-reuse.py'))['check']
    modules = checker(root, previous) if module_manifest_sha256 is None else checker(root, previous, manifest_digest=module_manifest_sha256)
    if modules['reload_required']:
        raise RuntimeError('Previous Owner must be recycled before native startup')
    module_directory = root / 'module-group' if (root / 'module-group').is_dir() else root / 'guest'
    for name, file in MODULES[:-1]:
        if name not in modules['loaded']:
            run(['/usr/bin/insmod', str(module_directory / file)])
    deadline = time.monotonic() + 60
    while True:
        device = find_device(instance)
        if device:
            break
        if time.monotonic() >= deadline:
            raise RuntimeError('Actual host-owned PCI function did not appear')
        time.sleep(0.2)
    if (int((device / 'vendor').read_text(), 16) != profile['vendor_id'] or
            int((device / 'device').read_text(), 16) != profile['device_id']):
        raise RuntimeError('PCI identity differs from the retained host profile')
    if (device / 'driver').exists():
        raise RuntimeError('Actual PCI function already has a driver')
    if b'wsx_owner_configuration_abi=2\0' in (module_directory / 'wsx_virtio_msg_pci.ko').read_bytes():
        if 'wsx_virtio_msg_pci' not in modules['loaded']:
            run(['/usr/bin/insmod', str(module_directory / 'wsx_virtio_msg_pci.ko')])
        driver = Path('/sys/bus/pci/drivers/wsx-virtio-msg-pci')
        if (driver/'configuration_abi').read_text().strip() != '2':
            raise RuntimeError('Expected Owner configuration ABI 2')
        (driver/'configure').write_text(f"{device.name} {instance} {profile['port']} 0 {profile['aperture_size']} {profile['device_number']}\n")
    else:
        run(['/usr/bin/insmod', str(module_directory / 'wsx_virtio_msg_pci.ko'),
         'pci_bdf=' + device.name, 'host_instance=' + str(instance),
         'service_port=' + str(profile['port']), 'aperture_bar=0',
         'aperture_size=' + str(profile['aperture_size']),
         'device_number=' + str(profile['device_number'])])
    (device / 'driver_override').write_text('wsx-virtio-msg-pci\n')
    Path('/sys/bus/pci/drivers_probe').write_text(device.name + '\n')
    if (device / 'driver').resolve(strict=True).name != 'wsx-virtio-msg-pci':
        raise RuntimeError('Actual Guest PCI owner binding failed')
    if (device/'configuration').exists():
        expected=f"{instance} {profile['port']} 0 {profile['aperture_size']} {profile['device_number']}"
        if (device/'configuration').read_text().strip() != expected:
            raise RuntimeError('Bound per-device configuration differs from the requested Owner')
    cards = [str(Path('/dev/dri') / c.name) for c in Path('/sys/class/drm').glob('card[0-9]*')
             if c.name[4:].isdigit() and device.resolve() in c.resolve().parents]
    if len(cards) != 1:
        raise RuntimeError('Expected one DRM card under the actual PCI owner')
    spec = dict(guest_interface_abi=1, instance_id=str(instance), drm_node=cards[0],
                input_port=profile['port'] + 1, sharing_path=sharing_path)
    if audio and audio.get('status') == 'ready':
        spec.update(pulse_server=audio['server'], pulse_client_config=audio['client_config'])
    try:
        run(guest_command, input=json.dumps(spec))
    except subprocess.CalledProcessError as error:
        detail = (error.stderr or error.stdout or '').strip()
        if detail:
            print(detail[-4096:], file=sys.stderr)
        raise
    access = run(['/usr/bin/python3', '-I', str(root / 'drm-access.py'),
                  '--instance', str(instance)])
    return dict(prepared=True, instance=str(instance), device=device.name,
                access=json.loads(access.stdout), sharing_path=sharing_path, gpu_started=False)


def observed_kernel_identity(root, notes_path=Path('/sys/kernel/notes'), kernel_image=None):
    """Read the live kernel identity without opening a device or loading modules."""
    with notes_path.open('rb') as stream:
        notes = stream.read(1024 * 1024 + 1)
    if not notes or len(notes) > 1024 * 1024:
        raise ValueError('Invalid running kernel notes')
    extractor = runpy.run_path(str(root / 'kernel-exports.py'))
    identity = extractor['build_id'](notes)
    if not 8 <= len(identity) <= 128:
        raise ValueError('Invalid running kernel build ID')
    result = {'kernel_build_id': identity, 'kernel_release': os.uname().release,
              'runtime_admitted': False}
    if kernel_image is not None:
        symbols, verified_id, _ = extractor['exports'](extractor['bounded'](kernel_image), notes)
        if verified_id != identity:
            raise ValueError('Kernel image differs from running kernel')
        result['kernel_crc_identity'] = extractor['crc_identity'](symbols)
    return result


def screen_module_group(root, kernel, manifest_sha256):
    """Verify one staged group and inspect its CRCs without changing kernel state."""
    if os.geteuid() != 0:
        raise RuntimeError('Root control channel required')
    directory = root / 'module-group'
    group_tools = runpy.run_path(str(root / 'module-group.py'))
    group = group_tools['verify'](
        directory, manifest_sha256, root / 'module-reuse.py')
    audit = runpy.run_path(str(root / 'kernel-exports.py'))['audit'](kernel, directory)
    reuse = runpy.run_path(str(root / 'module-reuse.py'))
    loaded = reuse['inspect_loaded'](root, directory)
    if loaded and len(loaded) != len(reuse['MODULES']):
        raise RuntimeError('Independent module group is only partially loaded')
    if loaded:
        group_tools['verify_loaded'](group, reuse['SYS'])
    return {'manifest_sha256': group['manifest_sha256'], 'kernel': audit,
            'loaded_module_count': len(loaded),
            'runtime_admitted': False,
            'scope': 'Read-only CRC and loaded module identity screening; loader and component admission are separate'}


if __name__ == '__main__':
    request = json.load(sys.stdin)
    root = Path(globals().get('__file__', '.')).resolve().parent
    if request['action'] == 'preflight':
        result = preflight(root, request['kernel'], request.get('previous'), request.get('distribution', 'wsx-omarchy'), request.get('compatibility'), request.get('requested_sharing_path'), request.get('module_manifest_sha256'), request.get('session_instance'))
    elif request['action'] == 'kernel-identity':
        result = observed_kernel_identity(root, kernel_image=request.get('kernel'))
    elif request['action'] == 'screen-module-group':
        result = screen_module_group(root, request['kernel'], request['module_manifest_sha256'])
    elif request['action'] == 'audit-default-modules':
        if os.geteuid() != 0:
            raise RuntimeError('Root control channel required')
        directory = root / 'guest'
        expected = ('drm_kms_helper.ko', 'virtio_dma_buf.ko', 'virtio-gpu.ko',
                    'virtio_msg.ko', 'virtio_msg_vsock.ko', 'wsx_virtio_msg_pci.ko')
        if not all((directory / name).exists() for name in expected):
            result = {'available': False, 'runtime_admitted': False}
        else:
            result = {'available': True, 'kernel': runpy.run_path(str(root / 'kernel-exports.py'))['audit'](
                request['kernel'], directory), 'runtime_admitted': False}
    elif request['action'] == 'compatibility':
        result = verify_components(request['distribution'], request['compatibility'])
    elif request['action'] == 'prepare':
        result = prepare(root, json.loads((root / 'wsx-runtime.json').read_text()), request['instance'], request.get('previous'), request.get('sharing_path', 'opaque-v1'), request.get('module_manifest_sha256'), request.get('guest_protocol_major', 1), request.get('audio'))
    elif request['action'] == 'recycle':
        result = runpy.run_path(str(root / 'module-reuse.py'))['recycle'](root, request.get('previous'), request.get('allow_reload', False), request.get('module_manifest_sha256'))
    else:
        raise ValueError('Unsupported control action')
    print(json.dumps(result))
