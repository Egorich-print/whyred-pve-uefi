#!/usr/bin/env bash
# Host-side wrapper: build PVE rootfs + mainline kernel in the Lima VM,
# then sparse-compress the rootfs image with our own sparse-rs.
set -euo pipefail

VM="${1:-pve-builder}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"

limactl start "$VM" >/dev/null 2>&1 || true
limactl cp "$REPO/pve/vm-build-rootfs.sh" "$VM:/tmp/vm-build-rootfs.sh"
limactl cp "$REPO/pve/kernel-config.fragment" "$VM:/tmp/kernel-config.fragment"

echo "[*] building kernel + rootfs (kernel ~40-60 min, rootfs apt ~20 min)"
limactl shell "$VM" -- bash /tmp/vm-build-rootfs.sh

mkdir -p "$REPO/dist"
limactl copy "VM:$HOME/out/Image.gz-whyred" "$REPO/dist/" 2>/dev/null || \
    limactl copy "$VM:out/Image.gz-whyred" "$REPO/dist/"
limactl copy "$VM:out/pve_rootfs_arm64.img" "$REPO/dist/" 2>/dev/null || \
    limactl copy "$VM:out/pve_rootfs_arm64.img" "$REPO/dist/"

# pack ABL-bootable kernel image (Plan B boot path) with our tool
cd "$REPO/tools"
cargo run --release -q -p payload-packer -- "$REPO/dist/Image.gz-whyred" \
    --out "$REPO/dist/boot_pve_whyred.img" \
    --cmdline_extra "root=PARTLABEL=userdata rootwait rw"

echo "[*] converting rootfs to Android sparse format (faster fastboot flash)"
cargo run --release -q -p sparse-rs -- img2simg "$REPO/dist/pve_rootfs_arm64.img" \
    --out "$REPO/dist/pve_rootfs_arm64.sparse.img" --block-size 4096

ls -la "$REPO/dist/"
