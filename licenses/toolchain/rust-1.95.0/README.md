# Rust 1.95.0 standard library notices

`COPYRIGHT-library.html` and the `licenses/` directory are copied without
modification from the documentation shipped with the Rust 1.95.0 toolchain.
The inspected toolchain reports Rust commit
`59807616e1fa2540724bfbac14d7976d7e4a3860`; the SHA-256 of
`COPYRIGHT-library.html` is
`90567e2718bf7fd65a71a3a43c5596488e80e5f51ed02bfea6fec54458b5f3d1`.
The original distribution inspected was the x86_64-unknown-linux-gnu host
toolchain. Its copyright document and license directory were also compared
byte for byte with the locally available Windows MSVC toolchain documentation
and matched. The bundled document describes the standard library and its
third-party source dependencies; it is a broad notice inventory and does not
assert that every listed dependency is linked into a particular WSX binary.

The WSX community Windows build pins Rust 1.95.0. This notice snapshot does
not identify all link-time inputs or provide notices for Microsoft tools,
Windows components, the compiler itself, or separately distributed runtimes.
Review those inputs and their applicable distribution terms independently.
