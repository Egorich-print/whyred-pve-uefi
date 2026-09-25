#!/usr/bin/env bash
# Regenerate dist/SHA256SUMS from the artifacts that are actually present.
# Fails closed: the core whyred set must exist, and the manifest is written
# atomically so a failed run never leaves a manifest describing a partial set.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO/dist"

# what the operator actually flashes; the raw rootfs is an intermediate and
# is hashed only when it happens to be present (it is 8 GiB)
REQUIRED=(uefi_whyred.img boot_pve_whyred.img pve_rootfs_arm64.sparse.img)
for f in "${REQUIRED[@]}"; do
    [ -s "$f" ] || { echo "missing or empty: dist/$f — run scripts/build-edk2.sh and scripts/build-rootfs.sh"; exit 1; }
done

ARTIFACTS=()
for f in "${REQUIRED[@]}" Image.gz-whyred Image.gz-lavender uefi_lavender.img \
         boot_pve_lavender.img pve_rootfs_arm64.img; do
    [ -s "$f" ] && ARTIFACTS+=("$f")
done

TMP=$(mktemp ./SHA256SUMS.XXXXXX)
trap 'rm -f "$TMP"' EXIT
for f in "${ARTIFACTS[@]}"; do
    shasum -a 256 "$f" >> "$TMP"
done
mv "$TMP" SHA256SUMS
trap - EXIT

echo "── dist/ ─────────────────────────────"
ls -la | awk 'NR>3 {printf "%12s  %s\n", $5, $9}'
echo "──────────────────────────────────────"
cat SHA256SUMS
