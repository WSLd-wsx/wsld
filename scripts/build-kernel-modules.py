#!/usr/bin/env python3
"""Build the pinned WSX six-module source set against a prepared kernel tree."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
from kernel_build_generation import generation
import kernel_symvers
import kernel_target

ROOT = Path(__file__).resolve().parents[1]
SOURCES = ROOT / 'kernel-modules/reference-6.18'
COMPAT_SOURCES = ROOT / 'kernel-modules/compat-drm-client-lock'
COMPAT_FILES = ('drm_file.c', 'drm_client_event.c', 'drm_modeset_helper.c', 'drm_fb_helper.c')
COMMIT = 'a989171929bed4ea681be8d77333845ce1c1732a'
RELEASE = '6.18.58.1-microsoft-standard-WSL2'
CONFIG = '3b40bc0f4c90a824a133d72f7960be923ce5c43c01a52476ac74848c4e8ad5d5'
SYMVERS = '3a318755e6fd473e964b3ea4d7aa6e75395539ecb7fb6f87efa0b8a53d52399c'
MODULES = {'graphics-module': ('drm_kms_helper.ko', 'virtio_dma_buf.ko', 'virtio-gpu.ko'),
           'owner-module': ('virtio_msg.ko', 'virtio_msg_vsock.ko', 'wsx_virtio_msg_pci.ko')}


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def compatible_drm_sources(kernel):
    """Select the retained DRM sources only when all target upstream bytes match."""
    lock_path = COMPAT_SOURCES / 'sources.lock.json'
    if lock_path.is_symlink() or not lock_path.is_file():
        raise ValueError('DRM compatibility source lock is missing or unsafe')
    lock = json.loads(lock_path.read_text())
    if (not isinstance(lock, dict) or set(lock) != {'schema_version', 'upstream_commit', 'files'}
            or lock['schema_version'] != 1 or not re.fullmatch(r'[0-9a-f]{40}', lock['upstream_commit'])
            or not isinstance(lock['files'], dict) or set(lock['files']) != set(COMPAT_FILES)):
        raise ValueError('Invalid DRM compatibility source lock')
    matches = []
    for name in COMPAT_FILES:
        expected = lock['files'][name]
        retained = COMPAT_SOURCES / name
        upstream = kernel / 'drivers/gpu/drm' / name
        if (not isinstance(expected, str) or not re.fullmatch(r'[0-9a-f]{64}', expected)
                or retained.is_symlink() or not retained.is_file() or digest(retained) != expected):
            raise ValueError('DRM compatibility source digest mismatch: ' + name)
        matches.append(not upstream.is_symlink() and upstream.is_file() and digest(upstream) == expected)
    return lock['files'] if all(matches) else {}


def target_drm_sources(kernel):
    """Use the source from the already verified target kernel for DRM helpers."""
    result = {}
    for name in COMPAT_FILES:
        upstream = kernel / 'drivers/gpu/drm' / name
        if upstream.is_symlink() or not upstream.is_file():
            raise ValueError('Target DRM source is missing or unsafe: ' + name)
        result[name] = digest(upstream)
    return result


def adapt_target_kbuild(kernel, output):
    """Use the target kernel's vblank-helper availability for the KMS build."""
    source = kernel / 'drivers/gpu/drm/drm_vblank_helper.c'
    header = kernel / 'include/drm/drm_vblank_helper.h'
    if source.is_symlink() or header.is_symlink():
        raise ValueError('Unsafe target DRM vblank helper')
    if source.is_file() and header.is_file():
        return False
    if source.exists() or header.exists():
        raise ValueError('Incomplete target DRM vblank helper')
    kbuild = output / 'graphics-module/Kbuild'
    contents = kbuild.read_text()
    token = ' kms/drm_vblank_helper.o'
    if contents.count(token) != 1:
        raise ValueError('Unexpected KMS vblank helper build definition')
    kbuild.write_text(contents.replace(token, '', 1))
    return True


