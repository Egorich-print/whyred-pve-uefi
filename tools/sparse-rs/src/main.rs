use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use sparse_rs::{raw_to_sparse, sparse_to_raw};

#[derive(clap::Parser)]
enum Cmd {
    #[command(name = "simg2img")]
    /// Sparse image -> raw image (like simg2img)
    SimG2Img {
        input: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
    },
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

fn main() -> ExitCode {
    let cmd: Cmd = clap::Parser::parse();
    let res = match cmd {
        Cmd::SimG2Img { input, out } => std::fs::read(&input)
            .map_err(|e| e.to_string())
            .and_then(|d| {
                sparse_to_raw(&d[..]).map_err(|e| e.0)
            })
            .and_then(|raw| {
                std::fs::File::create(&out)
                    .and_then(|mut f| f.write_all(&raw))
                    .map_err(|e| e.to_string())
                    .map(|_| format!("{} -> {} ({} bytes)", input.display(), out.display(), raw.len()))
            }),
        Cmd::Img2Simg { input, out, block_size } => std::fs::read(&input)
            .map_err(|e| e.to_string())
            .and_then(|d| raw_to_sparse(&d, block_size).map_err(|e| e.0))
            .and_then(|sp| {
                std::fs::File::create(&out)
                    .and_then(|mut f| f.write_all(&sp))
                    .map_err(|e| e.to_string())
                    .map(|_| format!("{} -> {} ({} bytes)", input.display(), out.display(), sp.len()))
            }),
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
