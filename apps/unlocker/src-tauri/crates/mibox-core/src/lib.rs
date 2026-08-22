//! Fastboot protocol over libusb (rusb).
//!
//! Transport: bulk OUT to `fastboot` interface, bulk IN for responses.
//! Packet grammar (Android bootable/bootloader/bootloader_messages):
//!   INFO<text>  DATA<size:8hex>  OKAY[reason]  FAIL[reason]
//! Device: Google VID 0x18d1, PID 0xd00d in fastboot mode.

use anyhow::{anyhow, bail, Context, Result};
use rusb::{Context as UsbContext, DeviceHandle, UsbContext as _};

pub const GOOGLE_VID: u16 = 0x18d1;
pub const FASTBOOT_PID: u16 = 0xd00d;
const BULK_OUT: u8 = 0x01;
const BULK_IN: u8 = 0x81;
/// fastboot protocol max packet: 64 (v1) / 512 (USB3). Use conservative 64? ABL
/// on sdm660 negotiates 4096 max-download but command packets stay ≤64.
const MAX_PKT: usize = 64;
const TIMEOUT_MS: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq)]
pub enum FastbootResponse {
    Ok(Option<String>),
    Info(String),
    Data(usize),
    Fail(Option<String>),
}

/// Parse one response packet per protocol grammar. Pure function — unit tested.
pub fn parse_response(pkt: &[u8]) -> Result<FastbootResponse> {
    if pkt.len() < 4 {
        bail!("short packet ({})", pkt.len());
    }
    let text = String::from_utf8_lossy(&pkt[4..]).trim_end_matches('\0').to_string();
    match &pkt[..4] {
        b"INFO" => Ok(FastbootResponse::Info(text)),
        b"OKAY" => Ok(FastbootResponse::Ok((!text.is_empty()).then_some(text))),
        b"FAIL" => Ok(FastbootResponse::Fail((!text.is_empty()).then_some(text))),
        b"DATA" if pkt.len() >= 12 => {
            let n = usize::from_str_radix(&pkt[4..12].iter().map(|&b| b as char).collect::<String>(), 16)
                .map_err(|e| anyhow!("bad DATA size: {e}"))?;
            Ok(FastbootResponse::Data(n))
        }
        other => bail!("unknown response prefix {:?}", String::from_utf8_lossy(other)),
    }
}

pub struct FastbootDevice {
    handle: DeviceHandle<UsbContext>,
}

impl FastbootDevice {
    /// Open the first USB device that looks like a fastboot device.
    pub fn open_first() -> Result<Self> {
        let ctx = UsbContext::new()?;
        for dev in ctx.devices()?.iter() {
            let desc = dev.device_descriptor()?;
            if desc.vendor_id() == GOOGLE_VID {
                // PID 0xd00d = fastboot; some OEM firmwares keep other PIDs with
                // interface name "fastboot" — accept both.
                if desc.product_id() == FASTBOOT_PID || Self::has_fastboot_iface(&dev)? {
                    let h = dev.open()?;
                    h.set_auto_detach_kernel_driver(true).ok();
                    h.claim_interface(0).or_else(|_| h.claim_interface(1)).context("claim iface")?;
                    return Ok(Self { handle: h });
                }
            }
        }
        bail!("no fastboot device found")
    }

    fn has_fastboot_iface(dev: &rusb::Device<UsbContext>) -> Result<bool> {
        let cfg = dev.active_config_descriptor().or_else(|_| dev.config_descriptor(0))?;
        let is_ff = cfg
            .interfaces()
            .flat_map(|i| i.descriptors())
            .any(|d| d.class_code() == 0xff);
        Ok(is_ff)
        // ponytail: class-code heuristic only, proper interface-name string check if false positives appear
    }

    /// Send a command and collect responses until terminal OKAY/FAIL.
    pub fn command(&mut self, cmd: &str) -> Result<(FastbootResponse, Vec<String>)> {
        self.send(cmd.as_bytes())?;
        let mut infos = Vec::new();
        loop {
            let mut buf = [0u8; MAX_PKT];
            let n = self.handle.read_bulk(BULK_IN, &mut buf, TIMEOUT_MS)?;
            match parse_response(&buf[..n])? {
                FastbootResponse::Info(t) => infos.push(t),
                term @ (FastbootResponse::Ok(_) | FastbootResponse::Fail(_) | FastbootResponse::Data(_)) => {
                    return Ok((term, infos));
                }
            }
        }
    }

    fn send(&mut self, data: &[u8]) -> Result<()> {
        for chunk in data.chunks(MAX_PKT) {
            let n = self.handle.write_bulk(BULK_OUT, chunk, TIMEOUT_MS)?;
            if n != chunk.len() {
                bail!("short write {n}/{}", chunk.len());
            }
        }
        Ok(())
    }

    pub fn getvar(&mut self, var: &str) -> Result<String> {
        match self.command(&format!("getvar:{var}"))? {
            (FastbootResponse::Ok(v), _) => Ok(v.unwrap_or_default()),
            (FastbootResponse::Fail(e), _) => bail!("getvar {var}: {}", e.unwrap_or_default()),
            _ => bail!("unexpected getvar reply"),
        }
    }

    /// Upload + flash a partition. Payload sent in 512MiB-friendly chunks of MAX_PKT.
    pub fn flash(&mut self, part: &str, payload: &[u8]) -> Result<Vec<String>> {
        let (resp, _) = self.command(&format!("download:{:08x}", payload.len()))?;
        match resp {
            FastbootResponse::Data(n) if n == payload.len() => {}
            FastbootResponse::Data(n) => bail!("device wants {n}, we have {}", payload.len()),
            _ => bail!("no DATA phase for download"),
        }
        for chunk in payload.chunks(1 << 20) {
            self.write_all(chunk)?;
        }
        let (term, infos) = self.command(&format!("flash:{part}"))?;
        match term {
            FastbootResponse::Ok(_) => Ok(infos),
            FastbootResponse::Fail(e) => bail!("flash {part}: {}", e.unwrap_or_default()),
            _ => bail!("unexpected flash reply"),
        }
    }

    fn write_all(&mut self, mut data: &[u8]) -> Result<()> {
        while !data.is_empty() {
            let n = self.handle.write_bulk(BULK_OUT, &data[..data.len().min(1 << 14)], TIMEOUT_MS)?;
            data = &data[n..];
        }
        Ok(())
    }

    pub fn reboot_bootloader(&mut self) -> Result<()> {
        match self.command("reboot-bootloader")? {
            (FastbootResponse::Ok(_), _) => Ok(()),
            (FastbootResponse::Fail(e), _) => bail!("reboot: {}", e.unwrap_or_default()),
            _ => bail!("unexpected reboot reply"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_packet_kinds() {
        assert_eq!(parse_response(b"OKAY").unwrap(), FastbootResponse::Ok(None));
        assert_eq!(parse_response(b"OKAYdone").unwrap(), FastbootResponse::Ok(Some("done".into())));
        assert_eq!(parse_response(b"INFOflashing").unwrap(), FastbootResponse::Info("flashing".into()));
        assert_eq!(parse_response(b"DATA00010000\0").unwrap(), FastbootResponse::Data(0x10000));
        matches!(parse_response(b"FAILpartition not found"), Ok(FastbootResponse::Fail(Some(_))));
        assert!(parse_response(b"WAT?").is_err());
        assert!(parse_response(b"NO").is_err());
        assert!(parse_response(b"DATAsmall").is_err()); // bad hex size
    }
}
