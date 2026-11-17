# Candidate private core boundary and license review

> Historical experiment paths referenced below were moved to the prototype repository; see [Experiment migration](EXPERIMENT-MIGRATION.md).

Review date: 2026-11-09. This is a static source and evidence review, not a
license change, a completed authorship audit, or approval for distribution. It
records a proposed protection boundary without moving files or changing builds.
Existing grants, notices, independent repository responsibilities, and current
runtime admission rules remain in force.

## Material GPL finding, 2026-11-09 follow-up

The selected packaged Native Host has a GPL-2.0-or-later peer and upstream codec
in its actual build dependency path. This supersedes any earlier suggestion in
this document that the selected Native binary is provisionally clear for
proprietary-only distribution. MIT baseline findings remain true, but do not
resolve the separate GPL dependency. See the trace at the end of this review.

## Decision boundary

Protect two connected areas, subject to the file-level qualifications below:

1. Core desktop coordination: exact registration and ownership resolution,
   graphics component admission, per-lifetime startup and observation, stop,
   recovery, and failure retention.
2. Native graphics ownership: host/renderer resource coordination, mapping and
   reclaim, queue and GPU completion, exact-device ownership and retirement.

A private repository is a source-access choice, not a license or a distribution
exemption. MIT-licensed code may be included in proprietary binaries with its
notice, but previous recipients retain their MIT rights. An MIT-marked file in a
private repository is not proof that a grant reached anyone outside the rights
holder; check actual distribution and rights before making a claim about prior
recipients. The owner may separately license only material for which the owner
has the necessary rights. Do not remove upstream notices or assume that source
identity proves authorship.

## Core coordination candidates

| Scope | Candidate files | Current evidence and disposition |
| --- | --- | --- |
| Desktop lifetime, startup, stop, recovery and identity | `core/src/desktop.rs`, `desktop_distro.rs`, `desktop_distro_windows.rs`, `desktop_windows.rs`, `platform/registration.rs`, `platform/desktop_lock.rs` | Original Core is covered by Business Source License 1.1 under `core/LICENSE`, to the extent of WGY's rights. Candidate for private development and a separate proprietary grant after provenance review. Do not remove identity, ownership, or recovery checks during extraction. |
| Graphics admission, selected runtime profile, device matching | `core/src/graphics.rs`, `graphics_diagnostics.rs`, `compatibility.rs`, `core/src/platform/display.rs`, `core/compatibility/` | Same Core scope; inspect embedded and reference inputs separately. `direct_profile` selects the verified 64/32 reclaim profile; retain it together with the pinned component closure and evidence. |
| Runtime observation and Guest adapters | `core/guest/desktop-control.py`, `desktop-observe.py`, `drm-access.py`, `module-reuse.py`, `kernel-exports.py`, `kernel-symbol-audit.py`, plus `core/src/platform/desktop.ps1` and `desktop-discovery.ps1` | Candidate operational inputs, not automatically proprietary: `docs/LICENSE-AUDIT.md` identifies a prototype-derived DRM adapter and does not establish complete authorship for the Guest scripts. Trace the exact source and grants. Python and PowerShell embedded in the PE are recoverable by recipients. |
| Persistence and contract dependencies | `core/src/store.rs`, `guest_protocol.rs` and relevant platform helpers | Needed to preserve operational behavior, but private treatment is a product choice rather than necessary to protect the graphics technique. Keep Guest protocol public and versioned; do not make Core invoke Guest Python internals. |

These are initial file boundaries, not a claim that every function needs to be
private. Preserve independent builds and do not move code until a dependency
and build plan is approved. The current public-build independence requirement
would need an explicit architecture decision if a complete build requires a
private Core source checkout.

## Native graphics baseline and restrictions

The verified reference profile and its bounded evidence are described by
`docs/PROTOTYPE-MIGRATION-CHECKLIST.md`. The Native baseline is not a single
self-contained source file. The close-pair build records 358 Native inputs,
renderer/Rutabaga inventories, a paired patch and a 12-file bundle. The recorded
`artifacts/graphics-close-pair-20261105-01/final-source-inventory.json` is a
hash inventory, not a license inventory. VM-exit and later pairs have separate
receipts; determine the exact selected product closure before distribution.

