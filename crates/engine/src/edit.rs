//! Every user change to a `Project` is an `Edit` (command pattern). The UI sends edits over IPC
//! as JSON: `{"type":"setTrackGain","track":3,"gainDb":-6.0}`.
//! Mirrored by `ui/src/lib/types.ts` — keep in sync.

use serde::{Deserialize, Serialize};

use crate::error::{EngineError, Result};
use crate::model::{
    Clip, ClipId, MAX_CLIP_GAIN_DB, MAX_TRACK_GAIN_DB, MAX_TRACKS, MIN_GAIN_DB, Project, SourceId,
    SourceRef, TRACK_COLORS, Track, TrackId,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Edit {
    /// Appends a track (or inserts at `index`). Colour from `TRACK_COLORS[track count % 8]`.
    AddTrack {
        name: String,
        index: Option<usize>,
    },
    RemoveTrack {
        track: TrackId,
    },
    RenameTrack {
        track: TrackId,
        name: String,
    },
    /// Moves a track to `index` (clamped to the valid range).
    MoveTrack {
        track: TrackId,
        index: usize,
    },
    /// Clamped to `MIN_GAIN_DB..=MAX_TRACK_GAIN_DB`.
    SetTrackGain {
        track: TrackId,
        gain_db: f32,
    },
    /// Clamped to -1.0..=1.0.
    SetTrackPan {
        track: TrackId,
        pan: f32,
    },
    SetTrackMute {
        track: TrackId,
        mute: bool,
    },
    SetTrackSolo {
        track: TrackId,
        solo: bool,
    },
    SetTrackColor {
        track: TrackId,
        color: String,
    },
    /// Clamped to `MIN_GAIN_DB..=MAX_TRACK_GAIN_DB`.
    SetMasterGain {
        gain_db: f32,
    },
    /// Registers an already-decoded source. `source.id` is ignored; a fresh id is allocated.
    AddSource {
        source: SourceRef,
    },
    /// Places the whole source on `track` at `start`.
    AddClip {
        track: TrackId,
        source: SourceId,
        start: u64,
    },
    RemoveClip {
        clip: ClipId,
    },
    /// Moves a clip to (possibly another) track and timeline position.
    MoveClip {
        clip: ClipId,
        track: TrackId,
        start: u64,
    },
    /// Sets new bounds. Must satisfy the `Clip` invariants against the source length;
    /// fades are shrunk if they no longer fit.
    TrimClip {
        clip: ClipId,
        start: u64,
        offset: u64,
        length: u64,
    },
    /// Splits at absolute timeline frame `at` (must be strictly inside the clip). The left part
    /// keeps the id and fade-in; the right part gets a new id and the fade-out.
    SplitClip {
        clip: ClipId,
        at: u64,
    },
    /// Fades are clamped so that `fade_in + fade_out <= length`.
    SetClipFades {
        clip: ClipId,
        fade_in: u64,
        fade_out: u64,
    },
    /// Clamped to `MIN_GAIN_DB..=MAX_CLIP_GAIN_DB`.
    SetClipGain {
        clip: ClipId,
        gain_db: f32,
    },
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
    /// Continuous adjustments share a key so one gesture undoes as one step. Structural edits
    /// are listed explicitly (not with a wildcard) so a new variant must decide its key.
    pub fn coalesce_key(&self) -> Option<CoalesceKey> {
        match self {
            Edit::SetTrackGain { track, .. } => Some(CoalesceKey::TrackGain(*track)),
            Edit::SetTrackPan { track, .. } => Some(CoalesceKey::TrackPan(*track)),
            Edit::SetMasterGain { .. } => Some(CoalesceKey::MasterGain),
            Edit::SetClipGain { clip, .. } => Some(CoalesceKey::ClipGain(*clip)),
            Edit::SetClipFades { clip, .. } => Some(CoalesceKey::ClipFades(*clip)),
            Edit::MoveClip { clip, .. } => Some(CoalesceKey::MoveClip(*clip)),
            Edit::TrimClip { clip, .. } => Some(CoalesceKey::TrimClip(*clip)),
            Edit::AddTrack { .. }
            | Edit::RemoveTrack { .. }
            | Edit::RenameTrack { .. }
            | Edit::MoveTrack { .. }
            | Edit::SetTrackMute { .. }
            | Edit::SetTrackSolo { .. }
            | Edit::SetTrackColor { .. }
            | Edit::AddSource { .. }
            | Edit::AddClip { .. }
            | Edit::RemoveClip { .. }
            | Edit::SplitClip { .. } => None,
        }
    }
}

/// Validates and applies `edit` to `project` in place. On error the project is unchanged.
/// Unknown ids → `EngineError::NotFound`; impossible geometry → `EngineError::InvalidEdit`.
/// Keeps `Track::clips` sorted by `start` and enforces `MAX_TRACKS`.
pub fn apply_edit(project: &mut Project, edit: &Edit) -> Result<EditOutcome> {
    // Edit a copy and swap it in only on success, so a rejected edit can never leave the
    // project half-modified. Projects hold no audio, so the copy is cheap.
    let mut next = project.clone();
    let created = apply_to(&mut next, edit)?;
    *project = next;
    Ok(EditOutcome { created })
}

fn apply_to(project: &mut Project, edit: &Edit) -> Result<Option<u64>> {
    match edit {
        Edit::AddTrack { name, index } => add_track(project, name, *index).map(Some),
        Edit::RemoveTrack { track } => remove_track(project, *track).map(|()| None),
        Edit::RenameTrack { track, name } => rename_track(project, *track, name).map(|()| None),
        Edit::MoveTrack { track, index } => move_track(project, *track, *index).map(|()| None),
        Edit::SetTrackGain { track, gain_db } => {
            set_track_gain(project, *track, *gain_db).map(|()| None)
        }
        Edit::SetTrackPan { track, pan } => set_track_pan(project, *track, *pan).map(|()| None),
        Edit::SetTrackMute { track, mute } => track_mut(project, *track)
            .map(|t| t.mute = *mute)
            .map(|()| None),
        Edit::SetTrackSolo { track, solo } => track_mut(project, *track)
            .map(|t| t.solo = *solo)
            .map(|()| None),
        Edit::SetTrackColor { track, color } => track_mut(project, *track)
            .map(|t| t.color = color.clone())
            .map(|()| None),
        Edit::SetMasterGain { gain_db } => {
            project.master.gain_db = clamp_gain(*gain_db, MAX_TRACK_GAIN_DB)?;
            Ok(None)
        }
        Edit::AddSource { source } => add_source(project, source).map(Some),
        Edit::AddClip {
            track,
            source,
            start,
        } => add_clip(project, *track, *source, *start).map(Some),
        Edit::RemoveClip { clip } => remove_clip(project, *clip).map(|()| None),
        Edit::MoveClip { clip, track, start } => {
            move_clip(project, *clip, *track, *start).map(|()| None)
        }
        Edit::TrimClip {
            clip,
            start,
            offset,
            length,
        } => trim_clip(project, *clip, *start, *offset, *length).map(|()| None),
        Edit::SplitClip { clip, at } => split_clip(project, *clip, *at).map(Some),
        Edit::SetClipFades {
            clip,
            fade_in,
            fade_out,
        } => set_clip_fades(project, *clip, *fade_in, *fade_out).map(|()| None),
        Edit::SetClipGain { clip, gain_db } => {
            set_clip_gain(project, *clip, *gain_db).map(|()| None)
        }
    }
}

fn invalid(msg: impl Into<String>) -> EngineError {
    EngineError::InvalidEdit(msg.into())
}

fn not_found(kind: &str, id: u64) -> EngineError {
    EngineError::NotFound(format!("{kind} {id}"))
}

fn overflow() -> EngineError {
    invalid("clip position overflows the timeline")
}

/// Trims and rejects blank names; a name is what the user sees, so whitespace is not a name.
fn normalize_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(invalid("track name must not be empty"));
    }
    Ok(trimmed.to_string())
}

