# Omarchy window-close acceptance

One real Windows test passed with the reversible Guest retirement test overlay
listed in RETIREMENT-FIX-20261105.md. This is not acceptance of the unmodified
published r12 image or recovery after a forcibly terminated Host.

The compiled CLI launched wsx-omarchy-4.0.6-r12. Screenshot confirms the desktop
was displayed. Automatic display fitting returned stale_generation, so the CLI
reported failure while retaining the running desktop. This issue remains open.

The test sent normal WM_CLOSE through CloseMainWindow to Host PID 24648,
instance a0b45000-7156-42eb-9522-06b3322e3965. The Host exited within 30 seconds.
Its log records session retirement, exact PCI retirement, RESET acknowledgement,
zero usable Guest RAM pages and normal helper/input/device/bootstrap shutdown.
Read-only postchecks found no Host, no PCI 7156:00:00.0 and Owner refcount zero.
The WSL VM identity remained D146D43E-BB4F-4D62-A1BB-83C79AF98180. No forced
termination, module unloading, manual PCI removal or service restart occurred.

Evidence: artifacts/window-close-20261105-03/{summary.json,close-result.json,
native.log,before-close.png}. Summary contains artifact SHA-256 digests.

The earlier disconnected-Host experiment is a failure: retire returned, but PCI
remove blocked for about 217 seconds and the user reported WSL becoming unusable
until a service intervention. Its hardcoded wsl_restarted=false output was not
an observation and must not be treated as recovery evidence. Keep this route
out of product admission pending diagnosis.
