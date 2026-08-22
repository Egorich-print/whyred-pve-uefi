# whyred — eMMC GPT partition map

Sources: LineageOS `device/xiaomi/sdm660-common/BoardConfigCommon.mk` +
`device/xiaomi/whyred/BoardConfig.mk` (lineage-18.1), edk2-msm
`configs/devices/whyred.conf`, MIUI fastboot ROM layout. Sizes in bytes.

| Partition  | Size | Notes |
|------------|------:|-------|
| `xbl`, `xbl_config` | ~3.5 MiB / ~64 KiB | Qualcomm bootloader (do not touch) |
| `sbl1`     | 512 KiB | |
| `rpm`, `tz`, `hyp`, `keymaster`, `cmnlib`, `cmnlib64`, `devcfg`, `abl` | ≤2 MiB each | firmware; `abl` = Android bootloader that loads our payload |
| `modem`    | ~80 MiB | baseband |
| `bluetooth`,`dsp` | ~1–4 MiB | wcn3990 bt, adsp |
| `persist`  | 32 MiB | sensors calib |
| `splash`   | ~20 MiB | MIUI logo |
| `misc`     | 1 MiB | bootloader msgs |
| `cust`     | ~570 MiB | MIUI carrier/customization |
| `recovery` | 67108864 (0x04000000) = 64 MiB | |
| **`boot`** | **67108864 (0x04000000) = 64 MiB** | **target for UEFI payload** |
| `system`   | 3221225472 = 3072 MiB | |
| `vendor`   | 838860800 = 800 MiB | |
| `cache`    | 268435456 = 256 MiB | reuse candidate |
| `userdata` | remainder (~52 GiB on 64 GB model) | **target for PVE rootfs + LXC storage** |

No `dtbo` partition on whyred (A-only MIUI device); no A/B slots.
No dynamic partitions (`super`) — direct GPT.

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

## Safety rules

1. Before any flash, dump actual GPT from the device:
   `adb shell ls -la /dev/block/bootdevice/by-name > gpt-listing.txt`
2. Never write outside `boot`, `cache`, `recovery`, `userdata`.
3. Keep a stock MIUI fastboot ROM available for unbrick via EDL (test point)
   — whyred is EDL-recoverable with authorized programmer or MiFlash.
