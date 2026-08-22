#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use mibox_core::FastbootDevice;
use serde::Serialize;

#[derive(Serialize)]
pub struct DeviceStatus {
    connected: bool,
    serial: Option<String>,
    product: Option<String>,
    unlocked: bool,
    error: Option<String>,
}

const TARGET_SERIAL: &str = "19680/68UA04603";

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
            let matches_serial = serial.as_deref() == Some(TARGET_SERIAL);
            let mismatch = (!matches_serial && serial.is_some())
                .then(|| format!("serial mismatch (expected {TARGET_SERIAL})"));
            DeviceStatus {
                connected: true,
                serial,
                product,
                unlocked,
                error: mismatch,
            }
        }
        Err(e) => DeviceStatus { connected: false, serial: None, product: None, unlocked: false, error: Some(e.to_string()) },
    }
}

#[tauri::command]
fn flash_partition(partition: String, path: String) -> Result<Vec<String>, String> {
    let payload = std::fs::read(&path).map_err(|e| e.to_string())?;
    FastbootDevice::open_first()
        .and_then(|mut d| d.flash(&partition, &payload))
        .map_err(|e| e.to_string())
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
    for cmd in ["oem get_token", "flashing get_unlock_ability", "oem device-info"] {
        match d.command(cmd) {
            Ok((term, infos)) => {
                out.insert(cmd.into(), serde_json::json!({ "infos": infos, "terminal": format!("{term:?}") }));
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
