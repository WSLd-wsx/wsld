# Migration and integration status, 2026-11-04

## CachyOS Plasma connected, 2026-11-05

The owned `wsx-cachyos-plasma-2026.11.4-r1` now launches through the compiled Core
and the reused Native/Guest graphics chain. The actual Iris Xe desktop displayed;
its user confirmed display, mouse and keyboard. The corrected display adapter
returned 0 at 1920x1440/100%, and live status observed the exact active Plasma unit.
Build 04 passed 60 MSVC tests and the three identical-PE check; seven mock cases
pass separately. The first build's post-first-frame manifest-size failure remains
recorded. No stop/reload/restart was performed: normal retirement and full fresh
startup on the final public build remain untested. See [ADR 012](ADR-012.md) for
source reuse, exact hashes, evidence and limitations. The desktop is left running.

## Guest RAM profile migration omission corrected, 2026-11-05

The interactive freeze reproduced with the diagnostic pair: its 300th HDV Guest
RAM mapping failed with `0x80070718` before VM exit, with 299 usable cached pages
and no reclaim. Core had omitted the prototype-verified 64/32-page watermark,
leaving Native's default 1024 in effect. Core now supplies the verified value;
46 MSVC tests pass. See [ADR 010](ADR-010.md) and the
[migration cross-check](PROTOTYPE-MIGRATION-CHECKLIST.md). Historical prototype
reclaim success is separate from the current product interaction retest.
The product retest subsequently recorded 50 real reclaim cycles, 1,644 successful
maps and 2,407 presentations without map/reclaim failure; the user confirmed
repeated screensaver/menu operations remained normal. Desktop is left running.
This is bounded interaction acceptance, not indefinite stability or retirement.

## Current-authority startup, 2026-11-05

[ADR 009](ADR-009.md) removes the historical exit-log gate for fresh startup in a
different observed VM. No manual recovery receipt is required. The old failure
remains recorded, while current Host/HCS/Guest checks decide admission. The user
authorized terminating only failed Native PID 14364; evidence is preserved.
All 45 MSVC tests and the real clean-Guest preflight passed. The explicit diagnostic
runtime is selected for the new desktop; sustained stability remains unproven.

## Post-shutdown restart: initial display passed, subsequent stability failed

The 2026-11-05 restart returned 0 with a new first frame and an active Guest
session. The user initially interacted with the desktop and launched its
screensaver, but later the window and WSL-hosted commands stopped responding.
The user recovered through WSLService stop/start. This is a failed stability
acceptance, not a complete successful restart. The freeze trigger and the timing
of the final `0x80070718` aperture error remain unknown. Native PID 14364 remained
in its retained-failure path after service recovery. See [ADR 008](ADR-008.md)
and `artifacts/post-shutdown-hang-20261105-01` for evidence and diagnostic changes.
The bounded previous shutdown result below remains historical evidence only.

## Real Omarchy whole-WSL shutdown: passed, 2026-11-05

Run `003036dc-4383-4517-b3b5-165e4156c5d2` passed in
`artifacts/shutdown-acceptance-20261105-03`. The compiled Core started r11 without
Windows restart, process kill or module unload, then the independent collector
executed the authorized `wsl.exe --shutdown` once (exit 0 at 07:28:08Z).
Original VM `eaec1d8b-69dd-4f65-8b08-f765e1e3415c` exited. Native PID 18996 /
instance `a99abdf3-ae4b-44d0-acfa-938d1db875e9` closed memory/native device/bootstrap
and exited 0; input PID 19412 exited 0. RAM went from 196 slots / 802816 usable
bytes to zero slots/pages/bytes. Both pre-bound helpers exited 1 during VM shutdown;
actual HCS exit discharged Guest admission, while all native/memory barriers
completed independently. No quarantine remained. All nine registrations survived.
Collector VM enumerations immediately and at completion were empty: no replacement
VM was observed before the user's manual reconnection.

Core independently reported `native_retired`, `native_retirement_confirmed=true`
and `restart_requires_retirement=false`. Post-reconnect Windows-only readback
confirmed the same result with exit 0. No repeated shutdown or desktop restart was
performed during readback. All 43 raw files were archived and source/destination
SHA-256 checked; screenshot, logs, identities and results are retained.

This run emitted no `0x80070006` aperture-reconciliation marker: RAM closed through
the ordinary unmap path. The new MEM_FREE recovery branch remains separately
verified by the real isolated VM test, not claimed as exercised here. The source
collector's empty-array serialization and Core exit-code capture were corrected
after readback; raw test scripts/results remain unchanged and collection limits
are documented in the archive README. Subsequent restart, exact-pair WM_CLOSE,
race stress and fresh Iris Xe acceptance remain separate work.


## Fresh VM/Host recovery without Windows restart, 2026-11-05

[ADR 007](ADR-007.md) adds a separately recorded fresh-admission route for the
current independent Native Host architecture. Require exact original VM-exit
evidence, ended old Native identity, no graphics owners, a different current VM,
and fresh Guest admission. Preserve the old failure and never equate process exit
with graceful retirement. No Windows restart, process kill or module unload was
needed. Public build `artifacts/vm-generation-build-20261105-01` passed 45 MSVC
tests; the actual r11 restart check passed with no loaded modules or descriptors.
The full Omarchy shutdown retest subsequently passed with Windows kit `03`, as recorded above.
Earlier Windows-restart-only kit `02` is unarmed and superseded.


## VM-exit aperture and retirement-race repair, 2026-11-05

The new pair reconciles an already revoked Guest RAM aperture only after exact
original HCS VM exit and `MEM_FREE` across its whole old address range. Other
errors, live/reused/unknown memory and uncertain VM state retain the failure.
GPU/context/completion/Section/HDV closure barriers are unchanged. Both normal
Guest and external-stop system helpers now handshake before rendering; normal
PAM/systemd cleanup is preserved and close never launches a replacement helper.

The real diskless HCS/HDV test in `artifacts/vm-exit-api-probe-20261105-06` passed:
live VM refusal, normal unmap, actual VM-exit mapping reconciliation and successful
HDV teardown. Windows fixtures passed 58 aperture assertions, HCS return/thread
checks and three Rust tests including five helper subprocess scenarios. The actual
Python wrapper also passed handshake/EOF/invalid-command checks without retirement.
These are separate from the subsequent successful Omarchy shutdown run recorded above.

