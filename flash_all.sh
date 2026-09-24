#!/usr/bin/env bash
# whyred/lavender PVE-UEFI flash script — RUN MANUALLY, by the device owner.
# The build pipeline NEVER executes this.
#
# Prereqs: bootloader UNLOCKED (Mi Unlock), `adb` + `fastboot` in PATH,
# a verified backup of `boot` + `userdata` in EDL/TWRP, artifacts in dist/.
#
# Usage:
#   ./flash_all.sh --check              # verify artifacts only, no device writes
#   DEVICE=whyred ./flash_all.sh        # Plan B: mainline kernel + rootfs
#   PLAN=uefi DEVICE=whyred ./flash_all.sh   # Plan A: EDK2 UEFI payload + rootfs
#   SERIAL=<fastboot serial> ...        # only if two phones are attached
set -euo pipefail

REPO="$(cd "$(dirname "$0")" && pwd)"
DEVICE="${DEVICE:-whyred}"
PLAN="${PLAN:-kernel}"
SERIAL="${SERIAL:-}"
case "$DEVICE" in
    whyred|lavender) ;;
    *) echo "unknown DEVICE=$DEVICE (whyred|lavender)"; exit 1;;
esac
case "$PLAN" in
    kernel) BOOT_NAME="boot_pve_$DEVICE.img" ;;
    uefi)   BOOT_NAME="uefi_$DEVICE.img" ;;
    *) echo "unknown PLAN=$PLAN (kernel|uefi)"; exit 1;;
esac

DIST="$REPO/dist"
BOOT="$DIST/$BOOT_NAME"
ROOTFS="$DIST/pve_rootfs_arm64.sparse.img"
LOG="$REPO/flash-$DEVICE.log"

command -v fastboot >/dev/null || { echo "fastboot not in PATH"; exit 1; }

if [[ "${1:-}" == "--check" ]]; then
    for f in "$BOOT" "$ROOTFS"; do
        [ -s "$f" ] || { echo "missing or empty: $f"; exit 1; }
    done
    if [ -f "$DIST/SHA256SUMS" ]; then
        ( cd "$DIST" && shasum -a 256 -c SHA256SUMS ) || { echo "SHA256SUMS mismatch"; exit 1; }
    else
        echo "note: dist/SHA256SUMS absent — hashes not verified"
    fi
    echo "OK  $(basename "$BOOT")  $(wc -c <"$BOOT" | tr -d ' ') bytes"
    echo "OK  $(basename "$ROOTFS")  $(wc -c <"$ROOTFS" | tr -d ' ') bytes"
    exit 0
fi

exec >> >(tee -a "$LOG") 2>&1

# ---- device selection -------------------------------------------------------
DEVICES=()
while read -r line; do
    # shellcheck disable=SC2086
    set -- $line
    [ "${2:-}" = "fastboot" ] && DEVICES+=("${1:-}")
done < <(fastboot devices)
if [ "${#DEVICES[@]}" -eq 0 ]; then
    echo "no device in fastboot mode"; exit 1
elif [ "${#DEVICES[@]}" -gt 1 ] && [ -z "$SERIAL" ]; then
    echo "several fastboot devices attached (${DEVICES[*]}) — set SERIAL="; exit 1
fi
if [ -z "$SERIAL" ]; then
    SERIAL="${DEVICES[0]}"
fi
fb() { fastboot -s "$SERIAL" "$@"; }

sleep 2
PRODUCT="$(fb getvar product 2>&1 | tr -d '\r' | sed -n 's/.*product: *//p')"
UNLOCKED="$(fb getvar unlocked 2>&1 | tr -d '\r' | sed -n 's/.*unlocked: *//p')"
echo "=== flash $DEVICE / plan $PLAN ==="
echo "serial:   $SERIAL"
echo "product:  ${PRODUCT:-unknown}"
echo "unlocked: ${UNLOCKED:-unknown}"
[ "$PRODUCT" = "$DEVICE" ] || { echo "product '$PRODUCT' != DEVICE '$DEVICE' — wrong phone"; exit 1; }
[ "$UNLOCKED" = "yes" ] || { echo "bootloader is not unlocked — flashing will fail"; exit 1; }

cat <<EOF

This OVERWRITES the 'boot' partition and ERASES all phone data ('userdata').
A verified backup of boot + userdata must already exist.
Type the serial ($SERIAL) to continue, anything else aborts.
EOF
read -r CONFIRM
[ "$CONFIRM" = "$SERIAL" ] || { echo "aborted"; exit 1; }

# ---- artifact integrity before the first write ------------------------------
for f in "$BOOT" "$ROOTFS"; do
    [ -s "$f" ] || { echo "missing or empty: $f"; exit 1; }
done
if [ -f "$DIST/SHA256SUMS" ]; then
    ( cd "$DIST" && shasum -a 256 -c SHA256SUMS ) || { echo "SHA256SUMS mismatch — refusing to flash"; exit 1; }
fi

echo "[1/3] $BOOT_NAME -> boot"
fb flash boot "$BOOT"

echo "[2/3] rootfs -> userdata (sparse, ~10-20 min)"
fb flash userdata "$ROOTFS"

echo "[3/3] rebooting"
fb reboot

echo "serial console: screen /dev/cu.wchusbserial* 115200 | Web UI: https://10.15.0.254:8006"
