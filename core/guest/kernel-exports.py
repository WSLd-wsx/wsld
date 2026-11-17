"""Read x64 WSL kernel exports without loading modules or trusting headers.

Supports ELF64 and the gzip-compressed bzImage used by the pinned WSL build.
Other formats fail closed. Build ID must match the running Guest's kernel notes.
CRC admission is an ABI check, not proof of runtime GPU behavior.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import os
import zlib

LIMIT = 512 * 1024 * 1024


def bounded(path):
    with Path(path).open('rb') as stream:
        data = stream.read(LIMIT + 1)
    if len(data) > LIMIT:
        raise ValueError('Kernel input exceeds size limit')
    return data


def build_id(notes):
    found = []
    offset = 0
    while offset < len(notes):
        names, size, kind = struct.unpack_from('<III', notes, offset)
        offset += 12
        name = notes[offset:offset + names]
        offset += (names + 3) & ~3
        value = notes[offset:offset + size]
        offset += (size + 3) & ~3
        if offset > len(notes):
            raise ValueError('Truncated kernel note')
        if name == b'GNU\0' and kind == 3:
            found.append(value.hex())
    if len(found) != 1 or not found[0]:
        raise ValueError('Expected one GNU kernel build ID')
    return found[0]


def elf_image(image):
    if image.startswith(b'\x7fELF'):
        return image
    # Do not execute extract-vmlinux or assume an installed kernel is the booted one.
    offset = image.find(b'\x1f\x8b\x08')
    attempts = 0
    while offset >= 0 and attempts < 64:
        decoder = zlib.decompressobj(31)
        try:
            data = decoder.decompress(image[offset:], LIMIT + 1)
            if len(data) <= LIMIT and decoder.eof and data.startswith(b'\x7fELF'):
                return data
        except zlib.error:
            pass
        attempts += 1
        offset = image.find(b'\x1f\x8b\x08', offset + 1)
    raise ValueError('Unsupported or invalid WSL kernel compression')


def embedded_notes(image):
    """Read an image's own notes; this does not identify a running kernel."""
    data = elf_image(image)
    if data[:6] != b'\x7fELF\x02\x01' or struct.unpack_from('<H', data, 18)[0] != 62:
        raise ValueError('Expected little-endian x86-64 ELF kernel')
    table = struct.unpack_from('<Q', data, 40)[0]
    entry_size, count, names_index = struct.unpack_from('<HHH', data, 58)
    if entry_size != 64 or not 0 < names_index < count or count > 4096 or table + count * 64 > len(data):
        raise ValueError('Invalid ELF section table')
    sections = [struct.unpack_from('<IIQQQQIIQQ', data, table + i * 64) for i in range(count)]
    def contents(section):
        offset, size = section[4:6]
        if offset + size > len(data):
            raise ValueError('ELF section exceeds file')
        return data[offset:offset + size]
    names = contents(sections[names_index])
    found = []
    for section in sections:
        start = section[0]
        if start >= len(names):
            raise ValueError('ELF section name exceeds string table')
        end = names.find(b'\0', start)
        if end < 0:
            raise ValueError('Unterminated ELF section name')
        if names[start:end] == b'.notes':
            found.append(contents(section))
    if len(found) != 1:
        raise ValueError('Expected one kernel notes section')
    build_id(found[0])
    return found[0]