Public package: `artifacts/vm-exit-build-20261105-02/package`; all 44 Windows/MSVC
tests passed and all three public PEs are identical. Actual CLI version, cached
runtime verification and old-lifetime diagnosis passed without Guest execution.
Graphics: `artifacts/graphics-vm-exit-pair-20261105-03/bundle`.
Default cache: `reference-565b64806b7f`. Old cache entries remain available for
observation. The old lifetime remains `failed_retained`; PID 18984 was present at
07:11:40 UTC and later absent without successful retirement evidence. Its exit
cause is unknown; absence is not treated as recovery. No force kill, existing module unload or second WSL shutdown
was performed. See [ADR 006](ADR-006.md) for evidence, commands and limitations.


## Real VM shutdown retirement failed, 2026-11-05

The user-authorized `wsl.exe --shutdown` returned 0 and the original VM exited.
The independent Windows collector survived; after manual reconnection its complete
50-file evidence kit was archived with source/destination SHA-256 verification in
`artifacts/shutdown-acceptance-20261105-01`. All registration identities survived.
Native PID 18984 failed Guest RAM lease closure with `0x80070006`, retaining 218
slots (217 usable pages); it remains quarantined. Input exited 0, system helper
exited 1, and final native/memory retirement did not complete. Core correctly
reports `failed_retained`. The collector reports `passed=false`; no forced Host
termination or second shutdown was performed. A new VM was already present in
immediate enumeration; the normal Guest retirement invocation/shutdown race remains
unresolved. See [ADR 005](ADR-005.md) for exact identities, evidence and follow-up.
This is a real failure, separate from the passing distro-termination tests below.

## External distro termination retirement, 2026-11-05

The final Native pair detects the configured distro leaving WSL's running list
and retires only its exact PCI instance through a pre-existing system helper.
The helper handshakes before rendering and later receives a private stdin request;
the stopped distro is not restarted. The original HCS VM handle supplies a separate
VM-exit branch, while GPU/memory/HDV closure remain mandatory fallible barriers.
Core accepts VM-exit evidence only together with complete native retirement.

Two real r11 `wsl --terminate` tests passed, including the final helper design:
`artifacts/external-termination-live-20261105-01` and `02`. Guest retirement, zero
retained memory, native/input exit code zero, helper exit, unchanged registrations
and preservation of Ubuntu-24.04 were verified. Those tests left r11 stopped. Required
retired-Owner reloads were explicitly authorized and rechecked; no Host was killed
and no WSL/Windows shutdown was performed during those distro-only tests.

Current public package: `artifacts/external-lifetime-build-20261105-03/package`.
44 Windows/MSVC tests passed. Three public PEs share SHA-256
`c2f1c3ea97066d677b1b093f6443935f2d4e571704932fcf0d9833ca2ec44f4f`.
Graphics bundle: `artifacts/graphics-external-pair-20261105-02/bundle`; default
cache `reference-093dac80ac0d`. Isolated HCS policy and two encoding/parser tests
passed separately in `artifacts/external-lifetime-fixtures-20261105-01`.

The subsequent authorized real `wsl --shutdown` test failed native memory
retirement as recorded above; simulated API-return tests did not predict this
real VM-loss failure. Final-pair normal-close regression, stop/terminate races, later restart
and a fresh Iris Xe run remain unaccepted. See [ADR 005](ADR-005.md) for implementation,
commands, component hashes, API basis and the normal-close check/start race.

## Renderer WM_CLOSE normal retirement, 2026-11-05

The paired Native Host/renderer was rebuilt from verified reference inputs and
admitted through a new receipt. Real r11 first-frame and window-close acceptance
passed on NVIDIA GeForce RTX 3080 Ti: exact Guest instance retired, retained
memory reached zero, and Native and input processes exited naturally with code
zero. No forced termination, module unload or WSL/Windows restart was used.
Core independently reported `native_retired` and confirmed retirement. Closing
the window requests the existing stop coordinator before destroying output;
queue service remains available for Guest/GPU drain. The old pair is refused for
new startup, while still supported for observation.

Public package: `artifacts/window-close-build-20261105-01/package` (43 native
Windows/MSVC tests passed). Paired graphics build:
`artifacts/graphics-close-pair-20261105-01`. Screenshot, native log, identity checks,
negative pair-admission results and post-close observations:
`artifacts/window-close-live-20261105-01`. Three public EXEs are identical, SHA-256
`88f879ecd7b2649509f547a819bfb741b396633ae5df10f857a9ca923b1c8eaf`.

Post-close restart preflight passed and requires explicit retired-Owner reload;
no reload or second start was performed. An initial invocation omitted the
required runtime argument and was refused; that transcript is preserved alongside
the successful explicit-runtime preflight. Recovery of the earlier pre-reboot
record used newer Windows boot/absent owner and VM plus clean Guest checks, not
invented retirement. External Guest/VM loss, race stress, representative GPU apps
and a fresh Iris Xe run remain unaccepted. See [ADR 004](ADR-004.md).

## Real duplicate-install command acceptance, 2026-11-05

After the user restarted the local execution environment, both ordinary shell
and Windows PowerShell probes returned successfully. PowerShell then executed
the actual compiled command (without `--check`):
`wsld.exe --install --pre-release wsx-omarchy-4.0.6-r11`.
At 05:37:37 UTC it returned exit code 1 with:
`WSL distro name already exists: wsx-omarchy-4.0.6-r11; no overwrite or adoption performed`.
All nine WSL registration records, including the existing r11 GUID and disk
path, were identical before and after the command. The Owner observation
comparison was also unchanged. This passes duplicate-import protection; it is
not a new download/import or desktop acceptance. No distro was removed or
overwritten to obtain this result.

Raw stdout (empty), stderr, exit code, before/after registration snapshots,
the PowerShell test script and hashes are retained in
`artifacts/install-real-command-20261105-02`. Earlier attempts in `01` never
entered the test script and remain separate execution-channel diagnostics.

## Install pre-release option, 2026-11-05

The current test-image entry is `wsld.exe --install --pre-release ID-VERSION`.
The channel flag may also follow the version; `--check` remains metadata-only.
The former standalone command rejects. Stable selection, exact-version
requirements, verified cache and import behavior use the same Core implementation.
Native Rust/MSVC: 42 top-level tests passed. Twelve real Windows CLI cases passed
in `artifacts/install-option-acceptance-20261105-01`, including both option
positions, public r11 metadata, stable-channel exclusion and old-command refusal.
Build: `artifacts/install-option-build-20261105-01/package`.
All three identical public PE copies have SHA-256
`0289aefbe144ffb1e0811b7d61753ea187525f818cd795b696825689a16ce3d0`.

## r11 account initialization and desktop first frame, 2026-11-05

The compiled product was launched in a visible local Windows terminal with
`wsld.exe -d wsx-omarchy-4.0.6-r11`. The user entered credentials locally;
the runner did not redirect or record the credential streams. Guest observation
confirmed ABI 1, user `wen`, UID 1000 and `Asia/Shanghai`, selected by Core from
the Windows timezone. The same command continued through the existing r10
direct-Host workflow and returned exit code 0 at 04:29:01 UTC.

