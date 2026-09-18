# ADR-002: Reuse upstream edk2-msm SDM660 silicon; add lavender device port

> Дата: 2026-08-23 · Статус: accepted

## Контекст

whyred (SDM636) и lavender (SDM660) — один кристалл. edk2-msm уже содержит
`Silicon/Qualcomm/sdm660` + ACPI-таблицы, а для whyred — готовый
`configs/devices/whyred.conf`. Для lavender конфига не существовало, но
есть родственные порты Xiaomi (jason, wayne, clover).

## Решение

- **whyred**: ничего не портировать, собирать апстрим `./build.sh -d whyred --boot`.
- **lavender**: добавить в дерево edk2-msm минимальный набор:
  `configs/devices/lavender.conf`, `Platform/Xiaomi/sdm660/lavender.dsc`
  (tianma 1080×2340, GUID 827309bb-…-a11a4), `lavender.fdf.inc` и
  `FdtBlob_compat/lavender.dtb` (mainline tianma DTB).

## Последствия

- Патч lavender минимален (~4 файла), не трогает общие драйверы.
- Почему рассматривался только такой объём: любой «свой» дисплейный драйвер
  обречён без живого тестирования; переиспользование проверенных драйверов
  sdm660 даёт максимум шансов на первом запуске.
- Файлы порта живут в клоне edk2-msm внутри воркспейса и воспроизводятся
  скриптом `edk2/vm-port-lavender.sh` (идемпотентно).
