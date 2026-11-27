# In-process edition integration builds

The Community and Store candidates are separate builds. Each copies one PE to
`wsld.exe` and `wsx.exe` for the CLI and `wsxdesk.exe` for the GUI. The
Community build uses the public source crates and a verified runtime package.

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

The Windows ZIP includes `KERNEL-SOURCE-LINKS.md` with version-specific links
and SHA-256 digests for the archived upstream kernel source. The materials ZIP
retains module sources, configurations, and build evidence. Keep the linked
public kernel release assets available alongside community binary releases.

The hosted workflow reads the approved runtime repository from the
`WSX_RUNTIME_REPOSITORY` Actions secret. Set it to the canonical
`OWNER/REPOSITORY` name and give the runtime token Contents read access.
