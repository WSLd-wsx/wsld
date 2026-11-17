# Native product build and GUI acceptance, 2026-11-02

Rust 1.95.0, x86_64-pc-windows-msvc; installed MSVC tools directory 14.51.36231,
Windows SDK 10.0.26100.0; runtime WebView2 156.0.4314.58. The install requested
restart, but tests ran successfully without rebooting. Build profile: Cargo dev
(unoptimized, debug information). This is not a signed/optimized release.

## Results

| Check | Actual result |
| --- | --- |
| Public native tests | 16 passed, including ABI buffers/handles, task drainage, protocol rejection and cross-process store locking |
| Private native tests | 2 adapter/session gate tests passed |
| PE contract | One x64 console PE copied three times; hashes equal; public imports have no Pro/WebView dependency |
| SDK / C ABI | Shared SDK digest, native C layout, all four v2 exports and C import-library client passed |
| Windows processes | 24 cases passed with acceptance DLL adjacent; 24 also passed with ordinary DLL adjacent |
| CLI cases | Help/version/catalog/diagnostic/error paths show pro=0 webview=0; argv[0] spoofing and unknown names rejected |
| Loader negatives | Missing DLL/export, wrong ABI/architecture, failed init and cwd/PATH decoys passed |
| PowerShell | Pipeline, redirect and invalid-command exit cases passed for both CLI names |
| Real GUI lifecycle | Progress/success/cancel/failure/invalid input, JS exception then Core call, close during task and return to Core passed |
| Real WM_CLOSE | Sent to owned window during active task; drained and returned to Core |
| Thread / children | Main OS thread consistent; six observed WebView2 processes plus conhost retired in each GUI scenario |
| Missing runtime | Per-child nonexistent runtime path fails explicitly; system runtime untouched, no installer |
| Normal DLL | Built without acceptance feature; real window/screenshot/WM_CLOSE/Core-return and children retirement passed |
| Reopening | Subsequent new-process windows succeeded; same-process event-loop recreation is not enabled |

## Artifacts and hashes

Public files are under artifacts/native-20261102-a/: build.log, hashes.json,
package/{wsld.exe,wsx.exe,wsxdesk.exe}, and fixtures. All three EXEs have SHA-256:

`53BC5E74F0316E36AF8B4C53C0650393D7D18EFBA1C2158BDB618B94AC9E61AC`

Private evidence stays in wsx-pro/artifacts, outside this public repository:

- native-20261102-product/package/wsxpro.dll (ordinary build):
  `1BC42AD5BC32DFF22C23A8F1B537343BB43F3387008FC83736D5413A2D697E77`
- native-20261102-c/package/wsxpro.dll (acceptance feature):
  `7936C7B17CED29D38D03F12026770E2BA1DA1D1FA5D062E51EB2EF7B4EF0FD46`
- acceptance-20261102-b/: process, shell, GUI lifecycle/WM_CLOSE, runtime-negative
  JSON results and stdout/stderr; gui-lifecycle/window.png.
- product-smoke-20261102/: ordinary GUI/window.png, process cases, staging hashes.
- native-acceptance-20261102.json: consolidated results, artifact sizes and hashes.

The verified ordinary four-file package is also retained in the isolated NTFS
location recorded in wsx-pro/artifacts/product-smoke-20261102/staging/package.json.
It is a temporary test package, not an installation. Product GUI rejects direct
UNC execution; use the staging script instead of weakening path admission.

## Reproduce

Run in Windows PowerShell from the workspace, using fresh evidence directories.
Toolchain is the root containing bin/rustc.exe and bin/cargo.exe. The actual run
used the workspace's copied .build-tools/windows-rust development toolchain;
there is no prototype build/runtime dependency.

```powershell
$toolchain = (Resolve-Path .\.build-tools\windows-rust).Path
& .\wsld\scripts\build-msvc.ps1 -Toolchain $toolchain -Evidence "$PWD\wsld\artifacts\core-new"
& .\wsx-pro\scripts\build-msvc.ps1 -Toolchain $toolchain -Evidence "$PWD\wsx-pro\artifacts\pro-test-new" -Acceptance
& .\wsx-pro\scripts\acceptance.ps1 -CoreBuild "$PWD\wsld\artifacts\core-new" -ProBuild "$PWD\wsx-pro\artifacts\pro-test-new" -Evidence "$PWD\wsx-pro\artifacts\acceptance-new"
& .\wsx-pro\scripts\build-msvc.ps1 -Toolchain $toolchain -Evidence "$PWD\wsx-pro\artifacts\pro-normal-new"
$package = & .\wsx-pro\scripts\stage-package.ps1 -CorePackage "$PWD\wsld\artifacts\core-new\package" -ProPackage "$PWD\wsx-pro\artifacts\pro-normal-new\package" -Evidence "$PWD\wsx-pro\artifacts\stage-new"
& .\wsx-pro\scripts\gui-acceptance.ps1 -Package $package -Evidence "$PWD\wsx-pro\artifacts\smoke-new" -Scenario smoke
```

Build cache defaults to a separate NTFS TEMP directory. This run used
WSX_BUILD_CACHE=$env:TEMP\wsx-product-msvc-offline for Pro after a stalled initial
index request, with cached registry entries/archives and unchanged Cargo.lock.
The normal DLL build passed with CARGO_NET_OFFLINE=true. Clean network downloads
are not proven by that offline build; caches are disposable, not deployment files.

## Failures retained and limits

The first GUI test (acceptance-20261102-a) had correct lifecycle markers, exit 0,
screenshot and retired children, but falsely failed because Windows PowerShell
returned AutomationNull for empty stderr. gui-acceptance.ps1 now casts logs to
strings and also requires observed WebView2 children for both lifecycle and
WM_CLOSE retirement checks. A fresh run passed; old evidence was not overwritten.
Initial dependency-index/cache preparation failures are retained in private
native-20261102-a/b and offline retry logs. Only the stalled initial Cargo download
was terminated after a successful replacement build; no GUI was force-terminated.

No import, account/password initialization, desktop/GPU first frame/input/stop,
clean Windows deployment, publisher-signature validation, same-process second
loop negative invocation, ancestor-reparse execution test, Ctrl+C, ShellExecute or
Explorer flicker acceptance is claimed. conhost was observed; console UX remains
an ADR item. Compilation/diagnostic tasks do not implement unfinished business
operations. No existing WSL distributions, services, owners or disks were changed;
no commit, push, publication, installer packaging or license change occurred.
