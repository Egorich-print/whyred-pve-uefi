# whyred-pve-uefi

Proxmox VE (ARM64) on **Xiaomi Redmi Note 5 Pro (`whyred`, SDM636)** booted via
Tianocore EDK2 UEFI (edk2-msm / Renegade Project port).

```
XBL/ABL (Qualcomm) ──▶ UEFI payload in boot partition (edk2-msm, SOC=SDM660)
                       │
                       ├─▶ Linux mainline (sdm660-mainline) + PVE rootfs on userdata
                       └─▶ LXC containers (PVE ARM64 community port)
```

## Layout

| Path        | What |
|-------------|------|
| `tools/bootimg-rs`     | Android boot.img v0–v3 parse/unpack/pack (Rust 2024, macOS-native) |
| `tools/sparse-rs`      | Android sparse image ⇄ raw converter (`simg2img` / `img2simg`) |
| `tools/payload-packer` | Wrap UEFI FD / kernel payloads into flashable boot.img |
| `edk2/`                | Containerized build pipeline for `edk2-porting/edk2-msm -d whyred` |
| `pve/`                 | Debian 12 ARM64 + Proxmox VE ARM64 rootfs generator + kernel fragment |
| `apps/unlocker/`       | MiToolbox-Native: Tauri v2 + Svelte fastboot/USB toolbox (rusb) |
| `scripts/`             | One-command builders, dist assembly, SHA256 manifest |
| `dist/`                | Final artifacts + SHA256SUMS |
| `docs/`                | Partition map, memory map, research notes |

## Status

- [x] Phase 1 — recon & hardware extraction (`docs/`)
- [x] Phase 2 — host tooling in Rust (tested on macOS aarch64)
- [x] Phase 3 — EDK2 pipeline **VERIFIED**: `dist/boot-whyred.img` built via edk2-msm `-d whyred` in Lima VM
- [x] Phase 4 — PVE ARM64 rootfs pipeline (`pve/mkrootfs.sh`)
- [x] Phase 5 — MiToolbox-Native app scaffold with fastboot protocol impl
- [x] Phase 6 — packaging, guides, SHA256 manifest

**Nothing here touches the device.** All flashing steps are documented for the
human operator in `FLASHING_GUIDE.md`; the agent stops before `fastboot flash`.

## Build everything (host, no device needed)

```sh
scripts/build-all.sh     # rust tools + edk2 payload + rootfs image → dist/
```

Requires: Rust 1.98+, podman (or Lima), ~10 GB disk. See `ARCHITECTURE.md`.

## Device reference

- Codename: `whyred` · SoC: SDM636 · S/N: 19680/68UA04603
- Boot chain: Qualcomm XBL → ABL → EDK2 UEFI payload (`boot` partition)
- Mainline status: official postmarketOS device `xiaomi-whyred`,
  DT `sdm636-xiaomi-whyred.dts` (sdm660-mainline/linux)

License: MIT.