The running kernel Build ID and all 615 import CRCs across six modules passed
the existing audit with no mismatches or unobserved imports. No module reload
was required. Actual Vulkan inventory identifies NVIDIA GeForce RTX 3080 Ti;
the implementation selects by active display/LUID, without a GPU-brand branch.
The admitted Native Host, renderer, input and module closure were reused.

Native PID 16912 and input PID 5468 started for instance
`fdfbbf93-cb83-4f79-9f0d-fa19fd58f36d`, bound to r11 registration
`eb828f35-0228-4065-ab55-9f16b6bf5324`. Native logs record successful HDV startup
and `WSX_PRESENT_FRAME` with `result=0`; Core records `first_frame_observed`.
Long-form `--distribution` status observed the exact Guest systemd session
active/running with exit status 0, a matched registration binding and visible
Native window. The user independently confirmed the desktop was displayed.
An identity-checked 800x480 window screenshot shows the Omarchy desktop.

Evidence: `artifacts/r11-desktop-live-20261105-01`, including raw preflight,
Native log, Guest/session status, command result, screenshot and SHA256SUMS.
Product: `artifacts/default-desktop-build-20261105-03/package`; unchanged PE
SHA-256 `e21bf5ea139c532fa45ea544dfc4ec8d7dc7e8d482f81232096a6c81f41e7947`.
The first evidence-summary script hit PowerShell 5's case-insensitive JSON-key
restriction on Linux manifest names; raw evidence was already preserved. The
archiver was corrected without rebuilding or relaunching the product.

The desktop is left running for the user. Physical input, representative GPU
applications, password authentication, normal stop and restart are not accepted
by this startup test. `native_running_unverified` remains the conservative live
status label because this historical Native Host has no live health protocol;
it does not erase the separately observed successful first frame. No release
promotion or publication was performed.

## User-managed legacy recovery observed, 2026-11-05

After the user reported removal of the old service, read-only Windows checks at
04:26-04:27 UTC observed a new OS boot (04:12:56 UTC), the official WSLService
running as PID 9108 with automatic startup, no historical plugin registration,
all six COM paths restored, and no native/input Host processes. The historical
service executable is absent. All four official runtime binaries still match
the preparation hashes and have valid Microsoft signatures. No recovery script
mutation was performed during this observation; do not rerun the old prepared
transaction against this changed environment.

The compiled product's r11 desktop status returned exit 0: the same r11
registration GUID and owned disk remain, the distro is stopped, and no native
Owner is observed. Guest was not started for status. This is recovery through a
new Windows boot, not evidence of normal GPU retirement. r11 initialization and
desktop acceptance remain pending. Evidence and reproducible read-only checks:
`artifacts/legacy-recovery-observed-20261105`.

## Legacy service recovery preparation, 2026-11-05

The current historical service-owned Native Host blocks the direct r11 launch.
Read-only investigation found that the old service does not wait for the
plugin's pending Guest/device retirement before VM destruction. A historical
native log corroborates failed Guest retirement and retained resources. No stop
was attempted against the current lifetime, and normal retirement is unaccepted.

A Windows-local schema 2 maintenance kit backs up the old deployment, six COM
registrations, related interfaces and logs. Official WSL files and Microsoft
signatures verified. PowerShell syntax and authorization-refusal tests passed;
service, plugin, COM paths and Host identities remained unchanged.
The user then explicitly authorized disabling the next service start only.
`StageReboot` completed at 2026-11-05T03:47Z: start mode is Disabled while the
same service PID 3936 remains Running and Host/input PIDs 14744/14884 retain
their identities. The plugin and service paths are unchanged. The receipt and
`staged-inspection.json` are archived in the evidence directory. No restart,
service stop, COM restoration or post-reboot recovery was executed.
See [recovery procedure and evidence](LEGACY-WSL-RECOVERY.md).

## Default r11 desktop entry and reuse, 2026-11-05

`wsld.exe -d wsx-omarchy-4.0.6-r11` and
`wsld.exe --distribution wsx-omarchy-4.0.6-r11` now select the same default desktop
workflow; explicit `desktop start` also accepts the verified default cache.
This reuses the r10 `start_with_registration` implementation, Native Host,
renderer/input binaries, six modules, VMBus discovery, Guest prepare/start,
first-frame observation and guarded retirement. No graphics protocol or GPU-brand
branch was rewritten. The prior successful r10 acceptance remains recorded below.

Changes add exact r11 import receipt/GUID/hash binding (also enforced for legacy
candidate-ID launch), a versioned default runtime cache, and local interactive
first-run Guest initialization. Core maps the Windows timezone, reads a username,
and delegates password prompts directly to Guest without capture. Account ABI,
non-root UID and actual username/timezone are checked before startup. The admitted
68,677,120-byte Native Host previously exceeded the observation code's generic
64 MiB ceiling; it now requires that exact size plus the existing pinned digest.

Build: `artifacts/default-desktop-build-20261105-03/package`.
SHA-256 for all three identical public PE copies:
`e21bf5ea139c532fa45ea544dfc4ec8d7dc7e8d482f81232096a6c81f41e7947`.
Native Rust/MSVC: 40 top-level tests passed, including import binding and account
validation. Existing Python Guest tests: 16 passed. Six real Windows cases passed
in `artifacts/default-desktop-acceptance-20261105-02`: both aliases and explicit
start reach the existing-owner refusal, long-form status works without waking a
stopped distro, and invalid/missing names reject. Existing Host identity is
unchanged. The first harness incorrectly expected account details from status
of a stopped distro; that expectation was corrected and rerun, without changing
status to start a stopped Guest.

Read-only new-image preflight: `artifacts/r11-desktop-preflight-20261105-01`.
The newly owned r11 Guest booted successfully for observation and has Guest ABI 1
but no initialized user or pending initialization. Its actual component manifests
and 599 files verified (Mesa 59, Venus 2, opaque consumer 538). The existing module
audit matched all 615 imports with zero missing/conflicting CRCs against the
running `6.18.54.1-microsoft-standard-WSL2` kernel; Build ID
`de17a562c219da52bb9bb56182d657b775f45676`. These checks reuse the existing audit,
not a redesign of the graphics or module contract. No modules were loaded.

The unchanged reference closure is staged under
`%LOCALAPPDATA%\WSX\cache\graphics\reference-7d84b42d17ab`. The verified identity
utilities were copied once from the recorded prototype evidence (adapter inventory
SHA-256 `4cef7ea4d19a49dd8e6aff486a0b99e9d3e67adede1f0f32c6b4371f9482e6ea`,
Vulkan identity SHA-256
`7b026cfc76c31b72a0151b308285c3ba194cf0b028758c9100ae9702345ad066`).
Neither a prototype checkout nor its absolute path is a product runtime dependency.
The command still rejects absent or modified cache components; automatic runtime
component download/distribution remains separate work.

