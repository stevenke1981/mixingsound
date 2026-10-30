//! Every user change to a `Project` is an `Edit` (command pattern). The UI sends edits over IPC
//! as JSON: `{"type":"setTrackGain","track":3,"gainDb":-6.0}`.
//! Mirrored by `ui/src/lib/types.ts` — keep in sync.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::{ClipId, Project, SourceId, SourceRef, TrackId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Edit {
    /// Appends a track (or inserts at `index`). Colour from `TRACK_COLORS[track count % 8]`.
    AddTrack { name: String, index: Option<usize> },
    RemoveTrack { track: TrackId },
    RenameTrack { track: TrackId, name: String },
    /// Moves a track to `index` (clamped to the valid range).
    MoveTrack { track: TrackId, index: usize },
    /// Clamped to `MIN_GAIN_DB..=MAX_TRACK_GAIN_DB`.
    SetTrackGain { track: TrackId, gain_db: f32 },
    /// Clamped to -1.0..=1.0.
    SetTrackPan { track: TrackId, pan: f32 },
    SetTrackMute { track: TrackId, mute: bool },
    SetTrackSolo { track: TrackId, solo: bool },
    SetTrackColor { track: TrackId, color: String },
    /// Clamped to `MIN_GAIN_DB..=MAX_TRACK_GAIN_DB`.
    SetMasterGain { gain_db: f32 },
    /// Registers an already-decoded source. `source.id` is ignored; a fresh id is allocated.
    AddSource { source: SourceRef },
    /// Places the whole source on `track` at `start`.
    AddClip { track: TrackId, source: SourceId, start: u64 },
    RemoveClip { clip: ClipId },
    /// Moves a clip to (possibly another) track and timeline position.
    MoveClip { clip: ClipId, track: TrackId, start: u64 },
    /// Sets new bounds. Must satisfy the `Clip` invariants against the source length;
    /// fades are shrunk if they no longer fit.
    TrimClip { clip: ClipId, start: u64, offset: u64, length: u64 },
    /// Splits at absolute timeline frame `at` (must be strictly inside the clip). The left part
    /// keeps the id and fade-in; the right part gets a new id and the fade-out.
    SplitClip { clip: ClipId, at: u64 },
    /// Fades are clamped so that `fade_in + fade_out <= length`.
    SetClipFades { clip: ClipId, fade_in: u64, fade_out: u64 },
    /// Clamped to `MIN_GAIN_DB..=MAX_CLIP_GAIN_DB`.
    SetClipGain { clip: ClipId, gain_db: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditOutcome {
    /// Id allocated by AddTrack / AddSource / AddClip / SplitClip (right part).
    pub created: Option<u64>,
}

/// Coalescing key: consecutive edits with the same `Some(key)` collapse into one undo step
/// (e.g. dragging a fader). `None` = never coalesce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoalesceKey {
    TrackGain(TrackId),
    TrackPan(TrackId),
    MasterGain,
    ClipGain(ClipId),
    ClipFades(ClipId),
    MoveClip(ClipId),
    TrimClip(ClipId),
}

impl Edit {
    pub fn coalesce_key(&self) -> Option<CoalesceKey> {
        todo!("history worker")
    }
}

/// Validates and applies `edit` to `project` in place. On error the project is unchanged.
/// Unknown ids → `EngineError::NotFound`; impossible geometry → `EngineError::InvalidEdit`.
/// Keeps `Track::clips` sorted by `start` and enforces `MAX_TRACKS`.
pub fn apply_edit(project: &mut Project, edit: &Edit) -> Result<EditOutcome> {
    let _ = (project, edit);
    todo!("history worker")
}
