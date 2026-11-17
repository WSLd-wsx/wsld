# Guest bootstrap and versioned adapters: v2 development contract

Status: **integrated candidate**, not a shipped protocol or desktop acceptance.
The old image implementation remains while the new path is being built; no
automatic v1 fallback is specified for an image declaring the v2 bootstrap.

## Responsibility

Core selects the graphics route and Host, resolves the distribution and owns
adapter distribution and trusted catalog policy. A small root Guest bootstrap
receives a bounded request via the local WSL channel and owns files only in its
adapter store. Adapter code executes within the Guest. Mesa/Venus and desktop
components remain owned and packaged by the rootfs. Native Host and renderer do
not interpret adapter code, and GPU retirement remains their responsibility.

## Implemented development slice

Protocol `wsx-guest-bootstrap`, major 2, uses one JSON request/reply with a
bounded ASCII request ID. Implemented operations are `capabilities`,
`adapter.stage`, `adapter.inspect`, `adapter.select`, `session.pin` and
`session.inspect`. There is no generic shell, path or code
execution operation. The distinct fixed dispatch entry supports session-pinned
adapter execution; its capability response does not prove desktop acceptance.

The schema-1 `wsx-guest-adapter` manifest declares protocol major 2, a reviewed
implementation (`hyprland` or `plasma`) and exactly six named files:
`protocol.py`, `desktop.py`, `display.py`, `retirement.py`, `device-view.py`
and `wsx-guest.py`.
Each file has a byte length and SHA-256. The current six-file set is an initial
staging contract; it has not yet been shown sufficient to run an actual desktop.
The current implementation limits manifests to 64 KiB, files to 2 MiB each and
the total payload to 8 MiB. Guest staging validates all bytes before committing a
new content-addressed directory. It leaves existing versions intact and serializes
concurrent staging. An internal selection and session-pin primitive has fixture
tests showing that a later selection does not alter an earlier session's pin.
The fixed dispatch entry executes `host-call`, `prepare` and `start` from the
verified session-pinned directory. Its adapter control wire protocol remains
`wsx-guest` v1; bootstrap v2 and adapter protocol versions are separate.
Session pins are published without overwriting existing pins and can be read
back only after the selected adapter's files have been reverified. A session
pin does not prove that its desktop or GPU resources have retired.
There is no pin release or garbage collection implementation yet. A digest proves
byte identity, not publisher authenticity.
The Core build can embed a catalog using `WSX_GUEST_ADAPTER_CATALOG`. Its
`schema_version: 1` and `adapters` entries bind an implementation to a manifest
SHA-256. The default catalog is empty and rejects every adapter. The catalog is
a product build input and must come from a trusted release process; embedding it
does not itself authenticate the build or its source. The admitted request path
checks this catalog before constructing the stage request. The Guest endpoint is
root-only and verifies integrity; it does not independently authenticate the
publisher. Selection and pinning do not themselves execute an adapter.

`adapter.inspect` reports file integrity and explicitly does not claim publisher
verification or active state. The current Core bundle verifier and protocol
helpers include a Windows-to-Guest transport and an admitted stage/select/pin
sequence. An image declaring `guest_bootstrap_major: 2` takes that route in
desktop startup and requires a Native runtime declaring `guest-bootstrap-v2`.
The bootstrap is staged by the image recipe. Existing published images do not
declare this new contract. No image or Host built from these changes has yet
passed a real desktop lifecycle acceptance.

At product build time, package each reviewed adapter with
`scripts/package-guest-adapter.py` and generate a catalog with
`scripts/create-guest-adapter-catalog.py`. Pass its path and the corresponding
bundle directories to `build.cmd` as `-GuestAdapterCatalog`,
`-HyprlandAdapter` and/or `-PlasmaAdapter`. The build embeds the catalog and
copies the matching files beside the executable under `guest-adapters/`. Pass
`-GuestAdapterNotices` with the reviewed rootfs `LICENSE-MAP.md`,
`docs/licensing/OUTPUT-LICENSES.md` and `docs/licensing/licenses/WSX-MIT.txt` under those base
names; the package copies them with the adapters. Verify any further upstream
notice and corresponding-source obligations for the exact bundle before release.
The catalog derives trust from the reviewed build inputs, not from the digest
alone. A build with no catalog admits no adapters.

## Required before product use

- Build reviewed adapter bundles from actual staged images using the rootfs
  export tool and retain their source provenance and per-component licenses.
- Supply a reviewed adapter catalog to a trusted product build; make selection
  resistant to untrusted replacements and interrupted writes.
- Restrict product selection and session pinning to the admitted path, then
  implement exact-session release. Running sessions retain their selected version;
  uncertain retirement retains files.
- Validate Windows transport, lifecycle, failure recovery and concurrency on a
  real system; do not make a later CLI control an exited process's memory state.
- Bind new adapter capabilities to actual Guest image and Native Host runtime
  admission without silently relabeling the old v1 manifest.
- Build a new image that explicitly declares the contract; actual Linux desktop,
  input, display, cancellation and GPU retirement need separate acceptance.
  Staging-only tests are not product or hardware acceptance.
