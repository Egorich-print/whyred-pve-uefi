#!/bin/bash
# Runs INSIDE the Lima VM (pve-builder, Ubuntu arm64).
# DEVICE=whyred|lavender (default whyred)
# Installs the edk2-msm build dependencies, fetches the tree and builds the
# UEFI payload as a boot image named edk2-out/uefi_<device>.img
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
    build-essential uuid-dev clang llvm iasl nasm \
    gcc-aarch64-linux-gnu abootimg python3-pil python3-git gettext \
    git ca-certificates curl xz-utils

DEVICE="${DEVICE:-whyred}"
case "$DEVICE" in
    whyred|lavender) ;;
    *) echo "unknown DEVICE=$DEVICE" >&2; exit 1;;
esac

cd "$HOME"
if [ ! -d edk2-msm/.git ]; then
    git clone --recursive https://github.com/edk2-porting/edk2-msm.git
else
    cd edk2-msm && git pull --ff-only && git submodule update --init --recursive && cd ..
fi

cd "$HOME/edk2-msm"
echo "[vm] edk2-msm commit: $(git rev-parse --short HEAD)"
rm -rf "$HOME/edk2-out"
mkdir -p "$HOME/edk2-out"

# CLANG38 default; fall back to GCC5 (cross-prefixed) on failure.
./build.sh --device "$DEVICE" --boot -u -O "$HOME/edk2-out" ||
    ./build.sh --device "$DEVICE" --boot -u --toolchain GCC5 -O "$HOME/edk2-out"

# Normalize the output name so the host wrapper has a stable contract.
SRC=""
for cand in "$HOME/edk2-out/uefi_$DEVICE.img" "$HOME/edk2-out/boot-$DEVICE.img" "$HOME/edk2-out/$DEVICE.img"; do
    [ -s "$cand" ] && SRC="$cand" && break
done
if [ -z "$SRC" ]; then
    echo "[vm] no boot image for $DEVICE in edk2-out:" >&2
    ls -la "$HOME/edk2-out" >&2
    exit 1
fi
cp "$SRC" "$HOME/edk2-out/uefi_$DEVICE.img"
echo "[vm] built $HOME/edk2-out/uefi_$DEVICE.img ($(wc -c <"$HOME/edk2-out/uefi_$DEVICE.img") bytes)"