| Area | Identified source / evidence | License finding and treatment |
| --- | --- | --- |
| Native entry, mapping, reclaim and session | `wsx-proto/prototypes/venus-rutabaga/device-host/hdv-adapter/memory-session/{native_main.rs,lib.rs,owner.rs,retirement.rs}` and `guest-memory/ram.rs` | These prototype files explicitly carry MIT SPDX identifiers; both corresponding Cargo manifests declare MIT. The migration checklist identifies the reference `lib.rs` and `ram.rs` as byte-matching the diagnostic source inventory. Retain MIT notices. Do not describe these baseline files as exclusively proprietary or erase any prior grants. A separately authored proprietary addition needs its own verified rights and boundary. The build's actual patched inputs must be matched to the final receipt before claiming exact byte identity. |
| Native close and retirement adaptations | `integration/graphics-close/`, `integration/graphics-lifetime/` | Project helpers and patch contributions are MIT under `integration/LICENSE`; `guest_retirement.rs` explicitly identifies MIT. Existing upstream notices remain effective. Real close/VM-exit evidence is bounded and does not prove every failure or retirement scenario. |
| Guest PCI Owner ABI 2 | `integration/graphics-owner/owner.c` | Explicit GPL-2.0-or-later. Retain its source, notices and applicable distribution obligations. It is not a proprietary component merely because a builder or repository is private. Combination and corresponding-source scope require actual distribution analysis. |
| Renderer, Rutabaga, OpenVMM-derived material and dependencies | Source inventories and bundle receipts for the selected pair | `LICENSING.md` records top-level MIT observations for OpenVMM and virglrenderer and BSD-3-Clause for Rutabaga, but expressly does not establish a per-file or transitive audit. Track exact revisions, changes, notices, build inputs and licenses. No blanket proprietary classification or complete redistribution clearance. |
| Experimental staging transfer | `integration/graphics-staging/` | MIT SPDX-marked files retain MIT; other files require their own review. This path is disabled and lacks complete component/lifecycle and real desktop acceptance. Do not substitute experiments for the admitted opaque-v1 pair. |
| Rootfs Guest side | `wsx-rootfs/guest/`, `vendor/` and packaged components | Apply `wsx-rootfs/LICENSE-MAP.md`: Guest default is PolyForm for owned expression, with specific MIT exceptions; vendor patches and KWin additions have their own MIT/GPL treatment. Rootfs owns the Guest interface and userspace. Publicly distributed code and prior grants cannot be made secret by moving subsequent work. |

## Evidence limits and next steps

- Historical acceptance includes the documented 64/32 reference trial and
  separate product runs; it is not present-day multi-distro, sustained-load or
  complete abnormal-retirement acceptance.
- Match the actual release-selected Native and renderer hashes to their exact
  final source inventories and patches. Inventory third-party and toolchain
  notices and verify authorship of proprietary candidate additions. Existing
  Core Cargo notices do not cover the external graphics closure.
- Identify what was actually distributed under MIT, PolyForm or other terms;
  private repository visibility alone does not establish prior distribution.
- If BSL 1.1 is adopted for selected public code, choose its parameters and
  document exact scope separately. It does not supersede prior grants or change
  the terms of third-party components. No BSL choice or license text is made here.
- Obtain legal review of the actual binary/source distribution and any mixed
  GPL work before release. This review does not establish legal compatibility.

## Selected packaged pair: hash-to-source trace, 2026-11-09

Scope is the concrete `packaged-graphics-20261108-03` local package, not a claim
about a currently running process or a universal default. Runtime selection is
per release contract and optional explicit update; other admitted receipts and
legacy pairings exist. The packaged `verified-files.json` hashes to
`442f6032d8645c413b7398e2654c669169e79e9b46e79220134bb881b51d0561`.
The packaged binaries were independently rehashed and compared with the
`desk-title-pair-20261106-01/package` build outputs:

| Component | SHA-256, both copies | Source evidence |
| --- | --- | --- |
| `wsx-native-host.exe` | `62c86d5316651d86c5c8e3d96245c29049cae5b9c87ba40e6b3e1b057efa0246` | `desk-title-pair-20261106-01/build-result.json`, `source-inventory.json` and `paired-close.patch` |
| `libvirglrenderer-1.dll` | `a9b91b562ae400b40bb4b1f5c82b5ee5b67a927b7ca9a1fb3da6dcf00a1f7b9e` | Same paired build records |

The pair build's own bundle receipt is `c9ebacee38a1c3fe56018e7bb3e211a2bd53ebe71a61e64bfd7c1ca80a7d6367`;
`desk-title-multi-20261106-01/result.json` records composition of the selected
`442f...` receipt from the previous `ec504...` multi-owner bundle, replacing
Native and renderer. These are different bundle receipts, not conflicting binary
hashes. The title pair build reports successful commands and `gpu_executed=false`.
Separate RTX dual-desktop observations used recorded Guest test overlays; they
are not clean-image or general acceptance.

The paired build inventory contains 4,018 entries; its SHA-256 is
`1ca65b73d35ff6020523301aabaa3ac0418371679acf95b2e5ad8317d233e30d`.
The retained patch SHA-256 is
`9953708e45a2e38d7e51ff27a2aa94ef282cd9ec4fd01873dc5773c797491bb2`.
For this review, all 16 distinct files named in that patch were copied from the
recorded `ram-threshold-native-build-01` and `ring-timer-port-01/virglrenderer-1.3.0`
reference trees into temporary scratch, patched in order, and hashed. All 16
matched the paired build's inventory. This establishes the recorded patch-to-file
link for those files; it is not a bit-reproducible rebuild, an independent audit
of every inventory entry, or proof that the inventory alone contains the complete
build closure. Copied headers, libraries, Cargo dependencies, toolchain and
reference-source changes are not all represented by the patch. The temporary
original build directory is no longer present.

