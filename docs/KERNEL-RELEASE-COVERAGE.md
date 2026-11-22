# WSL kernel release coverage

`scripts/build-release-with-kernels.ps1` is a release-build entry under
development. It inventories versioned official WSL kernel source tags using a
fresh `git ls-remote` and GitHub stable release list, checks target coverage and Core receipt admission, and
stops before building the package on any missing or invalid target. The normal
developer build entry remains available and does not claim release coverage.

The coverage gate deliberately cannot pass yet. The reference builder now
prepares a pinned toolchain, kernel source and module build for 6.18.58.1, but
the all-target builder and a verified target kernel binary provenance chain are
not implemented. They must be integrated and reviewed before the hard blocker
in `kernel-release-coverage.py` can be removed. An offline tag snapshot is
diagnostic only and never authorizes release packaging. Custom kernels and
unversioned tags are not covered by the current inventory.

Stable releases are processed in descending numeric version order; Drafts and
Pre-releases are excluded. A published release without its source tag fails the
scan. `scripts/compare-kernel-module-crcs.py` compares a target kernel export
table against all six modules in an existing group, including the group's own
exports. Matching CRCs are a reuse candidate, not loader, provenance, protocol
or GPU acceptance. Historical target kernel images must be obtained and
verified separately; they should remain runner-local instead of inflating the
downloadable module package. An existing package and its latest pointer must
be treated as untrusted inputs and verified before reuse.

Each target record is `targets/TAG.json` below the coverage directory. Schema 1
contains `tag`, immutable `source_commit`, relative `kernel_image`,
`kernel_image_sha256`, `kernel_build_id`, relative `audit`, relative `receipt`,
`receipt_sha256`, and `module_anchor` pointing to a module inside the receipt's
Guest directory. Audits must record all six module file hashes, every imported
symbol and any missing or mismatched CRCs, alongside the kernel image hash and
build ID. The checker validates file hashes and rejects incomplete or changed
records. Existing checks do not authenticate an upstream kernel binary or prove
the audit was performed against it; they are not a substitute for a trusted build.

An optional builder executable receives `--tag`, `--commit`, and `--coverage`
for each target without valid local evidence. After each call the same checks
are repeated. A builder exit code alone cannot establish coverage. Identical
module receipts may be referenced by several target records, but each target
needs its own verified kernel image and audit. Only component receipts already
admitted by Core can enter the package. GPU behavior and retirement require
separate hardware acceptance and are not inferred from symbol checks.

`scripts/build-coverage-target.py` is a diagnostic adapter for this builder
interface. It expects `target-inputs/TAG.json` to identify an existing kernel
package under the coverage directory, its exact version and SHA-256. It builds
into a unique retained attempt directory, reuses a separate source/toolchain
cache and records static CRC results. It intentionally does not create an
admitted target record: source/binary correspondence, component pairing and
runtime acceptance remain separate requirements. If an input record is absent,
the adapter queries Microsoft's WSL feed for the exact kernel version and
downloads the highest available package revision, retaining its observed hash
and feed evidence. Missing versions, interrupted downloads and ambiguous cache
state fail explicitly. An observed download hash is an integrity check, not
publisher authentication or source-to-binary provenance.

When a same-generation local snapshot exists, the builder first checks its
module imports against the target image exports. A complete CRC mismatch
permits a new build; a possible match requires loader metadata and independent
admission. An inspection error is never interpreted as incompatibility. On
2026-11-12, the saved Microsoft 6.18.54.1 image was checked against the six
modules from runner build 43046940732 for 6.18.58.1. All six had CRC
mismatches; the recorded comparison is
`artifacts/wsl-preview-kernel-6.18.54.1-20261111/real-crc-prescreen-20261112.json`.
That result excludes reuse of this specific group on that image; it is not a
module load, desktop, source provenance or release acceptance test.

## GitHub Actions and Packages

Diagnostic index schema 2 references one independent, content-addressed
build-input archive for the indexed generation; all groups in that index share
that descriptor. The archive is separate from the evidence archive and can be
fetched by immutable OCI manifest digest. Bootstrap verifies the archive hash
and generation before staging the snapshot. This archive currently contains
recorded WSX build inputs only, explicitly marks corresponding source as
incomplete, and cannot satisfy the publication gate. Target-specific source
and redistribution review are still required. Schema 1 remains readable.

The product distribution unit is one archive per distinct, content-addressed
six-module group, plus a small index mapping verified kernel identities and
compatibility evidence to candidate groups. Multiple kernel targets may refer
to the same group. Core first checks its shared local cache, then downloads a
candidate group when needed, verifies its content and component contract, and
checks the actual running kernel before loading. A CRC match alone is not
runtime admission. No compatible group must produce a clear error without
force-loading or rebuilding the rootfs. The diagnostic exporter generates one
archive and immutable OCI manifest per group and a compact index. It does not
publish or establish runtime admission. A diagnostic GHCR reader fetches a
group by immutable manifest digest and rechecks the archive and all six module
hashes; it is not yet connected to the Windows product's download path.