def exports_detailed(image, running_notes):
    data = elf_image(image)
    if data[:6] != b'\x7fELF\x02\x01' or struct.unpack_from('<H', data, 18)[0] != 62:
        raise ValueError('Expected little-endian x86-64 ELF kernel')
    table = struct.unpack_from('<Q', data, 40)[0]
    entry_size, count, names_index = struct.unpack_from('<HHH', data, 58)
    if entry_size != 64 or not 0 < names_index < count or count > 4096:
        raise ValueError('Invalid ELF section table')
    sections = [struct.unpack_from('<IIQQQQIIQQ', data, table + i * 64) for i in range(count)]

    def contents(section):
        offset, size = section[4:6]
        if offset + size > len(data):
            raise ValueError('ELF section exceeds file')
        return data[offset:offset + size]

    def cstring(blob, start):
        if start < 0 or start >= len(blob):
            raise ValueError('ELF string offset out of bounds')
        end = blob.index(0, start)
        return blob[start:end].decode('ascii')

    names = contents(sections[names_index])
    by_name = {cstring(names, s[0]): s for s in sections}
    actual_id = build_id(contents(by_name['.notes']))
    if actual_id != build_id(running_notes):
        raise ValueError('Kernel image build ID differs from the running kernel')
    strings = by_name['__ksymtab_strings']
    string_data = contents(strings)
    result = {}
    for suffix in ('', '_gpl'):
        symbols = by_name['__ksymtab' + suffix]
        rows = contents(symbols)
        crcs = contents(by_name['__kcrctab' + suffix])
        if len(rows) % 12 or len(crcs) != len(rows) // 3:
            raise ValueError('Unsupported kernel export/CRC table layout')
        for offset in range(0, len(rows), 12):
            relative = struct.unpack_from('<i', rows, offset + 4)[0]
            name = cstring(string_data, symbols[3] + offset + 4 + relative - strings[3])
            crc = struct.unpack_from('<I', crcs, offset // 3)[0]
            if name in result:
                raise ValueError('Duplicate kernel export')
            namespace_offset = struct.unpack_from('<i', rows, offset + 8)[0]
            namespace = (cstring(string_data, symbols[3] + offset + 8 + namespace_offset - strings[3])
                         if namespace_offset else '')
            result[name] = {'crc': crc, 'export': 'EXPORT_SYMBOL_GPL' if suffix else 'EXPORT_SYMBOL',
                            'namespace': namespace}
    return result, actual_id, hashlib.sha256(data).hexdigest()


def exports(image, running_notes):
    details, actual_id, elf_hash = exports_detailed(image, running_notes)
    return {name: item['crc'] for name, item in details.items()}, actual_id, elf_hash


def crc_identity(symbols):
    """Hash all exported symbol CRCs using the WSX kernel-crc-v1 format."""
    if not isinstance(symbols, dict) or not symbols or len(symbols) > 1_000_000:
        raise ValueError('Invalid kernel symbol table')
    digest = hashlib.sha256(b'WSX-KERNEL-CRC-V1\n')
    for name, crc in sorted(symbols.items()):
        if (not isinstance(name, str) or not name or len(name) > 512
                or not name.isascii() or any(ord(c) < 33 or ord(c) > 126 for c in name)
                or not isinstance(crc, int) or isinstance(crc, bool) or not 0 <= crc <= 0xffffffff):
            raise ValueError('Invalid kernel symbol CRC entry')
        digest.update(name.encode('ascii') + b'\t' + f'{crc:08x}'.encode('ascii') + b'\n')
    return digest.hexdigest()


def module_entries(path, option):
    result = subprocess.run(['/usr/bin/modprobe', option, str(path)], check=True,
                            capture_output=True, text=True, timeout=20)
    entries = {}
    for line in result.stdout.splitlines():
        crc, name = line.split()
        if name in entries:
            raise ValueError('Duplicate module symbol')
        entries[name] = int(crc, 16)
    return entries


def crc_compatible(records):
    return len(records) == 6 and all(
        r['imports'] > 0 and r['has_module_layout'] and not r['unobserved'] and not r['mismatches']
        for r in records.values())


def checked_module_files(candidate_directory):
    expected = {'drm_kms_helper.ko', 'virtio_dma_buf.ko', 'virtio-gpu.ko',
                'virtio_msg.ko', 'virtio_msg_vsock.ko', 'wsx_virtio_msg_pci.ko'}
    modules = sorted(Path(candidate_directory).glob('*.ko'))
    if {module.name for module in modules} != expected or any(module.is_symlink() for module in modules):
        raise ValueError('Expected the exact six regular module files')
    if any(not module.is_file() for module in modules):
        raise ValueError('Expected the exact six regular module files')
    return modules


def audit(image_path, candidate_directory):
    image = bounded(image_path)
    kernel, identity, elf_hash = exports(image, bounded('/sys/kernel/notes'))
    modules = checked_module_files(candidate_directory)
    provided = dict(kernel)
    vermagic_by_module = {}
    for module in modules:
        vermagic = subprocess.check_output(['/usr/bin/modinfo', '-F', 'vermagic', str(module)], text=True).strip()
        if not vermagic.split() or vermagic.split()[0].split('.')[0] != os.uname().release.split('.')[0]:
            raise ValueError('Module kernel major differs from running kernel')
        vermagic_by_module[module.name] = vermagic
        # Reuse the verified WSL major-version family. Full release spelling is
        # not the admission gate: actual export CRCs and the kernel loader are.
        # Never use force-vermagic or force-modversion flags.
        # modprobe reports ENODATA for modules that export no symbols.
        result = subprocess.run(['/usr/bin/modprobe', '--show-exports', str(module)], capture_output=True, text=True, timeout=20)
        if result.returncode and 'No data available' not in result.stderr:
            raise RuntimeError(result.stderr)
        for line in result.stdout.splitlines():
            crc, name = line.split()
            if name in provided:
                raise ValueError('Duplicate kernel/package export: ' + name)
            provided[name] = int(crc, 16)
    records = {}
    for module in modules:
        imports = module_entries(module, '--show-modversions')
        missing = sorted(set(imports) - set(provided))
        mismatches = [{'symbol': n, 'required': hex(c), 'actual': hex(provided[n])}
                      for n, c in imports.items() if n in provided and c != provided[n]]
        records[module.name] = dict(imports=len(imports), has_module_layout='module_layout' in imports,
                                   vermagic=vermagic_by_module[module.name],
                                   unobserved=missing, mismatches=mismatches,
                                   sha256=hashlib.sha256(bounded(module)).hexdigest())
    admitted = crc_compatible(records)
    return dict(schema_version=1, kernel=os.uname().release, kernel_build_id=identity,
                kernel_image_sha256=hashlib.sha256(image).hexdigest(), vmlinux_sha256=elf_hash,
                kernel_export_count=len(kernel), kernel_crc_identity=crc_identity(kernel), modules=records,
                symbol_crc_compatibility_admitted=admitted,
                scope='Running-kernel build-ID and all module import CRCs; not GPU behavior or retirement')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('kernel_image')
    parser.add_argument('candidate_directory')
    args = parser.parse_args()
    result = audit(args.kernel_image, args.candidate_directory)
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result['symbol_crc_compatibility_admitted'] else 1)
