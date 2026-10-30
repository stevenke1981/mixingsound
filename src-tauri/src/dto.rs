//! IPC payload types (Rust → UI). Mirrored by `ui/src/lib/types.ts` — keep in sync.

use engine::Project;
use engine::render::StereoLevel;
use serde::Serialize;

/// Returned by every command that may change the project, so the UI never has stale state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub project: Project,
    pub can_undo: bool,
    pub can_redo: bool,
    /// Absolute path of the current .msproj, `None` if never saved.
    pub project_path: Option<String>,
    /// True when the project changed since the last save/open/new.
    pub dirty: bool,
    /// `None` when no output device could be opened (app still edits/exports).
    pub device_name: Option<String>,
    /// Sources whose files were not found when opening (their clips are silent).
    pub missing_sources: Vec<u64>,
    /// Non-fatal notices for the user (e.g. "project resampled from 44100 Hz to 48000 Hz",
    /// "2 files could not be imported: …").
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackLevel {
    pub id: u64,
    pub level: StereoLevel,
}

/// Payload of the `engine-status` event, emitted ~30 Hz (also while paused, so meters decay).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatusEvent {
    pub playing: bool,
    pub position: u64,
    pub master: StereoLevel,
    pub tracks: Vec<TrackLevel>,
    pub clipped: bool,
    /// Device/stream error since the last event.
    pub error: Option<String>,
}

/// Payload of the `export-progress` event.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgressEvent {
    pub fraction: f32,
}

pub const EVENT_ENGINE_STATUS: &str = "engine-status";
pub const EVENT_EXPORT_PROGRESS: &str = "export-progress";