**Actual r11 desktop startup is blocked, not accepted.** This machine still runs
service-style Native Host PID 14744, owned by custom WSLService PID 3936 at
`C:\Program Files\WSX\wsl-runtime-20261029-rtx01\wslservice.exe`. The Host uses
`--owner`, not the admitted independent `--direct` lifecycle. Both commands refuse
before account setup or native/device creation. No service, Host or WSL was stopped
or restarted. A service stop could affect all active distros and this development
workspace; resolving that existing environment requires a separately authorized
maintenance action, not relaxed owner checks. Non-echoed first-run interaction,
r11 first frame/input and r11 normal retirement have not run. No screenshot of a
new r11 desktop exists. `artifacts/default-desktop-delivery-20261105.json` records
the build and evidence paths. Original bytes are protected in
`.handoff-backups/20261105T031843Z-default-desktop` at the workspace root. No commit
or push was performed for these changes.


## Public release installation and download progress, 2026-11-05

Implemented `--install ID`, `--install ID-VERSION`, and
`--install --pre-release ID-VERSION` in the shared Core. See [INSTALL.md](INSTALL.md)
for channel/version selection, destination names, cache validation, safe import,
space policy and recovery limits. `--check` resolves public metadata without
payload download or import. Download progress now shows percentage, received/total
MiB, transfer speed and ETA; resumed bytes do not inflate the measured speed.

Native Windows Rust/MSVC tests: 39 top-level tests passed (including helper tests).
Python Guest-observer tests: 16 passed. Seven real Windows CLI/public metadata
cases passed in `artifacts/install-cli-20261105-02`. The first harness attempt
reported a null PowerShell Process.ExitCode; exit-code collection was fixed and
rerun. This was a test harness failure, not an installation failure.

The compiled EXE in `artifacts/install-build-20261105-02/package` downloaded and
verified public r11 (1,397,490,868 compressed bytes), decoded a 3,868,303,360-byte
tar, and imported it into WSL 2 between 03:06:49 and 03:11:27 UTC.
`artifacts/install-r11-20261105-01` contains the full command output, ownership,
space/registration snapshots and running CLI module list (no Pro/Tauri/WebView2).
Actual WSL distro name: `wsx-omarchy-4.0.6-r11`; registration GUID:
`{eb828f35-0228-4065-ab55-9f16b6bf5324}`. Owned directory:
`%LOCALAPPDATA%\WSX\instances\install-10844-1793848196706597900`.
All previous registrations remained unchanged. The existing Native Host PID 14744
retained the same executable and creation time. The new distro, tar and verified
cache are retained; no source archives were downloaded, no account initialized,
and no desktop started. This is import acceptance only.

The final progress build is
`artifacts/install-progress-build-20261105-01/package`; all three PE copies have
SHA-256 `84c1c4ff58721a978bb7f998e72d12e4e40e26dacafa74f6c7a03995e938d47d`.
This final binary rehashed and reused the real r11 cache without a rootfs download,
left cached file sizes/timestamps unchanged, and rejected duplicate installation.
Evidence: `artifacts/install-cache-20261105-01`. Rootfs SHA-256:
`d27ba2a61818b8a6c3ed51ad44ca31279659c99e7d5deacc2603d1ba3d1ea841`.
The full import was not repeated for the progress-only change; no second test
instance was created. Progress formatting/resume-speed calculation has a native
unit test; the final binary's live download progress was not retested by discarding
the valid rootfs cache. Its cache-hit message was exercised on Windows.

Current public stable selections `wsx-omarchy` and `wsx-omarchy-4.0.6-r10` reject:
all published Omarchy images are pre-releases. r11 still has null minimum Core
version and unrun boot/desktop metadata, so promotion alone cannot admit stable
installation. CPU-feature admission for CachyOS, automatic interrupted-import
recovery, actual Ctrl+C-during-import testing and r11 desktop admission remain
uncompleted. Source changes and full evidence pointers are recorded in
`artifacts/install-delivery-20261105.json`. No commit, push or release was performed
for this change. The earlier distro-name implementation was preserved.


## WSL distro name terminology, 2026-11-05

Daily commands use the WSL **distro name**, exactly the NAME column from
`wsl.exe -l -v`. Help uses `DISTRO_NAME`; JSON reports `distro_name`, while
internal identifiers are explicitly labelled `registration_guid` and
`installation_id`. No alternate candidate-ID/GUID resolution was added and no
WSL distro was renamed. Existing installation journals remain compatible.

Latest package: `artifacts/distro-name-build-20261105-03/package`.
All three EXEs share SHA-256
`5caffa1cc4eaa72c2fefcb5b4a18a17b8a234687e8c13d66f7405e4a978e718e`.
The build passed 33 MSVC Rust tests. The ten Windows cases in
`artifacts/distro-name-acceptance-20261105-02` passed, including canonical
distro-name output for Ubuntu-24.04 and Omarchy and internal GUID/creation-time
refusal checks using an inert event fixture. Existing native processes and
running distros were unchanged. Earlier full desktop recovery limitations remain.

## Distro-addressed desktop commands, 2026-11-05

The CLI now accepts `-d DISTRO desktop status|stop|start|restart-check`.
Status discovers the actual WSL registration, live Native processes/events and
Guest account/session observations without reading the installation index,
active lifetime pointer or persisted phase. Start retains installation ownership
and existing retirement/operation journals; it does not adopt arbitrary distros.
See [distro commands](DISTRO-DESKTOP.md) and [ADR 003](ADR-003.md).

`artifacts/distro-desktop-build-20261105-04` passed 33 Windows MSVC Rust tests.
The three identical EXEs have SHA-256
`9fa33fba27eb96d24219bc6d54c9e9f1ca8f975a50abf41a43675a9ac79a5a9b`.
Sixteen Python tests passed, including four Guest observation cases.
`artifacts/distro-desktop-acceptance-20261105-01` passed ten real Windows
command/adapter cases. A private corrupt index did not prevent live or stopped
distro status. Known stopped Omarchy remained stopped; a pre-existing unsupported
Native Host caused mutations to refuse. An inert random-event fixture verified
discovery, wrong-registration/PID-generation refusal and exact stop signalling.
Existing Native processes and running distros were unchanged. This is not a
GPU/HDV or full desktop lifecycle acceptance.

The current Windows account is compute; the previous widey r10 import is absent.
Legacy PID 14744 under Program Files was observed but not stopped or adopted.
Native live-status protocol support, abnormal retirement and real distro-based
desktop restart remain pending. Startup still requires the owned r10 candidate
and the explicit pinned runtime. There is no new stable release admission.

