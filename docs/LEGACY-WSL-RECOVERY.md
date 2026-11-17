# Historical service deployment recovery

## Recovery observed after user action

At 2026-11-05T04:26Z the user reported removal of the old service. Read-only
checks observed a new Windows boot, the official automatic WSLService running,
six restored COM paths, no historical plugin value or native/input Hosts, and
the old service executable absent. Four official binaries match their prepared
hashes and have valid Microsoft signatures. The product r11 status succeeds,
preserving its registration/disk and reporting the distro stopped without waking
it. See `artifacts/legacy-recovery-observed-20261105` and `docs/STATUS.md`.

The historical transaction below is now superseded by this observed state. Do
not rerun StageReboot, FinishStock or Restore against the changed environment.
Normal GPU retirement was not demonstrated by this reboot-based recovery.

## Observed blocker, 2026-11-05

The current historical service-owned Native Host has no independent public stop
entry point. Its plugin requests retirement in `OnVMStopping`, waits 1 second,
then returns `E_PENDING` when its worker is still active. The deployed service's
`PluginManager::OnVmStopping` logs that result without awaiting retirement;
`LxssUserSessionImpl::_VmTerminate` subsequently resets the utility VM. This
does not establish the required Guest/device/renderer retirement ordering.

Read-only source references in the separate prototype repository:

- `prototypes/venus-rutabaga/device-host/production/service-plugin.c`, `stopping`.
- `artifacts/venus-rutabaga/rtx3080ti-service-source-03/source/src/windows/service/exe/PluginManager.cpp`, `OnVmStopping`.
- The same source tree's `LxssUserSession.cpp`, `_VmTerminate`.

A previous native log reports `Expected one PCI function ... found 0`, failed
Guest retirement and retained native state. This is historical failure evidence,
not a new attempted stop of the current lifetime. Current Host PID 14744 and
input PID 14884 are children of custom service PID 3936. The current log contains
device startup but no observed Guest frame. Absence of frames does not prove
absence of device resources. Do not signal a fabricated DRAINED response, kill
the Host, unload its driver or relabel service shutdown as successful retirement.

The old `--owner` deployment differs from the admitted direct Host used by the
r10 product path. New r11 launch remains blocked by the old Owner. Updating Core
does not replace code already loaded in the old service process.

## Prepared Windows-local maintenance kit

`%LOCALAPPDATA%\WSX\maintenance\legacy-retirement-20261105-02` contains the
schema 2 script, original files, SHA-256 records, registry exports, current and
historical failure logs, exact six COM paths, and identical public executables.
The earlier schema 1 script is superseded and disabled.
Repository evidence: `artifacts/legacy-retirement-20261105-01`.

The existing official WSL service and three companion COM DLLs passed Microsoft
Authenticode verification. Official service SHA-256:
`262951e58df4bc82b63a79cbc64439c7d3ad7baa4344834442a5a82b2f9dd293`.
Old custom service SHA-256:
`73e5738a1ba74af5d1e49d127cb03529d097ac19145183d83c30d20a34ac2a14`.
Old Native Host SHA-256:
`e74c3f0a8234c2d03a6db29c29d88b8ac6610391feb769b1d5dc0423e2e3414f`.

Preparation and refusal checks passed on Windows PowerShell: both scripts parse,
all three mutation actions reject without explicit confirmation, and service
configuration/PID, plugin, six COM paths and Host identities stay unchanged.
The user subsequently authorized the next-start change only. `StageReboot`
completed at 2026-11-05T03:47Z: service start mode changed from Auto to Disabled;
PID 3936 stayed Running, Host/input PIDs 14744/14884 and creation times stayed
unchanged, and the configured service/plugin paths remained the same. Evidence
includes the action receipt and `staged-inspection.json`. Windows restart,
post-reboot recovery and rollback have **not been executed**. The next-start
change is not GPU retirement acceptance. Once the service stops, WSL cannot
start again until its startup policy is restored through authorized maintenance.

## Explicitly authorized maintenance only

This is a machine-specific recovery procedure, not a product runtime dependency.
Run from elevated Windows PowerShell. It requires separate authorization because
normal Windows restart interrupts all current applications and WSL sessions.
It is recovery from a legacy lifecycle defect, not proof of normal GPU teardown.

1. Review the kit and use `legacy-wsl.ps1 -Action Inspect` (read only).
2. After authorization, use `-Action StageReboot -ConfirmMaintenance`. This only
   disables the service's next automatic start. It does not stop the running
   service, VM or Owner and does not change COM registration or plugin loading.
3. Save all work and manually perform a normal Windows restart. The script never
   initiates restart, forces process termination or requests `wsl --shutdown`.
4. From the same Windows-local kit after restart, use
   `-Action FinishStock -ConfirmMaintenance`. It requires a different Windows
   boot, disabled/stopped service and no graphics Host. It revalidates files and
   official signatures, registers the two official proxy DLLs, restores six COM
   paths, removes only the historical WSX plugin value, selects the official
   service, restores its prior start policy and starts the official service.
   No distro or desktop is launched. Registry/file backups remain intact.

If staging must be cancelled **before** reboot, `-Action Restore
-ConfirmMaintenance` restores only the original start policy after checking the
original lifetime and configuration. After reboot, `Restore` requires the same
disabled/stopped/no-Host gate; it restores the prepared custom proxies, COM paths,
plugin and service configuration, without starting the custom service. It is not
a generic rollback after a newly started service. A failed intermediate action
retains its receipt and requires inspection; do not force past its gate.

The workspace's physical disk may need to be mounted again after reboot. Do not
guess its disk number or auto-mount it. The Windows-local kit and its `product`
directory remain available independently of the WSL workspace.

After recovery, separately verify the official service/COM paths and absence of
old Owners, inspect the r11 registration, then resume account initialization and
the existing direct-Host startup path. Account/password entry, first frame,
input, stop and restart remain separate real acceptance steps.
