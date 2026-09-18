# ADR-004: Shared SDM660 rootfs, per-device boot payloads

> Дата: 2026-08-23 · Статус: accepted

## Контекст

whyred и lavender используют одну платформу (SDM660/636): одинаковые ядро,
конфиг, модули, fstab (`PARTLABEL=userdata`), сетевой стек. Различаются
только DTB и кадровый буфер.

## Решение

- **Один** rootfs-образ `pve_rootfs_arm64.img` (+ sparse) на всё семейство.
- **На устройство** — свои `boot` payload'ы: `uefi_{whyred,lavender}.img`,
  `boot_pve_{whyred,lavender}.img`, ядра `Image.gz-{whyred,lavender}`.
- Пайплайны параметризованы `DEVICE=whyred|lavender`
  (`scripts/build-edk2.sh`, `flash_all.sh`).

## Последствия

- Экономия 8 ГиБ на образ и ~30 мин сборки на каждое устройство.
- Hostname/персонализация — на этапе первого запуска, а не в образе
  (компромисс: образ не несёт идентичности устройства).
- В `kernel-config.fragment` и DTB всё device-specific остаётся вне rootfs.
