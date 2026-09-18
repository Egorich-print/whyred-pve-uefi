//! Minimal Qualcomm Sahara v2 + Firehose client for reading devinfo from EDL 9008.
//!
//! Sahara protocol: little-endian u32 fields, USB bulk transfers.
//! Firehose: XML commands over the same USB interface after loader upload.

use anyhow::{bail, Context, Result};
use rusb::{Context as UsbContext, UsbContext as _};
use std::io::{Read, Write};
use std::time::Duration;

const QCOM_VID: u16 = 0x05C6;
const QCOM_PID: u16 = 0x9008;
const TIMEOUT: Duration = Duration::from_secs(5);

// Sahara commands
const SAHARA_HELLO: u32 = 0x01;
const SAHARA_HELLO_RESP: u32 = 0x02;
const SAHARA_CMD_READ: u32 = 0x03;
const SAHARA_CMD_END: u32 = 0x04;
const SAHARA_CMD_DONE: u32 = 0x05;
const SAHARA_CMD_DONE_RESP: u32 = 0x06;
const SAHARA_CMD_RESET: u32 = 0x07;
const SAHARA_CMD_MEM_DEBUG: u32 = 0x08;
const SAHARA_CMD_MEM_READ: u32 = 0x09;
const SAHARA_CMD_MEM_WRITE: u32 = 0x0A;
const SAHARA_CMD_EXEC: u32 = 0x0B;
const SAHARA_CMD_EXEC_RESP: u32 = 0x0C;
const SAHARA_CMD_EXEC_DATA: u32 = 0x0D;
const SAHARA_CMD_READ_DATA: u32 = 0x0E;

// Sahara exec commands
const EXEC_CMD_SERIAL: u32 = 0x04;
const EXEC_CMD_MSM_HWID: u32 = 0x18;
const EXEC_CMD_OEM_HASH: u32 = 0x19;

// Sahara modes
const MODE_IMAGE_TX: u32 = 0x00;
const MODE_COMMAND: u32 = 0x01;

fn le32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn put32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

struct SaharaClient {
    handle: rusb::DeviceHandle<UsbContext>,
    ep_in: u8,
    ep_out: u8,
    max_cmd_len: u32,
    version: u32,
}

impl SaharaClient {
    fn open() -> Result<Self> {
        let ctx = UsbContext::new()?;
        for dev in ctx.devices()?.iter() {
            let desc = dev.device_descriptor()?;
            if desc.vendor_id() == QCOM_VID && desc.product_id() == QCOM_PID {
                let mut handle = dev.open()?;
                // find bulk endpoints
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
                handle.claim_interface(0).ok();
                return Ok(Self { handle, ep_in, ep_out, max_cmd_len: 4096, version: 2 });
            }
        }
        bail!("no EDL device found")
    }

    fn read(&mut self, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len.max(256)];
        let n = self.handle.read_bulk(self.ep_in, &mut buf, Duration::from_secs(10))?;
        buf.truncate(n);
        Ok(buf)
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.handle.write_bulk(self.ep_out, data, TIMEOUT)?;
        Ok(())
    }

    /// Sahara handshake: receive HELLO, send HELLO_RESP
    fn handshake(&mut self) -> Result<u32> {
        let hello = self.read(48)?;
        let cmd = le32(&hello, 0);
        if cmd != SAHARA_HELLO {
            bail!("expected HELLO (0x01), got {cmd:#x}");
        }
        self.version = le32(&hello, 8);
        self.max_cmd_len = le32(&hello, 16);
        let mode = le32(&hello, 20);
        eprintln!("[sahara] HELLO: version={}, max_cmd={}, mode={}", self.version, self.max_cmd_len, mode);

        // send HELLO_RESP
        let mut resp = vec![0u8; 48];
        put32(&mut resp, 0, SAHARA_HELLO_RESP);
        put32(&mut resp, 4, 48); // length
        put32(&mut resp, 8, self.version);
        put32(&mut resp, 12, 1); // version_min
        put32(&mut resp, 16, self.max_cmd_len);
        put32(&mut resp, 20, mode); // match device mode
        // supported modes
        put32(&mut resp, 24, 0); // mode 0 = image tx
        put32(&mut resp, 28, 1); // mode 1 = command
        self.write(&resp)?;

        // read response — could be CMD_READ, CMD_EXEC, CMD_END
        let pkt = self.read(self.max_cmd_len as usize)?;
        let cmd = le32(&pkt, 0);
        eprintln!("[sahara] response: cmd={cmd:#x}");
        Ok(cmd)
    }

    /// Execute a Sahara exec command (serial, HWID, etc.)
    fn exec_cmd(&mut self, exec_id: u32) -> Result<Vec<u8>> {
        let mut req = vec![0u8; 12];
        put32(&mut req, 0, SAHARA_CMD_EXEC);
        put32(&mut req, 4, 12);
        put32(&mut req, 8, exec_id);
        self.write(&req)?;
        let resp = self.read(self.max_cmd_len as usize)?;
        let cmd = le32(&resp, 0);
        if cmd == SAHARA_CMD_EXEC_RESP {
            let data_len = le32(&resp, 4) as usize - 16; // subtract header
            Ok(resp[16..16 + data_len.min(resp.len() - 16)].to_vec())
        } else if cmd == SAHARA_CMD_EXEC_DATA {
            // data follows in next packet
            let data_len = le32(&resp, 8) as usize;
            self.read(data_len)
        } else {
            bail!("unexpected exec response: cmd={cmd:#x}")
        }
    }

    /// Handle CMD_READ: device requests loader data at offset+length
    fn handle_cmd_read_raw(pkt: &[u8], loader: &[u8]) -> Result<(usize, usize)> {
        let offset = le32(pkt, 8) as usize;
        let length = le32(pkt, 12) as usize;
        eprintln!("[sahara] CMD_READ: offset={offset:#x}, length={length}");
        if offset + length > loader.len() {
            bail!("loader too short: need {} bytes, have {}", offset + length, loader.len());
        }
        Ok((offset, length))
    }
}

