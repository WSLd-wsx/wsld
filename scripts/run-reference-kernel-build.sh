#!/usr/bin/env bash
# Build the pinned WSL reference without installing packages or loading modules.
set -euo pipefail
if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    printf 'Usage: bash %s NEW_WORKSPACE [PACKAGE_CACHE]\n' "$0" >&2
    exit 2
fi
repo=$(cd "$(dirname "$0")/.." && pwd -P)
jobs=${WSX_KERNEL_BUILD_JOBS:-4}
if ! [[ "$jobs" =~ ^[0-9]+$ ]] || (( 10#$jobs < 1 || 10#$jobs > 32 )); then
    printf 'WSX_KERNEL_BUILD_JOBS must be between 1 and 32\n' >&2
    exit 2
fi
target_image=${WSX_TARGET_KERNEL_IMAGE:-}
target_notes=${WSX_TARGET_KERNEL_NOTES:-}
if [[ -n "$target_image" ]]; then
    target_image=$(realpath -e -- "$target_image")
    if [[ -n "$target_notes" ]]; then target_notes=$(realpath -e -- "$target_notes"); fi
    if [[ ! -f "$target_image" || ( -n "$target_notes" && ! -f "$target_notes" ) ]]; then
        printf 'Target kernel image and supplied notes must be regular files\n' >&2
        exit 2
    fi
elif [[ -n "$target_notes" || ${WSX_ALLOW_FULL_KERNEL_BUILD:-0} != 1 ]]; then
    printf 'Provide WSX_TARGET_KERNEL_IMAGE; full kernel build requires explicit WSX_ALLOW_FULL_KERNEL_BUILD=1\n' >&2
    exit 2
fi
if [ -e "$1" ] || [ -L "$1" ]; then
    printf 'Workspace already exists\n' >&2
    exit 1
fi
mkdir -p -- "$1"
workspace=$(cd "$1" && pwd -P)
if [[ -n "$target_image" && -z "$target_notes" ]]; then
    target_notes="$workspace/target-kernel-notes"
    python3 "$repo/scripts/extract-kernel-notes.py" --kernel-image "$target_image" \
        --output "$target_notes" > "$workspace/target-kernel-notes.json"
fi
if [[ ${WSX_USE_RUNNER_ENV:-0} == 1 ]]; then
    if [[ -z "$target_image" ]]; then
        printf 'Runner build requires the target kernel image\n' >&2
        exit 2
    fi
    mkdir -p "$workspace/environment" "$workspace/downloads"
    dpkg-query -W -f='${binary:Package} ${Version}\n' > "$workspace/runner-packages.txt"
    python3 "$repo/scripts/prepare-kernel-toolchain.py" --root "$workspace/environment" --archive "$workspace/downloads/gcc.tar.xz"
    python3 "$repo/scripts/prepare-wsl-kernel-source.py" --archive "$workspace/downloads/kernel.tar.gz" --output "$workspace/kernel"
    prefix="$workspace/environment/opt/wsx-gcc/bin/x86_64-linux-"
    python3 "$repo/scripts/prepare-wsl-module-tree.py" --kernel-tree "$workspace/kernel" \
        --kernel-image "$target_image" --kernel-notes "$target_notes" --jobs "$jobs" --compiler-prefix "$prefix" --allow-runner-config \
        > "$workspace/kernel-prepare.log" 2>&1 || { cat "$workspace/kernel-prepare.log" >&2; exit 1; }
    python3 "$repo/scripts/build-kernel-modules.py" --kernel-tree "$workspace/kernel" \
        --output "$workspace/modules-build" --compiler-prefix "$prefix" --kernel-image "$target_image" \
        --kernel-notes "$target_notes" --jobs "$jobs" --allow-runner-config > "$workspace/modules-build.log" 2>&1 \
        || { cat "$workspace/modules-build.log" >&2; if [[ -f "$workspace/modules-build/build.log" ]]; then cat "$workspace/modules-build/build.log" >&2; fi; exit 1; }
    python3 "$repo/scripts/finalize-kernel-build.py" "$workspace"
    printf 'Runner reference build complete; environment is recorded but not pinned for release.\n'
    exit 0
fi
prepare=(python3 "$repo/scripts/prepare-kernel-build-root.py"
    --lock "$repo/kernel-modules/build-inputs/ubuntu-noble-packages.lock.json"
    --output "$workspace/environment")
if [ "$#" -eq 2 ]; then prepare+=(--package-cache "$2"); fi
"${prepare[@]}"
gcc_archive="$workspace/downloads/gcc.tar.xz"
kernel_archive="$workspace/downloads/kernel.tar.gz"
if [ "$#" -eq 2 ]; then
    if [ -f "$2/gcc.tar.xz" ]; then gcc_archive="$2/gcc.tar.xz"; fi
    if [ -f "$2/kernel.tar.gz" ]; then kernel_archive="$2/kernel.tar.gz"; fi
fi
python3 "$repo/scripts/prepare-kernel-toolchain.py" --root "$workspace/environment/rootfs" --archive "$gcc_archive"
python3 "$repo/scripts/prepare-wsl-kernel-source.py" --archive "$kernel_archive" --output "$workspace/kernel"
root="$workspace/environment/rootfs"
common=(--unshare-net --ro-bind "$root" / --ro-bind "$repo" /home --proc /proc --dev /dev
    --setenv PATH /opt/wsx-gcc/bin:/usr/sbin:/usr/bin:/sbin:/bin --setenv HOME /tmp)
if [[ -n "$target_image" ]]; then
    bwrap "${common[@]}" --bind "$workspace/kernel" /work --tmpfs /tmp \
        --ro-bind "$target_image" /tmp/target-kernel --ro-bind "$target_notes" /tmp/target-notes \
        /usr/bin/python3 /home/scripts/prepare-wsl-module-tree.py --kernel-tree /work \
        --kernel-image /tmp/target-kernel --kernel-notes /tmp/target-notes --jobs "$jobs" \
        > "$workspace/kernel-prepare.log" 2>&1
    bwrap "${common[@]}" --ro-bind "$workspace/kernel" /work --bind "$workspace" /tmp \
        --ro-bind "$target_image" /tmp/target-kernel --ro-bind "$target_notes" /tmp/target-notes \
        /usr/bin/python3 /home/scripts/build-kernel-modules.py --kernel-tree /work \
        --output /tmp/modules-build --compiler-prefix /opt/wsx-gcc/bin/x86_64-linux- \
        --kernel-image /tmp/target-kernel --kernel-notes /tmp/target-notes --jobs "$jobs" \
        > "$workspace/modules-build.log" 2>&1
else
    bwrap "${common[@]}" --bind "$workspace/kernel" /work --tmpfs /tmp \
        /usr/bin/python3 /home/scripts/build-wsl-stock-kernel.py --kernel-tree /work --jobs "$jobs" \
        > "$workspace/stock-build.log" 2>&1
    bwrap "${common[@]}" --ro-bind "$workspace/kernel" /work --bind "$workspace" /tmp \
        /usr/bin/python3 /home/scripts/build-kernel-modules.py --kernel-tree /work \
        --output /tmp/modules-build --compiler-prefix /opt/wsx-gcc/bin/x86_64-linux- --jobs "$jobs" \
        > "$workspace/modules-build.log" 2>&1
fi
python3 "$repo/scripts/finalize-kernel-build.py" "$workspace"
printf 'Reference build complete. Modules are not loaded or admitted. Evidence: %s\n' "$workspace"
