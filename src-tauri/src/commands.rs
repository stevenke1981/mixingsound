//! IPC commands. Names/arguments are the contract with `ui/src/lib/api.ts`
//! (Tauri maps camelCase JS argument keys to these snake_case parameters).
//!
//! After any project change: rebuild the graph (`engine::build_graph`) and push it to the
//! real-time engine, then return a fresh `AppSnapshot`.

use engine::bounce::{BounceOptions, BounceReport};
use engine::Edit;
use tauri::{AppHandle, State};

use crate::dto::AppSnapshot;
use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

/// First call from the UI: starts the audio engine if not started (failure is non-fatal →
/// `device_name: None` + a warning), creates an empty project at the device rate
/// (48000 Hz without a device) and returns the snapshot.
#[tauri::command]
pub fn init(state: State<'_, AppState>) -> CmdResult<AppSnapshot> {
    let _ = state;
    todo!("tauri worker")
}

#[tauri::command]
pub fn new_project(state: State<'_, AppState>) -> CmdResult<AppSnapshot> {
    let _ = state;
    todo!("tauri worker")
}

/// Decodes files in parallel (one thread per file, capped at available_parallelism), builds
/// peaks, then — as ONE undo step — adds per file: source, a new track named after the file
/// stem, and a clip at frame 0. Files that fail are skipped and reported in `warnings`.
#[tauri::command]
pub async fn import_files(state: State<'_, AppState>, paths: Vec<String>) -> CmdResult<AppSnapshot> {
    let _ = (state, paths);
    todo!("tauri worker")
}

#[tauri::command]
pub fn apply_edit(state: State<'_, AppState>, edit: Edit) -> CmdResult<AppSnapshot> {
    let _ = (state, edit);
    todo!("tauri worker")
}

/// Ends a continuous gesture (fader/knob release, drag end): breaks undo coalescing.
#[tauri::command]
pub fn end_gesture(state: State<'_, AppState>) -> CmdResult<()> {
    let _ = state;
    todo!("tauri worker")
}

#[tauri::command]
pub fn undo(state: State<'_, AppState>) -> CmdResult<AppSnapshot> {
    let _ = state;
    todo!("tauri worker")
}

#[tauri::command]
pub fn redo(state: State<'_, AppState>) -> CmdResult<AppSnapshot> {
    let _ = state;
    todo!("tauri worker")
}

#[tauri::command]
pub fn transport_play(state: State<'_, AppState>) -> CmdResult<()> {
    let _ = state;
    todo!("tauri worker")
}

#[tauri::command]
pub fn transport_pause(state: State<'_, AppState>) -> CmdResult<()> {
    let _ = state;
    todo!("tauri worker")
}

#[tauri::command]
pub fn transport_stop(state: State<'_, AppState>) -> CmdResult<()> {
    let _ = state;
    todo!("tauri worker")
}

#[tauri::command]
pub fn transport_seek(state: State<'_, AppState>, frame: u64) -> CmdResult<()> {
    let _ = (state, frame);
    todo!("tauri worker")
}

/// `start`/`end` both `None` disables looping; otherwise both required, start < end.
#[tauri::command]
pub fn transport_set_loop(
    state: State<'_, AppState>,
    start: Option<u64>,
    end: Option<u64>,
) -> CmdResult<()> {
    let _ = (state, start, end);
    todo!("tauri worker")
}

/// `[min, max]` per column for source frames `start..end` (see `engine::peaks::Peaks::query`).
#[tauri::command]
pub fn get_waveform(
    state: State<'_, AppState>,
    source_id: u64,
    start: u64,
    end: u64,
    columns: usize,
) -> CmdResult<Vec<[f32; 2]>> {
    let _ = (state, source_id, start, end, columns);
    todo!("tauri worker")
}

/// `path: None` saves to the current project path (error if there is none).
/// Appends ".msproj" if missing. Clears `dirty`.
#[tauri::command]
pub fn save_project(state: State<'_, AppState>, path: Option<String>) -> CmdResult<AppSnapshot> {
    let _ = (state, path);
    todo!("tauri worker")
}

/// Loads, converts to the device rate with `Project::with_sample_rate` if needed (warning),
/// decodes all present sources in parallel, rebuilds peaks, resets history, stops transport.
#[tauri::command]
pub async fn open_project(state: State<'_, AppState>, path: String) -> CmdResult<AppSnapshot> {
    let _ = (state, path);
    todo!("tauri worker")
}

/// Bounces on a blocking worker thread, emitting `export-progress`. Does not hold the state
/// lock while rendering (clones the `Arc<RenderGraph>` first).
#[tauri::command]
pub async fn export_wav(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    options: BounceOptions,
) -> CmdResult<BounceReport> {
    let _ = (app, state, path, options);
    todo!("tauri worker")
}
