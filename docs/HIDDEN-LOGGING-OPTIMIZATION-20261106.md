# Hidden desktop logging optimization candidate

> Historical experiment paths referenced below were moved to the prototype repository; see [Experiment migration](EXPERIMENT-MIGRATION.md).

The 180.5-second real hidden-view sample on Omarchy r13 found Native private
bytes 338,321,408 -> 338,305,024, with a 338,305,024–360,607,744 range;
the retained Native log grew by 15,483,100 bytes. GPU memory and individual
resource counts were not measured. See `artifacts/hidden-resource-20261106-01`.
This does not prove absence of leaks, and the log grew in bursts, not at a
constant rate.

The candidate filters only listed routine Renderer INFO records and the two
high-frequency Native success messages. Unknown INFO messages, warnings,
errors, nonzero results, rejection and lifecycle records are retained. Core
currently uses `WSX_PRESENT_FRAME: ... result=0` to admit the first frame; the
candidate retains the first such success per Native process. For explicit
per-frame diagnostics, set `WSX_GRAPHICS_VERBOSE=1` before launching the Host.
Changing it after startup does not change a running process.

`integration/graphics-close/logging-policy.py` applies the policy to a fresh,
verified source copy. `native-log-filter.rs` has a device-free regression test.
`quiet-graphics-pair-20261106-01` records the paired GNU graphics build and
`quiet-graphics-multi-20261106-01/bundle` combines the rebuilt Native Host
with the exact existing multi-owner runtime bytes. Source inventory comparison
with `desk-title-pair-20261106-01` found only the logging source changes and
relocated build paths. The resulting receipt is
`afd788d441a97952c02cf10f400aefbb03fd2bf1568db0cde9a6850b486752f2`;
its Host is `b8ff875feda466a308deb4963db49378e8800b0f4630a6935aa064a08d26f07f`.
The replacement edge is exact-baseline and exact-profile scoped, requires an
explicit selection, and is marked hardware acceptance pending. Existing
runtime selection and installed Guest files were not modified. Core reference
verification checks bytes, not runtime startup or kernel symbol compatibility.

At the initial candidate-build stage, the candidate had not yet run against a
real desktop. Do not claim lower
real-world log volume, GPU stability, safe retirement or first-frame success
until an owned test desktop is normally retired and restarted with this exact
candidate, with its peer kept running, followed by measured logs, view
reattachment, input and normal retirement acceptance. The candidate does not
change hidden rendering or address the observed white-screen delay.

## First real-machine candidate acceptance

The user authorized normal stop and one candidate restart. The prior Owner
retired with Guest, zero-mapping, PCI and process evidence while CachyOS stayed
running. Candidate `afd788d441a9` started Omarchy r13 and preserved the first
success marker. The user confirmed display/input, and a hidden/reopen cycle
retained the same Owner. Three-minute hidden log growth was 2,373,560 bytes;
this was not a controlled comparison with the prior workload. Working set and
handle count grew during the short sample, while private bytes were nearly
stable; GPU memory and long-run behavior remain unmeasured. See
`artifacts/quiet-graphics-acceptance-20261106-01/RESULT.md`.

A later source correction distinguishes `submit_failed=0` from actual failure.
Candidate `9d521ccbf1baae258cddccdcdcfd326bd717b082d3e6184848133eeab8809116`
was built from that correction and is explicitly scoped for the same baseline
and Guest profiles, but has not been run on hardware. The first candidate's
real-machine acceptance does not transfer to it. Neither is automatically
selected. No normal retirement of the new candidate was tested.
