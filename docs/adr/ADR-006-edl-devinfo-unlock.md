# ADR-006: Unlock route — EDL/devinfo вместо Mi Unlock (серверный токен)

> Дата: 2026-08-23 · Статус: proposed (блокировано до получения devinfo-дампов)

## Контекст

whyred залочен (`unlocked:no`, anti-rollback 4, secure boot ON).
Все программные пути отказали:

| Команда | Результат |
|---------|-----------|
| `fastboot flashing unlock` | FAILED: Token Verify Failed |
| `fastboot oem unlock` / `unlock-go` | FAILED: Token Verify Failed |
| `fastboot flash/erase devinfo` | Erase is not allowed in Lock State |
| `fastboot oem edl` / `reboot-edl` | Invalid Parameter / unknown command |
| `adb reboot edl` | MIUI перехватывает, система грузится как обычно |
| `fastboot oem device-info` | **вешает ABL** до физической перезагрузки |

Mi Unlock использует RSA-подпись сервера Xiaomi — воспроизвести её нельзя.

## Решение (предлагаемое)

1. Вход в **EDL 9008** через test points (dupont-перемычка).
2. Sahara v2 + firehose-загрузчик (Xiaomi-signed) → чтение `devinfo`.
3. Патч флага `is_unlocked` по известной структуре Qualcomm LK
   (`magic[13] "ANDROID-BOOT!"` + bool-поля), запись только `devinfo`,
   с предварительным бэкапом.
4. Fallback — Windows-VM (UTM) с официальным Mi Unlock Tool.

## Статус и ограничения

- Sahara v2 удалось поднять (pyusb), loader загружается, но firehose-обмен
  нестабилен: устройство не перечисляется заново, ответы бинарные.
- Secure Boot может отвергнуть неподходящий по подписи загрузчик —
  нужен firehose, извлечённый из **whyred** fastboot ROM.
- Правило: read-only разведка (GPT, devinfo) до любых записей; оригиналы —
  в `dist/backups/`.
