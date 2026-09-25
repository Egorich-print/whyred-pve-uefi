//! Android sparse image format (system/core/libsparse) reader and writer.
//! Little-endian on-disk. Chunks: RAW 0xCAC1, FILL 0xCAC2, DONT_CARE 0xCAC3, CRC32 0xCAC4.

use std::io::{Read, Seek, Write};

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

/// Header facts of a sparse image, read without decoding it.
pub struct SparseHeader {
    pub block_size: u32,
    pub total_blocks: u32,
    pub logical_size: u64,
}

/// Validate and read a sparse image header. Cheap: reads 28 bytes only, so it
/// can gate a multi-gigabyte flash before any bytes are sent.
pub fn read_header(data: &[u8]) -> Result<SparseHeader> {
    if data.len() < 28 || le32(data, 0) != SPARSE_MAGIC {
        return err("not a sparse image (bad magic)");
    }
    let file_hdr = le16(data, 8) as usize;
    let chunk_hdr = le16(data, 10) as usize;
    let block_size = le32(data, 12);
    let total_blocks = le32(data, 16);
    if block_size == 0 || !block_size.is_power_of_two() || file_hdr < 28 || chunk_hdr < 12 {
        return err(format!("bad header: block_size={block_size}"));
    }
    let logical_size = (total_blocks as u64).saturating_mul(block_size as u64);
    if logical_size > MAX_RAW_BYTES {
        return err(format!(
            "declared size {logical_size} exceeds limit {MAX_RAW_BYTES}"
        ));
    }
    Ok(SparseHeader {
        block_size,
        total_blocks,
        logical_size,
    })
}

/// Convert Android sparse image bytes to raw bytes.
pub fn sparse_to_raw(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 28 || le32(data, 0) != SPARSE_MAGIC {
        return err("not a sparse image (bad magic)");
    }
    let file_hdr = le16(data, 8) as usize;
    let chunk_hdr = le16(data, 10) as usize;
    let blk_sz = le32(data, 12) as usize;
    let total_blks = le32(data, 16);
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

/// Convert a raw image into a sparse image without holding it in memory.
///
/// `raw_len` must be the exact length of the stream; the sparse header needs
/// the block count up front, and a mismatch is rejected rather than producing
/// an image that decodes to the wrong size. Memory use is one block plus a
/// pending RAW chunk (capped), so an 8 GiB rootfs converts in ~64 MiB.
pub fn raw_to_sparse<R: Read, W: Write + Seek>(
    mut source: R,
    raw_len: u64,
    block_size: u32,
    mut out: W,
) -> Result<u64> {
    const MAX_RAW_CHUNK: usize = 64 << 20;
    let bs = block_size as usize;
    if !bs.is_power_of_two() || bs < 512 {
        return err("block size must be power of two >= 512");
    }
    if raw_len == 0 || !raw_len.is_multiple_of(bs as u64) {
        return err("raw size must be a non-zero multiple of block size");
    }
    if raw_len > MAX_RAW_BYTES {
        return err(format!("raw size {raw_len} exceeds limit {MAX_RAW_BYTES}"));
    }
    let total_blks = raw_len / bs as u64;
    if total_blks > u32::MAX as u64 {
        return err("too many blocks for the 32-bit sparse header");
    }

    let mut header = Vec::with_capacity(28);
    header.extend_from_slice(&SPARSE_MAGIC.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes()); // major
    header.extend_from_slice(&0u16.to_le_bytes()); // minor
    header.extend_from_slice(&28u16.to_le_bytes()); // file_hdr_sz
    header.extend_from_slice(&12u16.to_le_bytes()); // chunk_hdr_sz
    header.extend_from_slice(&block_size.to_le_bytes());
    header.extend_from_slice(&(total_blks as u32).to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes()); // total_chunks, patched below
    header.extend_from_slice(&0u32.to_le_bytes()); // image checksum
    let chunks_at = 20usize;
    out.write_all(&header).map_err(io_err)?;

    let mut chunks: u32 = 0;
    let mut block = vec![0u8; bs];
    let mut pending: Option<Pending> = None;
    let mut read_total: u64 = 0;

    let flush = |pending: &mut Option<Pending>, out: &mut W, chunks: &mut u32| -> Result<()> {
        let Some(p) = pending.take() else {
            return Ok(());
        };
        p.write_to(out).map_err(io_err)?;
        *chunks += 1;
        Ok(())
    };

    while read_total < raw_len {
        let mut filled = 0usize;
        while filled < bs {
            let n = source.read(&mut block[filled..]).map_err(io_err)?;
            if n == 0 {
                return err(format!(
                    "input ended after {read_total} bytes, expected {raw_len}"
                ));
            }
            filled += n;
        }
        read_total += bs as u64;

        let kind = classify(&block);
        match &mut pending {
            Some(p) if p.same_as(&kind) => p.push(&block),
            _ => {
                flush(&mut pending, &mut out, &mut chunks)?;
                let mut p = Pending::new(kind);
                p.push(&block);
                pending = Some(p);
            }
        }
        if pending.as_ref().is_some_and(|p| p.len() >= MAX_RAW_CHUNK) {
            flush(&mut pending, &mut out, &mut chunks)?;
        }
    }
    flush(&mut pending, &mut out, &mut chunks)?;
    out.flush().map_err(io_err)?;

    if out
        .seek(std::io::SeekFrom::Start(chunks_at as u64))
        .and_then(|_| {
            let mut buf = [0u8; 4];
            buf.copy_from_slice(&chunks.to_le_bytes());
            out.write_all(&buf)
        })
        .is_err()
    {
        return err("output is not seekable: cannot patch the chunk count");
    }
    Ok(read_total)
}

#[derive(PartialEq, Clone, Copy)]
enum Kind {
    DontCare,
    Fill(u32),
    Raw,
}

fn classify(block: &[u8]) -> Kind {
    if block.iter().all(|&b| b == 0) {
        Kind::DontCare
    } else if block.chunks(4).all(|w| w == &block[..4]) {
        Kind::Fill(le32(block, 0))
    } else {
        Kind::Raw
    }
}

/// One chunk being accumulated: counters for the trivial kinds, a buffer for RAW.
struct Pending {
    kind: Kind,
    blocks: u32,
    data: Vec<u8>,
}

impl Pending {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            blocks: 0,
            data: Vec::new(),
        }
    }
    fn same_as(&self, kind: &Kind) -> bool {
        self.kind == *kind
    }
    fn push(&mut self, block: &[u8]) {
        self.blocks += 1;
        if self.kind == Kind::Raw {
            self.data.extend_from_slice(block);
        }
    }
    fn len(&self) -> usize {
        if self.kind == Kind::Raw {
            self.data.len()
        } else {
            0
        }
    }
    fn write_to<W: Write>(&self, out: &mut W) -> std::io::Result<()> {
        match self.kind {
            Kind::DontCare => {
                out.write_all(&DONT_CARE.to_le_bytes())?;
                out.write_all(&0u16.to_le_bytes())?;
                out.write_all(&self.blocks.to_le_bytes())?;
                out.write_all(&12u32.to_le_bytes())
            }
            Kind::Fill(value) => {
                out.write_all(&FILL.to_le_bytes())?;
                out.write_all(&0u16.to_le_bytes())?;
                out.write_all(&self.blocks.to_le_bytes())?;
                out.write_all(&16u32.to_le_bytes())?;
                out.write_all(&value.to_le_bytes())
            }
            Kind::Raw => {
                out.write_all(&RAW.to_le_bytes())?;
                out.write_all(&0u16.to_le_bytes())?;
                out.write_all(&self.blocks.to_le_bytes())?;
                out.write_all(&((12 + self.data.len()) as u32).to_le_bytes())?;
                out.write_all(&self.data)
            }
        }
    }
}

