# Opaque buffer probe

The complete `wsld` source checkout retains the MIT-identified
`native-memory-build-09` diagnostic source and its Apache-2.0 Khronos Vulkan
headers from the reviewed
`virglrenderer-win32-port-11` source tree. `source-receipt.json` records the
copied inputs. The Apache license is in `APACHE-2.0.txt`; copyright and SPDX
notices remain in the individual headers.

The community binary package includes this README and the Apache license for
attribution; it does not include the probe source, headers, `build.ps1`, or
`source-receipt.json`. The build instructions below apply only to a complete
`wsld` source checkout containing those files. No prototype checkout is needed
to build from that complete source checkout.

Run `build.ps1 -Toolchain <Rust toolchain directory> -Output <new path>` from
Windows with the x64 MSVC tools and Windows SDK installed. The script verifies
the source receipt, compiles to an isolated temporary directory, and checks the
entry point without loading Vulkan. It emits a probe executable, not a product
runtime bundle or an admitted graphics component.

The probe takes an explicit vendor, device, Vulkan driver ID and Windows LUID.
It exports and imports an opaque Win32 buffer on the selected Vulkan device and
checks CPU visibility. It performs no GPU submissions. Success is limited to
that buffer and adapter: it does not establish opaque image import, Guest or HDV
mapping, desktop presentation, or safe retirement. Failure is not automatically
proof that a different renderer path is supported. Elevated use is explicitly
opt-in in the diagnostic and does not make driver selection authoritative.

The normal Windows build compiles this probe and embeds it in the public PE.
The launcher extracts it into the owned lifetime directory, verifies the bytes,
and matches results against the active adapter before selecting opaque-v1.
The launch gate checks the pinned runtime and Guest preflight independently.
The probe does not admit a staging runtime or guarantee later allocations.

The local adaptation accepts driver ID `0` to discover the driver associated
with the exact LUID rather than hard-coding a vendor. In that mode the caller
must compare the returned device and driver UUIDs with its independently
selected adapter before using the observation. The probe alone does not make
that comparison against the caller's inventory.

`--query-images` reuses the prototype's read-only opaque image-format query.
It reports 16 exact format, tiling and usage profiles and external semaphore
properties without creating a logical device or submitting GPU work. The Core
acceptance build verifies the query's adapter identity and output shape; a
successful query is not an image import or a Guest mapping.