def build(kernel, output, compiler_prefix, jobs, kernel_image=None, kernel_notes=None, allow_runner_config=False, target_inspection=None):
    build_generation, _ = generation(ROOT)
    if output.exists():
        raise ValueError('Output directory already exists')
    target = None
    if target_inspection:
        if not allow_runner_config or kernel_image is None or kernel_notes is None:
            raise ValueError('Target builds require diagnostic configuration, image and notes')
        target = kernel_target.identity(kernel, kernel_image, target_inspection)
        prepared = json.loads((kernel / 'wsx-target-prepared.json').read_text())
        expected = dict(target, prepared_config_sha256=digest(kernel / '.config'),
                        kernel_notes_sha256=kernel_target.digest(kernel_notes))
        if prepared != expected:
            raise ValueError('Prepared target evidence does not match current inputs')
    expected_release = target['release'] if target else RELEASE
    if (kernel / 'include/config/kernel.release').read_text().strip() != expected_release:
        raise ValueError('Prepared kernel release is not the pinned reference')
    config_sha = digest(kernel / '.config')
    if config_sha != CONFIG and not allow_runner_config:
        raise ValueError('Prepared kernel config differs from the pinned reference')
    symvers_sha = digest(kernel / 'Module.symvers')
    if (kernel_image is None) != (kernel_notes is None):
        raise ValueError('Both target kernel image and notes are required')
    symvers_evidence = None
    if kernel_image is not None:
        derived, symvers_evidence = kernel_symvers.derive(kernel_image, kernel_notes)
        if derived != (kernel / 'Module.symvers').read_bytes():
            raise ValueError('Module.symvers differs from the build-ID-matched kernel exports')
    elif symvers_sha != SYMVERS:
        raise ValueError('Module.symvers differs from the pinned reference; target kernel evidence is required')
    source_lock = json.loads((SOURCES / 'sources.lock.json').read_text())
    if (not isinstance(source_lock, dict) or source_lock.get('schema_version') != 1
            or source_lock.get('reference_kernel_commit') != COMMIT
            or not isinstance(source_lock.get('files'), dict) or not source_lock['files']):
        raise ValueError('Invalid source lock')
    files = source_lock['files']
    for name, expected in files.items():
        if not isinstance(name, str) or not name or not isinstance(expected, str) or not re.fullmatch(r'[0-9a-f]{64}', expected):
            raise ValueError('Invalid source lock entry')
        relative = Path(name)
        if relative.is_absolute() or not relative.parts or '..' in relative.parts or relative.parts[0] not in MODULES:
            raise ValueError('Unsafe source path')
        file = SOURCES / relative
        if file.is_symlink() or not file.is_file() or digest(file) != expected:
            raise ValueError('Source digest mismatch: ' + name)
    actual_files = set()
    for name in MODULES:
        directory = SOURCES / name
        if directory.is_symlink() or not directory.is_dir():
            raise ValueError('Unsafe module source directory: ' + name)
        for path in directory.rglob('*'):
            if path.is_symlink() or not (path.is_file() or path.is_dir()):
                raise ValueError('Unsafe module source input: ' + str(path))
            if path.is_file():
                actual_files.add(path.relative_to(SOURCES).as_posix())
    if actual_files != set(files):
        raise ValueError('Source lock does not cover the complete module source tree')
    compatible_sources = target_drm_sources(kernel) if target else compatible_drm_sources(kernel)
    compiler = subprocess.check_output([compiler_prefix + 'gcc', '-dumpfullversion'], text=True).strip()
    linker = subprocess.check_output([compiler_prefix + 'ld', '--version'], text=True).splitlines()[0]
    if compiler != '13.2.0' or not linker.endswith(' 2.41'):
        raise ValueError('Compiler or linker does not match the reviewed toolchain')
    output.mkdir(parents=True, exist_ok=False)
    for name in files:
        dest = output / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(SOURCES / name, dest)
        if digest(dest) != files[name]:
            raise ValueError('Copied module source differs from lock: ' + name)
    for name, expected in compatible_sources.items():
        dest = output / 'graphics-module/kms' / name
        source = kernel / 'drivers/gpu/drm' / name if target else COMPAT_SOURCES / name
        if source.is_symlink() or not source.is_file() or digest(source) != expected:
            raise ValueError('Target DRM source changed during compilation: ' + name)
        shutil.copyfile(source, dest)
        if digest(dest) != expected:
            raise ValueError('Copied DRM compatibility source differs from lock: ' + name)
    target_vblank_omitted = adapt_target_kbuild(kernel, output) if target else False
    commands = []
    overrides = [f'{key}={compiler_prefix}{tool}' for key, tool in
                 [('CC', 'gcc'), ('LD', 'ld'), ('AR', 'ar'), ('NM', 'nm'),
                  ('OBJCOPY', 'objcopy'), ('OBJDUMP', 'objdump'), ('STRIP', 'strip')]]
    with (output / 'build.log').open('w') as log:
        for group in MODULES:
            command = ['make', '-C', str(kernel), f'-j{jobs}', 'LOCALVERSION=', *overrides,
                       'KCFLAGS=-Werror', 'M=' + str(output / group), 'modules']
            commands.append(command)
            subprocess.run(command, check=True, stdout=log, stderr=subprocess.STDOUT)
    modules = {}
    result_dir = output / 'modules'
    result_dir.mkdir()
    for group, names in MODULES.items():
        for name in names:
            source = output / group / name
            if not source.is_file() or source.is_symlink():
                raise ValueError('Expected module is missing: ' + name)
            module_output = result_dir / name
            shutil.copyfile(source, module_output)
            modules[name] = {'sha256': digest(module_output), 'bytes': module_output.stat().st_size}
    final_generation, inputs = generation(ROOT)
    if final_generation != build_generation:
        raise ValueError('Module build inputs changed during compilation')
    dependency_file = os.environ.get('WSX_KERNEL_RUNNER_DEPENDENCIES')
    runner_dependencies = json.loads(Path(dependency_file).read_text()) if dependency_file else None
    if runner_dependencies is not None:
        encoded = json.dumps(runner_dependencies, sort_keys=True, separators=(',', ':')).encode()
        if hashlib.sha256(encoded).hexdigest() != inputs.get('@runner-dependencies'):
            raise ValueError('Runner dependency evidence changed during compilation')
    result = {'schema_version': 1, 'kernel_source_commit': target['source_commit'] if target else COMMIT,
              'module_source_reference_commit': COMMIT, 'kernel_release': expected_release,
              'target_source_correspondence_verified': False if target else None,
              'diagnostic_target': target,
              'build_generation': build_generation,
              'build_inputs': inputs,
              'kernel_config_sha256': config_sha, 'kernel_symvers_sha256': symvers_sha,
              'runner_config_unreviewed': allow_runner_config,
              'kernel_symvers_evidence': symvers_evidence,
              'source_lock_sha256': digest(SOURCES / 'sources.lock.json'), 'commands': commands,
              'compatible_drm_source_sha256': compatible_sources,
              'target_vblank_helper_omitted': target_vblank_omitted,
              'compiler_version': compiler, 'linker_version': linker,
              'runner_dependencies': runner_dependencies,
              'modules': modules, 'module_loaded': False, 'symbol_crc_audited': False,
              'runtime_admitted': False, 'release_ready': False}
    (output / 'build-result.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel-tree', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--compiler-prefix', required=True)
    parser.add_argument('--kernel-image', type=Path)
    parser.add_argument('--kernel-notes', type=Path)
    parser.add_argument('--jobs', type=int, default=4)
    parser.add_argument('--allow-runner-config', action='store_true')
    parser.add_argument('--target-inspection', type=Path)
    args = parser.parse_args()
    if not 1 <= args.jobs <= 32 or '\0' in args.compiler_prefix:
        raise ValueError('Invalid build options')
    print(json.dumps(build(args.kernel_tree.resolve(), args.output.absolute(), args.compiler_prefix, args.jobs,
                           args.kernel_image, args.kernel_notes, args.allow_runner_config, args.target_inspection)))


if __name__ == '__main__':
    main()
