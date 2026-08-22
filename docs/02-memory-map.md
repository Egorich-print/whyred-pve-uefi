# whyred / SDM636 — physical memory map

Source: mainline Linux `arch/arm64/boot/dts/qcom/sdm630.dtsi` (common base for
SDM630/632/636/660, torvalds master 2026-08) + `sdm660-xiaomi-common.dtsi` +
`sdm636-xiaomi-whyred.dts` (sdm660-mainline/linux).

## DRAM

| Region | Base | Size | Notes |
|--------|------|------|-------|
| Main RAM | **0x80000000** | device: 3/4/6 GiB | kernel load @0x80008000 |
| qhee_code | 0x85800000 | | TZ-reserved |
| rmtfs_mem | 0x85e00000 | 1 MiB | modem FS |
| smem | 0x86000000 | | shared memory |
| tz_mem | 0x86200000 | | trustzone |
| mpss_region | 0x8ac00000 | | modem PIL |
| adsp_region | 0x92a00000 | | audio DSP |
| mba_region | 0x94800000 | | |
| venus_region | 0x9f800000 | | video CMA |
| ramoops | **0xa0000000** | 4 MiB | pstore (whyred dts) |

## SoC MMIO (key nodes)

| Peripheral | Base | Label |
|------------|------|-------|
| GCC (clocks) | 0x01000000 | `gcc` |
| BIMC/SNOC/CNOC/MMNOC | 0x1008000/0x1626000/0x1500000/0x1745000 | interconnect |
| TLMM pinctrl | 0x03100000 | `tlmm` |
| USB3 controller | 0xa8f8800 (DWC3 @0xa800000) | `usb3` — gadget/RNDIS host link |
| QUSB2 PHY0 | 0xc012000 | |
| eMMC (sdhc_1) | 0xc0c4000 | HS400, 8-bit |
| microSD (sdhc_2) | 0xc084000 | |
| MDSS display | 0xc900000 (MDP @0xc901000) | XBL-inited framebuffer → simple-framebuffer |
| BLSP1_UART2 (**console**) | **0xc170000** | serial debug, 115200n8 |
| BLSP2_UART1 | 0xc1af000 | BT HCI |
| SPMI (PMIC PM660/PM660L) | 0x800f000 | |
| APCS mailbox | 0x17911000 | |
| **GIC** | **0x17a00000** | `intc`, GICv2-style msm-qgic2, CPU iface @0x17b00000? see dtsi |
| WCN3990 WiFi | 0x18800000 | |
| Adreno A5xx GPU | 0x5000000 (GPUCC @0x5065000) | |

## Framebuffer handoff

XBL/ABL initializes the panel and passes framebuffer via cmdline
(`simple-framebuffer` node in whyred dts: 1080×2160, stride 4320 bytes).
EDK2-msm sdm660 implements a GOP driver over this pre-initialized fb
(SimpleInit) — no display re-init needed from UEFI side.

## Serial console wiring

BLSP1_UART2 @0xc170000 → test points on the mainboard (TX/RX pads near SIM
tray shield), 115200 8n1. Kernel arg: `earlycon=qcom_geni? no — msm_serial`
(legacy UART, driver `msm_serial`, console=ttyMSM0? on mainline sdm660:
`console=ttyMSM0,115200n8`).
