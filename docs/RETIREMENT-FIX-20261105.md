# Window-close retirement investigation

The Omarchy r12 close test failed twice. Native preserved resources after the
Guest protocol rejected session retirement; the original binary discarded the
structured Guest error. PID 6096 belongs to the second failed lifetime, instance
500daf92-0dd8-48b4-8831-0939fae89c31. Its retained state is not normal retirement.

Candidate fixes in the cooperating repositories:

- Guest status no longer requires an ephemeral descriptor after validated logout.
  An unknown/missing descriptor does not prove retirement.
- Guest waits at most 15 seconds for exact-instance processes, seat and owner
  records to disappear. It never kills residual processes or acknowledges GPU
  retirement. Timeout diagnostics contain PID/name/cgroup, not environment data.
- A disappearing systemd transient unit is reconciled only with an observed
  inactive state. Failed/active states still reject.
- Guest atomically retains a bounded per-instance session.stop reply in the
  root-only /var/lib/wsx/retirement-diagnostics directory.
- The Native adapter source preserves a bounded structured Guest error. That
  Native source change has not been rebuilt/deployed to the retained process.

The Omarchy installation has an explicit reversible test overlay, not the
unmodified published r12 bytes. Backups are under
/var/lib/wsx/diagnostics/protocol-close-test*-20261105. Current test files:

- guest-protocol.py: d91aa194c8c218e184f89e4606623a38139424ccb73682d530bb46be9724a65d
- protocol-retirement.py: f1012a34f3ab8be184cdf5fde94eb5fbe8c4ae2b25b31fef7c69ceda4f128997

Evidence is retained in artifacts/retirement-fix-20261105-01. Twelve real isolated
PAM sessions without GPU passed with the drain fix. Two subsequent sessions
verified the corrected private diagnostic directory and actual reply files.
The earlier diagnostic-directory failure remains in pam-repeat-v2.log; it was
fixed before journal-test.log. Rootfs local tests: 217 run, 12 environment skips;
Core Guest-boundary tests: 3 passed. These are not successful GPU/window-close
acceptance. The exact original Guest rejection was not retained and must not be
claimed as conclusively diagnosed. Failure-path UI responsiveness and local
recovery from retained GPU resources remain unresolved.