/// NaN has no meaningful position on a fader, so it is rejected instead of clamped.
fn clamp_gain(gain_db: f32, max_db: f32) -> Result<f32> {
    if gain_db.is_nan() {
        return Err(invalid("gain must be a number"));
    }
    Ok(gain_db.clamp(MIN_GAIN_DB, max_db))
}

fn clamp_pan(pan: f32) -> Result<f32> {
    if pan.is_nan() {
        return Err(invalid("pan must be a number"));
    }
    Ok(pan.clamp(-1.0, 1.0))
}

fn track_index(project: &Project, id: TrackId) -> Result<usize> {
    project
        .tracks
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| not_found("track", id))
}

fn ensure_track(project: &Project, id: TrackId) -> Result<()> {
    track_index(project, id).map(|_| ())
}

fn track_mut(project: &mut Project, id: TrackId) -> Result<&mut Track> {
    project.track_mut(id).ok_or_else(|| not_found("track", id))
}

fn clip_pos(project: &Project, id: ClipId) -> Result<(usize, usize)> {
    project.find_clip(id).ok_or_else(|| not_found("clip", id))
}

fn clip_mut(project: &mut Project, id: ClipId) -> Result<&mut Clip> {
    let (ti, ci) = clip_pos(project, id)?;
    Ok(&mut project.tracks[ti].clips[ci])
}

fn source_frames(project: &Project, id: SourceId) -> Result<u64> {
    project
        .source(id)
        .map(|s| s.frames)
        .ok_or_else(|| not_found("source", id))
}

/// Rejects `start + length` values that would overflow `Clip::end`.
fn ensure_span(start: u64, length: u64) -> Result<()> {
    start.checked_add(length).map(|_| ()).ok_or_else(overflow)
}

/// Keeps a track's clips ordered by start. Equal starts keep insertion order, so a clip added
/// later at the same position plays after the earlier one.
fn insert_sorted(clips: &mut Vec<Clip>, clip: Clip) {
    let at = clips.partition_point(|c| c.start <= clip.start);
    clips.insert(at, clip);
}

fn add_track(project: &mut Project, name: &str, index: Option<usize>) -> Result<u64> {
    let name = normalize_name(name)?;
    let count = project.tracks.len();
    if count >= MAX_TRACKS {
        return Err(invalid(format!(
            "a project holds at most {MAX_TRACKS} tracks"
        )));
    }
    let id = project.alloc_id();
    let track = Track {
        id,
        name,
        gain_db: 0.0,
        pan: 0.0,
        mute: false,
        solo: false,
        color: TRACK_COLORS[count % TRACK_COLORS.len()].to_string(),
        clips: Vec::new(),
    };
    project
        .tracks
        .insert(index.map_or(count, |i| i.min(count)), track);
    Ok(id)
}

