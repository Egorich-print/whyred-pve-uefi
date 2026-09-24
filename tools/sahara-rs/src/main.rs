//! Minimal Qualcomm Sahara v2 client: upload a Firehose loader over EDL 9008.
//!
//! Protocol: little-endian u32 fields over USB bulk. After the loader is
//! accepted the device re-enumerates and Firehose XML takes over — this tool
//! stops there on purpose.
//!
//! The loader to upload is never guessed: pass it explicitly, because a loader
//! built for another SoC can wedge the device until it is power-cycled.

use anyhow::{Context, Result, bail};
use rusb::{Context as UsbContext, UsbContext as _};
use std::time::Duration;

const QCOM_VID: u16 = 0x05C6;
const QCOM_PID: u16 = 0x9008;
const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_WIRE_PACKET: usize = 1 << 20;

const SAHARA_HELLO_REQ: u32 = 0x1;
const SAHARA_HELLO_RSP: u32 = 0x2;
const SAHARA_READ_DATA: u32 = 0x3;
const SAHARA_END_TRANSFER: u32 = 0x4;
const SAHARA_DONE_REQ: u32 = 0x5;
const SAHARA_DONE_RSP: u32 = 0x6;
const SAHARA_RESET_RSP: u32 = 0x8;
const SAHARA_CMD_READY: u32 = 0xB;
const SAHARA_SWITCH_MODE: u32 = 0xC;
const SAHARA_STATUS_SUCCESS: u32 = 0x0;

struct SaharaClient {
    handle: rusb::DeviceHandle<UsbContext>,
    ep_in: u8,
    ep_out: u8,
    max_cmd_len: u32,
}

impl SaharaClient {
    fn open() -> Result<Self> {
        let ctx = UsbContext::new()?;
        for dev in ctx.devices()?.iter() {
            let desc = dev.device_descriptor()?;
            if desc.vendor_id() != QCOM_VID || desc.product_id() != QCOM_PID {
                continue;
            }
            let handle = dev.open()?;
            let cfg = dev.active_config_descriptor()?;
            let mut ep_in = 0x81u8;
            let mut ep_out = 0x01u8;
            for itf in cfg.interfaces() {
                for desc in itf.descriptors() {
                    for ep in desc.endpoint_descriptors() {
                        match ep.direction() {
                            rusb::Direction::In => ep_in = ep.address(),
                            rusb::Direction::Out => ep_out = ep.address(),
                        }
                    }
                }
            }
            handle.claim_interface(0).context("claiming interface 0")?;
            return Ok(Self {
                handle,
                ep_in,
                ep_out,
                max_cmd_len: 4096,
            });
        }
        bail!("no EDL device found (VID {QCOM_VID:#06x} PID {QCOM_PID:#04x})")
    }

    fn read(&mut self, len: usize) -> Result<Vec<u8>> {
        let len = len.clamp(256, MAX_WIRE_PACKET);
        let mut buf = vec![0u8; len];
        let n = self
            .handle
            .read_bulk(self.ep_in, &mut buf, TIMEOUT)
            .context("bulk read")?;
        buf.truncate(n);
        if buf.len() < 8 {
            bail!("short packet: {} bytes", buf.len());
        }
        Ok(buf)
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        let n = self
            .handle
            .write_bulk(self.ep_out, data, TIMEOUT)
            .context("bulk write")?;
        if n != data.len() {
            bail!("short write: {n} of {} bytes", data.len());
        }
        Ok(())
    }

    fn read_pkt(&mut self) -> Result<Vec<u8>> {
        self.read(self.max_cmd_len as usize)
    }

    /// Answer HELLO and return the first command packet, unconsumed.
    fn handshake(&mut self) -> Result<Vec<u8>> {
        let hello = self.read(48)?;
        if le32(&hello, 0) != SAHARA_HELLO_REQ {
            bail!("expected HELLO (0x1), got {:#x}", le32(&hello, 0));
        }
        let version = le32(&hello, 8);
        let max_cmd_len = le32(&hello, 16);
        let mode = le32(&hello, 20);
        if !(1..=3).contains(&version) {
            bail!("unexpected Sahara version {version}");
        }
        if max_cmd_len as usize > MAX_WIRE_PACKET {
            bail!("device max_cmd_len {max_cmd_len} exceeds {MAX_WIRE_PACKET}");
        }
        self.max_cmd_len = max_cmd_len.clamp(256, MAX_WIRE_PACKET as u32);
        eprintln!(
            "[sahara] HELLO: version={version}, max_cmd={}, mode={mode}",
            self.max_cmd_len
        );

        let mut resp = vec![0u8; 48];
        put32(&mut resp, 0, SAHARA_HELLO_RSP);
        put32(&mut resp, 4, 48);
        put32(&mut resp, 8, version);
        put32(&mut resp, 12, 1);
        put32(&mut resp, 16, self.max_cmd_len);
        put32(&mut resp, 20, mode);
        for (i, v) in [1u32, 2, 3, 4, 5, 6].iter().enumerate() {
            put32(&mut resp, 24 + i * 4, *v);
        }
        self.write(&resp)?;
        self.read_pkt()
    }
}

fn le32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn put32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn field(pkt: &[u8], off: usize) -> Result<u32> {
    if pkt.len() < off + 4 {
        bail!("short packet: need {} bytes, got {}", off + 4, pkt.len());
    }
    Ok(le32(pkt, off))
}

enum Step {
    Send(Vec<u8>),
    Wait,
    Finish,
}

