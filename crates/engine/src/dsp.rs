//! Small real-time-safe DSP helpers. No allocation anywhere in this module.

use crate::model::MIN_GAIN_DB;

/// Parameter smoothing time for gain/pan/mute changes.
pub const SMOOTHING_MS: f32 = 10.0;

/// dB → linear. `db <= MIN_GAIN_DB` (and NaN) → 0.0.
pub fn db_to_gain(db: f32) -> f32 {
    if db.is_nan() || db <= MIN_GAIN_DB { 0.0 } else { 10f32.powf(db / 20.0) }
}

/// Linear → dB. `gain <= 0.0` → `f32::NEG_INFINITY`.
pub fn gain_to_db(gain: f32) -> f32 {
    let _ = gain;
    todo!("mixer worker")
}

/// Pan law for **mono** sources: constant power, -3 dB at centre.
/// Returns (left, right) gains; pan -1 → (1, 0), 0 → (0.7071, 0.7071), 1 → (0, 1).
pub fn pan_mono(pan: f32) -> (f32, f32) {
    let _ = pan;
    todo!("mixer worker")
}

/// Pan law for **stereo** sources: balance. Centre = (1, 1); panning right attenuates the left
/// channel with cos(pan·π/2) and leaves right at 1 (mirror for left).
pub fn pan_balance(pan: f32) -> (f32, f32) {
    let _ = pan;
    todo!("mixer worker")
}

/// Clip fade envelope at frame `pos` within a clip of `length` frames (0 ≤ pos < length).
/// Equal-power (sine) curves: fade-in g = sin(π/2 · pos/fade_in) for pos < fade_in,
/// fade-out g = sin(π/2 · (length - pos)/fade_out) for pos ≥ length - fade_out; product if both.
/// Zero-length fades → 1.0.
pub fn fade_gain(pos: u64, length: u64, fade_in: u64, fade_out: u64) -> f32 {
    let _ = (pos, length, fade_in, fade_out);
    todo!("mixer worker")
}

/// Linear-ramp parameter smoother. Ramp length fixed at construction.
#[derive(Debug, Clone, Copy)]
pub struct Smoother {
    current: f32,
    target: f32,
    step: f32,
    remaining: u32,
    ramp_frames: u32,
}

impl Smoother {
    /// `ramp_frames` = round(SMOOTHING_MS · sample_rate / 1000), at least 1.
    pub fn new(value: f32, sample_rate: u32) -> Self {
        let _ = (value, sample_rate);
        todo!("mixer worker")
    }

    /// Starts a ramp from the current value to `target` (no-op if already targeting it).
    pub fn set_target(&mut self, target: f32) {
        let _ = target;
        todo!("mixer worker")
    }

    /// Jumps immediately to `value`.
    pub fn snap(&mut self, value: f32) {
        let _ = value;
        todo!("mixer worker")
    }

    /// Advances one frame and returns the new value.
    pub fn next_value(&mut self) -> f32 {
        todo!("mixer worker")
    }

    pub fn is_settled(&self) -> bool {
        todo!("mixer worker")
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    pub fn current(&self) -> f32 {
        self.current
    }
}