Build 01 retained a missing offline Cargo-index failure on the new machine.
The isolated cache was seeded with 23 archives verified against Cargo.lock.
Build 03 exposed a timestamp collision between parallel decoder test directories;
the fixture now adds an atomic sequence. Production decoding behavior is unchanged.
Both failure logs remain archived. No dependency version, license or GUI ABI changed.

## Renderer-close failure and reconciliation, 2026-11-05

Manual renderer close on the final running restart lifetime caused output/stream
failure and native quarantine. PID 7580 remained alive; no native retirement was
confirmed. This does not invalidate the earlier CLI stop/restart results, but
renderer close is now an observed failing path. Core distinguishes retained
failure, unexplained process exit, running-unverified and retired observations;
it no longer requires a CLI stop phase before accepting full retirement evidence.
External distribution termination and WSL shutdown recovery remain unimplemented
in the graphics component and untested. See [exit and recovery](DESKTOP-EXIT-RECOVERY.md).

Build `artifacts/desktop-lifecycle-build-20261105-01` passed 29 top-level native
MSVC Rust tests (including helper tests). All three EXEs have SHA-256
`a8d65eb3aa921bd7cd025a7f0423c3ba3bb8ff014c6471154cd654438a264c91`.
The built CLI diagnosed the actual lifetime as `failed_retained`; check-only
restart exited 1 with the specific retained-resource error. The active pointer
and native process creation identity were unchanged. No stop signal, module
reload or WSL shutdown was executed. The archived native log SHA-256 is
`4db4ee6bf4b93203261df3f59fcf72be9dbb01cd12a4b7a282f3b8f93e9b2106`.
Raw status, verification JSON, stderr and the log are in the build directory.
No new desktop startup, renderer-close repair or external-interruption recovery
is claimed by these diagnostic checks.

## Retired-lifetime restart acceptance, 2026-11-05

The user authorized the narrow normal reload of the retired, unbound, zero-refcount
`wsx_virtio_msg_pci` Owner. Core now verifies the previous native/Guest retirement,
loaded module Build IDs and matching old Owner parameters, reuses the other five
modules, archives the previous pointer and starts a fresh native UUID. No force
unload, WSL restart or other-instance stop was performed.

The final native build is `artifacts/desktop-restart-build-20261105-04`. Its
26 top-level Windows Rust tests and 12 Python tests passed. All three public PE
copies have SHA-256
`27c98b9e9faeb217dedb1e5f69fa74ffd66f6a12e4580f6c82529e4d79a6ff74`.
The real final restart displayed Omarchy, recorded 883 successful opaque frames
at the archived cutoff, and returned the PowerShell caller with code 0 after
16.071 seconds while the desktop stayed alive. CMD status succeeded. A duplicate
start against the live Owner returned code 1 without changing the active pointer.
The current desktop was deliberately left running for user testing.

Evidence: `artifacts/desktop-restart-20261105/summary.json`,
`restart-04-desktop.png`, `restart-04-native.log`, `restart-04-result.json`,
`cmd-status.json`, `duplicate-start-result.json` and the preserved lifetime
receipts. Build 02 also restarted and subsequently retired normally, but exposed
a persistent WSL caller handle leak. Build 03 fixed handle isolation but WSL
rejected over-quoted options with code 127 before the desktop; that native
lifetime was normally retired before retry. Both failures and retirements are
retained. The final fix gives both long-lived child processes explicit standard-
handle inheritance lists and leaves ordinary WSL option tokens unquoted.

Initial no-authorization and read-only checks left the active pointer unchanged.
Busy references, mismatched binaries/parameters, surviving sessions, foreign
descriptors and normal-rmmod failure are covered by simulated Guest tests; these
are separate from the real restart evidence. Graceful input-child exit, apps,
password authentication, hostile replacement races and abnormal recovery remain
unaccepted. No claim is made that the reference closure is a general release
package. See DESKTOP-CANDIDATE.md for the restart commands and boundaries.

## Actual candidate desktop result, 2026-11-04

The compiled Windows `wsld.exe` connected the owned Omarchy r10 import, dynamic
active-adapter identity, pinned native graphics closure and real Guest ABI.
It displayed the Omarchy wallpaper and panel, recorded 1,235 successful
opaque-v1 presentations and matching opaque contracts, with no presentation or
queue command errors in the bounded log. The user then requested normal stop.
The exact Guest device retired, retained memory pages reached zero, native
memory/device/bootstrap closure completed, and both graphics processes exited.
Core reported `native_retirement_confirmed=true`. The owned PCI function is
absent; shared modules remain loaded. No WSL/service restart, module unload,
staging or other-instance stop occurred.

The six reused modules matched all 615 imports against the running kernel's
Build-ID-matched export table and package exports. The earlier 275 unknown
imports are resolved without rebuilding modules or repeating historical GPU
qualification. Current hardware: Iris Xe, driver 31.0.101.5081. RTX reuse remains
supported by the earlier identical-component records, not a new RTX test.

Evidence: `artifacts/desktop-integration-20261104/summary.json`,
`desktop-first-frame.png`, `native-before-stop.log`, `native-after-stop.log`,
`final-status.json`, `remaining-graphics-processes.json` and `lifetime-evidence/`.
The actual desktop ran build `desktop-build-20261104-03`, public PE SHA-256
`6b54c7139834bee5ee66e2352330c8b5efe9faaadf635c9ee28dc60c6b56f044`.
All three names have identical bytes. See [candidate lifecycle](DESKTOP-CANDIDATE.md).

A later Guest status call found its transient `/run/wsx` descriptor missing
after the persistent caller exited and the idle distribution stopped. This was
not used to infer GPU retirement: the Windows receipt and native drain/closure
evidence are retained independently. The systemd caller recorded a successful
session result with normal SIGTERM during the requested stop.

Input, representative apps, password authentication, repeated native lifetimes,
abnormal recovery and sustained stability were not accepted by this bounded
trial. The pinned historical Host contains an input-only `kill` fallback after
GPU retirement; its use was not separately instrumented. Core did not issue any
force-kill. A missing input process alone does not prove graceful input shutdown.

The initial attempt failed before native creation on PowerShell 5 JSON array
wrapping. The fix and safe preflight retry were exercised by the successful run.
The run also exposed inherited caller pipe handles in PowerShell process launch.
The final source uses direct Rust Windows process creation with independent
standard handles; its subprocess regression and final builds are recorded
separately and must not be called another GPU acceptance run.

## Candidate import follow-up, 2026-11-04

