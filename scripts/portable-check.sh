#!/usr/bin/env bash
# Supplemental local checks, never a substitute for Windows/MSVC acceptance.
set -euo pipefail
root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
cargo test --manifest-path "$root/Cargo.toml" --locked
cargo build --manifest-path "$root/Cargo.toml" --locked