In particular, reference `native/lib.rs` and `native/guest-memory/ram.rs` match
the selected pair inventory exactly and carry MIT SPDX identifiers. Patched
`native/native_main.rs` and `native/owner.rs` also reconstruct to the inventory;
their baseline files carry MIT identifiers. Patch content includes view hide and
restore, present recovery, protocol/Guest retirement, exact-owner coordination,
VM exit and memory handling. The recorded integration scope grants the project's
`graphics-close` and `graphics-lifetime` helpers and patch contributions under
MIT. The present-recovery source itself carries an MIT SPDX identifier. These
identified contributions should therefore be treated as MIT, not categorized as
exclusive proprietary additions. Check who actually received copies before
asserting that any particular historical grant was exercised.

### Proprietary authorization assessment

- **Core desktop coordination:** candidate for a separate proprietary license
  to the extent WGY owns the expression, subject to contribution and embedded
  script provenance. Its current PolyForm grant and any rights already granted
  remain effective. `core/src/desktop_windows.rs`,
  `desktop_distro_windows.rs`, `graphics.rs`, and `compatibility.rs` are useful
  starting boundaries; they also contain ordinary/public-contract code, so this
  is not a claim that every line must be private.
- **Specific selected Native/renderer modifications in the retained patch:**
  recorded as MIT integration contributions, with MIT baseline and upstream
  notices where applicable. No exclusive proprietary classification is
  supported by this evidence. Private storage or proprietary binary packaging
  does not cancel the recorded grant.
- **Other Native/renderer additions:** not cleared for proprietary-exclusive
  treatment. Identify exact new expression, authorship, source history, prior
  grants and upstream obligations first. New independently owned additions may
  be separately licensed without removing required notices or existing grants;
  whether an addition is separable and how the combined distribution is licensed
  require review of its actual implementation.
- **GPL PCI Owner and Guest/upstream modifications:** preserve applicable GPL,
  MIT and other terms. They are outside an exclusive proprietary claim without
  an additional valid rights basis and any applicable distribution compliance.

Two quiet logging candidates are separate explicitly selected updates, not the
packaged pair: `afd788...` has bounded real-machine observations and
`9d521c...` has not run on hardware according to
`docs/HIDDEN-LOGGING-OPTIMIZATION-20261106.md`; neither is automatically selected.
Do not transfer the packaged pair's observations or this patch replay to them.


## Native Host GPL build-path finding

For the selected `62c86d...` Windows Native Host, the recorded
`desk-title-pair-20261106-01/native-link.log` shows compilation of
`wsx-virtio-msg-peer`. Its exact `build.rs`, `Cargo.toml`, `bus.c`, `platform.h`,
`upstream/virtio-msg-prot.h`, and `src/lib.rs` were matched by SHA-256 to the
selected pair's `source-inventory.json`.

- The peer `Cargo.toml` declares `GPL-2.0-or-later`; `bus.c` and `platform.h`
  carry that SPDX identifier.
- `build.rs` preprocesses `platform.h`, generates C wrappers for the upstream
  codec, compiles `bus.c` and generated wrappers, archives them as `libvmsg_bus.a`,
  and requests static linkage. The Rust peer calls `wsx_vmsg_bus_reply` and
  generated codec functions. This is a production build path, not merely a GPL
  test file present in a source inventory.
- `upstream/virtio-msg-prot.h` identifies Advanced Micro Devices copyright and
  GPL-2.0-or-later. `sources.lock.json` records its QEMU source revision and
  modifications. A WGY-only relicensing decision cannot override upstream rights.
- The retained executable's ordinary symbol-table search did not locate the
  C symbols; linker-level map or disassembly was not available for this review.
  The pinned inputs, production Cargo build log and build script provide strong
  evidence of the GPL static dependency, but exact surviving object functions
  have not been independently proven from the final executable.

Conclusion: treat proprietary-only distribution of this Native executable as
blocked pending qualified license review and resolution. MIT permits proprietary
use subject to notice, but it does not negate GPL conditions on a combined work.
Options for investigation are: distribute the affected combined work under an
appropriate GPL license with corresponding-source compliance; obtain all needed
rights from relevant copyright holders; or replace the GPL peer/codec with a
properly rights-cleared independently developed implementation and revalidate the
full product path. Do not claim an ABI, DLL, process boundary or separate
repository alone eliminates the issue. A replacement changes the pinned closure:
new hashes, compatibility checks, source inventories and real desktop/lifecycle
acceptance are needed; retain the verified baseline.

The separate Guest PCI Owner is also GPL, but this Windows Host build-path issue
is independent of whether the Guest module is distributed separately. Renderer
COPYING identifies MIT and Rutabaga declares BSD-3-Clause; these top-level
observations and the existing MIT integration notices do not complete an audit
of all renderer, build, Cargo, toolchain and redistribution dependencies.
This is a technical and license-evidence finding, not a legal opinion.
