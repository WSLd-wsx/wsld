# Keyboard LED synchronization work

> Historical experiment paths referenced below were moved to the prototype repository; see [Experiment migration](EXPERIMENT-MIGRATION.md).

The current input protocol sends physical input from Windows to the Guest. It
does not negotiate or transmit Guest keyboard LED state. The existing Guest
uinput device does not advertise EV_LED. Do not send reverse-direction bytes to
the current Host: it rejects unexpected Guest input and disconnects the device.

Windows `SetKeyboardState` changes only the calling thread and cannot control
physical indicators. Microsoft's documented `SendInput` approach changes the
system toggle state and can affect Windows applications. The input hook also
currently intercepts keyboard events, so synthetic synchronization must be
distinguished from physical events and must not be forwarded to the Guest.

`integration/graphics-close/keyboard-led-policy.h` is an isolated policy
experiment, not a product implementation. It requires a serialized focus
owner, mutation-boundary foreground checks and a reliable observation of
Windows state. Independent input Host processes cannot safely apply it without
cross-process arbitration. A focus loss may race another window's acquisition,
an external toggle, a process crash or a desktop switch. In those cases blindly
restoring a saved state can corrupt Windows or a peer desktop state. This policy
must not be wired into live Hosts until these cases have a tested resolution.

Remaining work: a negotiated versioned reverse input channel, Guest LED
reporting, a single cross-process Windows focus owner, tagged synthetic input,
failure and crash recovery, and real two-distro and Windows focus-switch tests.
Neither existing release has this capability. No Guest or Host was replaced by
this experiment. The Windows MSVC policy unit test exercises ordinary local
transitions only; it does not establish system-wide or hardware behavior.
