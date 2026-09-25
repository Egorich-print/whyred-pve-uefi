#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use mibox_core::FastbootDevice;
use serde::Serialize;

#[derive(Serialize)]
struct DeviceStatus {
    connected: bool,
    serial: Option<String>,
    product: Option<String>,
    unlocked: bool,
    error: Option<String>,
}

/// Partitions this GUI may write. `userdata` is deliberately absent: the
/// rootfs is GiB-sized and belongs to ./flash_all.sh, not to a GUI flash.
const WRITABLE: [&str; 3] = ["boot", "recovery", "cache"];
const MAX_IMAGE_BYTES: u64 = 512 << 20;

#[tauri::command]
fn device_status() -> DeviceStatus {
    match FastbootDevice::open_first() {
        Ok(mut dev) => {
            let serial = dev.getvar("serialno").ok();
            let product = dev.getvar("product").ok();
            let unlocked = dev
                .getvar("unlocked")
                .map(|v| v == "yes" || v == "true")
                .unwrap_or(false);
            let known = matches!(product.as_deref(), Some("whyred") | Some("lavender"));
            let warning =
                (!known).then(|| "unrecognized product — check before flashing".to_string());
            DeviceStatus {
                connected: true,
                serial,
                product,
                unlocked,
                error: warning,
            }
        }
        Err(e) => DeviceStatus {
            connected: false,
            serial: None,
            product: None,
            unlocked: false,
            error: Some(e.to_string()),
        },
    }
}

#[tauri::command]
fn flash_partition(
    partition: String,
    path: String,
    confirm_serial: String,
) -> Result<Vec<String>, String> {
    if !WRITABLE.contains(&partition.as_str()) {
        return Err(format!(
            "partition '{partition}' is not writable from this GUI ({})",
            WRITABLE.join(", ")
        ));
    }
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() == 0 || meta.len() > MAX_IMAGE_BYTES {
        return Err(format!(
            "image size {} is out of range (1..={MAX_IMAGE_BYTES} bytes)",
            meta.len()
        ));
    }
    let mut d = FastbootDevice::open_first().map_err(|e| e.to_string())?;
    let serial = d.getvar("serialno").map_err(|e| e.to_string())?;
    if serial != confirm_serial {
        return Err(format!(
            "serial mismatch: attached '{serial}', confirmed '{confirm_serial}'"
        ));
    }
    let unlocked = d.getvar("unlocked").unwrap_or_default();
    if unlocked != "yes" && unlocked != "true" {
        return Err("bootloader is locked — fastboot will refuse writes".into());
    }
    let payload = std::fs::read(&path).map_err(|e| e.to_string())?;
    d.flash(&partition, &payload).map_err(|e| e.to_string())
}

#[tauri::command]
fn reboot_bootloader() -> Result<(), String> {
    FastbootDevice::open_first()
        .and_then(|mut d| d.reboot_bootloader())
        .map_err(|e| e.to_string())
}

/// OEM unlock sequence for Xiaomi: get token, request unlock.
/// NOTE: on MIUI the actual authorization is signed by Xiaomi's servers;
/// full offline unlock requires an authorized firehose/EDL programmer and is
/// intentionally NOT automated here. This command surfaces what fastboot allows.
#[tauri::command]
fn oem_unlock_probe() -> Result<serde_json::Value, String> {
    let mut d = FastbootDevice::open_first().map_err(|e| e.to_string())?;
    let mut out = serde_json::Map::new();
    // `oem device-info` is deliberately absent: on this ABL it wedges fastboot
    // until a physical reboot (see docs/reverse-unlock.md).
    for cmd in ["oem get_token", "flashing get_unlock_ability"] {
        match d.command(cmd) {
            Ok((term, infos)) => {
                out.insert(
                    cmd.into(),
                    serde_json::json!({ "infos": infos, "terminal": format!("{term:?}") }),
                );
            }
            Err(e) => {
                out.insert(cmd.into(), serde_json::json!({ "error": e.to_string() }));
            }
        }
    }
    Ok(serde_json::Value::Object(out))
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            device_status,
            flash_partition,
            reboot_bootloader,
            oem_unlock_probe
        ])
        .run(tauri::generate_context!())
        .expect("error running MiToolbox-Native");
}
