#!/bin/sh
# lavender-boot-fastboot.sh — run via UART screen, forces fastboot via BCB
# Also dumps all useful info before rebooting.

echo "========================================="
echo "  LAVENDER BOOTLOADER FORCE SCRIPT"
echo "========================================="

# --- 1. Find partitions ---
echo ""
echo "[1/5] PARTITION DISCOVERY"
MISC=""
DEVINFO=""
for base in /dev/block/bootdevice/by-name /dev/block/mmcblk1/by-name /dev/bootdevice/by-name; do
  [ -d "$base" ] && echo "  by-name dir: $base" && ls "$base"/ 2>/dev/null
  [ -z "$MISC" ] && [ -e "$base/misc" ] && MISC="$base/misc"
  [ -z "$DEVINFO" ] && [ -e "$base/devinfo" ] && DEVINFO="$base/devinfo"
done
# fallback: scan partitions
if [ -z "$MISC" ]; then
  echo "  scanning mmcblk1p* for misc..."
  for p in /dev/block/mmcblk1p*; do
    name=$(blkid -s LABEL -o value "$p" 2>/dev/null)
    [ "$name" = "misc" ] && MISC="$p" && echo "  found misc: $p"
  done
fi
# find devinfo by number if by-name failed
if [ -z "$DEVINFO" ]; then
  for n in 43 42 41 40 39; do
    [ -b "/dev/block/mmcblk1p${n}" ] && DEVINFO="/dev/block/mmcblk1p${n}" && echo "  devinfo candidate: $DEVINFO" && break
  done
fi
echo "  MISC=$MISC"
echo "  DEVINFO=$DEVINFO"

# --- 2. Boot info ---
echo ""
echo "[2/5] BOOT INFO"
echo "  cmdline: $(cat /proc/cmdline 2>/dev/null | head -c 500)"
echo "  kernel: $(uname -a 2>/dev/null)"
echo "  id: $(id 2>/dev/null)"

# --- 3. Partition table ---
echo ""
echo "[3/5] PARTITION TABLE"
fdisk -l /dev/block/mmcblk1 2>/dev/null | head -30 || \
  sgdisk -p /dev/block/mmcblk1 2>/dev/null | head -30 || \
  partx -s /dev/block/mmcblk1 2>/dev/null | head -30 || \
  echo "  (no fdisk/sgdisk/partx)"

# --- 4. Devinfo dump ---
echo ""
echo "[4/5] DEVINFO HEX"
if [ -n "$DEVINFO" ] && [ -r "$DEVINFO" ]; then
  echo "  reading $DEVINFO..."
  dd if="$DEVINFO" bs=256 count=4 2>/dev/null | xxd 2>/dev/null || \
    dd if="$DEVINFO" bs=256 count=4 2>/dev/null | od -A x -t x1 2>/dev/null
elif [ -n "$DEVINFO" ]; then
  echo "  $DEVINFO not readable (permission denied?)"
  # try raw partition scan
  for n in 43 42 41 40; do
    p="/dev/block/mmcblk1p${n}"
    [ -r "$p" ] && echo "  reading $p..." && dd if="$p" bs=256 count=4 2>/dev/null | xxd 2>/dev/null && break
  done
else
  echo "  devinfo partition not found"
fi

# --- 5. Force fastboot via BCB ---
echo ""
echo "[5/5] FORCE FASTBOOT"
if [ -n "$MISC" ] && [ -w "$MISC" ]; then
  # write BCB: command = "bootonce-bootloader" (32 bytes at offset 0)
  printf 'bootonce-bootloader\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00' | dd of="$MISC" bs=1 count=32 conv=notrunc 2>&1
  echo "  BCB written to $MISC"
  echo "  REBOOTING in 2 seconds..."
  sync
  sleep 2
  reboot
elif [ -n "$MISC" ]; then
  echo "  $MISC not writable"
else
  echo "  misc partition not found — cannot force fastboot"
  echo "  REBOOTING anyway (button combo needed)..."
  reboot
fi
