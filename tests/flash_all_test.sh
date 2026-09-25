#!/usr/bin/env bash
# Exercises the fail-closed gates of flash_all.sh --check. No device is ever
# touched: --check exits before any fastboot access.
set -uo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
FAIL=0

stub_bin="$TMP/bin"
mkdir -p "$stub_bin"
printf '#!/bin/sh\nexit 0\n' > "$stub_bin/fastboot"
chmod +x "$stub_bin/fastboot"
export PATH="$stub_bin:$PATH"

mk_boot() { # valid | broken
    cp "$REPO/dist/boot_pve_whyred.img" "$1"
    [ "$2" = broken ] && printf 'not a boot image' | dd of="$1" bs=1 seek=0 conv=notrunc status=none
    return 0
}

mk_rootfs() { # logical_bytes
    python3 - "$1" "$2" <<'PY'
import struct, sys
out, logical = sys.argv[1], int(sys.argv[2])
bs, blocks = 4096, max(1, logical // 4096)
hdr = struct.pack('<IHHHHIIII', 0xED26FF3A, 1, 0, 28, 12, bs, blocks, 1, 0)
chunk = struct.pack('<HHII', 0xCAC3, 0, blocks, 12)
open(out, 'wb').write(hdr + chunk)
PY
}

check() { # description, expected_exit, extra_env...
    local desc=$1 want=$2; shift 2
    local out rc
    out=$(env "$@" DIST_DIR="$FIXTURE" "$REPO/flash_all.sh" --check 2>&1); rc=$?
    if [ "$rc" -eq "$want" ]; then
        printf 'ok   %-46s (exit %s)\n' "$desc" "$rc"
    else
        printf 'FAIL %-46s (exit %s, want %s)\n%s\n' "$desc" "$rc" "$want" "$out"
        FAIL=1
    fi
}

FIXTURE="$TMP/good"; mkdir -p "$FIXTURE"
mk_boot "$FIXTURE/boot_pve_whyred.img" valid
mk_rootfs "$FIXTURE/pve_rootfs_arm64.sparse.img" $((8 * 1024 * 1024 * 1024))
check "complete, consistent artifacts" 0
check "unknown DEVICE" 1 DEVICE=nokia
check "unknown PLAN" 1 PLAN=bsd

FIXTURE="$TMP/brokenboot"; mkdir -p "$FIXTURE"
mk_boot "$FIXTURE/boot_pve_whyred.img" broken
mk_rootfs "$FIXTURE/pve_rootfs_arm64.sparse.img" $((8 * 1024 * 1024 * 1024))
check "corrupt boot image" 1

FIXTURE="$TMP/bigrootfs"; mkdir -p "$FIXTURE"
mk_boot "$FIXTURE/boot_pve_whyred.img" valid
mk_rootfs "$FIXTURE/pve_rootfs_arm64.sparse.img" $((64 * 1024 * 1024 * 1024))
check "rootfs larger than userdata" 1

FIXTURE="$TMP/badsparse"; mkdir -p "$FIXTURE"
mk_boot "$FIXTURE/boot_pve_whyred.img" valid
printf 'not sparse' > "$FIXTURE/pve_rootfs_arm64.sparse.img"
check "rootfs is not a sparse image" 1

FIXTURE="$TMP/uefi"; mkdir -p "$FIXTURE"
cp "$REPO/dist/uefi_whyred.img" "$FIXTURE/uefi_whyred.img"
mk_rootfs "$FIXTURE/pve_rootfs_arm64.sparse.img" $((8 * 1024 * 1024 * 1024))
check "Plan A payload with matching profile" 0 PLAN=uefi

[ "$FAIL" -eq 0 ] && echo "flash_all gates: ALL PASS" || { echo "flash_all gates: FAILURES"; exit 1; }
