# STATUS — whyred-pve-uefi

> Обновлено: 2026-08-23 · Слой: Execution · Статус: Active (артефакты готовы, прошивка блокирована)

## Что готово

| Компонент | Состояние | Пруф |
|-----------|-----------|------|
| Rust-тулинг | ✅ 7/7 тестов | `tools/` (bootimg-rs, sparse-rs, payload-packer) |
| UEFI payload whyred | ✅ собран | `dist/uefi_whyred.img` (6.3 MB, edk2-msm `-d whyred`) |
| UEFI payload lavender | ✅ собран после написания порта | `dist/uefi_lavender.img` |
| Ядра mainline | ✅ собраны (sdm660-mainline, 7.0.14) | `dist/Image.gz-{whyred,lavender}` |
| PVE rootfs arm64 | ✅ 8 GiB ext4 / 4.76 GB sparse | официальный репозиторий Proxmox (trixie) |
| MiToolbox-Native | ✅ компилируется (Tauri v2) | `apps/unlocker/` |

## Что блокирует

1. **whyred залочен** — `flashing unlock`/`oem unlock` отклонены
   (`Token Verify Failed`, нужна серверная подпись Xiaomi). Mi Unlock
   требует Windows либо EDL-патч `devinfo` (см. ADR-006).
2. **lavender** — bootloader разблокирован, но зависает на splash
   (phantom Volume Up), fastboot через кнопки недоступен; EDL по тестпоинтам
   работает нестабильно (Sahara v2 поднят, firehose — нет).
3. Крупные `.img`-артефакты (rootfs 8 GiB, sparse 4.76 GB) не хранятся в git —
   воспроизводятся `scripts/build-{edk2,rootfs}.sh`; их SHA256 сохранены в
   `dist/SHA256SUMS`.

## Следующий шаг

- whyred: разборка → EDL test points → чтение `devinfo` → патч (ADR-006).
- lavender: BCB `bootonce-bootloader` в `misc` → fastboot → TWRP → `dd devinfo`.
- Альтернатива: Windows-VM (UTM) + официальный Mi Unlock Tool.
