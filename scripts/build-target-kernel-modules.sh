#!/usr/bin/env bash
# Diagnostic target build; never load or publish the resulting modules.
set -euo pipefail
if [[ $# != 7 ]]; then
    printf 'Usage: %s TAG COMMIT PACKAGE PACKAGE_SHA256 PACKAGE_VERSION NEW_WORKSPACE REUSABLE_CACHE\n' "$0" >&2
    exit 2
fi
tag=$1
commit=$2
package=$(realpath -e -- "$3")
package_sha=$4
package_version=$5
workspace_arg=$6
cache_arg=$7
repo=$(cd "$(dirname "$0")/.." && pwd -P)
jobs=${WSX_KERNEL_BUILD_JOBS:-4}
if ! [[ "$jobs" =~ ^[0-9]+$ ]] || (( 10#$jobs < 1 || 10#$jobs > 32 )); then
    printf 'WSX_KERNEL_BUILD_JOBS must be between 1 and 32\n' >&2
    exit 2
fi
if [[ -e "$workspace_arg" || -L "$workspace_arg" ]]; then
    printf 'Workspace already exists\n' >&2
    exit 1
fi
mkdir -p -- "$cache_arg" "$workspace_arg"
cache=$(cd "$cache_arg" && pwd -P)
workspace=$(cd "$workspace_arg" && pwd -P)
python3 "$repo/scripts/inspect-wsl-kernel-package.py" --package "$package" --sha256 "$package_sha" \
    --version "$package_version" --output "$workspace/target-inspection"
config_sha=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["kernel_config_sha256"])' "$workspace/target-inspection/inspection.json")
build_revision=$(python3 -c 'import json,re,sys; v=json.load(open(sys.argv[1]))["upstream_build_metadata"]["git_revision"]; assert re.fullmatch(r"[0-9a-f]{40}",v); print(v)' "$workspace/target-inspection/inspection.json")
if grep -qx '# CONFIG_DRM is not set' "$workspace/target-inspection/kernel_config"; then
    printf 'Unsupported target: the official kernel has CONFIG_DRM disabled; the current six-module closure requires built-in DRM.\n' >&2
    exit 2
fi
python3 "$repo/scripts/prepare-target-kernel-source.py" --tag "$tag" --commit "$commit" \
    --config "$workspace/target-inspection/kernel_config" --config-sha256 "$config_sha" \
    --cache "$cache/wsl-source.git" --output "$workspace/kernel" --build-revision "$build_revision"
# Verify the checked-out kernel source before preparation generates files.
if [[ $(git -C "$workspace/kernel" rev-parse HEAD) != "$commit" ]] || \
   ! git -C "$workspace/kernel" diff --quiet --exit-code || \
   ! git -C "$workspace/kernel" diff --cached --quiet --exit-code; then
    printf 'Kernel source differs from the selected commit\n' >&2
    exit 1
fi
python3 "$repo/scripts/archive-target-kernel-source.py" --kernel "$workspace/kernel" \
    --output "$workspace/kernel-source.tar.gz" --receipt "$workspace/kernel-source-receipt.json"
mkdir "$workspace/environment"
mkdir -p "$cache/toolchain"
python3 "$repo/scripts/prepare-kernel-toolchain.py" --root "$cache/toolchain" --archive "$cache/gcc.tar.xz" --reuse
prefix="$cache/toolchain/opt/wsx-gcc/bin/x86_64-linux-"
python3 "$repo/scripts/prepare-wsl-module-tree.py" --kernel-tree "$workspace/kernel" \
    --kernel-image "$workspace/target-inspection/kernel" --kernel-notes "$workspace/target-inspection/kernel_notes" \
    --target-inspection "$workspace/target-inspection/inspection.json" --compiler-prefix "$prefix" \
    --allow-runner-config --jobs "$jobs" > "$workspace/kernel-prepare.log" 2>&1 \
    || { cat "$workspace/kernel-prepare.log" >&2; exit 1; }
python3 "$repo/scripts/build-kernel-modules.py" --kernel-tree "$workspace/kernel" --output "$workspace/modules-build" \
    --compiler-prefix "$prefix" --kernel-image "$workspace/target-inspection/kernel" \
    --kernel-notes "$workspace/target-inspection/kernel_notes" --target-inspection "$workspace/target-inspection/inspection.json" \
    --allow-runner-config --jobs "$jobs" > "$workspace/modules-build.log" 2>&1 \
    || { cat "$workspace/modules-build.log" >&2; if [[ -f "$workspace/modules-build/build.log" ]]; then cat "$workspace/modules-build/build.log" >&2; fi; exit 1; }
python3 "$repo/scripts/compare-kernel-module-crcs.py" \
    --kernel-exports "$workspace/target-inspection/kernel-exports.json" \
    --modules "$workspace/modules-build/modules" \
    --output "$workspace/static-crc-audit.json"
python3 "$repo/scripts/finalize-kernel-build.py" "$workspace"
printf 'Diagnostic target built; no module was loaded or admitted for release.\n'
