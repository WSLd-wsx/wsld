# Desktop research for the WSX Guest protocol

Research date: 2026-11-05. Scope: upstream documentation and interface/source
inspection, not installation or hardware acceptance of every desktop. The user
explicitly does not require a full desktop test matrix before the initial Alpha.
Only the existing Omarchy/Hyprland and CachyOS/Plasma product paths have bounded
WSX machine evidence. Other rows below are design coverage, not supported targets.

## Findings that constrain the public protocol

A distribution, desktop shell, compositor and graphics transport are different
identities. A distro may offer several sessions; a desktop may use different
compositors. None of those names should select Windows business logic. Future
popularity is uncertain: COSMIC, Hyprland and Niri are useful architecture probes,
not proven future market leaders. No reliable cross-distribution usage census was
established in this investigation; this is not a numerical popularity ranking.

| Family | Window-system/implementation evidence | Display and DPI differences | Session, input and permissions consequences |
| --- | --- | --- | --- |
| GNOME | GNOME Shell/Mutter; GNOME 49 disabled the dedicated X11 session by default, with Xwayland applications separate. | Mutter exposes its own DisplayConfig D-Bus API, configuration serials and logical monitor configuration. Core must not equate buffer pixels with logical desktop coordinates. | gnome-session increasingly relies on the systemd user manager; session ownership, user bus and shutdown coordination belong to Guest. Systemd service state alone is not complete compositor/device retirement. |
| KDE Plasma | KWin and the Plasma session; X11 versus Wayland availability must be detected for the shipped release. | KScreen/KWin control differs from Mutter. The current WSX KScreen adapter already demonstrated that logical output size and physical mode size differ under scaling. | startplasma performs environment/user-bus/session work. Guest must preserve normal logout and input/seat ordering; Core must not stop arbitrary Plasma units or assume a single process owns the desktop. |
| Xfce | Xfce 4.20 documents experimental Wayland support with external compositors such as labwc/Wayfire while retaining X11 support. | X11 display configuration and Wayland output-management backends differ. Input settings may belong to the compositor rather than Xfce settings. | startxfce4 and xfce4-session own the desktop session; logout supports a distinct normal-logout operation. Neither the Xfce label nor X11 availability proves a compatible WSX graphics carrier. |
| Cinnamon | Cinnamon/Muffin, with both X11 and Wayland transitions to account for. The July 2026 Mint update describes non-experimental Wayland support planned for the next release, not proof of availability in every current image. | Upstream work explicitly addresses HiDPI, multiple monitors and Xwayland clients. Detect the actual session and capabilities rather than applying old X11 assumptions to all Cinnamon images. | cinnamon-session exposes session-management and inhibitor interfaces. Guest must honor normal logout semantics and permissions; a timeout is not permission for Core to kill the compositor. |
| COSMIC | A separate Wayland desktop; cosmic-comp uses Rust/Smithay and has its own session/settings components. Epoch 1 shipped in December 2025. | cosmic-randr configures Wayland outputs; scaling includes explicit handling of Xwayland applications. Sharing a library or programming language does not make its control API identical to another compositor. | Current source enables libinput/libseat, optional logind/systemd and libei backends. These are Guest implementation choices; advertise actual packaged/working capabilities rather than compile-time possibilities. |
| Hyprland | A Wayland compositor with an independently evolving ecosystem; Aquamarine handles low-level DRM/KMS backends and is not a generic replacement for every compositor library. | hyprctl/monitor configuration has its own modes, scale, transforms and layout semantics. Core must not send configuration snippets or assume that syntax remains stable across Guest upgrades. | A compositor is not by itself a complete desktop session. Guest owns shell/autostart, user environment, input bridge and cleanup. Core binds an opaque session identity, not a guessed Hyprland PID or socket. |

Primary references for the six rows:

