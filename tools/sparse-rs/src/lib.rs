//! Android sparse image format (system/core/libsparse) reader and writer.
//! Little-endian on-disk. Chunks: RAW 0xCAC1, FILL 0xCAC2, DONT_CARE 0xCAC3, CRC32 0xCAC4.

use std::io::{self, Write};

pub const SPARSE_MAGIC: u32 = 0xED26FF3A;
const RAW: u16 = 0xCAC1;
const FILL: u16 = 0xCAC2;
const DONT_CARE: u16 = 0xCAC3;
const CRC32: u16 = 0xCAC4;
/// Hard ceiling for a decoded raw image; the project rootfs is 8 GiB.
const MAX_RAW_BYTES: u64 = 64 << 30;

#[derive(Debug)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

fn err<T>(m: impl Into<String>) -> Result<T> {
    Err(Error(m.into()))
}

fn le32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn le16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn chunk_fill_value(c: &Chunk) -> u32 {
    match c {
        Chunk::Fill { value, .. } => *value,
        _ => 0,
    }
}

#[derive(Debug)]
enum Chunk {
    Raw(Vec<u8>),
    Fill { value: u32, blocks: u32 },
    DontCare(u32), // blocks
}

/// Convert Android sparse image bytes to raw bytes.
pub fn sparse_to_raw(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 28 || le32(data, 0) != SPARSE_MAGIC {
        return err("not a sparse image (bad magic)");
    }
    let _major = le16(data, 4);
    let _minor = le16(data, 6);
    let file_hdr = le16(data, 8) as usize;
    let chunk_hdr = le16(data, 10) as usize;
    let blk_sz = le32(data, 12) as usize;
    let total_blks = le32(data, 16);
    let _total_chunks = le32(data, 20);
    if blk_sz == 0 || !blk_sz.is_power_of_two() || file_hdr < 28 || chunk_hdr < 12 {
        return err(format!("bad header: blk={blk_sz}"));
    }
    let want = (total_blks as usize)
        .checked_mul(blk_sz)
        .ok_or_else(|| Error("declared size overflows".into()))?;
    if want as u64 > MAX_RAW_BYTES {
        return err(format!(
            "declared size {want} exceeds limit {MAX_RAW_BYTES}"
        ));
    }
    let mut out = vec![0u8; want];
    let mut pos = file_hdr;
    let mut out_off = 0usize;
    while pos + chunk_hdr <= data.len() {
        let ctype = le16(data, pos);
        let blocks = le32(data, pos + 4) as usize;
        let total_sz = le32(data, pos + 8) as usize;
        if total_sz < chunk_hdr || pos + total_sz > data.len() {
            return err("truncated/corrupt chunk header");
        }
        let body = &data[pos + chunk_hdr..pos + total_sz];
        match ctype {
            RAW => {
                let need = blocks * blk_sz;
                if out_off + need > out.len() {
                    return err("RAW overruns declared size");
                }
                if body.len() != need {
                    return err("RAW chunk length mismatch");
                }
                out[out_off..out_off + need].copy_from_slice(body);
                out_off += need;
            }
            FILL => {
                let need = blocks * blk_sz;
                if body.len() != 4 || out_off + need > out.len() {
                    return err("bad FILL chunk");
                }
                let word = [body[0], body[1], body[2], body[3]];
                for i in 0..need {
                    out[out_off + i] = word[i % 4];
                }
                out_off += need;
            }
            DONT_CARE => {
                let need = blocks * blk_sz;
                if out_off + need > out.len() {
                    return err("DONT_CARE overruns declared size");
                }
                out_off += need;
            }
            CRC32 => {}
            _ => return err(format!("unknown chunk type {ctype:#06x}")),
        }
        pos += total_sz;
    }
    if out_off != out.len() {
        return err(format!(
            "chunks cover {out_off} of {} declared bytes",
            out.len()
        ));
    }
    Ok(out)
}

