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

## Реализация (2026-09-26)

- `pve/vm-build-rootfs.sh`: chroot пересоздаётся, если нет маркера `.pve-build`
  (защита от повторного использования загрязнённого дерева, например с
  `proxmox-ve` и его ядром); `proxmox-default-kernel` отсутствие — проверяется;
  `apt-get clean` + удаление списков уменьшают образ.
- Одинаковый rootfs действительно обслуживает оба устройства: fstab, systemd,
  сеть и hostname не привязаны к whyred; per-device — только ядро, DTB и
  `boot_pve_*.img`.
- `sparse-rs img2simg` переведён на потоковый кодировщик: 8 ГиБ rootfs
  конвертируется при ~13 МБ RSS вместо ~17 ГиБ пиковой памяти.

## Последствия

- Экономия 8 ГиБ на образ и ~30 мин сборки на каждое устройство.
- Hostname/персонализация — на этапе первого запуска, а не в образе
  (компромисс: образ не несёт идентичности устройства).
- В `kernel-config.fragment` и DTB всё device-specific остаётся вне rootfs.
