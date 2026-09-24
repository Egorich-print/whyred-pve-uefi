#!/bin/sh
# lavender-boot-fastboot.sh — run in a root shell on the device (UART/pmOS).
# Dumps partition info + devinfo, then asks for confirmation before writing
# the 32-byte BCB command that makes ABL enter fastboot on the next boot.
#
# Usage: MISC=/dev/block/by-name/misc sh lavender-boot-fastboot.sh
# If MISC is not set the script only collects information and writes nothing.
set -eu

BCB_CMD="bootonce-bootloader"
BCB_HEX=$(printf '%s' "$BCB_CMD" | od -An -tx1 -v | tr -d ' \n')

echo "========================================="
echo "  LAVENDER RECON / FORCE FASTBOOT"
echo "========================================="

find_part() {
    name=$1
    for base in /dev/block/bootdevice/by-name /dev/block/mmcblk1/by-name \
                /dev/block/by-name /dev/bootdevice/by-name; do
        if [ -e "$base/$name" ]; then
            echo "$base/$name"
            return 0
        fi
    done
    return 1
}

MISC="${MISC:-}"
DEVINFO="${DEVINFO:-}"
BLOCKDEV="${BLOCKDEV:-/dev/block/mmcblk1}"

echo ""
echo "[1/4] PARTITION DISCOVERY"
if [ -z "$MISC" ]; then
    MISC=$(find_part misc || true)
    echo "  by-name lookup: ${MISC:-not found}"
fi
if [ -z "$DEVINFO" ]; then
    DEVINFO=$(find_part devinfo || true)
    echo "  devinfo: ${DEVINFO:-not found}"
fi
if [ -n "$MISC" ]; then echo "  MISC=$MISC"; fi
if [ -n "$DEVINFO" ]; then echo "  DEVINFO=$DEVINFO"; fi
echo "  (no partition numbers are guessed — pass MISC=/dev/... explicitly)"

echo ""
echo "[2/4] BOOT INFO"
echo "  cmdline: $(cut -c1-500 /proc/cmdline 2>/dev/null || echo unavailable)"
echo "  kernel:  $(uname -a 2>/dev/null || echo unavailable)"
echo "  GPT:"
if command -v sgdisk >/dev/null 2>&1; then
    sgdisk -p "$BLOCKDEV" 2>/dev/null || true
elif command -v fdisk >/dev/null 2>&1; then
    fdisk -l "$BLOCKDEV" 2>/dev/null || true
fi

echo ""
echo "[3/4] DEVINFO HEXDUMP (first 1 KiB)"
if [ -n "$DEVINFO" ] && [ -r "$DEVINFO" ]; then
    dd if="$DEVINFO" bs=1024 count=1 2>/dev/null | od -A x -t x1z | head -70
else
    echo "  devinfo not readable — set DEVINFO=/dev/... and rerun"
fi

echo ""
echo "[4/4] BCB WRITE"
if [ -z "$MISC" ] || [ ! -b "$MISC" ]; then
    echo "  no misc block device — nothing written (recon only)"
    exit 0
fi
if [ ! -w "$MISC" ]; then
    echo "  $MISC is not writable — nothing written"
    exit 0
fi
SIZE=$(blockdev --getsize64 "$MISC" 2>/dev/null || echo 0)
echo "  target: $MISC ($SIZE bytes)"
if [ "$SIZE" -lt 4096 ]; then
    echo "  partition smaller than 4 KiB — refusing to write"
    exit 1
fi
echo "  This overwrites the first 32 bytes of $MISC with '$BCB_CMD'."
printf '  Type the partition path to write it, anything else aborts: '
read -r CONFIRM
[ "$CONFIRM" = "$MISC" ] || { echo "aborted — nothing written"; exit 0; }

BACKUP=$(mktemp /tmp/misc-backup.XXXXXX)
dd if="$MISC" of="$BACKUP" bs=4096 count=1 2>/dev/null
echo "  backup of first 4 KiB: $BACKUP"

printf '%s' "$BCB_HEX" | tr -d '\n' | xxd -r -p | dd of="$MISC" bs=1 count=32 conv=notrunc 2>/dev/null
dd if="$MISC" bs=32 count=1 2>/dev/null | od -A n -t c | tr -d ' \n' > /tmp/bcb-readback
if grep -q "^$BCB_CMD" /tmp/bcb-readback 2>/dev/null; then
    echo "  readback OK: $(cat /tmp/bcb-readback)"
else
    echo "  READBACK MISMATCH: $(cat /tmp/bcb-readback 2>/dev/null)"
    exit 1
fi
sync
echo "  done — power off now (do NOT reboot), then start with USB unplugged:"
echo "    poweroff   # then reconnect USB only"
