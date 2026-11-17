# Single-distro retained view v1

This supersedes window-close-as-logout and the closing-overlay proposal for
normal WM_CLOSE. The first implementation retains the original HWND, surface,
Vulkan objects, Native process and Guest desktop. It hides the window without
signalling WSXDirectStop. It is not backend reconnection, process handover,
surface destruction or a power-saving mode. Rendering may continue while hidden.

Renderer-window messages (same Windows session/integrity boundary):

- WM_APP+3 queries view-v1: 0x57535811 attached, 0x57535812 detached.
- WM_APP+4, wParam=1, lParam=0 restores the same window and returns 0x57535811.
- Other message versions do not attach. Unknown/legacy implementations are never
  silently treated as supporting view-v1.

Core discovers the existing process from the OS, verifies its executable closure,
WSL registration/disk and process creation time, then negotiates view-v1.
`wsld -d DISTRO` with that live Host restores the existing view. It rejects an
ambiguous Host, changed registration, stopping session or failed lifetime.
`desktop status` exposes the queried view state independently of desktop health.
No persistent detached flag, additional desktop or replacement Host is created.

`desktop stop` still signals the exact Native stop event and runs normal session,
device and GPU retirement. External distro termination and VM exit retain their
existing retirement paths. Detaching never claims GPU retirement. Same-name
reimport never adopts an old process. Multiple distro devices remain unadmitted.

Windows fixture acceptance exercises five detach/attach cycles, rejected message
versions, an unsignalled lifecycle event on close, a separate explicit stop, and
owner reference barriers. It uses real Win32 windows but no GPU or HDV.
The current machine has r11 but no r12. Real r12 desktop continuity, input after
reattach, hidden-window resource growth and external-stop regression remain
required acceptance; historical window tests do not establish these results.

Evidence: artifacts/view-detach-20261106-01, view-core-build-20261106-*,
and view-detach-pair-20261106-01. Newly built graphics closures must be explicitly
admitted as paired components before product deployment; old runtime receipts
must not be overwritten or their hashes bypassed.

Validation completed: Windows MSVC workspace tests passed (1 ABI, 57 Core,
5 release, 2 process-store and 4 host tests). Five real Win32 view cycles passed.
Core three-name PE hashes match: b84b3580a04fa91164d172889f33f256e28100e25e6711764c4589989306b94b.
Paired renderer/Native build passed. No real Guest desktop was launched or
modified during this implementation turn. Runtime admission and GPU acceptance
remain pending; do not deploy by replacing files under an old receipt.

Core now admits the exact view-v1 candidate through a directed compatibility
edge from the published protocol baseline and exact Guest profiles. The baseline
manifest stays immutable and automatic runtime selection stays unchanged.
See ../core/compatibility/host-updates.json and the cooperating rootfs document
host-update-compatibility.md. Real acceptance is not implied by an admission edge.

Explicit local selection uses %LocalAppData%/WSX/config/graphics-runtime.json:
{"schema_version":1,"receipt_sha256":"FULL_REVIEWED_RECEIPT_DIGEST"}.
This is a component preference, not a runtime-state authority. Core checks the
selection against the current image's exact profile and directed update edge;
unknown/unreviewed selections reject without silently falling back. Absence
keeps the release baseline. Selection never alters a running Owner.

Update: r12 was installed on this machine. Live testing found that the original
input companion exited on hidden windows. The two-file candidate is withdrawn;
input-persistence.py and VIEW-INPUT-FAILURE-20261106.md describe the correction
and the failed-retirement blocker for further live acceptance.

## Corrected live acceptance after manual VM restart

Startup returned 0 with 2048x1152 at scale 1. Three view cycles retained Native
PID 17904 and input PID 18224; the user confirmed keyboard/mouse operation.
Explicit normal stop then completed in 17.945 seconds, with RESET acknowledged,
zero usable mappings, exact PCI disappearance, Owner refcount zero and both
processes absent. No forced cleanup or VM restart occurred during this new
lifetime. Logind's 10-second user grace period remained unchanged.

This result uses the explicit reversible Guest bug-fix overlay on installed r12
and corrected input runtime e09a6ff1324d. It is not acceptance of the unmodified
published r12 image. Distributing the Guest fixes requires a new rootfs revision.
Evidence: artifacts/retirement-recovery-20261106-01/final-acceptance.json.
Earlier failed-lifetime results remain historical failures, not rewritten success.
