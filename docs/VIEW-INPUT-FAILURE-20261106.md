# View reattachment input failure

The user reported a screensaver and no keyboard/mouse response after three view
cycles. The Native process/window remained alive. No wsx-input-host process
remained. The verified input host source exited its loop whenever its target
window became invisible. Thus the first detach ended the input stream and the
Guest receiver destroyed its uinput devices. Reattaching only restored graphics.
The previous three-cycle result established window/session continuity, not usable
interactive continuity, and is not whole-feature acceptance.

input-persistence.py now builds the verified input companion from pinned sources
with a retained-view lifetime. Hiding the window keeps the peer and heartbeat;
keyboard focus and mouse buttons are released while hidden. A destroyed window,
changed PID or dead Owner still ends the companion. No input wire/Guest change
is made. Full paired builds now rebuild and package this third changed file.
The old candidate's directed update edge was replaced by the corrected closure:
e09a6ff1324dda48dfdf01313e6b358d2c8f50105b97e51795a2d46edc009901.
The new companion compiled; a real MSVC window-policy test passed five hide/show
cycles plus foreign-PID and destroyed-window rejection. This is not a completed
GPU/input regression test.

The explicit-stop test for the installed unmodified r12 failed independently:
Guest retirement helper exited 1; Native PID 22288 quarantined resources and did
not exit within 45 seconds. No forced cleanup was performed. Current resources
must not be silently reused. Preserve artifacts/host-compat-acceptance-20261106-01/
stop-result.json and native-after-stop.log. Do not attempt manual PCI removal,
which previously disrupted WSL. Actual retesting of the corrected input closure
remains blocked by this retained lifetime and the Guest retirement failure.

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
