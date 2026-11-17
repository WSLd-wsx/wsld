# Multi-distro PCI Owner configuration ABI 2

The module loads once without per-lifetime parameters. Root configures one exact
BDF, VMBus UUID, service port, aperture BAR/size and device number via the driver's
configure attribute, then uses driver_override and drivers_probe on that BDF.
Probe validates VMBus ancestry and actual BAR geometry, consumes a bounded slot
and copies immutable configuration into the device Owner. There are 16 slots;
duplicate addresses/UUIDs and overlapping input/graphics port pairs reject.
No wildcard PCI matching or automatic device takeover is introduced.

Each Owner retains the existing virtio/DRM parent, vsock, release completion and
retirement mutex. Retiring/removing A never unregisters the shared driver or B.
Bound configuration readback must match the requested instance. Legacy module
parameters are not silently reinterpreted as ABI 2. The sysfs configuration_abi
attribute and pinned module build identity gate reuse.

Core's per-registration lock still enforces one desktop per distro. A separate
Windows bootstrap mutex serializes module load/configure preparation; it does
not run a business service or hold other desktops after startup. Concurrent
admission requires a pinned ABI 2 component; old runtimes retain rejection.
Module reuse verifies all six exact build notes, permits bound peer devices,
and never unloads ABI 2 to start another distro. Guest namespace-local session
admission still rejects a second seat/descriptor within the same distro.

Candidate receipt: 84760c978aa6bf299854c355bd44eb1765c870c21f9be0c1913e20aeec11ddc9.
Only the Owner module changes relative to the corrected retained-view runtime.
Kernel modules compile with -Werror in an isolated copy of the verified kernel
build tree. Windows MSVC workspace tests and simulated module-reuse policy passed.
ABI 2 was loaded in the authorized live transition below. Two desktop acceptance
is incomplete; compilation does not establish concurrent graphics support.

Live transition preparation found the old ABI 1 Owner retired, with no Native or
input process, zero references/holders and no bound PCI function. Replacing that
loaded module requires specific consent under AGENTS.md. Prepared transition
uses ordinary rmmod only after repeating those checks, then insmod of the fully
verified ABI 2 module; it never force unloads or restarts WSL/services.
Evidence: artifacts/multi-owner-kernel-20261106-01,
artifacts/multi-owner-core-20261106-01 and
artifacts/multi-owner-acceptance-20261106-01.

Required live acceptance: two different distro desktops/windows; independent
input/focus; repeated detach/attach; stopping or terminating A leaves B usable;
A restart while B remains live; duplicate same-distro startup and malformed/
duplicate configuration rejection; final independent reference/mapping retirement.
The Guest public protocol remains unchanged. No multi-distro GPU acceptance is
inherited from the single-distro test or module compilation.

## Authorized transition and first live result

The user authorized the bounded replacement and updated the permanent module
policy. Repeated checks passed; ordinary rmmod/insmod migrated the idle module
to ABI 2 without restarting WSL or touching other modules. Omarchy r12 then
started with exit code 0 and a real first frame using a per-device Owner.
Live malformed and duplicate configuration writes returned EINVAL/EEXIST, while
the existing device configuration remained unchanged. CachyOS metadata/CPU
admission, download, verification, import and interactive initialization completed.

The first concurrent install attempt exposed an index lock retained throughout
Host startup. Core now drops that read lock after copying immutable installation
facts; per-registration and bootstrap lifecycle locks remain. The corrected MSVC
build passed the same 72 tests. No second desktop has yet been accepted.

## Concurrent Guest device view and failed acceptance

On 2026-11-06 the second Native Owner, PCI function and input companion started
while Omarchy remained live. The first CachyOS attempt failed because Venus
selected the first global render node, belonging to Omarchy. A distro-local
`/dev/dri` mount view fixed that selection: KWin and Xwayland then opened only
CachyOS card2/renderD130. Its first frame still failed: debugfs proved Omarchy
seatd/Hyprland had hotplug-opened card2 as DRM master; KWin was not master and
its opaque framebuffer ioctl returned EACCES. Do not remove DRM_MASTER checks
or alter the GPU protocol to hide this ownership conflict.

