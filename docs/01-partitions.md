# whyred — eMMC GPT partition map

> **MEASURED ON LIVE DEVICE 2026-08-23** (`fastboot getvar all`, see
> exp002-getvar-all.txt and Vivanta docs/hardware/whyred/partitions.md).
> Estimates below kept for provenance; authoritative = measured table.

Measured highlights: boot 0x4000000 · recovery 0x4000000 · cache 0x10000000 ·
system 0xC0000000 · **vendor 0x80000000 (2 GiB!)** · cust 0x34000000 (832 MiB) ·
splash 0x4000000 · persist±bak 0x2000000 · modem 0xC000000 · dsp 0x1000000 ·
xbl±bak 0x380000 · misc 0x400000 · **userdata 0xCD77F7E00 = 54.94 GiB ext4**.
No dtbo partition; no A/B slots; eMMC variant "SDM EMMC".

## Historical estimates (pre-survey)

Sources: LineageOS sdm660-common BoardConfigCommon.mk, edk2-msm
whyred.conf, MIUI fastboot ROM layout. Vendor/cust/splash were
underestimated here — see measured block above.

## Boot image parameters (stock/ABL expectations)

Authoritative values — postmarketOS `device/testing/device-xiaomi-whyred/deviceinfo`
(+ LineageOS sdm660-common BoardConfigCommon.mk, edk2-msm `whyred.conf`):

```
pagesize            4096
base                0x00000000
kernel offset       0x00008000   => phys load 0x80008000
ramdisk offset      0x01000000
second offset       0x00000000
tags offset         0x00000100
header version      1 (edk2-msm whyred.conf) / 0 (pmos) — ABL accepts both
os version          11.0.0, patch level 2020-12
cmdline             androidboot.hardware=qcom user_debug=31 msm_rtb.filter=0x37
                    ehci-hcd.park=3 lpm_levels.sleep_disabled=1
                    sched_enable_hmp=1 sched_enable_power_aware=1
                    service_locator.enable=1 swiotlb=1
                    androidboot.configfs=true androidboot.usbcontroller=a800000.dwc3
                    loop.max_part=7 usbcore.autosuspend=7
dtb                 qcom/sdm636-xiaomi-whyred (appended to kernel, append_dtb=true)
```

> **Endianness gotcha (verified 2026-08-23):** edk2-msm packs its boot image
> with Ubuntu `abootimg`, which writes header fields LITTLE-endian
> (pages 2048), unlike AOSP `mkbootimg` (big-endian). whyred ABL accepts
> both. `bootimg-rs` auto-detects. Real artifact: pagesize 2048,
> "kernel" = UEFI FD 6333313 B, ramdisk = 1 byte.

## Safety rules

1. Bootloader is LOCKED (`unlocked:no`, anti-rollback v4) as of 2026-08-23 —
   Mi Unlock is a hard prerequisite for any flash.
2. Never send undocumented `oem *` commands to this ABL: `oem device-info`
   WEDGES the fastboot handler until physical reboot (EXP-002 §incident).
3. Never write outside `boot`, `cache`, `recovery`, `userdata`.
4. Keep a stock MIUI fastboot ROM for EDL unbrick insurance.
