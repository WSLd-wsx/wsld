"""Static PE inspection, not Windows runtime evidence."""
import hashlib,json,struct,sys
from pathlib import Path
def inspect_pe(path):
    data = path.read_bytes()
    u16 = lambda offset: struct.unpack_from('<H', data, offset)[0]
    u32 = lambda offset: struct.unpack_from('<I', data, offset)[0]
    pe = u32(0x3c)
    assert data[pe:pe + 4] == b'PE\0\0'
    opt = pe + 24
    directories = opt + (112 if u16(opt) == 0x20b else 96)
    sections = opt + u16(pe + 20)

    def offset(rva):
        for i in range(u16(pe + 6)):
            section = sections + 40 * i
            size, va, rawsize, raw = struct.unpack_from('<IIII', data, section + 8)
            if va <= rva < va + max(size, rawsize):
                return raw + rva - va
        if rva < u32(opt + 60):
            return rva
        raise ValueError(f'Invalid RVA {rva:x}')

    def string(rva):
        start = offset(rva)
        return data[start:data.index(b'\0', start)].decode('ascii')

    imports = []
    import_rva = u32(directories + 8)
    if import_rva:
        current = offset(import_rva)
        while any(data[current:current + 20]):
            imports.append(string(u32(current + 12)))
            current += 20
    exports = []
    export_rva = u32(directories)
    if export_rva:
        table = offset(export_rva)
        names = offset(u32(table + 32))
        exports = [string(u32(names + 4 * i)) for i in range(u32(table + 24))]
    return dict(machine=hex(u16(pe + 4)), subsystem=u16(opt + 68),
                imports=imports, exports=exports,
                delay_import_rva=u32(directories + 13 * 8),
                sha256=hashlib.sha256(data).hexdigest(), size=len(data))


if __name__ == "__main__":
    print(json.dumps({Path(p).name:inspect_pe(Path(p)) for p in sys.argv[1:]},indent=2))
