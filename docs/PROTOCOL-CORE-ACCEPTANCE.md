# Protocol-only installation admission

New installation resolution and post-download admission require a release-declared
Guest protocol. Missing metadata cannot select the legacy adapter. Existing
import-receipt reconciliation remains separate. Control calls reuse the existing
protocol 1.0 adapter for capabilities, account/preflight, session observation and
retirement, and display operations. No Guest private Python API was introduced.

The published Omarchy r12 and CachyOS 2026.11.5-r1 manifests are regression fixtures
under `core/tests/fixtures/protocol-*.json`; changing an image revision does not
change protocol admission. Unsupported majors and missing declarations reject.

## Evidence (2026-11-05)

Native Windows Rust/MSVC build: `artifacts/protocol-core-build-20261105-01/`.
69 Rust tests and two mocked Python Guest-boundary tests passed. The three public
EXE copies have identical SHA-256:
`daf3b9a891eee2469391350b33050cdf0b3ace1cfd36a202fc1be46f9e29375e`.

The compiled `package/wsld.exe` executed these read-only checks against published
release metadata:

```powershell
.\wsld.exe --install --pre-release wsx-omarchy-4.0.6-r12 --check
.\wsld.exe --install --pre-release wsx-cachyos-plasma-2026.11.5-r1 --check
.\wsld.exe --install --pre-release wsx-omarchy-4.0.6-r11 --check
```

The new images returned 0; the old r11 returned 1 with missing Guest protocol
metadata. Logs are retained beside the package. These calls did not download
rootfs payloads, import distributions, initialize accounts or start desktops.
New-image WSL/GPU acceptance remains outstanding. Runtime byte admission and the
published Native pairing still apply when starting a desktop.

## Omarchy r12 live acceptance

After the user restarted WSL, HCS confirmed the original VM was absent and Linux
showed no old Owner module. The explicitly authorized cleanup terminated only the
identity-checked old Host PID 21104. Its previous retirement failure remains a
failure, not a normal retirement result.

The compiled CLI launched the already initialized `wsx-omarchy-4.0.6-r12` with exit
code 0. Native PID 7888 used Guest protocol mode and observed a first frame.
Protocol display query/apply fitted 1920x1440 at scale 1.0 / Windows DPI 96. The
user confirmed visible desktop, mouse and keyboard operation. Evidence is in
`artifacts/r12-protocol-launch-20261105-01/`, including the Native log and
`acceptance.json`. The window was left running. Normal close/retirement, restart,
sustained stability and simultaneous desktops are not accepted by this result.
