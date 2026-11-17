# In-process edition integration builds

The community and Store candidates are separate builds. Each copies one PE to
`wsld.exe`, `wsx.exe`, and `wsxdesk.exe`. The public community build does not
depend on wsx-pro; the private Store build consumes the public source crates.

On Windows with the pinned Rust/MSVC toolchain, pass a reviewed runtime package
and the corresponding Native and Guest catalogs to
`scripts/build-community-slice.ps1`. The script verifies both closures, embeds
the catalogs, builds and tests the crate, records PE hashes and imports, and
collects dependency notices for review. `scripts/package-community-slice.ps1`
can assemble a diagnostic ZIP from that output and the same runtime package.
It does not establish licensing or product release approval.

The Store counterpart is `wsx-pro/scripts/build-store-slice.ps1`; it checks the
same runtime closure and builds the private Store executable. The existing
`build-store-package.ps1` accepts `-LinkedPackage` for the new PE, the matching
runtime as `-CorePackage`, and the collected notice directory as `-ProNotices`.
It produces an unsigned MSIX. Store signing, account entitlement, purchase,
update behavior, fresh installation and real Store distribution are separate
acceptance steps. Do not replace an existing installed Store package during
local validation without checking ownership and the requested test scope.

An MSVC community build was also made from a source snapshot containing no
wsx-pro repository. On the current Windows machine, one sampled CLI run from
each edition loaded GUI system libraries including USER32, GDI32 and DWMAPI;
neither sample observed wsxpro.dll or WebView2. The two editions' three copies
were hash-identical within the edition, and their CLI help, version, full pipe,
invalid argument, unknown file name and forged argv[0] checks passed. An
early-closing PowerShell pipeline produced inconsistent results in both new
builds, while the older DLL baseline also returned `-1`; Ctrl+C,
Explorer launch and packaged Store behavior require further testing.
