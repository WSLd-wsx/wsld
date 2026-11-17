# Product architecture

The public executable owns Core and its workers. CLI uses ordinary Rust methods.
The GUI receives a minimal versioned function table and polls bounded task
snapshots; it cannot own another instance database. Two independent Cargo
workspaces keep public builds independent of private code. Public SDK definitions
are authoritative; private consumption is checked before build.

The executable keeps its console subsystem and identical bytes under all three
names. GUI load/run occurs on its main OS thread. COM STA, run_return, one loop,
and process-lifetime DLL retention are mandatory. WebView2 may spawn its normal
runtime processes; these are not business sidecars. No custom WslService.

Shared data root is %LocalAppData%\WSX. Local instance provenance and WSL
registration must be reconciled, never conflated. Persistent transitions require
ownership, atomic replacement and a held cross-process lock. Unknown or interrupted
operations remain recoverable facts rather than guessed running/success states.

Rootfs supplies the versioned Guest interface and pinned user-space graphics.
Core must admit a paired Windows host/kernel/component closure before launching
any desktop. GPU ownership remains in the independent verified graphics host,
not in Pro. Unit exit/window closure is not native-resource retirement.

See ABI.md, MIGRATION.md, ADR-001.md and ROOTFS-HANDOFF.md in wsld for contracts
and remaining admission limits. No prototype path is a runtime dependency.
