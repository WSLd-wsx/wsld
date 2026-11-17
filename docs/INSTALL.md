# Release installation

List the current public stable releases with `wsld.exe -l -o` (or
`wsld.exe --list --online`). The list checks completion-bound metadata, Core
compatibility and current CPU admission; rejected releases are reported separately.
It does not download or verify rootfs payload bytes. On 2026-11-07 a Windows
query listed `wsx-omarchy-4.0.6-r14`; availability can change.

The CLI and Core implement these installation entry points:

```powershell
.\wsld.exe --install wsx-omarchy
.\wsld.exe --install wsx-omarchy-4.0.6-r10
.\wsld.exe --install --pre-release wsx-omarchy-4.0.6-r11
```

The first selects the highest eligible numeric Omarchy version from the complete
paginated public release listing, ignoring upload order. The second requires an
exact published stable release. The third requires an exact published pre-release.
`--pre-release` is an option of `--install` and may also follow the version.
The former standalone `--install-pre-release` command is no longer accepted.
Drafts are rejected in both channels. Explicit versions never fall back. Unknown
distribution IDs and unspecified CachyOS rolling ordering are rejected.

Append `--check` for public metadata resolution only, without downloading the
rootfs or importing a distro. This does not attest to payload bytes or available
WSL/runtime compatibility. Ordinary commands perform the complete acquisition and
import sequence in that invocation, then enter first launch. `--no-launch` skips
first launch and exits after import. `--check` and `--no-launch` are mutually
exclusive. No background business daemon is created.

On 2026-11-05, Omarchy r9, r10 and r11 were pre-releases. The later stable
catalog is described above; availability and admission can change. The historical
r11 pre-release declares Guest ABI 1, a null minimum Core version, and `not_run` for WSL boot,
first frame and desktop. Explicit candidate import allows these unresolved test
results and reports them honestly. Stable import requires a declared supported
minimum Core version and passing boot/first-frame/desktop metadata. Changing only
the GitHub pre-release flag is not sufficient stable admission. Images with
explicit `x86-64-v3` requirements now undergo CPU/OS admission on every active
Windows logical processor before rootfs acquisition, with checks repeated before
import. Unknown requirements still reject. `--check` reports the CPU observation
without downloading the rootfs or importing it. See [CPU compatibility](CPU-COMPATIBILITY.md)
for feature coverage, requirement provenance and the separate Guest startup check.

## Implementation and ownership

`host/src/cli.rs` calls `core/src/install.rs`. `release.rs` resolves the channel and
version and checks the completion-bound manifest before a large transfer.
`candidate.rs` provides the existing shared, resumable byte-verified cache.
`catalog.rs` verifies the complete image/metadata closure. `import.rs` uses the
same zstd decoder and Windows WSL import path as the earlier candidate workflow.
`platform/registration.rs` reads WSL registration identity without starting Linux.

Stable and pre-release installations use the resolved exact tag as the distro
name, for example `wsx-omarchy-4.0.6-r11`. A stable request without a version
first resolves the latest stable release, then uses that release's full tag.
These are the names shown
by `wsl.exe -l -v` and used by subsequent `-d DISTRO_NAME` commands. An existing
WSL name causes refusal; installation is not upgrade, repair, adoption or overwrite.
After external `wsl.exe --unregister`, a completed import record becomes
`unregistered` history when the name and its previous disk are absent from current
WSL registrations. History and files remain; reinstallation uses a new directory
and GUID. An incomplete import still requires diagnosis and is not bypassed.

The schema-versioned store lock prevents concurrent WSX mutations. An exclusive
`instances/install-PID-NONCE` directory and ownership journal are created before
decoding/import. WSL registration is rechecked at import and its actual BasePath
and WSL 2 version are verified afterward. `wsx-import.json` records the observed
registration GUID, image tag/hash and channel. External WSL changes cannot be
serialized by the WSX lock; a concurrent registration change causes a diagnostic
failure, not takeover or automatic unregister.

## Cache, space and interruption

All channels use `%LOCALAPPDATA%\Temp\WSX\releases\TAG`. Existing complete
caches in `%LOCALAPPDATA%\WSX\cache\releases\TAG` remain eligible for reuse
after verification and are not moved or deleted during migration. A complete cache hit
requires the current public completion marker to match, followed by size/hash and
metadata verification. No rootfs network body is requested on a valid cache hit.
Small release/metadata requests still occur to establish current publication and
channel. Promotion can reuse the same image bytes if the completion marker is
unchanged. Modified/corrupt completed caches fail closed and remain for diagnosis.