An explicit experimental import of wsx-omarchy-4.0.6-r10 completed using the
Windows MSVC Core. WSL reported a version 2 registration named
`WSX-Test-Omarchy-1793804506366543700-23900`. A separate read-only Windows registry
inspection confirmed its base path matches the new owned directory under
`%LOCALAPPDATA%\WSX\instances\candidate-23900-1793804506366543700`. The Core
record deliberately remains `imported_test_path_unverified` because the import transaction did not verify the registry path. Candidate
desktop startup now checks that path automatically, recording it separately. Guest boot, initialization,
desktop, GPU, cancellation and failure recovery were not tested by this import.
The manifest's minimum Core compatibility remains unknown; this result does not
admit a stable release. No existing instance was removed or adopted.

The same test instance was subsequently booted as root using the Windows WSL
control executable. `/etc/os-release` reported Arch Linux, PID 1 reported
`systemd`, the kernel reported `6.18.54.1-microsoft-standard-WSL2`, and the
actual `/usr/bin/wsx-guest --help` reported Guest lifecycle ABI 1. The initial
probe's shell quoting failed and a separate `which` probe failed because the
image lacks `/usr/bin/which`; direct executable inspection succeeded. No user
initialization, guest desktop, input or GPU validation was performed. The
shared `/proc/modules` listing did not show WSX graphics modules at inspection.
The Guest initialization record subsequently reported user `wen`, locale
`en_US.UTF-8`, and timezone `Asia/Shanghai`. An independent `getent passwd` and
`id` invocation confirmed the account and UID 1000 in this instance. Password
authentication, device access and desktop startup remain untested. The current
Windows timezone was `China Standard Time`; the corresponding Guest timezone
file was checked before the interactive initialization.

The current Rust/MSVC product slice builds and has passed Windows GUI
acceptance. This is not complete installation or desktop/GPU acceptance. The original migration run did not perform WSL import or desktop testing; the
later candidate results are recorded above. No publication, existing-desktop
stop, service restart, driver unload or distribution removal ran in this integration.
At the time of this historical integration, Core licensing remained undecided;
the later selection is documented in [LICENSING.md](../LICENSING.md). Pro remains private.

## Delivered source

- Per-repository English instructions, responsibility/architecture/development
  docs, incompatible v2 ABI SDK, migration inventory and lifecycle/deployment ADR.
- Separate Cargo workspaces. Public Rust Core/CLI, actual-path dispatch, bounded
  diagnostic tasks and a fixed-disk/reparse-aware sibling DLL loader. The original
  C source is removed after verified backup; no alternate C build remains.
- Pro wsxpro.dll source: pinned Tauri, minimal Core adapter, local manager UI,
  shared inventory, explicit diagnostic controls, COM STA, run_return, one loop,
  close admission and in-flight drainage. Both normal and acceptance-feature DLLs now build and run on Windows.
- Framework-independent public EXE icon resource 32512. Native icon behavior was verified in real windows without an upstream patch.
- Shared instance index with schema/generation, process locking, atomic replace,
  interruption preservation and immutable existing disk/ownership identity.
  Windows inventory observes WSL separately and never infers desktop readiness.
- Full-ID/exact-version parsing, Omarchy numeric ordering, CachyOS latest refusal,
  local release-index resolution and streaming byte/hash verification of complete
  payloads. Metadata is parsed from the same verified bytes. Resolved metadata
  does not authorize installation or attest publisher signatures.
- Native build/packaging scripts, same-PE hashes, public import/private export
  checks, C layout/import-library fixtures, Windows process/PowerShell/GUI tests,
  verified NTFS package staging and screenshot/child-retirement checks.

## Earlier migration results (historical; see native follow-up below)

| Check | Result and scope |
| --- | --- |
| Original files | All 12 original files re-hashed successfully in workspace backup .handoff-backups/20261102T131940Z; private bytes are outside the public repository |
| Public standalone checkout | cargo test --locked --offline passed with no private sibling; Linux |
| Public tests | 16 top-level tests passed, including one subprocess helper; Linux, not MSVC |
| Private adapter/session tests | 2 passed: init/thread/lease/shutdown guards and close/in-flight drainage; no Tauri construction |
| CLI process tests | 14 passed using Linux ELF copies, including actual-path/argv0 dispatch; not Windows PE/loader tests |
| C header | x64 C layout assertions compiled and ran on Linux with Zig; not native MSVC |
| Windows target | Core, SDK and host metadata type-check passed; no resource compilation, link or execution |
| Native build entry | FAILED at preflight: Visual Studio installation not found; user confirmed MSVC is not installed |
| Full Pro type-check | NOT completed: supplemental Linux cross-check stopped on missing offline atk 0.18.2 dependency; it is not a substitute for the required Windows build |
| SDK / scripts | Shared SDK hash check and PowerShell syntax parsing passed on Windows |
| Real Omarchy archive | 4.0.6-r7, 1,344,970,700 bytes and all four metadata payloads verified locally |
| Real CachyOS archive | 2026.11.1-r2, 2,015,878,334 bytes and all four metadata payloads verified locally |
| New Windows binaries / screenshot | NONE; historical prototype artifacts are not product outputs |

The two actual rootfs manifests both retain minimum_wsx_core_version=null.
Their hashes are in artifacts/local-20261102/real-archive-results.json. Local
archive verification is not a download, decoder, import or GPU result.

## Reproduction and evidence

Public: `cargo test --locked`, `cargo build --locked`. On Windows with Rust
1.95.0, MSVC x64 tools and Windows SDK: `build.cmd -Evidence <new-path>`.
Pro: `build.cmd -Evidence <new-path> -Acceptance`, then scripts/acceptance.ps1
with both build evidence paths. No installer or agreement is automated.

Evidence in this checkout:

- artifacts/local-20261102/final-tests.log and standalone.json/standalone.log.
- artifacts/local-20261102/linux-processes.json (explicitly Linux ELF).
- artifacts/local-20261102/final-windows-metadata.log.
- artifacts/local-20261102/real-archive-results.json and *-verified-final.json.
- artifacts/windows-msvc-preflight-20261102/build.log (actual failure).
- Private repository artifacts/preflight-20261102/final-session-tests.log and
  scripts-and-sdk.log. No private source is copied into public evidence.

artifacts/delivery-20261102.json records source/artifact paths and SHA-256. The
workspace .build-tools directory is a disposable copied development tool/cache,
not a product runtime dependency. Both source repositories remain independently
buildable with their documented toolchain prerequisites.

## Required next work and precise blockers

1. The prepared native build/process/GUI suite has now passed. Extend coverage
   of the remaining cases below; do not treat the suite as full business acceptance.
2. Native Ctrl+C, Explorer/ShellExecute console behavior, clean Windows runtime
   deployment, ancestor-reparse negative execution, renderer crash recovery and
   cross-compiler v2 DLL mixing are not yet accepted. See ADR-001.
