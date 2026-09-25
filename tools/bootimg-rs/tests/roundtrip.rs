//! Round-trip property tests: generated images must survive pack→parse and
//! keep every section byte-identical. Deterministic LCG, no dependencies.

use bootimg_rs::BootImage;

/// xorshift64* — deterministic, no dev-dependency
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| (self.next() >> 33) as u8).collect()
    }
}

fn sample(version: u32, page: u32, rng: &mut Rng) -> BootImage {
    let dtb = version == 2;
    let dtbo = version >= 1;
    let klen = 1 + (rng.next() % 9000) as usize;
    let rlen = 1 + (rng.next() % 5000) as usize;
    let slen = (rng.next() % 300) as usize;
    let blen = (rng.next() % 400) as usize;
    let kernel = rng.bytes(klen);
    let ramdisk = rng.bytes(rlen);
    let second = vec![0x5A; slen];
    let dtbo_bytes = rng.bytes(blen);
    let dtb_bytes = rng.bytes(700);
    BootImage {
        version,
        kernel_size: 0,
        kernel_addr: 0x8000,
        ramdisk_size: 0,
        ramdisk_addr: 0x1000000,
        second_size: 0,
        second_addr: 0,
        tags_addr: 0x100,
        page_size: page,
        os_version: 11 << 14,
        name: "whyred-pve".into(),
        cmdline: "console=ttyMSM0,115200n8 root=PARTLABEL=userdata".into(),
        extra_cmdline: "quiet".into(),
        id: [1, 2, 3, 4, 5, 6, 7, 8],
        recovery_dtbo_size: 0,
        recovery_dtbo_offset: 0x1234,
        dtb_size: 0,
        dtb_addr: 0x5678,
        signature_size: 0,
        kernel,
        ramdisk: Some(ramdisk),
        second: if second.is_empty() {
            None
        } else {
            Some(second)
        },
        recovery_dtbo: if dtbo { Some(dtbo_bytes) } else { None },
        dtb: if dtb { Some(dtb_bytes) } else { None },
    }
}

#[test]
fn generated_images_round_trip() {
    for page in [2048u32, 4096, 65536, 524288] {
        for version in 0..=2u32 {
            let mut rng = Rng(0x9E3779B97F4A7C15 ^ (page as u64) << 8 ^ version as u64);
            for _ in 0..25 {
                let img = sample(version, page, &mut rng);
                let bytes = img.to_bytes().expect("pack");
                let back = BootImage::parse(&bytes[..]).expect("parse back");
                assert_eq!(back.version, img.version, "version page={page}");
                assert_eq!(back.page_size, page, "page_size page={page}");
                // a zero-length section is not representable: size 0 means absent
                let canon = |v: &Option<Vec<u8>>| v.as_ref().filter(|x| !x.is_empty()).cloned();
                assert_eq!(back.kernel, img.kernel, "kernel page={page}");
                assert_eq!(back.ramdisk, canon(&img.ramdisk), "ramdisk page={page}");
                assert_eq!(back.second, canon(&img.second), "second page={page}");
                assert_eq!(back.dtb, canon(&img.dtb), "dtb page={page}");
                assert_eq!(back.cmdline, img.cmdline, "cmdline page={page}");
                assert_eq!(back.extra_cmdline, img.extra_cmdline);
                assert_eq!(back.kernel_addr, 0x8000);
                assert_eq!(back.tags_addr, 0x100);
                assert_eq!(back.id, img.id);
                assert_eq!(back.os_version, img.os_version);
                assert_eq!(back.name, img.name);
                if version >= 1 {
                    assert_eq!(back.recovery_dtbo, canon(&img.recovery_dtbo));
                    assert_eq!(back.recovery_dtbo_offset, 0x1234);
                }
                if version == 2 {
                    assert_eq!(back.dtb_addr, 0x5678);
                }
                // repacking the parsed image must be byte-identical
                assert_eq!(back.to_bytes().expect("repack"), bytes, "stable re-pack");
            }
        }
    }
}

#[test]
fn zero_length_sections_are_absent_not_empty() {
    let mut rng = Rng(42);
    let mut img = sample(1, 4096, &mut rng);
    img.second = None;
    img.recovery_dtbo = Some(Vec::new());
    img.ramdisk = Some(Vec::new());
    let bytes = img.to_bytes().expect("pack");
    let back = BootImage::parse(&bytes[..]).expect("parse");
    assert!(back.second.is_none());
    assert!(back.recovery_dtbo.is_none());
    assert!(back.ramdisk.is_none());
}

#[test]
fn truncation_at_every_page_boundary_is_rejected_or_clean() {
    let mut rng = Rng(7);
    let img = sample(1, 4096, &mut rng);
    let bytes = img.to_bytes().expect("pack");
    for cut in [0usize, 8, 1648, 4095, 4096, 4097, 8192] {
        let res = BootImage::parse(&bytes[..cut.min(bytes.len())]);
        assert!(res.is_err(), "truncation at {cut} must not parse");
    }
    assert!(BootImage::parse(&bytes[..]).is_ok());
}