Interrupted downloads remain in `TAG.partial/*.download`; a later invocation
rehashes retained bytes and resumes using HTTP Range. If the server refuses a
range with HTTP 200, the managed partial is restarted. Changed or missing markers
and incomplete temporary cache directories are retained separately; a fresh
download is started. A fully downloaded asset with a mismatching digest is also
retained in a separate incomplete transaction so a later attempt can start fresh.
Remaining space is checked
against remaining payload bytes. Failed hash/range checks never commit a partial
image. Downloads use public GitHub asset URLs without credentials, honor the
existing WinHTTP proxy policy and support `WSX_HTTP_PROXY=127.0.0.1:PORT` for a
loopback HTTP proxy. Source archives are not downloaded; their metadata closure
is still checked. Integrity hashes are not publisher signatures.

The verified zstd image is decoded directly to a tar file in
`%LOCALAPPDATA%\Temp\WSX\import-tar`,
then passed to the system `wsl.exe --import ... --version 2`. Linux files are never
unpacked into a Windows tree. Tar output is bounded by available free space after
reserving the manifest installation estimate and 512 MiB, with a 64 GiB maximum.
The manifest logical unpacked size is not a tar length. This conservative policy
may reject a tight disk even when a more exact space model would suffice.

Progress/stages go to stderr; successful structured results go to stdout. Download
progress includes percent, received/total MiB, average transfer MiB/s and ETA,
updated at most once per second plus completion. Resumed progress includes retained
bytes in percent but excludes them from the measured transfer speed. The decoder
also reports compressed-byte SHA-256 verification progress, then tar bytes written,
average MiB/s and elapsed time once per second. The GUI may use the manifest's
logical unpacked size to show an explicitly approximate extraction percentage;
it is not a tar length and is capped below 100% until extraction finishes.
A flush message precedes completion; completion is printed only after frame/checksum,
trailing-data, alignment and disk-flush checks pass. Cache reuse
is explicitly reported after rehashing; no rootfs body is downloaded again. During
the non-cancellable WSL import call, Ctrl+C/Ctrl+Break are deferred until the call
returns; this is not rollback. Forced process exit/console close/OS shutdown may
leave an interrupted journal. Decoder/import failure retains owned files, logs,
and records; repeated same-name installation refuses until diagnosed. There is
no automatic incomplete-import recovery, unregister, VHD deletion or garbage collection. A completed
import retains its tar and compressed cache subject to the maintenance policy below
and possible Windows temporary-file cleanup.

## Acceptance boundaries

Import success means the verified image has an owned WSL 2 registration with the
expected path. By default installation then invokes the same Core first-launch
flow as `-d DISTRO_NAME` / `--distribution DISTRO_NAME`. This checks the current
registration GUID, disk and import receipt, observes the actual Guest account,
and prompts locally if initialization has not completed. Windows timezone mapping
and the established `America/New_York` fallback apply. Password input goes directly
to Guest getpass through inherited console handles, never through Core logging.

`--no-launch` returns the import result without starting Linux, initializing an
account or loading a graphics runtime. For example:

```powershell
.\wsld.exe --install --pre-release wsx-cachyos-plasma-2026.11.4-r1 --no-launch
```

Normal first launch requires an interactive terminal for an uninitialized Guest.
Redirected input/output cannot supply a password. If initialization or desktop
startup fails, the command exits nonzero and explicitly reports that installation
already completed. Resume with `wsld.exe -d "DISTRO_NAME"`; do not reinstall.
A completed account is observed and never initialized again. Guest partial setup
can resume only with its recorded username/settings and owned UID; ambiguous
state continues to fail. Interactive Guest output may appear on stdout; `--no-launch`
remains the structured import-only path for automation.

Desktop launch checks the declared graphics runtime in the local cache. If it is
missing, Core can prepare it from `graphics/reference-RECEIPT_PREFIX` beside the
product executable. It verifies the exact receipt, component and identity-tool
hashes, stages into a new directory under the store lock, verifies again and
renames it into the cache. Existing or interrupted data is retained. Product build
scripts require `GraphicsReference` and `GraphicsIdentityTools` inputs and package
the supplied verified component bundles on every build. Supply all runtime receipts
needed by the images distributed with that product; unknown pairings still reject.
The public image metadata currently declares no graphics binary download source,
so an executable distributed without its matching component package cannot fetch it.
No image or desktop acceptance is implied by packaging.