3. Explicit pre-release inspection, byte-verified downloads and bounded zstd-to-tar
   decoding have been added; see CANDIDATES.md. The r10 image was downloaded, resumed, verified, decoded and imported in the
   isolated candidate workflow. The new release installer now shares this engine
   with owned import journals and deferred Ctrl+C during WSL import; automatic
   recovery remains unimplemented. See INSTALL.md for current admission.
4. No current public stable rootfs is available; candidate metadata still has
   unknown minimum Core compatibility. Paired desktop runtime admission is separate
   and still requires the Windows host/kernel/input closure. Drafts must stay excluded from ordinary installation. Future CachyOS
   rolling latest ordering remains unresolved.
5. Guest initialization needs a tested account-result/UID contract; current
   initialize emits no structured result. Device ACL/input reconnect and native
   retirement require the paired component delivery and real independent-instance
   tests. No existing distro/desktop may be used as implicit test ownership.
6. The r10 isolated import, initialized-account observation, first frame and
   normal native stop are recorded above. Password authentication, physical
   input, apps, restart and abnormal recovery remain unaccepted. Historical r2
   acceptance stays separate.

## Toolchain installation follow-up, 2026-11-02

The user explicitly authorized installation of MSVC x64 and Windows SDK. The
Microsoft-signed stable Build Tools installer completed with exit code 3010
(success, restart requested). No restart was performed. The selected installed
tools are MSVC 14.51.36231 and Windows SDK 10.0.26100.0, with Rust 1.95.0.
Native Rust x64 compilation/link/execution, MSVC compilation/execution of the
public C ABI layout assertions, and SDK compilation of the public icon resource
all passed before restarting. These are toolchain checks, not full product or GUI
acceptance. The earlier preflight failure above remains historical evidence.

Commands and raw installation/verification logs, installer signature/hash,
selected versions and test binary hash are retained in
artifacts/toolchain-install-20261102/. Run verify.ps1 using Windows PowerShell
with -NoProfile -ExecutionPolicy Bypass to repeat the three checks. This one-off
authorized installation does not add automatic toolchain installation to builds.

## Native product follow-up, 2026-11-02

See [native acceptance](NATIVE-ACCEPTANCE.md) for reproduction, exact hashes and
limitations. Public MSVC tests: 16 passed. Private adapter tests: 2 passed.
24 real Windows process cases passed with the actual DLL adjacent to CLI,
including loader failures, cwd/PATH decoys and argv[0] spoofing. PowerShell
pipeline/redirection/error exit checks passed for both CLI names. Real Tauri
lifecycle and WM_CLOSE tests, child retirement and missing-runtime-path failure
passed. The normal DLL without the acceptance feature passed separate window
creation/screenshot/WM_CLOSE/Core-return and the same 24 process cases.

These are unoptimized development-profile binaries, not a signed installer or
optimized release. No WSL import, account initialization or desktop was performed.

## Final integration build and verification

`artifacts/desktop-build-20261104-05` contains the final MSVC build after the
process-layer correction. Its 26 top-level Rust tests passed (including child
helpers); the detached-owner regression observed caller stdout EOF while the
child was still alive and waited for natural child exit. Five Python audit/parser
tests passed. The final Core recognized the previous real lifetime's full
retirement evidence and rejected retry-after-native and an unowned instance,
both with exit code 1 and no device changes.

Final three-name PE SHA-256:
`003500ad1cb63f9edea3e85167ffb7687de20bc3b7e53944598022e458a674e5`.
The final process-launch correction was not used to start a second GPU lifetime.
Actual desktop acceptance remains tied to build 03 above. The failed PowerShell
fixture and early-exiting DETACHED_PROCESS test are preserved in
`artifacts/desktop-detachment-20261104` and `artifacts/desktop-build-20261104-04`.
See [ADR 002](ADR-002.md) for the resulting contract and remaining work.

Final source/build follow-up: `desktop-restart-build-20261105-05` retains the
successful build-04 startup path and removes Python kill-on-timeout behavior
from module/device operations. Core's bounded observation preserves the caller
and intent if the operation does not finish. This failure-path change passed
26 native Rust and 12 Python tests; it did not require another module reload.
Final identical PE SHA-256: `5f4ad98ce3f53ca52458ee533913d17d78100b643787dd4e3968b2fe9be2289f`. The live
desktop acceptance above remains correctly attributed to build 04.


## Work-area resolution and DPI fitting (2026-11-05)

Implemented in Core (`display.rs`, `desktop_display_windows.rs`), a localized
Win32 adapter (`platform/display.rs`) and an instance-bound Guest control adapter
(`guest/desktop-display.py`). All desktop startup paths fit after the first frame;
`-d|--distribution NAME desktop fit` refits an existing restored window. Current
WSL registration, Guest compositor and Native identity are the authorities.
No user monitor config or graphics runtime binary was changed. ADR 011 records
the advertised-mode policy, clean fractional scales and explicit limits.

Real Windows/MSVC: `artifacts/display-fit-build-20261105-02/build.log` contains
49 passing tests. One public PE was copied to the three entry names, each SHA-256
`04d2f30b87bae271b4d4ba8624f9134b28637a47176141922a48fd36aa60afbf`.
Four separate host-side Python protocol tests pass; these are simulated Guest
control tests, not additional live hardware acceptance.

Real dynamic fit: old Native PID 16972 / Guest compositor PID 287 changed from
784x441 at Guest scale 1.75 to 2048x1152 at scale 1.0 without a Host restart. Windows
reports work area (0,0)-(2560,1392), DPI 96, frame overhead 16x39; final outer
window (248,100)-(2312,1291) fits completely. The advertised common mode with the
largest fitting area is 2048x1152 at 60 Hz. Both endpoints acknowledged it, and
subsequent frames succeeded. Evidence: `artifacts/display-fit-20261105-01`.

The user then authorized normal stop. Actual retirement confirms the exact
Guest instance, close_complete with zero slots/pages, normal input/helper exits
and closed Native memory/device/bootstrap. The checked restart reported an
exact-reference retired Owner module requiring reload. A separate explicit
user authorization allowed only its ordinary checked same-version reload.
No force kill, WSL shutdown, service restart or other distro stop occurred.

Real fresh startup: the final compiled CLI exited 0, created Native PID 22064 /
instance 6038dda3-1073-4936-a477-0bfc864c15bb, and automatically applied the same
2048x1152 / 100% fit. Runtime profile still explicitly selects the 64-page reclaim
watermark. Native/renderer remain the pinned a2485afcee73 diagnostic pair; GPU
and lifetime implementations were not replaced. The bounded captured log has
2596 successful presentations, 94 completed reclaims and no Native entry failure
or map-quota error. A 120-second independent Windows observer finished without
stopping anything; it does not establish GPU health or long-term stability.