fn remove_track(project: &mut Project, id: TrackId) -> Result<()> {
    let at = track_index(project, id)?;
    project.tracks.remove(at);
    Ok(())
}

fn rename_track(project: &mut Project, id: TrackId, name: &str) -> Result<()> {
    let name = normalize_name(name)?;
    track_mut(project, id)?.name = name;
    Ok(())
}

fn move_track(project: &mut Project, id: TrackId, index: usize) -> Result<()> {
    let from = track_index(project, id)?;
    let track = project.tracks.remove(from);
    // After removal the list is one shorter, so `len()` is the last valid insert position.
    let to = index.min(project.tracks.len());
    project.tracks.insert(to, track);
    Ok(())
}

fn set_track_gain(project: &mut Project, id: TrackId, gain_db: f32) -> Result<()> {
    let gain_db = clamp_gain(gain_db, MAX_TRACK_GAIN_DB)?;
    track_mut(project, id)?.gain_db = gain_db;
    Ok(())
}

fn set_track_pan(project: &mut Project, id: TrackId, pan: f32) -> Result<()> {
    let pan = clamp_pan(pan)?;
    track_mut(project, id)?.pan = pan;
    Ok(())
}

fn add_source(project: &mut Project, source: &SourceRef) -> Result<u64> {
    if source.frames == 0 {
        return Err(invalid("source has no audio frames"));
    }
    if !matches!(source.channels, 1 | 2) {
        return Err(invalid(format!(
            "unsupported channel count {}",
            source.channels
        )));
    }
    let id = project.alloc_id();
    project.sources.push(SourceRef {
        id,
        ..source.clone()
    });
    Ok(id)
}

fn add_clip(project: &mut Project, track: TrackId, source: SourceId, start: u64) -> Result<u64> {
    ensure_track(project, track)?;
    let frames = source_frames(project, source)?;
    if frames == 0 {
        return Err(invalid("source has no audio frames"));
    }
    ensure_span(start, frames)?;
    let id = project.alloc_id();
    let clip = Clip {
        id,
        source_id: source,
        start,
        offset: 0,
        length: frames,
        gain_db: 0.0,
        fade_in: 0,
        fade_out: 0,
    };
    insert_sorted(&mut track_mut(project, track)?.clips, clip);
    Ok(id)
}

fn remove_clip(project: &mut Project, id: ClipId) -> Result<()> {
    let (ti, ci) = clip_pos(project, id)?;
    project.tracks[ti].clips.remove(ci);
    Ok(())
}

fn move_clip(project: &mut Project, id: ClipId, track: TrackId, start: u64) -> Result<()> {
    ensure_track(project, track)?;
    let (ti, ci) = clip_pos(project, id)?;
    let length = project.tracks[ti].clips[ci].length;
    ensure_span(start, length)?;
    // Removing from the source track keeps it sorted; the target insert keeps it sorted too.
    let mut clip = project.tracks[ti].clips.remove(ci);
    clip.start = start;
    insert_sorted(&mut track_mut(project, track)?.clips, clip);
    Ok(())
}

fn trim_clip(
    project: &mut Project,
    id: ClipId,
    start: u64,
    offset: u64,
    length: u64,
) -> Result<()> {
    if length == 0 {
        return Err(invalid("clip length must be at least one frame"));
    }
    let (ti, ci) = clip_pos(project, id)?;
    let frames = source_frames(project, project.tracks[ti].clips[ci].source_id)?;
    if offset.checked_add(length).is_none_or(|end| end > frames) {
        return Err(invalid("trim reaches past the end of the source"));
    }
    ensure_span(start, length)?;
    let track = &mut project.tracks[ti];
    let clip = &mut track.clips[ci];
    clip.start = start;
    clip.offset = offset;
    clip.length = length;
    clip.fade_in = clip.fade_in.min(length);
    clip.fade_out = clip.fade_out.min(length - clip.fade_in);
    track.clips.sort_by_key(|c| c.start);
    Ok(())
}

fn split_clip(project: &mut Project, id: ClipId, at: u64) -> Result<u64> {
    let (ti, ci) = clip_pos(project, id)?;
    let clip = project.tracks[ti].clips[ci].clone();
    let end = clip.start.checked_add(clip.length).ok_or_else(overflow)?;
    if at <= clip.start || at >= end {
        return Err(invalid("split point must lie strictly inside the clip"));
    }
    let left_len = at - clip.start;
    let right_len = end - at;
    let right_offset = clip.offset.checked_add(left_len).ok_or_else(overflow)?;
    let right_id = project.alloc_id();
    // Left keeps identity and fade-in; its fade-out would now sit at the cut, so it is removed.
    let left = Clip {
        length: left_len,
        fade_in: clip.fade_in.min(left_len),
        fade_out: 0,
        ..clip.clone()
    };
    // Right starts at the cut, so it has no fade-in and keeps the original fade-out.
    let right = Clip {
        id: right_id,
        start: at,
        offset: right_offset,
        length: right_len,
        fade_in: 0,
        fade_out: clip.fade_out.min(right_len),
        ..clip
    };
    project.tracks[ti].clips[ci] = left;
    insert_sorted(&mut project.tracks[ti].clips, right);
    Ok(right_id)
}

fn set_clip_fades(project: &mut Project, id: ClipId, fade_in: u64, fade_out: u64) -> Result<()> {
    let clip = clip_mut(project, id)?;
    clip.fade_in = fade_in.min(clip.length);
    clip.fade_out = fade_out.min(clip.length - clip.fade_in);
    Ok(())
}

