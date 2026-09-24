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

#[derive(Clone, Copy, PartialEq)]
enum Endian {
    Be,
    Le,
}

fn rd32(b: &[u8], off: usize, e: Endian) -> u32 {
    let w = [b[off], b[off + 1], b[off + 2], b[off + 3]];
    match e {
        Endian::Be => u32::from_be_bytes(w),
        Endian::Le => u32::from_le_bytes(w),
    }
}
fn be32(b: &[u8], off: usize) -> u32 {
    rd32(b, off, Endian::Be)
}
fn rd64(b: &[u8], off: usize, e: Endian) -> u64 {
    let w = [
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
        b[off + 4],
        b[off + 5],
        b[off + 6],
        b[off + 7],
    ];
    match e {
        Endian::Be => u64::from_be_bytes(w),
        Endian::Le => u64::from_le_bytes(w),
    }
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

fn align(n: usize, page: usize) -> usize {
    n.div_ceil(page) * page
}

/// Does `buf` hold a structurally valid v0/v1/v2 layout when read as `e`?
/// Page size must be sane, the version must fit its header, and every section
/// must lie inside the file. Guessing on `kernel_size` alone is not enough:
/// a little-endian image whose kernel size is a multiple of 256 has a
/// byte-swapped reading that still looks plausible.
fn v02_layout_valid(buf: &[u8], e: Endian) -> bool {
    let r32 = |o: usize| rd32(buf, o, e);
    let page = r32(36) as usize;
    if !page.is_power_of_two() || !(512..=(1 << 30)).contains(&page) || page > buf.len() {
        return false;
    }
    let version = match r32(40) {
        0..=2 => r32(40),
        _ => 0,
    };
    let need = match version {
        2 => V2_HDR,
        1 => V1_HDR,
        _ => V0_HDR,
    };
    if page < need {
        return false;
    }
    let sections = [
        r32(8),
        r32(16),
        r32(24),
        if version >= 1 { r32(V0_HDR) } else { 0 },
        if version == 2 { r32(V1_HDR) } else { 0 },
    ];
    let mut off = page;
    for s in sections {
        match off.checked_add(align(s as usize, page)) {
            Some(next) => off = next,
            None => return false,
        }
    }
    off <= buf.len()
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
        if buf.len() < V0_HDR {
            return err(format!("truncated: {} < {V0_HDR}", buf.len()));
        }
        // v3/v4 are big-endian by spec (AOSP libbootimg boot_img_hdr_v3)
        let hs = rd32(&buf, 28, Endian::Be);
        let ver = rd32(&buf, V3_HDR, Endian::Be);
        if (hs == V3_HDR as u32 || hs == V4_HDR as u32) && (3..=4).contains(&ver) {
            return Self::parse_v34(buf);
        }
        for e in [Endian::Be, Endian::Le] {
            if v02_layout_valid(&buf, e) {
                return Self::parse_v02(buf, e);
            }
        }
        err("no valid v0-v2 header layout (bad page size or section sizes)")
    }

    fn parse_v02(buf: Vec<u8>, e: Endian) -> Result<Self> {
        // AOSP mkbootimg writes big-endian; abootimg (used by edk2-msm/Renegade)
        // writes little-endian. The caller picked `e` by structural validation.
        let r32 = |o: usize| rd32(&buf, o, e);
        let r64 = |o: usize| rd64(&buf, o, e);
        let hdr_ver_field = r32(40);
        let version = match hdr_ver_field {
            0..=2 => hdr_ver_field, // could still be legacy dt_size!=0; treated as v0 payload below
            _ => 0,                 // legacy: field is dt_size
        };
        let need = match version {
            2 => V2_HDR,
            1 => V1_HDR,
            _ => V0_HDR,
        };
        if buf.len() < need {
            return err(format!("truncated: {} < {need}", buf.len()));
        }
        let page = r32(36) as usize;
        // Legacy images may store a real dt blob size in the version slot.
        let (dtb_size_hdr, dtb_blob) = (hdr_ver_field, version == 0 && hdr_ver_field > 2);

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

        let kernel_size = r32(8);
        let kernel = take(kernel_size)?;
        let ramdisk_size = r32(16);
        let ramdisk = if ramdisk_size > 0 {
            Some(take(ramdisk_size)?)
        } else {
            None
        };
        let second_size = r32(24);
        let second = if second_size > 0 {
            Some(take(second_size)?)
        } else {
            None
        };
        let recovery_dtbo_size = if version >= 1 { r32(V0_HDR) } else { 0 };
        let recovery_dtbo = if recovery_dtbo_size > 0 {
            Some(take(recovery_dtbo_size)?)
        } else {
            None
        };
        let dtb_size = if dtb_blob {
            dtb_size_hdr
        } else if version == 2 {
            r32(V1_HDR)
        } else {
            0
        };
        let dtb = if dtb_size > 0 {
            Some(take(dtb_size)?)
        } else {
            None
        };

        Ok(BootImage {
            version,
            kernel_size,
            kernel_addr: r32(12),
            ramdisk_size,
            ramdisk_addr: r32(20),
            second_size,
            second_addr: r32(28),
            tags_addr: r32(32),
            page_size: page as u32,
            os_version: r32(44),
            name: cstr(&buf[48..64]),
            cmdline: cstr(&buf[64..576]),
            extra_cmdline: cstr(&buf[608..1632]),
            id: (0..8)
                .map(|i| r32(576 + i * 4))
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            recovery_dtbo_size,
            recovery_dtbo_offset: if version >= 1 { r64(V0_HDR + 4) } else { 0 },
            dtb_size,
            dtb_addr: if version == 2 { r64(V1_HDR + 4) } else { 0 },
            signature_size: 0,
            kernel,
            ramdisk,
            second,
            recovery_dtbo,
            dtb,
        })
    }

    fn parse_v34(buf: Vec<u8>) -> Result<Self> {
        let version = rd32(&buf, V3_HDR, Endian::Be);
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
        let ramdisk = if ramdisk_size > 0 {
            Some(take(ramdisk_size)?)
        } else {
            None
        };
        let cmdline_size = be32(&buf, V4_HDR) as usize;
        let cmdline_end = if cmdline_size > 0 {
            (44 + cmdline_size).min(V3_HDR)
        } else {
            V3_HDR
        };
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
            cmdline: cstr(&buf[44..cmdline_end]),
            extra_cmdline: String::new(),
            id: [0; 8],
            recovery_dtbo_size: 0,
            recovery_dtbo_offset: 0,
            dtb_size: 0,
            dtb_addr: 0,
            signature_size: if version == 4 { be32(&buf, V4_HDR) } else { 0 },
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
        if !page.is_power_of_two() || page < hdr_size || page > (1 << 30) {
            return err(format!("bad page size {page}"));
        }
        let len32 = |v: &Option<Vec<u8>>| -> Result<u32> {
            v.as_ref().map_or(Ok(0), |d| {
                u32::try_from(d.len()).map_err(|_| Error("section too large".into()))
            })
        };
        let kernel_len =
            u32::try_from(self.kernel.len()).map_err(|_| Error("kernel too large".into()))?;
        let ramdisk_len = len32(&self.ramdisk)?;
        let second_len = len32(&self.second)?;
        let dtbo_len = len32(&self.recovery_dtbo)?;
        let dtb_len = len32(&self.dtb)?;
        let mut h = vec![0u8; V0_HDR];
        h[..8].copy_from_slice(MAGIC);
        let put32 =
            |h: &mut Vec<u8>, off: usize, v: u32| h[off..off + 4].copy_from_slice(&v.to_be_bytes());
        put32(&mut h, 8, kernel_len);
        put32(&mut h, 12, self.kernel_addr);
        put32(&mut h, 16, ramdisk_len);
        put32(&mut h, 20, self.ramdisk_addr);
        put32(&mut h, 24, second_len);
        put32(&mut h, 28, self.second_addr);
        put32(&mut h, 32, self.tags_addr);
        put32(&mut h, 36, self.page_size);
        if self.version == 0 {
            // legacy slot holds dt_size
            put32(&mut h, 40, dtb_len);
        } else {
            put32(&mut h, 40, self.version);
        }
        put32(&mut h, 44, self.os_version);
        let name: Vec<u8> = self
            .name
            .chars()
            .flat_map(|c| c.to_string().into_bytes())
            .take(16)
            .collect();
        h[48..48 + name.len()].copy_from_slice(&name);
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
            tail.extend_from_slice(&dtbo_len.to_be_bytes());
            tail.extend_from_slice(&self.recovery_dtbo_offset.to_be_bytes());
            tail.extend_from_slice(&(hdr_size as u32).to_be_bytes());
            if self.version == 2 {
                tail.extend_from_slice(&dtb_len.to_be_bytes());
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
            recovery_dtbo: if version >= 1 {
                Some(vec![9; 100])
            } else {
                None
            },
            dtb: if version == 2 {
                Some(vec![7; 300])
            } else {
                None
            },
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
        let mut h = vec![0u8; V4_HDR + 4];
        h[..8].copy_from_slice(MAGIC);
        let ks = 10u32;
        h[8..12].copy_from_slice(&ks.to_be_bytes()); // kernel_size
        h[12..16].copy_from_slice(&5u32.to_be_bytes()); // ramdisk_size
        h[28..32].copy_from_slice(&(V3_HDR as u32).to_be_bytes()); // header_size
        h[V3_HDR..V3_HDR + 4].copy_from_slice(&3u32.to_be_bytes()); // header_version
        h[V4_HDR..V4_HDR + 4].copy_from_slice(&7u32.to_be_bytes()); // cmdline_size
        h[44..50].copy_from_slice(b"hello ");
        let mut img = h;
        img.resize(3 * 4096, 0); // header page + kernel page + ramdisk page
        img[4096..4096 + 10].fill(42); // kernel bytes
        img[4096 + 4096..4096 + 4096 + 5].fill(77); // ramdisk bytes
        let p = BootImage::parse(&img[..]).unwrap();
        assert_eq!(p.version, 3);
        assert_eq!(p.kernel, vec![42u8; 10]);
        assert_eq!(p.ramdisk, Some(vec![77u8; 5]));
        assert!(p.cmdline.starts_with("hello"));
    }

    fn byte_swap_headers(bytes: &mut [u8]) {
        for off in [
            8usize, 12, 16, 20, 24, 28, 32, 36, 40, 44, 576, 580, 584, 588, 592, 596, 600, 604,
        ] {
            bytes[off..off + 4].reverse();
        }
    }

    #[test]
    fn little_endian_kernel_multiple_of_256() {
        let mut img = sample(0);
        img.kernel = vec![0x5Au8; 0x0010_0000];
        let mut bytes = img.to_bytes().unwrap();
        byte_swap_headers(&mut bytes);
        let back = BootImage::parse(&bytes[..]).unwrap();
        assert_eq!(back.page_size, 4096);
        assert_eq!(back.kernel.len(), 0x0010_0000);
        assert_eq!(back.kernel, img.kernel);
        assert_eq!(back.kernel_addr, img.kernel_addr);
        assert_eq!(back.cmdline, img.cmdline);
    }

    #[test]
    fn multibyte_name_does_not_panic() {
        let mut img = sample(1);
        img.name = "whyred".into();
        img.name.push('中');
        img.name.push('文');
        img.name.push('字');
        let bytes = img.to_bytes().unwrap();
        let back = BootImage::parse(&bytes[..]).unwrap();
        assert!(back.name.starts_with("whyred"));
    }

    #[test]
    fn rejects_bad_page_size() {
        let mut img = sample(1);
        img.page_size = 0;
        assert!(img.to_bytes().is_err());
        img.page_size = 3000;
        assert!(img.to_bytes().is_err());
    }
}
