//! Android boot.img v0/v1/v2/v3 parser, unpacker, packer.
//!
//! All header integers are BIG-endian (AOSP `bootimg.h`). Images are laid out
//! on `page_size` boundaries: [header][kernel][ramdisk][second][recovery_dtbo][dtb].

use std::io::{self, Read, Write};

pub const MAGIC: &[u8; 8] = b"ANDROID!";
const V0_HDR: usize = 1648;
const V1_HDR: usize = V0_HDR + 4 + 8 + 4; // + recovery_dtbo_size/offset, header_size
const V2_HDR: usize = V1_HDR + 4 + 8; // + dtb_size, dtb_addr
const V3_HDR: usize = 1580; // compact layout
const V4_HDR: usize = V3_HDR + 4; // + signature_size

#[derive(Debug, Clone, PartialEq)]
pub struct BootImage {
    pub version: u32,
    // v0-v2 fields
    pub kernel_size: u32,
    pub kernel_addr: u32,
    pub ramdisk_size: u32,
    pub ramdisk_addr: u32,
    pub second_size: u32,
    pub second_addr: u32,
    pub tags_addr: u32,
    pub page_size: u32,
    pub os_version: u32,
    pub name: String,
    pub cmdline: String,
    pub extra_cmdline: String,
    pub id: [u32; 8],
    pub recovery_dtbo_size: u32,
    pub recovery_dtbo_offset: u64,
    pub dtb_size: u32,
    pub dtb_addr: u64,
    // v3/v4 fields
    pub signature_size: u32,
    // payloads
    pub kernel: Vec<u8>,
    pub ramdisk: Option<Vec<u8>>,
    pub second: Option<Vec<u8>>,
    pub recovery_dtbo: Option<Vec<u8>>,
    pub dtb: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error(msg.into()))
}

pub type Result<T> = std::result::Result<T, Error>;

