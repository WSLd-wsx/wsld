# Closing feedback and logind policy

The user chose to preserve logind's normal user-manager grace period. This
supersedes the proposal to ship UserStopDelaySec=0 in RETIREMENT-LATENCY-20261106.
The rootfs stage no longer installs that override. The exact r12 test override
was removed and systemd-logind reloaded without restarting; UserStopDelayUSec
was read back as 10000000.

The renderer window now creates a child feedback surface on its own existing
Win32 message thread when WM_CLOSE requests normal retirement. It animates
"Closing desktop..." using a timer and GDI, independently of the Vulkan queue.
Repeated close requests remain idempotent. Window resizing updates the feedback
surface. The overlay lives until its parent is destroyed after actual retirement;
it neither cancels work nor claims that the whole Linux distro has shut down.
An unresolved retirement keeps the window visible; the animation is not proof
of progress or successful retirement.

The patch is applied to checked renderer sources by both graphics-close staging
and the paired runtime builder. Upstream reference files remain untouched.
The Windows MSVC integration fixture passed with the actual overlay visible,
repeated WM_CLOSE, retained presenter references and final owner-driven close.
This fixture does not execute Vulkan or HDV. Full runtime acceptance must be
recorded separately after rebuilding and admitting the paired component.

Evidence: artifacts/closing-ui-20261106-01/window-test/test-result.json.

The paired renderer DLL and Native Host rebuild passed. Evidence and SHA-256
closure are in artifacts/closing-ui-pair-20261106-01/build-result.json and its
bundle receipt. This component has not replaced the r12 admitted runtime or
passed a real desktop close test yet.
