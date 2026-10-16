//! BLE transport for Windows, macOS and Android. The plugin owns the native
//! adapter/JNI lifecycle; print protocol work stays on the app's blocking worker.
use crate::printer::{identify, log, Device, PrinterInfo, CHAR_UUID, SERVICE_UUID};
use niimbot_core::{Decoder, Packet, Transport, CONNECT_BYTES};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tauri::{async_runtime::block_on, AppHandle};
use tauri_plugin_blec::{
    models::{CharProps, ScanFilter, WriteType},
    Connection, Handler, OnDisconnectHandler,
};
use uuid::Uuid;

fn handler() -> Result<&'static Handler, String> {
    // On Android JNI initialization runs asynchronously after RunEvent::Ready.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(handler) = tauri_plugin_blec::get_handler() {
            return Ok(handler);
        }
        if Instant::now() >= deadline {
            return Err("Bluetooth initialization timed out. Restart the app.".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub fn scan(app: &AppHandle) -> Result<Vec<Device>, String> {
    let handler = handler()?;
    let permitted =
        block_on(tauri_plugin_blec::check_permissions(true)).map_err(|e| e.to_string())?;
    if !permitted {
        return Err(
            "Bluetooth permission is required. Allow Nearby devices access in app settings.".into(),
        );
    }
    log(app, "info", "Scanning for D110 printers over Bluetooth LE…");
    block_on(async {
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        // These printers do not always advertise the service UUID. Filter names
        // after discovery, just as the Linux BlueZ transport does.
        handler
            .discover(Some(tx), 6000, ScanFilter::None, false)
            .await
            .map_err(|e| e.to_string())?;
        let mut devices = HashMap::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(7);
        while let Ok(Some(found)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            for device in found {
                if device.name.to_ascii_uppercase().starts_with("D110") {
                    devices.insert(
                        device.address.clone(),
                        Device {
                            id: device.address.clone(),
                            name: device.name,
                            address: device.address,
                            rssi: device.rssi,
                        },
                    );
                }
            }
        }
        // Stop and join the producer before dropping its receiver.
        handler.stop_scan().await.map_err(|e| e.to_string())?;
        let mut devices: Vec<_> = devices.into_values().collect();
        devices.sort_by(|a, b| b.rssi.cmp(&a.rssi).then_with(|| a.name.cmp(&b.name)));
        log(app, "info", format!("Printers found: {}", devices.len()));
        Ok(devices)
    })
}

#[derive(Default)]
struct Received {
    decoder: Decoder,
    packets: VecDeque<Packet>,
}

pub struct Session {
    connection: Connection,
    characteristic: Uuid,
    service: Uuid,
    received: Arc<(Mutex<Received>, Condvar)>,
    write_size: usize,
    app: AppHandle,
    pub info: Option<PrinterInfo>,
    closed: bool,
}

impl Session {
    pub fn connect(id: &str, app: AppHandle) -> Result<Self, String> {
        let handler = handler()?;
        let connection =
            block_on(handler.connect(id, OnDisconnectHandler::None, false)).map_err(|e| {
                format!(
                    "Could not connect: {e}. Turn on your printer and close NIIMBOT on your phone."
                )
            })?;
        let mut session = Self {
            connection,
            characteristic: Uuid::parse_str(CHAR_UUID).map_err(|e| e.to_string())?,
            service: Uuid::parse_str(SERVICE_UUID).map_err(|e| e.to_string())?,
            received: Arc::new((Mutex::new(Received::default()), Condvar::new())),
            write_size: 20,
            app,
            info: None,
            closed: false,
        };
        let services = block_on(session.connection.services()).map_err(|e| e.to_string())?;
        let characteristic = services
            .iter()
            .find(|s| s.uuid == session.service)
            .and_then(|s| {
                s.characteristics
                    .iter()
                    .find(|c| c.uuid == session.characteristic)
            })
            .ok_or("NIIMBOT Bluetooth print characteristic was not found.")?;
        if !characteristic
            .properties
            .contains(CharProps::WriteWithoutResponse)
            || !characteristic.properties.contains(CharProps::Notify)
        {
            return Err(
                "Printer does not support the required BLE write/notify operations.".into(),
            );
        }
        session.write_size = block_on(session.connection.mtu())
            .unwrap_or(23)
            .saturating_sub(3)
            .clamp(20, 244) as usize;
        let received = Arc::clone(&session.received);
        block_on(session.connection.subscribe(
            session.characteristic,
            Some(session.service),
            move |bytes: Vec<u8>| {
                let (queue, available) = &*received;
                if let Ok(mut rx) = queue.lock() {
                    let packets = rx.decoder.feed(&bytes);
                    rx.packets.extend(packets);
                    while rx.packets.len() > 128 {
                        rx.packets.pop_front();
                    }
                    available.notify_one();
                }
            },
        ))
        .map_err(|e| format!("Could not subscribe to printer notifications: {e}"))?;
        session.write_raw(CONNECT_BYTES)?;
        std::thread::sleep(Duration::from_millis(200));
        let device = block_on(session.connection.device()).map_err(|e| e.to_string())?;
        if !device.name.to_ascii_uppercase().starts_with("D110") {
            return Err("The connected device is not a D110 printer.".into());
        }
        let device = Device {
            id: device.address.clone(),
            name: device.name,
            address: device.address,
            rssi: device.rssi,
        };
        let app = session.app.clone();
        session.info = Some(identify(&mut session, device, &app)?);
        Ok(session)
    }

    pub fn is_connected(&self) -> Result<bool, String> {
        Ok(!self.closed && self.connection.is_connected())
    }

    fn write_raw(&self, bytes: &[u8]) -> Result<(), String> {
        for chunk in bytes.chunks(self.write_size) {
            block_on(self.connection.send_data(
                self.characteristic,
                Some(self.service),
                chunk,
                WriteType::WithoutResponse,
            ))
            .map_err(|e| format!("Bluetooth write failed: {e}. Reconnect your printer."))?;
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.info = None;
        if self.connection.is_connected() {
            let _ = block_on(
                self.connection
                    .unsubscribe(self.characteristic, Some(self.service)),
            );
            block_on(self.connection.disconnect()).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

impl Transport for Session {
    fn send(&mut self, packet: &Packet) -> Result<(), String> {
        if packet.command != 0x84 && packet.command != 0x85 {
            log(
                &self.app,
                "debug",
                format!("TX {:02X} {:02X?}", packet.command, packet.data),
            );
        }
        self.write_raw(&packet.encode()?)?;
        let mut rx = self
            .received
            .0
            .lock()
            .map_err(|_| "BLE receive lock failed")?;
        if let Some(index) = rx
            .packets
            .iter()
            .position(|p| p.command == 0xdb || p.command == 0)
        {
            let fault = rx.packets.remove(index).ok_or("BLE error queue changed")?;
            return Err(format!(
                "Printer rejected command {:02X}: RX {:02X} {:02X?}",
                packet.command, fault.command, fault.data
            ));
        }
        Ok(())
    }

    fn request(
        &mut self,
        packet: &Packet,
        expected: u8,
        timeout: Duration,
    ) -> Result<Packet, String> {
        self.received
            .0
            .lock()
            .map_err(|_| "BLE receive lock failed")?
            .packets
            .clear();
        self.send(packet)?;
        let deadline = Instant::now() + timeout;
        let (queue, available) = &*self.received;
        let mut rx = queue.lock().map_err(|_| "BLE receive lock failed")?;
        loop {
            while let Some(response) = rx.packets.pop_front() {
                log(
                    &self.app,
                    "debug",
                    format!("RX {:02X} {:02X?}", response.command, response.data),
                );
                if response.command == expected {
                    return Ok(response);
                }
                if response.command == 0xdb || response.command == 0 {
                    return Err(format!(
                        "Printer rejected command {:02X}: RX {:02X} {:02X?}",
                        packet.command, response.command, response.data
                    ));
                }
            }
            if !self.connection.is_connected() {
                return Err("Printer disconnected. Connect again.".into());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "No response 0x{expected:02X} to command 0x{:02X} within {} ms.",
                    packet.command,
                    timeout.as_millis()
                ));
            }
            let (next, _) = available
                .wait_timeout(
                    rx,
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(100)),
                )
                .map_err(|_| "BLE notification wait failed")?;
            rx = next;
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            log(&self.app, "debug", format!("Disconnect: {error}"));
        }
    }
}
