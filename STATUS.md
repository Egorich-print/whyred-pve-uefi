# STATUS — whyred-pve-uefi

> Обновлено: 2026-09-25 · Слой: Execution · Статус: Active ( tooling green, device work blocked )

## Что готово и проверено

| Компонент | Состояние | Чем проверено |
|-----------|-----------|---------------|
| Rust-тулинг | ✅ | `scripts/check.sh`: 19 тестов, clippy 0 warning, fmt clean |
| `bootimg-rs` | ✅ | v0–v4 парсинг, структурный детект endianness (регрессия на LE + кратный 256 размер) |
| `sparse-rs` | ✅ | round-trip + отказ на неполном покрытии/неизвестных chunk'ах/абсурдном размере |
| `payload-packer` | ✅ | пустой payload отклонён, лимит 64 MiB, `second_addr=0` как в документации |
| `sahara-rs` | ✅ unit-tested | автомат HELLO→READ_DATA→END→DONE_RSP; успех только при `DONE_RSP status=0` |
| `flash_all.sh` | ✅ fail-closed | product/serial/unlocked проверки, manifest до первой записи, `misc` не трогается |
| Сборка EDK2/rootfs | ⚠️ требует VM | скрипты параметризованы `DEVICE`, артефакты нормализуются, ошибки фатальны |
| Tauri-приложение | ✅ | `withGlobalTauri`, allowlist boot/cache/recovery, подтверждение серийником, `oem device-info` удалён |

## Артефакты

В git лежат только `dist/Image.gz-whyred`, `dist/Image.gz-lavender` и
`dist/SHA256SUMS` (соответствуют этим двум файлам). `uefi_*.img`,
`boot_pve_*.img`, `pve_rootfs_arm64*.img` **отсутствуют** в рабочем дереве:
собираются заново через `scripts/build-edk2.sh` + `scripts/build-rootfs.sh`
(10–30 мин + 40–60 мин в Lima VM) либо восстанавливаются из внешнего хранилища.
`./flash_all.sh --check` честно падает, пока их нет.

## Блокеры (устройство)

1. **whyred залочен** — `flashing unlock` / `oem unlock` отклонены
   (`Token Verify Failed`; нужна серверная подпись Xiaomi). Реальный путь —
   ADR-006 (EDL → чтение devinfo → точечный патч), статус **proposed**:
   подтверждён только Sahara-хендшейк и загрузка loader'а, Firehose-чтение
   ни разу не завершилось. Ожидаемый путь: `tools/edl-recon.py --loader …`
   → `bkerler/edl r devinfo` → `tools/analyze-devinfo.py`.
2. **lavender** — fastboot через кнопки недоступен (зависает на splash,
   phantom VolUp). EDL через тестпоинты грузит loader, но чтение не завершено.
   Обходной путь: BCB `bootonce-bootloader` в `misc` через UART-скрипт
   (`scripts/lavender-boot-fastboot.sh`, только 32 байта, с бэкапом и readback).
3. **Ни один UEFI-образ не загружался на устройстве** — проверены только сборка
   и целостность. Plan B (ABL + mainline) — первый по ADR-001.

## Следующий шаг

- whyred: EDL-разведка до первого байта записи (read-only), затем решение
  «патчить devinfo или Mi Unlock в Windows-VM».
- lavender: довести BCB-вход в fastboot → TWRP → бэкап `devinfo`/`boot`.
- Пересобрать артефакты в Lima VM, чтобы `--check` проходил, и обновить
  этот файл.
