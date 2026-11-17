# Rootfs integration admission

The current graphics/profile migration cross-check is
[PROTOTYPE-MIGRATION-CHECKLIST.md](PROTOTYPE-MIGRATION-CHECKLIST.md). The 2026-11-05
interactive quota failure was a host-side omission of the already verified 64/32
CPU aperture reclaim profile; no rootfs change is required for that correction.
Historical component/session checkpoints below retain their original scope.

Read-only review on 2026-11-02 covered release-protocol, Guest ABI, ownership,
component package/admission, build evidence and CachyOS runtime handoffs, plus
DIRECT-HOST and ROOTFS-HANDOFF in the reference repository.

Release/Guest prose saying no candidate exists or all hardware is not_run is a
historical checkpoint. CachyOS r2 build 2091dc5bac6340325612f5d3c9c002327329e7ef,
archive 154e8c959cfbaadaaf58c46b44ac6458c3992b3b14a336667f58079efc2c2645,
has a separate clean-import Iris Xe record: 1129 frames, normal 126.102-second
logout, context destruction and 1795 frames after same-instance restart. It is
still a Draft; minimum compatible Core remains unresolved. This does not admit
it to ordinary installation or inherit acceptance for this new client.

CORE-1 requires instance-ancestry-checked DRM ACLs for the initialized UID.
CORE-2 requires receipt-checked input reconnection without replacing a live GPU
owner. RF-1/RF-2/RF-3 were addressed in r2's bounded normal-session trial; abnormal
recovery, physical input confirmation, authentication, RTX and full device
teardown are not universally accepted. Do not patch image defects in the client.

The native graphics host requires a versioned distributable renderer/input/module
closure. Existing development deployments and paths cannot serve as product
runtime dependencies. Module selection, kernel matching and native retirement
must be admitted before enabling start/stop. No service, staging, WSL restart,
forced termination or unload is authorized here. Preserve existing desktops.

## Concrete Guest result gap found during migration

The inspected guest/wsx-guest.py initialize function writes initialized.json with
Guest ABI, username, locale and timezone, then returns without a structured
stdout result. The persistent record does not include UID. `status` is systemctl
text for an existing session, not an account discovery operation. This client
must not invent UID 1000 or claim initialization based only on exit status.
Agree a versioned account-result/describe contract (or a separately specified
verified account lookup) and test it against both recipes before enabling the
initialization transaction. No cooperating repository was changed.

scripts/archive.py computes unpacked_size from inventory logical file sizes;
it is not the tar stream size. A future decoder needs a separately justified
tar output ceiling, a disk-space budget, cancellation and retained partial-file
semantics. Candidate tar decoding now uses an explicit operator-provided ceiling
and retains partial files on failure. Explicit independent candidate-test import
is implemented; stable installation and safe cancellation remain unfinished.

## Omarchy r10 test and direct host prerequisites

The isolated r10 guest booted with systemd, Guest ABI 1 and kernel
`6.18.54.1-microsoft-standard-WSL2`. The initialization record and independent
account lookup reported user `wen`, UID 1000 and `Asia/Shanghai`. Password
authentication, desktop and graphics remain untested.

The read-only prototype `rootfs-omarchy-r7-rtx-start-01` reports 953 presentations
on RTX 3080 Ti using opaque-v1, with no image patches or WSL restart; its record
explicitly reports the native renderer and virtio-gpu hashes matched the Iris
trial. Preserve one shared implementation for both GPUs and select paths by
actual capability, not GPU-vendor branches. The bounded r7 test did not
establish input, apps, restart or full native retirement. Its runtime receipt
identifies native host, renderer, epoxy, input host and six kernel modules by
hash; some were deployed from a machine-local path, not a reproducible product
package. The r7 evidence cannot establish compatibility for r10.

The direct host requires the real running WSL VM identity, a fresh instance UUID,
an adjacent runtime profile and complete renderer/input/module closure. The
prototype checks for an existing PCI owner before startup and uses an event-based
stop receipt. Reuse those contracts only after producing a versioned, independently
rebuildable or distributed closure, matching the running kernel and symbols,
checking existing owners, and implementing instance-scoped device access. No
experimental shared-kernel module was loaded in this test.

The instance-scoped DRM authorization adapter from the r7 RTX and r2 Iris
records is embedded in Core as `core/guest/drm-access.py`; both evidence copies
have SHA-256 `15aa9e0db467da5977a3d9197965bec66543da4a8e32a505bf86f787aa629b01`.
It requires a trusted Guest descriptor, validates VMBus ancestry and device
siblings, and applies an ACL to the initialized user. It is not yet called by
product startup; ACL retirement and complete host ownership are not admitted.

