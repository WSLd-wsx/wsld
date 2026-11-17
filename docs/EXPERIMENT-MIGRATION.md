# Historical experiment migration

The old standalone `graphics-close`, `graphics-staging` and `graphics-owner`
experiments were copied, with their source paths, hashes and applicable notices,
to `wsx-proto/prototypes/wsld-retired-experiments-20261113/`. They are historical
material and are not a product build or runtime dependency. The independent
`wsx-host` repository owns the current Native Host and renderer sources.

The graphics capability probe, Guest protocol, kernel module sources and
`graphics-lifetime` test inputs remain here because product builds or tests use
them. Legacy feature-gated Guest shadow diagnostics are still present; removing
those requires a separate change to their callers and feature tests.