Both Guests need the device view before their desktops start. The rootfs helper
verifies the exact VMBus ancestor and primary/render siblings, requires init's
mount namespace and no current Guest DRM client, makes /dev and /run propagation
slave, and binds a private tmpfs containing only the admitted device nodes over
/dev/dri. This is a cooperative device-selection boundary, not security against
Guest root creating other device nodes. It preserves incoming host propagation
and prevents outgoing view mounts from changing another distro. Existing rootfs
images require an explicit backed-up test overlay or a new immutable revision.

The additive Guest protocol capability `graphics.distro-device-view-v1` records
this behavior; protocol major/minor remain 1.0. Core's ABI 2 startup now requires
that capability before Native creation. This is not permission to run beside
unqualified pre-existing desktops. All participating Guests must implement the
view before startup. Published r12/CachyOS r1 images do not advertise it.

The user authorized normal stopping of both test desktops. Omarchy fully retired
and restarted with the isolated view, first frame and display fitting, exit 0.
CachyOS logout left instance-tagged accessibility/settings D-Bus user services;
the Guest drain timed out and Native PID 19264 entered its existing permanent
quarantine. A Guest fix now normally stops only service/scope units wholly tagged
by the exact instance, checks ownership again, and never stops the user manager
or a shared unit. Applying it to this test Guest drained the residual services.
That does not restore the already quarantined Native thread or prove GPU/PCI
retirement. Do not kill it or remove its live PCI function as recovery.

Also fixed: PowerShell automatic Matches variable interference in peer liveness
observation; cross-filesystem descriptor archival from /run to /var; bounded
recovery of pre-Native retry journals without discarding their original history.

Evidence: acceptance directory retry 3, drm-master-observe.txt,
*-device-view-after.txt, *-isolation-stop.json, cachyos-user-unit-drain-2.json,
omarchy-isolated-launch-result.json. Both windows together, focus isolation,
independent peer stop/restart and final two-Owner retirement remain not passed.
The quarantined old Native has no live recovery command; further same-distro
acceptance is blocked by that concrete retained lifetime. No WSL/service restart,
force termination, PCI removal or module unload was used in this retry.

## Presentation failure after the isolated Omarchy first frame

The user reported that the visible Omarchy window accepted neither keyboard nor
mouse. Later Native logs show VK_ERROR_OUT_OF_DATE_KHR from QueuePresentKHR,
followed by fatal stream quarantine and unconfirmed input companion retirement.
The screenshot and initial exit code 0 establish first frame only; Omarchy is
now also failed/retained (PID 19988), not an accepted usable desktop. Input focus
isolation remains untested; this observation must not be labeled only an input
node-selection failure.

The paired candidate renderer/Owner introduces private result 0x57535802 for an
unpresented frame whose source fence AND presentation queue completed. It drains
both before destroying the obsolete swapchain, releases the temporary native
image borrow, acknowledges logical FLUSH and retries a fresh frame later. It does
not log that attempt as a successful visible frame. Pre-submit deferral retains
its existing distinct result. Fence timeout, queue/device loss and unknown errors
retain failure semantics. Both Native and renderer must update together.

Five Windows tests exercised the actual patched presenter with mocked Vulkan
callbacks and a real independent Win32 window: drained resize, source timeout,
queue failure, device loss and suboptimal presentation. These are device-free
checks, not GPU recovery acceptance. The complete paired graphics build passed.
Candidate multi-Owner receipt:
ec504e4fb9b0938b5c76ff38303204f35da5cdfc3f9f723c11b95037ee10a1dc.
Only Native and renderer differ from the previous ABI 2 bundle; input companion
and all kernel modules are unchanged. No failed live process can be hot-patched
with these new files. Current PIDs 19264/19988 lack a resumable quarantine path;
replacing their files or killing them cannot count as GPU retirement.

Build evidence: present-recovery-pair-20261106-01, present-recovery-multi-20261106-01,
present-recovery-check-20261106-01 and multi-owner-core-20261106-10.

## Input isolation and title follow-up

After the authorized manual shutdown and exact failed-Host cleanup, both desktops
reached first frame concurrently. The user confirmed both accepted normal input.
Each view detached and reattached with its original PID while its peer remained
running. The user then confirmed input typed in one desktop also reached the
other: those first results do not establish safe concurrent input isolation.

