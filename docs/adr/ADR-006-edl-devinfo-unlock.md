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

## Статус и ограничения (обновлено 2026-09-25)

- Sahara v2 подтверждён на живом устройстве: `tools/edl-recon.py --loader …`
  (или `tools/sahara-rs`) доводят загрузку до `DONE_RSP status=0` — это
  теперь проверяемое состояние, а не «вроде загрузился».
- Firehose-чтение не завершилось ни разу: после загрузки устройство должно
  перечислиться заново. Чтения ведутся поддерживаемым клиентом
  (`Tools/edl/venv-edl/bin/edl r devinfo`) — свой самопис firehose удалён
  из проекта.
- Запись `devinfo` допустима только через partition-aware интерфейс
  (`edl w devinfo …`). `edl wf <file>` пишет от сектора 0 и сносит MBR/GPT.
- Secure Boot может отвергнуть неподходящий по подписи загрузчик —
  нужен firehose, извлечённый из **whyred** fastboot ROM.
- Правило: read-only разведка (GPT, devinfo) до любых записей; оригиналы — в `backups/` (вне git).
