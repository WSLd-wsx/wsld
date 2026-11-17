#!/bin/sh
# Retire only the exact WSX PCI function after its distro has stopped.
# Run in WSL's system distribution; never restart the stopped user distro.
set -eu
instance=$1
case "$instance" in
  ????????-????-????-????-????????????) ;;
  *) echo 'Invalid instance identity' >&2; exit 2 ;;
esac
case "$instance" in *[!0-9a-f-]*) echo 'Invalid instance identity' >&2; exit 2 ;; esac
test "$(id -u)" = 0
# The helper is launched while the original VM is alive. Native waits for this
# handshake before rendering and later writes one command to the existing stdin.
# No new WSL execution is requested after distro/VM loss. EOF abandons it safely.
printf 'WSX_SYSTEM_READY\n'
IFS= read -r command || exit 0
test "$command" = retire || { echo 'Invalid retirement command' >&2; exit 2; }
match=
for device in /sys/bus/pci/devices/*; do
    test -e "$device" || continue
    parent=$(readlink -f "$device")
    while test "$parent" != /; do
        parent=$(dirname "$parent")
        if test -L "$parent/subsystem" && test "$(basename "$(readlink -f "$parent/subsystem")")" = vmbus; then
            actual=$(tr -d '{}' < "$parent/device_id" | tr 'A-F' 'a-f')
            if test "$actual" = "$instance"; then
                test -z "$match" || { echo 'Ambiguous PCI owner' >&2; exit 3; }
                match=$device
            fi
            break
        fi
    done
done
test -n "$match" || { echo 'Exact PCI owner not found' >&2; exit 3; }
test "$(basename "$(readlink -f "$match/driver")")" = wsx-virtio-msg-pci
test -e "$match/retire"
# This synchronous kernel operation must finish while Native continues servicing
# UNREF/RESET. Distro absence alone never acknowledges device or GPU retirement.
printf '1\n' > "$match/retire"
printf 'WSX_GUEST_RETIRED instance=%s pci=%s source=system-distro\n' "$instance" "$(basename "$match")"
