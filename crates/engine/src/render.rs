//! The mixer: renders a `RenderGraph` into stereo output. Shared by the real-time driver and
//! offline bounce, so both produce identical audio.
//!
//! Signal flow per track: Σ clips (source sample × clip gain × fade) → pan (mono: constant
//! power; stereo: balance) → fader gain × audible (smoothed) → track meter → sum.
//! Master: Σ tracks × master gain (smoothed) → master meter → output.
//! A mono source contributes to both channels via `pan_mono`.

use serde::Serialize;

use crate::dsp::{Smoother, fade_gain, pan_balance, pan_mono};
use crate::graph::{GraphClip, GraphTrack, RenderGraph};
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
    slots: Vec<TrackSlot>,
    master_gain: Smoother,
    master_meter: Accum,
    clipped: bool,
    /// False until the first render after `new`/`reset`; the master gain snaps on that render.
    primed: bool,
    /// Per-track scratch of `MAX_BLOCK` frames: mono sources, then stereo L/R before panning.
    track_mono: Vec<f32>,
    track_l: Vec<f32>,
    track_r: Vec<f32>,
    /// `(slot, id)` in graph order for the last render, read back by `take_meters`.
    graph_tracks: Vec<(usize, TrackId)>,
    track_count: usize,
}

/// Per-slot state. Smoothers and meters belong to the slot, not to the track, so a slot whose
/// track id changes starts fresh instead of ramping from the previous track's level.
struct TrackSlot {
    /// Id rendered in this slot by the last render; `None` if the slot was absent from it.
    track_id: Option<TrackId>,
    gain: Smoother,
    pan: Smoother,
    meter: Accum,
}

impl TrackSlot {
    fn new(sample_rate: u32) -> Self {
        Self {
            track_id: None,
            gain: Smoother::new(0.0, sample_rate),
            pan: Smoother::new(0.0, sample_rate),
            meter: Accum::default(),
        }
    }
}

/// Running peak and sum of squares for one stereo meter. `f64` sums keep RMS accurate over
/// long meter windows.
#[derive(Debug, Clone, Copy, Default)]
struct Accum {
    peak_l: f32,
    peak_r: f32,
    sum_sq_l: f64,
    sum_sq_r: f64,
    frames: u64,
}

impl Accum {
    fn push(&mut self, l: f32, r: f32) {
        self.peak_l = self.peak_l.max(l.abs());
        self.peak_r = self.peak_r.max(r.abs());
        self.sum_sq_l += f64::from(l) * f64::from(l);
        self.sum_sq_r += f64::from(r) * f64::from(r);
        self.frames = self.frames.saturating_add(1);
    }

    fn level(&self) -> StereoLevel {
        let rms = |sum_sq: f64| {
            if self.frames == 0 {
                0.0
            } else {
                (sum_sq / self.frames as f64).sqrt() as f32
            }
        };
        StereoLevel {
            peak_l: self.peak_l,
            peak_r: self.peak_r,
            rms_l: rms(self.sum_sq_l),
            rms_r: rms(self.sum_sq_r),
        }
    }
}

/// Pan coefficients for one value of pan, for both the mono and the stereo law.
#[derive(Debug, Clone, Copy)]
struct PanGains {
    mono_l: f32,
    mono_r: f32,
    bal_l: f32,
    bal_r: f32,
}

impl PanGains {
    fn new(pan: f32) -> Self {
        let (mono_l, mono_r) = pan_mono(pan);
        let (bal_l, bal_r) = pan_balance(pan);
        Self {
            mono_l,
            mono_r,
            bal_l,
            bal_r,
        }
    }
}

