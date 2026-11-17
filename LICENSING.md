# Licensing scope

On 2026-11-16 the owner selected Business Source License 1.1 for original WSX
repository material. The terms, noncommercial Additional Use Grant, four-year
Change Date and Apache-2.0 Change License are in [LICENSE](LICENSE). Material
under separate third-party or earlier grants retains its applicable terms.
The prior PolyForm license text is preserved in
[licenses/historical](licenses/historical/PolyForm-Noncommercial-1.0.0.txt);
changing current terms does not revoke earlier grants.

BSL 1.1 is source-available and is not an OSI-approved open-source license.

## Scope and exceptions

| Material | License and boundary |
| --- | --- |
| Original `core/` material | Business Source License 1.1, subject to [LICENSE](LICENSE); separately licensed and upstream files retain their own terms. |
| Original `host/`, `abi-sdk/`, `include/`, tests, resources, scripts and documentation | Business Source License 1.1, except for upstream material below or a more specific existing notice. This includes the SDK; no separate permissive SDK grant is implied. |
| `core/data/cldr47-windows-zones.tsv` | Unicode-3.0; retain `core/data/UNICODE-LICENSE.txt`, including when the data is embedded in the public PE. |
| `integration/graphics-lifetime/` | MIT for the project's source, helpers and patch contributions, following the existing MIT reference adapters; see `integration/LICENSE`. Upstream material in generated patches or outputs keeps its existing notices and license. |
| `integration/graphics-identity/display-inventory.c` | MIT, with the original prototype notice retained; see `integration/LICENSE` and `integration/graphics-identity/PROVENANCE.md`. The Vulkan identity adaptation follows the repository's original-code terms and uses the existing Khronos headers under their Apache-2.0 license. |
| `kernel-modules/reference-6.18/` | Separate kernel module source inputs; individual files retain their own upstream SPDX identifiers, copyright notices and licenses. See `kernel-modules/README.md` and its retained license texts. The root BSL license does not replace those terms. |
| `kernel-modules/build-inputs/` | Build configuration and dependency lock evidence; preserve any applicable notices and verify third-party provenance separately. No new license is assigned by this directory boundary. |
| Cargo dependencies | Their own licenses, listed with exact versions in `THIRD-PARTY-NOTICES.md`. The root license does not relicense dependencies or restrict rights granted directly by upstream authors. |
| Downloaded or generated graphics bundles, Linux modules, kernel and rootfs images | Separate works and component-specific licenses, not a blanket BSL grant. A hash receipt establishes identity, not licensing compliance. |
| `store-edition`, Tauri and WebView2 | Outside this repository's grant. Pro remains proprietary. This change grants no new Pro license and accepts no Microsoft redistribution terms. |

The SDK license does not grant rights to proprietary Pro code. WGY's selection
of a license for WGY-owned material does not transfer third-party copyrights or
grant rights that WGY does not hold. Existing third-party grants remain intact.
On 2026-11-17 the rights holder confirmed that project-owned `core/guest/` code
adapted from WSX prototypes or `wsx-rootfs` is owned by WGY or authorized for
publication under its existing applicable terms. This does not alter existing
third-party or earlier grants.

## Compatibility and separation

Retain upstream license texts and copyright notices even where a permissive
dependency can be used in the public executable. Patches intended for an upstream
component must use that component's compatible terms, not the root noncommercial
restriction. Keep such patches in a separately licensed integration boundary.

A directory, Rust module, C ABI, DLL or process boundary alone is not proof of
license compatibility. In particular, do not combine GPL-covered code with the
BSL-covered public executable and assume a folder-level exception resolves
the restriction. Evaluate the actual combined work, linking, communication and
redistribution obligations; replace an incompatible dependency or distribute a
genuinely separate component under its required terms. LGPL, MPL and other
licenses require their own analysis rather than a generic GPL rule.

References: [Business Source License 1.1](https://mariadb.com/bsl11/)
and the [GNU FAQ on aggregation and combination](https://www.gnu.org/licenses/gpl-faq.en.html#MereAggregation).

## Review evidence and remaining scope

The historical review verified 23 registry archive checksums against Cargo.lock, inspected
their package license declarations and bundled notices, and retained exact notice
bytes. `ruzstd` required its missing text from the archive's recorded upstream
commit. Additional `serde_json` lexical attribution was retained. No GPL-family
license was found in this public Cargo dependency set. This does not establish
the composition or licensing of external graphics binaries.

The seven prototype inputs in `docs/provenance.json` matched their recorded
source hashes during the original review, including the then-current icon. The
icon was subsequently replaced. On 2026-11-08 WGY confirmed that the then-current
replacement icon artwork was original; its SHA-256 was
`0b59d21df135aa31aba1addefb07274a067d11ba29218d3dd35f4d679b6b36f4`.
On 2026-11-15 the user supplied updated artwork; the current
`host/resources/icon.ico` SHA-256 is
`7059579aaa1db2c56e3914bc0e5db97f8472fbe8ab61ede28c9838f087d6712b`,
matching the copies in wsx-host and store-edition. The original receipt describes the
earlier prototype icon, not either replacement. On 2026-11-17 WGY confirmed that
the current artwork identified by this SHA-256 is WGY's original work. Hashes
establish byte identity; this confirmation is specific to the current icon and
does not establish authorship or third-party rights for other assets.

The read-only graphics reference declares MIT for OpenVMM and virglrenderer,
BSD-3-Clause for Rutabaga, and MIT on the migrated Guest retirement adapter.
These top-level observations are not a complete file-by-file or transitive audit.
Before distributing a graphics bundle, retain its full pinned source closure,
per-file licenses, notices and required corresponding source, including kernel
module obligations. Use `docs/PROTOTYPE-MIGRATION-CHECKLIST.md` and actual build
inventories; do not treat historical runtime acceptance as license clearance.

The Rust 1.95.0 standard library documentation and its bundled third-party
notices are retained in [licenses/toolchain/rust-1.95.0](licenses/toolchain/rust-1.95.0/).
This is a broad toolchain notice inventory, not a list of libraries proven to be
linked. Compiler runtime inputs and any Microsoft redistributable still need
review against the actual toolchain and package contents. The Cargo inventory
does not cover them. No complete product redistribution audit is claimed.

## Maintaining notices

Run `python3 scripts/check-licenses.py` (Python 3.11 or later) after changes to
Cargo.lock or licensing files. It checks the recorded lockfile, package set,
manifest declarations and notice hashes offline. It does not infer legal
compatibility from a license identifier. When dependencies change, inspect the
exact new source archives and refresh the inventory and notices together.

Distribute LICENSE, NOTICE, this scope document, THIRD-PARTY-NOTICES.md, the
`licenses/` directory, `core/data/UNICODE-LICENSE.txt` and `integration/LICENSE`
alongside the public executable. The existing Windows build script copies these
files without modifying the single PE. Preserve relative paths so notice links
remain valid. Copy the applicable license and notice files when distributing the
SDK or an integration patch separately. This documentation change neither
publishes a release nor stages an installed deployment.

The historical PCI Owner experiment was moved to the prototype repository with
its GPL-2.0-or-later notice and license text intact. The independently maintained
kernel module sources under `kernel-modules/` retain their own upstream notices.
