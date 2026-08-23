#!/bin/bash
# Adds xiaomi-lavender device port to a cloned edk2-msm tree and builds it.
# Modeled on Platform/Xiaomi/sdm660/whyred.* — SDM660 silicon is shared.
set -euxo pipefail

cd "$HOME/edk2-msm"

# ---- 1. device config (mirrors whyred.conf) ----
cat > configs/devices/lavender.conf <<'EOF'
SOC_PLATFORM="SDM660"
VENDOR_NAME="Xiaomi"
PLATFORM_NAME="lavender"

# mkbootimg config
BOOTIMG_OS_PATCH_LEVEL="2020-12"
BOOTIMG_OS_VERSION=11.0.0
BOOTIMG_HEADER_VERSION=1
EOF

# ---- 2. platform DSC (tianma panel: 1080x2340) ----
cat > Platform/Xiaomi/sdm660/lavender.dsc <<'EOF'
[Defines]
  VENDOR_NAME                    = Xiaomi
  PLATFORM_NAME                  = lavender
  PLATFORM_GUID                  = 827309bb-7075-45e0-a62b-6af3e30a11a4
  PLATFORM_VERSION               = 0.1
  DSC_SPECIFICATION              = 0x00010019
  OUTPUT_DIRECTORY               = Build/$(PLATFORM_NAME)
  SUPPORTED_ARCHITECTURES        = AARCH64
  BUILD_TARGETS                  = DEBUG|RELEASE
  SKUID_IDENTIFIER               = DEFAULT
  FLASH_DEFINITION               = Platform/Qualcomm/sdm660/sdm660.fdf
  DEVICE_DXE_FV_COMPONENTS       = Platform/Xiaomi/sdm660/lavender.fdf.inc

!include Platform/Qualcomm/sdm660/sdm660.dsc

[BuildOptions.common]
  GCC:*_*_AARCH64_CC_FLAGS = -DENABLE_SIMPLE_INIT

[PcdsFixedAtBuild.common]
  gQcomTokenSpaceGuid.PcdMipiFrameBufferWidth|1080
  gQcomTokenSpaceGuid.PcdMipiFrameBufferHeight|2340

  # Simple Init
  gSimpleInitTokenSpaceGuid.PcdGuiDefaultDPI|350

  gRenegadePkgTokenSpaceGuid.PcdDeviceVendor|"Redmi"
  gRenegadePkgTokenSpaceGuid.PcdDeviceProduct|"Note7"
  gRenegadePkgTokenSpaceGuid.PcdDeviceCodeName|"lavender"
EOF

# ---- 3. per-device FDF include (same as whyred: shared ACPI + DTB) ----
sed -e 's/whyred/lavender/g' Platform/Xiaomi/sdm660/whyred.fdf.inc \
    > Platform/Xiaomi/sdm660/lavender.fdf.inc || true
grep -q 'FdtBlob_compat/lavender.dtb' Platform/Xiaomi/sdm660/lavender.fdf.inc ||
    sed -i 's/whyred\.dtb/lavender.dtb/' Platform/Xiaomi/sdm660/lavender.fdf.inc

# ---- 4. DTB: mainline tianma variant from our kernel build ----
LINUX="$HOME/rootfs-build/linux"
[ -f "$LINUX/arch/arm64/boot/dts/qcom/sdm660-xiaomi-lavender-tianma.dtb" ] || {
    cd "$LINUX" && make -j"$(nproc)" dtbs && cd -
}
cp "$LINUX/arch/arm64/boot/dts/qcom/sdm660-xiaomi-lavender-tianma.dtb" \
   Platform/Xiaomi/sdm660/FdtBlob_compat/lavender.dtb

# ---- 5. build ----
mkdir -p "$HOME/edk2-out"
./build.sh --device lavender --boot -u -O "$HOME/edk2-out" ||
    ./build.sh --device lavender --boot -u --toolchain GCC5 -O "$HOME/edk2-out"

echo "=== OUTPUT ==="
ls -la "$HOME/edk2-out"
