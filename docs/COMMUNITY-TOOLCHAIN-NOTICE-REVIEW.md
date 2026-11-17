# Community toolchain notice review

The deleted private test release `community-test-44037165202-1` was inspected
on 2026-11-17. Its Windows package SHA-256 was
`50815e68f99f364a6600bcdfcd8df0f4d35dc366938274b4a3a7dbdff540efd6`;
the three entry points were identical PE bytes, SHA-256
`843cfdc38640a3e511068b876ff56e0044bbb922fba598234fbe3062328116cd`.
The release no longer exists. These observations are not evidence for a later
build or an authorization to republish that release.

Static inspection of that PE found Windows and Universal CRT API-set imports;
its PE delay-import directory was empty. The package did not contain a separate
Microsoft VC runtime redistributable DLL or installer. Calling installed system
components is not by itself evidence that their binaries were redistributed.
Static imports do not identify everything linked into a binary, loaded at
runtime, or covered by toolchain distribution terms. The separate Host, Guest,
renderer and their dependencies require their own component-level review.

The repository includes the Rust 1.95.0 standard library copyright and license
documentation under `licenses/toolchain/rust-1.95.0/`, which the community
package copies with its other license materials. Future community builds record
the exact main PE's imports, hash and delay-import directory indicator in
`gui-third-party/community-pe-imports.json`. Inspect this record and the actual
package and toolchain for each release; it is evidence, not license approval.
