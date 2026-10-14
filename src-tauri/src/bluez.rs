use dbus::arg::{PropMap, Variant};
use dbus::blocking::{stdintf::org_freedesktop_dbus::Properties, Connection};
use dbus::channel::Token;
use dbus::message::MatchRule;
use dbus::Path;
use niimbot_core::{
    model_id, protocol_version, Decoder, Packet, Transport, CONNECT_BYTES, D110_MODEL_ID,
};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const BLUEZ: &str = "org.bluez";
const DEVICE: &str = "org.bluez.Device1";
const ADAPTER: &str = "org.bluez.Adapter1";
const CHAR: &str = "org.bluez.GattCharacteristic1";
const SERVICE: &str = "org.bluez.GattService1";
const SERVICE_UUID: &str = "e7810a71-73ae-499d-8c15-faa9aef0c3f2";
const CHAR_UUID: &str = "bef8d6c9-9c21-4c9e-b632-bd58c1009f9f";
const METHOD_TIMEOUT: Duration = Duration::from_secs(5);
type Objects = HashMap<Path<'static>, HashMap<String, PropMap>>;

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

fn system_bus() -> Result<Connection, String> {
    Connection::new_system().map_err(|e| format!(
        "Немає доступу до системної D-Bus: {e}. Запустіть застосунок у звичайній Linux-сесії з активним Bluetooth."
    ))
}

fn objects(conn: &Connection) -> Result<Objects, String> {
    let proxy = conn.with_proxy(BLUEZ, "/", METHOD_TIMEOUT);
    let (objects,): (Objects,) = proxy
        .method_call(
            "org.freedesktop.DBus.ObjectManager",
            "GetManagedObjects",
            (),
        )
        .map_err(|e| {
            format!("Не вдалося прочитати пристрої BlueZ: {e}. Перевірте службу bluetooth.")
        })?;
    Ok(objects)
}

fn text_prop(map: &PropMap, key: &str) -> Option<String> {
    map.get(key)?.0.as_str().map(str::to_owned)
}

fn bool_prop(map: &PropMap, key: &str) -> bool {
    map.get(key).and_then(|v| v.0.as_i64()) == Some(1)
}

fn device_from(path: &Path<'_>, interfaces: &HashMap<String, PropMap>) -> Option<Device> {
    let props = interfaces.get(DEVICE)?;
    let name = text_prop(props, "Name").or_else(|| text_prop(props, "Alias"))?;
    if !name.to_ascii_uppercase().starts_with("D110") {
        return None;
    }
    Some(Device {
        id: path.to_string(),
        name,
        address: text_prop(props, "Address").unwrap_or_default(),
        rssi: props
            .get("RSSI")
            .and_then(|v| v.0.as_i64())
            .and_then(|v| i16::try_from(v).ok()),
    })
}

pub fn scan(app: &AppHandle) -> Result<Vec<Device>, String> {
    let conn = system_bus()?;
    let managed = objects(&conn)?;
    let mut adapters: Vec<_> = managed
        .iter()
        .filter_map(|(path, interfaces)| {
            interfaces
                .get(ADAPTER)
                .map(|p| (path.to_string(), bool_prop(p, "Powered")))
        })
        .collect();
    adapters.sort_by(|a, b| a.0.cmp(&b.0));
    if adapters.is_empty() {
        return Err("Bluetooth-адаптер не знайдено.".into());
    }
    let powered: Vec<_> = adapters
        .iter()
        .filter(|(_, powered)| *powered)
        .map(|(p, _)| p.clone())
        .collect();
    if powered.is_empty() {
        return Err("Bluetooth вимкнено. Увімкніть його в налаштуваннях Linux.".into());
    }
    let mut started = Vec::new();
    let result = (|| {
        for path in &powered {
            let proxy = conn.with_proxy(BLUEZ, path.as_str(), METHOD_TIMEOUT);
            let mut filter: PropMap = HashMap::new();
            filter.insert("Transport".into(), Variant(Box::new("le".to_string())));
            filter.insert("DuplicateData".into(), Variant(Box::new(false)));
            let _: () = proxy
                .method_call(ADAPTER, "SetDiscoveryFilter", (filter,))
                .map_err(|e| format!("Не вдалося налаштувати BLE-пошук: {e}"))?;
            let _: () = proxy
                .method_call(ADAPTER, "StartDiscovery", ())
                .map_err(|e| format!("Не вдалося почати пошук: {e}"))?;
            started.push(path.clone());
        }
        log(app, "info", "Пошук D110 через BLE…");
        let deadline = Instant::now() + Duration::from_secs(6);
        while Instant::now() < deadline {
            conn.process(Duration::from_millis(100))
                .map_err(|e| e.to_string())?;
        }
        let mut devices: Vec<_> = objects(&conn)?
            .iter()
            .filter_map(|(path, interfaces)| device_from(path, interfaces))
            .collect();
        devices.sort_by(|a, b| b.rssi.cmp(&a.rssi).then_with(|| a.name.cmp(&b.name)));
        log(
            app,
            "info",
            format!("Знайдено принтерів: {}", devices.len()),
        );
        Ok(devices)
    })();
    // Stop only discovery sessions started by this connection; BlueZ reference-counts them.
    for path in started {
        let _: Result<(), _> =
            conn.with_proxy(BLUEZ, path, METHOD_TIMEOUT)
                .method_call(ADAPTER, "StopDiscovery", ());
    }
    result
}

