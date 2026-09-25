#!/usr/bin/env bash
# Host-side wrapper: build the mainline kernel + PVE rootfs in the Lima VM,
# copy the artifacts, pack the ABL-bootable boot image and sparse-compress the
# rootfs with our own tools.
# Usage: scripts/build-rootfs.sh [vm-name] [device]   (default: pve-builder whyred)
set -euo pipefail

VM="${1:-pve-builder}"
DEVICE="${2:-${DEVICE:-whyred}}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"

case "$DEVICE" in
    whyred|lavender) ;;
    *) echo "unknown device: $DEVICE (whyred|lavender)"; exit 1;;
esac

command -v limactl >/dev/null || { echo "limactl not in PATH (Lima is required)"; exit 1; }
: "${PROXMOX_KEY_FPR:?set PROXMOX_KEY_FPR to the Proxmox release-key fingerprint (wiki.proxmox.com)}"
: "${PVE_HOSTNAME:=pve-arm64}"

limactl start "$VM" >/dev/null 2>&1 || true
limactl cp "$REPO/pve/vm-build-rootfs.sh" "$VM:/tmp/vm-build-rootfs.sh"
limactl cp "$REPO/pve/kernel-config.fragment" "$VM:/tmp/kernel-config.fragment"

echo "[*] building kernel + rootfs (kernel ~40-60 min, rootfs apt ~20 min)"
limactl shell "$VM" -- env DEVICE="$DEVICE" PROXMOX_KEY_FPR="$PROXMOX_KEY_FPR" \
    PVE_HOSTNAME="$PVE_HOSTNAME" bash /tmp/vm-build-rootfs.sh

mkdir -p "$REPO/dist"
# limactl shell inherits the HOST cwd and $HOME here is the host's: resolve the
# guest home once, then use absolute guest paths everywhere
GUEST_HOME=$(limactl shell "$VM" -- bash -lc 'echo $HOME')
KERNEL="$GUEST_HOME/out/Image.gz-$DEVICE"
limactl shell "$VM" -- test -s "$KERNEL" || {
    echo "VM did not produce $KERNEL — the guest build only builds the configured device kernel"; exit 1;
}
limactl copy "$VM:$KERNEL" "$REPO/dist/Image.gz-$DEVICE"
limactl copy "$VM:$GUEST_HOME/out/pve_rootfs_arm64.img" "$REPO/dist/"

cd "$REPO/tools"
cargo run --release -q -p payload-packer -- "$REPO/dist/Image.gz-$DEVICE" \
    --out "$REPO/dist/boot_pve_$DEVICE.img" \
    --cmdline-extra "root=PARTLABEL=userdata rootwait rw"

echo "[*] converting rootfs to Android sparse format (faster fastboot flash)"
cargo run --release -q -p sparse-rs -- img2simg "$REPO/dist/pve_rootfs_arm64.img" \
    --out "$REPO/dist/pve_rootfs_arm64.sparse.img" --block-size 4096

ls -la "$REPO/dist/"
