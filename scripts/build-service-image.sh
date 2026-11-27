#!/usr/bin/env bash
# Build the Community service distribution and collect corresponding Debian sources.
set -euo pipefail
output=$(realpath -m "${1:?output directory required}")
mkdir "$output"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
rootfs="$work/rootfs"
mirror=https://deb.debian.org/debian
keyring=/usr/share/keyrings/debian-archive-keyring.gpg

debootstrap --arch=amd64 --variant=minbase --include=python3,ca-certificates \
    --force-check-gpg --keyring="$keyring" bookworm "$rootfs" "$mirror"
chroot "$rootfs" useradd --uid 1000 --create-home --shell /bin/sh wsx-service
cat > "$rootfs/etc/wsl.conf" <<'CONF'
[general]
guiApplications=true

[user]
default=wsx-service

[automount]
enabled=false

[interop]
enabled=false
appendWindowsPath=false
CONF
cat > "$rootfs/etc/apt/sources.list" <<'SOURCES'
deb https://deb.debian.org/debian bookworm main
deb https://deb.debian.org/debian bookworm-updates main
deb https://deb.debian.org/debian-security bookworm-security main
SOURCES
sed 's/^deb /deb-src /' "$rootfs/etc/apt/sources.list" > "$rootfs/etc/apt/sources.list.d/wsx-build-sources.list"
chroot "$rootfs" apt-get -o APT::Update::Error-Mode=any update
chroot "$rootfs" env DEBIAN_FRONTEND=noninteractive apt-get --assume-yes --no-remove upgrade
chroot "$rootfs" dpkg-query -W -f='${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n' \
    > "$output/wsx-service-packages.tsv"
mapfile -t sources < <(awk -F '\t' '{print $3 "=" $4}' "$output/wsx-service-packages.tsv" | sort -u)
mkdir "$rootfs/tmp/wsx-sources"
chroot "$rootfs" /bin/sh -ec 'cd /tmp/wsx-sources; apt-get --download-only --only-source source "$@"' sh "${sources[@]}"
tar -C "$rootfs/tmp/wsx-sources" -czf "$output/wsx-service-sources.tar.gz" .
rm -rf "$rootfs/tmp/wsx-sources" "$rootfs/debootstrap"
rm "$rootfs/etc/apt/sources.list.d/wsx-build-sources.list"
chroot "$rootfs" apt-get clean
rm -rf "$rootfs/var/lib/apt/lists/"* "$rootfs/usr/share/man" \
    "$rootfs/usr/share/info" "$rootfs/usr/share/locale"
du -sh "$rootfs" "$output/wsx-service-sources.tar.gz"
# Leave WSL to supply /dev and its WSLg mounts.
tar --numeric-owner --exclude='./dev/*' -C "$rootfs" -cf "$output/wsx-service-rootfs.tar" .
(cd "$output" && sha256sum wsx-service-rootfs.tar wsx-service-packages.tsv wsx-service-sources.tar.gz > SHA256SUMS)
chmod -R a+rX "$output"
