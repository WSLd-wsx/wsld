# Kernel module source evidence review: run 44042563283

On 2026-11-17, the assets of private Release
`wsx-kernel-44042563283-1` were downloaded and reviewed against Actions run
44042563283, built from wsld commit
`e0ae188ac5ac4da9bf2be5ba8264faf23da9b27a`.

Both per-version SHA-256 lists passed. The 202 entries in the recorded build
inputs matched their own hashes; the 201 repository files matched the exact
wsld commit, and the remaining entry matched the recorded runner dependency
data. The canonical build-generation hash was independently recalculated as
`d5e8885e164baf05702b844d5b6b20540ce77f45ff41451c9f8edc2bc470ece6`.
All 167 entries in the reusable module-source archive matched both their
declared hashes and those recorded build inputs.

For each version, the 13 retained build-evidence files matched their manifest,
the six individual module records matched the source and build manifests, and
the community evidence collector independently rehashed the module archives
and their members. The original build's static kernel-symbol CRC report records
`crc_match: true` for both versions.

| Kernel package | Module archive SHA-256 | Kernel source archive SHA-256 | Verified Git tree |
| --- | --- | --- | --- |
| 6.18.54.1-1 | `e05aa1a3a0fccea3d2ced4a822f8cb7101c1a8ebf2b6df1588f4ac91a7c1af3e` | `e76808d821dffdad285b77a2f9c5aa8d186bd272a1a25546bd0a8835f7a334dd` | `93d73e945e0cd96bd6f6919f06b2ede5641adb0a` |
| 6.18.58.1-1 | `d2b344b9e92504a5062d370e2faf34a33d6e735d9c627956804d9553a7cabc25` | `7e1d19b1ebc6271e060487739217c621febff1a2d956e4da8f179cf82fac4708` | `caee93906e5d20a06448f519fa72b6bcc0f01a98` |

The two downloaded source archives were independently rehashed as Git trees and
matched the tree identities recorded in the build. The recorded package build
revision and the source revision resolve to the same tree for each version.
This checks retained evidence; it does not independently prove the Microsoft
kernel package's publisher or the provenance of its binary.

The artifacts still record `source_correspondence_verified: false` and
`corresponding_source_complete: false`. This review does not change those
records, does not claim an independent reproducible build, and is not a final
legal conclusion about the complete corresponding-source obligations. A binary
need not have a bit-identical rebuilt hash merely to satisfy the GPL; assess
whether the actual required source and build/control materials are available.
Runtime module admission and desktop/GPU acceptance are also separate.

## Installation integration follow-up

The released module source and build-input archives do not include several WSX
integration scripts used to select, prepare or load modules, including
`core/guest/desktop-control.py` and `core/guest/module-reuse.py`. These are
separate WSX integration files; their presence or absence alone does not decide
which scripts an applicable license requires. Later module source exports include
these integration files and the repository license documents at the module-build
revision, bound to the build-input hashes. The scripts actually distributed with
an independently versioned WSX Core must also be considered at that Core's own
revision. This follow-up does not modify the existing release or change either
verification status.
