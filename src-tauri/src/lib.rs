#[cfg(target_os = "linux")]
mod bluez;

#[cfg(not(target_os = "linux"))]
compile_error!(
    "This MVP uses Linux BlueZ. Add an Android BLE transport before building for Android."
);

use bluez::{Device, PrinterInfo, Session};
use niimbot_core::{Bitmap, PrintOptions, Stage};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

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
            .map_err(|_| "Принтер уже виконує іншу операцію. Дочекайтеся завершення.")?;
        action(&mut guard)
    })
    .await
    .map_err(|e| format!("Помилка Bluetooth-потоку: {e}"))?
}

#[tauri::command]
async fn scan_printers(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Device>, String> {
    with_service(&state, move |_| bluez::scan(&app)).await
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
            .ok_or("Не вдалося визначити модель принтера.")?;
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
        bluez::log(&app, "info", "Принтер відключено.");
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
            .ok_or("Спочатку підключіть принтер.")?;
        if !session.is_connected()? {
            return Err("З'єднання втрачено. Перепідключіть принтер.".into());
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
                bluez::log(
                    &app,
                    "info",
                    format!("Друк підтверджено: {completed_copies} копій."),
                );
                Ok(PrintReport { completed_copies })
            }
            Err(error) => {
                let message = format!(
                    "{}. Завершено копій: {}. Перевірте етикетку перед повторним друком.",
                    error.message, error.completed_copies
                );
                bluez::log(&app, "error", &message);
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
        return Err("Некоректний розмір файлу для збереження.".into());
    }
    if matches!(kind, ArtifactKind::Png) && !data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Файл не є PNG-зображенням.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let (name, label, extension) = match kind {
            ArtifactKind::Png => ("d110-label.png", "PNG-зображення", "png"),
            ArtifactKind::Log => ("d110-printer.log", "Журнал принтера", "log"),
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
        let path = file
            .into_path()
            .map_err(|e| format!("Некоректний шлях файлу: {e}"))?;
        std::fs::write(&path, &data).map_err(|e| format!("Не вдалося зберегти файл: {e}"))?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn run() {
    let app = tauri::Builder::default()
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
        ])
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
