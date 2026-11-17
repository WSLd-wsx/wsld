# Historical reference Guest launcher

The `442f6032d8645c413b7398e2654c669169e79e9b46e79220134bb881b51d0561`
reference receipt remains unchanged because released Guest compatibility
metadata identifies its exact bytes. Its `guest/start-guest.py` entry describes
an unused historical prototype launcher. It is not the Guest v2 adapter entry
and is no longer included in new product packages or copied to new runtime
directories. The previously distributed bytes retain their original license.

Core may accept that one file as absent only for this exact pinned receipt. If
the file is present, Core continues to verify it. Every other receipt entry,
Native component and kernel module retain their existing checks. Display and
Vulkan identity queries are compiled into the product executable.
The verifier reports `unused_guest_script_omitted` when this exception applies.
Other receipts cannot use this exception without a separate source and runtime
review; packaging rejects an unreviewed launcher instead of silently shipping it.

This change is not hardware, desktop, restart or retirement acceptance. An
existing runtime cache is not rewritten or deleted by this change.
