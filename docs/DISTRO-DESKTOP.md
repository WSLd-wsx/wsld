# First-launch integration update, 2026-11-05

Default `--install` now follows import with the same first-launch flow as
`-d DISTRO_NAME`. `--no-launch` opts out. Release account initialization is shared
across ABI 1 images and precedes graphics-runtime admission; passwords remain in
the inherited local terminal. Completed accounts are observed, never reset.
Failures retain installation and allow retry through `-d`. The graphics adapter now also admits the exact CachyOS Plasma pre-release and
paired runtime described in [ADR 012](ADR-012.md). The historical pre-initialization rejection described below is
superseded for default launch. See [INSTALL.md](INSTALL.md).

# Distro-addressed desktop commands

Fresh startup uses current HCS/Host observations and clean Guest checks, without
requiring a previous failure log to say "retired" or a manually written recovery
receipt. See [ADR 009](ADR-009.md). Old failed lifetimes remain diagnostic evidence.

Current stability regression: the next post-shutdown r11 desktop initially worked
but later stopped responding together with WSL-hosted commands. See
[ADR 008](ADR-008.md). Do not interpret the successful first frame or bounded
shutdown below as sustained restart acceptance.

Latest actual acceptance: [ADR 007](ADR-007.md) records successful fresh admission
without Windows restart and complete Omarchy `wsl --shutdown` retirement on the
new pair. Native/input exited 0, retained memory reached zero, both helpers exited,
and Core confirmed `native_retired`. Evidence is
`artifacts/shutdown-acceptance-20261105-03`. This supersedes earlier pending-shutdown
statements below; it does not certify subsequent restart or all race scenarios.


Current follow-up: [ADR 006](ADR-006.md) implements exited-VM aperture
reconciliation and pre-binds both retirement helpers. Isolated Windows API and
regression tests pass; full new-pair GPU desktop acceptance remains pending.
Use `reference-565b64806b7f` with the new public package. Earlier commands, hashes
and real desktop results below describe historical builds. A retained failed Host
still requires separate recovery; the new binary does not alter that process.


```powershell
wsld.exe -d wsx-omarchy-4.0.6-r11
wsld.exe --distribution wsx-omarchy-4.0.6-r11
wsld.exe -d DISTRO_NAME desktop status
wsld.exe -d DISTRO_NAME desktop stop
wsld.exe -d DISTRO_NAME desktop restart-check --reference-runtime DIRECTORY
wsld.exe -d DISTRO_NAME desktop start --reference-runtime DIRECTORY
wsld.exe -d DISTRO_NAME desktop start --reference-runtime DIRECTORY --reload-retired-owner
```

`--distribution` is an alias for `-d`. DISTRO_NAME is the WSL distro name, exactly the NAME column shown
by `wsl.exe -l -v`, for example `Omarchy` or `WSX-Omarchy-R7-RTX`. Do not include
the default marker (`*`), STATE or VERSION columns. Quote a name containing spaces.
No release-ID, candidate-ID or GUID lookup is attempted as an alternative.
Status returns that canonical name in `distro_name` and `distribution.name`; the
internal GUID is explicitly named `registration_guid`. Startup diagnostics label
the internal ownership record as `installation_id`. Existing on-disk journal
schemas and legacy candidate commands remain compatible. With no subcommand (or
`desktop start` without a runtime argument), startup uses the versioned cache
`%LOCALAPPDATA%\WSX\cache\graphics\reference-093dac80ac0d`. This is the paired window-close/external-lifetime repair of the experimental r7 runtime.
The input binary, six modules and Guest interfaces remain unchanged. This is
not a stable component-distribution claim.
No prototype checkout or developer deployment is searched at runtime.
The old candidate-ID commands remain available for legacy diagnostics.

## State authorities