fn be32(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn be64(b: &[u8], off: usize) -> u64 {
    u64::from_be_bytes([
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
        b[off + 4],
        b[off + 5],
        b[off + 6],
        b[off + 7],
    ])
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

fn align(n: usize, page: usize) -> usize {
    n.div_ceil(page) * page
}

impl BootImage {
    /// Parse from reader. Auto-detects v0/v1/v2 and v3/v4 headers.
    pub fn parse<R: Read>(mut r: R) -> Result<Self> {
        let mut buf = Vec::new();
        r.read_to_end(&mut buf)
            .map_err(|e| Error(format!("read: {e}")))?;
        if buf.len() < 8 || &buf[..8] != MAGIC {
            return err("not an Android boot image (bad magic)");
        }
        let candidate_v3 = be32(&buf, 20) == V3_HDR as u32 || be32(&buf, 20) == V4_HDR as u32;
        if candidate_v3 && be32(&buf, 24) >= 3 && be32(&buf, 24) <= 4 {
            return Self::parse_v34(buf);
        }
        Self::parse_v02(buf)
    }

    fn parse_v02(buf: Vec<u8>) -> Result<Self> {
        let hdr_ver_field = be32(&buf, 40);
        let version = match hdr_ver_field {
            0 | 1 | 2 => hdr_ver_field, // could still be legacy dt_size!=0; treated as v0 payload below
            _ => 0,                     // legacy: field is dt_size
        };
        let need = match version {
            2 => V2_HDR,
            1 => V1_HDR,
            _ => V0_HDR,
        };
        if buf.len() < need {
            return err(format!("truncated: {} < {need}", buf.len()));
        }
        let page = be32(&buf, 36) as usize;
        if !(512..=65536).contains(&page) || !page.is_power_of_two() {
            return err(format!("bad page size {}", be32(&buf, 36)));
        }
        // Legacy images may store a real dt blob size in the version slot.
        let (dtb_size_hdr, dtb_blob) = if version == 0 && hdr_ver_field > 2 {
            (hdr_ver_field, true)
        } else if version == 2 {
            (be32(&buf, V1_HDR + 12), false)
        } else {
            (0, false)
        };

        let mut off = page;
        let mut take = |size: u32| -> Result<Vec<u8>> {
            let s = size as usize;
            if off.saturating_add(s) > buf.len() {
                return err("image truncated in section");
            }
            let d = buf[off..off + s].to_vec();
            off += align(s, page);
            Ok(d)
        };

        let kernel_size = be32(&buf, 8);
        let kernel = take(kernel_size)?;
        let ramdisk_size = be32(&buf, 16);
        let ramdisk = if ramdisk_size > 0 {
            Some(take(ramdisk_size)?)
        } else {
            None
        };
        let second_size = be32(&buf, 24);
        let second = if second_size > 0 {
            Some(take(second_size)?)
        } else {
            None
        };
        let recovery_dtbo_size = if version >= 1 { be32(&buf, V0_HDR) } else { 0 };
        let recovery_dtbo = if recovery_dtbo_size > 0 {
            Some(take(recovery_dtbo_size)?)
        } else {
            None
        };
        let dtb_size = if dtb_blob { dtb_size_hdr } else if version == 2 { be32(&buf, V1_HDR) } else { 0 };
        let dtb = if dtb_size > 0 { Some(take(dtb_size)?) } else { None };

        Ok(BootImage {
            version,
            kernel_size,
            kernel_addr: be32(&buf, 12),
            ramdisk_size,
            ramdisk_addr: be32(&buf, 20),
            second_size,
            second_addr: be32(&buf, 28),
            tags_addr: be32(&buf, 32),
            page_size: page as u32,
            os_version: be32(&buf, 44),
            name: cstr(&buf[48..64]),
            cmdline: cstr(&buf[64..576]),
            extra_cmdline: cstr(&buf[608..1632]),
            id: (0..8).map(|i| be32(&buf, 576 + i * 4)).collect::<Vec<_>>().try_into().unwrap(),
            recovery_dtbo_size,
            recovery_dtbo_offset: if version >= 1 { be64(&buf, V0_HDR + 4) } else { 0 },
            dtb_size,
            dtb_addr: if version == 2 { be64(&buf, V1_HDR + 4) } else { 0 },
            signature_size: 0,
            kernel,
            ramdisk,
            second,
            recovery_dtbo,
            dtb,
        })
    }

    fn parse_v34(buf: Vec<u8>) -> Result<Self> {
        let version = be32(&buf, 24);
        let need = if version == 3 { V3_HDR } else { V4_HDR };
        if buf.len() < need {
            return err("truncated v3/v4 header");
        }
        let page = 4096usize; // fixed by spec for v3+
        let kernel_size = be32(&buf, 8);
        let ramdisk_size = be32(&buf, 12);
        let mut off = page;
        let mut take = |size: u32| -> Result<Vec<u8>> {
            let s = size as usize;
            if off.saturating_add(s) > buf.len() {
                return err("image truncated in section");
            }
            let mut d = buf[off..off + s].to_vec();
            d.resize(align(s, page), 0);
            d.truncate(s);
            off += align(s, page);
            Ok(d)
        };
        let kernel = take(kernel_size)?;
        let ramdisk = if ramdisk_size > 0 { Some(take(ramdisk_size)?) } else { None };
        Ok(BootImage {
            version,
            kernel_size,
            kernel_addr: 0,
            ramdisk_size,
            ramdisk_addr: 0,
            second_size: 0,
            second_addr: 0,
            tags_addr: 0,
            page_size: page as u32,
            os_version: be32(&buf, 16),
            name: String::new(),
            cmdline: cstr(&buf[44..V3_HDR.min(1580)]),
            extra_cmdline: String::new(),
            id: [0; 8],
            recovery_dtbo_size: 0,
            recovery_dtbo_offset: 0,
            dtb_size: 0,
            dtb_addr: 0,
            signature_size: if version == 4 { be32(&buf, V3_HDR) } else { 0 },
            kernel,
            ramdisk,
            second: None,
            recovery_dtbo: None,
            dtb: None,
        })
    }

    fn push_page(out: &mut Vec<u8>, data: &[u8], page: usize) {
        out.extend_from_slice(data);
        out.resize(align(out.len(), page), 0);
    }

    /// Serialize to bytes (v0/v1/v2 only; whyred ABL consumes v1).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self.version > 2 {
            return err("pack supported for v0/v1/v2");
        }
        let page = self.page_size as usize;
        let hdr_size = match self.version {
            2 => V2_HDR,
            1 => V1_HDR,
            _ => V0_HDR,
        };
        let mut h = vec![0u8; V0_HDR];
        h[..8].copy_from_slice(MAGIC);
        let put32 = |h: &mut Vec<u8>, off: usize, v: u32| h[off..off + 4].copy_from_slice(&v.to_be_bytes());
        put32(&mut h, 8, self.kernel.len() as u32);
        put32(&mut h, 12, self.kernel_addr);
        put32(&mut h, 16, self.ramdisk.as_ref().map_or(0, |r| r.len()) as u32);
        put32(&mut h, 20, self.ramdisk_addr);
        put32(&mut h, 24, self.second.as_ref().map_or(0, |s| s.len()) as u32);
        put32(&mut h, 28, self.second_addr);
        put32(&mut h, 32, self.tags_addr);
        put32(&mut h, 36, self.page_size);
        if self.version == 0 {
            // legacy slot holds dt_size; we never pack legacy-dt, keep 0
            put32(&mut h, 40, 0);
        } else {
            put32(&mut h, 40, self.version);
        }
        put32(&mut h, 44, self.os_version);
        h[48..48 + self.name.len().min(16)].copy_from_slice(&self.name.as_bytes()[..self.name.len().min(16)]);
        let cmd: Vec<u8> = self.cmdline.bytes().take(512).collect();
        h[64..64 + cmd.len()].copy_from_slice(&cmd);
        let extra: Vec<u8> = self.extra_cmdline.bytes().take(1024).collect();
        h[608..608 + extra.len()].copy_from_slice(&extra);
        for (i, v) in self.id.iter().enumerate() {
            put32(&mut h, 576 + i * 4, *v);
        }
        let mut out = vec![0u8; page];
        out[..V0_HDR].copy_from_slice(&h);
        if self.version >= 1 {
            let mut tail = Vec::new();
            tail.extend_from_slice(&(self.recovery_dtbo.as_ref().map_or(0, |r| r.len()) as u32).to_be_bytes());
            tail.extend_from_slice(&self.recovery_dtbo_offset.to_be_bytes());
            tail.extend_from_slice(&(hdr_size as u32).to_be_bytes());
            if self.version == 2 {
                tail.extend_from_slice(&(self.dtb.as_ref().map_or(0, |d| d.len()) as u32).to_be_bytes());
                tail.extend_from_slice(&self.dtb_addr.to_be_bytes());
            }
            out[V0_HDR..V0_HDR + tail.len()].copy_from_slice(&tail);
        }
        Self::push_page(&mut out, &self.kernel, page);
        if let Some(r) = &self.ramdisk {
            Self::push_page(&mut out, r, page);
        }
        if let Some(s) = &self.second {
            Self::push_page(&mut out, s, page);
        }
        if let Some(r) = &self.recovery_dtbo {
            Self::push_page(&mut out, r, page);
        }
        if let Some(d) = &self.dtb {
            Self::push_page(&mut out, d, page);
        }
        Ok(out)
    }
}

