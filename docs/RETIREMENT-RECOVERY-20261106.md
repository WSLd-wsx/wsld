# Retained lifetime recovery preparation

The original r12 protocol helper rejected logout immediately while asynchronous
PAM/user services still existed. Current exact-instance observation found no
processes, descriptor or seat after that rejection. The error was
Guest instance process remains; session retained. Native already returned from
its queue loop and entered quarantine, so later Guest quiescence does not resume
that original Owner or establish GPU retirement. PCI refcount remains 1.

The installed test r12 now has a reversible overlay: bounded exact-instance
session drain and status independent of the removed ephemeral descriptor.
Original SHA-256 and bytes are retained under /var/lib/wsx/diagnostics/
retirement-fix-20261106. Full source/hash evidence is in
artifacts/retirement-recovery-20261106-01. A unique real PAM session without GPU
passed the public session.stop call after deployment. No GPU retirement was
claimed by that test. The default logind grace period remains unchanged.

Corrected input runtime e09a6ff1324dda48dfdf01313e6b358d2c8f50105b97e51795a2d46edc009901
is staged and selected. Full byte verification passed. Latest CLI is
artifacts/host-compat-build-20261106-05/package/wsld.exe. Prepared live tests now
also require the same input companion PID throughout all view cycles.

PID 22288 remains quarantined in original VM e31334e1-cd8a-45e4-89ec-a1bddf393a1c.
No module unload, PCI remove, process kill or VM restart has been performed for
this recovery. Manual PCI removal after Host disconnection previously disrupted
WSL and is not a valid recovery procedure. A confirmed original-VM exit is the
existing verified fresh-admission boundary for this unrecoverable old process.
Such a shutdown affects all WSL distros and requires separate explicit consent.
The local test overlay is not an immutable published rootfs update; distribution
of the Guest bug fixes requires a new image revision.

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
