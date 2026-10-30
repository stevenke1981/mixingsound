//! `RenderGraph`: an immutable, fully-resolved snapshot of the project that the mixer renders.
//! Built on the UI/control thread after every edit and handed to the audio thread as
//! `Arc<RenderGraph>`. dB → linear, mute/solo → `audible`, ids → audio buffers are all
//! resolved here so `render` does no lookups.

use std::collections::HashMap;
use std::sync::Arc;

use crate::buffer::AudioBuffer;
use crate::dsp::db_to_gain;
use crate::error::{EngineError, Result};
use crate::model::{MAX_TRACKS, Project, SourceId, TrackId};

/// Decoded audio for every source, keyed by `SourceRef::id`.
pub type SourcePool = HashMap<SourceId, Arc<AudioBuffer>>;

#[derive(Debug, Clone)]
pub struct GraphClip {
    pub buffer: Arc<AudioBuffer>,
    pub start: u64,
    pub offset: u64,
    pub length: u64,
    /// Linear clip gain.
    pub gain: f32,
    pub fade_in: u64,
    pub fade_out: u64,
}

impl GraphClip {
    pub fn end(&self) -> u64 {
        self.start + self.length
    }
}

#[derive(Debug, Clone)]
pub struct GraphTrack {
    pub id: TrackId,
    /// Index into the mixer's preallocated per-track state (`< MAX_TRACKS`).
    pub slot: usize,
    /// Linear fader gain.
    pub gain: f32,
    pub pan: f32,
    /// false when muted, or when another track is soloed and this one is not.
    pub audible: bool,
    /// Sorted by `start`.
    pub clips: Vec<GraphClip>,
}

#[derive(Debug, Clone)]
pub struct RenderGraph {
    pub sample_rate: u32,
    pub tracks: Vec<GraphTrack>,
    /// Linear master gain.
    pub master_gain: f32,
    /// Exclusive end of the last clip.
    pub length: u64,
}

impl RenderGraph {
    pub fn empty(sample_rate: u32) -> Self {
        Self { sample_rate, tracks: Vec::new(), master_gain: 1.0, length: 0 }
    }
}

/// Resolves `project` against `pool`. Clips whose source is not in the pool (offline media)
/// are skipped so the rest of the project still plays.
pub fn build_graph(project: &Project, pool: &SourcePool) -> Result<RenderGraph> {
    if project.tracks.len() > MAX_TRACKS {
        return Err(EngineError::InvalidEdit(format!("more than {MAX_TRACKS} tracks")));
    }
    let any_solo = project.tracks.iter().any(|t| t.solo);
    let tracks = project
        .tracks
        .iter()
        .enumerate()
        .map(|(slot, track)| GraphTrack {
            id: track.id,
            slot,
            gain: db_to_gain(track.gain_db),
            pan: track.pan.clamp(-1.0, 1.0),
            audible: !track.mute && (!any_solo || track.solo),
            clips: track
                .clips
                .iter()
                .filter_map(|clip| {
                    pool.get(&clip.source_id).map(|buffer| GraphClip {
                        buffer: Arc::clone(buffer),
                        start: clip.start,
                        offset: clip.offset,
                        length: clip.length,
                        gain: db_to_gain(clip.gain_db),
                        fade_in: clip.fade_in,
                        fade_out: clip.fade_out,
                    })
                })
                .collect(),
        })
        .collect();
    Ok(RenderGraph {
        sample_rate: project.sample_rate,
        tracks,
        master_gain: db_to_gain(project.master.gain_db),
        length: project.length(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Clip, Track};

    fn track(id: TrackId, mute: bool, solo: bool) -> Track {
        Track {
            id,
            name: format!("T{id}"),
            gain_db: 0.0,
            pan: 0.0,
            mute,
            solo,
            color: "#fff".into(),
            clips: vec![Clip {
                id: id + 100,
                source_id: 1,
                start: 10,
                offset: 0,
                length: 5,
                gain_db: 0.0,
                fade_in: 0,
                fade_out: 0,
            }],
        }
    }

    #[test]
    fn resolves_solo_mute_and_missing_sources() {
        let mut p = Project::new(48_000);
        p.tracks = vec![track(1, false, true), track(2, false, false), track(3, true, true)];
        let mut pool = SourcePool::new();
        pool.insert(1, Arc::new(AudioBuffer::new(vec![vec![0.0; 5]])));
        let g = build_graph(&p, &pool).unwrap();
        let audible: Vec<bool> = g.tracks.iter().map(|t| t.audible).collect();
        assert_eq!(audible, [true, false, false]);
        assert_eq!(g.length, 15);
        assert_eq!(g.tracks[0].clips.len(), 1);

        let g = build_graph(&p, &SourcePool::new()).unwrap();
        assert!(g.tracks.iter().all(|t| t.clips.is_empty()));
    }
}
