#!/usr/bin/env bash
# Host-side wrapper: build EDK2 UEFI payload for whyred in the Lima VM.
# Usage: scripts/build-edk2.sh [vm-name] [device]   (default: pve-builder whyred)
set -euo pipefail

VM="${1:-pve-builder}"
DEVICE="${2:-${DEVICE:-whyred}}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"

limactl start "$VM" >/dev/null 2>&1 || true

echo "[*] syncing build script"
limactl cp "$REPO/edk2/vm-build-edk2.sh" "$VM:/tmp/vm-build-edk2.sh"

echo "[*] building edk2-msm for whyred (this takes 10-30 min)"
limactl shell "$VM" -- bash -c "DEVICE=$DEVICE bash /tmp/vm-build-edk2.sh"

mkdir -p "$REPO/dist"
for f in $(limactl shell "$VM" -- ls edk2-out 2>/dev/null | grep -E '\.(img|fd)$' || true); do
    limactl cp "$VM:edk2-out/$f" "$REPO/dist/"
done

echo "[*] dist/ contents:"
ls -la "$REPO/dist/"
