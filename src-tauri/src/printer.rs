use niimbot_core::{model_id, protocol_version, Packet, Transport, D110_MODEL_ID};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const SERVICE_UUID: &str = "e7810a71-73ae-499d-8c15-faa9aef0c3f2";
pub const CHAR_UUID: &str = "bef8d6c9-9c21-4c9e-b632-bd58c1009f9f";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub address: String,
    pub rssi: Option<i16>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterInfo {
    pub device: Device,
    pub model_id: u16,
    pub protocol_version: Option<u8>,
    pub dpi: u16,
    pub head_width: u16,
}

#[derive(Clone, Serialize)]
pub struct LogEntry {
    pub level: &'static str,
    pub message: String,
}

pub fn log(app: &AppHandle, level: &'static str, message: impl Into<String>) {
    let entry = LogEntry {
        level,
        message: message.into(),
    };
    eprintln!("[{}] {}", entry.level, entry.message);
    let _ = app.emit("printer-log", entry);
}

/// Shared by BlueZ and the portable BLE transport so model validation and the
/// post-connect handshake cannot diverge between platforms.
pub fn identify<T: Transport>(
    transport: &mut T,
    device: Device,
    app: &AppHandle,
) -> Result<PrinterInfo, String> {
    fn optional<T: Transport>(
        transport: &mut T,
        app: &AppHandle,
        command: u8,
        data: Vec<u8>,
        expected: u8,
    ) -> Option<Packet> {
        match transport.request(
            &Packet::new(command, data),
            expected,
            Duration::from_secs(1),
        ) {
            Ok(packet) => Some(packet),
            Err(error) => {
                log(app, "debug", error);
                None
            }
        }
    }
    let status = optional(transport, app, 0xa5, vec![1], 0xb5);
    let response = transport.request(&Packet::new(0x40, vec![8]), 0x48, Duration::from_secs(2))?;
    let model_id = model_id(&response.data).ok_or("Printer returned an empty model identifier.")?;
    if model_id != D110_MODEL_ID {
        return Err(format!("Detected model {model_id} (0x{model_id:04X}). This version supports D110 (2304); D110_M uses a different protocol."));
    }
    let _ = optional(transport, app, 0xa5, vec![1], 0xb5);
    for sub in [8, 11, 13, 10, 7, 3, 12, 9] {
        let _ = optional(transport, app, 0x40, vec![sub], 0x40 + sub);
    }
    let _ = optional(transport, app, 0xdc, vec![4], 0xd9);
    log(
        app,
        "info",
        format!("D110 connected: model {model_id}, 203 dpi, 96 px."),
    );
    Ok(PrinterInfo {
        device,
        model_id,
        protocol_version: status.as_ref().and_then(|p| protocol_version(&p.data)),
        dpi: niimbot_core::DPI,
        head_width: niimbot_core::HEAD_WIDTH,
    })
}
