# MISSION REPORT — whyred-pve-uefi

**Date:** 2026-08-23 · **Operator:** Egorich-print · **Device:** Redmi Note 5 Pro (`whyred`, S/N 19680/68UA04603)
**Boundary honored:** pipeline stopped at `dist/` artifacts — zero `fastboot` writes executed.

## Exit criteria status

| # | Criterion | Status |
|---|-----------|--------|
| 1 | Rust tools compiled & tested on macOS | ✅ 7/7 unit tests green (`tools/`) |
| 2 | EDK2 payload boot.img integrity | ✅ `boot-whyred.img` built by edk2-msm `-d whyred`, parsed by `bootimg-rs` |
| 3 | PVE ARM64 rootfs image | ✅ `pve/vm-build-rootfs.sh` → `pve_rootfs_arm64(.sparse).img` |
| 4 | GitHub repo, MIT, atomic history | ✅ https://github.com/Egorich-print/whyred-pve-uefi |
| 5 | Final report + SHA256 | ✅ this file + `dist/SHA256SUMS` |

## Key discoveries

1. **edk2-msm already ships `whyred.conf`** (SOC_PLATFORM=SDM660) — UEFI port
   existed upstream; no new platform code required.
2. **Proxmox VE went official-arm64 on 2026-08-05** (v9.2, trixie) — rootfs
   uses `download.proxmox.com/debian/pve trixie pve-no-subscription`.
3. **whyred is postmarketOS-mainline supported**: DT
   `sdm636-xiaomi-whyred.dts` merged into sdm660-mainline/linux; authoritative
   mkbootimg params from pmaports deviceinfo.
4. **abootimg writes LE headers** vs AOSP BE — `bootimg-rs` auto-detects;
   validated against the real Renegade artifact (pagesize 2048 quirk).
5. Dual-path boot de-risks bring-up: Plan B flashes mainline kernel directly
   through known-good ABL using the same rootfs.

## Artifacts (see dist/SHA256SUMS for hashes)

| Artifact | Purpose |
|----------|---------|
| `uefi_whyred.img` / `boot-whyred.img` | Plan A — UEFI payload → `fastboot flash boot` |
| `boot_pve_whyred.img` | Plan B — mainline kernel+DTB boot.img (ABL direct) |
| `pve_rootfs_arm64.sparse.img` | Proxmox VE arm64 rootfs → `fastboot flash userdata` |
| `Image.gz-whyred` | raw kernel for custom packing |

## Next steps (human operator)

1. Mi Unlock via official tool (FLASHING_GUIDE.md §1)
2. UART pads check (BLSP1_UART2 @115200) recommended before first flash
3. Flash Plan B first, verify serial console + RNDIS WebUI
4. Switch `boot` to Plan A (UEFI) when comfortable
