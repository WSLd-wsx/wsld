# Explicit candidate testing

For combined public download and import, use `--install --pre-release EXACT_TAG`;
see [release installation](INSTALL.md). The commands below remain granular
diagnostics sharing the same verified cache and import engine.

`wsx candidate show wsx-omarchy-4.0.6-r10` queries the public GitHub release
API without credentials. `wsx candidate fetch wsx-omarchy-4.0.6-r10` downloads
the image and metadata using public release download URLs, checks
the completion record, expected asset set, downloaded sizes and SHA-256 digests, and commits
the verified cache under `%LOCALAPPDATA%\Temp\WSX\releases`. Existing complete
caches in `%LOCALAPPDATA%\WSX\cache\releases` can still be verified and reused.
Drafts and stable
releases are rejected by these explicitly candidate-only commands. Private
repositories can optionally supply a token on standard input with `--token-stdin`.
The token is not placed in command arguments or sent to redirected asset hosts.
To use a local HTTP proxy for this process, set `WSX_HTTP_PROXY` to
`127.0.0.1:PORT`. This does not change system proxy settings. Without it, WinHTTP
uses its automatic proxy configuration. SOCKS proxy URLs are not supported.

`wsx candidate unpack TAG --max-tar-bytes N` verifies the cache again and streams
zstd to a tar file under `%LOCALAPPDATA%\Temp\WSX\tar`. N is an explicit output ceiling and
space reservation, not the manifest's unpacked file size. A failed download or
decode retains its partial output for diagnosis. Downloads retry interrupted
transfers using checked HTTP byte ranges. A later invocation can resume the same
transaction only if its completion marker matches; previously completed assets
are rehashed, and the retained partial is rehashed before appending. Cancellation
and automatic cleanup of failed transactions are not implemented.
For a completed cache, fetch compares the current completion marker and verifies
the image and metadata before reuse. Source archives are not downloaded or verified;
their metadata is checked for consistency only. A mismatch is reported and the cache is retained.

These commands do not establish publisher signatures or admit installation. The
component source index for schema 2 still needs full semantic and archive inventory
validation. Stable compatibility admission is described in INSTALL.md. The graphics runtime
closure, account initialization, desktop startup and release acceptance remain
separate from successful acquisition.

`candidate import-test TAG --max-tar-bytes N` is an explicit experimental import,
not stable installation admission. It verifies the candidate image again, creates
a new ownership record and independent disk directory, decodes a fresh tar with
the supplied bound, then calls the Windows system WSL import with version 2.
Failure retains the ownership record, disk and diagnostics; temporary tar files
may be removed by Windows. It never unregisters or deletes the distribution or
disk. Guest initialization, desktop operation, hardware compatibility,
remain separate. Import now verifies the WSL 2 registration and actual base path
and records its GUID; the legacy stored phase name is retained for compatibility.

`wsld timezone` reads the Windows timezone identifier and maps it to the CLDR 47
global IANA zone. The table and Unicode license are in `core/data`. The global
mapping defaults to `America/New_York` for an unknown Windows identifier. It is
a default rather than a precise location: one Windows timezone can
cover several IANA zones with differing historical rules. Initialization should
allow explicit selection and verify that the Guest has the selected zone file.
