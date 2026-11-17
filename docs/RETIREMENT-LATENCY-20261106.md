# Retirement latency investigation

Real Windows window-close tests on the r12 Guest test overlay measured 12.65 to
12.95 seconds before the change. Guest instrumentation measured 12.27 seconds
in session retirement, with less than 0.3 seconds in loginctl/systemctl commands.
The remaining time waited for instance-tagged user services (portals, uwsm,
PipeWire and accessibility services) to exit after the login session ended.

The live login1 UserStopDelayUSec property was 10000000. A reversible test
configuration at /etc/systemd/logind.conf.d/90-wsx-session-lifetime.conf sets
UserStopDelaySec=0. The supported systemd-logind reload applied it without a
service restart; the live property became zero. Normal window-close latency
then measured 2.656 seconds, with zero retained mappings and normal shutdown of
the Host and its helpers. No device barrier or timeout was shortened.

Rootfs stage-protocol.py now packages the default under /usr/lib/systemd/
logind.conf.d. Administrator configuration in /etc retains normal precedence.
The policy removes the user-manager idle grace period after the last session;
it does not override lingering or authorize stopping another active session.
Published r12 assets are unchanged. The installed r12 has reversible test overlays.

Evidence: artifacts/retirement-profile-20261106-01,
artifacts/retirement-drain-20261106-01 and
artifacts/retirement-delay-fix-20261106-01. Timing is observed through Windows
log polling and includes log flush latency. Rootfs tests: 217 run, 12 skipped.
Temporary-root protocol staging also verified the packaged configuration.
