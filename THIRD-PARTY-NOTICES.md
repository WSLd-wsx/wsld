# Third-party notices

These components retain their upstream licenses and copyright notices. The
project's Business Source License does not replace or restrict those
grants for the upstream components themselves.

## Public Cargo dependency inventory

The registry packages in the root Cargo.lock were downloaded from
static.crates.io and their archive SHA-256 values matched Cargo.lock. This
inventory includes build-time and target-specific packages; it is deliberately
broader than the Windows x64 PE's linked code. It is not a binary composition
report. Exact sources and notice hashes are in
[the inventory](licenses/dependency-inventory.json).

The table preserves the packages' declared license expressions, including the
legacy `MIT/Apache-2.0` spelling. For distributions made by this project, the MIT
option is used wherever offered, together with any additional required license
(such as Unicode-3.0). All supplied alternative license texts are retained.

| Package | Version | Upstream license declaration | Preserved notices |
| --- | --- | --- | --- |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/block-buffer-0.10.4/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/block-buffer-0.10.4/LICENSE-MIT) |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/cfg-if-1.0.5/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/cfg-if-1.0.5/LICENSE-MIT) |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/cpufeatures-0.2.17/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/cpufeatures-0.2.17/LICENSE-MIT) |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/crypto-common-0.1.7/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/crypto-common-0.1.7/LICENSE-MIT) |
| digest | 0.10.7 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/digest-0.10.7/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/digest-0.10.7/LICENSE-MIT) |
| generic-array | 0.14.7 | MIT | [LICENSE](licenses/dependencies/generic-array-0.14.7/LICENSE) |
| itoa | 1.0.18 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/itoa-1.0.18/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/itoa-1.0.18/LICENSE-MIT) |
| libc | 0.2.191 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/libc-0.2.191/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/libc-0.2.191/LICENSE-MIT) |
| memchr | 2.8.3 | Unlicense OR MIT | [COPYING](licenses/dependencies/memchr-2.8.3/COPYING), [LICENSE-MIT](licenses/dependencies/memchr-2.8.3/LICENSE-MIT), [UNLICENSE](licenses/dependencies/memchr-2.8.3/UNLICENSE) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/proc-macro2-1.0.107/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/proc-macro2-1.0.107/LICENSE-MIT) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/quote-1.0.47/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/quote-1.0.47/LICENSE-MIT) |
| ruzstd | 0.8.2 | MIT | [LICENSE](licenses/dependencies/ruzstd-0.8.2/LICENSE) |
| serde | 1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/serde-1.0.229/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/serde-1.0.229/LICENSE-MIT) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/serde_core-1.0.229/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/serde_core-1.0.229/LICENSE-MIT) |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/serde_derive-1.0.229/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/serde_derive-1.0.229/LICENSE-MIT) |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/serde_json-1.0.154/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/serde_json-1.0.154/LICENSE-MIT), [LEXICAL-NOTICE](licenses/dependencies/serde_json-1.0.154/LEXICAL-NOTICE) |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/sha2-0.10.9/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/sha2-0.10.9/LICENSE-MIT) |
| syn | 3.0.9 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/syn-3.0.9/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/syn-3.0.9/LICENSE-MIT) |
| twox-hash | 2.1.5 | MIT | [LICENSE.txt](licenses/dependencies/twox-hash-2.1.5/LICENSE.txt) |
| typenum | 1.20.1 | MIT OR Apache-2.0 | [LICENSE](licenses/dependencies/typenum-1.20.1/LICENSE), [LICENSE-APACHE](licenses/dependencies/typenum-1.20.1/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/typenum-1.20.1/LICENSE-MIT) |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | [LICENSE-APACHE](licenses/dependencies/unicode-ident-1.0.27/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/unicode-ident-1.0.27/LICENSE-MIT), [LICENSE-UNICODE](licenses/dependencies/unicode-ident-1.0.27/LICENSE-UNICODE) |
| version_check | 0.9.5 | MIT/Apache-2.0 | [LICENSE-APACHE](licenses/dependencies/version_check-0.9.5/LICENSE-APACHE), [LICENSE-MIT](licenses/dependencies/version_check-0.9.5/LICENSE-MIT) |
| zmij | 1.0.23 | MIT | [LICENSE-MIT](licenses/dependencies/zmij-1.0.23/LICENSE-MIT) |

`ruzstd` 0.8.2 omits LICENSE from its crate archive. Its MIT text was retrieved
from the exact upstream commit recorded in the archive, not from the current
branch. `serde_json`'s additional lexical attribution is retained separately.

## Unicode CLDR data

`core/data/cldr47-windows-zones.tsv` is the CLDR 47 global Windows-to-IANA timezone
mapping. Copyright Unicode, Inc.; licensed under Unicode-3.0. The existing
[Unicode notice](core/data/UNICODE-LICENSE.txt) is preserved. Upstream source:
[CLDR release-47 windowsZones.xml](https://github.com/unicode-org/cldr/blob/release-47/common/supplemental/windowsZones.xml).

## Graphics integration code

The source and patch contributions in `integration/graphics-lifetime/` are
covered by [integration/LICENSE](integration/LICENSE)
(MIT). The migrated `guest_retirement.rs` already carried an MIT identifier.
The reference presenter, window adapter and direct bootstrap also carry MIT
identifiers. Existing upstream notices remain applicable to copied or patched
code; generated bundles do not inherit a blanket MIT or BSL license.

The opaque graphics probe under `integration/graphics-opaque-probe/` retains
MIT-marked diagnostic source from the verified WSX prototype and Khronos Vulkan
headers under Apache-2.0. Their original SPDX and copyright notices are retained;
the [Apache-2.0 text](integration/graphics-opaque-probe/APACHE-2.0.txt) and
[source provenance](integration/graphics-opaque-probe/README.md) accompany builds.

## Components outside this inventory

The independent Native Host, virglrenderer, Rutabaga, OpenVMM, libepoxy, input
companions, Linux modules, rootfs contents, Rust standard library and compiler
runtime components, and any separately distributed Microsoft runtime require
their own exact-version notices and distribution review. They are not covered
by the Cargo table above. Pro and WebView2 are also outside this public inventory.
See [licensing scope and outstanding review](LICENSING.md).
