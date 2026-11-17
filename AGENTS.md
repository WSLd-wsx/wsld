# Engineering instructions

## GUI edition transition (2026-11-13)

The intended product has separately built community and Store editions, each
using one PE copied to the three executable names. The public community GUI
and shared frontend live in wsld; Store-specific authorization and GUI actions
remain in the private wsx-pro repository. The target is Rust crate integration,
not a product dependency on a private DLL. The existing DLL remains the active
entry until the new editions are built and validated. Measure CLI process loads,
console behavior and exit codes before accepting a crate-based replacement;
do not claim that static GUI dependencies are absent without evidence.

The planned wsld release license is BSL 1.1. The current repository license
remains in force until its specific terms and transition are implemented; retain
all upstream notices. wsx-pro remains private.

## Collaborative workspace scope (2026-11-05)

The user authorizes coordinated local changes in all four sibling repositories
under the `/mnt/wsl/u/wsx` workspace: `wsld`, `wsx-rootfs`, `wsx-pro` and
`wsx-proto`. This supersedes earlier read-only or narrower repository-edit scope
in this session. Before changing a repository, follow its own `AGENTS.md` and
any applicable nested instructions. Keep repository responsibilities, license
boundaries, independent builds and existing unrelated changes intact. Reuse and
trace verified prototype work; this authorization does not make the prototype
a product runtime dependency. Do not create a workspace-level `AGENTS.md`.

This expands local editing scope only. It grants no new authorization for
commits, pushes, publication, remote workflows, license changes, destructive
operations or disruption of existing desktops/services. Existing explicit
authorizations retain their original scope.

Use Rust 1.95.0 and Windows x64 MSVC as the product baseline. Favor Vibe Coding:
ordinary modules, concrete types, few necessary dependencies and diagnostic errors.
Keep unsafe code in small ABI and Windows adapters. Core is an internal Rust API,
not RPC, a service, or a general plugin framework.

Build one public PE and copy it unchanged as wsld.exe, wsx.exe and wsxdesk.exe.
Dispatch on the actual executable path. CLI calls Core directly and never loads
Pro, Tauri or WebView2. Only wsxdesk loads the absolute sibling wsxpro.dll; no
alternate DLL names, cwd/PATH fallback, mode switches or separate GUI executable.
Preserve fixed local disk admission and restricted dependency search.

The public SDK in wsld owns the versioned C boundary. Pro consumes a checked
snapshot or controlled relative dependency. Never pass Rust object layouts or
allocator ownership across modules. Buffers belong to callers; task handles are
process-local, bounded and invalid after release. Close admission, drain calls,
cancel at safe points and join workers before host state is retired. Catch Rust
panics at FFI; this cannot recover invalid pointers, SEH or process abort.

Load and run Tauri on the main OS thread with COM STA and run_return. Permit
one GUI event loop per process. Do not expose restart. Retain the DLL until
process exit on success AND error paths; FreeLibrary is not proven safe.
Use bundled frontend assets, minimal permissions and restricted navigation.
Missing WebView2 must fail without automatic installation.

CLI and GUI share %LocalAppData%\WSX data and one Core implementation. Persist
schema-versioned business facts with atomic replacement and cross-process locking;
do not persist memory-task handles or pretend a later CLI owns an exited task.
WSL registration is authoritative for existence; local records establish WSX
ownership and provenance, not proof that a desktop is running. Never overwrite
existing disks during upgrade or adopt unowned distributions. Passwords use a
non-echoed controlled input channel, never arguments, environment or logs.
Cancellation must respect irreversible-operation safe points. Preserve uncertain
state and diagnostics for recovery; never manufacture success or GPU retirement.

Daily desktop commands are addressed by the WSL distro name with
`-d DISTRO_NAME` or `--distribution DISTRO_NAME` (default desktop launch), with
`desktop status|stop|fit|start|restart-check` for explicit operations. DISTRO_NAME is the NAME column shown by `wsl.exe -l -v`,
without the default marker, state or version. Public output uses `distro_name` for
that name and explicitly labels internal identities as `registration_guid` or
`installation_id`. Resolve the actual WSL registration identity on each
invocation. Query WSL, the existing Guest session and the corresponding live
Native Host; do not treat a stored startup phase as current health. Status must
not depend on the installation index or active-lifetime pointer. Keep installation
ownership and interrupted-operation journals until their recovery guarantees can
be replaced. Mark legacy Host capabilities and unverified observations explicitly.
Do not adopt a same-name reimport or wake a known stopped distro for status.
The independent Native Host has no custom WSLService plugin dependency. Windows
restart is not the only recovery boundary: after current successful HCS discovery
excludes the old VM, with ended old Host identity, no other graphics owners and a different current VM,
permit separately recorded fresh admission only after clean Guest checks. Historical
failure logs and manually written recovery receipts must not lock out that fresh
start. Current HCS, Guest and Host observations are the running-state authorities. Keep
normal retirement false and preserve the failed lifetime. Never recycle retained
modules or backing under this route, or infer GPU drain from a killed process.

Release installation uses `--install ID[-VERSION]` for public stable images and
`--install --pre-release ID-VERSION` for exact public test images. Never fall back
between channels or explicit versions. Share the verified cache across channels;
rehash existing payloads, retain resumable partials, and never infer success from
file existence. Stable minimum Core/acceptance metadata must be known. Import is
not account initialization or desktop/GPU acceptance. Never overwrite or adopt an
existing distro name; preserve interrupted import ownership and diagnostics.

