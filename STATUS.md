# STATUS — whyred-pve-uefi

> Обновлено: 2026-09-26 (ночная миссия) · Слой: Execution ·
> Статус: Active — tooling green, артефакты пересобраны, device work blocked

## Что готово и проверено

| Компонент | Состояние | Чем проверено |
|-----------|-----------|---------------|
| Чистка дерева | ✅ | `scripts/clean.sh` (-dry-run, идемпотентность), критерии A1–A8 в `docs/CLEANUP-2026-09-26.md` |
| Rust-тулинг | ✅ | `scripts/check.sh`: 24 теста в tools/ (6 bootimg + 3 property + 9 sparse + 2 payload-packer + 4 sahara) + 1 в mibox-core; clippy `-D warnings` чист, fmt чист |
| `bootimg-rs` | ✅ | v0–v4 парсинг, структурный детект endianness (регрессия на LE + кратный 256 размер) |
| `sparse-rs` | ✅ | round-trip + отказ на неполном покрытии/неизвестных chunk'ах/абсурдном размере |
| `payload-packer` | ✅ | пустой payload отклонён, лимит 64 MiB, `second_addr=0` как в документации |
| `sahara-rs` + `edl-recon.py` | ✅ unit-tested | оба автомата HELLO→READ_DATA→END→DONE_RSP с транскрипт-тестами (Rust 4 + Python 4); успех только при `DONE_RSP status=0` |
| `sparse-rs` | ✅ streaming | кодирование 8 ГиБ при ~13 МБ RSS (было ~17 ГиБ пик), `info` читает заголовок за 28 байт для гейта прошивки |
| `flash_all.sh` | ✅ fail-closed | product/serial/unlocked проверки, manifest до первой записи, `misc` не трогается, заголовки boot.img и sparse rootfs валидируются, 8 гейт-тестов в `tests/flash_all_test.sh` |
| Сборка EDK2/rootfs | ✅ прогнана | `pve-builder` (Lima, aarch64): EDK2-артефакты перенесены и провалидированы, rootfs пересобирается с нуля (маркер `.pve-build` + чистка apt) |
| `bootimg-rs validate` | ✅ | профили `kernel`/`uefi`; встроен в `flash_all.sh` до первой записи |
| Tauri-приложение | ✅ | `withGlobalTauri`, allowlist boot/cache/recovery, подтверждение серийником, `oem device-info` удалён |

## Артефакты и чистка

В git версионируются только `dist/Image.gz-whyred`, `dist/Image.gz-lavender` и
`dist/SHA256SUMS` — это 32 МБ, которые нельзя воспроизвести дешевле, чем хранить.
Остальное пересобирается и лежит в `dist/` локально:

| Артефакт | Размер | Проверка |
|----------|--------|----------|
| `uefi_whyred.img` | 6.34 MB | `validate --profile uefi` OK; FD 6 333 313 B — совпадает с измерением в docs/01-partitions.md |
| `uefi_lavender.img` | 6.01 MB | `validate --profile uefi` OK |
| `boot_pve_whyred.img` / `boot_pve_lavender.img` | 16.28 MB | `validate --profile kernel` OK: v1, page 4096, kernel @0x8000, console+root в cmdline |
| `pve_rootfs_arm64.sparse.img` | 1.69 GiB (1 818 792 392 B) | пересобран 2026-09-26 без PVE-стека ядер и apt-кэшей (было 4.76 GB); заголовок проверен: logical 8 GiB ≤ userdata 51.37 GiB |

`./flash_all.sh --check` проверяет манифест, размеры и заголовки boot-образа и
sparse-rootfs до любой записи; без `dist/SHA256SUMS` он отказывается работать.

Сборочные выводы (`dist/*.img`) и кэши (`target/`, `__pycache__/`) не хранятся
в репозитории и удаляются `scripts/clean.sh` (`--dist` — изображения,
`--vm` — выводы в Lima VM). Подробности и критерии чистки:
`docs/CLEANUP-2026-09-26.md`.

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
3. **Ни один UEFI-образ не загружался на устройстве** — проверены только сборка,
   целостность и заголовки. Plan B (ABL + mainline) — первый по ADR-001.

## Известные ограничения (не блокеры)

- Отпечаток Proxmox-ключа лежит в `pve/proxmox-release-key.fpr` с пометкой
  «verify before trusting»; сборка требует явного `PROXMOX_KEY_FPR`.

## Следующий шаг

- whyred: EDL-разведка до первого байта записи (read-only), затем решение
  «патчить devinfo или Mi Unlock в Windows-VM».
- lavender: довести BCB-вход в fastboot → TWRP → бэкап `devinfo`/`boot`.
- Всё, что можно было сделать без телефона, сделано: артефакты собраны и
  провалидированы, пайплайн прогнан end-to-end на `pve-builder`.
