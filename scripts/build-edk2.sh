#!/usr/bin/env bash
# Host-side wrapper: build the EDK2 UEFI payload in the Lima VM and copy the
# result to dist/uefi_<device>.img.
# Usage: scripts/build-edk2.sh [vm-name] [device]   (default: pve-builder whyred)
set -euo pipefail

VM="${1:-pve-builder}"
DEVICE="${2:-${DEVICE:-whyred}}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"

case "$DEVICE" in
    whyred|lavender) ;;
    *) echo "unknown device: $DEVICE (whyred|lavender)"; exit 1;;
esac

command -v limactl >/dev/null || { echo "limactl not in PATH (Lima is required)"; exit 1; }

limactl start "$VM" >/dev/null 2>&1 || true
limactl cp "$REPO/edk2/vm-build-edk2.sh" "$VM:/tmp/vm-build-edk2.sh"

if [ "$DEVICE" = "lavender" ]; then
    echo "[*] applying the lavender device port to the edk2-msm tree"
    limactl cp "$REPO/edk2/vm-port-lavender.sh" "$VM:/tmp/vm-port-lavender.sh"
    limactl shell "$VM" -- env DEVICE="$DEVICE" PORT_ONLY=1 bash /tmp/vm-port-lavender.sh
fi

echo "[*] building edk2-msm for $DEVICE (10-30 min)"
limactl shell "$VM" -- env DEVICE="$DEVICE" bash /tmp/vm-build-edk2.sh

OUT_NAME="uefi_$DEVICE.img"
mkdir -p "$REPO/dist"
limactl shell "$VM" -- test -s "edk2-out/$OUT_NAME" || {
    echo "build did not produce edk2-out/$OUT_NAME — nothing copied"; exit 1;
}
limactl cp "$VM:edk2-out/$OUT_NAME" "$REPO/dist/$OUT_NAME"
echo "[*] copied edk2-out/$OUT_NAME -> dist/$OUT_NAME"
ls -la "$REPO/dist/"
