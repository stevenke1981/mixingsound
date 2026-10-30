//! Project data model. Serialised to JSON (project file) and sent to the UI over IPC,
//! so field names are camelCase on the wire. Mirrored by `ui/src/lib/types.ts` — keep in sync.
//!
//! Units:
//! * all positions/lengths are **frames at `Project::sample_rate`** (u64);
//! * gains are dB (f32), `-inf` is represented as `MIN_GAIN_DB` or below;
//! * pan is -1.0 (hard left) ..= 1.0 (hard right).

use serde::{Deserialize, Serialize};

pub type TrackId = u64;
pub type ClipId = u64;
pub type SourceId = u64;

pub const PROJECT_VERSION: u32 = 1;
/// Gains at or below this value are treated as silence (linear 0.0).
pub const MIN_GAIN_DB: f32 = -96.0;
pub const MAX_TRACK_GAIN_DB: f32 = 12.0;
pub const MAX_CLIP_GAIN_DB: f32 = 24.0;
/// Hard limit on tracks; the real-time mixer preallocates per-track state for this many.
pub const MAX_TRACKS: usize = 128;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub version: u32,
    pub sample_rate: u32,
    /// Track order is display and mix order.
    pub tracks: Vec<Track>,
    pub master: MasterBus,
    pub sources: Vec<SourceRef>,
    /// Monotonic id allocator shared by tracks, clips and sources. Never reused.
    pub next_id: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub gain_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    /// CSS colour string, e.g. "#d98c3f".
    pub color: String,
    /// Kept sorted by `start`. Overlapping clips on one track are summed.
    pub clips: Vec<Clip>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: ClipId,
    pub source_id: SourceId,
    /// Timeline position of the clip's first frame.
    pub start: u64,
    /// First source frame played (left-trim amount).
    pub offset: u64,
    /// Number of frames played. Invariant: `offset + length <= source.frames`, `length >= 1`.
    pub length: u64,
    pub gain_db: f32,
    /// Fade lengths in frames. Invariant: `fade_in + fade_out <= length`.
    pub fade_in: u64,
    pub fade_out: u64,
}

impl Clip {
    /// Exclusive timeline end.
    pub fn end(&self) -> u64 {
        self.start + self.length
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterBus {
    pub gain_db: f32,
}

/// An imported audio file. Audio data itself lives in `graph::SourcePool`, never in the project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub id: SourceId,
    /// Display name (file stem).
    pub name: String,
    /// In memory: absolute path. On disk (project file): relative to the project file's
    /// directory when possible — `project_io` converts both ways.
    pub path: String,
    /// 1 or 2 (decoder folds >2 channels to stereo).
    pub channels: u16,
    /// Length in frames **at the project sample rate** (after resampling).
    pub frames: u64,
    pub original_sample_rate: u32,
}

/// Default track colours, cycled by track index.
pub const TRACK_COLORS: [&str; 8] = [
    "#d98c3f", "#c9b458", "#7fae6a", "#4fa3a5", "#5b86c9", "#8e6fc4", "#c46f9f", "#c95f55",
];

impl Project {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            version: PROJECT_VERSION,
            sample_rate,
            tracks: Vec::new(),
            master: MasterBus { gain_db: 0.0 },
            sources: Vec::new(),
            next_id: 1,
        }
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    pub fn source(&self, id: SourceId) -> Option<&SourceRef> {
        self.sources.iter().find(|s| s.id == id)
    }

    /// Returns (track index, clip index).
    pub fn find_clip(&self, id: ClipId) -> Option<(usize, usize)> {
        self.tracks.iter().enumerate().find_map(|(ti, t)| {
            t.clips.iter().position(|c| c.id == id).map(|ci| (ti, ci))
        })
    }

    /// Copy of the project re-timed to `sample_rate`: every frame quantity (clip start/offset/
    /// length/fades, source frames) is scaled and rounded, then clip invariants are re-enforced
    /// (length ≥ 1, offset + length ≤ source frames, fade_in + fade_out ≤ length).
    /// Used when opening a project whose rate differs from the output device.
    pub fn with_sample_rate(&self, sample_rate: u32) -> Project {
        let scale = |v: u64| -> u64 {
            ((v as u128 * sample_rate as u128 + self.sample_rate as u128 / 2)
                / self.sample_rate as u128) as u64
        };
        let mut out = self.clone();
        out.sample_rate = sample_rate;
        for s in &mut out.sources {
            s.frames = scale(s.frames);
        }
        let frames_of = |id: SourceId| out.sources.iter().find(|s| s.id == id).map(|s| s.frames);
        let source_frames: Vec<Option<u64>> = out
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|c| frames_of(c.source_id)))
            .collect();
        let mut frames_iter = source_frames.into_iter();
        for t in &mut out.tracks {
            for c in &mut t.clips {
                let frames = frames_iter.next().flatten().unwrap_or(u64::MAX).max(1);
                c.start = scale(c.start);
                c.offset = scale(c.offset).min(frames - 1);
                c.length = scale(c.length).clamp(1, frames - c.offset);
                c.fade_in = scale(c.fade_in).min(c.length);
                c.fade_out = scale(c.fade_out).min(c.length - c.fade_in);
            }
        }
        out
    }

    /// Exclusive end of the last clip on any track (0 for an empty project).
    pub fn length(&self) -> u64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(Clip::end))
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_uses_camel_case() {
        let mut p = Project::new(48_000);
        p.master.gain_db = -1.5;
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"sampleRate\":48000"));
        assert!(json.contains("\"nextId\":1"));
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn with_sample_rate_rescales_and_keeps_invariants() {
        let mut p = Project::new(44_100);
        p.sources.push(SourceRef {
            id: 1,
            name: "a".into(),
            path: "a.wav".into(),
            channels: 1,
            frames: 44_100,
            original_sample_rate: 44_100,
        });
        p.tracks.push(Track {
            id: 2,
            name: "t".into(),
            gain_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            color: TRACK_COLORS[0].into(),
            clips: vec![Clip {
                id: 3,
                source_id: 1,
                start: 44_100,
                offset: 22_050,
                length: 22_050,
                gain_db: 0.0,
                fade_in: 441,
                fade_out: 441,
            }],
        });
        let q = p.with_sample_rate(48_000);
        let c = &q.tracks[0].clips[0];
        assert_eq!(q.sources[0].frames, 48_000);
        assert_eq!((c.start, c.offset, c.length, c.fade_in), (48_000, 24_000, 24_000, 480));
        assert!(c.offset + c.length <= q.sources[0].frames);
    }
}
