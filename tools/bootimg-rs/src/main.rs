use std::path::PathBuf;
use std::process::ExitCode;

#[derive(clap::ValueEnum, Clone, Copy)]
enum Profile {
    /// boot_pve_*.img — mainline kernel, must carry the serial console cmdline
    Kernel,
    /// uefi_*.img — EDK2 payload, boots its own console (edk2-msm leaves cmdline empty)
    Uefi,
}

use bootimg_rs::{BootImage, unpack};

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
    /// Check a boot image against the whyred/lavender ABL expectations
    /// (page size, load offsets, cmdline, partition-size budget)
    Validate {
        image: PathBuf,
        /// refuse images larger than the boot partition
        #[arg(long, default_value_t = 64)]
        max_mib: u64,
        /// Plan B (mainline kernel) or Plan A (EDK2 UEFI payload)
        #[arg(long, value_enum, default_value_t = Profile::Kernel)]
        profile: Profile,
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
            Ok(format!(
                "unpacked {} components to {}",
                files.len(),
                out.display()
            ))
        }
        Cmd::Validate {
            image,
            max_mib,
            profile,
        } => {
            let img = BootImage::parse(std::fs::File::open(&image)?)?;
            let len = std::fs::metadata(&image)?.len();
            let mut problems = Vec::new();
            if len > max_mib << 20 {
                problems.push(format!("image {len} B exceeds the {max_mib} MiB budget"));
            }
            if img.version > 2 {
                problems.push(format!("header version {} (ABL wants 0..2)", img.version));
            }
            let page = img.page_size;
            if !page.is_power_of_two() || !(512..=1 << 21).contains(&page) {
                problems.push(format!("page_size {page} is not a sane power of two"));
            }
            if img.kernel_addr & (page - 1) != 0 {
                problems.push(format!(
                    "kernel_addr {:#x} is not page aligned",
                    img.kernel_addr
                ));
            }
            if img.second.is_none() && img.second_addr != 0 {
                problems.push(format!(
                    "second_addr {:#x} set although no second stage is present",
                    img.second_addr
                ));
            }
            match profile {
                Profile::Kernel => {
                    if !img.cmdline.contains("console=ttyMSM0") {
                        problems.push(
                            "cmdline has no console=ttyMSM0 (serial bring-up would be blind)"
                                .into(),
                        );
                    }
                    if !img.cmdline.contains("root=") {
                        problems
                            .push("cmdline has no root= (Plan B boots the kernel directly)".into());
                    }
                    if page != 4096 {
                        problems.push(format!(
                            "page_size {page}, pmOS deviceinfo says 4096 for Plan B"
                        ));
                    }
                    if img.kernel_addr != 0x8000 {
                        problems.push(format!(
                            "kernel_addr {:#x}, expected 0x8000",
                            img.kernel_addr
                        ));
                    }
                }
                Profile::Uefi => {
                    if page > 1 << 20 {
                        problems.push(format!(
                            "page_size {page} is unusually large for a UEFI payload"
                        ));
                    }
                }
            }
            if img.kernel.is_empty() {
                problems.push("empty kernel payload".into());
            }
            if problems.is_empty() {
                Ok(format!(
                    "OK {}: v{}, page {page}, kernel {} B at {:#x}, {} sections",
                    image.display(),
                    img.version,
                    img.kernel.len(),
                    img.kernel_addr,
                    1 + usize::from(img.ramdisk.is_some())
                        + usize::from(img.second.is_some())
                        + usize::from(img.dtb.is_some())
                        + usize::from(img.recovery_dtbo.is_some())
                ))
            } else {
                Err(problems.join("; ").into())
            }
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
