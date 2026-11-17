# Verified prototype migration cross-check

This records product wiring as well as binary/source reuse. A copied executable
does not inherit a validated deployment profile automatically. Evidence paths
below refer to the read-only sibling `wsx-proto` checkout for development review;
the product must never search that checkout at runtime. Historical acceptance is
not a substitute for acceptance of this product lifetime.

## Native runtime profile

Authority: `artifacts/venus-rutabaga/ram-threshold-native-build-01/native_main.rs`
defines a strict `Profile` with `deny_unknown_fields`. Core constructs its wire
JSON in `core/src/graphics.rs::direct_profile`; `desktop_windows.rs::start_inner`
writes it adjacent to the copied Native executable before launch.

| Profile fields | Verified meaning | Product wiring / review result |
| --- | --- | --- |
| `distribution` | Actual WSL distro name | Selected registration name, not an installation ID or prototype name. |
| `port` | Per-lifetime Hyper-V socket pair | Fresh allocated candidate pair; actual exclusive bind decides availability. |
| `vendor_id`, `device_id`, `device_number` | Verified virtual device carrier | Existing admitted carrier parameters; these are not physical GPU vendor selection. |
| `aperture_size`, `max_guest_pages` | Carrier extent and admission budget | Existing 1 GiB / 262144-page reference values. Not a measured Windows mapping quota. |
| `guest_ram_reclaim_pages` | CPU aperture cache high watermark; low is half | **Migration omission fixed:** Core now explicitly selects 64 (clamped to the admission budget), reusing the real 64/32 reference trial. Previously omitted, so Native defaulted to 1024. |
| `adapter_device_path`, `device_uuid`, `driver_uuid`, `node_mask` | Actual active display/Vulkan identity | Derived from the live LUID join; no Iris/RTX branch or copied prototype identity. |

The 64/32 reference is `artifacts/venus-rutabaga/ram-reclaim-threshold-01`.
Its `test-profile.json`, `result.json` and actual `native.log` agree on 17 real
reclaim cycles, 645 presentations and 49 presentations after the last reclaim.
All completed reclaims end at 32 usable pages. This was a bounded Iris Xe/CachyOS
trial, not current Omarchy/RTX acceptance or an infinite-load guarantee.
Checked copies and SHA-256 provenance are in
`wsld/artifacts/interactive-hang-20261105-01/reference-reclaim-evidence` and
`reference-verification.json`. The originals remain unchanged.

## Implementation and components

| Area | Reused implementation/evidence | Product state and limit |
| --- | --- | --- |
| Safe CPU cache reclaim | Reference `lib.rs` scoped batch checkpoint and `guest-memory/ram.rs` | These two source files match the diagnostic pair's source inventory byte-for-byte. `Rc::get_mut` proves all views/slices ended before unmap; no eviction inside live views. Core profile omission is fixed without rewriting this implementation. |
| Quota/revoke failure | Original `GuestLease` and cache fault retention | Native errors remain terminal; unknown mappings are retained. The fix is preventive reclaim, not retry after uncertain failure. A single unusually large batch may still exceed a quota. |
| Renderer, input, Guest modules | 12-entry hash-pinned reference bundle | Input, epoxy, launch script, Guest launcher and six modules retain reference hashes. Native/renderer lifetime changes have separately archived patches/build receipts. No GPU-vendor fork. |
| Native lifecycle | Independent Host and original COM ownership | Existing closure plus product WM_CLOSE/external-stop/VM-exit adaptations. Bounded successful shutdown evidence does not cover interactive stability; later quota failure is separately retained. |
| Guest device/DRM permissions | VMBus-instance ancestry and reference ACL adapter | Core embeds the verified DRM adapter digest and performs live kernel CRC/component checks; account identity comes from Guest. |
| Rootfs session | Guest prepare/session/start ABI | Rootfs owns userspace and session behavior; Core orchestrates the admitted instance. No image patch is used to compensate for this host-profile omission. |
| Running-state authority | Current WSL/HCS, Guest and corresponding Host | ADR 009 removes historical log/handwritten receipt gates for a fresh VM; old failures remain diagnostics. Existing startup provenance records still exist. |
| Display work area and Windows DPI | ring-timer-port-01 `host-output-window.h` PMv2 / FIT_CLIENT and presenter source-mode fitting; pinned Hyprland `hl.monitor` and clean scale rules | Core now selects a fitting advertised common mode, controls the exact Guest session, centers and verifies the renderer. Existing runtime binaries are unchanged. Startup and explicit `desktop fit` share the implementation. See ADR 011 for evidence and limits. |

When migrating or updating the pinned reference, review the actual selected
profile and optional defaults together with source/binary receipts and their
bounded validation record. Record a deliberate omission explicitly; do not rely
on executable defaults without checking whether the verified trial used them.

Current product follow-up: ADR 010 records actual Omarchy/RTX reclamation beyond
300 cumulative mappings and the user's passing repeated screensaver/menu test.
This is separate from the original Iris Xe/CachyOS evidence and does not replace
future sustained-load or retirement acceptance.

## CachyOS Plasma migration, 2026-11-05

- Reused `rootfs-cachyos-r2-start-01` evidence and the rootfs-packaged verified
  Plasma session adapter; Core does not duplicate session startup.
- DRM access adapter is byte-identical to that successful trial. Preserve the
  current 64/32 RAM reclaim profile and existing kernel-symbol admission.
- New Native helper delegates Plasma logout to Guest before exact-device
  retirement. Renderer sources and Native GPU/memory implementation are unchanged.
- Product startup, KWin display fitting and user-confirmed input passed on current
  Iris Xe. Historical r2 logout/restart does not certify current Native teardown.
- See [ADR 012](ADR-012.md) and its source comparison, live failures and evidence.