/// Write unpacked components into `dir`.
pub fn unpack(img: &BootImage, dir: &std::path::Path) -> io::Result<Vec<std::path::PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let w = |name: &str, data: &[u8]| -> io::Result<std::path::PathBuf> {
        let p = dir.join(name);
        let mut f = io::BufWriter::new(std::fs::File::create(&p)?);
        f.write_all(data)?;
        Ok(p)
    };
    let mut files = vec![w("kernel", &img.kernel)?];
    if let Some(r) = &img.ramdisk {
        files.push(w("ramdisk", r)?);
    }
    if let Some(s) = &img.second {
        files.push(w("second", s)?);
    }
    if let Some(d) = &img.dtb {
        files.push(w("dtb", d)?);
    }
    if let Some(r) = &img.recovery_dtbo {
        files.push(w("recovery_dtbo", r)?);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(version: u32) -> BootImage {
        BootImage {
            version,
            kernel_size: 0,
            kernel_addr: 0x8000,
            ramdisk_size: 0,
            ramdisk_addr: 0x1000000,
            second_size: 0,
            second_addr: 0xf00000,
            tags_addr: 0x100,
            page_size: 4096,
            os_version: (11 << 11) | (2020 << 3), // arbitrary encoding bits for roundtrip only
            name: "whyred".into(),
            cmdline: "console=ttyMSM0,115200n8 androidboot.hardware=qcom".into(),
            extra_cmdline: "quiet".into(),
            id: [0; 8],
            recovery_dtbo_size: 0,
            recovery_dtbo_offset: 7,
            dtb_size: 0,
            dtb_addr: 9,
            signature_size: 0,
            kernel: vec![0xA5u8; 5000], // spans pages
            ramdisk: Some(vec![1, 2, 3, 4]),
            second: None,
            recovery_dtbo: if version >= 1 { Some(vec![9; 100]) } else { None },
            dtb: if version == 2 { Some(vec![7; 300]) } else { None },
        }
    }

    #[test]
    fn roundtrip_v01_v2() {
        for v in [0u32, 1, 2] {
            let img = sample(v);
            let bytes = img.to_bytes().unwrap();
            assert_eq!(&bytes[..8], MAGIC);
            let back = BootImage::parse(&bytes[..]).unwrap();
            assert_eq!(back.version, v);
            assert_eq!(back.kernel, img.kernel);
            assert_eq!(back.ramdisk, img.ramdisk);
            assert_eq!(back.cmdline, img.cmdline);
            assert_eq!(back.kernel_addr, 0x8000);
            assert_eq!(back.tags_addr, 0x100);
            if v == 2 {
                assert_eq!(back.dtb.as_deref(), Some(&[7u8; 300][..]));
                assert_eq!(back.dtb_addr, 9);
            }
            if v >= 1 {
                assert_eq!(back.recovery_dtbo_offset, 7);
            }
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(BootImage::parse(&[0u8; 10][..]).is_err());
        let mut b = sample(1).to_bytes().unwrap();
        b.truncate(5000); // cut inside kernel
        assert!(BootImage::parse(&b[..]).is_err());
    }

    #[test]
    fn v3_parses() {
        let mut h = vec![0u8; V3_HDR];
        h[..8].copy_from_slice(MAGIC);
        let ks = 10u32;
        h[8..12].copy_from_slice(&ks.to_be_bytes()); // kernel_size
        h[12..16].copy_from_slice(&5u32.to_be_bytes()); // ramdisk_size
        h[20..24].copy_from_slice(&(V3_HDR as u32).to_be_bytes()); // header_size
        h[24..28].copy_from_slice(&3u32.to_be_bytes()); // header_version
        h[44..50].copy_from_slice(b"hello ");
        let mut img = h;
        img.resize(3 * 4096, 0); // header page + kernel page + ramdisk page
        img[4096..4096 + 10].fill(42); // kernel bytes
        img[4096 + 4096..4096 + 4096 + 5].fill(77); // ramdisk bytes
        let p = BootImage::parse(&img[..]).unwrap();
        assert_eq!(p.version, 3);
        assert_eq!(p.kernel, vec![42u8; 10]);
        assert_eq!(p.ramdisk, Some(vec![77u8; 5]));
    }
}
