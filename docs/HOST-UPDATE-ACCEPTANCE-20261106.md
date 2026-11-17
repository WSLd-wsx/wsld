# Directed Host update acceptance

The immutable r12 rootfs was installed through compiled wsld.exe --install
--pre-release wsx-omarchy-4.0.6-r12. Account credentials were entered only in the
local terminal. The user confirmed desktop display. The exact candidate runtime
is selected in the user configuration, with its full receipt pinned by Core.
All 12 component entries passed byte verification. Relative to the protocol
baseline, only Native Host and renderer DLL differ; Guest modules are identical.

Three real close/reattach cycles passed. PID 22288, creation FILETIME
134384089281783782 and instance 90450e7c-cc76-48fa-8127-474f8502734f remained
constant. Each detached snapshot retained a running Guest session; each wsld -d
invocation restored attached state without creating another Host.

The initial install command returned 1 because display fitting reported a
pre-mutation stale_generation, despite the desktop being displayed. Core now
re-queries both endpoints at most three times only for that precise pre-mutation
rejection. It never retries uncertain post-application changes. The latest
MSVC build has 72 passing tests, including bounded retry and incompatible Host
selection rejection. Fresh-start acceptance of this retry remains pending.

Keyboard/mouse confirmation after reattachment and explicit-stop retirement are
pending. Do not report whole-product acceptance from the completed view cycles.
Evidence: artifacts/host-compat-acceptance-20261106-01; latest CLI package:
artifacts/host-compat-build-20261106-04/package. Published rootfs assets and
existing distro disks were not modified or replaced by the Host update.

Final outcome: FAILED. User input confirmation reported no keyboard/mouse after
detach. The old input companion exited on hidden windows. Explicit stop also
failed Guest retirement and retained PID 22288. See VIEW-INPUT-FAILURE-20261106.md.
The window-only cycles above must not be reported as interactive acceptance.

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
