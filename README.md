# whyred-pve-uefi

Proxmox VE (ARM64) on **Xiaomi SDM636/SDM660 phones**, targeting a
Tianocore EDK2 UEFI boot path (edk2-msm / Renegade Project port). No UEFI
payload has booted on either device yet — see `STATUS.md`.

| Device | Codename | SoC | State |
|--------|----------|-----|-------|
| Redmi Note 5 Pro | `whyred` | SDM636 | bootloader locked (2026-08-23) |
| Redmi Note 7 | `lavender` | SDM660 | unlocked, stuck at splash, reachable via EDL test points |

Shared rootfs image; per-device UEFI payload and kernel boot images.
Device serials are deliberately not published — see STATUS.md.

```
XBL/ABL (Qualcomm) ──▶ Plan B: mainline Linux kernel (sdm660-mainline) + PVE rootfs
                       └▶ Plan A: UEFI payload (edk2-msm, SOC=SDM660) → extlinux → Linux
                                              └─▶ LXC containers (official Proxmox VE arm64)
```

## Layout

| Path        | What |
|-------------|------|
| `tools/bootimg-rs`     | Android boot.img v0–v4 parse/unpack/pack (Rust 2024, macOS-native) |
| `tools/sparse-rs`      | Android sparse image ⇄ raw converter (`simg2img` / `img2simg`) |
| `tools/payload-packer` | Wrap UEFI FD / kernel payloads into flashable boot.img |
| `tools/sahara-rs`      | Qualcomm Sahara v2 loader upload over EDL (unit-tested state machine) |
| `tools/edl-recon.py`   | Same upload via pyusb (the path that works on this macOS host) |
| `tools/analyze-devinfo.py` | Read-only survey of a devinfo dump |
| `tools/test_tools.py` | Unit tests for the EDL state machines and the analyser |
| `edk2/`                | Lima VM pipeline for `edk2-msm -d <device>` + the lavender port |
| `pve/`                 | Debian trixie ARM64 + official Proxmox VE arm64 rootfs generator |
| `apps/unlocker/`       | MiToolbox-Native: Tauri v2 fastboot toolbox (boot/cache/recovery only) |
| `scripts/`             | Device-parameterized builders, dist manifest |
| `dist/`                | Build outputs + SHA256SUMS (kernels tracked, large images not) |
| `docs/`                | Partition map, memory map, unlock research, `docs/adr/`, `docs/AUDIT-2026-09-25.md` |
| `STATUS.md`            | **Single source of truth** for status and blockers |

## Build (host, no device needed)

```sh
export PROXMOX_KEY_FPR=<release-key fingerprint; see pve/proxmox-release-key.fpr>
scripts/build-edk2.sh   pve-builder whyred      # → dist/uefi_whyred.img
scripts/build-rootfs.sh pve-builder whyred      # kernel + rootfs + boot_pve + sparse
scripts/make-dist.sh                            # regenerate dist/SHA256SUMS
scripts/check.sh                                # fmt/clippy/tests + gates + manifest
scripts/clean.sh [--dist]                       # drop caches (and build images)
```

Requires Rust 1.98+, Lima (`limactl`), and ~40 GB free in the VM.
The rootfs tree is rebuilt from scratch unless it carries the `.pve-build`
marker, so a polluted or half-installed chroot can never be reused silently.
Only `Image.gz-*` and `dist/SHA256SUMS` are tracked in git; every other
artifact is reproduced by the commands above and removed again by
`scripts/clean.sh --dist` (which rewrites the manifest for the files that
remain). `flash_all.sh` refuses to flash unless the manifest matches.

Before flashing, any boot image can be checked on its own:

```sh
cargo run --release -p bootimg-rs --manifest-path tools/Cargo.toml -- \
    validate --profile kernel dist/boot_pve_whyred.img
```

## Flash (manual, device owner only)

```sh
./flash_all.sh --check                # artifact + manifest verification
DEVICE=whyred ./flash_all.sh          # Plan B: kernel + rootfs
PLAN=uefi  DEVICE=whyred ./flash_all.sh   # Plan A, after Plan B is proven
```

The script verifies `getvar product`/`unlocked`/serial, re-checks the
manifest before the first write, and asks for a typed confirmation. It never
erases `misc` and never writes `devinfo`. See `FLASHING_GUIDE.md`.

## Further reading

| Document | What |
|----------|------|
| `STATUS.md` | current state, blockers, next steps (single source of truth) |
| `ARCHITECTURE.md` | layout, build rationale, known limits |
| `FLASHING_GUIDE.md` | operator runbook (backup → unlock → flash → first boot) |
| `docs/01-partitions.md` | measured GPT, boot-image parameters, write policy |
| `docs/02-memory-map.md` | DRAM/GIC/UART/eMMC map and the 0x40000000 base |
| `docs/lavender.md` | SDM660 sibling: edk2-msm port, deltas, EDL status |
| `docs/reverse-unlock.md` | EDL/devinfo unlock research and safety rails |
| `docs/AUDIT-2026-09-25.md` | full-project review: findings, fixes, evidence |
| `docs/CLEANUP-2026-09-26.md` | what was removed from the tree and why |
| `REPORT.md` | mission report of the original 2026-08-23 build (historical) |
| `docs/adr/` | ADR-001…006: the decisions behind all of the above |

## Decisions

`docs/adr/` — dual boot path, lavender edk2-msm port, official Proxmox arm64,
shared SDM660 rootfs, bootimg endianness, EDL/devinfo unlock route.

License: MIT.