#[derive(Default)]
struct Received {
    decoder: Decoder,
    packets: VecDeque<Packet>,
}

pub struct Session {
    conn: Connection,
    device: Device,
    characteristic: String,
    received: Arc<Mutex<Received>>,
    match_token: Option<Token>,
    notifications_started: bool,
    write_size: usize,
    app: AppHandle,
    pub info: Option<PrinterInfo>,
}

impl Session {
    pub fn connect(id: &str, app: AppHandle) -> Result<Self, String> {
        let conn = system_bus()?;
        let managed = objects(&conn)?;
        let device = managed
            .iter()
            .find(|(path, _)| *path == id)
            .and_then(|(path, interfaces)| device_from(path, interfaces))
            .ok_or("Принтер не знайдено. Повторіть пошук D110.")?;
        log(&app, "info", format!("Підключення до {}…", device.name));
        let mut session = Self {
            conn,
            device,
            characteristic: String::new(),
            received: Arc::default(),
            match_token: None,
            notifications_started: false,
            write_size: 20,
            app,
            info: None,
        };
        let proxy =
            session
                .conn
                .with_proxy(BLUEZ, session.device.id.as_str(), Duration::from_secs(15));
        if !proxy.get::<bool>(DEVICE, "Connected").unwrap_or(false) {
            let _: () = proxy.method_call(DEVICE, "Connect", ())
                .map_err(|e| format!("Не вдалося підключитися: {e}. Закрийте NIIMBOT на телефоні й увімкніть принтер."))?;
        }
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            if proxy
                .get::<bool>(DEVICE, "ServicesResolved")
                .unwrap_or(false)
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err("Принтер не надав BLE-сервіси за 12 секунд.".into());
            }
            session
                .conn
                .process(Duration::from_millis(100))
                .map_err(|e| e.to_string())?;
        }
        let managed = objects(&session.conn)?;
        let prefix = format!("{}/", session.device.id);
        let service_path = managed
            .iter()
            .find_map(|(path, interfaces)| {
                let props = interfaces.get(SERVICE)?;
                (path.to_string().starts_with(&prefix)
                    && text_prop(props, "UUID")?.eq_ignore_ascii_case(SERVICE_UUID))
                .then(|| path.to_string())
            })
            .ok_or("BLE-сервіс NIIMBOT не знайдено.")?;
        let (characteristic, props) = managed
            .iter()
            .find_map(|(path, interfaces)| {
                let props = interfaces.get(CHAR)?;
                (text_prop(props, "Service")? == service_path
                    && text_prop(props, "UUID")?.eq_ignore_ascii_case(CHAR_UUID))
                .then(|| (path.to_string(), props))
            })
            .ok_or("BLE-канал друку NIIMBOT не знайдено.")?;
        let flags: Vec<_> = props
            .get("Flags")
            .and_then(|v| v.0.as_iter())
            .map(|items| {
                items
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        if !flags.iter().any(|f| f == "write-without-response")
            || !flags.iter().any(|f| f == "notify")
        {
            return Err(format!(
                "Канал принтера не підтримує потрібні BLE-операції: {flags:?}"
            ));
        }
        session.write_size = props
            .get("MTU")
            .and_then(|v| v.0.as_u64())
            .map(|mtu| mtu.saturating_sub(3).clamp(20, 244) as usize)
            .unwrap_or(20);
        session.characteristic = characteristic;
        let mut rule =
            MatchRule::new_signal("org.freedesktop.DBus.Properties", "PropertiesChanged");
        rule.path = Some(Path::new(session.characteristic.clone()).map_err(|e| e.to_string())?);
        rule.sender = Some(BLUEZ.into());
        let received = Arc::clone(&session.received);
        session.match_token = Some(
            session
                .conn
                .add_match(
                    rule,
                    move |(interface, changed, _): (String, PropMap, Vec<String>), _, _| {
                        if interface == CHAR {
                            if let Some(items) = changed.get("Value").and_then(|v| v.0.as_iter()) {
                                let bytes: Vec<u8> = items
                                    .filter_map(|v| v.as_u64().and_then(|b| u8::try_from(b).ok()))
                                    .collect();
                                if let Ok(mut rx) = received.lock() {
                                    let packets = rx.decoder.feed(&bytes);
                                    rx.packets.extend(packets);
                                    while rx.packets.len() > 128 {
                                        rx.packets.pop_front();
                                    }
                                }
                            }
                        }
                        true
                    },
                )
                .map_err(|e| format!("Не вдалося підписатися на відповіді принтера: {e}"))?,
        );
        let _: () = session
            .conn
            .with_proxy(BLUEZ, session.characteristic.as_str(), METHOD_TIMEOUT)
            .method_call(CHAR, "StartNotify", ())
            .map_err(|e| format!("Не вдалося увімкнути BLE-повідомлення: {e}"))?;
        session.notifications_started = true;
        session.write_raw(CONNECT_BYTES)?;
        session.pump(Duration::from_millis(200))?;

        // D110 firmware can return a short status packet. Model id is authoritative.
        let status = session.optional(0xa5, vec![1], 0xb5);
        let model = session.request(&Packet::new(0x40, vec![8]), 0x48, Duration::from_secs(2))?;
        let model_id =
            model_id(&model.data).ok_or("Принтер повернув порожній ідентифікатор моделі.")?;
        if model_id != D110_MODEL_ID {
            return Err(format!("Виявлено модель {model_id} (0x{model_id:04X}). Ця версія підтримує D110 (2304); D110_M має інший протокол."));
        }
        // Replicate the B1-family post-connect handshake. Unsupported info reads
        // are optional, but every request has a bounded wait and is logged.
        let _ = session.optional(0xa5, vec![1], 0xb5);
        for sub in [8, 11, 13, 10, 7, 3, 12, 9] {
            let _ = session.optional(0x40, vec![sub], 0x40 + sub);
        }
        let _ = session.optional(0xdc, vec![4], 0xd9);
        let info = PrinterInfo {
            device: session.device.clone(),
            model_id,
            protocol_version: status.as_ref().and_then(|p| protocol_version(&p.data)),
            dpi: niimbot_core::DPI,
            head_width: niimbot_core::HEAD_WIDTH,
        };
        log(&session.app, "info", format!("D110 підключено. Модель {model_id}, 203 dpi, 96 px; BLE-запис {} байт із паузою 10 мс.", session.write_size));
        session.info = Some(info);
        Ok(session)
    }

    fn optional(&mut self, command: u8, data: Vec<u8>, expected: u8) -> Option<Packet> {
        match self.request(
            &Packet::new(command, data),
            expected,
            Duration::from_millis(1000),
        ) {
            Ok(packet) => Some(packet),
            Err(error) => {
                log(&self.app, "debug", error);
                None
            }
        }
    }

    pub fn is_connected(&self) -> Result<bool, String> {
        self.conn
            .with_proxy(BLUEZ, self.device.id.as_str(), METHOD_TIMEOUT)
            .get::<bool>(DEVICE, "Connected")
            .map_err(|e| e.to_string())
    }

    fn pump(&self, duration: Duration) -> Result<(), String> {
        let deadline = Instant::now() + duration;
        loop {
            self.conn
                .process(
                    Duration::from_millis(20)
                        .min(deadline.saturating_duration_since(Instant::now())),
                )
                .map_err(|e| format!("Помилка читання Bluetooth: {e}"))?;
            if Instant::now() >= deadline {
                return Ok(());
            }
        }
    }

    fn write_raw(&self, bytes: &[u8]) -> Result<(), String> {
        let proxy = self
            .conn
            .with_proxy(BLUEZ, self.characteristic.as_str(), METHOD_TIMEOUT);
        for chunk in bytes.chunks(self.write_size) {
            let mut attempts = 0;
            loop {
                let mut options: PropMap = HashMap::new();
                options.insert("type".into(), Variant(Box::new("command".to_string())));
                let result: Result<(), dbus::Error> =
                    proxy.method_call(CHAR, "WriteValue", (chunk.to_vec(), options));
                match result {
                    Ok(()) => break,
                    Err(e) if e.name() == Some("org.bluez.Error.InProgress") && attempts < 3 => {
                        attempts += 1;
                        self.pump(Duration::from_millis(20))?;
                    }
                    Err(e) => {
                        return Err(format!("Помилка BLE-запису: {e}. Перепідключіть принтер."))
                    }
                }
            }
            // D110's unacknowledged BLE writes need pacing, including split frames.
            self.pump(Duration::from_millis(10))?;
        }
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        if self.notifications_started {
            let result: Result<(), dbus::Error> = self
                .conn
                .with_proxy(BLUEZ, self.characteristic.as_str(), METHOD_TIMEOUT)
                .method_call(CHAR, "StopNotify", ());
            if let Err(e) = result {
                errors.push(e.to_string());
            }
            self.notifications_started = false;
        }
        if let Some(token) = self.match_token.take() {
            let _ = self.conn.remove_match(token);
        }
        if self.info.take().is_some() || self.is_connected().unwrap_or(false) {
            let result: Result<(), dbus::Error> = self
                .conn
                .with_proxy(BLUEZ, self.device.id.as_str(), METHOD_TIMEOUT)
                .method_call(DEVICE, "Disconnect", ());
            if let Err(e) = result {
                errors.push(e.to_string());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
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
        // Row writes have no ACK, but firmware can still notify a rejection.
        // Surface it immediately instead of uploading the rest of a failed page.
        let mut rx = self
            .received
            .lock()
            .map_err(|_| "BLE receive lock failed")?;
        if let Some(index) = rx
            .packets
            .iter()
            .position(|p| p.command == 0xdb || p.command == 0x00)
        {
            let fault = rx.packets.remove(index).ok_or("BLE error queue changed")?;
            return Err(format!(
                "Принтер відхилив команду {:02X}: RX {:02X} {:02X?}",
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
        // Drain notifications queued before this request so stale status/acks cannot
        // complete it. Only one operation owns this session at a time.
        for _ in 0..128 {
            if !self
                .conn
                .process(Duration::ZERO)
                .map_err(|e| e.to_string())?
            {
                break;
            }
        }
        self.received
            .lock()
            .map_err(|_| "BLE receive lock failed")?
            .packets
            .clear();
        self.send(packet)?;
        let deadline = Instant::now() + timeout;
        loop {
            loop {
                let next = self
                    .received
                    .lock()
                    .map_err(|_| "BLE receive lock failed")?
                    .packets
                    .pop_front();
                let Some(response) = next else {
                    break;
                };
                log(
                    &self.app,
                    "debug",
                    format!("RX {:02X} {:02X?}", response.command, response.data),
                );
                if response.command == expected {
                    return Ok(response);
                }
                if response.command == 0xdb || response.command == 0x00 {
                    return Err(format!(
                        "Принтер відхилив команду {:02X}: RX {:02X} {:02X?}",
                        packet.command, response.command, response.data
                    ));
                }
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "Немає відповіді 0x{expected:02X} на команду 0x{:02X} за {} мс.",
                    packet.command,
                    timeout.as_millis()
                ));
            }
            self.pump(
                Duration::from_millis(20).min(deadline.saturating_duration_since(Instant::now())),
            )?;
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            log(&self.app, "debug", format!("Відключення: {error}"));
        }
    }
}
