# Product C ABI v2

The authoritative SDK is abi-sdk/src/lib.rs plus abi-sdk/include/wsx_abi.h.
The SDK package version is 0.2.0. Pro consumes the fixed relative SDK and records
a verified digest; there is no independently maintained private declaration.
Legacy C v1 and experimental HostV1 are incompatible and are rejected by the
new required exports: wsx_pro_version, wsx_pro_init_v2, wsx_pro_run_v2 and
wsx_pro_shutdown_v2. Never search for the experimental DLL name or old exports.

Windows x64 C calling convention (__cdecl/extern C), repr(C), fixed-width
integers: HostV2 is 80 bytes, aligned to 8; Snapshot is 16 bytes. Both records
carry exact size/version 2. Capability bits 1 (diagnostics) and 2 (inventory)
and zero reserved fields are required. The diagnostic-start slot is explicitly
not an install API. C/Rust layout tests cover sizes, alignment and offsets.

Host callbacks are synchronous bounded queries/start requests; diagnostic workers
run in Core. Each process has at most 16 unreleased tasks. Handles are nonzero
monotonic u64, never reused, and have no meaning across processes. Start clears
output first; query requires initialized size/version and returns progress/state
(0 running, 1 succeeded, 2 cancelled, 3 failed). Cancellation is cooperative;
release invalidates the handle and joins outside the registry lock. Shutdown
closes admission and waits for all tasks and concurrent releases to drain.
Business import cancellation must later use irreversible-operation safe points,
not the diagnostic timer's cancellation behavior.

Strings are UTF-8 bytes with explicit u32 lengths and NO NUL terminator. Caller
owns every output buffer and required-size pointer. Short buffers return -8 and
remain unchanged; null/zero capacity is a length query. Inventory JSON is capped
at 1 MiB; if it grows between calls the caller retries. No allocator-owned object,
String, Vec, Future, trait object or ordinary Rust struct crosses the ABI.
Pointers must be aligned, accessible, live allocations. This is not a sandbox
for an untrusted DLL or malformed pointers.

Status: 0 success; -1 invalid input; -2 ABI mismatch; -3 already initialized/busy;
-4 fixture initialization failure; -5 closed; -6 invalid/retired handle; -7 task
capacity; -8 short buffer; -9 panic/internal error; -11 inventory failure;
-12 GUI/runtime failure. error_text returns stable English descriptions. Detailed
operation failures are written to diagnostics without credentials.

The host owns callback lifetime. DLL initialization copies the table. GUI entry
holds a runtime lease that excludes shutdown; each callback and close gate share
a session mutex. Close gates calls, requests cancellation and drains active work
before the GUI lease ends. Shutdown then clears the borrowed table. One GUI loop
per process, on the loading/main OS thread, COM STA and run_return only. Repeated
shutdown is harmless; reinitialization after a used GUI is refused. The host
retains the DLL even after failure until process exit. No FreeLibrary.

Rust panic guards cover ABI entry/callbacks. catch_unwind is not recovery from
SEH, invalid pointers, C++ exceptions, allocation abort or arbitrary corruption.
Host context is process-owned and retired only after DLL callback quiescence.
# Optional distro launch extension

The original `HostV2` remains unchanged. A host may resolve the optional
`wsx_pro_attach_launch_v1` export after v2 initialization and before the GUI
loop. `WsxLaunchV1` is 32 bytes on x64, version 1, with zero reserved fields.
It contains one blocking callback; Pro executes it on a bounded worker and
drains in-flight calls before shutdown. UTF-8 name and WSL registration GUID
are length-delimited and contain no NUL. Mode 1 opens a verified WSX desktop;
mode 2 opens an ordinary WSL terminal. Core rechecks registration and ownership
at invocation. The caller supplies an error buffer of at least 1024 bytes;
the callback returns a single UTF-8 error (possibly truncated at a character
boundary). It must never be retried merely to obtain a longer buffer because
the requested operation may have side effects. An older host can use the v2
GUI without this optional extension; launch controls require a newer host.