Each invocation resolves the actual HKCU WSL registration GUID, name, disk and
WSL version, and queries WSL for running distributions. It discovers current
Native Host processes from Windows, including executable path, creation time,
VM UUID and instance UUID from the real command line. It verifies the admitted
native binary hash and adjacent runtime profile before associating a process
with the selected distribution. Unknown or inaccessible processes are reported
as unresolved ownership, not absence.

Status reads neither `instances.json`, the active desktop pointer nor historical
`state.json`. It does not rewrite any of them. Query scripts and raw output are
kept under `WSX/diagnostics/desktop-query-*`; these are diagnostics and are never
used as future running-state authority. There is no persistent Core daemon or
cross-invocation in-memory task.

The reference Host has only a named stop event, not a live status protocol.
`supports_live_status_protocol` is therefore false. Event availability and
process/window presence are actual Windows observations; known retained-failure
markers still come from its legacy log. No process/window observation is labelled
healthy rendering or GPU retirement. Production retirement remains conservative.

If WSL reports the distro running, a read-only Guest helper queries the trusted
initialization record and actual account, then the exact Native-instance session
descriptor and systemd unit. It never calls prepare/start/stop or loads modules.
Missing initialization is reported as missing, and failed queries as failed.
With no unique verified Host, it does not guess a Guest instance from old files.
Already-stopped distros skip Guest execution. A second running-list check narrows
the race, but an external termination between that check and `wsl --exec` can
still cause WSL to restart the distro: this adapter has no atomic existing-only
execution API. This limitation is not claimed solved.

## Mutation and ownership

Start and stop serialize with a process-lifetime Windows mutex keyed by the real
registration GUID and re-observe identity after locking. Stop rechecks process
creation time, executable, VM/instance command-line identity and registration
GUID immediately before signalling only the matching named event. It writes no
persisted stop/running phase. Repeated stop without an observed Host is a no-op
with retirement explicitly unconfirmed. Known quarantine, ambiguous ownership,
missing VM or a stopped Guest with a retained Host refuses recovery.

New startups record immutable `registration.json` alongside the native runtime
configuration. This binds that lifetime to the actual registration GUID and disk;
a same-name reimport cannot inherit that binding. Existing lifetimes without it
are visible as `legacy_unverified` but are not adopted for distro-addressed stop.
Legacy candidate controls retain their old, narrower receipt-based behavior.
The runtime directory and files remain subject to same-user replacement races;
there is no new publisher signature or hostile-user isolation claim.

Startup resolves the selected registration against the installation journal's
ownership and actual disk path, then uses the existing Core startup implementation.
The owned r10 candidate and r11 pre-release import are admitted. This is intentionally not
permission to adopt arbitrary pre-existing Omarchy/CachyOS installations.
Installation journals, device-creation intents and previous retirement receipts
still gate startup/reload. The active pointer remains part of that legacy startup
transaction; this change removes it from observation/control, not from recovery.
Shared kernel/VM resources also retain the existing single-graphics-owner limit.

## Remaining work

A paired Native Host with a queryable lifecycle result is needed to replace
legacy log diagnosis and provide complete abnormal retirement. The renderer-close
repair now passes full r11 window-close retirement with the paired Native and
renderer builds. Exact r11 external termination now passes real retirement acceptance. The
original-VM-exit branch observed the real VM exit, but shutdown acceptance failed
Guest RAM lease closure with `0x80070006`; Native remains quarantined (ADR 005).
A strictly newer Windows boot permits fresh admission only after the old Host
and VM are absent and Guest checks reject residual ownership; this is not proof
of normal retirement. Status does not repair
quarantine or turn missing processes into retirement proof.

The initial distro-command tests used a different Windows user/registration set
from the earlier r10 machine. Their inert event fixtures are separate from the
subsequent real r11 GPU startup/retirement acceptance. No historical distro was
taken over to fill that test gap.

## Validation and delivery

