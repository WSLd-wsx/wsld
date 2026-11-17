# Current VM-exit repair

Current regression: the post-shutdown successor initially rendered and accepted
interaction, then became unresponsive together with WSL-hosted commands. See
[ADR 008](../../docs/ADR-008.md). Opt-in native API timing and a finite Windows-local
read-only collector were added for diagnosis; they are not a proven fix. The
previous bounded shutdown success does not certify continued interactive health.

The current implementation is specified in [ADR 006](../../docs/ADR-006.md).
It pre-binds both Guest and system retirement helpers before desktop readiness,
preserving normal PAM/systemd cleanup without launching WSL commands at close.
Guest RAM reconciliation requires exact original HCS VM exit plus `MEM_FREE`
across the original aperture after HDV returns `ERROR_INVALID_HANDLE`; GPU drains
and final HDV teardown remain mandatory. No generic invalid-handle success path
or speculative local unmap was added.

Final graphics build: `artifacts/graphics-vm-exit-pair-20261105-03`.
Real isolated VM policy evidence: `artifacts/vm-exit-api-probe-20261105-06`.
MSVC C/Rust/helper fixtures: `artifacts/vm-exit-fixtures-20261105-04`.
The new pair subsequently passed full Omarchy whole-WSL shutdown acceptance with
the ADR 007 Core build in `artifacts/shutdown-acceptance-20261105-03`. Exact-pair
renderer-close/terminate regressions and subsequent restart remain separate. The
original failed lifetime is preserved. The earlier implementation and
failed shutdown evidence below are historical context, superseded by ADR 006.

# External distro and VM retirement

This integration extends the verified renderer-close pair. `prepare.py` is called
by `../graphics-close/build-pair.py` only after the pinned reference inputs and
close patch have been applied to a new staging tree. Reference repositories are
read-only. Public Core stays Rust/MSVC and independent of this explicit graphics
reference builder. No service, business daemon or replacement GPU protocol is added.

## Authority and order

The Native graphics Owner polls `wsl --list --running --quiet` asynchronously,
with one bounded outstanding query, and matches the exact configured distro name.
A successful observation of that distro running must precede a stopped transition.
Failed, malformed and oversized queries are uncertainty, never absence proof.

A small system-distro helper starts with the graphics lifetime, announces readiness
before desktop admission, then waits on its private stdin. After observing distro
stop, Native writes one retirement request to that already running helper. This
does not run `wsl -d` against the stopped distro or spawn a new system environment
after VM loss. The helper validates the exact instance's VMBus ancestry and PCI
driver, then writes the existing synchronous `retire` attribute. No other distro,
session or module is stopped. Queue service continues through UNREF/RESET and
GPU drain; only actual successful helper completion acknowledges Guest retirement.
An unused helper exits on stdin EOF. Native observes helper exit without killing.
This process is part of graphics lifecycle control, not a second business Core.

Original-VM exit uses `HcsWaitForComputeSystemExit` against the retained HCS handle.
Timeout or any unknown error does not authorize reclamation. Confirmed exit can
replace the Guest-users barrier and HCS device-removal request, but never replaces
GPU/context/completion drain, memory lease closure, HDV teardown or callback rundown.
All existing fallible barriers remain. Stream failure without exact VM-exit proof
still retains quarantine. A later VM with the same distro name is not adopted.

The existing normal-close Guest path remains available while the distro runs.
Concurrent normal-close/external-termination races are not stress-tested; the
legacy `wsl --exec` normal-close path still has a check/start race. The external
termination path itself uses the pre-existing system helper. WSL/system-helper
unavailability or unsupported HDV cleanup must remain diagnostic failures.

## Tests

`test-msvc.ps1 -Toolchain RUST -StagedNative STAGED_NATIVE -Evidence NEW_DIRECTORY`
builds and runs the real C owner code with simulated HCS returns, plus Rust tests
for UTF-8/UTF-16 WSL listing and exact-name/error handling. The fixture checks
timeout/error refusal, latched exact-handle exit, pending operation retention and
thread ownership. It creates no VM or GPU and is not shutdown acceptance.

`scripts/test-distro-termination.ps1` requires an explicit owned r11 Native receipt
and binary hash. It captures the real window, executes only that distro's
`wsl --terminate`, observes bounded process exit without forced cleanup, and checks
that the target stays stopped, other running distros and all registrations survive,
and actual Guest/GPU/memory/Native retirement completes. It never uses `--shutdown`.

See [ADR 005](../../docs/ADR-005.md) and `docs/STATUS.md` for exact builds/results.
The user subsequently authorized one actual VM shutdown and manually restored
Codex. `scripts/test-wsl-shutdown.ps1` ran from an independent Windows-local
collector. Its evidence in `artifacts/shutdown-acceptance-20261105-01` records a
real failure: original VM exit was confirmed, but Guest RAM lease closure failed
with `0x80070006`; Native remains quarantined and Core reports `failed_retained`.
Do not inherit the passing distro-termination result as VM-shutdown acceptance.

## API basis

[Microsoft WSL command reference](https://learn.microsoft.com/en-us/windows/wsl/basic-commands)
distinguishes stopping one distribution from shutting down the whole utility VM.
[HCS exit wait](https://learn.microsoft.com/en-us/virtualization/api/hcs/reference/hcswaitforcomputesystemexit)
observes a specified compute-system handle.
[HDV teardown](https://learn.microsoft.com/en-us/virtualization/api/hcs/reference/hdv/hdvteardowndevicehost)
is a separate fallible native operation. These API descriptions do not establish
that every GPU driver/VM-loss cleanup combination works; real evidence is separate.