/// Pure state machine: one device packet in, bytes-to-send (or terminal) out.
fn step(pkt: &[u8], loader: &[u8]) -> Result<Step> {
    let cmd = field(pkt, 0)?;
    match cmd {
        SAHARA_READ_DATA => {
            let image = field(pkt, 8)?;
            let offset = field(pkt, 12)? as usize;
            let len = field(pkt, 16)? as usize;
            if image != 0 {
                bail!("device asked for image id {image}, only 0 is supported");
            }
            let end = offset
                .checked_add(len)
                .ok_or_else(|| anyhow::anyhow!("offset {offset} + len {len} overflows"))?;
            if end > loader.len() {
                bail!(
                    "loader too short: device needs {end} bytes, have {}",
                    loader.len()
                );
            }
            eprintln!("[sahara] READ_DATA offset={offset:#x} len={len}");
            Ok(Step::Send(loader[offset..end].to_vec()))
        }
        SAHARA_END_TRANSFER => {
            let status = field(pkt, 12)?;
            if status != SAHARA_STATUS_SUCCESS {
                bail!("device reported image transfer failure (status {status:#x})");
            }
            let mut done = vec![0u8; 12];
            put32(&mut done, 0, SAHARA_DONE_REQ);
            put32(&mut done, 4, 12);
            put32(&mut done, 8, SAHARA_STATUS_SUCCESS);
            Ok(Step::Send(done))
        }
        SAHARA_DONE_RSP => {
            let status = field(pkt, 8)?;
            if status != SAHARA_STATUS_SUCCESS {
                bail!("loader rejected (DONE_RSP status {status:#x})");
            }
            Ok(Step::Finish)
        }
        SAHARA_RESET_RSP => Ok(Step::Finish),
        SAHARA_CMD_READY | SAHARA_HELLO_REQ | SAHARA_SWITCH_MODE => Ok(Step::Wait),
        other => bail!("unexpected Sahara command {other:#x}"),
    }
}

fn usage() -> &'static str {
    "usage: sahara-rs <loader.elf|loader.bin>\n\
     \n\
     Uploads a Firehose loader to a device in EDL 9008 mode.\n\
     The loader must match the SoC of the attached device (SDM636 vs SDM660\n\
     loaders are NOT interchangeable). Storage writes are never issued here:\n\
     after the loader runs the device re-enumerates and Firehose takes over."
}

fn main() -> Result<()> {
    let loader_path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("{}", usage());
            bail!("missing loader path");
        }
    };
    let loader =
        std::fs::read(&loader_path).with_context(|| format!("reading loader {loader_path}"))?;
    if loader.is_empty() {
        bail!("loader {loader_path} is empty");
    }
    eprintln!("[main] loader {loader_path}: {} bytes", loader.len());

    let mut c = SaharaClient::open()?;
    let mut pkt = c.handshake()?;

    loop {
        match step(&pkt, &loader)? {
            Step::Send(bytes) => c.write(&bytes)?,
            Step::Wait => {}
            Step::Finish => {
                eprintln!("[main] loader accepted");
                eprintln!(
                    "[main] the device should now re-enumerate; run Firehose commands against the new device."
                );
                return Ok(());
            }
        }
        pkt = c.read_pkt()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkt(cmd: u32, fields: &[(usize, u32)], len: usize) -> Vec<u8> {
        let mut v = vec![0u8; len];
        put32(&mut v, 0, cmd);
        put32(&mut v, 4, len as u32);
        for (off, val) in fields {
            put32(&mut v, *off, *val);
        }
        v
    }

    const LOADER: &[u8] = &[0x7f; 4096];

    #[test]
    fn full_upload_transcript() {
        let read = pkt(SAHARA_READ_DATA, &[(8, 0), (12, 0), (16, 80)], 20);
        match step(&read, LOADER).unwrap() {
            Step::Send(b) => assert_eq!(b, &LOADER[..80]),
            _ => panic!("expected loader chunk"),
        }
        let read_last = pkt(SAHARA_READ_DATA, &[(8, 0), (12, 4088), (16, 8)], 20);
        assert!(matches!(step(&read_last, LOADER).unwrap(), Step::Send(_)));
        let end = pkt(SAHARA_END_TRANSFER, &[(8, 0), (12, 0)], 16);
        match step(&end, LOADER).unwrap() {
            Step::Send(b) => assert_eq!(le32(&b, 0), SAHARA_DONE_REQ),
            _ => panic!("expected DONE request"),
        }
        let done = pkt(SAHARA_DONE_RSP, &[(8, 0)], 12);
        assert!(matches!(step(&done, LOADER).unwrap(), Step::Finish));
    }

    #[test]
    fn rejects_failure_statuses() {
        let end_fail = pkt(SAHARA_END_TRANSFER, &[(8, 0), (12, 1)], 16);
        assert!(step(&end_fail, LOADER).is_err());
        let done_fail = pkt(SAHARA_DONE_RSP, &[(8, 2)], 12);
        assert!(step(&done_fail, LOADER).is_err());
    }

    #[test]
    fn rejects_bad_requests() {
        assert!(step(&[0u8; 4], LOADER).is_err());
        let past_end = pkt(SAHARA_READ_DATA, &[(8, 0), (12, 4090), (16, 80)], 20);
        assert!(step(&past_end, LOADER).is_err());
        let other_image = pkt(SAHARA_READ_DATA, &[(8, 1), (12, 0), (16, 80)], 20);
        assert!(step(&other_image, LOADER).is_err());
        let short_read = pkt(SAHARA_READ_DATA, &[(8, 0)], 12);
        assert!(step(&short_read, LOADER).is_err());
    }

    #[test]
    fn reset_response_finishes() {
        let r = pkt(SAHARA_RESET_RSP, &[], 8);
        assert!(matches!(step(&r, LOADER).unwrap(), Step::Finish));
    }
}
