use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sparse_rs::{raw_to_sparse, read_header, sparse_to_raw};

#[derive(clap::Parser)]
enum Cmd {
    #[command(name = "simg2img")]
    /// Sparse image -> raw image (like simg2img)
    SimG2Img {
        input: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
    },
    #[command(name = "info")]
    /// Validate a sparse image header and print its logical size (no decoding)
    Info { input: PathBuf },
    #[command(name = "img2simg")]
    /// Raw image -> sparse image (like img2simg)
    Img2Simg {
        input: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, default_value_t = 4096)]
        block_size: u32,
    },
}

fn simg2img(input: &Path, out: &Path) -> Result<String, String> {
    let data = std::fs::read(input).map_err(|e| e.to_string())?;
    let raw = sparse_to_raw(&data[..]).map_err(|e| e.0)?;
    let mut f = std::fs::File::create(out).map_err(|e| e.to_string())?;
    f.write_all(&raw).map_err(|e| e.to_string())?;
    Ok(format!(
        "{} -> {} ({} bytes)",
        input.display(),
        out.display(),
        raw.len()
    ))
}

fn img2simg(input: &Path, out: &Path, block_size: u32) -> Result<String, String> {
    let len = std::fs::metadata(input).map_err(|e| e.to_string())?.len();
    let src = std::fs::File::open(input).map_err(|e| e.to_string())?;
    let dst = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let raw_len = raw_to_sparse(src, len, block_size, dst).map_err(|e| e.0)?;
    let sparse_len = std::fs::metadata(out).map_err(|e| e.to_string())?.len();
    Ok(format!(
        "{} ({raw_len} B) -> {} ({sparse_len} B sparse)",
        input.display(),
        out.display()
    ))
}

fn sparse_info(input: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(input).map_err(|e| e.to_string())?;
    let mut head = [0u8; 28];
    f.read_exact(&mut head).map_err(|e| e.to_string())?;
    let h = read_header(&head).map_err(|e| e.0)?;
    Ok(format!(
        "{}: block_size={} total_blocks={} logical={} B ({} MiB)",
        input.display(),
        h.block_size,
        h.total_blocks,
        h.logical_size,
        h.logical_size / (1 << 20)
    ))
}

fn main() -> ExitCode {
    let cmd: Cmd = clap::Parser::parse();
    let res = match cmd {
        Cmd::Info { input } => sparse_info(&input),
        Cmd::SimG2Img { input, out } => simg2img(&input, &out),
        Cmd::Img2Simg {
            input,
            out,
            block_size,
        } => img2simg(&input, &out, block_size),
    };
    match res {
        Ok(m) => {
            println!("{m}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
