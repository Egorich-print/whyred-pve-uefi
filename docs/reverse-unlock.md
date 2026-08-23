# Reverse-engineering the bootloader unlock — whyred (no Windows / no Mi Unlock)

Status: **tooling staged, awaiting device in fresh fastboot** (2026-08-23).

## Threat model (why this is feasible)

MiUL unlock = Xiaomi server signs a request bound to the device token
(`VQEBIAEQ…` captured in EXP-002); ABL verifies an RSA signature against a
baked-in OEM public key. Forging that signature is out of scope forever.
BUT the lock itself is enforced by ABL reading a flag — and ABL is not the
only writer of eMMC. Qualcomm's **EDL (Emergency Download, USB PID 0x9008)**
runs before ABL and, given a firehose programmer accepted by PBL, reads and
writes raw partitions with no MIUI involvement.

## Assets acquired

| File | What |
|------|------|
| `Tools/edl/` | bkerler/edl client (venv at `Tools/edl/venv-edl`) |
| `loaders-local/jasmine_prog_emmc_firehose_Sdm660_ddr.elf` | Xiaomi-signed SDM660 firehose (jasmine/Mi A2 — same OEM cert family as whyred; proven to load on whyred-class devices per martview logs) |
| `loaders-local/000cc0e100000000_7be49b72f9e43372_fhprg_bqx2_peek.bin` | Qualcomm factory loader matching our exact Sahara HW ID `000cc0e1` (SDM636) — fallback if OEM-signed one is rejected |

## Entry path to EDL

1. From responsive fastboot: `fastboot oem edl` — documented verb for
   SDM636/660 MIUI ABLs (xiaomi-miui.gr unbrick guide). Wedge risk accepted:
   force-reboot recovery is proven cheap on this unit now.
2. Fallback (physical): test points under back cover / deep-flash cable.

macOS detection: VID 05c6 PID 9008 (`system_profiler SPUSBDataType`).

## Recon sequence (READ-ONLY, no device mutation)

```sh
cd ~/ai-workstation/Tools/edl
L=loaders-local/jasmine_prog_emmc_firehose_Sdm660_ddr.elf
./venv-edl/bin/python edl.py printgpt --loader=$L        # full GPT w/ sector offsets
./venv-edl/bin/python edl.py r devinfo /tmp/devinfo.bin --loader=$L
./venv-edl/bin/python edl.py r misc    /tmp/misc.bin    --loader=$L
hexdump -C /tmp/devinfo.bin | head -80                  # find "is_unlocked" struct
```

## Unlock hypothesis (to verify empirically)

MIUI ABL of this era stores its lock state in the **devinfo** partition
(0x800000 bytes; present in GPT dump). Mi Unlock Tool ultimately flips a
field there (plus userdata wipe). If we find a plaintext flag structure
(`unlocked`, `IsUnlock`, magic+version+bool), the patch is:

1. BACKUP original devinfo (+ boot, recovery) via EDL
2. Flip flag byte(s), keep everything else byte-identical
3. `edl wf devinfo.patched.bin` (write partition)
4. Reboot → `fastboot getvar unlocked` must say `yes`; boot shows orange state

If devinfo turns out to be hash-chained or stored elsewhere (misc/fsc),
fall back to: full stock ROM restore via EDL (unbrick path doubles as
research platform), or Win11-ARM VM running x64 Mi Unlock under emulation
(slow but known to work in UTM/QEMU).

## Safety rails

- Every read happens before any write; originals kept in `dist/backups/`
- Writes limited to devinfo (single 8 MiB partition, no bootloader chain touched)
- Worst case: EDL remains available for stock restore — same tool, same loader