/// Convert raw bytes to a sparse image. Zeros -> DONT_CARE, uniform-word
/// repeats -> FILL, everything else -> RAW.
pub fn raw_to_sparse(raw: &[u8], block_size: u32) -> Result<Vec<u8>> {
    let bs = block_size as usize;
    if !bs.is_power_of_two() || bs < 512 {
        return err("block size must be power of two >= 512");
    }
    if !raw.len().is_multiple_of(bs) {
        return err("raw size must be multiple of block size");
    }

    let mut chunks: Vec<Chunk> = Vec::new();
    for block in raw.chunks(bs) {
        let c = if block.iter().all(|&b| b == 0) {
            Chunk::DontCare(1)
        } else if bs.is_multiple_of(4) && block.chunks(4).all(|w| w == &block[..4]) {
            Chunk::Fill {
                value: le32(block, 0),
                blocks: 1,
            }
        } else {
            Chunk::Raw(block.to_vec())
        };
        match (&c, chunks.last_mut()) {
            (Chunk::DontCare(n), Some(Chunk::DontCare(m))) => *m += n,
            (Chunk::Fill { blocks: n, .. }, Some(Chunk::Fill { value, blocks: m }))
                if *value == chunk_fill_value(&c) =>
            {
                *m += n
            }
            _ => chunks.push(c),
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(&SPARSE_MAGIC.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // major
    out.extend_from_slice(&0u16.to_le_bytes()); // minor
    out.extend_from_slice(&28u16.to_le_bytes()); // file_hdr_sz
    out.extend_from_slice(&12u16.to_le_bytes()); // chunk_hdr_sz
    out.extend_from_slice(&(block_size).to_le_bytes());
    out.extend_from_slice(&((raw.len() / bs) as u32).to_le_bytes());
    out.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // image checksum

    for c in &chunks {
        match c {
            Chunk::Raw(d) => {
                out.extend_from_slice(&RAW.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&((d.len() / bs) as u32).to_le_bytes());
                out.extend_from_slice(&((12 + d.len()) as u32).to_le_bytes());
                out.extend_from_slice(d);
            }
            Chunk::Fill { value, blocks } => {
                out.extend_from_slice(&FILL.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&blocks.to_le_bytes());
                out.extend_from_slice(&16u32.to_le_bytes());
                out.extend_from_slice(&value.to_le_bytes());
            }
            Chunk::DontCare(blocks) => {
                out.extend_from_slice(&DONT_CARE.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&blocks.to_le_bytes());
                out.extend_from_slice(&12u32.to_le_bytes());
            }
        }
    }
    Ok(out)
}

/// Write buffer to any writer.
pub fn write_all<W: Write>(mut w: W, data: &[u8]) -> io::Result<()> {
    w.write_all(data)?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_compress() {
        let mut raw = vec![0u8; 4096 * 10]; // dont-care region
        raw.extend_from_slice(&[0xAAu8; 4096 * 3]); // fill region
        raw.extend_from_slice(
            &(0..1024u32)
                .flat_map(|i| i.to_le_bytes())
                .collect::<Vec<u8>>(),
        ); // raw region
        raw.extend([0xEFu8; 4096]); // another fill block

        let sparse = raw_to_sparse(&raw, 4096).unwrap();
        assert_eq!(le32(&sparse, 0), SPARSE_MAGIC);
        assert!(sparse.len() < raw.len(), "sparse must compress this input");

        let back = sparse_to_raw(&sparse).unwrap();
        assert_eq!(back.len(), raw.len());
        // DONT_CARE regions decode as zeros; original zeros there too.
        for (a, b) in raw.chunks(4096).zip(back.chunks(4096)) {
            if a.iter().all(|&x| x == 0) {
                assert!(b.iter().all(|&x| x == 0));
            } else {
                assert_eq!(a, b);
            }
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(sparse_to_raw(&[0u8; 40]).is_err());
        assert!(raw_to_sparse(&[1, 2, 3], 4096).is_err());
    }

    #[test]
    fn fill_words_little_endian() {
        let mut raw = vec![0u8; 4096];
        raw[0..4].copy_from_slice(&0xDEADBEEFu32.to_le_bytes()); // uniform word block
        let sp = raw_to_sparse(&raw, 4096).unwrap();
        let back = sparse_to_raw(&sp).unwrap();
        assert_eq!(back, raw);
    }

    fn header(total_blks: u32) -> Vec<u8> {
        let mut h = Vec::new();
        h.extend_from_slice(&SPARSE_MAGIC.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes());
        h.extend_from_slice(&28u16.to_le_bytes());
        h.extend_from_slice(&12u16.to_le_bytes());
        h.extend_from_slice(&4096u32.to_le_bytes());
        h.extend_from_slice(&total_blks.to_le_bytes());
        h.extend_from_slice(&0u32.to_le_bytes());
        h.extend_from_slice(&0u32.to_le_bytes());
        h
    }

    #[test]
    fn rejects_incomplete_coverage() {
        let mut data = header(2);
        data.extend_from_slice(&DONT_CARE.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes()); // 1 of 2 blocks
        data.extend_from_slice(&12u32.to_le_bytes());
        assert!(sparse_to_raw(&data).is_err());
    }

    #[test]
    fn rejects_unknown_chunk_type() {
        let mut data = header(1);
        data.extend_from_slice(&0xBEEFu16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&12u32.to_le_bytes());
        assert!(sparse_to_raw(&data).is_err());
    }

    #[test]
    fn rejects_absurd_declared_size() {
        let data = header(0x0200_0000);
        assert!(sparse_to_raw(&data).is_err());
    }
}
