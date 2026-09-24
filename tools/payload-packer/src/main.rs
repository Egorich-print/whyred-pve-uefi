//! Wrap an EDK2 UEFI FD (or kernel) payload into a boot.img that ABL on
//! whyred accepts. Defaults mirror postmarketOS device-xiaomi-whyred
//! deviceinfo + edk2-msm configs/devices/whyred.conf.

use std::path::PathBuf;
use std::process::ExitCode;

use bootimg_rs::BootImage;

/// whyred defaults (docs/01-partitions.md)
const BASE: u32 = 0x0000_0000;
const KERNEL_OFF: u32 = 0x0000_8000;
const RAMDISK_OFF: u32 = 0x0100_0000;
const TAGS_OFF: u32 = 0x0000_0100;
const PAGESIZE: u32 = 4096;
/// boot partition size cap: refuse to build images ABL would truncate
const BOOT_PARTITION_MAX: usize = 64 * 1024 * 1024;

#[derive(clap::Parser)]
#[command(about, version)]
struct Args {
    /// UEFI .fd or raw kernel payload
    payload: PathBuf,
    #[arg(short, long, default_value = "uefi_whyred.img")]
    out: PathBuf,
    /// optional ramdisk (e.g. initramfs for direct-boot kernels)
    #[arg(long)]
    ramdisk: Option<PathBuf>,
    #[arg(long, default_value_t = 1)]
    header_version: u32,
    /// extra cmdline appended after the stock one
    #[arg(long, default_value = "")]
    cmdline_extra: String,
    /// boot.img header name field
    #[arg(long, default_value = "whyred-pve-uefi")]
    name: String,
}

fn main() -> ExitCode {
    let args: Args = clap::Parser::parse();
    match run(args) {
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

fn run(a: Args) -> Result<String, Box<dyn std::error::Error>> {
    let payload = std::fs::read(&a.payload)?;
    if payload.is_empty() {
        return Err("empty payload would produce a header-only image".into());
    }
    if payload.len() > BOOT_PARTITION_MAX - PAGESIZE as usize {
        return Err(format!(
            "payload {} bytes exceeds boot partition budget ({BOOT_PARTITION_MAX})",
            payload.len()
        )
        .into());
    }
    let cmdline = format!(
        "console=ttyMSM0,115200n8 androidboot.hardware=qcom \
         androidboot.usbcontroller=a800000.dwc3 {extra}",
        extra = a.cmdline_extra
    );
    let img = BootImage {
        version: a.header_version,
        kernel_size: 0,
        kernel_addr: BASE.wrapping_add(KERNEL_OFF),
        ramdisk_size: 0,
        ramdisk_addr: BASE.wrapping_add(RAMDISK_OFF),
        second_size: 0,
        second_addr: 0,
        tags_addr: BASE.wrapping_add(TAGS_OFF),
        page_size: PAGESIZE,
        os_version: 11 << 14, // A<<14 | B<<7 | C → 11.0.0
        name: a.name.clone(),
        cmdline,
        extra_cmdline: String::new(),
        id: [0; 8],
        recovery_dtbo_size: 0,
        recovery_dtbo_offset: 0,
        dtb_size: 0,
        dtb_addr: 0,
        signature_size: 0,
        kernel: payload,
        ramdisk: a.ramdisk.map(std::fs::read).transpose()?,
        second: None,
        recovery_dtbo: None,
        dtb: None,
    };
    let bytes = img.to_bytes()?;
    if bytes.len() > BOOT_PARTITION_MAX {
        return Err("final image exceeds boot partition".into());
    }
    std::fs::write(&a.out, bytes)?;
    Ok(format!(
        "packed {} -> {} ({} bytes)",
        a.payload.display(),
        a.out.display(),
        a.out.metadata()?.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(payload: &str, out: &str) -> Args {
        Args {
            payload: payload.into(),
            out: out.into(),
            ramdisk: None,
            header_version: 1,
            cmdline_extra: String::new(),
            name: "test".into(),
        }
    }

    #[test]
    fn rejects_empty_and_oversized_payloads() {
        let dir = std::env::temp_dir().join("payload-packer-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let empty = dir.join("empty.bin");
        let out = dir.join("out.img");
        std::fs::write(&empty, b"").unwrap();
        assert!(run(args(empty.to_str().unwrap(), out.to_str().unwrap())).is_err());

        let big = dir.join("big.bin");
        let f = std::fs::File::create(&big).unwrap();
        f.set_len(BOOT_PARTITION_MAX as u64).unwrap();
        drop(f);
        assert!(run(args(big.to_str().unwrap(), out.to_str().unwrap())).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn packs_small_payload() {
        let dir = std::env::temp_dir().join("payload-packer-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let payload = dir.join("payload.bin");
        let out = dir.join("out.img");
        std::fs::write(&payload, vec![0xA5u8; 5000]).unwrap();
        run(args(payload.to_str().unwrap(), out.to_str().unwrap())).unwrap();
        let img = bootimg_rs::BootImage::parse(&std::fs::read(&out).unwrap()[..]).unwrap();
        assert_eq!(img.kernel.len(), 5000);
        assert_eq!(img.second_size, 0);
        assert_eq!(img.second_addr, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
