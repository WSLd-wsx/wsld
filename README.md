# wsld

**A Linux desktop on Windows, powered by WSL 2 and your GPU.**

`wsld` is the engine and command-line interface for WSX. It installs supported
Linux desktop images into Windows Subsystem for Linux (WSL), manages their
lifecycle, and connects a Linux desktop to a Windows graphics runtime. The goal
is to let a WSL desktop use the Windows host GPU for accelerated graphics while
remaining accessible as a desktop window on Windows.

WSX uses a paired Windows Host, Linux Guest and kernel-module graphics stack.
The GPU is shared through that stack; this is not a claim of exclusive physical
PCIe passthrough or support for every GPU, driver, WSL kernel or Linux desktop.
Hardware acceleration and desktop startup depend on the actual compatible
components and runtime checks.

> **Alpha software:** compatibility and features are changing. Use a test
> environment, keep backups, and do not depend on this build for critical work.
> An available download or successful build does not mean that every desktop,
> GPU or update path has passed acceptance.

## What wsld does

- Finds and installs supported WSX desktop images into new WSL 2 distributions.
- Checks the Windows, WSL and graphics environment before installation, and
  verifies downloaded image and component data.
- Starts, observes and stops supported WSX desktops; fits the desktop to the
  current Windows monitor work area and DPI.
- Provides a Windows manager for viewing WSL distributions and opening a
  terminal. The community command line remains available without the Store
  edition.
- Reuses a verified local image cache when available. Component and kernel
  compatibility are checked separately from a successful image download.

The current catalog focuses on WSX images, including Omarchy-based builds.
CachyOS Plasma has also been exercised as a specific pre-release candidate;
availability and compatibility vary by exact release. `wsld` does not adopt or
convert arbitrary existing WSL installations into WSX desktops. See
[installation](docs/INSTALL.md) and [desktop commands](docs/DISTRO-DESKTOP.md).

## Requirements

- **Windows:** 64-bit Windows on an x86-64 PC, with hardware virtualization
  enabled and WSL 2 available. The installer checks for WSL **3.0.3 or later**.
- **Administrator access (Community Edition):** Use an elevated Command
  Prompt or PowerShell for installation and desktop commands. WSX needs Windows
  Host Compute Service (HCS) permissions to attach and remove the virtual PCI
  device used by the desktop. Elevate the same Windows account that owns the
  WSL distribution. Starting the desktop manager (`wsxdesk.exe`) from Explorer
  requests elevation through UAC.
- **Graphics:** a Windows GPU and driver with the Vulkan capabilities required by
  the selected WSX graphics path, plus a compatible WSL kernel, WSX modules and
  matching Host/Guest components. The active display adapter is checked at
  runtime. A particular GPU family or driver is not automatically supported.
- **CPU:** image requirements vary. For example, the recorded CachyOS Plasma
  candidate requires x86-64-v3; WSX checks declared requirements before import.
- **Desktop manager:** Microsoft Edge WebView2 Runtime is required for the GUI.
  The package does not install it automatically.
- **Storage and network:** a local fixed drive with enough free space for the
  download, extracted image and WSL virtual disk, plus internet access to obtain
  the release metadata and image. Space requirements vary by image.

Install or update WSL using Microsoft's instructions, then restart Windows if
its setup asks you to. Open PowerShell with **Run as administrator**, change
to the extracted WSX directory, and run:

```powershell
wsl.exe --version
.\wsld.exe --check-environment
```

The environment check reports observed prerequisites; it does not guarantee a
successful desktop session or replace a hardware acceptance test.

## Author-tested environments

The project has been tested on the author's Windows/WSL environments, with
results limited to the specific builds and operations recorded in the project:

| Hardware | Recorded observations |
| --- | --- |
| Intel Iris Xe integrated graphics | A CachyOS Plasma candidate displayed a desktop with mouse and keyboard; an Omarchy candidate also has a recorded zero-copy desktop lifecycle. |
| NVIDIA GeForce RTX 3080 Ti | Omarchy candidate desktop and selected graphics lifecycle tests were exercised on the active NVIDIA adapter. |
| Intel Core i5-11320H, eight logical processors | CPU and image-admission checks were exercised; those checks alone did not install an image or test a desktop. |

These are observations from different recorded tests, not a compatibility matrix
or a guarantee for the latest build. AMD GPUs, different drivers, mixed-GPU
systems and untested Windows/WSL versions need independent validation. See
[project status](docs/STATUS.md), [CachyOS integration](docs/ADR-012.md) and
[CPU compatibility](docs/CPU-COMPATIBILITY.md).

## Community Edition: download and use

