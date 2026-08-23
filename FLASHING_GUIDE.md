# FLASHING_GUIDE.md — whyred → Proxmox VE (read fully before touching the device)

> **Everything below is executed by YOU, the device owner.**
> The build pipeline stops at `dist/` artifacts; it never talks to hardware.

## 0. What you need

| Item | Why |
|------|-----|
| Xiaomi Redmi Note 5 Pro (`whyred`), bootloader **unlocked** | ⚠️ unit surveyed 2026-08-23 is LOCKED (`unlocked:no`, anti v4, USB serial `4699bca9`) — Mi Unlock required first (§1) |
| USB-A ↔ USB-C data cable | fastboot + RNDIS |
| `platform-tools` (adb/fastboot) on any host | flashing |
| Optional: UART 3.3V adapter @ BLSP1_UART2 pads (115200 8n1) | bring-up debug |
| Stock MIUI fastboot ROM for whyred | unbrick insurance (EDL/test point) |

## 1. Unlock the bootloader (one-time)

1. Add Mi account in Settings → My device → all specs → tap "MIUI version" 7×
   → Developer options → OEM unlocking + Mi Unlock status bound.
2. Run Xiaomi's **Mi Unlock Tool** on Windows with the same Mi account;
   wait out the binding period (168 h historically).
3. Verify lock state via `fastboot getvar unlocked` → must say `yes`.
   ⚠️ NEVER run `fastboot oem device-info` on this ABL — it wedges the
   fastboot handler until physical reboot (measured 2026-08-23, EXP-002).

`MiToolbox-Native` (`apps/unlocker/`) shows live unlock state and serial.
It cannot bypass server-signed authorization — by design.

## 2. Artifacts

```
dist/
├── uefi_whyred.img          # Plan A: EDK2 UEFI payload (flash → boot)
├── boot_pve_whyred.img      # Plan B: mainline kernel+DTB boot image (flash → boot)
├── pve_rootfs_arm64.img     # raw ext4 rootfs (Proxmox VE arm64)
├── pve_rootfs_arm64.sparse.img  # same, Android sparse — flash this one
└── SHA256SUMS
```

Verify first:

```sh
./flash_all.sh --check        # sha256 only, no device access
shasum -a 256 -c dist/SHA256SUMS
```

## 3. Flash sequence

```sh
adb reboot bootloader          # or VOL− + POWER from off state
./flash_all.sh                 # interactive confirm, then flashes boot + userdata
```

Manual equivalent (if you prefer explicit control):

```sh
fastboot getvar product                      # must say whyred
fastboot flash boot    dist/uefi_whyred.img
fastboot flash userdata dist/pve_rootfs_arm64.sparse.img
fastboot erase misc || true
fastboot reboot
```

**Rollback / unbrick**: re-flash stock MIUI fastboot ROM with MiFlash in EDL
mode (test point: short the EDL pad under the SIM shield) — always works.

## 4. First boot & access

| Path | Behavior |
|------|----------|
| Plan A (UEFI in `boot`) | SimpleInit/boot manager over panel fb → extlinux.conf → Linux |
| Plan B (`boot_pve_whyred.img`) | ABL boots kernel straight away |

- **Serial**: `screen /dev/tty.usbserial-* 115200` → login `root` /
  initial password `whyred` (CHANGE IMMEDIATELY).
- **USB RNDIS Web UI**: connect cable to your computer → new NIC appears →
  set host IP `10.15.0.1/24` → browse **https://10.15.0.254:8006**.
- eMMC remaining space beyond the 8 GiB rootfs image is unused until you grow
  the filesystem: `resize2fs /dev/mmcblk0p<userdata>` after `parted resize`.

## 5. Post-install PVE checklist

```sh
passwd                                    # rotate root password
pvecm create whyred-cluster               # optional clustering
pveam update && pveam available           # LXC templates (arm64)
lxc-checkconfig                           # all green = container-ready kernel
```

Create an LXC: Datacenter → storage `local` → templates → download
`debian-12-standard_*_arm64.tar.gz`… then CT create, unprivileged.

## 6. Troubleshooting

| Symptom | Action |
|---------|--------|
| No display, LED loop | UART console; try Plan B boot image |
| fastboot `FAILED (remote: size too large)` | rootfs sparse > userdata? rebuild smaller SIZE_MB |
| UEFI boots but no disk | edk2-msm sdm660 eMMC driver + HS400 timing — capture UART log, file issue upstream |
| RNDIS missing | check `lsmod g_ether`, `/etc/network/interfaces.d/usb0`, `dmesg \| grep dwc3` |
