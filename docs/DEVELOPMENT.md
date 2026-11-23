# Development and evidence

Core builds do not require sibling repositories. Product packaging requires
explicitly supplied graphics and Guest component inputs; a sibling checkout is
not a substitute for those inputs or a runtime dependency.

Use native Windows Rust 1.95.0, x86_64-pc-windows-msvc, Visual Studio x64 tools
and Windows SDK. `build.cmd -Evidence <unused path> [-Toolchain <Rust directory>]`
requires `-GraphicsReference` (one or more verified bundle directories) for
product packages, and uses an isolated NTFS cache
under TEMP, overridable with WSX_BUILD_CACHE. Core Cargo builds remain independent
of component bundles. Sources
may reside on WSL; Cargo output and registry SQLite must reside on NTFS. Cache
contents are disposable development prerequisites, never deployment dependencies.
Display and Vulkan identity queries are compiled into Core and run through an
isolated invocation of the same product executable.
Both builds use lockfiles. No Node, Python or gh is needed by the product.

Public: cargo test --locked --target x86_64-pc-windows-msvc; build.cmd archives
one PE under three names and rejects differing hashes. Pro builds independently
and consumes ../wsld/abi-sdk at the fixed SDK package version. Its SDK validation
checks the shared authoritative header/source digest. Package the resulting DLL
beside the public EXEs in a fresh local NTFS test directory; the GUI deliberately
rejects UNC paths. Archive logs, PE imports/exports, hashes and screenshots back
into each repository's artifacts directory. Do not run existing deployments.

Acceptance-only code is selected by explicit Cargo features. Keep diagnostic
counting tasks distinct from real operations. Never claim Windows, GUI, WSL or
GPU acceptance based on source assertions or simulated fixtures. Timeouts preserve
processes and logs instead of force-killing them into a passing result.

The public dependency notices and their exact-version inventory are documented in
[licensing scope](../LICENSING.md) and [third-party notices](../THIRD-PARTY-NOTICES.md).
Run `python3 scripts/check-licenses.py` after changing dependencies. The Windows
build copies the public notices beside the three unchanged copies of the PE.
External graphics, toolchain runtime and WebView2 deployment terms still require
their own distribution review before shipping.
Tauri/Wry declare MIT OR Apache-2.0, Tao Apache-2.0, webview2-com MIT in the
inspected pinned baseline. This is not a complete transitive audit or acceptance
of WebView2 redistribution terms. No runtime auto-installer is included.

## Guest protocol regression

See [ADR 013](ADR-013.md) for the current wire boundary and evidence.
`tests/test_guest_boundary.py` checks Core/Native rejection and delegation without
WSL side effects. Rootfs conformance, declaration, staging and schema tests remain
in its independent checkout. They are development checks only; the public Core
checkout still builds independently. Do not test a new protocol by modifying a
running image or reinterpret fixture success as GPU acceptance.

## GUI diagnostics

`wsxdesk.exe` detaches from its console at startup; CLI entry points retain their
normal console and redirection. GUI diagnostic output is discarded by default.
Set `WSX_GUI_LOG` to an absolute writable file path before launching Desk to
append GUI diagnostic output to that file. A
console-subsystem executable can briefly show a console before it detaches.
Startup errors are shown in a message box. WSL inventory captures bounded query
output in memory rather than creating discovery logs at every refresh.

Desktop operation journals and Native Host lifecycle evidence are retained
separately: they support recovery and retirement checks and must not be removed
or disabled as ordinary diagnostic logging. WebView2 controls its own cache.
