# WSL registration audit, 2026-11-06

Scope: read-only observations on the current Windows machine, WSL 3.0.3.0,
comparing its `Ubuntu-24.04` with the WSX Omarchy r12 and CachyOS r1 test
registrations. No registry values, distro state or user configuration were changed.
Registry and running-default-user observations are archived in
`artifacts/wsl-registration-audit-20261106-01/`.

`wsld.exe` imports an image by invoking the system `wsl.exe --import NAME DIR TAR
--version 2` (`core/src/import.rs`); WSL creates the distro registration. Core's
registry adapter (`core/src/platform/registration.rs`) opens and reads keys; it
does not create or write the distro's registry entry. The separate legacy WSL
service maintenance script can mutate service/COM/plugin registrations and is
not part of this normal install path or evidence that it ran during this audit.

The three observed GUID subkeys use the same kinds for their common value names.
Each reports Version DWORD 2, Flags DWORD 15, State DWORD 1, RunOOBE DWORD 0,
Modern DWORD 1 and VhdFileName string `ext4.vhdx`. Their GUID, DistributionName,
BasePath, ShortcutPath and TerminalProfilePath are naturally per-registration.
Ubuntu has Flavor `ubuntu` and OsVersion `24.04`; the WSX entries have Flavor
`arch` and did not contain OsVersion. DefaultUid was 1000 for Ubuntu and 0 for
both WSX entries. These values are observations, not a public schema guarantee;
`State` is not used as a substitute for WSL's live process state.

Both WSX Guests contain `/etc/wsl.conf` with `[user] default = wen`; direct
`wsl.exe -d NAME --exec id` reported username wen and UID 1000 for each running
WSX distro. Ubuntu similarly reported wgy and UID 1000. Thus the observed registry
DefaultUid difference is real, but does not mean these current WSX sessions start
as root. Initialization code sets the imported Guest's default in wsl.conf.
Microsoft documents wsl.conf as the default-user configuration for imported
distros. These observations do not establish that all future WSL versions or all
install methods use identical internal fields or value semantics.

Microsoft documents `wsl --import`, `wsl --list` and the WSL API configuration
queries. Its documented WslGetDistributionConfiguration function returns WSL
version, default UID, flags and environment, but not an equivalent public API for
all registry fields or installation paths. Direct inspection of Lxss internals
therefore remains a version-sensitive implementation dependency. Prefer documented
interfaces for the information they actually expose and keep disk/registration
identity checks explicitly guarded and diagnosed; do not manufacture Ubuntu's
registry values in WSX entries.
