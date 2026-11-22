# In-process edition integration builds

The community and Store candidates are separate builds. Each copies one PE to
`wsld.exe` and `wsx.exe` for the CLI. The community GUI uses `wsldesk.exe`
and the Store GUI uses `wsxdesk.exe`. The public community build does not depend
on store-edition; the private Store build consumes the public source crates.

On Windows with the pinned Rust/MSVC toolchain, pass a reviewed runtime package
and the corresponding Native and Guest catalogs to
`scripts/build-community-slice.ps1`. The script verifies both closures, embeds
the catalogs, builds and tests the crate, records PE hashes and imports, and
collects dependency notices for review. `scripts/package-community-slice.ps1`
can assemble a diagnostic ZIP from that output and the same runtime package.
It does not establish licensing or product release approval.

The GitHub Community workflow increments the patch version from the highest
existing `vMAJOR.MINOR.PATCH` tag or the version in
`community/tauri.conf.json`, whichever is higher. It verifies the Windows file
and product versions before creating the matching tag and Release. Existing tags
reserve their versions, including tags created for drafts. Promote a draft to
a pre-release and then to a stable release by editing the same GitHub Release.

The Store counterpart is `store-edition/scripts/build-store-slice.ps1`; it checks the
same runtime closure and builds the private Store executable. The compiled
edition selects the interface. Test entitlement, purchases and updates with
the actual Store package identity and the intended Microsoft Store account.
The Store GUI and privileged CLI commands request elevation through UAC.
The handoff verifies the elevated token, the WSX package family and the
originating Windows user before opening the GUI or running Core operations.
Read-only CLI commands run in the current terminal. A CLI handoff waits for
the elevated operation and returns its exit code. The Store submission must
obtain approval for the package's `allowElevation` capability.

`build-store-package.ps1` accepts `-LinkedPackage` for the new PE, the matching
runtime as `-CorePackage`, and the collected notice directory as `-ProNotices`.
It produces an unsigned MSIX. Store signing, account entitlement, purchase,
update behavior, fresh installation and real Store distribution are separate
acceptance steps. Do not replace an existing installed Store package during
local validation without checking ownership and the requested test scope.

An MSVC community build was also made from a source snapshot containing no
store-edition repository. On the current Windows machine, one sampled CLI run from
each edition loaded GUI system libraries including USER32, GDI32 and DWMAPI;
neither sample observed wsxpro.dll or WebView2. The two editions' three copies
were hash-identical within the edition, and their CLI help, version, full pipe,
invalid argument, unknown file name and forged argv[0] checks passed. An
early-closing PowerShell pipeline produced inconsistent results in both new
builds, while the older DLL baseline also returned `-1`; Ctrl+C,
Explorer launch and packaged Store behavior require further testing.