fn set_clip_gain(project: &mut Project, id: ClipId, gain_db: f32) -> Result<()> {
    let gain_db = clamp_gain(gain_db, MAX_CLIP_GAIN_DB)?;
    clip_mut(project, id)?.gain_db = gain_db;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PROJECT_VERSION;

    fn source(frames: u64, channels: u16) -> SourceRef {
        SourceRef {
            id: 0,
            name: "take".into(),
            path: "take.wav".into(),
            channels,
            frames,
            original_sample_rate: 48_000,
        }
    }

    /// Project holding one 1000-frame stereo source (id 1); the next allocated id is 2.
    fn project() -> Project {
        let mut p = Project::new(48_000);
        apply_edit(
            &mut p,
            &Edit::AddSource {
                source: source(1_000, 2),
            },
        )
        .unwrap();
        p
    }

    fn new_track(p: &mut Project, name: &str) -> TrackId {
        let edit = Edit::AddTrack {
            name: name.into(),
            index: None,
        };
        apply_edit(p, &edit).unwrap().created.unwrap()
    }

    fn new_clip(p: &mut Project, track: TrackId, start: u64) -> ClipId {
        let edit = Edit::AddClip {
            track,
            source: 1,
            start,
        };
        apply_edit(p, &edit).unwrap().created.unwrap()
    }

    fn clip(p: &Project, id: ClipId) -> &Clip {
        let (ti, ci) = p.find_clip(id).unwrap();
        &p.tracks[ti].clips[ci]
    }

    fn clip_ids(p: &Project, track: TrackId) -> Vec<ClipId> {
        p.track(track).unwrap().clips.iter().map(|c| c.id).collect()
    }

    /// Applies a bad edit, asserts it failed and that the project is byte-for-byte unchanged.
    fn rejected(p: &mut Project, edit: &Edit) -> EngineError {
        let before = p.clone();
        let err = apply_edit(p, edit).expect_err("edit should be rejected");
        assert_eq!(
            *p, before,
            "a rejected edit must leave the project unchanged"
        );
        err
    }

    fn assert_invalid(p: &mut Project, edit: &Edit) {
        let err = rejected(p, edit);
        assert!(matches!(err, EngineError::InvalidEdit(_)), "got {err:?}");
    }

    fn assert_not_found(p: &mut Project, edit: &Edit) {
        let err = rejected(p, edit);
        assert!(matches!(err, EngineError::NotFound(_)), "got {err:?}");
    }

    #[test]
    fn add_track_appends_with_neutral_mix_settings() {
        let mut p = project();
        let outcome = apply_edit(
            &mut p,
            &Edit::AddTrack {
                name: "Vox".into(),
                index: None,
            },
        )
        .unwrap();

        let track = &p.tracks[0];
        assert_eq!(outcome.created, Some(track.id));
        assert_eq!(track.name, "Vox");
        assert_eq!((track.gain_db, track.pan), (0.0, 0.0));
        assert!(!track.mute && !track.solo);
        assert!(track.clips.is_empty());
    }

    #[test]
    fn add_track_colour_cycles_through_palette() {
        let mut p = project();
        for i in 0..9 {
            new_track(&mut p, &format!("t{i}"));
        }

        assert_eq!(p.tracks[0].color, TRACK_COLORS[0]);
        assert_eq!(p.tracks[1].color, TRACK_COLORS[1]);
        assert_eq!(
            p.tracks[8].color, TRACK_COLORS[0],
            "palette wraps after 8 tracks"
        );
    }

    #[test]
    fn add_track_inserts_at_index_and_clamps_past_end() {
        let mut p = project();
        let a = new_track(&mut p, "a");
        let b = new_track(&mut p, "b");
        let c = apply_edit(
            &mut p,
            &Edit::AddTrack {
                name: "c".into(),
                index: Some(1),
            },
        )
        .unwrap()
        .created
        .unwrap();
        let d = apply_edit(
            &mut p,
            &Edit::AddTrack {
                name: "d".into(),
                index: Some(99),
            },
        )
        .unwrap()
        .created
        .unwrap();

        let order: Vec<TrackId> = p.tracks.iter().map(|t| t.id).collect();
        assert_eq!(order, vec![a, c, b, d]);
    }

    #[test]
    fn add_track_rejects_blank_name() {
        let mut p = project();
        assert_invalid(
            &mut p,
            &Edit::AddTrack {
                name: "   ".into(),
                index: None,
            },
        );
    }

    #[test]
    fn add_track_rejects_when_track_limit_reached() {
        let mut p = project();
        for i in 0..MAX_TRACKS {
            new_track(&mut p, &format!("t{i}"));
        }

        assert_invalid(
            &mut p,
            &Edit::AddTrack {
                name: "extra".into(),
                index: None,
            },
        );
        assert_eq!(p.tracks.len(), MAX_TRACKS);
    }

    #[test]
    fn ids_are_monotonic_across_creating_edits() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c1 = new_clip(&mut p, t, 0);
        let split = apply_edit(&mut p, &Edit::SplitClip { clip: c1, at: 100 })
            .unwrap()
            .created
            .unwrap();
        let src = apply_edit(
            &mut p,
            &Edit::AddSource {
                source: source(10, 1),
            },
        )
        .unwrap()
        .created
        .unwrap();

        let ids = [t, c1, split, src];
        assert!(
            ids.windows(2).all(|w| w[0] < w[1]),
            "ids {ids:?} must strictly increase"
        );
        assert_eq!(p.next_id, src + 1);
    }

    #[test]
    fn remove_track_also_removes_its_clips_but_keeps_source() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 0);

        apply_edit(&mut p, &Edit::RemoveTrack { track: t }).unwrap();

        assert!(p.tracks.is_empty());
        assert!(p.find_clip(c).is_none());
        assert!(p.source(1).is_some());
    }

    #[test]
    fn remove_track_unknown_id_is_not_found() {
        let mut p = project();
        assert_not_found(&mut p, &Edit::RemoveTrack { track: 999 });
    }

    #[test]
    fn rename_track_trims_surrounding_whitespace() {
        let mut p = project();
        let t = new_track(&mut p, "old");

        apply_edit(
            &mut p,
            &Edit::RenameTrack {
                track: t,
                name: "  Drums  ".into(),
            },
        )
        .unwrap();

        assert_eq!(p.track(t).unwrap().name, "Drums");
    }

    #[test]
    fn rename_track_rejects_empty_name_and_keeps_old_name() {
        let mut p = project();
        let t = new_track(&mut p, "keep");

        assert_invalid(
            &mut p,
            &Edit::RenameTrack {
                track: t,
                name: "".into(),
            },
        );
        assert_eq!(p.track(t).unwrap().name, "keep");
    }

    #[test]
    fn move_track_moves_to_index_and_clamps_to_last_position() {
        let mut p = project();
        let a = new_track(&mut p, "a");
        let b = new_track(&mut p, "b");
        let c = new_track(&mut p, "c");

        apply_edit(&mut p, &Edit::MoveTrack { track: a, index: 1 }).unwrap();
        let order: Vec<TrackId> = p.tracks.iter().map(|t| t.id).collect();
        assert_eq!(order, vec![b, a, c]);

        apply_edit(
            &mut p,
            &Edit::MoveTrack {
                track: b,
                index: 50,
            },
        )
        .unwrap();
        let order: Vec<TrackId> = p.tracks.iter().map(|t| t.id).collect();
        assert_eq!(order, vec![a, c, b]);
    }

    #[test]
    fn set_track_gain_clamps_to_track_range() {
        let mut p = project();
        let t = new_track(&mut p, "t");

        apply_edit(
            &mut p,
            &Edit::SetTrackGain {
                track: t,
                gain_db: 100.0,
            },
        )
        .unwrap();
        assert_eq!(p.track(t).unwrap().gain_db, MAX_TRACK_GAIN_DB);

        apply_edit(
            &mut p,
            &Edit::SetTrackGain {
                track: t,
                gain_db: -200.0,
            },
        )
        .unwrap();
        assert_eq!(p.track(t).unwrap().gain_db, MIN_GAIN_DB);

        apply_edit(
            &mut p,
            &Edit::SetTrackGain {
                track: t,
                gain_db: -6.0,
            },
        )
        .unwrap();
        assert_eq!(p.track(t).unwrap().gain_db, -6.0);
    }

    #[test]
    fn set_track_gain_rejects_nan_and_keeps_previous_value() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        apply_edit(
            &mut p,
            &Edit::SetTrackGain {
                track: t,
                gain_db: -3.0,
            },
        )
        .unwrap();

        assert_invalid(
            &mut p,
            &Edit::SetTrackGain {
                track: t,
                gain_db: f32::NAN,
            },
        );
        assert_eq!(p.track(t).unwrap().gain_db, -3.0);
    }

    #[test]
    fn set_track_pan_clamps_to_unit_range_and_rejects_nan() {
        let mut p = project();
        let t = new_track(&mut p, "t");

        apply_edit(&mut p, &Edit::SetTrackPan { track: t, pan: 3.0 }).unwrap();
        assert_eq!(p.track(t).unwrap().pan, 1.0);
        apply_edit(
            &mut p,
            &Edit::SetTrackPan {
                track: t,
                pan: -0.25,
            },
        )
        .unwrap();
        assert_eq!(p.track(t).unwrap().pan, -0.25);
        assert_invalid(
            &mut p,
            &Edit::SetTrackPan {
                track: t,
                pan: f32::NAN,
            },
        );
    }

    #[test]
    fn set_track_mute_solo_and_color_update_their_fields() {
        let mut p = project();
        let t = new_track(&mut p, "t");

        apply_edit(
            &mut p,
            &Edit::SetTrackMute {
                track: t,
                mute: true,
            },
        )
        .unwrap();
        apply_edit(
            &mut p,
            &Edit::SetTrackSolo {
                track: t,
                solo: true,
            },
        )
        .unwrap();
        apply_edit(
            &mut p,
            &Edit::SetTrackColor {
                track: t,
                color: "#123456".into(),
            },
        )
        .unwrap();

        let track = p.track(t).unwrap();
        assert!(track.mute && track.solo);
        assert_eq!(track.color, "#123456");
    }

    #[test]
    fn set_master_gain_clamps_to_track_range_and_rejects_nan() {
        let mut p = project();

        apply_edit(&mut p, &Edit::SetMasterGain { gain_db: 50.0 }).unwrap();
        assert_eq!(p.master.gain_db, MAX_TRACK_GAIN_DB);
        assert_invalid(&mut p, &Edit::SetMasterGain { gain_db: f32::NAN });
        assert_eq!(p.master.gain_db, MAX_TRACK_GAIN_DB);
    }

    #[test]
    fn track_edits_with_unknown_id_are_not_found() {
        let mut p = project();
        let edits = [
            Edit::RenameTrack {
                track: 999,
                name: "x".into(),
            },
            Edit::MoveTrack {
                track: 999,
                index: 0,
            },
            Edit::SetTrackGain {
                track: 999,
                gain_db: 0.0,
            },
            Edit::SetTrackPan {
                track: 999,
                pan: 0.0,
            },
            Edit::SetTrackMute {
                track: 999,
                mute: true,
            },
            Edit::SetTrackSolo {
                track: 999,
                solo: true,
            },
            Edit::SetTrackColor {
                track: 999,
                color: "#000000".into(),
            },
        ];

        for edit in &edits {
            assert_not_found(&mut p, edit);
        }
    }

    #[test]
    fn add_source_allocates_fresh_id_and_ignores_given_id() {
        let mut p = project();
        let mut src = source(500, 1);
        src.id = 77;

        let outcome = apply_edit(&mut p, &Edit::AddSource { source: src }).unwrap();

        assert_eq!(outcome.created, Some(2));
        assert_eq!(p.sources.last().unwrap().id, 2);
    }

    #[test]
    fn add_source_rejects_empty_or_unsupported_channel_layouts() {
        let mut p = project();

        assert_invalid(
            &mut p,
            &Edit::AddSource {
                source: source(0, 2),
            },
        );
        assert_invalid(
            &mut p,
            &Edit::AddSource {
                source: source(10, 0),
            },
        );
        assert_invalid(
            &mut p,
            &Edit::AddSource {
                source: source(10, 3),
            },
        );
        assert_eq!(p.sources.len(), 1);
    }

    #[test]
    fn add_clip_spans_whole_source_with_neutral_settings() {
        let mut p = project();
        let t = new_track(&mut p, "t");

        let c = new_clip(&mut p, t, 250);

        let clip = clip(&p, c);
        assert_eq!((clip.source_id, clip.start), (1, 250));
        assert_eq!((clip.offset, clip.length), (0, 1_000));
        assert_eq!((clip.gain_db, clip.fade_in, clip.fade_out), (0.0, 0, 0));
    }

    #[test]
    fn add_clip_keeps_track_sorted_and_equal_starts_stable() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let late = new_clip(&mut p, t, 500);
        let early = new_clip(&mut p, t, 100);
        let mid = new_clip(&mut p, t, 300);
        let early_twin = new_clip(&mut p, t, 100);

        assert_eq!(clip_ids(&p, t), vec![early, early_twin, mid, late]);
    }

    #[test]
    fn add_clip_rejects_unknown_track_or_source() {
        let mut p = project();
        let t = new_track(&mut p, "t");

        assert_not_found(
            &mut p,
            &Edit::AddClip {
                track: 999,
                source: 1,
                start: 0,
            },
        );
        assert_not_found(
            &mut p,
            &Edit::AddClip {
                track: t,
                source: 999,
                start: 0,
            },
        );
    }

    #[test]
    fn add_clip_rejects_start_that_overflows_timeline() {
        let mut p = project();
        let t = new_track(&mut p, "t");

        assert_invalid(
            &mut p,
            &Edit::AddClip {
                track: t,
                source: 1,
                start: u64::MAX - 10,
            },
        );
    }

    #[test]
    fn remove_clip_drops_only_that_clip() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let keep = new_clip(&mut p, t, 0);
        let drop = new_clip(&mut p, t, 10);

        apply_edit(&mut p, &Edit::RemoveClip { clip: drop }).unwrap();

        assert_eq!(clip_ids(&p, t), vec![keep]);
    }

    #[test]
    fn remove_clip_unknown_id_is_not_found() {
        let mut p = project();
        assert_not_found(&mut p, &Edit::RemoveClip { clip: 999 });
    }

    #[test]
    fn move_clip_to_other_track_updates_both_tracks() {
        let mut p = project();
        let a = new_track(&mut p, "a");
        let b = new_track(&mut p, "b");
        let moving = new_clip(&mut p, a, 0);
        let resident = new_clip(&mut p, b, 50);

        apply_edit(
            &mut p,
            &Edit::MoveClip {
                clip: moving,
                track: b,
                start: 100,
            },
        )
        .unwrap();

        assert!(clip_ids(&p, a).is_empty());
        assert_eq!(clip_ids(&p, b), vec![resident, moving]);
        assert_eq!(clip(&p, moving).start, 100);
    }

    #[test]
    fn move_clip_within_track_resorts_by_new_start() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let first = new_clip(&mut p, t, 0);
        let second = new_clip(&mut p, t, 400);

        apply_edit(
            &mut p,
            &Edit::MoveClip {
                clip: first,
                track: t,
                start: 900,
            },
        )
        .unwrap();

        assert_eq!(clip_ids(&p, t), vec![second, first]);
    }

    #[test]
    fn move_clip_rejects_unknown_target_track_without_change() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 0);

        assert_not_found(
            &mut p,
            &Edit::MoveClip {
                clip: c,
                track: 999,
                start: 5,
            },
        );
        assert_eq!(clip(&p, c).start, 0);
    }

    #[test]
    fn move_clip_rejects_start_that_overflows_timeline() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 0);

        assert_invalid(
            &mut p,
            &Edit::MoveClip {
                clip: c,
                track: t,
                start: u64::MAX,
            },
        );
    }

    #[test]
    fn trim_clip_sets_bounds_inside_source() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 0);

        apply_edit(
            &mut p,
            &Edit::TrimClip {
                clip: c,
                start: 40,
                offset: 100,
                length: 300,
            },
        )
        .unwrap();

        let clip = clip(&p, c);
        assert_eq!((clip.start, clip.offset, clip.length), (40, 100, 300));
    }

    #[test]
    fn trim_clip_shrinks_fades_to_fit_new_length() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 0);
        apply_edit(
            &mut p,
            &Edit::SetClipFades {
                clip: c,
                fade_in: 100,
                fade_out: 300,
            },
        )
        .unwrap();

        apply_edit(
            &mut p,
            &Edit::TrimClip {
                clip: c,
                start: 0,
                offset: 0,
                length: 200,
            },
        )
        .unwrap();

        let clip = clip(&p, c);
        assert_eq!((clip.fade_in, clip.fade_out), (100, 100));
        assert!(clip.fade_in + clip.fade_out <= clip.length);
    }

    #[test]
    fn trim_clip_shrinks_fade_in_when_shorter_than_it() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 0);
        apply_edit(
            &mut p,
            &Edit::SetClipFades {
                clip: c,
                fade_in: 600,
                fade_out: 300,
            },
        )
        .unwrap();

        apply_edit(
            &mut p,
            &Edit::TrimClip {
                clip: c,
                start: 0,
                offset: 0,
                length: 50,
            },
        )
        .unwrap();

        assert_eq!((clip(&p, c).fade_in, clip(&p, c).fade_out), (50, 0));
    }

    #[test]
    fn trim_clip_rejects_zero_length() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 0);

        assert_invalid(
            &mut p,
            &Edit::TrimClip {
                clip: c,
                start: 0,
                offset: 0,
                length: 0,
            },
        );
    }

    #[test]
    fn trim_clip_rejects_range_past_source_end() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 0);

        assert_invalid(
            &mut p,
            &Edit::TrimClip {
                clip: c,
                start: 0,
                offset: 900,
                length: 200,
            },
        );
        assert_invalid(
            &mut p,
            &Edit::TrimClip {
                clip: c,
                start: 0,
                offset: u64::MAX,
                length: 2,
            },
        );
    }

    #[test]
    fn trim_clip_resorts_when_start_moves_past_neighbour() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let first = new_clip(&mut p, t, 0);
        let second = new_clip(&mut p, t, 200);

        apply_edit(
            &mut p,
            &Edit::TrimClip {
                clip: first,
                start: 300,
                offset: 0,
                length: 100,
            },
        )
        .unwrap();

        assert_eq!(clip_ids(&p, t), vec![second, first]);
    }

    #[test]
    fn split_clip_produces_left_and_right_geometry() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 100);
        apply_edit(
            &mut p,
            &Edit::TrimClip {
                clip: c,
                start: 100,
                offset: 50,
                length: 400,
            },
        )
        .unwrap();
        apply_edit(
            &mut p,
            &Edit::SetClipFades {
                clip: c,
                fade_in: 40,
                fade_out: 60,
            },
        )
        .unwrap();
        apply_edit(
            &mut p,
            &Edit::SetClipGain {
                clip: c,
                gain_db: -3.0,
            },
        )
        .unwrap();

        let right_id = apply_edit(&mut p, &Edit::SplitClip { clip: c, at: 300 })
            .unwrap()
            .created
            .unwrap();

        let left = clip(&p, c).clone();
        let right = clip(&p, right_id).clone();
        assert_eq!((left.start, left.offset, left.length), (100, 50, 200));
        assert_eq!((left.fade_in, left.fade_out), (40, 0));
        assert_eq!(right.id, right_id);
        assert_eq!((right.start, right.offset, right.length), (300, 250, 200));
        assert_eq!((right.fade_in, right.fade_out), (0, 60));
        assert_eq!(left.gain_db, -3.0);
        assert_eq!(right.gain_db, -3.0);
        assert_eq!(clip_ids(&p, t), vec![c, right_id]);
    }

    #[test]
    fn split_clip_clamps_fades_to_each_part() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 0);
        apply_edit(
            &mut p,
            &Edit::SetClipFades {
                clip: c,
                fade_in: 150,
                fade_out: 300,
            },
        )
        .unwrap();

        let right_id = apply_edit(&mut p, &Edit::SplitClip { clip: c, at: 50 })
            .unwrap()
            .created
            .unwrap();
        let left = clip(&p, c).clone();
        assert_eq!((left.length, left.fade_in, left.fade_out), (50, 50, 0));
        assert_eq!(
            clip(&p, right_id).fade_out,
            300,
            "right length 950 keeps a 300 fade-out"
        );

        let tail_id = apply_edit(
            &mut p,
            &Edit::SplitClip {
                clip: right_id,
                at: 900,
            },
        )
        .unwrap()
        .created
        .unwrap();
        assert_eq!(clip(&p, right_id).fade_out, 0);
        assert_eq!(clip(&p, tail_id).length, 100);
        assert_eq!(
            clip(&p, tail_id).fade_out,
            100,
            "300 fade-out clamps to the 100-frame tail"
        );
    }

    #[test]
    fn split_clip_rejects_points_outside_clip_interior() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 100);

        assert_invalid(&mut p, &Edit::SplitClip { clip: c, at: 100 });
        assert_invalid(&mut p, &Edit::SplitClip { clip: c, at: 50 });
        assert_invalid(&mut p, &Edit::SplitClip { clip: c, at: 1_100 });
        assert_invalid(&mut p, &Edit::SplitClip { clip: c, at: 2_000 });
        assert_eq!(p.tracks[0].clips.len(), 1);
    }

    #[test]
    fn split_clip_inserts_right_part_in_sorted_position() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 0);
        let neighbour = new_clip(&mut p, t, 200);

        let right = apply_edit(&mut p, &Edit::SplitClip { clip: c, at: 300 })
            .unwrap()
            .created
            .unwrap();

        assert_eq!(clip_ids(&p, t), vec![c, neighbour, right]);
    }

    #[test]
    fn split_clip_unknown_id_is_not_found() {
        let mut p = project();
        assert_not_found(&mut p, &Edit::SplitClip { clip: 999, at: 1 });
    }

    #[test]
    fn set_clip_fades_clamps_so_fades_fit_length() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 0);
        apply_edit(
            &mut p,
            &Edit::TrimClip {
                clip: c,
                start: 0,
                offset: 0,
                length: 100,
            },
        )
        .unwrap();

        apply_edit(
            &mut p,
            &Edit::SetClipFades {
                clip: c,
                fade_in: 300,
                fade_out: 300,
            },
        )
        .unwrap();
        assert_eq!((clip(&p, c).fade_in, clip(&p, c).fade_out), (100, 0));

        apply_edit(
            &mut p,
            &Edit::SetClipFades {
                clip: c,
                fade_in: 30,
                fade_out: 100,
            },
        )
        .unwrap();
        assert_eq!((clip(&p, c).fade_in, clip(&p, c).fade_out), (30, 70));
    }

    #[test]
    fn set_clip_gain_clamps_to_clip_range_and_rejects_nan() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let c = new_clip(&mut p, t, 0);

        apply_edit(
            &mut p,
            &Edit::SetClipGain {
                clip: c,
                gain_db: 30.0,
            },
        )
        .unwrap();
        assert_eq!(clip(&p, c).gain_db, MAX_CLIP_GAIN_DB);
        apply_edit(
            &mut p,
            &Edit::SetClipGain {
                clip: c,
                gain_db: -500.0,
            },
        )
        .unwrap();
        assert_eq!(clip(&p, c).gain_db, MIN_GAIN_DB);
        assert_invalid(
            &mut p,
            &Edit::SetClipGain {
                clip: c,
                gain_db: f32::NAN,
            },
        );
        assert_eq!(clip(&p, c).gain_db, MIN_GAIN_DB);
    }

    #[test]
    fn clip_edits_with_unknown_id_are_not_found() {
        let mut p = project();
        let t = new_track(&mut p, "t");
        let _ = new_clip(&mut p, t, 0);
        let edits = [
            Edit::TrimClip {
                clip: 999,
                start: 0,
                offset: 0,
                length: 1,
            },
            Edit::SetClipFades {
                clip: 999,
                fade_in: 0,
                fade_out: 0,
            },
            Edit::SetClipGain {
                clip: 999,
                gain_db: 0.0,
            },
        ];

        for edit in &edits {
            assert_not_found(&mut p, edit);
        }
    }

    #[test]
    fn non_creating_edits_report_no_created_id() {
        let mut p = project();
        let t = new_track(&mut p, "t");

        let outcome = apply_edit(
            &mut p,
            &Edit::SetTrackGain {
                track: t,
                gain_db: 0.0,
            },
        )
        .unwrap();

        assert_eq!(outcome, EditOutcome { created: None });
    }

    #[test]
    fn coalesce_key_is_set_only_for_continuous_gestures() {
        assert_eq!(
            Edit::SetTrackGain {
                track: 3,
                gain_db: 0.0
            }
            .coalesce_key(),
            Some(CoalesceKey::TrackGain(3))
        );
        assert_eq!(
            Edit::SetTrackPan { track: 3, pan: 0.0 }.coalesce_key(),
            Some(CoalesceKey::TrackPan(3))
        );
        assert_eq!(
            Edit::SetMasterGain { gain_db: 0.0 }.coalesce_key(),
            Some(CoalesceKey::MasterGain)
        );
        assert_eq!(
            Edit::SetClipGain {
                clip: 4,
                gain_db: 0.0
            }
            .coalesce_key(),
            Some(CoalesceKey::ClipGain(4))
        );
        assert_eq!(
            Edit::SetClipFades {
                clip: 4,
                fade_in: 0,
                fade_out: 0
            }
            .coalesce_key(),
            Some(CoalesceKey::ClipFades(4))
        );
        assert_eq!(
            Edit::MoveClip {
                clip: 4,
                track: 1,
                start: 0
            }
            .coalesce_key(),
            Some(CoalesceKey::MoveClip(4))
        );
        assert_eq!(
            Edit::TrimClip {
                clip: 4,
                start: 0,
                offset: 0,
                length: 1
            }
            .coalesce_key(),
            Some(CoalesceKey::TrimClip(4))
        );
        assert_eq!(
            Edit::SetTrackMute {
                track: 3,
                mute: true
            }
            .coalesce_key(),
            None
        );
        assert_eq!(Edit::SplitClip { clip: 4, at: 1 }.coalesce_key(), None);
        assert_eq!(Edit::RemoveClip { clip: 4 }.coalesce_key(), None);
    }

    #[test]
    fn project_version_is_unchanged_by_edits() {
        let mut p = project();
        new_track(&mut p, "t");

        assert_eq!(p.version, PROJECT_VERSION);
    }
}