1. Open [GitHub Releases](https://github.com/WSLd-wsx/wsld/releases) and select
   a Community Edition release. During alpha, candidates may appear under
   **Pre-release**. Download `wsx-community-windows-x64.zip` and the accompanying
   `wsx-community-materials.zip` and `SHA256SUMS` from the **same release**.
2. Check the downloaded ZIP hashes against `SHA256SUMS`, then extract the
   Windows ZIP to a local fixed drive. Keep the files and directories together;
   the executables alone do not contain the complete paired runtime. The
   materials ZIP contains component notices and source evidence.
3. Open Command Prompt or PowerShell with **Run as administrator** using the
   Windows account that owns the WSL distribution, then go to the extracted
   directory and check the environment. Run installation and desktop commands
   from that terminal. Listing distributions, help and version work in a
   standard terminal. When opening `wsxdesk.exe` from Explorer, approve the UAC
   prompt with the account that owns the distribution.

   ```powershell
   Get-FileHash .\wsx-community-windows-x64.zip -Algorithm SHA256
   .\wsld.exe --check-environment
   ```

   Run `Get-FileHash` from the download directory, or provide the full path to
   the ZIP. Compare its value with the corresponding line in `SHA256SUMS`; do
   the same for the materials ZIP.

4. List eligible stable desktop images and install Omarchy:

   ```powershell
   .\wsld.exe -l -o
   .\wsld.exe --install wsx-omarchy
   ```

   The install command chooses the newest eligible stable Omarchy image. If the
   stable catalog is empty, there may be no eligible stable image yet; check the
   [rootfs Releases](https://github.com/WSLd-wsx/wsx-rootfs/releases) page for an
   available pre-release instead. Use the exact tag
   shown by the catalog to select a specific version. A pre-release requires its
   exact published tag and `--pre-release`:

   ```powershell
   .\wsld.exe --install --pre-release <EXACT_RELEASE_TAG>
   ```

   Installation imports a new WSL distribution and normally continues to first
   launch. Follow the local terminal prompts to create the Linux account. Use
   `--no-launch` to import without starting it, or `--check` to check release
   metadata without downloading or importing. If first launch fails after import,
   resume with the distribution name instead of reinstalling.

5. Find the actual distribution name in WSL, then use that name for desktop
   commands:

   ```powershell
   wsl.exe -l -v
   .\wsld.exe -d "DISTRO_NAME"
   .\wsld.exe -d "DISTRO_NAME" desktop status
   .\wsld.exe -d "DISTRO_NAME" desktop fit
   .\wsld.exe -d "DISTRO_NAME" desktop stop
   ```

   Replace `DISTRO_NAME` with the value in WSL's **NAME** column; quote names
   containing spaces. `desktop stop` stops the WSX desktop. For the separate
   WSL distribution termination command, run `wsld.exe --help`.

`wsld.exe` and `wsx.exe` are equivalent command-line entry points, so the same
commands also work with `.\wsx.exe`. Run `--help` for available commands and
see [installation details](docs/INSTALL.md) for exact versions, caching and
recovery behavior.

## Community GUI

Launch `wsxdesk.exe` from the extracted package to open the WSX manager. Its
community interface shows registered WSL distributions and their running state,
identifies verified WSX desktops, refreshes the inventory, and opens a terminal
for a selected distribution. It also shows the CLI command when an action requires
the Store edition.

Community desktop installation and desktop launch use the CLI described above;
one-click installation and desktop launch in the manager are Pro features of the
Store edition. The GUI and GPU desktop are different components: a visible manager
window does not establish that a Linux desktop or GPU path has passed validation.

## Alpha status and planned work

The alpha is being updated frequently. Areas under active development and
validation include broader GPU and driver coverage, additional WSL kernel and
Linux desktop combinations, more complete display and lifecycle testing, and
improvements to installation, recovery and desktop management. Exact features
and availability may change between builds; consult release notes and
[project status](docs/STATUS.md) for the scope of each candidate.

Please report reproducible issues with the WSX version, Windows and WSL versions,
GPU and driver, image tag, command and relevant error output. Remove passwords,
tokens and other personal information before sharing logs. During this rapid
alpha phase, **pull requests are temporarily not being accepted**; issue reports
and feedback are welcome.

## Licenses and binary terms

Original WGY-owned material in this repository is licensed under the
[Business Source License 1.1](LICENSE), with a noncommercial production-use
grant and an Apache-2.0 Change License after the applicable Change Date. BSL is
source-available and is not an OSI-approved open-source license. See
[licensing scope](LICENSING.md), [NOTICE](NOTICE) and
[third-party notices](THIRD-PARTY-NOTICES.md) for exceptions and retained rights.

The proprietary, precompiled WSX Host and Guest components have separate binary
terms. The community grant permits authorized recipients to use them free for
noncommercial purposes; it restricts redistribution, modification and reverse
engineering, subject to the terms and applicable law. The complete
`WSX-PRODUCT-TERMS.md` is provided with the Host and Guest materials in the
community package. Those terms do not remove or narrow applicable third-party
license rights. Kernel modules, their source materials, and other third-party
components retain their own licenses and notices.

## WSX Desk and Pro

[WSX Desk](https://wsxdesk.com) is the commercial Microsoft Store edition built
on the `wsld` engine, with a separately maintained proprietary Store backend.
Its planned Pro experience includes one-click installation and launching of
verified WSX desktops from the manager, together with Store licensing and
purchase integration. The Store edition and its Pro entitlement are subject to
separate terms; Store availability and entitlement behavior depend on the
actual published product and have not been established by a community build.

## Development

For source builds, architecture and the current acceptance scope, see
[development](docs/DEVELOPMENT.md), [edition builds](docs/EDITION-BUILDS.md)
and [project status](docs/STATUS.md). The Community and Store editions are
built separately. Building the source does not by itself establish component
license completeness, Windows desktop acceptance or release readiness.

Copyright © 2026 WGY ([wgy-wsx](https://github.com/wgy-wsx)); contact:
[wgy@wsxdesk.com](mailto:wgy@wsxdesk.com).
