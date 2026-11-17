# Native runtime contract migration

The graphics Host is developed in the separate `wsx-host` repository. The
versioned `wsx-native-runtime` declaration describes one independently built
zero-copy or staging-copy runtime, its file hashes and required Guest contract.
It does not itself establish origin, successful GPU execution, kernel module
compatibility, runtime admission or retirement.

`runtime_bundle::verify` accepts the declaration bytes and an independently
trusted SHA-256 digest, validates the supported ABI and observed Guest
capabilities, and verifies every listed component. A digest stored beside a
writable bundle is not an independent trust anchor. Product packaging or an
authenticated update mechanism must supply the trusted digest. The verifier
alone does not close file replacement races; copying and rechecking components
inside the protected lifetime directory remains necessary.

This is an initial verifier and is not yet connected to desktop startup. Current
startup still uses historical receipts and the staging route remains behind
diagnostic features. A complete transition requires trusted manifest delivery,
route selection under the existing machine gate, independently selected kernel
modules, existing release compatibility, safe lifetime staging, status and
failure handling, followed by real zero-copy and staging desktop acceptance.
Do not remove the old receipt checks or claim automatic staging fallback until
those pieces have been implemented and verified.

For a product build, `WSX_RUNTIME_CATALOG` can specify a reviewed catalog as a
Core build input; it is embedded into the executable, rather than read from the
runtime directory. Without this build input the embedded catalog is empty and
admits no new runtime. Catalog creation and integrity verification do not replace
component provenance review, acceptance, or safe device and module admission.
The embedded-catalog verifier is not yet called by desktop startup.
