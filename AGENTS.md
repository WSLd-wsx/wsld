# Public shell contributor guide

- `core/`, `host/`, and `gui/` contain the public desktop orchestration, Windows launcher, and shared frontend. `abi-sdk/` and `include/` define the public integration interfaces.
- `community/`, `scripts/`, and `.github/` contain packaging and build tooling. `kernel-modules/` contains separately distributed kernel sources. Public integration fixtures and tests live in `integration/` and `tests/`.
- Build on Windows with the pinned Rust toolchain, MSVC x64, and the Windows SDK. Run `cargo fmt --all -- --check`, `cargo test --locked --target x86_64-pc-windows-msvc`, and `python3 scripts/check-licenses.py`. See [development](docs/DEVELOPMENT.md) for packaging with `build.cmd`.
- Keep public changes focused. Use English for repository prose and commit messages, preserve license notices, and report the checks run.
- The core Host/Guest transport and rendering components are supplied as precompiled closed-source binaries. Public shell changes must preserve and use their public contract interfaces.
