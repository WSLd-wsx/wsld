# Image CPU compatibility

Core enforces the image's completion-bound `cpu_requirements` before rootfs
acquisition, again after acquisition, and before import allocation. The currently
published schema accepts exactly `{"architecture_level":"x86-64-v3"}`. Unknown
levels, extra keys and malformed requirements are rejected. Omarchy manifests
without an additional CPU requirement retain their previous behavior; catalog
validation requires an explicit v3 requirement for CachyOS Plasma.

## Requirement provenance

Published `wsx-cachyos-plasma-2026.11.4-r1` declares x86-64-v3. Its manifest hash is
`73df030b3b38fe66f978ec1891323ab062d4424dbec2e72bc73047bc6571d91e`, checked against
its completion marker in `artifacts/install-cachyos-20261105-01`.

Read-only rootfs references: `scripts/finalize-cachyos.py` explicitly writes the
requirement; `schema/manifest-v1.schema.json` and `scripts/contract.py` enforce it.
`docs/cachyos-validation.md` records that the historical private KWin binary's ELF
notes require v3. `scripts/stage-cachyos.py` repeats the v3 check across exposed
Guest CPUs before Plasma session setup. The current recipe carries this explicit
contract; it is not an automatically inferred per-build ISA minimum. This change
does not claim to have re-audited every ELF in the new r1 archive. Only a rootfs
build/component audit and a new compatible release contract could lower the
published requirement. CPU or GPU model names are not evidence for lowering it.

## Native Windows observation

`core/src/cpu.rs` implements a pure admission policy. `platform/cpu.rs` contains
Windows and instruction adapters; no new crate, privileged helper or driver is
needed. Native AMD64 is verified with IsWow64Process2, rejecting emulated x64 on
another native architecture. A short-lived helper thread enumerates actual active
processor-group masks, including non-contiguous indices. It pins only itself to
each logical processor and collects bounded CPUID leaves. Query/pinning errors or
a changed processor topology reject admission. The calling CLI/GUI thread's
affinity is unchanged; the helper exits after collecting observations.

The policy checks baseline x86-64, v2 and v3 feature prerequisites, including
SSE3/SSSE3/SSE4, CX16, LAHF/SAHF, POPCNT, AVX/AVX2, BMI1/BMI2, F16C/FMA, LZCNT and
MOVBE. It also checks Windows SSE2 support, XSAVE, OSXSAVE and enabled XMM/YMM
state in XCR0. XGETBV is executed only when both XSAVE and OSXSAVE are advertised.
Missing leaves never supply inferred feature bits. No AVX workload is executed
as a compatibility test, no vendor exception exists, and build-time CPU flags do
not substitute for runtime observation. Failure names the logical processor and
missing capabilities. Additional features such as AVX-512 are not required.

Sources: [LLVM's x86-64 feature-level definitions](https://github.com/llvm/llvm-project/blob/main/llvm/lib/Target/X86/X86.td),
[Intel's AVX CPU/OS detection procedure](https://cdrdv2-public.intel.com/821612/248966-Optimization-Reference-Manual-V1-050.pdf),
[Windows native architecture](https://learn.microsoft.com/en-us/windows/win32/api/wow64apiset/nf-wow64apiset-iswow64process2),
and [processor group inventory](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-group_relationship).

## Evidence and limits

`wsld.exe --install --pre-release wsx-cachyos-plasma-2026.11.4-r1 --check`
returns current CPU/OS admission alongside exact-channel release metadata. The
`cpu_compatibility` object includes checked processor count, CPUID register values
and XCR0 observations. These are invocation-scoped diagnostics, not a persisted
permission to skip future checks. No-force and no-override behavior is deliberate.

This reports the Windows host's currently exposed CPU/OS capabilities. It does
not prove a future WSL VM exposes the identical feature set, nor establish Guest
boot, package ABI, GPU or desktop compatibility. The Guest's independent check
remains mandatory before its session starts. This change does not wake another
distro, install anything for probing, or enable CachyOS desktop lifecycle in Core.

Regression tests cover an independently specified complete register fixture,
every mandatory bit removed separately, absent leaves, disabled or missing OS
AVX state, mixed logical processors, unknown schemas, sparse topology and native
Windows observation without caller-affinity changes. Synthetic failures are not
claimed as separate physical CPU models tested.
