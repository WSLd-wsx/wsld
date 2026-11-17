# Native Host GPL corresponding-source scope

> Historical experiment paths referenced below were moved to the prototype repository; see [Experiment migration](EXPERIMENT-MIGRATION.md).

Review date: 2026-11-09. This is a technical source-scope assessment for a
specific local build, not a complete source offer, license clearance, legal
opinion, or authorization to publish. Preserve existing grants and notices.

## Identified binary and boundary

The local `packaged-graphics-20261108-03` bundle receipt is
`442f6032d8645c413b7398e2654c669169e79e9b46e79220134bb881b51d0561`.
Its `wsx-native-host.exe` is SHA-256
`62c86d5316651d86c5c8e3d96245c29049cae5b9c87ba40e6b3e1b057efa0246`.
The corresponding `desk-title-pair-20261106-01` build log compiles the
GPL-2.0-or-later `wsx-virtio-msg-peer`. Its build.rs statically archives
`bus.c` and wrappers generated from the GPL QEMU codec header and directs Cargo
to link the archive. Exact input hashes match the pair's source inventory.
The ordinary PE symbol table did not independently confirm retained functions;
the final linker map is unavailable. Treat the whole Host executable as the
conservative GPL combined-work candidate pending professional review. Supplying
only the peer's source is not a defensible complete-source plan.

GPL-2.0-or-later permits assessing GPL version 2 where all incorporated works
allow it; actual compatible licensing of every included component is still an
unresolved prerequisite. GPLv2 section 3 describes complete corresponding
source for an executable as the source for its modules, interface definitions,
and scripts controlling compilation and installation, subject to its stated
system-component exception. The correct delivery method depends on the chosen
GPL version and binary-distribution method; do not assume an arbitrary web link
or written offer is sufficient in every case.

## Minimum technical source inventory to prepare

The following is a conservative preparation inventory, not a final legal ruling
that every file in a broad tree is necessarily part of corresponding source.

1. **Native Host source and build:** all source actually used to build the exact
   executable, including Native Rust/C source, headers, IDL, Cargo.toml and
   Cargo.lock, build.rs, cross-target configuration and compile/link scripts;
   include the `desk-title-pair-20261106-01/paired-close.patch` and every added
   helper used in that build. The patch alone is not complete source.
2. **GPL peer and codec:** `native/standard-device/peer` production Rust sources,
   `Cargo.toml`, `build.rs`, `bus.c`, `platform.h`, patched upstream
   `virtio-msg-prot.h` and required headers; record upstream revision,
   `sources.lock.json`, the two codec patches and the actual patched header.
   Include the AMD copyright and applicable GPL text and notices. Generated
   bindings and wrappers must be either supplied or reproducibly regenerated
   from included inputs and generator instructions.
3. **Linked source closure:** exact used versions of local Native crates and
   relevant OpenVMM and Rutabaga sources, plus applicable Cargo registry packages,
   generated interface files and associated notices. The root lockfile lists 253
   packages (212 registry entries); a lockfile count is not a list of code linked
   into the executable. Resolve target/features/build dependencies and preserve
   package sources and checksums. Determine whether toolchain/system components
   qualify for any exception rather than automatically omitting them.
4. **Renderer and other dependencies:** the actual PE import table names
   `libvirglrenderer-1.dll`. Its source, modifications, build instructions and
   notices are a necessary candidate for review because the Host and renderer
   are a paired, directly linked runtime. Whether this DLL and any further
   components fall inside the legally required corresponding-source/combined-work
   scope depends on the actual GPL version, linking and distribution analysis;
   a DLL boundary alone is not decisive. Renderer imports `libepoxy-0.dll`;
   identify its exact source, license and applicable conditions separately.
5. **Install and interface material:** preserve the exact configuration,
   interface definitions and scripts necessary to build and install the combined
   work, including the selected 64/32 runtime profile and verified pair recipe.
   Separate evidence receipts from sources; hashes are not replacement source.
6. **Guest kernel components:** the selected bundle also distributes six `.ko`
   files. `wsx_virtio_msg_pci.ko` hash
   `577bbfbeb0dc1c4f2e213f4b6045621c52fb1df728b3ba602b0bdacebb2c4918`
   matches `multi-owner-kernel-20261106-01/result.json`, which records the
   owner, parent and other inputs and its build command. Its GPL source and
   corresponding build materials require an independent distribution review.
   Audit the other five modules individually. Their source obligations do not
   automatically establish that every other product component is the same work.

## Recoverability checked

The selected source inventory has SHA-256
`1ca65b73d35ff6020523301aabaa3ac0418371679acf95b2e5ad8317d233e30d`
and 4,018 entries. A read-only byte comparison against the current pinned
reference inputs found:

| Inventory group | Matching reference | Changed from reference | Absent from reference |
| --- | ---: | ---: | ---: |
| Native | 332 | 26 | 5 |
| OpenVMM | 3,083 | 0 | 0 |
| Rutabaga | 128 | 0 | 0 |
| Renderer source | 437 | 5 | 2 |

These categories add to 4,018; counts include build/test/reference material and
are not a proof that every entry is linked. All five absent Native files match
the current `integration/graphics-lifetime` copies; both absent renderer headers
match `integration/graphics-close`. All 16 distinct files addressed by the
retained paired patch were independently reconstructed in scratch and matched
the inventory. The other differences include relocated paths and build wrappers;
the original scratch directory is missing. It remains necessary to reconstruct
and hash-check *all* actual build inputs, especially the changed manifests and
wrappers, generated codec, registry packages, libraries and generated files,
then perform a clean rebuild or otherwise establish correspondence. Source
inventory excludes archives and does not inventory the complete toolchain or
Cargo registry. Do not represent the current record as a ready-to-distribute
complete corresponding source package.

## Distribution scope and unresolved issues

- Conservative current assumption: the complete linked Native Host executable,
  not just the GPL peer, is subject to the applicable combined-work obligations.
  Confirm exact linked contents, upstream rights and version compatibility.
- A separate private repo is an access control decision. Private use is distinct
  from external binary distribution. Source recipients' GPL rights cannot be
  restricted by a proprietary notice on the combined work.
- Core, Pro, input host, renderer and Guest modules must be analyzed by their
  actual technical and legal relationships. Core invokes a separate Host
  process; this alone does not establish whether it is a separate work. The Host
  directly imports the paired renderer; assess that relationship explicitly.
  Do not declare all components GPL, or automatically exempt them, solely from
  a process, binary, DLL or repository boundary.
- Before any external distribution: recover exact source closure and build
  scripts; audit per-file licenses, source availability and distribution method;
  reconcile the current MIT/PolyForm/BSL and third-party grants; determine any
  Microsoft/toolchain redistribution terms; and obtain qualified legal review.
  No publication, license change or distribution is authorized by this review.