The private `operations` component workflow is the operational reference:
it uses a workflow-scoped token, a pinned source commit, pre-publication checks,
an immutable GHCR digest and a registry read-back. Its component transport uses
a scratch Docker image. WSX kernel groups instead use an OCI artifact layer so
that the product can consume the archive without requiring Docker. Equivalent
visibility, source-delivery, digest read-back and publication checks remain to
be implemented and verified for the public kernel package. Unlike that
cross-repository workflow, the kernel builder and package are owned by this
repository; the eventual publishing job should use its workflow `GITHUB_TOKEN`
with `packages: write`, subject to actual repository/package access checks.
On 2026-11-12, a read-only request to a public GHCR OCI image observed a blob
GET redirect from `ghcr.io` to `pkg-containers.githubusercontent.com`.
This supports the current explicit host allowlist but does not prove that every
future GHCR deployment uses the same host or that the WSX package is accessible.
The Windows MSVC WinHTTP adapter was also exercised against that public image:
anonymous token, index and platform manifest retrieval, redirected config blob
download, and SHA-256 verification passed. This was a read-only network test,
not a WSX module package or desktop acceptance.

The current `.github/workflows/kernel-modules-build.yml` discovers stable and
preview WSL releases from 3.0.3 onward and produces versioned GitHub Release
assets. Its builds retain source and verification evidence; they do not by
themselves establish runtime acceptance or complete corresponding source.

The workflow also checks the latest stable release of `microsoft/WSL2-Linux-Kernel`
and Microsoft's WSL kernel package index. It prebuilds the newest exact package
for that source release if it is newer than the kernels referenced by released
WSL. An RC tag, a prerelease, or a source release without a matching package
does not trigger this path. The prebuild runs the normal static module CRC
verification and saves the build workspace in the Actions cache.

The release target list still comes from official WSL releases. A cached build
can be reused when that list includes the same full kernel package version.
The release build fetches the exact package again and checks its SHA-256,
the upstream source commit and the build generation before reusing the
workspace. It still runs the normal snapshot and release evidence checks.
The cache follows the newest stable source and may expire or be evicted.
A cache miss or identity mismatch uses the regular build path.

The following observations concern a historical diagnostic run and not the
current release workflow:
The retained `artifacts/runner-draft-43046940732/crc-comparison.json` records
all six module imports, `module_layout`, and matching CRCs for kernel build ID
`746c3677b684922ab4a9cc8c863d394238d87022`. This historical audit belongs to
that recorded module set. On 2026-11-12 the SHA-256 of each file under
`artifacts/runner-draft-43046940732/extracted/modules-build/modules` matched
its entry in that audit. This is not a load or retirement result for any new
build generation. Its build receipt records generation `48a1ec95802cdea4b9e422eb71890563a6e69ec4fff790f80ecfb5a8d04f961e`
and `runner_config_unreviewed: true`; the current source fingerprint differs.
This old receipt therefore cannot satisfy the current generation's release gate.

The earlier `kernel-coverage.yml` was an experimental GHCR coverage scanner;
it is no longer an active workflow. An archival copy is retained in the
private operations repository, outside its workflow directory. Its scripts and
historical evidence remain in this repository, but the current versioned
GitHub Release pipeline does not depend on this workflow. Build generation
continues to record the runner dependency versions for the active pipeline.

The intended scheduled flow is: retrieve and verify the latest prior package;
fetch the current stable Microsoft releases; process newest first; re-audit
existing module groups against each target; build missing compatible groups;
preserve module and provenance outputs while removing target-specific build
trees; then publish an immutable package and update a latest pointer only after
all required gates pass. The diagnostic scanner can fetch and integrity-check
an existing GHCR snapshot and stage it locally, but the schedule, complete
historical builders and publication steps are not yet connected and the
workflow does not claim them.

The intended distribution is a public GitHub Container Registry package linked
to this repository, containing content-addressed module groups, provenance and
required source/notice materials. Publication and runtime package download are
not implemented. The release gate remains closed until target binary/source
provenance, module CRC coverage, package immutability, corresponding-source
delivery, Core admission and hardware lifecycle evidence are addressed. A
GitHub-hosted build is not a Windows/WSL desktop test. Package visibility is
configured separately on GitHub and must be reviewed before changing it.
An authenticated organization package-list query on 2026-11-12 returned four
visible container packages, none named `wsld-kernel-modules`. This is a
read-only observation within that account's permissions, not proof that no
inaccessible package or historical registry state exists. Initialization must
not treat a registry 404 or this observation as an empty-package authorization.
# Preview monitoring

The preview monitor reads Microsoft's public WSL prereleases, selects the
highest numeric prerelease, resolves its tag to a commit and reads the kernel
package dependency from `packages.config` at that commit. A WSL prerelease can
depend on an older kernel than the newest stable kernel release. The separate
kernel release inventory also records the newest numeric kernel prerelease or
RC source tag, including an RC tag without a corresponding GitHub release.
Discovery does not establish a matching Microsoft binary package. The current
target builder explicitly rejects RC tags until that mapping is verified.
Neither a WSL prerelease nor a dependency version proves source-to-binary
correspondence. Preview outputs are never treated as stable admission.

The earlier preview monitoring workflow was pinned to one kernel version and
is no longer active. The current module build workflow discovers both stable
and preview WSL releases; its normal source, compatibility and release checks
still apply to every selected version.

Snapshot schema 2 retains each build receipt by SHA-256 alongside the module
group and target CRC audit. Validation checks that the receipt identifies the
indexed source commit, generation and exact module bytes. Schema 1 snapshots
remain readable, but cannot be merged directly with schema 2 or used for
publication without explicit evidence migration. A receipt alone does not
prove source provenance, license compliance, loading, or desktop acceptance.

Snapshot schema 3 additionally records `build_source_commit` separately from
the target's `source_commit`. This permits a verified module group built against
one kernel source to be represented as a candidate for another kernel without
rewriting its build history. Each target still requires its own kernel and CRC
evidence; schema 3 alone does not establish cross-kernel compatibility.

