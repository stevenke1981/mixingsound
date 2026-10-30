//! Small real-time-safe DSP helpers. No allocation anywhere in this module.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use crate::model::MIN_GAIN_DB;

/// Parameter smoothing time for gain/pan/mute changes.
pub const SMOOTHING_MS: f32 = 10.0;

/// dB → linear. `db <= MIN_GAIN_DB` (and NaN) → 0.0.
pub fn db_to_gain(db: f32) -> f32 {
    if db.is_nan() || db <= MIN_GAIN_DB {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// Linear → dB. `gain <= 0.0` → `f32::NEG_INFINITY`.
pub fn gain_to_db(gain: f32) -> f32 {
    // NaN fails the comparison, so it maps to silence like any non-positive gain.
    if gain > 0.0 {
        20.0 * gain.log10()
    } else {
        f32::NEG_INFINITY
    }
}

/// Pan law for **mono** sources: constant power, -3 dB at centre.
/// Returns (left, right) gains; pan -1 → (1, 0), 0 → (0.7071, 0.7071), 1 → (0, 1).
pub fn pan_mono(pan: f32) -> (f32, f32) {
    let theta = (sanitize_pan(pan) + 1.0) * FRAC_PI_4;
    // The max(0) removes the ~-4e-8 that cos(π/2) yields in f32, so hard right is exactly silent left.
    (theta.cos().max(0.0), theta.sin().max(0.0))
}

/// Pan law for **stereo** sources: balance. Centre = (1, 1); panning right attenuates the left
/// channel with cos(pan·π/2) and leaves right at 1 (mirror for left).
pub fn pan_balance(pan: f32) -> (f32, f32) {
    let pan = sanitize_pan(pan);
    let left = if pan > 0.0 {
        (pan * FRAC_PI_2).cos().max(0.0)
    } else {
        1.0
    };
    let right = if pan < 0.0 {
        (-pan * FRAC_PI_2).cos().max(0.0)
    } else {
        1.0
    };
    (left, right)
}

/// NaN means centre; anything outside -1..=1 is clamped so the trig laws stay in their domain.
fn sanitize_pan(pan: f32) -> f32 {
    if pan.is_nan() {
        0.0
    } else {
        pan.clamp(-1.0, 1.0)
    }
}

/// Clip fade envelope at frame `pos` within a clip of `length` frames (0 ≤ pos < length).
/// Equal-power (sine) curves: fade-in g = sin(π/2 · pos/fade_in) for pos < fade_in,
/// fade-out g = sin(π/2 · (length - pos)/fade_out) for pos ≥ length - fade_out; product if both.
/// Zero-length fades → 1.0.
pub fn fade_gain(pos: u64, length: u64, fade_in: u64, fade_out: u64) -> f32 {
    let mut gain = 1.0;
    if fade_in > 0 && pos < fade_in {
        gain *= (FRAC_PI_2 * (pos as f32 / fade_in as f32)).sin();
    }
    // Remaining frames including the current one would be `length - pos`; the fade-out region is
    // the last `fade_out` frames, so the sine reaches 1.0 exactly at its boundary.
    let remaining = length.saturating_sub(pos);
    if fade_out > 0 && remaining <= fade_out {
        gain *= (FRAC_PI_2 * (remaining as f32 / fade_out as f32)).sin();
    }
    gain
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
        let frames = (f64::from(SMOOTHING_MS) * f64::from(sample_rate) / 1000.0).round();
        Self {
            current: value,
            target: value,
            step: 0.0,
            remaining: 0,
            ramp_frames: frames.max(1.0) as u32,
        }
    }

    /// Starts a ramp from the current value to `target` (no-op if already targeting it).
    /// NaN is ignored so a bad parameter can never poison the audio output.
    pub fn set_target(&mut self, target: f32) {
        if target.is_nan() || target == self.target {
            return;
        }
        self.target = target;
        self.remaining = self.ramp_frames;
        self.step = (target - self.current) / self.ramp_frames as f32;
    }

    /// Jumps immediately to `value`.
    pub fn snap(&mut self, value: f32) {
        if value.is_nan() {
            return;
        }
        self.current = value;
        self.target = value;
        self.step = 0.0;
        self.remaining = 0;
    }

    /// Advances one frame and returns the new value.
    pub fn next_value(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            // Land exactly on the target at the end of the ramp instead of accumulating step error.
            self.current = if self.remaining == 0 {
                self.target
            } else {
                self.current + self.step
            };
        }
        self.current
    }

    pub fn is_settled(&self) -> bool {
        self.remaining == 0
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    pub fn current(&self) -> f32 {
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f32 = 1e-6;

    fn assert_close(actual: f32, expected: f32, tol: f32) {
        assert!(
            (actual - expected).abs() <= tol,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn db_to_gain_maps_zero_db_to_unity() {
        assert_close(db_to_gain(0.0), 1.0, TOL);
    }

    #[test]
    fn db_to_gain_maps_minus_six_db_to_half_amplitude() {
        assert_close(db_to_gain(-6.0206), 0.5, 1e-4);
    }

    #[test]
    fn db_to_gain_treats_floor_nan_and_below_as_silence() {
        assert_eq!(db_to_gain(MIN_GAIN_DB), 0.0);
        assert_eq!(db_to_gain(-200.0), 0.0);
        assert_eq!(db_to_gain(f32::NAN), 0.0);
        assert_eq!(db_to_gain(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn db_to_gain_just_above_floor_is_audible() {
        assert!(db_to_gain(MIN_GAIN_DB + 0.1) > 0.0);
    }

    #[test]
    fn gain_to_db_maps_unity_to_zero_db() {
        assert_close(gain_to_db(1.0), 0.0, TOL);
    }

    #[test]
    fn gain_to_db_maps_non_positive_and_nan_to_negative_infinity() {
        assert_eq!(gain_to_db(0.0), f32::NEG_INFINITY);
        assert_eq!(gain_to_db(-1.0), f32::NEG_INFINITY);
        assert_eq!(gain_to_db(f32::NAN), f32::NEG_INFINITY);
    }

    #[test]
    fn db_round_trip_is_stable_across_fader_range() {
        for db in [-60.0_f32, -12.0, -6.0, -1.0, 0.0, 3.0, 6.0, 12.0] {
            assert_close(gain_to_db(db_to_gain(db)), db, 1e-4);
        }
    }

    #[test]
    fn gain_round_trip_is_stable_for_positive_gains() {
        for gain in [0.001_f32, 0.25, 1.0, 1.5, 4.0] {
            let back = db_to_gain(gain_to_db(gain));
            assert!(
                (back - gain).abs() <= gain * 1e-5,
                "gain {gain} came back as {back}"
            );
        }
    }

    #[test]
    fn pan_mono_hard_left_is_full_left_only() {
        assert_eq!(pan_mono(-1.0), (1.0, 0.0));
    }

    #[test]
    fn pan_mono_centre_is_minus_three_db_each_side() {
        let (l, r) = pan_mono(0.0);
        assert_close(l, std::f32::consts::FRAC_1_SQRT_2, TOL);
        assert_close(r, std::f32::consts::FRAC_1_SQRT_2, TOL);
    }

    #[test]
    fn pan_mono_hard_right_is_full_right_only() {
        assert_eq!(pan_mono(1.0), (0.0, 1.0));
    }

    #[test]
    fn pan_mono_is_constant_power_across_the_range() {
        for step in -10..=10 {
            let (l, r) = pan_mono(step as f32 / 10.0);
            assert_close(l * l + r * r, 1.0, 1e-6);
        }
    }

    #[test]
    fn pan_mono_clamps_out_of_range_and_treats_nan_as_centre() {
        assert_eq!(pan_mono(-5.0), (1.0, 0.0));
        assert_eq!(pan_mono(5.0), (0.0, 1.0));
        assert_eq!(pan_mono(f32::NAN), pan_mono(0.0));
    }

    #[test]
    fn pan_balance_centre_leaves_both_channels_at_unity() {
        assert_eq!(pan_balance(0.0), (1.0, 1.0));
    }

    #[test]
    fn pan_balance_hard_right_silences_left_only() {
        assert_eq!(pan_balance(1.0), (0.0, 1.0));
    }

    #[test]
    fn pan_balance_hard_left_silences_right_only() {
        assert_eq!(pan_balance(-1.0), (1.0, 0.0));
    }

    #[test]
    fn pan_balance_half_right_attenuates_left_with_cosine() {
        let (l, r) = pan_balance(0.5);
        assert_close(l, std::f32::consts::FRAC_1_SQRT_2, TOL);
        assert_eq!(r, 1.0);
    }

    #[test]
    fn pan_balance_half_left_attenuates_right_with_cosine() {
        let (l, r) = pan_balance(-0.5);
        assert_eq!(l, 1.0);
        assert_close(r, std::f32::consts::FRAC_1_SQRT_2, TOL);
    }

    #[test]
    fn pan_balance_left_gain_decreases_monotonically_when_panning_right() {
        let gains: Vec<f32> = (0..=10).map(|i| pan_balance(i as f32 / 10.0).0).collect();
        assert!(gains.windows(2).all(|w| w[1] <= w[0]));
    }

    #[test]
    fn fade_gain_without_fades_is_unity_everywhere() {
        assert_eq!(fade_gain(0, 100, 0, 0), 1.0);
        assert_eq!(fade_gain(50, 100, 0, 0), 1.0);
        assert_eq!(fade_gain(99, 100, 0, 0), 1.0);
    }

    #[test]
    fn fade_gain_zero_length_fades_are_unity() {
        assert_eq!(fade_gain(0, 10, 0, 0), 1.0);
        assert_eq!(fade_gain(9, 10, 0, 0), 1.0);
    }

    #[test]
    fn fade_in_starts_silent_reaches_half_sine_at_midpoint_and_unity_at_end() {
        assert_eq!(fade_gain(0, 100, 10, 0), 0.0);
        assert_close(fade_gain(5, 100, 10, 0), (FRAC_PI_2 * 0.5).sin(), TOL);
        assert_close(fade_gain(10, 100, 10, 0), 1.0, TOL);
        assert_eq!(fade_gain(50, 100, 10, 0), 1.0);
    }

    #[test]
    fn fade_out_is_unity_before_region_and_silent_at_last_frame_limit() {
        assert_eq!(fade_gain(89, 100, 0, 10), 1.0);
        assert_close(fade_gain(90, 100, 0, 10), 1.0, TOL);
        assert_close(fade_gain(95, 100, 0, 10), (FRAC_PI_2 * 0.5).sin(), TOL);
        assert_close(fade_gain(99, 100, 0, 10), (FRAC_PI_2 * 0.1).sin(), TOL);
    }

    #[test]
    fn both_fades_meeting_mid_clip_each_reach_unity_at_their_boundary() {
        assert_close(fade_gain(4, 10, 5, 5), (FRAC_PI_2 * 0.8).sin(), TOL);
        assert_close(fade_gain(5, 10, 5, 5), 1.0, TOL);
    }

    #[test]
    fn fade_gain_stays_within_unit_range() {
        for pos in 0..100 {
            let g = fade_gain(pos, 100, 30, 40);
            assert!((0.0..=1.0).contains(&g), "pos {pos} gave {g}");
        }
    }

    #[test]
    fn smoother_ramp_length_follows_sample_rate() {
        assert_eq!(Smoother::new(0.0, 48_000).ramp_frames, 480);
        assert_eq!(Smoother::new(0.0, 44_100).ramp_frames, 441);
    }

    #[test]
    fn smoother_ramp_length_is_at_least_one_frame() {
        assert_eq!(Smoother::new(0.0, 0).ramp_frames, 1);
        assert_eq!(Smoother::new(0.0, 1).ramp_frames, 1);
    }

    #[test]
    fn smoother_reaches_target_in_exactly_ramp_frames() {
        let mut s = Smoother::new(0.0, 48_000);
        s.set_target(1.0);
        for _ in 0..479 {
            s.next_value();
        }
        assert!(!s.is_settled());
        assert_eq!(s.next_value(), 1.0);
        assert!(s.is_settled());
        assert_eq!(s.next_value(), 1.0);
    }

    #[test]
    fn smoother_ramp_is_linear() {
        let mut s = Smoother::new(0.0, 48_000);
        s.set_target(0.48);
        for k in 1..=480 {
            assert_close(s.next_value(), k as f32 * 0.001, 1e-4);
        }
    }

    #[test]
    fn smoother_set_same_target_does_not_restart_ramp() {
        let mut s = Smoother::new(0.0, 48_000);
        s.set_target(1.0);
        for _ in 0..100 {
            s.next_value();
        }
        s.set_target(1.0);
        for _ in 0..379 {
            s.next_value();
        }
        assert!(
            !s.is_settled(),
            "re-targeting the same value must keep the original 480-frame ramp"
        );
        assert_eq!(s.next_value(), 1.0);
        assert!(s.is_settled());
    }

    #[test]
    fn smoother_retarget_mid_ramp_starts_from_current_value() {
        let mut s = Smoother::new(0.0, 48_000);
        s.set_target(1.0);
        for _ in 0..240 {
            s.next_value();
        }
        let mid = s.current();
        assert_close(mid, 0.5, 1e-4);
        s.set_target(0.0);
        let first = s.next_value();
        assert_close(first, mid - mid / 480.0, 1e-4);
        for _ in 1..480 {
            s.next_value();
        }
        assert!(s.is_settled());
        assert_eq!(s.current(), 0.0);
    }

    #[test]
    fn smoother_snap_jumps_and_settles_immediately() {
        let mut s = Smoother::new(0.0, 48_000);
        s.snap(0.3);
        assert!(s.is_settled());
        assert_eq!(s.current(), 0.3);
        assert_eq!(s.next_value(), 0.3);
        assert_eq!(s.target(), 0.3);
    }

    #[test]
    fn smoother_snap_cancels_a_running_ramp() {
        let mut s = Smoother::new(0.0, 48_000);
        s.set_target(1.0);
        s.next_value();
        s.snap(0.2);
        assert!(s.is_settled());
        assert_eq!(s.next_value(), 0.2);
    }

    #[test]
    fn smoother_ignores_nan_targets_and_values() {
        let mut s = Smoother::new(0.5, 48_000);
        s.set_target(f32::NAN);
        s.snap(f32::NAN);
        assert!(s.is_settled());
        assert_eq!(s.current(), 0.5);
        assert_eq!(s.target(), 0.5);
    }
}
