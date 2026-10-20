#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod extraction;

use extraction::{Report, Request};
use std::sync::atomic::{AtomicBool, Ordering};

static EXTRACTING: AtomicBool = AtomicBool::new(false);

fn game_root() -> Result<std::path::PathBuf, String> {
    if cfg!(debug_assertions) {
        return Ok(std::env::var_os("WAW_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local/test-root")
            }));
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let root = executable.parent().ok_or("Cannot locate the app folder.")?;
    if !root.join("CoDWaW.exe").is_file() {
        return Err("Put this executable in your World at War folder, beside CoDWaW.exe, then open it again.".into());
    }
    Ok(root.to_path_buf())
}

#[tauri::command]
fn zone_directory() -> Result<String, String> {
    Ok(game_root()?.join("zone/english").to_string_lossy().into())
}

#[tauri::command]
fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
async fn extract(request: Request) -> Result<Report, String> {
    game_root()?;
    if EXTRACTING.swap(true, Ordering::SeqCst) {
        return Err("An extraction is already running.".into());
    }

    let result = tauri::async_runtime::spawn_blocking(move || extraction::run(request)).await;
    EXTRACTING.store(false, Ordering::SeqCst);
    result.map_err(|error| error.to_string())?
}

fn main() {
    // Each copy gets its own WebView profile, avoiding locks from another instance.
    let profile = tempfile::Builder::new()
        .prefix("waw-ff-ui-")
        .tempdir()
        .expect("Cannot create the temporary browser profile");
    let profile_path = profile.path().to_path_buf();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                .data_directory(profile_path)
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![zone_directory, quit, extract])
        .run(tauri::generate_context!())
        .expect("Cannot start WaW FF Extractor");
}
