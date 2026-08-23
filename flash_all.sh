#!/usr/bin/env bash
# whyred PVE-UEFI flash script — RUN MANUALLY, by the device owner.
# The build pipeline NEVER executes this.
#
# Prereqs: bootloader unlocked (Mi Unlock), `adb` + `fastboot` in PATH,
# artifacts in dist/: uefi_whyred.img, pve_rootfs_arm64.sparse.img
#
# Usage:
#   ./flash_all.sh --check              # verify artifacts only, no device writes
#   DEVICE=lavender ./flash_all.sh      # full sequence for lavender (default whyred)
set -euo pipefail

DEVICE="${DEVICE:-whyred}"
case "$DEVICE" in
    whyred)   UEFI_NAME=uefi_whyred.img;    KERNEL_NAME=boot_pve_whyred.img;   PRODUCT=whyred ;;
    lavender) UEFI_NAME=uefi_lavender.img;  KERNEL_NAME=boot_pve_lavender.img; PRODUCT=lavender ;;
    *) echo "unknown device $DEVICE"; exit 1;;
esac

DIST="$(cd "$(dirname "$0")" && pwd)/dist"
UEFI="$DIST/$UEFI_NAME"
KERNEL="$DIST/$KERNEL_NAME"
ROOTFS="$DIST/pve_rootfs_arm64.sparse.img"   # shared across SDM660 family
LOG="${LOG:-flash-$DEVICE.log}"

sha() { shasum -a 256 "$1" | awk '{print $1}'; }

if [[ "${1:-}" == "--check" ]]; then
    for f in "$UEFI" "$KERNEL" "$ROOTFS"; do
        [ -f "$f" ] || { echo "missing: $f"; exit 1; }
        echo "OK  $(basename "$f")  sha256=$(sha "$f")"
    done
    exit 0
fi

exec >> >(tee -a "$LOG") 2>&1

echo "=== whyred PVE-UEFI flash — $(date) ==="
echo "This ERASES userdata (all phone data). Ctrl-C to abort, Enter to continue."
read -r

fastboot devices || { echo "no fastboot device"; exit 1; }

# safety: confirm we are talking to the right device class
PRODUCT=$(fastboot getvar product 2>&1 | grep -oE 'whyred' || true)
[[ "$PRODUCT" == "whyred" ]] || {
    echo "product is not whyred — override with WHYRED_OK=1"; [[ "${WHYRED_OK:-0}" == 1 ]] || exit 1;
}

echo "[1/5] UEFI payload -> boot"
fastboot flash boot "$UEFI"

echo "[2/5] rootfs -> userdata (sparse, ~10-20 min)"
fastboot flash userdata "$ROOTFS"

echo "[3/5] erase stale misc/recovery state"
fastboot erase misc || true

echo "[4/5] rebooting"
fastboot reboot

echo "[5/5] serial console: screen /dev/tty.usbserial* 115200 | Web UI: http://10.15.0.254:8006 after USB RNDIS link"