For a release contract with a Core-reviewed automatic Host update, launch checks
the optional packaged or cached candidate before creating an Owner. The Guest
preflight checks the running kernel build identity, all candidate import CRCs,
loaded module identities and the declared Guest component contract. Startup
repeats admission under the machine gate. A known module mismatch may select
another available pair; unrelated errors are retained and reported. Explicit
runtime preferences take precedence. Missing pairs are not downloaded or built
automatically, and a matching kernel major alone is not compatibility proof.
The first automatic pair has one Iris Xe Omarchy r15 zero-copy lifecycle; it is
not evidence for every WSL kernel, graphics adapter, or staging path.

First-launch account admission applies to completed release imports using Guest
ABI 1, including CachyOS. Desktop admission includes pinned Omarchy r10/r11 and
`wsx-cachyos-plasma-2026.11.4-r1` with its exact verified archive. CachyOS requires
x86-64-v3 admission and the paired Plasma retirement runtime
`reference-685325f5032f`; an older Omarchy-only helper is rejected. Resume an
initialized CachyOS instance with `wsld.exe -d wsx-cachyos-plasma-2026.11.4-r1`.
See [CachyOS integration and acceptance limits](ADR-012.md) and
[DISTRO-DESKTOP.md](DISTRO-DESKTOP.md).

Run `scripts/test-install.ps1 -Executable PATH -Evidence NEW_DIRECTORY` for live
public metadata/CLI cases. Run `scripts/test-install-real.ps1 -Executable PATH
-Evidence NEW_DIRECTORY -Tag EXACT_PRE_RELEASE` only when creating a new test
distro is authorized. It records existing registrations and retains the new test
instance. Unit tests cover numeric ordering, channel isolation, exact failure,
compatibility refusal, cache tampering, decode bounds and store exclusion. They
do not substitute for a real WSL import or desktop acceptance.

## Protocol-only rebuilt images

New stable and pre-release installation now requires a valid
`desktop_compatibility` declaration before acquiring the rootfs payload. Missing
or unsupported protocol metadata is rejected without a legacy fallback. The
verified manifest is checked again after acquisition and retained in the import
receipt. Historical receipt reconciliation remains separate from new admission.

Regression fixtures use the published Omarchy 4.0.6-r12 and CachyOS Plasma
2026.11.5-r1 manifests. Compatibility follows their protocol, capabilities and
runtime pairing, not these tag names. These tests do not establish WSL boot or
GPU acceptance for either new image.

## Automatic maintenance

WSX keeps distribution disks, ownership records and recovery journals under
`%LOCALAPPDATA%\WSX`. These are shared by the
standalone CLI and packaged GUI; WSX does not intentionally remove them when
the Store app is uninstalled or reset.
In particular, an imported distribution's virtual disk must not be treated as
disposable app cache.

The packaged GUI stores its WebView data in the package's `LocalCacheFolder`
under `webview`. An unpackaged GUI uses `%LOCALAPPDATA%\WSX\webview` instead.
Existing WebView directories are not migrated or deleted. Windows does not
promise to clean `LocalCacheFolder` under disk pressure. A status query can
produce recovery evidence, so WSX retains its files in shared diagnostics until
completion checks establish that the result is safe to move to temporary storage.
Failures and uncertain results remain in persistent diagnostics.

WSX performs conservative maintenance during inventory refresh and after a
successful import, at most once an hour per process. It holds the cross-process
store lock and skips maintenance if an index transaction is pending. It does
not run a background service and does not remove distributions, virtual disks,
ownership records, interrupted imports or resumable downloads.

- Import TARs become eligible 24 hours after a completed import, only when the
  current WSL registration, disk and immutable import receipt still match. The
  same verified import's stdout/stderr become eligible after seven days.
- Completed, unambiguous status queries for a stopped distro may be removed
  after seven days. Failed, ambiguous and older unmarked queries are retained.
- Omarchy release caches become eligible 90 days after their last recorded full
  verification. The highest known cached Omarchy version is retained. Deletion
  requires a valid use record, unchanged completion metadata, the expected flat
  file set and no reparse points. Legacy caches without a use record are not
  deleted; a subsequent successful verification creates one. At most one
  release cache is removed per pass. CachyOS cache eviction is deferred because
  its latest-version ordering is not established.
- Native Host lifetime evidence, active or uncertain desktops, user-selected
  diagnostic files and partial downloads are never selected for automatic
  deletion by this maintenance path. An interrupted cache deletion may leave a
  `.wsx-cleanup-*` directory for inspection.

Retention thresholds are minimum ages for WSX maintenance, not guarantees for
files in Temp: Windows or user tools may remove temporary files sooner or later.
WSX holds completed import TARs open without delete sharing while WSL imports
them; missing files cause an error and never establish import success.
