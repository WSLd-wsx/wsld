# Kernel release source review

Each newly built target includes `source-review-VERSION-RUN-ATTEMPT.json` and
its descriptor in `source-materials-VERSION-RUN-ATTEMPT.json`. The report binds
the module archive, module-source archive, kernel source archive, recorded
build inputs, build evidence and kernel configuration by SHA-256. The release
checksums cover both the report and the source-materials file.

`source_correspondence_verified` means that the delivered files match the
retained build receipt, the source archive's Git tree and the recorded static
module CRC evidence. It does not claim an independent reproducible build, runtime
acceptance, or prove provenance of Microsoft's kernel binary. The report
identifies this scope explicitly.

`corresponding_source_complete` applies to the whole delivery, not to either
individual source archive. It remains false unless a separate review for the
exact asset hashes is supplied. That review must address applicable licenses,
source and patch coverage, build and installation control materials, notices, and
whether recipients can access the corresponding source. An asset-specific JSON
review may be passed to `review-kernel-source-materials.py --review`; its fields
are `schema_version: 1`, `decision: "complete"`, `scope:
"corresponding-source-and-recipient-access"`, `reviewer`, `review_reference`,
and `binding`, exactly matching the generated report. This is an attributed
review record, not a cryptographic identity or automatic legal determination.
The optional `WSX_KERNEL_SOURCE_REVIEW` path passes such a review into the build
script when a suitable review is available. Missing review is not a release gate.

The `corresponding_source_complete` fields inside the intermediate build-input
and module-source descriptors retain their original meaning and remain false:
those standalone archives do not include the target kernel source. Consumers
should use the version-level field and report when present, and preserve the
legacy interpretation for older releases. Source access must remain available
for as long as required by the applicable licenses.