Guest input devices now carry the exact instance UUID. Before session launch,
a distro-local /dev/input tmpfs view exposes only its three event nodes. The
Guest advertises graphics.distro-input-view-v1 and ABI 2 Core requires both the
DRM and input view capabilities. Device selection and missing/duplicate names
have synthetic tests. A real uinput/mount test created three disposable devices
in each test distro and verified only those nodes appeared in its view; it did
not run a compositor or inject input. Omarchy subsequently started using its
actual input view; its event nodes matched its instance UUID, and the user
confirmed display, mouse and keyboard worked. Cross-distro isolation with both
fixed desktops remains untested.

CachyOS's first input-view startup rejected an input fd held by systemd-logind.
A verified logind monitor is now distinguished from user input clients; other
holders receive a bounded wait and detailed diagnostic before rejection. That
failed startup left an exact input child, which was normally terminated after
identity/owner checks. Its failed session state caused the old Guest retirement
adapter to reject and Native PID 19192 to quarantine. This is not GPU retirement.
The updated protocol reports a drained failed session separately while retaining
GPU retirement as a distinct obligation. Omarchy's later failed pre-compositor
session was successfully retired using that correction. The CachyOS old Native
has already entered an immutable quarantine loop and cannot load the fix; no
manual PCI removal, force kill, WSL shutdown or service restart was performed.

The initial renderer title is now WSX desk. A real, GPU-free Win32 window test
verified the exact initial title and retained-view behavior. A new paired runtime
was compiled and staged with receipt
442f6032d8645c413b7398e2654c669169e79e9b46e79220134bb881b51d0561.
The already-running Omarchy process keeps its previous renderer until restarted.
The new title runtime has not yet been accepted on a real desktop.

## Two-desktop acceptance after input-view correction

On the RTX 3080 test machine, after the user manually shut down WSL and the old
VM was observed absent, the exact quarantined CachyOS Host PID 19192 was cleaned
up under explicit authorization. Its prior normal retirement remains unconfirmed.
The already-running Omarchy Host observed the VM exit and completed native memory
closure without forced termination. No prior failure record was overwritten.

Using the staged title/Owner ABI 2 runtime, CachyOS and Omarchy both reached first
frame in the same new WSL VM. Each had its own Native PID, input child, PCI Owner
and Guest session. Their /dev/input views contained three different event nodes
labelled with the corresponding instance UUID. The user tested both desktops,
confirmed mouse, keyboard, menus and ordinary text, and confirmed that typing in
one terminal did not reach the other. A same-Host detach/reattach test on each
window passed while its peer stayed running.

CachyOS was then normally stopped while the same Omarchy Host stayed active;
Guest/PCI retirement, zero retained mappings and natural helper/input exits were
observed. CachyOS restarted with a new Host and instance while Omarchy remained.
The user confirmed both desktops and input isolation still worked. The same test
was repeated in the opposite direction: Omarchy normally retired and restarted
while the same CachyOS Host remained, and the user again confirmed both windows
and input isolation. Both desktops were left running. The distro Guest processes
used explicitly recorded test overlays, including a test-compiled static input
binary; these are not rebuilt or published rootfs releases.

Core status reads WSL's existing registry data as the authoritative WSL registration
identity. It does not write the WSL registration. Current startup also uses local
installation provenance and lifetime journals for ownership/recovery; those are
persistent data, not cross-command in-memory state. A prior failed lifetime in a
new VM can still require all graphics Owners to be absent before fresh admission;
this remaining conservative recovery rule is distinct from ordinary concurrent
stops and restarts tested above. The peer admission path verifies each existing
Host identity, current registration, current VM, ABI 2 runtime and both required
Guest isolation capabilities before starting another Host.

Evidence is under artifacts/multi-owner-acceptance-20261106-01: dual-v2-live-result,
input view records, v2-peer-stop-result, v2-reverse-peer-stop-result, restart
outputs and dual-final-passive records. The WSX desk title is independently
verified by the real Win32 window test in artifacts/desk-title-window-test-20261106-01.
Neither a clean rebuilt image, cross-distro WSL termination/shutdown with both
Owners active, sustained soak, Iris Xe rerun, signing nor release acceptance is
implied by this RTX 3080 session.
