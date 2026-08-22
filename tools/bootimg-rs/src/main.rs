use std::path::PathBuf;
use std::process::ExitCode;

use bootimg_rs::{unpack, BootImage};

#[derive(clap::Parser)]
enum Cmd {
    /// Show header info
    Info { image: PathBuf },
    /// Unpack components into a directory
    Unpack {
        image: PathBuf,
        #[arg(short, long, default_value = "unpacked")]
        out: PathBuf,
    },
    /// Pack components into a boot image
    Pack {
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long)]
        kernel: PathBuf,
        #[arg(long)]
        ramdisk: Option<PathBuf>,
        #[arg(long)]
        second: Option<PathBuf>,
        #[arg(long)]
        dtb: Option<PathBuf>,
        #[arg(long)]
        recovery_dtbo: Option<PathBuf>,
        /// 0,1,2 (whyred ABL: use 1)
        #[arg(long, default_value_t = 1)]
        header_version: u32,
        #[arg(long, default_value_t = 4096)]
        page_size: u32,
        // whyred defaults from postmarketOS deviceinfo
        #[arg(long, default_value_t = 0x0)]
        base: u32,
        #[arg(long, default_value_t = 0x8000)]
        kernel_offset: u32,
        #[arg(long, default_value_t = 0x1000000)]
        ramdisk_offset: u32,
        #[arg(long, default_value_t = 0x0)]
        second_offset: u32,
        #[arg(long, default_value_t = 0x100)]
        tags_offset: u32,
        #[arg(long, default_value_t = 0)]
        dtb_offset: u64,
        #[arg(long, default_value = "")]
        cmdline: String,
        #[arg(long, default_value = "whyred-pve-uefi")]
        name: String,
    },
}

fn main() -> ExitCode {
    let cmd: Cmd = clap::Parser::parse();
    match run(cmd) {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cmd: Cmd) -> Result<String, Box<dyn std::error::Error>> {
    match cmd {
        Cmd::Info { image } => {
            let img = BootImage::parse(std::fs::File::open(image)?)?;
            Ok(format!("{img:#?}"))
        }
        Cmd::Unpack { image, out } => {
            let img = BootImage::parse(std::fs::File::open(image)?)?;
            let files = unpack(&img, &out)?;
            Ok(format!("unpacked {} components to {}", files.len(), out.display()))
        }
        Cmd::Pack {
            out,
            kernel,
            ramdisk,
            second,
            dtb,
            recovery_dtbo,
            header_version,
            page_size,
            base,
            kernel_offset,
            ramdisk_offset,
            second_offset,
            tags_offset,
            dtb_offset,
            cmdline,
            name,
        } => {
            let img = BootImage {
                version: header_version,
                kernel_size: 0,
                kernel_addr: base.wrapping_add(kernel_offset),
                ramdisk_size: 0,
                ramdisk_addr: base.wrapping_add(ramdisk_offset),
                second_size: 0,
                second_addr: base.wrapping_add(second_offset),
                tags_addr: base.wrapping_add(tags_offset),
                page_size,
                os_version: (11 << 11) | ((2020 - 2000) << 4), // os 11.0.0, patch 2020-12
                name,
                cmdline,
                extra_cmdline: String::new(),
                id: [0; 8],
                recovery_dtbo_size: 0,
                recovery_dtbo_offset: 0,
                dtb_size: 0,
                dtb_addr: dtb_offset,
                signature_size: 0,
                kernel: std::fs::read(kernel)?,
                ramdisk: ramdisk.map(std::fs::read).transpose()?,
                second: second.map(std::fs::read).transpose()?,
                recovery_dtbo: recovery_dtbo.map(std::fs::read).transpose()?,
                dtb: dtb.map(std::fs::read).transpose()?,
            };
            std::fs::write(&out, img.to_bytes()?)?;
            Ok(format!("wrote {} ({header_version})", out.display()))
        }
    }
}