Historical naming-aligned package: `artifacts/distro-name-build-20261105-03/package`.
It passed 33 MSVC Rust tests and the ten Windows cases in
`artifacts/distro-name-acceptance-20261105-02`, with `distro_name` checked against
the actual WSL names. The WSL verbose listing is archived alongside those results.
The earlier implementation evidence below remains historical.

Build: `artifacts/distro-desktop-build-20261105-04/package`.
Command: `scripts/build-desktop-incremental.ps1 -Toolchain RUST_DIRECTORY -Evidence NEW_DIRECTORY`.
Native MSVC Rust tests: 33 passed. Python Guest/kernel/module tests: 16 passed.
The initial missing-cache failure and later pre-existing decoder-fixture timestamp
collision are retained in builds 01 and 03; the fixture collision was corrected.

Run `scripts/test-distro-desktop.ps1 -Package PACKAGE -Evidence NEW_DIRECTORY
-Toolchain RUST_DIRECTORY -RunningDistro EXISTING_RUNNING_NAME -StoppedDistro
EXISTING_STOPPED_NAME` for the bounded Windows acceptance. It creates its own
corrupt-index directory and inert event process, never a graphics device. The
2026-11-05 run passed all ten cases and preserved both the running distro set and
all pre-existing Native process identities. The attempted start was refused by
the unresolved existing-owner guard before installation ownership checks; ownership
resolution itself is separately unit-tested. Full desktop startup is not claimed.

## First launch and r11 binding

The new entry reuses `start_with_registration`, existing VMBus device discovery,
module CRC admission, Guest prepare/start, native readiness, first-frame detection
and guarded retirement. It does not create a second graphics engine. Legacy r10
ownership remains admitted. r11 requires its pre-release ownership record and
`wsx-import.json` to match the current WSL registration GUID, disk, full tag and
verified archive SHA-256. Same-name reimports and changed image receipts reject.
The legacy candidate-ID entry also enforces this r11 receipt before boot.

After ownership/foreign-Host/runtime checks, Core observes the real Guest account.
On first launch it requires an interactive local terminal, obtains the Windows
IANA timezone through the existing Core mapping, prompts for a Linux username,
and calls the existing `wsx-guest initialize` with inherited console handles.
The Guest owns non-echoed password entry and confirmation. Passwords never enter
Core arguments, environment, diagnostic capture or stored state. Core verifies the
resulting non-root UID, ABI, username and timezone before continuing. Redirected
input refuses initialization. A failed initializer retains its Guest pending state;
Core does not invent defaults or automatically repair it. This console interaction
still needs real user completion; no test password or account was created.

On the current machine an older `--owner` Native Host PID 14744 is held by the
custom WSLService PID 3936. It lacks the admitted independent direct-Host binding.
Both new aliases currently refuse before initializing an account or creating a
new Host. The service and existing process were not stopped or changed. Stopping
that WSL service may affect every running distro, including the development
workspace; no such operation is implicit in desktop launch. Resolving that old
environment's lifecycle is required before a new first-frame acceptance.

Current code/build and live refusal evidence are recorded in STATUS.md. The
historical successful r10 GPU run remains valid for its recorded scope; the r11
component/ABI checks do not inherit a new successful desktop run from it.

## Current window-close delivery, 2026-11-05

Use `artifacts/window-close-build-20261105-01/package` with the paired versioned
cache above. Windows/MSVC passed 43 tests. Full r11 first-frame and WM_CLOSE
retirement passed in `artifacts/window-close-live-20261105-01`; the legacy runtime
and a mixed Native/renderer bundle were refused by actual CLI invocations.
The post-close restart preflight reports normal retirement and a required explicit
Owner reload; this run did not unload the module or start a second lifetime.

Distro status still reports current WSL/Guest/process observations, without reading
historical active pointers. Consequently an absent Host does not set historical
`native_retirement_confirmed` to true. The archived exact-lifetime result and legacy
candidate diagnostic independently record the verified normal close. See
[ADR 004](ADR-004.md) and [exit/recovery](DESKTOP-EXIT-RECOVERY.md).