impl Mixer {
    /// Allocates everything the mixer will ever need.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            slots: (0..MAX_TRACKS)
                .map(|_| TrackSlot::new(sample_rate))
                .collect(),
            master_gain: Smoother::new(1.0, sample_rate),
            master_meter: Accum::default(),
            clipped: false,
            primed: false,
            track_mono: vec![0.0; MAX_BLOCK],
            track_l: vec![0.0; MAX_BLOCK],
            track_r: vec![0.0; MAX_BLOCK],
            graph_tracks: vec![(0, 0); MAX_TRACKS],
            track_count: 0,
        }
    }

    /// Forget smoothing history (next render snaps to graph targets) and clear meters.
    /// Call on seek/stop and before a bounce.
    pub fn reset(&mut self) {
        for slot in &mut self.slots {
            slot.track_id = None;
            slot.meter = Accum::default();
        }
        self.master_meter = Accum::default();
        self.clipped = false;
        self.primed = false;
    }

    /// Renders timeline frames `pos .. pos + out_l.len()` into `out_l`/`out_r` (overwritten,
    /// not added). `out_l.len() == out_r.len()`; any length allowed. Output is NOT clamped.
    /// Real-time safe: no allocation, locking, I/O or logging.
    pub fn render(&mut self, graph: &RenderGraph, pos: u64, out_l: &mut [f32], out_r: &mut [f32]) {
        // Targets are set once per call, so chunking inside this call cannot restart a ramp.
        self.begin(graph);
        let total = out_l.len().min(out_r.len());
        let mut done = 0;
        while done < total {
            let n = (total - done).min(MAX_BLOCK);
            let (Some(l), Some(r)) = (out_l.get_mut(done..done + n), out_r.get_mut(done..done + n))
            else {
                break;
            };
            self.render_block(graph, pos.saturating_add(done as u64), l, r);
            done += n;
        }
    }

    /// Copies accumulated meter values into `out` and resets the accumulators.
    pub fn take_meters(&mut self, out: &mut MeterSnapshot) {
        out.master = self.master_meter.level();
        out.clipped = self.clipped;
        out.track_count = self.track_count;
        let rows = out.tracks.iter_mut().zip(out.track_ids.iter_mut());
        for (i, (level, id)) in rows.enumerate().take(self.track_count) {
            let (slot, track_id) = self.graph_tracks.get(i).copied().unwrap_or((usize::MAX, 0));
            *id = track_id;
            *level = self
                .slots
                .get(slot)
                .map_or_else(StereoLevel::default, |s| s.meter.level());
        }
        for slot in &mut self.slots {
            slot.meter = Accum::default();
        }
        self.master_meter = Accum::default();
        self.clipped = false;
    }

    /// Records the graph's track order and applies targets to every slot. A slot whose id
    /// changed (or was absent last render) snaps instead of ramping.
    fn begin(&mut self, graph: &RenderGraph) {
        let mut present = [false; MAX_TRACKS];
        self.track_count = graph.tracks.len().min(MAX_TRACKS);
        for (i, track) in graph.tracks.iter().enumerate() {
            if let Some(entry) = self.graph_tracks.get_mut(i) {
                *entry = (track.slot, track.id);
            }
            let (Some(slot), Some(seen)) =
                (self.slots.get_mut(track.slot), present.get_mut(track.slot))
            else {
                continue;
            };
            *seen = true;
            apply_track_params(slot, track);
        }
        for (slot, is_present) in self.slots.iter_mut().zip(present) {
            if !is_present {
                slot.track_id = None;
            }
        }
        if self.primed {
            self.master_gain.set_target(graph.master_gain);
        } else {
            self.master_gain.snap(graph.master_gain);
            self.primed = true;
        }
    }

    /// Renders one block of at most `MAX_BLOCK` frames. Track output is accumulated straight
    /// into `out_*`, then the master gain is applied in place.
    fn render_block(
        &mut self,
        graph: &RenderGraph,
        pos: u64,
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) {
        let n = out_l.len();
        let (Some(mono), Some(left), Some(right)) = (
            self.track_mono.get_mut(..n),
            self.track_l.get_mut(..n),
            self.track_r.get_mut(..n),
        ) else {
            return;
        };
        out_l.fill(0.0);
        out_r.fill(0.0);
        let end = pos.saturating_add(n as u64);
        for track in &graph.tracks {
            let Some(slot) = self.slots.get_mut(track.slot) else {
                continue;
            };
            mono.fill(0.0);
            left.fill(0.0);
            right.fill(0.0);
            sum_clips(track, pos, end, mono, left, right);
            mix_track(slot, out_l, out_r, mono, left, right);
        }
        for (ol, or) in out_l.iter_mut().zip(out_r.iter_mut()) {
            let gain = self.master_gain.next_value();
            *ol *= gain;
            *or *= gain;
            self.master_meter.push(*ol, *or);
            if ol.abs() > 1.0 || or.abs() > 1.0 {
                self.clipped = true;
            }
        }
    }
}

