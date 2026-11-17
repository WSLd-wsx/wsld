# Kernel module delivery and selection

Community and Store packages carry `kernel-modules/index.json` and the
verified version-addressed archives. Build staging selects the newest metadata
for each archive and checks its size and SHA-256; packaging checks the staged
files again. An archive name identifies a candidate, not kernel compatibility
or publisher authenticity.

At desktop startup, Core first tries exact local versions without requiring a
network. If none is admitted, it discovers the kernel-module Release and ranks
exact remote candidates ahead of non-exact candidates. It compares non-exact
local and remote candidates within the same major and minor kernel series and
prefers the local copy when the candidate version is the same. An unavailable
network does not prevent trying local candidates. An invalid cached download is
retained separately before a fresh download; no cached bytes are overwritten.

Before selection is admitted, Core verifies the archive and six expected
modules, stages the group without overwriting files, and asks Guest control to
check actual kernel symbol CRCs and the identities of already loaded modules.
Normal preflight and loader checks remain authoritative; a version or CRC
match alone does not prove safe loading, GPU operation or successful retirement.
When every candidate fails, the command reports the failure and keeps the
diagnostic files. A private Release is inaccessible to an ordinary anonymous
product build; credentials exist only in explicitly enabled test builds.

Local acceptance on 2026-11-15 built both package formats and confirmed that
they contained the 6.18.54.1 and 6.18.58.1 archives and the index. A community
test build selected the latter on a 6.18.58.1 WSL kernel, passed live CRC
screening and reported the first Omarchy r19 frame. The user subsequently
confirmed display and input, and authorized normal stop. The native log recorded
Guest retirement, zero retained Guest RAM slots, normal input companion exit,
and closed native device and memory; subsequent observation found the matching
Host process and PCI device absent. The status query reported
`native_retirement_confirmed: false` because it does not infer retirement from
absence alone; the preceding log entries provide the separate retirement evidence.
A separate credentialed Windows test
used the product selector to discover, download and verify the private Release
archive; it did not load a module or launch a desktop. It also checked that
remote exact selection precedes a local nearby version. The private test is
feature-gated and is not part of a normal product build.
Subsequent read-only checks found that each of those six downloaded module
files had the same SHA-256 as the group used for the r19 first-frame run, and
all six matched the currently loaded module build notes. This establishes
matching test inputs; it does not mean that the remote download itself was
used to start the desktop.

As a separate read-only negative check on that running 6.18.58.1 kernel, the
6.18.54.1 archive's modules were compared with the loaded module build notes.
The check rejected the differing `drm_kms_helper` identity before any load or
unload. This exercises loaded-module identity rejection only; it is not an
end-to-end nearest-version fallback or a test of its CRC result.

A further read-only audit used the actual running 6.18.58.1 WSL kernel image
and the six files extracted from the 6.18.54.1 archive. The product CRC audit
returned `symbol_crc_compatibility_admitted: false`; all six module records
had mismatches: `drm_kms_helper` 135, `virtio-gpu` 146, `virtio_dma_buf` 2,
`virtio_msg` 15, `virtio_msg_vsock` 19, and `wsx_virtio_msg_pci` 22. These
counts are per module and can include the same symbol more than once. The Ubuntu test
environment used its `/usr/sbin` kmod tool paths in place of the Guest's
`/usr/bin` paths. No module was loaded or unloaded by this audit.

The private community workflow run `43712142522` built source commit
`0555caf75a763b0d0309ea965e9342dbe00e5226`, collected the Release modules,
and read back its private pre-release assets. An independent download verified
the ZIP SHA-256 (`79b0740ad471cb7bdcedcd730863c4a6697d50e3d0e20d7e29a4d3807811979f`),
both module archives against the bundled index, and identical hashes for the
three executable names. The downloaded CLI returned its version successfully.
This is packaging and CLI evidence; it does not establish desktop acceptance,
public availability of the private module Release, or license clearance.
