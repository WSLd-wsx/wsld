# Desktop exit and recovery contract

> Historical experiment paths referenced below were moved to the prototype repository; see [Experiment migration](EXPERIMENT-MIGRATION.md).

Current restart result: initial first frame and user interaction succeeded, but
later runtime stability failed and the user recovered through WSLService.
[ADR 008](ADR-008.md) preserves the evidence; the trigger is unresolved. The
bounded successful shutdown below does not certify subsequent interactive health.

Latest actual acceptance: [ADR 007](ADR-007.md) records successful fresh admission
without Windows restart and complete Omarchy `wsl --shutdown` retirement on the
new pair. Native/input exited 0, retained memory reached zero, both helpers exited,
and Core confirmed `native_retired`. Evidence is
`artifacts/shutdown-acceptance-20261105-03`. This supersedes earlier pending-shutdown
statements below; it does not certify subsequent restart or all race scenarios.


Current follow-up: [ADR 006](ADR-006.md) implements exited-VM aperture
reconciliation and pre-binds both retirement helpers. Isolated Windows API and
regression tests pass; full new-pair GPU desktop acceptance remains pending.
Use `reference-565b64806b7f` with the new public package. Earlier commands, hashes
and real desktop results below describe historical builds. A retained failed Host
still requires separate recovery; the new binary does not alter that process.


All exit sources belong to the product lifecycle. A window disappearing, a
distribution stopping, and a process exiting are observations, not proof of
Guest device, GPU allocation or native memory retirement. Core must reconcile
the lifetime regardless of who requested its stop. There is no persistent Core
daemon; live queue and device ownership stays in the graphics Host.

## Required paths and evidence

| Trigger | Required behavior | Current acceptance |
| --- | --- | --- |
| `candidate desktop-stop ID` | Signal the exact lifetime; keep queues serving while Guest/GPU/memory drain; permit guarded restart after evidence checks. Repeated stop after retirement is a no-op. | Normal stop/restart have real Windows evidence. New idempotent reconciliation is unit/build checked, pending a live retired lifetime. |
| Renderer window close | Request the same stop coordinator before output destruction; retain required objects and queues until drain. | Passed on r11 with the paired close runtime; the legacy runtime still has its recorded failure. |
| External distribution termination, such as `wsl --terminate NAME` | Observe Guest loss and reconcile pending work and device resources without assuming Guest cooperation. | Exact r11 termination passed twice, including the pre-bound system helper. Unexplained Host exit still rejects. |
| External `wsl --shutdown` or VM loss | Observe loss of the recorded VM generation, reconcile native resources under an explicit VM-loss contract; discover a new VM and revalidate configuration on next start. | Real shutdown confirmed original HCS VM exit but failed Guest RAM closure with 0x80070006; Native remains quarantined. Missing VM/process alone is insufficient (ADR 005). |

“WSL stop” means external distribution termination here. This change introduces
neither a `wsl stop` command nor a top-level `wsld stop` alias.

## Historical renderer-close failure, 2026-11-05 local time

Core instance: `candidate-23900-1793804506366543700`.
Native lifetime: `{b1b084aa-df03-497d-828d-f8d06bd92ff2}`.
After the user closed the renderer window, PID 7580 remained alive without a
main window. The native log contained:

```text
WSX_OUTPUT_STATE: reason=closed_or_unavailable state=2 cancelled=1 width=0 height=0 result=-1000000000
WSX_NATIVE: failed stream quarantined before Guest device retirement
WSX_NATIVE: retaining native state and the original COM thread after failure
```

The read-only reference `native_main.rs` confirms an indefinite quarantine loop
after stream failure: the failed stream cannot service Guest UNREF/RESET. Sending
the ordinary stop event afterward cannot restore that service. Killing the Host,
unloading its Owner, removing its pointer or repeating the reload flag is not
a verified recovery. Tauri WM_CLOSE and CLI-requested native stop acceptance
concern different paths and do not cover this renderer close.

## Core changes

Status reports `observed_lifecycle` separately from historical startup milestones:
`native_running_unverified`, `failed_retained`, `native_exit_unverified`, or
`native_retired`. Known quarantine overrides apparent retirement markers. Host
liveness does not establish Guest, rendering or VM health, and does not identify
which external action caused an interruption.

Restart evaluates exact-instance retirement evidence without requiring a prior
CLI-written `stop_requested` phase. Stop refuses known quarantine/unexplained
exit and is idempotent after verified retirement. These changes implement safe
diagnosis and admission, not abnormal GPU recovery.

## Remaining component work

The paired component now intercepts close before output destruction and has a
new hash-pinned receipt. Reference repositories remain read-only. External distro-stop handling now passes real r11 acceptance. Original-VM exit
handling observed real VM exit but failed native memory closure during shutdown; a live lifecycle query
is still unimplemented. Signalling stop only after window destruction is too late.

Separate isolated Windows tests are required for idle/rendering close, duplicate
stop, stop/close races, distribution termination, VM shutdown, Host failure and
subsequent start. Record VM/process identities, GPU/memory retirement and logs
per case. Do not shut down unrelated instances to satisfy the matrix. Historical
Iris/RTX rendering compatibility does not establish lifecycle recovery.

## Isolated close repair evidence

Source inspection found an existing stop-event branch in the reference window
procedure. The shared output passed a null event and therefore took its
cancel-output branch. [The paired integration patch](../integration/graphics-close/README.md)
publishes the exact direct-bootstrap event and wires shared output to it.
Its real Win32/MSVC isolated fixture passed repeated WM_CLOSE without output
cancellation, required-event rejection, and reference-gated final window close.
Evidence: `artifacts/graphics-window-close-20261105-01`.

That isolated fixture is not GPU retirement evidence. The subsequent full
paired build and r11 real-window acceptance are separate results, recorded below.

## Real paired-runtime acceptance

`window-close-build-20261105-01` plus `graphics-close-pair-20261105-01` started the
owned r11 desktop and displayed a successful frame. The identity-checked real
WM_CLOSE test passed in `window-close-live-20261105-01/close`: exact Guest instance
retired, retained slots/pages/bytes were zero, Native/bootstrap closed, and Native
and input processes both exited naturally with code zero. Core reported
`native_retirement_confirmed=true`. The test never sent a CLI stop event, killed
a process, unloaded a module or restarted WSL/Windows.

The post-close restart plan confirms retirement and requires an explicit Owner
reload. It created no device and unloaded nothing. Second startup was not run.
A prior lifetime from the user's Windows reboot was separately recovered only
under the newer-boot, absent-Host/VM and clean-Guest gates. Its record was retained
with `native_retirement_confirmed=false`; this does not implement in-boot WSL
shutdown or abnormal GPU recovery. See [ADR 004](ADR-004.md).

## External lifetime follow-up

[ADR 005](ADR-005.md) supersedes the earlier external-stop implementation gaps.
The final pair passed real distro termination while retaining Ubuntu and leaving
r11 stopped, with exact Guest/device/GPU/memory/Host retirement. It uses an already
running system-distro helper, never a new helper launch in reaction to VM loss.
The original VM's HCS exit is a separate authority path; it still requires real
native cleanup. Actual `wsl --shutdown` was subsequently authorized and tested;
Guest RAM closure failed with `0x80070006`, and Core correctly retained the failed
lifetime. Evidence: `artifacts/shutdown-acceptance-20261105-01`.