The user explicitly confirmed display, text sizing and mouse operation normal
on the fresh desktop. `artifacts/display-start-20261105-01/result.json` links
actual geometry, component identities and tests; `desktop-current.png` is the
unobscured current desktop. The initial startup capture was partly occluded by
another Windows application and is retained separately. The fresh desktop was
left running. No physical high-DPI / mixed-DPI monitor or new Iris Xe test was
performed. These remain distinct from simulated DPI/geometry cases and previous
prototype hardware acceptance.

## Host CPU admission for CachyOS (2026-11-05)

The previous exact r1 install test exited 1 before payload acquisition because
Core rejected every non-null CPU requirement. The actual published CachyOS
manifest requires x86-64-v3; see `docs/CPU-COMPATIBILITY.md` for provenance and
the distinction between explicit release policy and a fresh ELF ISA audit.

Core now checks the complete required CPU feature set, Windows SSE2 state support,
XSAVE/OSXSAVE and XCR0 XMM/YMM state. A bounded native adapter observes every active
logical processor on a short-lived thread; native architecture, pinning and
topology failures reject rather than infer compatibility. No vendor/model branch,
new dependency, feature override, Guest launch or system setting change was added.
Unknown requirements remain errors. Normal release selection checks CPU admission
before downloading the rootfs; acquisition and import repeat it. `--check` reports
its invocation-scoped raw observations. Omarchy without additional CPU requirements
still passes; Guest startup must independently validate its exposed features.

Final build: `artifacts/cpu-admission-build-20261105-03` with 55 passing MSVC tests,
including six new CPU policy/adapter tests. Negative feature/OS cases use synthetic
observations. The Windows adapter test actually probes this machine and verifies
that the calling thread affinity is unchanged. Initial build 01 failed due to the
new machine's empty Cargo cache. Build 02 succeeded after seeding a dedicated NTFS
cache from 23 existing archives checked against Cargo.lock; build 03 removes
unnecessary CPUID unsafe blocks and has no compiler warnings. No dependency
versions or lockfile changed.

Actual machine: Intel Core i5-11320H, eight logical processors. The final compiled
`wsld.exe --install --pre-release wsx-cachyos-plasma-2026.11.4-r1 --check` exited 0,
reported all eight processors compatible and XCR0=231 (0xE7) on each. The exact
Omarchy r11 pre-release check also exited 0, with no additional CPU requirement.
All WSL registrations remained unchanged. These metadata/CPU checks did not
fetch the rootfs payload, import a distro or start a Guest/desktop.

Evidence: `artifacts/cpu-admission-20261105-01/result.json`, captured stdout/stderr,
registration snapshots and the build logs. All three unchanged AMD64 PE copies
have SHA-256 `cfdc28365c6f9549ba17b4a2ba17a3334a00980044a548d80c06bc2d0d8ba716`.
This supersedes the earlier installation note that CPU-feature admission was
unimplemented. It does not claim CachyOS desktop support, a new AMD hardware test
or a re-audit of every ELF in the r1 archive. No commit or push was performed.


## Default install first launch, 2026-11-05

`--install` now enters the shared Core first-launch path after a verified import.
`--no-launch` preserves import-only automation; combining it with `--check` is
rejected. `-d` and `--distribution` resume account initialization, with a fresh
WSL GUID/disk/receipt check and inherited local password input. An initialized
Guest is not reset. Import success and later startup failure are reported separately.
CachyOS ABI 1 account initialization is admitted; its graphics adapter is still
outside the admitted Omarchy r10/r11 desktop path and must fail explicitly.

Completed records whose names and disks no longer appear in WSL registrations
are retained as `unregistered` history. A subsequent install receives a new ID,
directory and registration GUID. Incomplete imports are still refused. The user
manually unregistered the earlier CachyOS test before this change; Core did not
unregister it or delete its VHD, tar or download cache.

Windows/MSVC build `artifacts/install-first-launch-build-20261105-01` passed
59 top-level tests (including synthetic orchestration/ownership tests). Six
compiled CLI option/error cases passed. The three identical PE copies have SHA-256
`0b1422bdbdb11b9da343b7b9dc90e6bcd74ed17276f2ba4315ac20a68cd57c07`.
Live acceptance is recorded separately in `artifacts/install-first-launch-20261105-01`.
Interactive initialization and desktop results must be read from that evidence;
compilation and callback tests do not establish them. No credential transcript is
captured.


The first real default installation reused the cached CachyOS r1 rootfs and
created `install-8600-1793876735555136000`, registration GUID
`{f86675ab-5edd-4aa9-af79-6accc78f0f28}`, without changing previous registrations.
Its first-launch stage exposed a brace-wrapped registry GUID being passed to the
strict mutex API; exit 1 correctly preserved the completed import. The adapter
now normalizes the GUID. This failure and its original binary are retained.

The corrected build is `artifacts/install-first-launch-build-20261105-02/package`,
with 60 passing native MSVC tests and identical PE SHA-256
`0380c5e6052f30ffa2c4ef831856e30f5b6685b9a726863fd5e7eb89dfe02173`.
It adds compressed verification and tar decoding progress. In a real local
terminal, `-d wsx-cachyos-plasma-2026.11.4-r1` completed Guest initialization:
user `wen`, UID 1000, timezone `Asia/Shanghai`, Guest ABI 1. No password transcript
was captured. Desktop startup remains unaccepted for this CachyOS image; first-run
orchestration does not extend the Omarchy graphics admission automatically.

The real decode-progress test exited 0, emitted 121 progress lines, and produced
6,272,327,680 bytes with SHA-256
`be747d1be9b1e239a9ceda0d412b6160d5c372d6c461740f5fe011206dae9c43`,
matching the actual import tar. Only the test's scratch tar was removed afterward.
The initialized-account resume and duplicate-import checks also passed, leaving the
installation index unchanged. Raw evidence is under the first-launch artifact directory.

## Guest protocol boundary, 2026-11-05

[ADR 012](ADR-012.md) records the subsequent CachyOS first-frame/display/input
acceptance and supersedes the earlier unsupported-desktop statements above for
that exact image/pair. [ADR 013](ADR-013.md) records the newer control protocol
implementation: 68 native MSVC tests and 62 Python tests passed. Broad desktop
research is complete without requiring every desktop to be installed for Alpha.
The new Guest protocol pair has not undergone fresh-image GPU/lifecycle acceptance.
Old-image read-only checks and new protocol fixture results are recorded separately.
Unknown Native packages still require a trust update; component delivery and
in-place private graphics upgrades are not completed by this change.