## Current external-lifetime delivery

The current package is `artifacts/external-lifetime-build-20261105-03/package`.
It requires the `reference-093dac80ac0d` cache above and passed 44 MSVC tests.
Two real r11 `wsl --terminate` tests passed, including the final pre-bound system
helper design. r11 remains stopped, Ubuntu remains running, and all registration
identities were unchanged in those tests. Subsequent real `wsl --shutdown`
acceptance failed native memory retirement; see [ADR 005](ADR-005.md)
for exact evidence and remaining VM-loss/concurrent-stop limits. Earlier build
names and test counts on this page describe their original historical sessions.

## Display fitting

Startup now fits the Guest desktop and renderer to the current monitor work area
and DPI after the first frame. To apply the same policy to an already running,
restored window (including after moving it to another monitor):

```powershell
.\wsld.exe -d wsx-omarchy-4.0.6-r11 desktop fit
```

`--distribution` is an alias for `-d`. The command reads live registration, Host
and Guest identity, selects an advertised common mode, and verifies physical
client extent and Guest scale. The work area excludes the Windows taskbar;
caption/border metrics come from the actual window's DPI/styles. No user config
is overwritten. Diagnostics contain intended and actual geometry. See ADR-011.md
for single-output, manual-refit and scale-rounding limits.

## Compatibility admission update

The historical r10/r11-only startup descriptions above are superseded by
[ADR 013](ADR-013.md). New import receipts retain verified release compatibility
metadata; known legacy image/component exceptions remain explicit. Daily commands
continue using the actual WSL distro name. `desktop compatibility` is read-only
for the Guest and skips Guest queries for a stopped distro; it verifies the paired
Host bundle but does not launch it. Core no longer imports a Guest Python function.
New protocol images call the public Guest boundary; existing images use the bounded
legacy adapter. New image tags do not automatically establish graphics compatibility.

## WSX-Service

WSX-Service is the dedicated WSL 2 distribution for shared desktop services.
The desktop launcher discovers it by its WSL registration name. Audio is its
first service. Clipboard integration is planned.

The Community Windows ZIP includes a minimal Debian image. On desktop
startup, Core imports and verifies the image if WSX-Service is missing.
The expected image hash is embedded in the Community executable. The
corresponding Debian sources are available in the release's materials ZIP.

Audio requires WSLg, Python 3.9 or later, and a non-root service user whose
numeric UID matches the desktop user. The Community image uses UID 1000. The
desktop Guest must advertise `audio.pulse-relay-v1`. The launcher starts the
relay by registration GUID and records its status in the session's
`audio.json`. Desktop startup can continue if the relay is unavailable.

To bind the launcher to a specific WSX-Service registration, set
`provider_registration_guid` in `<WSX data root>/config/wsx-service.json`.
The GUID must match the WSL 2 registration named WSX-Service. The service
registration must be separate from the desktop Guest registration.

Core exposes the service lifecycle through the CLI:

- `wsld.exe service install IMAGE --sha256 SHA256` verifies a trusted local
  root filesystem archive, imports it into Core's service directory, then
  checks the default user and the WSLg Pulse socket. Supply the digest from
  the same trusted release as the archive.
- `wsld.exe service status` reports registration, ownership, running state,
  and pending installation or upgrade.
- `wsld.exe service verify` checks the installed service and clears a pending
  installation when verification passes.
- `wsld.exe service upgrade` upgrades an idle Core-managed service in place
  using its configured APT repositories, then verifies it again. An interrupted
  or failed upgrade disables new audio relays until the upgrade is retried and
  verification passes. Upgrade output is retained under the data root's
  `service` directory.

The image must include APT and dpkg with configured signed repositories, in
addition to the runtime prerequisites above. Import and upgrade keep
service operations scoped to the current Windows user.
