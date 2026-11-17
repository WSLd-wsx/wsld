# Candidate desktop integration

This entry connects the existing owned Omarchy r10 import to the hash-pinned
r7 reference graphics Host. It is an explicit local candidate test, not stable
release admission or a generally distributable graphics package. The same native
Host, renderer and kernel modules were used by the historical Iris/RTX trials;
there is no GPU-brand branch and no graphics rewrite.

## Commands

Daily commands now use `-d DISTRO desktop ...`; see
[distro desktop commands](DISTRO-DESKTOP.md). The candidate-ID forms below remain
legacy diagnostics and document the retained startup/recovery implementation.

Build using `build.cmd -Evidence NEW_DIRECTORY`. During this integration,
`scripts/build-desktop-incremental.ps1 -Toolchain RUST_DIRECTORY -Evidence NEW_DIRECTORY`
runs the native Rust tests and copies the one PE three times, verifying hashes.
Stage an existing verified reference closure with
`scripts/stage-reference-graphics.ps1 -Reference REFERENCE_DIRECTORY -IdentityTools TOOL_DIRECTORY -Destination NEW_LOCAL_DIRECTORY`.
The Core pins the receipt and both identity-tool digests, verifies the staged
bytes, and copies them into a new lifetime under `%LOCALAPPDATA%\WSX\desktop`.
Neither build nor runtime resolves a prototype checkout or a developer path.
The reference files must be supplied explicitly; this is not a release downloader.

```powershell
wsld.exe candidate desktop-start INSTANCE_ID --reference-runtime DIRECTORY
wsld.exe candidate desktop-status INSTANCE_ID
wsld.exe candidate desktop-stop INSTANCE_ID
```

`INSTANCE_ID` is the Core index ID, not an arbitrary WSL registration. Startup
requires the recorded candidate ownership, Omarchy 4.0.6-r10, WSL 2 and an exact
registry disk-path match. It does not initialize accounts or handle credentials.

`candidate desktop-retry INSTANCE_ID --reference-runtime DIRECTORY` is restricted
to failed preflight attempts: no native creation intent or receipt may exist,
and every recorded preflight child must have exited. It archives the old active
pointer and preserves every old diagnostic file. Native/Guest side-effect
failures never automatically retry or clean up.

## Ownership and flow

The shared Core store lock serializes startup, status and stop. It is held
through startup; other CLI operations receive a busy error while it is held.
Each lifetime has an exclusively created active pointer, configuration, observed
identities, phase record, process receipts and stdout/stderr. A retained pointer
blocks another start even if a process is missing. No background business daemon
or persistent in-memory task is implied.

1. Verify reference hashes, local fixed volume and path/reparse admission.
2. Observe the WSL registry, current HCS VM and active display/Vulkan LUID join.
3. Reject existing graphics Hosts, shared candidate modules, seat or descriptors.
4. Verify initialized account, actual Guest component hashes/capabilities and
   running-kernel export CRCs. Preserve the image's Guest ABI and runtime.
5. Create a fresh Host UUID and port pair, write its adjacent native profile,
   and launch the pinned independent graphics Host with the direct entry. The
   Windows adapter creates a separate process group with no shared console and
   independent stdin/stdout/stderr, preserving caller pipeline completion.
6. Wait for actual HDV Start, bind only the matching VMBus/PCI device, call
   `wsx-guest prepare` and the byte-identical historical DRM ACL adapter.
7. Start the actual Guest session through `wsx-guest start`; retain its WSL
   caller and wait for a successful native presentation log before reporting
   `first_frame_observed`. This does not establish input, apps or stability.

Stop checks native PID, creation time and executable identity, then signals the
existing exact-instance event. The native component owns its already-verified
Guest retirement, queue service, GPU/memory drain and HCS removal sequence.
Status requires process exit, exact-instance Guest retirement, zero retained
memory pages and the explicit final native closure message. Core never force-kills or requests module unload, WSL/service restart or
unregister. The pinned Host has a historical input-only kill fallback after
GPU drain; the bounded test did not instrument that child exit reason. Loaded owner modules and Guest descriptors remain retained;
a fresh native lifetime requires the explicit guarded restart flow below. Unit exit or
window disappearance alone is not GPU retirement evidence.

## Kernel compatibility and reuse

The reported prior same-major WSL compatibility is a reuse baseline, not a reason
to rebuild these modules. `kernel-exports.py` reads the installed kernel ELF (or
gzip bzImage), compares its GNU Build ID with the running Guest's
`/sys/kernel/notes`, and reads PREL32 exports plus CRC tables. It compares all
six modules' imports against that actual kernel and the package's own exports.
It permits a differing full release spelling within the same major family,
requires complete CRC agreement, and never forces vermagic/modversions.
Unsupported image formats, changed Build IDs and missing symbols fail explicitly.
The kernel loader remains authoritative when modules are inserted.

