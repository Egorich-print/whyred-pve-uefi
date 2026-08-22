#!/usr/bin/env bash
# Assemble dist/ manifest: SHA256SUMS for all artifacts.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO/dist"

[ -f uefi_whyred.img ] || { echo "dist/uefi_whyred.img missing — run scripts/build-edk2.sh"; exit 1; }
[ -f pve_rootfs_arm64.sparse.img ] || [ -f pve_rootfs_arm64.img ] || {
    echo "rootfs missing — run scripts/build-rootfs.sh"; exit 1;
}

rm -f SHA256SUMS
for f in uefi_whyred.img boot_pve_whyred.img pve_rootfs_arm64.img pve_rootfs_arm64.sparse.img Image.gz-whyred; do
    [ -f "$f" ] && shasum -a 256 "$f" >> SHA256SUMS
done

echo "── dist/ ─────────────────────────────"
ls -la | awk 'NR>3 {printf "%12s  %s\n", $5, $9}'
echo "──────────────────────────────────────"
cat SHA256SUMS