/// Applies a graph track's targets to its slot. Audible-gated fader gain is the smoothed value.
fn apply_track_params(slot: &mut TrackSlot, track: &GraphTrack) {
    let target_gain = if track.audible { track.gain } else { 0.0 };
    if slot.track_id == Some(track.id) {
        slot.gain.set_target(target_gain);
        slot.pan.set_target(track.pan);
    } else {
        slot.track_id = Some(track.id);
        slot.gain.snap(target_gain);
        slot.pan.snap(track.pan);
        slot.meter = Accum::default();
    }
}

/// Sums every clip of `track` that overlaps `[pos, end)`. Clips are sorted by start, so the
/// scan stops at the first clip that begins at or after `end`.
fn sum_clips(
    track: &GraphTrack,
    pos: u64,
    end: u64,
    mono: &mut [f32],
    left: &mut [f32],
    right: &mut [f32],
) {
    for clip in &track.clips {
        if clip.start >= end {
            break;
        }
        add_clip(clip, pos, end, mono, left, right);
    }
}

/// Adds one clip's overlap with `[pos, end)` into the track scratch. Mono sources go to `mono`
/// (panned later with the mono law), stereo sources to `left`/`right` (balance law).
fn add_clip(
    clip: &GraphClip,
    pos: u64,
    end: u64,
    mono: &mut [f32],
    left: &mut [f32],
    right: &mut [f32],
) {
    let from = clip.start.max(pos);
    let to = clip.start.saturating_add(clip.length).min(end);
    if from >= to {
        return;
    }
    let within = from - clip.start;
    let avail = clip.buffer.frames();
    let Ok(src) = usize::try_from(clip.offset.saturating_add(within)) else {
        return;
    };
    if src >= avail {
        return;
    }
    // Both values are bounded by the block length, so the casts cannot truncate.
    let dst = (from - pos) as usize;
    let count = ((to - from) as usize).min(avail - src);
    let (dst_end, src_end) = (dst + count, src + count);
    let gain_at = |k: usize| {
        clip.gain
            * fade_gain(
                within.saturating_add(k as u64),
                clip.length,
                clip.fade_in,
                clip.fade_out,
            )
    };
    match clip.buffer.channels.as_slice() {
        [] => {}
        [only] => {
            if let (Some(d), Some(s)) = (mono.get_mut(dst..dst_end), only.get(src..src_end)) {
                for (k, (out, &x)) in d.iter_mut().zip(s).enumerate() {
                    *out += x * gain_at(k);
                }
            }
        }
        [l_src, r_src, ..] => {
            if let (Some(dl), Some(dr), Some(sl), Some(sr)) = (
                left.get_mut(dst..dst_end),
                right.get_mut(dst..dst_end),
                l_src.get(src..src_end),
                r_src.get(src..src_end),
            ) {
                let frames = dl.iter_mut().zip(dr.iter_mut()).zip(sl.iter().zip(sr));
                for (k, ((ol, or), (&xl, &xr))) in frames.enumerate() {
                    let g = gain_at(k);
                    *ol += xl * g;
                    *or += xr * g;
                }
            }
        }
    }
}

