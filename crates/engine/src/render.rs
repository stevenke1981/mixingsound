//! The mixer: renders a `RenderGraph` into stereo output. Shared by the real-time driver and
//! offline bounce, so both produce identical audio.
//!
//! Signal flow per track: Σ clips (source sample × clip gain × fade) → pan (mono: constant
//! power; stereo: balance) → fader gain × audible (smoothed) → track meter → sum.
//! Master: Σ tracks × master gain (smoothed) → master meter → output.
//! A mono source contributes to both channels via `pan_mono`.

use serde::Serialize;

use crate::graph::RenderGraph;
use crate::model::{MAX_TRACKS, TrackId};

/// Largest block `render` processes in one internal pass; longer outputs are chunked.
pub const MAX_BLOCK: usize = 4096;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StereoLevel {
    /// Linear sample peak since the last `take_meters`.
    pub peak_l: f32,
    pub peak_r: f32,
    /// Linear RMS since the last `take_meters` (0 if no frames rendered).
    pub rms_l: f32,
    pub rms_r: f32,
}

/// Meter readout. Fixed-size and `Copy` so it can travel through a lock-free ring buffer.
#[derive(Debug, Clone, Copy)]
pub struct MeterSnapshot {
    pub master: StereoLevel,
    /// Valid entries: `0..track_count`, in graph track order.
    pub tracks: [StereoLevel; MAX_TRACKS],
    pub track_ids: [TrackId; MAX_TRACKS],
    pub track_count: usize,
    /// True if any master output sample exceeded ±1.0 since the last take.
    pub clipped: bool,
}

impl Default for MeterSnapshot {
    fn default() -> Self {
        Self {
            master: StereoLevel::default(),
            tracks: [StereoLevel::default(); MAX_TRACKS],
            track_ids: [0; MAX_TRACKS],
            track_count: 0,
            clipped: false,
        }
    }
}

/// Holds all per-track state (smoothers, scratch buffers, meter accumulators), preallocated
/// for `MAX_TRACKS` in `new`. Smoother state is keyed by `GraphTrack::slot`; if the track id in
/// a slot changes between renders, that slot's smoothers snap to the new targets.
pub struct Mixer {
    // private fields: mixer worker
}

impl Mixer {
    /// Allocates everything the mixer will ever need.
    pub fn new(sample_rate: u32) -> Self {
        let _ = sample_rate;
        todo!("mixer worker")
    }

    /// Forget smoothing history (next render snaps to graph targets) and clear meters.
    /// Call on seek/stop and before a bounce.
    pub fn reset(&mut self) {
        todo!("mixer worker")
    }

    /// Renders timeline frames `pos .. pos + out_l.len()` into `out_l`/`out_r` (overwritten,
    /// not added). `out_l.len() == out_r.len()`; any length allowed. Output is NOT clamped.
    /// Real-time safe: no allocation, locking, I/O or logging.
    pub fn render(&mut self, graph: &RenderGraph, pos: u64, out_l: &mut [f32], out_r: &mut [f32]) {
        let _ = (graph, pos, out_l, out_r);
        todo!("mixer worker")
    }

    /// Copies accumulated meter values into `out` and resets the accumulators.
    pub fn take_meters(&mut self, out: &mut MeterSnapshot) {
        let _ = out;
        todo!("mixer worker")
    }
}
