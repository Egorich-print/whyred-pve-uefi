# ADR-001: Dual boot path — UEFI payload (Plan A) and direct-ABL kernel (Plan B)

> Дата: 2026-08-23 · Статус: accepted

## Контекст

Цель — Proxmox VE ARM64 на whyred/lavender через UEFI (Tianocore/edk2-msm).
UEFI-слой на телефоне — экспериментальный: драйвер дисплея опирается на
инициализированный XBL кадровый буфер, eMMC/HS400 и GOP не проверены на
живом устройстве. Прямая прошивка UEFI в `boot` без UART-диагностики
рискует получить не загружающийся аппарат без быстрого способа отката.

## Решение

Поддерживать **два взаимозаменяемых пути** в одном репозитории и на одном
rootfs:

- **Plan A (UEFI)**: `uefi_*.img` (payload edk2-msm) → `fastboot flash boot`
- **Plan B (direct)**: `boot_pve_*.img` (mainline `Image.gz`+DTB) → `fastboot flash boot`

Оба образа собираются `payload-packer`; rootfs общий.

## Последствия

- Bring-up начинается с Plan B (ABL — известный работающий загрузчик),
  UEFI подключается после подтверждения консоли.
- Один источник ошибок исключён: rootfs и ядро идентичны в обоих путях.
- Дублирование: два boot-образа вместо одного, ~22 MiB против 6 MiB.
