# ADR-003: Official Proxmox VE arm64 repository (trixie), component install

> Дата: 2026-08-23 · Статус: accepted

## Контекст

Исторически Proxmox VE ARM64 существовал только как community-порты
(apqa.cn/jiangcuo, jens-maus), требующие отдельного сопровождения и
содержащие отстающие пакеты. 2026-08-05 Proxmox выпустил **официальный
arm64** (PVE 9.2, база Debian trixie) с репозиторием на download.proxmox.com.

При установке метапакета `proxmox-ve` в chroot возникает стена:
он тянет `proxmox-default-kernel`, чей `initramfs` postinst падает
(`unshare: cannot change root filesystem propagation`).

## Решение

1. debootstrap trixie arm64 + **официальный** репозиторий
   `deb [arch=arm64] http://download.proxmox.com/debian/pve trixie pve-no-subscription`.
2. Ставить **компоненты** (`pve-manager`, `lxc-pve`), а не метапакет
   `proxmox-ve`: PVE-ядро не нужно — устройство грузит собственное mainline.
   **Уточнение (2026-09-26, найдено реальной сборкой):** этого недостаточно
   само по себе — `apt` по умолчанию тянет Recommends, а
   `pve-yew-mobile-gui` (в графе веб-интерфейса) имеет
   `Recommends: proxmox-ve`, а тот `Depends: proxmox-default-kernel` →
   весь PVE-стек ядер и `initramfs`, который не собирается в chroot.
   Поэтому установка идёт с `--no-install-recommends`, а нужные Recommends
   (`proxmox-firewall` для веб-UI) перечисляются явно. Проверка
   отсутствия `proxmox-default-kernel` остаётся в сборке как утверждение.
3. chroot подготавливать self-bind (`mount --bind $R $R`) — тогда unshare
   propagation работает, и хуки ядра не падают.

## Последствия

- Нет зависимости от неофициальных репозиториев и их ключей.
- KVM на устройстве не будет (нет EL2), но LXC-контейнеры — цель проекта —
  работают полностью.
- `pve-enterprise.sources`, который кладут пакеты, удаляется (no-subscription).
