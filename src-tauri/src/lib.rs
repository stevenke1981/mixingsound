//! Tauri shell: owns the project, history, decoded sources and the real-time engine, and
//! exposes them to the UI as commands. All errors cross IPC as user-facing `String`s.

mod commands;
mod dto;
mod state;
mod status;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::new())
        .setup(|app| {
            status::spawn_status_pump(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::init,
            commands::new_project,
            commands::import_files,
            commands::apply_edit,
            commands::end_gesture,
            commands::undo,
            commands::redo,
            commands::transport_play,
            commands::transport_pause,
            commands::transport_stop,
            commands::transport_seek,
            commands::transport_set_loop,
            commands::get_waveform,
            commands::save_project,
            commands::open_project,
            commands::export_wav,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MixingSound");
}
