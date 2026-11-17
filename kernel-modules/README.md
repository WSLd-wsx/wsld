# Kernel module build inputs

This directory is the repository's independent kernel-module source and build
input boundary. It remains part of the `wsld` repository; Core and the Windows
Host do not become part of the kernel module merely by being stored alongside it.
The module files retain their individual upstream and contributor notices and
licenses. This directory move does not assign a new license or establish the
legal status of combined works. Repository build scripts under `scripts/` keep
their existing licensing; this directory is not a standalone build system.

`compat-drm-client-lock/` retains four upstream DRM sources whose SHA-256 hashes
match the earlier verified graphics build and Microsoft commit
`d504d40ab83839cb61942a812a99d183c42227b9`. The builder uses them only when all
four target kernel source files match those exact bytes, not by version label.
The input lock and files participate in the build generation and preserve the
original upstream notices. This source match does not prove module loadability.

`reference-6.18` contains the exact 131 source inputs retained from the
previous 6.18.58.1 module build. It is not a general Linux kernel module source
tree. `sources.lock.json` pins the individual bytes, and the build generation
includes the lock and source tree. Preserve each file's original copyright and
SPDX notices. The
Linux kernel `COPYING` is retained as `COPYING.kernel`; per-file notices may
specify additional or different grants. This snapshot does not establish a
complete corresponding-source or binary redistribution audit.

The source evidence is `kernel-6.18.58.1-20261110-01`: Microsoft kernel source
commit `a989171929bed4ea681be8d77333845ce1c1732a`, the prior
`opaque-consumer-kernel-05` GPU input inventory, and the
`multi-owner-kernel-20261106-01` Owner input inventory. The prior migration
replaced four DRM helper inputs with the 6.18.58.1 upstream versions; the
retained adaptation receipt identifies each before/after hash. The virtio-msg
files retain upstream Google/Linaro GPL notices. The WSX opaque UAPI retains
its MIT SPDX notice. Hashes establish input identity, not complete authorship
or a legal conclusion.

`scripts/build-kernel-modules.py` takes a separately supplied prepared kernel
build tree and compiler toolchain. It verifies its pinned kernel release,
configuration, `Module.symvers`, source lock and source files, then builds in a
new output directory. The caller must supply an isolated build environment and
a read-only kernel tree. A successful build is not a kernel source provenance
proof, live CRC admission, permission to load a module, GPU acceptance, or a
release authorization. The current builder intentionally rejects other kernel
versions; it does not automatically repair source incompatibilities.

`scripts/run-reference-kernel-build.sh NEW_WORKSPACE [PACKAGE_CACHE]` prepares
an isolated build root from the pinned Ubuntu package lock, obtains and verifies
the kernel.org GCC archive and fixed Microsoft source archive, and builds six
modules. Set `WSX_TARGET_KERNEL_IMAGE` to the target kernel. Optionally set
`WSX_TARGET_KERNEL_NOTES` to independently obtained notes; otherwise the script
extracts the image's own notes and records their hashes. Image-internal notes
only establish internal consistency, not publisher provenance or that the image
matches a running kernel. The builder runs `modules_prepare` and
derives `Module.symvers` from the build-ID-matched image; it does not rebuild
the kernel. As an explicit diagnostic fallback only,
`WSX_ALLOW_FULL_KERNEL_BUILD=1` builds the reference vmlinux instead. Package
downloads happen before the network-isolated bubblewrap build; package maintainer
scripts are not run. A cache is optional and each cached package is rehashed.
The workspace must be new and remains on failure for inspection. The build
requires `python3`, `curl`, `dpkg-deb`, `tar`, `bwrap`, sufficient disk space and
a Linux environment that permits bubblewrap namespaces. It never boots the
built kernel or loads any module. The currently pinned input is only the
6.18.58.1 x86-64 source/configuration and GCC 13.2.0, not a build recipe for all
WSL kernel versions. Its files and logs need independent compatibility and
hardware acceptance before distribution.

The manual build workflow obtains Microsoft.WSL.Kernel 6.18.58.1-1 from the
Microsoft WSL package feed and verifies the pinned package, x64 kernel and
configuration digests before building. These pins reproduce the reference bytes;
they are not a signature check or proof that the package's recorded source
revision corresponds to the separately pinned public source tree. The workflow
does not establish historical-version coverage or publish any module package.

After a successful build, the finalizer rehashes all six modules, the kernel
configuration and `Module.symvers`. It retains the modules, receipts, logs and
kernel evidence, plus the reusable package/toolchain downloads and build root;
only the target kernel build tree and module object intermediates are removed.
Any failed verification retains intermediates for diagnosis. The optional cache
may contain pinned `.deb` files, `gcc.tar.xz` and `kernel.tar.gz`; every reused
archive is rehashed before use.

The fixed reference inputs are the Microsoft source archive for commit
`a989171929bed4ea681be8d77333845ce1c1732a`, the retained running-kernel
configuration, the Ubuntu Noble build-package lock, and the kernel.org GCC
13.2.0 archive. The previous package lock was recorded from Ubuntu signed
indexes; rechecking its file hashes preserves those recorded bytes but does not
independently authenticate future mirrors. The source archive, compiler and
package locks need their own provenance and license review before redistribution.
No rootfs or system package installation is performed.

For CI, notes extracted from the same image establish only internal build-ID
consistency, not that the image is an authentic Microsoft target. The image and
source relationship must be verified independently before release admission.

The isolated build-root preparation logic and its package lock originate from
the retained `prototypes/wsxdrm/prepare-build-root.py` experiment and
`wsxdrm-build-20261012/build-env/build-packages.lock.json` evidence. They have
been copied into this repository so a future build does not need the prototype
checkout. The input rootfs may contain third-party build tools with their own
licenses; this lock is not a complete distribution notice inventory.
