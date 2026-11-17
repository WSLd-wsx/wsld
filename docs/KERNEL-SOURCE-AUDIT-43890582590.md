# Kernel source audit for run 43890582590

This records a read-only reconstruction of the two kernel module sets produced
by `WSLd-wsx/wsld` Actions run 43890582590 at commit
`fd3310d26a2d2883d7714a253f634630a6954553`. The accompanying
[KERNEL-SOURCE-AUDIT-43890582590.json](KERNEL-SOURCE-AUDIT-43890582590.json)
contains exact per-module, archive, configuration and source hashes.

The original release assets and Actions job log were retrieved independently.
Both published checksum lists passed. The six files in each module archive were
hashed and their canonical group digests matched the diagnostic results in the
original run log. The reusable module-source archive's 163 WSX source and script
files matched that exact Git commit; its recorded runner dependencies plus the
commit reproduced the `fd1f036e...` build generation printed in the run log.
The complete historical build-input archive was reconstructed from that commit
and the retained runner dependency record. Its SHA-256
`4d04ba76a8e9e4b17ac58fa939b79b7f4c1a4c01f57b80b84f2d1d84a80cc8b6`
exactly matches the archive hash printed by the original run before that
temporary archive was deleted. The reconstructed archive is staged with the
local evidence; the original release itself has not been changed.

Microsoft's package metadata names a different commit from the `linux-msft-wsl`
tag used by the builder. Direct Git object API checks show that both commits in
each pair have the same Git tree:

| Kernel package | Package build revision | Build source commit | Shared Git tree |
| --- | --- | --- | --- |
| 6.18.54.1-1 | `26d1ceb550876e1bcbd7a1d0369d948bea246ca3` | `d504d40ab83839cb61942a812a99d183c42227b9` | `93d73e945e0cd96bd6f6919f06b2ede5641adb0a` |
| 6.18.58.1-1 | `75fe784cf9cf5fdd21dccfb4f397339c961c98aa` | `a989171929bed4ea681be8d77333845ce1c1732a` | `caee93906e5d20a06448f519fa72b6bcc0f01a98` |

The exact kernel packages were retrieved again, and their kernel image hashes and
build IDs matched the original inspection records. Their symbol tables were
independently reconstructed and the static module CRC comparison was repeated;
both module sets passed. The reconstructed symbol tables, inspection records,
and comparison results are included in the local evidence. These observations connect the published
archive identities to the original diagnostic run and identify the matching
upstream source trees. Complete upstream source snapshots were downloaded for
both source commits. Their contents were independently rehashed as Git blobs and
trees, and each computed tree ID matched the table above. The snapshot archive
SHA-256 values and file counts are recorded in the accompanying JSON. These
snapshots and the original release evidence are staged locally; their availability
here does not mean they have been uploaded or distributed to recipients.

The observations do **not** prove that the official Microsoft kernel
binary is reproducible from the declared revision, or recover the original
per-module build result and CRC reports: the original publishing script deleted
those temporary evidence archives. The historical source-completeness and
binary-source-correspondence flags therefore remain false. No release was
modified or retroactively reclassified by this audit.

For subsequent builds, the pipeline captures the clean pinned upstream Git tree
before preparation, verifies it against the package's recorded build revision,
and retains the full WSX build inputs, source snapshot, module hashes, build
result, original and prepared configuration, symbol table and CRC evidence.
Matching package-reported metadata and tree IDs is still not publisher signature
verification, independent reproducibility, a license review or runtime admission.
Those separate statuses must not be promoted merely because this evidence exists.