All four repositories may change under the collaborative workspace scope above.
Core must build independently of the private repository; use separate Cargo
workspaces. Pro remains proprietary.
When migrating verified prototype work, carry over the selected runtime profile
and optional defaults together with the pinned source/binary closure. Cross-check
actual logs and hashes against docs/PROTOTYPE-MIGRATION-CHECKLIST.md. Do not replace
verified configuration with executable defaults or repeat solved work without
checking the handoff. Keep historical acceptance distinct from current product tests.
Rootfs owns image construction, Guest userspace and lifecycle interfaces; Core
owns resolution, verification, import and host lifecycle/components. No prototype
absolute paths or developer deployments may be runtime/build dependencies.

Separate real Windows evidence, simulated tests, static checks and historical
observations. Diagnostic counting tasks are never installation or desktop success.
Unknown release/component compatibility must reject, without inventing versions.
Do not restart WSL/Windows/services, stop unrelated desktops/owners,
or destroy distributions. Staging development is isolated and remains disabled
until verified admission; no MSI, signing, billing or activation work.
Do not unregister distributions or delete VHDs automatically. Preserve unrelated
changes. Back up original untracked bytes before replacing them.

Communicate with the user in Chinese; new code, comments, errors, documentation
and commit messages use English. Commit and push only with explicit user
authorization. The user authorized the current product repositories to be committed
and pushed as WGY <wgy@wsxdesk.com>. This does not authorize release publication,
remote release workflows or acceptance of commercial terms.
The user selected Business Source License 1.1 for original wsld material,
as specified in LICENSE; previously granted and third-party licenses remain applicable. WGY (GitHub: wgy-wsx) <wgy@wsxdesk.com> is the copyright holder. Preserve the
separately licensed integration modules, upstream dependencies and Unicode data
as documented in LICENSING.md. Further license changes require user authorization.
Pro grants no new license.

## Repository responsibility

Own the public executable, internal Core, authoritative ABI SDK, host resources and Windows loader. Never depend on private GUI sources or Tauri.

Core and Guest must evolve independently through the versioned public Guest
protocol. New image admission is driven by verified release compatibility data,
not an image tag allowlist. Never call Guest Python internals from Core. Keep
legacy adapters isolated; declared protocol errors never downgrade to legacy.

Single-distro view-v1 keeps one Native Owner and Guest desktop alive when WM_CLOSE
hides the retained view. Repeated `wsld -d DISTRO` must negotiate and restore that
same process/window, never create a second desktop. View absence is not session
exit or GPU retirement. Explicit desktop stop and WSL termination still require
full retirement. Do not claim detached rendering is suspended or surfaces freed.

Multi-distro concurrency requires the pinned PCI Owner configuration ABI 2.
Keep shared modules loaded and configure/retire exact devices independently.
Preserve per-registration desktop exclusivity, exact VMBus ancestry and per-view
input focus. Never remove another distro's device or unload a referenced shared
module to start a desktop. Module migration and live concurrency acceptance are
separate from build success; preserve ABI 1 rejection for unqualified runtimes.

The product graphics direction is opaque-v1 zero-copy with automatic staging-copy
fallback and the Linaro-derived virtio-msg Host peer on both paths. Select one
verified graphics path per desktop Owner lifetime. Keep the opaque-v1 Host and
renderer independent of staging code. The staging path uses a separately built,
paired Host and renderer for that desktop, not a
second concurrent Owner or a business sidecar. Derive that pair from the
verified zero-copy Host, renderer and retirement implementation; reuse the
prototype's staging transfer only where its contract is verified. Compile the
same source for shared functionality where practical; avoid drifting copies and
keep staging-only work out of the zero-copy hot path. Process isolation does not prove
GPU completion or device retirement. Do not enable staging until its complete
component closure, capabilities, lifecycle and real desktop acceptance are
verified; this product decision does not itself establish that admission.
Unknown capability and failed operations must not be reclassified as unsupported
hardware. Uncertain previous lifetimes cannot be replaced by assuming success.
Preserve earlier experiments and evidence until their replacement is verified.

## Authorized bounded WSX Owner replacement

The user authorizes ordinary unload/reload or ABI migration of
wsx_virtio_msg_pci without repeated confirmation when fresh checks establish
that its exact WSX lifetime retired, no Native/input Owner is active, no PCI
function is bound, module reference count is zero and the holders directory is
empty. Verify the replacement component closure, kernel/symbol compatibility
and preserve diagnostics/rollback bytes before mutation. Repeat the checks at
the mutation boundary. Do not force unload, unload the other five shared modules,
replace a referenced Owner, or retire another distro's resources. ABI 2 remains
loaded across normal desktop starts/stops; do not unload it while any peer is
active. Failed or ambiguous checks stop the replacement with diagnostic evidence.
This authorization does not permit WSL/Windows/service restarts, arbitrary
module removal, distro deletion or concealed recovery by process termination.

ABI 2 concurrency also requires Guest capabilities graphics.distro-device-view-v1
and graphics.distro-input-view-v1 on every participating distro before desktop startup. Published older images
need a new rootfs revision or an explicit recorded test overlay. Global DRM
visibility can let one compositor seize a peer device as master despite a
compositor device environment variable. Never weaken DRM_MASTER/ownership checks.
First-frame evidence does not establish input or stability; presentation failures
must remain visible in current status after an initial successful launch.
