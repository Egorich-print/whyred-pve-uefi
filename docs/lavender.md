# xiaomi-lavender (Redmi Note 7) — SDM660 family support

Device identity (measured, kept out of the public repo): hwversion 1.29.0
Global, panel **tianma** NT36672A (confirmed by the successful
lavender-flasher build).
Bootloader state at last survey: **unlocked** (`verifiedbootstate=orange`,
AVB 1.0 lenient) — see Vivanta `docs/hardware/lavender/EXP-001.md`.

## What was added

| Piece | Detail |
|-------|--------|
| `edk2-msm` device port | `configs/devices/lavender.conf` + `Platform/Xiaomi/sdm660/lavender.{dsc,fdf.inc}` + `FdtBlob_compat/lavender.dtb` (GUID 827309bb-7075-45e0-a62b-6af3e30a11a4), panel 1080×2340 |
| UEFI payload | `dist/uefi_lavender.img` — built by `scripts/build-edk2.sh pve-builder lavender` (upstream `build.sh --device lavender --boot` after the port is applied); build verified, **never booted on the device** |
| PVE kernel | same sdm660-mainline tree; DTB variant `sdm660-xiaomi-lavender-tianma`; `dist/boot_pve_lavender.img` (produced by `scripts/build-rootfs.sh pve-builder lavender`) |
| Rootfs | **shared** with whyred — one 8 GiB ext4 image serves both devices |
| Flasher | `DEVICE=lavender ./flash_all.sh` (verifies `getvar product` = lavender) |

## Boot parameters (pmaports + live cmdline, identical offsets to whyred)

```
pagesize 4096 · base 0x0 · kernel +0x8000 · ramdisk +0x1000000
second 0 (no second stage) · tags +0x100 · header v1 (ABL accepts abootimg LE too)
console=ttyMSM0,115200n8   (earlycon=qcom_geni,0xc170000 in the generated extlinux)
```

## Memory map deltas vs stock assumptions

DRAM base **0x40000000** (bank0 1.5 GiB @0x40000000 + bank1 @0xA0000000),
kernel code lands ~0x40080000 after decompress — NOT 0x80000000.
This also corrects our earlier whyred note: SDM660 family DRAM starts at
0x40000000; the 0x85xxxxxx reserved regions sit inside bank0.

DTBO partition present (A/B naming differs per boot: `mmcblk0p52` on stock,
`mmcblk1p*` under the running Linux userspace — resolve by GPT, not by number);
our images bypass it via appended-DTB kernels and UEFI's own FDT handling.

## EXP-001 follow-ups closed by this work

| Open item | Resolution |
|-----------|------------|
| A-003 ABL load address | irrelevant for boot.img path: ABL relocates per platform table; header offset stays 0x8000-relative |
| R5 unsigned boot | orange state confirmed; `fastboot flash boot` path used instead of `fastboot boot` |
| R7 header integrity | the whyred boot images were built and validated offline; ABL acceptance is still unverified on both devices |
