# EXP-003 — lavender EDL session: what was measured (2026-08-23)

> Companion to EXP-002 (whyred `fastboot getvar all`, `exp002-getvar-all.txt`).
> Device serials and cpuid are kept out of the public repo; this file records
> the protocol-level facts only, so the EDL work can be resumed without the
> phone being plugged in again.

## Context

`lavender` (Redmi Note 7, SDM660) is unlocked but never reaches fastboot: the
stock ABL wedges on the splash screen and the phantom Volume Up event means
button entry does not work. The physical route is EDL 9008 via the test-point
pads under the SIM shield, driven from a macOS host.

## Confirmed

| Fact | Value |
|------|-------|
| EDL enumeration | VID `05c6` PID `9008`, product string `QUSB__BULK` |
| Transport that works here | **pyusb** — `rusb` does not enumerate this interface on this macOS host (hence `tools/edl-recon.py` next to `tools/sahara-rs`) |
| Sahara HELLO | received; v2 protocol, mode 0 (image tx pending) |
| HELLO timing | only after a **full power cycle** with the test point released; a plain USB reconnect re-presents the interface but sends no HELLO |
| Loader upload | succeeded repeatedly with the Xiaomi-signed SDM660 firehose (`jasmine_prog_emmc_firehose_Sdm660_ddr.elf`, 629,136 B) and with the Qualcomm HWID-matched `fhprg_bqx2_peek.bin` (634,384 B) |
| Post-upload | device did **not** re-enumerate within the session; the handle stopped answering |

## Not confirmed

- Firehose `configure`/`nop`/`read` never completed. The last observed reply to
  a `<read>` was a 16-byte NAK:
  `04 00 00 00 10 00 00 00 0d 00 00 00 01 00 00 00`
  (version 4, mode 0, status `0x0d` = NAK "invalid command in current state",
  NAK code 1) — i.e. the loader had not reached the Firehose XML state.
- `devinfo` has never been read on this device. Everything downstream of it
  (lock flag, patch offsets) remains unverified.

## Why the current tooling looks the way it does

- Both uploaders require the **terminal acknowledgement** (`DONE_RSP` with
  status 0, or `RESET_RSP`) before reporting success — the previous code
  reported "loader uploaded" after a `DONE` it never validated, which is how
  a stalled session can masquerade as a success.
- Firehose reads are delegated to `~/ai-workstation/Tools/edl`
  (bkerler/edl) instead of a second hand-rolled implementation in this repo:
  partition-aware, tested against a real device, and it can dump GPT, so the
  `devinfo` LBA comes from the partition table rather than a guessed sector.
- The project-side scripts never issue `<program>` or `<erase>`: everything
  they do is read-only by construction.

## Resume recipe

1. Power-cycle into EDL (test point → hold POWER ~10 s → release → plug USB).
2. `tools/edl-recon.py --loader ~/ai-workstation/Tools/edl/loaders-local/jasmine_prog_emmc_firehose_Sdm660_ddr.elf`
   — must end with `loader accepted`.
3. Wait for re-enumeration (`system_profiler SPUSBDataType`); the PID typically
   changes to `05c6:900e`.
4. `cd ~/ai-workstation/Tools/edl && ./venv-edl/bin/edl r gpt`, then
   `./venv-edl/bin/edl r devinfo devinfo.bin` (8 MiB per `docs/exp002-getvar-all.txt`).
5. `tools/analyze-devinfo.py devinfo.bin` — observations only; any patch needs
   the LK `device_info` layout and a stock dump for comparison.