/// Pans a track's scratch with the smoothed pan, applies the smoothed fader gain, meters the
/// result and adds it into the output. Pan trig is recomputed only while its ramp is running.
fn mix_track(
    slot: &mut TrackSlot,
    out_l: &mut [f32],
    out_r: &mut [f32],
    mono: &[f32],
    left: &[f32],
    right: &[f32],
) {
    let mut pan = PanGains::new(slot.pan.current());
    let frames = out_l
        .iter_mut()
        .zip(out_r.iter_mut())
        .zip(mono.iter().zip(left.iter().zip(right.iter())));
    for ((ol, or), (&m, (&sl, &sr))) in frames {
        if !slot.pan.is_settled() {
            pan = PanGains::new(slot.pan.next_value());
        }
        let gain = slot.gain.next_value();
        let l = (m * pan.mono_l + sl * pan.bal_l) * gain;
        let r = (m * pan.mono_r + sr * pan.bal_r) * gain;
        slot.meter.push(l, r);
        *ol += l;
        *or += r;
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2};
    use std::sync::Arc;

    use super::*;
    use crate::buffer::AudioBuffer;

    const SR: u32 = 48_000;
    const TOL: f32 = 1e-6;

    fn buffer(channels: Vec<Vec<f32>>) -> Arc<AudioBuffer> {
        Arc::new(AudioBuffer::new(channels))
    }

    fn clip(buffer: &Arc<AudioBuffer>, start: u64, offset: u64, length: u64) -> GraphClip {
        GraphClip {
            buffer: Arc::clone(buffer),
            start,
            offset,
            length,
            gain: 1.0,
            fade_in: 0,
            fade_out: 0,
        }
    }

    fn track(id: TrackId, slot: usize, gain: f32, pan: f32, clips: Vec<GraphClip>) -> GraphTrack {
        GraphTrack {
            id,
            slot,
            gain,
            pan,
            audible: true,
            clips,
        }
    }

    fn graph(tracks: Vec<GraphTrack>) -> RenderGraph {
        RenderGraph {
            sample_rate: SR,
            tracks,
            master_gain: 1.0,
            length: 0,
        }
    }

    fn run(
        mixer: &mut Mixer,
        graph: &RenderGraph,
        pos: u64,
        frames: usize,
    ) -> (Vec<f32>, Vec<f32>) {
        let mut l = vec![0.0; frames];
        let mut r = vec![0.0; frames];
        mixer.render(graph, pos, &mut l, &mut r);
        (l, r)
    }

    fn run_chunked(
        mixer: &mut Mixer,
        graph: &RenderGraph,
        start: u64,
        total: usize,
        chunk: usize,
    ) -> (Vec<f32>, Vec<f32>) {
        let mut l = Vec::with_capacity(total);
        let mut r = Vec::with_capacity(total);
        let mut done = 0;
        while done < total {
            let n = chunk.min(total - done);
            let (cl, cr) = run(mixer, graph, start + done as u64, n);
            l.extend(cl);
            r.extend(cr);
            done += n;
        }
        (l, r)
    }

    fn assert_close(actual: f32, expected: f32, tol: f32) {
        assert!(
            (actual - expected).abs() <= tol,
            "expected {expected}, got {actual}"
        );
    }

    /// Deterministic pseudo-random audio in [-1, 1) so chunking tests see non-trivial content.
    fn content(seed: u32, frames: usize, channels: usize) -> Arc<AudioBuffer> {
        let mut state = seed;
        let mut next = move || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / 16_777_216.0 * 2.0 - 1.0
        };
        let chans: Vec<Vec<f32>> = (0..channels)
            .map(|_| (0..frames).map(|_| next()).collect())
            .collect();
        buffer(chans)
    }

    /// Three tracks covering mono and stereo clips, fades, overlaps and pan. `variant_b` changes
    /// gains, pans and audibility so that switching graphs starts ramps.
    fn mixed_graph(
        variant_b: bool,
        mono: &Arc<AudioBuffer>,
        stereo: &Arc<AudioBuffer>,
    ) -> RenderGraph {
        let pick = |a: f32, b: f32| if variant_b { b } else { a };
        let mut t1_a = clip(mono, 100, 0, 6000);
        t1_a.gain = 0.9;
        t1_a.fade_in = 500;
        t1_a.fade_out = 700;
        let mut t1_b = clip(mono, 3000, 1000, 5000);
        t1_b.gain = 0.5;
        let mut t2_a = clip(stereo, 0, 0, 9000);
        t2_a.fade_out = 2000;
        let mut t2_b = clip(stereo, 8000, 500, 4000);
        t2_b.gain = 0.7;
        let t3 = clip(mono, 4500, 0, 5000);
        RenderGraph {
            sample_rate: SR,
            tracks: vec![
                GraphTrack {
                    id: 1,
                    slot: 0,
                    gain: pick(0.8, 0.2),
                    pan: pick(0.3, -0.9),
                    audible: true,
                    clips: vec![t1_a, t1_b],
                },
                GraphTrack {
                    id: 2,
                    slot: 1,
                    gain: pick(1.2, 1.0),
                    pan: pick(-0.6, 0.8),
                    audible: !variant_b,
                    clips: vec![t2_a, t2_b],
                },
                GraphTrack {
                    id: 3,
                    slot: 2,
                    gain: pick(0.5, 0.9),
                    pan: pick(0.0, 0.4),
                    audible: true,
                    clips: vec![t3],
                },
            ],
            master_gain: pick(0.7, 1.1),
            length: 10_000,
        }
    }

    #[test]
    fn empty_graph_renders_silence() {
        let (l, r) = run(&mut Mixer::new(SR), &graph(vec![]), 0, 256);
        assert!(l.iter().chain(&r).all(|&x| x == 0.0));
    }

    #[test]
    fn mono_clip_at_centre_splits_to_minus_three_db_on_both_sides() {
        let b = buffer(vec![vec![1.0; 100]]);
        let g = graph(vec![track(1, 0, 1.0, 0.0, vec![clip(&b, 0, 0, 100)])]);
        let (l, r) = run(&mut Mixer::new(SR), &g, 0, 100);
        for (&lv, &rv) in l.iter().zip(&r) {
            assert_close(lv, FRAC_1_SQRT_2, TOL);
            assert_close(rv, FRAC_1_SQRT_2, TOL);
        }
    }

    #[test]
    fn stereo_clip_passes_through_at_unity_when_centred() {
        let b = buffer(vec![vec![0.25; 50], vec![-0.5; 50]]);
        let g = graph(vec![track(1, 0, 1.0, 0.0, vec![clip(&b, 0, 0, 50)])]);
        let (l, r) = run(&mut Mixer::new(SR), &g, 0, 50);
        assert!(l.iter().all(|&x| x == 0.25));
        assert!(r.iter().all(|&x| x == -0.5));
    }

    #[test]
    fn clip_offset_and_start_select_the_source_window() {
        let samples: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let b = buffer(vec![samples]);
        let g = graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&b, 10, 5, 20)])]);
        let (l, r) = run(&mut Mixer::new(SR), &g, 0, 40);
        for (t, (&lv, &rv)) in l.iter().zip(&r).enumerate() {
            let expected = if (10..30).contains(&t) {
                (t - 5) as f32
            } else {
                0.0
            };
            assert_close(lv, expected, TOL);
            assert_eq!(rv, 0.0);
        }
    }

    #[test]
    fn clip_reading_past_buffer_end_is_silent_there() {
        let b = buffer(vec![vec![1.0; 8]]);
        let g = graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&b, 0, 0, 20)])]);
        let (l, _) = run(&mut Mixer::new(SR), &g, 0, 20);
        assert!(l[..8].iter().all(|&x| x == 1.0));
        assert!(l[8..].iter().all(|&x| x == 0.0));
    }

    #[test]
    fn clip_outside_the_block_renders_silence() {
        let b = buffer(vec![vec![1.0; 10]]);
        let early = graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&b, 0, 0, 10)])]);
        let (l, _) = run(&mut Mixer::new(SR), &early, 100, 20);
        assert!(l.iter().all(|&x| x == 0.0));

        let late = graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&b, 500, 0, 10)])]);
        let (l, _) = run(&mut Mixer::new(SR), &late, 0, 10);
        assert!(l.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn overlapping_clips_on_one_track_sum() {
        let b = buffer(vec![vec![0.5; 10]]);
        let g = graph(vec![track(
            1,
            0,
            1.0,
            -1.0,
            vec![clip(&b, 0, 0, 10), clip(&b, 5, 0, 10)],
        )]);
        let (l, _) = run(&mut Mixer::new(SR), &g, 0, 15);
        assert!(l[..5].iter().all(|&x| x == 0.5));
        assert!(l[5..10].iter().all(|&x| x == 1.0));
        assert!(l[10..].iter().all(|&x| x == 0.5));
    }

    #[test]
    fn clip_gain_and_track_gain_multiply() {
        let b = buffer(vec![vec![1.0; 4]]);
        let mut c = clip(&b, 0, 0, 4);
        c.gain = 0.5;
        let g = graph(vec![track(1, 0, 2.0, -1.0, vec![c])]);
        let (l, _) = run(&mut Mixer::new(SR), &g, 0, 4);
        assert!(l.iter().all(|&x| x == 1.0));
    }

    #[test]
    fn fade_in_and_fade_out_shape_the_clip_edges() {
        let b = buffer(vec![vec![1.0; 100]]);
        let mut c = clip(&b, 0, 0, 100);
        c.fade_in = 10;
        c.fade_out = 10;
        let g = graph(vec![track(1, 0, 1.0, -1.0, vec![c])]);
        let (l, _) = run(&mut Mixer::new(SR), &g, 0, 100);
        assert_eq!(l[0], 0.0);
        assert_close(l[5], (FRAC_PI_2 * 0.5).sin(), TOL);
        assert_close(l[10], 1.0, TOL);
        assert_close(l[90], 1.0, TOL);
        assert_close(l[95], (FRAC_PI_2 * 0.5).sin(), TOL);
        assert_close(l[99], (FRAC_PI_2 * 0.1).sin(), TOL);
    }

    #[test]
    fn inaudible_track_is_silent_on_first_render() {
        let b = buffer(vec![vec![1.0; 10]]);
        let mut silent = track(2, 1, 1.0, -1.0, vec![clip(&b, 0, 0, 10)]);
        silent.audible = false;
        let g = graph(vec![
            track(1, 0, 1.0, -1.0, vec![clip(&b, 0, 0, 10)]),
            silent,
        ]);
        let (l, _) = run(&mut Mixer::new(SR), &g, 0, 10);
        assert!(
            l.iter().all(|&x| x == 1.0),
            "only the audible track should sound"
        );
    }

    #[test]
    fn mute_ramps_to_exactly_zero_after_ramp_frames() {
        let b = buffer(vec![vec![1.0; 1000]]);
        let mut m = Mixer::new(SR);
        run(
            &mut m,
            &graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&b, 0, 0, 1000)])]),
            0,
            10,
        );

        let mut muted = track(1, 0, 1.0, -1.0, vec![clip(&b, 0, 0, 1000)]);
        muted.audible = false;
        let (l, _) = run(&mut m, &graph(vec![muted]), 10, 500);

        assert_close(l[0], 1.0 - 1.0 / 480.0, 1e-5);
        assert!(l[478] > 0.0);
        assert_eq!(l[479], 0.0);
        assert!(l[480..].iter().all(|&x| x == 0.0));
    }

    #[test]
    fn track_gain_change_on_same_id_ramps_instead_of_snapping() {
        let b = buffer(vec![vec![1.0; 1000]]);
        let mut m = Mixer::new(SR);
        run(
            &mut m,
            &graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&b, 0, 0, 1000)])]),
            0,
            10,
        );

        let (l, _) = run(
            &mut m,
            &graph(vec![track(1, 0, 0.5, -1.0, vec![clip(&b, 0, 0, 1000)])]),
            10,
            481,
        );
        assert_close(l[0], 1.0 - 0.5 / 480.0, 1e-5);
        assert_close(l[479], 0.5, TOL);
        assert_close(l[480], 0.5, TOL);
    }

    #[test]
    fn master_gain_scales_on_first_render_and_ramps_afterwards() {
        let b = buffer(vec![vec![1.0; 1000]]);
        let mut g = graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&b, 0, 0, 1000)])]);
        g.master_gain = 0.5;
        let mut m = Mixer::new(SR);
        let (first, _) = run(&mut m, &g, 0, 4);
        assert!(first.iter().all(|&x| x == 0.5));

        g.master_gain = 0.0;
        let (ramp, _) = run(&mut m, &g, 4, 480);
        assert_close(ramp[0], 0.5 * (1.0 - 1.0 / 480.0), 1e-5);
        assert_eq!(ramp[479], 0.0);
    }

    #[test]
    fn output_longer_than_max_block_is_fully_rendered() {
        let frames = MAX_BLOCK * 2 + 37;
        let b = buffer(vec![vec![1.0; frames + 10]]);
        let g = graph(vec![track(
            1,
            0,
            1.0,
            0.0,
            vec![clip(&b, 0, 0, frames as u64)],
        )]);
        let (l, r) = run(&mut Mixer::new(SR), &g, 0, frames);
        assert_eq!(l.len(), frames);
        assert_eq!(r.len(), frames);
        assert!(l.iter().chain(&r).all(|&x| x == FRAC_1_SQRT_2));
    }

    #[test]
    fn chunked_rendering_is_bit_exact_for_a_single_graph() {
        let mono = content(7, 10_000, 1);
        let stereo = content(9, 10_000, 2);
        let g = mixed_graph(false, &mono, &stereo);
        let reference = run(&mut Mixer::new(SR), &g, 0, 10_000);
        assert!(reference.0.iter().any(|&x| x != 0.0));
        for chunk in [1, 37, 512, 4096] {
            let got = run_chunked(&mut Mixer::new(SR), &g, 0, 10_000, chunk);
            assert!(got == reference, "chunk size {chunk} changed the output");
        }
    }

    #[test]
    fn chunked_rendering_is_bit_exact_across_a_graph_switch_mid_ramp() {
        let mono = content(11, 10_000, 1);
        let stereo = content(13, 10_000, 2);
        let a = mixed_graph(false, &mono, &stereo);
        let b = mixed_graph(true, &mono, &stereo);

        let mut whole = Mixer::new(SR);
        let (l1, r1) = run(&mut whole, &a, 0, 5000);
        let (l2, r2) = run(&mut whole, &b, 5000, 5000);
        let reference = ([l1, l2].concat(), [r1, r2].concat());

        for chunk in [1, 37, 512, 4096] {
            let mut m = Mixer::new(SR);
            let (cl1, cr1) = run_chunked(&mut m, &a, 0, 5000, chunk);
            let (cl2, cr2) = run_chunked(&mut m, &b, 5000, 5000, chunk);
            let got = ([cl1, cl2].concat(), [cr1, cr2].concat());
            assert!(
                got == reference,
                "chunk size {chunk} changed the output across the switch"
            );
        }
    }

    #[test]
    fn meters_report_peak_and_rms_of_a_known_signal() {
        let alternating: Vec<f32> = (0..1000)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        let b = buffer(vec![alternating]);
        let g = graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&b, 0, 0, 1000)])]);
        let mut m = Mixer::new(SR);
        run(&mut m, &g, 0, 1000);

        let mut snap = MeterSnapshot::default();
        m.take_meters(&mut snap);
        assert_eq!(snap.track_count, 1);
        assert_close(snap.tracks[0].peak_l, 0.5, TOL);
        assert_close(snap.tracks[0].rms_l, 0.5, TOL);
        assert_eq!(snap.tracks[0].peak_r, 0.0);
        assert_close(snap.master.peak_l, 0.5, TOL);
        assert_close(snap.master.rms_l, 0.5, TOL);
    }

    #[test]
    fn meter_rms_divides_by_all_frames_in_the_window() {
        let ones = buffer(vec![vec![1.0; 1000]]);
        let on = graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&ones, 0, 0, 1000)])]);
        let off = graph(vec![track(1, 0, 1.0, -1.0, vec![])]);
        let mut m = Mixer::new(SR);
        run(&mut m, &on, 0, 1000);
        run(&mut m, &off, 1000, 1000);

        let mut snap = MeterSnapshot::default();
        m.take_meters(&mut snap);
        assert_close(snap.tracks[0].rms_l, (0.5_f32).sqrt(), 1e-5);
        assert_close(snap.tracks[0].peak_l, 1.0, TOL);
    }

    #[test]
    fn take_meters_resets_accumulators_and_clipped_flag() {
        let b = buffer(vec![vec![0.8; 4]]);
        let mut g = graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&b, 0, 0, 4)])]);
        g.master_gain = 2.0;
        let mut m = Mixer::new(SR);
        run(&mut m, &g, 0, 4);

        let mut snap = MeterSnapshot::default();
        m.take_meters(&mut snap);
        assert!(snap.clipped);
        assert_close(snap.master.peak_l, 1.6, 1e-5);

        m.take_meters(&mut snap);
        assert!(!snap.clipped);
        assert_eq!(snap.master.peak_l, 0.0);
        assert_eq!(snap.master.rms_l, 0.0);
        assert_eq!(snap.tracks[0].peak_l, 0.0);
        assert_eq!(snap.tracks[0].rms_l, 0.0);
        assert_eq!(snap.track_count, 1, "track list survives a take");
    }

    #[test]
    fn clipped_flag_is_set_only_above_unity() {
        let at_unity = buffer(vec![vec![0.5; 4]]);
        let mut g = graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&at_unity, 0, 0, 4)])]);
        g.master_gain = 2.0;
        let mut m = Mixer::new(SR);
        let (l, _) = run(&mut m, &g, 0, 4);
        assert_eq!(l[0], 1.0);
        let mut snap = MeterSnapshot::default();
        m.take_meters(&mut snap);
        assert!(!snap.clipped);
    }

    #[test]
    fn slot_id_change_snaps_gain_and_pan_without_ramping() {
        let b = buffer(vec![vec![1.0; 2000]]);
        let mut m = Mixer::new(SR);
        run(
            &mut m,
            &graph(vec![track(1, 0, 1.0, 0.0, vec![clip(&b, 0, 0, 2000)])]),
            0,
            1000,
        );

        let other = graph(vec![track(2, 0, 0.5, -1.0, vec![clip(&b, 0, 0, 2000)])]);
        let (l, r) = run(&mut m, &other, 1000, 4);
        assert_eq!(l[0], 0.5);
        assert_eq!(r[0], 0.0);
    }

    #[test]
    fn same_id_with_new_gain_still_ramps_as_control() {
        let b = buffer(vec![vec![1.0; 2000]]);
        let mut m = Mixer::new(SR);
        run(
            &mut m,
            &graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&b, 0, 0, 2000)])]),
            0,
            1000,
        );

        let same = graph(vec![track(1, 0, 0.5, -1.0, vec![clip(&b, 0, 0, 2000)])]);
        let (l, _) = run(&mut m, &same, 1000, 1);
        assert!(l[0] > 0.5 && l[0] < 1.0, "expected a ramp, got {}", l[0]);
    }

    #[test]
    fn reset_forces_the_next_render_to_snap() {
        let b = buffer(vec![vec![1.0; 2000]]);
        let mut m = Mixer::new(SR);
        run(
            &mut m,
            &graph(vec![track(1, 0, 1.0, -1.0, vec![clip(&b, 0, 0, 2000)])]),
            0,
            1000,
        );
        m.reset();

        let (l, _) = run(
            &mut m,
            &graph(vec![track(1, 0, 0.5, -1.0, vec![clip(&b, 0, 0, 2000)])]),
            1000,
            1,
        );
        assert_eq!(l[0], 0.5);
    }

    #[test]
    fn take_meters_reports_tracks_in_graph_order_with_their_ids() {
        let b = buffer(vec![vec![0.1; 4]]);
        let g = graph(vec![
            track(7, 1, 1.0, 0.0, vec![clip(&b, 0, 0, 4)]),
            track(3, 0, 1.0, 0.0, vec![clip(&b, 0, 0, 4)]),
        ]);
        let mut m = Mixer::new(SR);
        run(&mut m, &g, 0, 4);

        let mut snap = MeterSnapshot::default();
        m.take_meters(&mut snap);
        assert_eq!(snap.track_count, 2);
        assert_eq!(snap.track_ids[0], 7);
        assert_eq!(snap.track_ids[1], 3);
    }

    #[test]
    fn slot_beyond_limit_is_ignored_without_panicking() {
        let b = buffer(vec![vec![1.0; 4]]);
        let g = graph(vec![track(
            9,
            MAX_TRACKS,
            1.0,
            -1.0,
            vec![clip(&b, 0, 0, 4)],
        )]);
        let mut m = Mixer::new(SR);
        let (l, r) = run(&mut m, &g, 0, 4);
        assert!(l.iter().chain(&r).all(|&x| x == 0.0));

        let mut snap = MeterSnapshot::default();
        m.take_meters(&mut snap);
        assert_eq!(snap.track_count, 1);
        assert_eq!(snap.tracks[0].peak_l, 0.0);
    }
}