fn io_err(e: std::io::Error) -> Error {
    Error(format!("io: {e}"))
}

/// Convert an in-memory raw image (convenience wrapper around the streaming
/// encoder; used by tests and small images).
pub fn raw_to_sparse_bytes(raw: &[u8], block_size: u32) -> Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    raw_to_sparse(raw, raw.len() as u64, block_size, &mut out)?;
    Ok(out.into_inner())
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

        let sparse = raw_to_sparse_bytes(&raw, 4096).unwrap();
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
        assert!(raw_to_sparse_bytes(&[1, 2, 3], 4096).is_err());
    }

    #[test]
    fn fill_words_little_endian() {
        let mut raw = vec![0u8; 4096];
        raw[0..4].copy_from_slice(&0xDEADBEEFu32.to_le_bytes()); // uniform word block
        let sp = raw_to_sparse_bytes(&raw, 4096).unwrap();
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
        assert!(raw_to_sparse_bytes(&[], 4096).is_err());
        assert!(raw_to_sparse_bytes(&[0u8; 8192], 3000).is_err());
    }

    #[test]
    fn streaming_encoder_handles_mixed_runs() {
        let mut raw: Vec<u8> = Vec::new();
        raw.extend(vec![0u8; 4096 * 5]); // dont-care
        raw.extend(vec![0xAAu8; 4096 * 3]); // fill
        raw.extend((0..4096 * 2).map(|i| (i % 251) as u8).collect::<Vec<_>>()); // raw
        raw.extend(vec![0x11u8; 4096]); // single fill
        raw.extend(vec![0u8; 4096 * 2]); // dont-care again
        let sparse = raw_to_sparse_bytes(&raw, 4096).unwrap();
        assert!(
            sparse.len() < raw.len() / 2,
            "should compress: {}",
            sparse.len()
        );
        let back = sparse_to_raw(&sparse).unwrap();
        assert_eq!(back, raw, "streamed sparse must decode to the same bytes");
        assert_eq!(
            le32(&sparse, 20),
            5,
            "chunk count must be patched in the header"
        );
    }

    #[test]
    fn header_is_validated_without_decoding() {
        let raw = vec![0u8; 4096 * 3];
        let sp = raw_to_sparse_bytes(&raw, 4096).unwrap();
        let h = read_header(&sp).unwrap();
        assert_eq!(h.block_size, 4096);
        assert_eq!(h.total_blocks, 3);
        assert_eq!(h.logical_size, 12288);
        assert!(read_header(&sp[..20]).is_err());
        assert!(read_header(&[0u8; 28]).is_err());
        let mut bad = sp.clone();
        bad[12..16].copy_from_slice(&3000u32.to_le_bytes());
        assert!(read_header(&bad).is_err());
    }

    #[test]
    fn streaming_encoder_rejects_short_input() {
        // declared 8192 but the stream ends after 4096
        let data = vec![0u8; 4096];
        let mut out = std::io::Cursor::new(Vec::new());
        assert!(raw_to_sparse(&data[..], 8192, 4096, &mut out).is_err());
    }
}
