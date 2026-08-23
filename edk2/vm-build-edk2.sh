#!/bin/bash
# Runs INSIDE the Lima VM (pve-builder, Ubuntu arm64).
# DEVICE=whyred|lavender (default whyred)
# Installs edk2-msm CI dependency set, clones the tree, builds UEFI payload
# for whyred as a fastboot boot image.
set -euxo pipefail

export DEBIAN_FRONTEND=noninteractive
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
    build-essential uuid-dev clang llvm iasl nasm \
    gcc-aarch64-linux-gnu abootimg python3-pil python3-git gettext \
    git ca-certificates curl xz-utils

DEVICE="${DEVICE:-whyred}"

cd "$HOME"
if [ ! -d edk2-msm/.git ]; then
    git clone --recursive https://github.com/edk2-porting/edk2-msm.git
else
    cd edk2-msm && git pull --ff-only && git submodule update --init --recursive && cd ..
fi

cd "$HOME/edk2-msm"
mkdir -p "$HOME/edk2-out"
# CLANG38 default; fall back to GCC5 (cross-prefixed) on failure.
./build.sh --device "$DEVICE" --boot -u -O "$HOME/edk2-out" ||
    ./build.sh --device "$DEVICE" --boot -u --toolchain GCC5 -O "$HOME/edk2-out"

echo "=== OUTPUT ==="
ls -la "$HOME/edk2-out"