- [GNOME 49 distribution changes](https://release.gnome.org/49/developers/),
  [gnome-session documentation](https://gnome.pages.gitlab.gnome.org/gnome-session/re01.html),
  [Mutter DisplayConfig interface](https://raw.githubusercontent.com/GNOME/mutter/main/data/dbus-interfaces/org.gnome.Mutter.DisplayConfig.xml).
- [KDE startplasma source](https://raw.githubusercontent.com/KDE/plasma-workspace/master/startkde/startplasma.cpp),
  [KScreen serialization](https://raw.githubusercontent.com/KDE/libkscreen/master/src/configserializer.cpp),
  [KScreen control implementation](https://raw.githubusercontent.com/KDE/libkscreen/master/src/doctor/doctor.cpp).
- [Xfce 4.20 tour](https://www.xfce.org/about/tour420/),
  [upstream-authored session logout manual](https://man.archlinux.org/man/extra/xfce4-session/xfce4-session-logout.1.en).
- [Mint June 2026 report, published July 8](https://blog.linuxmint.com/?m=202607),
  [Cinnamon session manager interface](https://raw.githubusercontent.com/linuxmint/cinnamon-session/master/cinnamon-session/org.gnome.SessionManager.xml).
- [COSMIC Epoch 1 update](https://blog.system76.com/blog/post/cosmic-epoch-1-updates),
  [cosmic-comp dependencies/backends](https://raw.githubusercontent.com/pop-os/cosmic-comp/master/Cargo.toml),
  [cosmic-randr](https://github.com/pop-os/cosmic-randr),
  [COSMIC Xwayland scaling work](https://blog.system76.com/blog/post/cosmic-alpha-7-never-been-beta).
- [Hyprland monitor interface](https://wiki.hypr.land/configuring/core/monitors/),
  [hyprctl](https://wiki.hypr.land/Configuring/Advanced-and-Cool/Using-hyprctl/),
  [Aquamarine scope](https://wiki.hypr.land/hypr-ecosystem/dev/aquamarine/).

Source branches can advance after this research. Their interface descriptions are
input to a WSX-owned contract, never runtime dependencies or unpinned build inputs.

## Input, DPI and authority details

Wayland routes input through the compositor; X11 inserts the X server into the
input/window-management path. Consequently a working X11 automation command does
not establish equivalent Wayland access. Xwayland application compatibility also
does not mean that the whole desktop is an X11 session.
[Upstream architecture comparison](https://wayland.freedesktop.org/architecture.html),
[Wayland protocol and focus](https://wayland.freedesktop.org/docs/book/Protocol.html).

| Family | Input/control authority to keep inside Guest | DPI consequence for WSX |
| --- | --- | --- |
| GNOME | Mutter/user session owns focus and display changes; remote-control consent is a separate portal/backend integration. Do not turn root access to WSL into a promise of authorized application capture. | Logical monitor layouts, supported mode scales and configuration serials must be queried; Windows DPI does not directly prescribe a Mutter configuration. |
| Plasma | KWin/user-bus output control and the session-owned input bridge are separate from Polkit actions and portal authorization. Preserve the existing ordinary-user KScreen execution. | KScreen's logical size must not replace physical mode dimensions; read back scale and mode after application. |
| Xfce | On Wayland, Xfce explicitly leaves keyboard/mouse settings to its chosen compositor; its initial portal support does not establish full remote input. On X11, settings use a different backend. | Xfce source separates X11 GDK window scaling from custom font DPI. Neither alone is a universal output-scale value; the Guest adapter must state what it can apply. |
| Cinnamon | Its session-manager inhibition/logout model is an additional lifecycle policy; inspect the actual Muffin/X11 or Wayland session instead of assuming GNOME's contemporary backend is interchangeable. | The transition's monitor/HiDPI work requires capability detection per packaged version; no unconditional fractional-scale guarantee follows from the Cinnamon name. |
| COSMIC | Smithay libinput/libseat and libei support demonstrate multiple device/remote-input integration points. Compile-time backend availability is not runtime permission or successful delivery. | Output configuration and Xwayland application scaling are distinct; report physical mode/scale while keeping application-specific adjustments Guest-owned. |
| Hyprland | The compositor IPC, session input device and privileged screencopy/plugin permissions are different authorities. A historical 0.49 permission interface already shows optional enforcement, so no universal policy can be inferred across releases. | Monitor scale, layout and transforms belong to the installed Hyprland adapter; IPC/config syntax changes must not become Core updates. |

These rows combine the primary sources above with explicit WSX design
consequences. Further details: [Xfce appearance implementation](https://raw.githubusercontent.com/xfce-mirror/xfce4-settings/master/dialogs/appearance-settings/main.c)
and [Hyprland 0.49 permission model](https://wiki.hypr.land/0.49.0/Configuring/Permissions/).
The latter is a versioned example, not a claim about current defaults. No new
portal, universal input injection, font-policy rewrite or desktop-wide permission
bypass is implemented by this research.

## Additional architecture probes

Niri is a scrollable-tiling Wayland compositor built on Smithay with its own IPC;
it tests whether the design accidentally assumes a traditional stacking desktop.
Sway is an i3-compatible Wayland compositor and tests the wlroots ecosystem.
LXQt's documented Wayland session supports multiple stacking and tiling
compositors; Budgie 10.10 recommends labwc. These demonstrate why selecting by
desktop name is insufficient. Their value here is architectural, not a promise
to add them to the Alpha.
[Sources: Niri](https://github.com/niri-wm/niri),
[Niri dependencies](https://raw.githubusercontent.com/niri-wm/niri/main/Cargo.toml),
[Sway](https://github.com/swaywm/sway),
[LXQt session matrix](https://github.com/lxqt/lxqt/wiki/Wayland-Session),
[Budgie transition](https://buddiesofbudgie.org/blog/budgie-10-10-released).

## Resulting protocol rules

1. **Separate compatibility layers.** Negotiate the Host/Guest control protocol,
   session lifecycle, display control, input transport and graphics producer/
   consumer contracts independently. A supported control protocol does not prove
   opaque presentation, GPU zero-copy, input delivery or safe resource retirement.
   Keep the already validated Venus/Rutabaga route; this research does not enable
   staging, substitute a remote-desktop data path or redesign the kernel protocol.
2. **Discover sessions and capabilities.** A future image can offer several named
   sessions. Session IDs are opaque; desktop/compositor labels are informational.
   Require only capabilities actually used. Multi-output, fractional scaling,
   rotation, HDR, VRR, clipboard, audio and remote-input permission are optional,
   versioned capabilities. Unsupported is distinct from failed or unavailable.
3. **Model outputs before commands.** Use an output collection, opaque output/mode
   IDs, physical pixels, logical layout, rational scale, refresh units, transform,
   enabled state and configuration generation. Report the supported scale domain;
   Windows DPI is a preference to negotiate, not an arbitrary supported scale.
   Alpha can expose one output without making single-output a protocol invariant.
4. **Make display changes explicit.** Include the expected session/configuration
   generation, requested outputs, persistence intent and actual applied result.
   Reject stale generations and unsupported atomicity; never silently report a
   requested mode that was rounded, rejected or overwritten. Temporary versus
   persistent behavior must be advertised. Mutter's serials and wlroots output
   configuration demonstrate why this is necessary.
5. **Keep input and authorization separate.** Distinguish a session-owned virtual
   device path from permission-mediated remote input/capture. Keyboard physical
   keys, text/IME, pointer coordinates, relative capture, touch and clipboard are
   distinct facilities. Guest owns layout, compose/IME and compositor integration;
   session ownership/epoch scopes input and requires key/button release on loss.
6. **Use observed lifecycle states.** A launch request, session readiness, first
   presented frame, lock screen, compositor failure, normal logout and GPU/device
   retirement are different observations. Normal logout may be inhibited. Do not
   infer safe native retirement from a stopped unit, hidden window or dead PID.
7. **Keep privileges narrow.** WSL root control admits the owned account/device;
   the actual desktop runs as the ordinary user. Guest handles PAM, user D-Bus,
   logind/libseat/seatd and Polkit/portal policies. No generic shell or arbitrary
   path-execution method is part of the protocol. Passwords stay on the separate
   controlled interactive initialization channel.
8. **Make upgrades boring.** Major versions change semantics; minor versions add
   compatible optional fields/capabilities. Missing required capabilities fail
   precisely. Image tags and package versions are not control-protocol versions.
   Release metadata binds bytes/provenance and required protocols; Guest checks
   its own packages against its own component lock. Unknown required graphics
   contracts are rejected, not guessed from desktop name or kernel major version.

Wayland's fractional-scale protocol explicitly separates surface size from
buffer size and expresses a preferred scale using a denominator of 120; WSX
should preserve the distinction without requiring every backend to support every
fraction. Output-management protocols themselves can be experimental or specific
to a compositor ecosystem.
[Wayland fractional scale](https://raw.githubusercontent.com/wayland-mirror/wayland-protocols/main/staging/fractional-scale/fractional-scale-v1.xml),
[wlroots output management](https://raw.githubusercontent.com/swaywm/wlr-protocols/master/unstable/wlr-output-management-unstable-v1.xml).

XDG RemoteDesktop and InputCapture portals describe session-scoped permissions
and input facilities. They are valuable Guest-side integrations where supported,
but do not supply a universal compositor configuration or WSX GPU retirement API.
Do not substitute a portal screenshot/screencast for the existing presentation
contract, or bypass a required consent decision.
[RemoteDesktop](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html),
[InputCapture](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.InputCapture.html).

## Alpha boundary

Implement the common boundary against the two existing Guest adapters. Validate
wire rejection, capability negotiation, snapshot binding and old-image migration
with focused tests. Keep historical/current machine evidence separate. Do not
install GNOME, Xfce, Cinnamon, COSMIC, Niri, Sway, LXQt or Budgie merely to claim a
complete matrix. A standard control interface cannot make an unported compositor
compatible with the private graphics consumer ABI: that port and its acceptance
remain future, separately recorded work.
