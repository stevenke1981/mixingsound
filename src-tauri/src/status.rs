//! Background thread emitting `engine-status` (~30 Hz) and collecting retired graphs.

use tauri::AppHandle;

pub fn spawn_status_pump(app: AppHandle) {
    let _ = app;
    todo!("tauri worker")
}
