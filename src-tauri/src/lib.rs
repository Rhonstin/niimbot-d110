#[cfg(target_os = "linux")]
mod bluez;
mod printer;
#[cfg(target_os = "linux")]
use bluez as backend;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "android"))]
mod ble;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "android"))]
use ble as backend;

use backend::Session;
use niimbot_core::{Bitmap, PrintOptions, Stage};
use printer::{Device, PrinterInfo};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::{FsExt, OpenOptions};

#[derive(Default)]
struct PrinterService {
    session: Option<Session>,
}

#[derive(Clone, Default)]
struct AppState {
    service: Arc<Mutex<PrinterService>>,
    cancellation_generation: Arc<AtomicU64>,
}

async fn with_service<T: Send + 'static>(
    state: &AppState,
    action: impl FnOnce(&mut PrinterService) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let service = Arc::clone(&state.service);
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = service
            .try_lock()
            .map_err(|_| "The printer is busy. Wait for the current operation to finish.")?;
        action(&mut guard)
    })
    .await
    .map_err(|e| format!("Bluetooth worker failed: {e}"))?
}

#[tauri::command]
async fn scan_printers(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Device>, String> {
    with_service(&state, move |_| backend::scan(&app)).await
}

#[tauri::command]
async fn connect_printer(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PrinterInfo, String> {
    with_service(&state, move |service| {
        if let Some(mut session) = service.session.take() {
            let _ = session.close();
        }
        let session = Session::connect(&id, app)?;
        let info = session
            .info
            .clone()
            .ok_or("Could not identify the printer model.")?;
        service.session = Some(session);
        Ok(info)
    })
    .await
}

#[tauri::command]
async fn disconnect_printer(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    with_service(&state, move |service| {
        if let Some(mut session) = service.session.take() {
            session.close()?;
        }
        printer::log(&app, "info", "Printer disconnected.");
        Ok(())
    })
    .await
}

#[tauri::command]
async fn connection_status(state: State<'_, AppState>) -> Result<Option<PrinterInfo>, String> {
    with_service(&state, |service| {
        if let Some(session) = service.session.as_ref() {
            if session.is_connected()? {
                return Ok(session.info.clone());
            }
        }
        service.session = None;
        Ok(None)
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrintRequest {
    width: u16,
    height: u16,
    bitmap: Vec<u8>,
    density: u8,
    copies: u8,
}

#[derive(Clone, Serialize)]
struct PrintProgress {
    copy: u8,
    copies: u8,
    stage: &'static str,
    percent: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrintReport {
    completed_copies: u8,
}

#[tauri::command]
async fn print_label(
    request: PrintRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PrintReport, String> {
    let bitmap = Bitmap::new(request.width, request.height, request.bitmap)?;
    let cancellation = Arc::clone(&state.cancellation_generation);
    let generation = cancellation.load(Ordering::SeqCst);
    with_service(&state, move |service| {
        let session = service
            .session
            .as_mut()
            .ok_or("Connect your printer first.")?;
        if !session.is_connected()? {
            return Err("Connection lost. Reconnect your printer.".into());
        }
        let options = PrintOptions {
            density: request.density,
            copies: request.copies,
            ..PrintOptions::default()
        };
        let result = niimbot_core::print(
            session,
            &bitmap,
            &options,
            |p| {
                let stage = match p.stage {
                    Stage::Configuring => "configuring",
                    Stage::Uploading => "uploading",
                    Stage::Printing => "printing",
                    Stage::Complete => "complete",
                };
                let _ = app.emit(
                    "print-progress",
                    PrintProgress {
                        copy: p.copy,
                        copies: p.copies,
                        stage,
                        percent: p.percent,
                    },
                );
            },
            || cancellation.load(Ordering::SeqCst) != generation,
        );
        match result {
            Ok(completed_copies) => {
                printer::log(
                    &app,
                    "info",
                    format!("Print confirmed: {completed_copies} copies."),
                );
                Ok(PrintReport { completed_copies })
            }
            Err(error) => {
                let message = format!(
                    "{}. Completed copies: {}. Check the physical label before printing again.",
                    error.message, error.completed_copies
                );
                printer::log(&app, "error", &message);
                // Failed jobs can leave firmware state uncertain. Require a fresh handshake.
                service.session = None;
                Err(message)
            }
        }
    })
    .await
}

#[tauri::command]
fn cancel_print(state: State<'_, AppState>) {
    state.cancellation_generation.fetch_add(1, Ordering::SeqCst);
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ArtifactKind {
    Png,
    Log,
}

#[tauri::command]
async fn save_artifact(
    kind: ArtifactKind,
    data: Vec<u8>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    if data.is_empty() || data.len() > 10 * 1024 * 1024 {
        return Err("Invalid file size.".into());
    }
    if matches!(kind, ArtifactKind::Png) && !data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("The file is not a PNG image.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let (name, label, extension) = match kind {
            ArtifactKind::Png => ("d110-label.png", "PNG image", "png"),
            ArtifactKind::Log => ("d110-printer.log", "Printer log", "log"),
        };
        let Some(file) = app
            .dialog()
            .file()
            .set_file_name(name)
            .add_filter(label, &[extension])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        // Android's document picker returns content:// URIs. FsExt opens them
        // through ContentResolver and also handles ordinary desktop paths.
        let display_path = file.to_string();
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        let mut output = app
            .fs()
            .open(file, options)
            .map_err(|e| format!("Could not open the file: {e}"))?;
        output
            .write_all(&data)
            .map_err(|e| format!("Could not save the file: {e}"))?;
        Ok(Some(display_path))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            scan_printers,
            connect_printer,
            disconnect_printer,
            connection_status,
            print_label,
            cancel_print,
            save_artifact
        ]);
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "android"))]
    let builder = builder.plugin(tauri_plugin_blec::init());
    let app = builder
        .build(tauri::generate_context!())
        .expect("Failed to build D110 Studio");
    app.run(|handle, event| {
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            let state = handle.state::<AppState>();
            state.cancellation_generation.fetch_add(1, Ordering::SeqCst);
            // A job owns the lock until its PrintEnd cleanup has run.
            if let Ok(mut service) = state.service.lock() {
                service.session = None;
            };
        }
    });
}
