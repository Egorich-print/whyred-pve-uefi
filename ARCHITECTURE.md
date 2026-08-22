# Architecture

## Boot chain

```
Power → XBL (Qualcomm, immutable) → ABL (aboot)
  ABL loads `boot` partition as Android boot.img:
    ┌───────────── Plan A: UEFI ─────────────┐   ┌── Plan B: direct kernel ──┐
    │ payload = edk2-msm UEFI FD             │   │ payload = Image.gz+DTB    │
    │ SOC_PLATFORM=SDM660, header v1         │   │ mainline sdm660-mainline  │
    │ SimpleInit GOP over XBL framebuffer    │   │ root=PARTLABEL=userdata   │
    │ exposes EFI env / boot manager         │   │ boots PVE directly        │
    └────────────────────────────────────────┘   └───────────────────────────┘
                    │                                          │
        ext4 driver → /boot/extlinux/extlinux.conf     console=ttyMSM0
                    ▼                                          ▼
              Linux mainline + Proxmox VE arm64 (official repo, trixie)
```

Plan B exists because bring-up order matters: ABL is a known-good loader;
UEFI is the experimental layer. Both consume the SAME rootfs image.

## Repository layout

| Path | Role |
|------|------|
| `tools/bootimg-rs` | lib+CLI: Android boot.img v0–v3 codec. BE headers, page-aligned sections |
| `tools/sparse-rs` | libsparse chunk codec; used to sparse-ify the 8GiB rootfs for fastboot |
| `tools/payload-packer` | FD/kernel → whyred boot.img (v1, pagesize 4096, base 0x0) |
| `edk2/vm-build-edk2.sh` | runs in Lima VM: edk2-msm CI deps + `./build.sh --device whyred --boot` |
| `pve/vm-build-rootfs.sh` | debootstrap trixie + official proxmox arm64 repo + kernel build + ext4 image |
| `pve/kernel-config.fragment` | container/PVE kconfig on top of defconfig |
| `apps/unlocker/` | Tauri v2 + Svelte-less minimal UI; `mibox-core` = fastboot protocol over rusb |
| `scripts/build-{edk2,rootfs}.sh` | host wrappers: sync script → run in VM → pull artifacts → pack |

## Memory & hardware facts (verified against sources)

- DRAM @ 0x80000000; kernel phys load 0x80008000 (`base=0x0, offset=0x8000`)
- GIC 0x17a00000 · UART console blsp1_uart2 **0xc170000** · eMMC sdhc_1 0xc0c4000
- USB3 DWC3 0xa800000 (`androidboot.usbcontroller=a800000.dwc3`)
- XBL-inited framebuffer 1080×2160 stride 4320 handed to simple-framebuffer
- boot partition 64 MiB — payload-packer refuses larger images

## Why these choices

- **Rust tools native on macOS** instead of porting mkbootimg/simg2img: no
  Homebrew formula covers them with our header-version needs; each is <500 LoC.
- **Lima Ubuntu VM for builds** instead of cross-compiling from macOS: edk2-msm
  CI itself targets Ubuntu; byte-for-byte same dependency set.
- **Official Proxmox arm64 repo** (2026-08-05 launch): removes the entire
  community-port maintenance burden that apqa.cn/jiangcuo repos carried.
- **Dual boot plan**: see above; de-risks UEFI-first flashing.

## Known limits

- EDK2 display driver relies on XBL having initialized the panel (standard
  renegade approach); if panel init differs on your unit, serial debug `-u`
  builds are the fallback.
- Xiaomi unlock authorization is server-signed; `MiToolbox-Native` surfaces
  fastboot-visible state but cannot forge tokens (see FLASHING_GUIDE.md).