/// Firehose: send XML command and read response
fn firehose_cmd(handle: &mut rusb::DeviceHandle<UsbContext>, ep_in: u8, ep_out: u8, cmd: &str) -> Result<String> {
    let xml = format!("{}\n", cmd);
    handle.write_bulk(ep_out, xml.as_bytes(), TIMEOUT)?;
    let mut resp = Vec::new();
    loop {
        let mut buf = [0u8; 4096];
        let n = handle.read_bulk(ep_in, &mut buf, TIMEOUT)?;
        resp.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&resp);
        if text.contains("ACK") || text.contains("NAK") || text.contains("</data>") {
            return Ok(text.to_string());
        }
    }
}

fn main() -> Result<()> {
    let loader_path = std::env::args().nth(1).unwrap_or_else(|| {
        "loaders-local/jasmine_prog_emmc_firehose_Sdm660_ddr.elf".to_string()
    });

    eprintln!("[main] opening EDL device...");
    let mut sahara = SaharaClient::open()?;

    eprintln!("[main] Sahara handshake...");
    let first_cmd = sahara.handshake()?;

    let loader = std::fs::read(&loader_path)
        .with_context(|| format!("reading {loader_path}"))?;
    eprintln!("[main] loader: {} bytes", loader.len());

    // Main loop: handle commands from device
    let mut got_loader = false;
    let mut pkt = match first_cmd {
        SAHARA_CMD_READ | SAHARA_CMD_READ_DATA => {
            // need to construct a fake pkt from the first_cmd we already consumed
            // Actually we consumed it in handshake, need to re-read
            sahara.read(sahara.max_cmd_len as usize)?
        }
        SAHARA_CMD_EXEC => {
            eprintln!("[main] command mode — reading info...");
            match sahara.exec_cmd(EXEC_CMD_SERIAL) {
                Ok(d) => eprintln!("  serial: {}", hex(&d)),
                Err(e) => eprintln!("  serial: {e}"),
            }
            match sahara.exec_cmd(EXEC_CMD_MSM_HWID) {
                Ok(d) => eprintln!("  hwid: {}", hex(&d)),
                Err(e) => eprintln!("  hwid: {e}"),
            }
            // send END to request image_tx mode
            let mut end = vec![0u8; 8];
            put32(&mut end, 0, SAHARA_CMD_END);
            put32(&mut end, 4, 8);
            sahara.write(&end)?;
            sahara.read(sahara.max_cmd_len as usize)?
        }
        SAHARA_CMD_END => {
            let mut done = vec![0u8; 8];
            put32(&mut done, 0, SAHARA_CMD_DONE);
            put32(&mut done, 4, 8);
            sahara.write(&done)?;
            sahara.read(sahara.max_cmd_len as usize)?
        }
        _ => bail!("unexpected first cmd: {first_cmd:#x}"),
    };

    loop {
        let cmd = le32(&pkt, 0);
        match cmd {
            SAHARA_CMD_READ | SAHARA_CMD_READ_DATA => {
                let (offset, length) = SaharaClient::handle_cmd_read_raw(&pkt, &loader)?;
                sahara.write(&loader[offset..offset + length])?;
            }
            SAHARA_CMD_END => {
                eprintln!("[main] loader upload complete, sending DONE");
                let mut done = vec![0u8; 8];
                put32(&mut done, 0, SAHARA_CMD_DONE);
                put32(&mut done, 4, 8);
                sahara.write(&done)?;
                got_loader = true;
                // read DONE response or reset
                match sahara.read(64) {
                    Ok(r) => {
                        let c = le32(&r, 0);
                        eprintln!("[main] DONE resp: cmd={c:#x}");
                    }
                    Err(_) => {}
                }
                break;
            }
            SAHARA_CMD_RESET => {
                eprintln!("[main] device reset — loader accepted");
                got_loader = true;
                break;
            }
            SAHARA_HELLO => {
                eprintln!("[main] re-handshake");
                sahara.handshake()?;
                break;
            }
            _ => {
                eprintln!("[main] cmd {cmd:#x} during upload, skipping");
            }
        }
        pkt = sahara.read(sahara.max_cmd_len as usize)?;
    }

    if got_loader {
        eprintln!("[main] loader uploaded successfully!");
        eprintln!("[main] device should now enumerate as new USB device.");
        eprintln!("[main] Run firehose commands (printgpt, r devinfo) with the new device.");
    }

    Ok(())
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join("")
}