On the 2026-11-04 local follow-up, the 12 r7 receipt entries were recovered from
read-only prototype artifacts and independently checked against their recorded
SHA-256 values. The experimental reference bundle is retained at
`artifacts/graphics-reference-omarchy-r7/`; its `verified-files.json` SHA-256 is
`7d84b42d17abfe6e3c3e3832cf9dc55dfbbb150fab143a5d0c91ecc372b3492f`.
All six module files declare the same `6.18.54.1-microsoft-standard-WSL2`
vermagic as the running Guest. Matching vermagic and hashes alone do not prove
kernel symbol CRCs, r10 component pairing, device-owner safety or runtime
retirement. The bundle is evidence, not an admitted product distribution.
The current Windows machine reports Iris Xe driver 31.0.101.5081; no WSX PCI
driver directory was found in the shared WSL kernel during the read-only check.
`wsld graphics verify-reference DIRECTORY KERNEL` checks the pinned evidence
receipt and all twelve files against their recorded hashes. KERNEL is a caller-
supplied observation; this command does not discover the running kernel, validate
symbol CRCs, authorize components or start a desktop.

The subsequent live preflight used the prototype's Vulkan identity and display
inventory programs on Windows. Vulkan reported two Iris entries with distinct
LUIDs. The active display path matched one of them; a future launcher must use
this dynamic identity join, never first-device or vendor-only selection. The
test Guest exposed its packaged opaque consumer (ABI 2) and Venus 26.2.4 with
opaque import/WSI flags. Its running kernel did not provide the probed headers'
`Module.symvers`; matching module vermagic alone is not symbol CRC validation.
No native host or new graphics device was launched in this preflight.

The 2026-11-04 follow-up reused the prototype identity tools to collect live
display and Vulkan observations. The new Core LUID join selected the unique
active adapter on both Linux test and Windows MSVC execution, and rejects
ambiguous synthetic inputs. No GPU-brand branch is used. The Guest-side
`core/guest/kernel-symbol-audit.py` compared imports against 744 installed WSL
modules; 304 observed candidate imports matched and none differed, while 311
imports were not observed. This is partial evidence, not verification of the
running kernel's complete export table. Full output is retained in
`artifacts/graphics-reference-omarchy-r7/kernel-symbol-audit.json`. The reference
kernel configuration differed from `/proc/config.gz` only in compiler naming and
two configuration capability flags; this does not establish identical CRCs.
The follow-up package-export comparison checked 36 additional internal imports,
with no mismatch, leaving 275 imports not observed in either comparison. Its
output is `artifacts/graphics-reference-omarchy-r7/kernel-symbol-audit-v2.json`.

## Connected candidate lifecycle, 2026-11-04

The symbol gap above is resolved by reading the installed WSL kernel image and
matching its GNU Build ID against the running Guest. All 615 imports across the
six existing modules match kernel/package exports: zero unknown, zero conflict.
No module rebuild or historical GPU retest was needed. Same-major reuse remains
the baseline, with actual CRCs and the kernel loader as the compatibility gates.

Core now owns an explicit candidate desktop startup/status/stop path; see
[contract and commands](DESKTOP-CANDIDATE.md). The VMBus discovery/binding sequence
is adapted from the pinned `guest/start-guest.py`; the r7 bootstrap's real
`wsx-guest prepare` call replaces its old prototype desktop entry. The DRM adapter
is retained byte-for-byte. Direct Host invocation, readiness and event retirement
reuse DIRECT-HOST.md and direct-launch.ps1. No hardware-vendor branch is added.

Initial Windows execution rejected a PowerShell 5 array serialization wrapper
before native creation. The fix transports native inventory JSON as strings and
parses it in Rust; the failed observation and diagnostics remain preserved.

The real candidate run then displayed Omarchy r10 using the pinned native Host
and all six original modules: 1,235 successful opaque presentations. User-requested
normal stop produced Guest retirement, zero retained pages, native final closure
and no remaining native/input processes. The PCI function disappeared; shared
modules remain loaded. Full evidence and exact tested PE hash are in STATUS.md.
The later missing transient Guest descriptor is recorded separately and is not
GPU retirement evidence. No image code patch was used.

The pinned Host source has a legacy input-child kill fallback after GPU retirement.
The observed input exit reason was not instrumented, so graceful input retirement
is not claimed. Repeated native lifetimes still need an admitted owner-rebind
policy; the current loader refuses to replace or unload an existing shared module.

## Retired Owner restart integration, 2026-11-05

Under explicit user authorization, Core reused the prototype's ordinary
`rmmod wsx_virtio_msg_pci` / new-instance insmod contract. Five other modules
remain loaded. Full prior retirement, current module Build IDs, old parameters,
zero refs/no holders and no old/foreign device or session are checked before
this operation. Rootfs code and guest lifecycle ABI were unchanged.

The final product CLI returned successfully after first frame while the new
Omarchy desktop remained alive. A duplicate start was refused without changing
its receipt. Native/Guest logs, pointer lineage, restart plan, no-consent refusal
and intermediate normally retired failures are archived under
`artifacts/desktop-restart-20261105`. This supersedes the earlier missing-restart
integration gap; general abnormal recovery and graceful input-child retirement
remain separate work.
