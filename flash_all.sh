#!/usr/bin/env bash
# whyred PVE-UEFI flash script — RUN MANUALLY, by the device owner.
# The build pipeline NEVER executes this.
#
# Prereqs: bootloader unlocked (Mi Unlock), `adb` + `fastboot` in PATH,
# artifacts in dist/: uefi_whyred.img, pve_rootfs_arm64.sparse.img
#
# Usage:
#   ./flash_all.sh --check     # verify artifacts only, no device writes
#   ./flash_all.sh             # full sequence
set -euo pipefail

DIST="$(cd "$(dirname "$0")" && pwd)/dist"
UEFI="$DIST/uefi_whyred.img"
ROOTFS="$DIST/pve_rootfs_arm64.sparse.img"
LOG="${LOG:-flash-whyred.log}"

sha() { shasum -a 256 "$1" | awk '{print $1}'; }

if [[ "${1:-}" == "--check" ]]; then
    for f in "$UEFI" "$ROOTFS"; do
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
