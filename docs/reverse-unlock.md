# Reverse-engineering the bootloader unlock — whyred (no Windows / no Mi Unlock)

Status: **tooling staged, device-side steps still unproven** (last measurement
2026-08-23, Sahara v2 handshake + loader upload reached; Firehose reads never
completed).

## Why this is feasible at all

Mi Unlock = Xiaomi's server signs a request bound to a device token; ABL
verifies an RSA signature against a baked-in OEM public key. Forging that
signature is out of scope. BUT the lock state itself is a flag read by ABL —
and ABL is not the only writer of eMMC. Qualcomm **EDL** (USB PID 0x9008)
runs before ABL and, given a firehose programmer accepted by PBL, reads and
writes raw partitions with no MIUI involvement.

The captured device token (MiUL target material) is **redacted here** and kept
out of git; only its first bytes are shown for traceability.

## Assets

| File | What |
|------|------|
| `~/ai-workstation/Tools/edl/` | bkerler/edl client (venv at `venv-edl/`) — Firehose side |
| `tools/sahara-rs` | tested Sahara v2 uploader (rusb; use on hosts where rusb enumerates the device) |
| `tools/edl-recon.py` | same upload via pyusb — the path that works on this macOS host |
| `loaders-local/jasmine_prog_emmc_firehose_Sdm660_ddr.elf` | Xiaomi-signed SDM660 firehose |
| `loaders-local/000cc0e100000000_…_fhprg_bqx2_peek.bin` | Qualcomm factory loader matching Sahara HW ID `000cc0e1` (SDM636) |

A loader built for the other SoC can wedge EDL until the next power cycle —
always pass the loader explicitly, never let a tool guess it.

## Entry path to EDL

1. `fastboot oem edl` / `reboot-edl` / `adb reboot edl` are **not** available on
   this ABL (Invalid Parameter / unknown command; MIUI swallows the adb
   variant). Do not spend time on them.
2. Physical route: short the EDL test-point pads (dupont on the 0.8 mm pads
   under the SIM shield) until the unit enumerates as `05c6:9008`
   (`system_profiler SPUSBDataType` on macOS).
3. Sahara HELLO only arrives after a **full power cycle** with the test-point
   short removed: disconnect USB + short, hold POWER ~10 s, release, then
   plug USB in. A plain USB reconnect does not restart the state machine.

## Read-only recon

```sh
# 1. try the maintained client first
cd ~/ai-workstation/Tools/edl
L=loaders-local/jasmine_prog_emmc_firehose_Sdm660_ddr.elf
./venv-edl/bin/python edl.py printgpt --loader=$L

# 2. if its Sahara framing stalls, upload with our tested client, then read
cd -
./tools/edl-recon.py --loader ~/ai-workstation/Tools/edl/$L
cd ~/ai-workstation/Tools/edl
./venv-edl/bin/python edl.py r devinfo devinfo.bin     # partition-aware read
./venv-edl/bin/python edl.py r misc    misc.bin

# 3. survey the dump (prints observations, never patch offsets)
./tools/analyze-devinfo.py devinfo.bin
```

Reads resolve the partition through the GPT — do not hand-pick sector
numbers. `devinfo` is 8 MiB on this layout; a 4 KiB read is not a full dump.

## Unlock hypothesis (unverified)

MIUI ABL of this era stores lock state in the **devinfo** partition
(0x800000 bytes, present in the GPT dump). If a plaintext flag structure turns
up (`is_unlocked`, magic + version + bool), the patch would be:

1. Back up `devinfo` (and `boot`, `recovery`) with bkerler/edl into
   `~/ai-workstation/Projects/whyred-pve-uefi/backups/<date>/` — outside git.
2. Flip only the identified flag byte(s), keep the rest byte-identical.
3. Write it back **partition-aware**:
   `./venv-edl/bin/python edl.py w devinfo devinfo.patched.bin --loader=$L`
   ⚠️ `edl wf <file>` writes from **sector 0** and would destroy MBR/GPT —
   never use it here.
4. Reboot → `fastboot getvar unlocked` must say `yes`; the bootloader shows
   the orange state.

If devinfo turns out to be hash-chained or the flag lives elsewhere
(misc/fsc), fall back to a stock ROM restore via EDL, or a Windows/x64 Mi
Unlock under emulation (UTM/QEMU, slow but known to work).

## Safety rails

- Every read happens before any write; originals live in `backups/` (git-ignored).
- Writes limited to `devinfo` — one 8 MiB partition, no bootloader chain.
- `flash_all.sh` never erases `misc` and never writes `devinfo`.
- Worst case: EDL remains available for a stock restore — same client, same
  loader. This has not been tested on this unit; assume the first attempt can
  end in a re-enumerating device only, nothing worse.