The live 2026-11-04 audit found 12,107 kernel exports and matched all 615 module
imports, including the previously unobserved 275 imports. There were zero
missing imports and zero CRC conflicts. Kernel Build ID:
`de17a562c219da52bb9bb56182d657b775f45676`.
Evidence: `artifacts/desktop-integration-20261104/kernel-exports.json`.
This resolves the symbol-observation gap; it is not a new GPU qualification.

## Current limits

The reference package lacks a normal versioned download/rebuild distribution
contract. Its exact historical hashes are the candidate admission boundary.
The Windows discovery helper uses PowerShell and stock HCS tooling; the Guest
helper uses Python already present in the image. No host Python, Node or gh is
required. The current default kernel location may reject a custom installation;
Build-ID checks prevent silently using a different installed kernel.

Random port selection is followed by the native exclusive bind. A collision
fails and retains state; there is no takeover or transport fallback. Checks do
not defeat hostile same-user replacement races or writable-directory attacks,
and no publisher signature is claimed. Persistent orphan reconciliation, complete ACL revocation and abnormal recovery
remain separate work. Retired native lifetimes use the explicit flow below. The product GUI has not yet been wired to these candidate controls.

See ROOTFS-HANDOFF.md and STATUS.md for actual results, including failures.
See [exit and recovery](DESKTOP-EXIT-RECOVERY.md) for renderer close, external
distribution termination and WSL shutdown. Renderer close passes in the
new paired component; the legacy component retains its recorded failure.
See ADR-004.md for the exact build and real r11 retirement evidence.

## Bounded real result

The original connected run (build desktop-build-20261104-03) displayed the actual
wallpaper/panel and recorded 1,235 successful opaque presentations. Requested
normal stop completed native/Guest retirement with zero retained pages and no
remaining graphics processes. The subsequent direct Rust process-launch fix is
validated by a harmless Windows subprocess fixture, not another GPU run. Failed
JSON wrapping and PowerShell pipe-detachment observations remain archived.

## Restart after confirmed retirement

The new candidate flow reuses a retired lifetime only after re-reading its
native receipt, process identity, exact-instance Guest retirement, zero retained
memory and final native closure. The previous profile and prepared Guest receipt
must belong to the same Core-owned registration. A running or uncertain lifetime
is not automatically stopped or adopted.

```powershell
wsld.exe candidate desktop-restart-check INSTANCE_ID --reference-runtime DIRECTORY
wsld.exe candidate desktop-start INSTANCE_ID --reference-runtime DIRECTORY --reload-retired-owner
```

The check command stages code and diagnostics and observes Windows/Guest state;
it never replaces the active pointer, unloads a module or creates a device.
It reports whether the retired Owner needs normal reload. Without the explicit
flag, startup refuses that step and retains the current pointer unchanged.
When no Owner module remains, startup does not need the reload flag.

Existing modules are admitted by their live sysfs GNU build notes compared with
the six hash-pinned ELF files, plus the running-kernel CRC audit. The other five
modules remain loaded. The previous Owner must have zero references, no holders,
no bound PCI function, no surviving VMBus device or tagged Guest session, and
parameters matching the previous UUID, BDF, port and aperture. Any seat or foreign
Guest descriptor blocks restart. An exact trusted retired descriptor, if still
present, is preserved in the Guest's `/var/lib/wsx/core-retired` archive.

Reload uses precisely `/usr/bin/rmmod wsx_virtio_msg_pci`, without force flags,
and later the already-verified insmod/bind sequence with a fresh native UUID.
The kernel rejects a racing reference. The active pointer is atomically replaced
only after admission and explicit authorization; its previous value and lifetime
receipts remain archived. An owner-recycle intent is written before any module
operation, and prevents the pre-native retry command from hiding interrupted
reconfiguration. No shared five-module unload, WSL/service restart, existing
instance change or prototype-source modification occurs.

Actual module reload remains subject to the operator's explicit execution
permission for this session; the flag is not supplied automatically by Core.

The Windows graphics Host and persistent Guest caller both use an explicit
three-handle inheritance list (stdin/stdout/stderr only). Regular option tokens
remain unquoted for WSL raw-tail parsing; arguments containing spaces or quotes
use Windows escaping. This preserves caller EOF without terminating either Owner.

### Verified invocation after retirement

Use the latest build listed in STATUS.md. Request `candidate desktop-stop ID`,
then poll `candidate desktop-status ID` until `native_retirement_confirmed` is
true. The stop command only requests retirement. Then execute:

```powershell
wsld.exe candidate desktop-start INSTANCE_ID --reference-runtime DIRECTORY --reload-retired-owner
```

This flag permits only the exact retired Owner to be normally reloaded; a live
Owner still rejects. The user authorized and the 2026-11-05 final build verified
this flow on the owned r10 instance, including return to PowerShell while the
desktop remained running and CMD status execution. The other five modules were
reused, not rebuilt or unloaded. A signature/source delivery scheme and general
abnormal-recovery policy are still outside this bounded candidate implementation.
