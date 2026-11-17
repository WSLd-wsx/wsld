# Migration inventory

Original bytes, relative paths, SHA-256 and Git status were preserved outside
both repositories in `.handoff-backups/20261102T131940Z/manifest.json` at the
workspace root. The private source backup is outside the public repository.
Both repositories had no commits on main; all twelve original files were untracked.
No existing history or unrelated changes were discarded.

| Original | Replacement |
| --- | --- |
| wsld/src/main.c | host/src/main.rs, cli.rs, windows.rs |
| wsld/src/core.c, core.h | core/src modules and host/src/bridge.rs |
| wsld/include/wsx_abi.h | abi-sdk authoritative v2 SDK; public include forwards to it |
| wsld/docs/ABI.md | explicit incompatible v2 lifecycle contract |
| wsld/tests/test_contract.py | behavioral SDK/layout and process acceptance |
| wsld/build.cmd | native Cargo/MSVC build and same-PE packaging |
| store-edition/src/pro.c | Rust dll-runtime/Core client and Tauri manager |
| store-edition/include/wsx_abi.h | controlled SDK include, no independent declaration |
| store-edition/build.cmd | independent locked Rust/Tauri build |
| both README.md | Rust/MSVC responsibility and validation status |

The former v1 synchronous execute API, Win32 button UI and post-GUI FreeLibrary
are superseded. Preserve fixed-disk admission, reparse rejection and restricted
DLL search; extend checks to ancestor paths. Prototype task and GUI code is
migration input only, copied with source hashes in provenance.json. The original
probe ABI v1 is also distinct from product v2. No dual DLL-name lookup exists.
