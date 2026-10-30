//! Offline render ("bounce") of a `RenderGraph` to a WAV file, as fast as possible, using the
//! same `render::Mixer` as real-time playback.

use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

use hound::{SampleFormat, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};

use crate::error::{EngineError, Result};
use crate::graph::RenderGraph;
use crate::render::{MAX_BLOCK, Mixer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BitDepth {
    Int16,
    Int24,
    Float32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BounceOptions {
    /// Timeline range `start..end` (frames). Must satisfy start < end.
    pub start: u64,
    pub end: u64,
    pub bit_depth: BitDepth,
    /// TPDF dither when writing integer formats. Ignored for Float32.
    pub dither: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BounceReport {
    pub path: String,
    pub frames: u64,
    /// Highest absolute sample in dBFS (before clamping). Silence reports -144.0 rather than
    /// -inf, because JSON cannot encode infinity.
    pub peak_db: f32,
    /// Samples that exceeded ±1.0 and were clamped (integer formats only).
    pub clipped_samples: u64,
}

/// Peak reported for silence, and the lowest value `peak_to_db` returns.
const PEAK_DB_FLOOR: f32 = -144.0;
const I16_SCALE: i32 = 32_767;
const I24_SCALE: i32 = 8_388_607;
/// Fixed seed so repeated bounces of the same graph are bit-identical.
const DITHER_SEED: u64 = 0x2545_F491_4F6C_DD1D;

/// Renders stereo at `graph.sample_rate`. Writes to a temporary file next to `path` and
/// renames on success, so a failed/cancelled bounce never leaves a partial file at `path`.
/// `progress(fraction 0..=1)` is called periodically; returning `false` cancels
/// (→ `EngineError::Cancelled`).
pub fn bounce_to_wav(
    graph: &RenderGraph,
    options: &BounceOptions,
    path: &Path,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<BounceReport> {
    if options.start >= options.end {
        return Err(EngineError::InvalidEdit(format!(
            "bounce range {}..{} is empty",
            options.start, options.end
        )));
    }
    let temp = part_path(path);
    let stats = match render_to_part(graph, options, &temp, progress) {
        Ok(stats) => stats,
        Err(err) => {
            remove_quietly(&temp);
            return Err(err);
        }
    };
    if let Err(source) = std::fs::rename(&temp, path) {
        remove_quietly(&temp);
        return Err(EngineError::io(path, source));
    }
    Ok(BounceReport {
        path: path.to_string_lossy().into_owned(),
        frames: options.end - options.start,
        peak_db: peak_to_db(stats.peak),
        clipped_samples: stats.clipped,
    })
}

/// Sibling temporary name: `<path>.part`, in the same directory so the final rename stays on
/// one volume.
fn part_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

/// Best-effort cleanup after a failed bounce. The original error is what the caller needs,
/// so a failed removal is not reported.
fn remove_quietly(path: &Path) {
    let _ = std::fs::remove_file(path);
}

struct BounceStats {
    peak: f32,
    clipped: u64,
}

fn render_to_part(
    graph: &RenderGraph,
    options: &BounceOptions,
    temp: &Path,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<BounceStats> {
    // The writer is created before the mixer so a bad target path fails without rendering.
    let spec = wav_spec(graph.sample_rate, options.bit_depth);
    let mut writer = WavWriter::create(temp, spec).map_err(|e| map_wav_error(temp, e))?;
    let mut mixer = Mixer::new(graph.sample_rate);
    mixer.reset();
    let stats = render_blocks(graph, options, &mut mixer, &mut writer, temp, progress)?;
    writer.finalize().map_err(|e| map_wav_error(temp, e))?;
    Ok(stats)
}

fn wav_spec(sample_rate: u32, bit_depth: BitDepth) -> WavSpec {
    let (bits_per_sample, sample_format) = match bit_depth {
        BitDepth::Int16 => (16, SampleFormat::Int),
        BitDepth::Int24 => (24, SampleFormat::Int),
        BitDepth::Float32 => (32, SampleFormat::Float),
    };
    WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample,
        sample_format,
    }
}

fn render_blocks<W: Write + Seek>(
    graph: &RenderGraph,
    options: &BounceOptions,
    mixer: &mut Mixer,
    writer: &mut WavWriter<W>,
    temp: &Path,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<BounceStats> {
    let total = options.end - options.start;
    let mut left = vec![0.0f32; MAX_BLOCK];
    let mut right = vec![0.0f32; MAX_BLOCK];
    let mut encoder = Encoder::new(options);
    let mut pos = options.start;
    while pos < options.end {
        let frames = usize::try_from(options.end - pos)
            .unwrap_or(MAX_BLOCK)
            .min(MAX_BLOCK);
        let (l, r) = (&mut left[..frames], &mut right[..frames]);
        mixer.render(graph, pos, l, r);
        for (&left_sample, &right_sample) in l.iter().zip(r.iter()) {
            encoder
                .write(writer, left_sample)
                .map_err(|e| map_wav_error(temp, e))?;
            encoder
                .write(writer, right_sample)
                .map_err(|e| map_wav_error(temp, e))?;
        }
        pos += frames as u64;
        let fraction = (pos - options.start) as f32 / total as f32;
        if !progress(fraction) {
            return Err(EngineError::Cancelled);
        }
    }
    Ok(encoder.into_stats())
}

fn map_wav_error(path: &Path, error: hound::Error) -> EngineError {
    match error {
        hound::Error::IoError(source) => EngineError::io(path, source),
        other => EngineError::Wav(other.to_string()),
    }
}

/// Converts rendered float samples to the target WAV format, tracking peak and clipping.
struct Encoder {
    bit_depth: BitDepth,
    dither: bool,
    rng: Xorshift64,
    peak: f32,
    clipped: u64,
}

impl Encoder {
    fn new(options: &BounceOptions) -> Self {
        Self {
            bit_depth: options.bit_depth,
            dither: options.dither && options.bit_depth != BitDepth::Float32,
            rng: Xorshift64::new(DITHER_SEED),
            peak: 0.0,
            clipped: 0,
        }
    }

    fn write<W: Write + Seek>(
        &mut self,
        writer: &mut WavWriter<W>,
        sample: f32,
    ) -> hound::Result<()> {
        // A finite graph never yields non-finite samples; zeroing them keeps the file valid.
        let sample = if sample.is_finite() { sample } else { 0.0 };
        // Peak is measured before clamping so the report shows how hot the mix really is.
        self.peak = self.peak.max(sample.abs());
        let scale = match self.bit_depth {
            BitDepth::Float32 => return writer.write_sample(sample),
            BitDepth::Int16 => I16_SCALE,
            BitDepth::Int24 => I24_SCALE,
        };
        if sample.abs() > 1.0 {
            self.clipped += 1;
        }
        let dither = if self.dither {
            tpdf(&mut self.rng)
        } else {
            0.0
        };
        let quantized = quantize(sample, scale, dither);
        if self.bit_depth == BitDepth::Int16 {
            writer.write_sample(quantized as i16)
        } else {
            writer.write_sample(quantized)
        }
    }

    fn into_stats(self) -> BounceStats {
        BounceStats {
            peak: self.peak,
            clipped: self.clipped,
        }
    }
}

/// Rounds `sample` (clamped to ±1.0) onto the integer grid of `scale`, after adding
/// `dither_lsb`. The result is clamped to the two's-complement range of the format.
fn quantize(sample: f32, scale: i32, dither_lsb: f32) -> i32 {
    let scaled = sample.clamp(-1.0, 1.0) * scale as f32 + dither_lsb;
    (scaled.round() as i32).clamp(-scale - 1, scale)
}

fn peak_to_db(peak: f32) -> f32 {
    if peak > 0.0 {
        (20.0 * peak.log10()).max(PEAK_DB_FLOOR)
    } else {
        PEAK_DB_FLOOR
    }
}

/// Tiny deterministic PRNG for dither. No external dependency is needed for this.
struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    /// A zero state would stay zero forever, so zero seeds are mapped to one.
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Uniform in [0, 1) with 24 bits of precision, which is exact in f32.
    fn next_unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }
}

/// TPDF dither in LSB. The sum of two independent uniforms in [-0.5, 0.5) lies in [-1, 1).
fn tpdf(rng: &mut Xorshift64) -> f32 {
    (rng.next_unit() - 0.5) + (rng.next_unit() - 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::AudioBuffer;
    use crate::graph::{GraphClip, GraphTrack};

    fn options(start: u64, end: u64, bit_depth: BitDepth) -> BounceOptions {
        BounceOptions {
            start,
            end,
            bit_depth,
            dither: false,
        }
    }

    /// One mono track of constant `level`, hard left, so the left output equals `level * gain`.
    fn constant_graph(level: f32, gain: f32, frames: usize) -> RenderGraph {
        let buffer = std::sync::Arc::new(AudioBuffer::new(vec![vec![level; frames]]));
        let clip = GraphClip {
            buffer,
            start: 0,
            offset: 0,
            length: frames as u64,
            gain,
            fade_in: 0,
            fade_out: 0,
        };
        let track = GraphTrack {
            id: 1,
            slot: 0,
            gain: 1.0,
            pan: -1.0,
            audible: true,
            clips: vec![clip],
        };
        RenderGraph {
            sample_rate: 48_000,
            tracks: vec![track],
            master_gain: 1.0,
            length: frames as u64,
        }
    }

    #[test]
    fn rejects_empty_or_inverted_range_without_touching_disk() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.wav");
        let graph = RenderGraph::empty(48_000);

        for (start, end) in [(10, 10), (20, 10)] {
            let result = bounce_to_wav(
                &graph,
                &options(start, end, BitDepth::Int16),
                &target,
                &mut |_: f32| true,
            );
            assert!(matches!(result, Err(EngineError::InvalidEdit(_))));
        }
        assert!(!target.exists());
        assert!(!part_path(&target).exists());
    }

    #[test]
    fn missing_directory_reports_io_error_and_leaves_no_part_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("missing").join("out.wav");
        let graph = RenderGraph::empty(48_000);

        let result = bounce_to_wav(
            &graph,
            &options(0, 100, BitDepth::Float32),
            &target,
            &mut |_: f32| true,
        );

        assert!(matches!(result, Err(EngineError::Io { .. })));
        assert!(!part_path(&target).exists());
        assert!(!target.exists());
    }

    #[test]
    fn part_path_appends_suffix_next_to_target() {
        assert_eq!(
            part_path(Path::new("dir/mix.wav")),
            PathBuf::from("dir/mix.wav.part")
        );
    }

    #[test]
    fn quantize_maps_zero_and_full_scale_to_grid_ends() {
        assert_eq!(quantize(0.0, I16_SCALE, 0.0), 0);
        assert_eq!(quantize(1.0, I16_SCALE, 0.0), 32_767);
        assert_eq!(quantize(-1.0, I16_SCALE, 0.0), -32_767);
        assert_eq!(quantize(1.0, I24_SCALE, 0.0), 8_388_607);
    }

    #[test]
    fn quantize_rounds_half_away_from_zero() {
        assert_eq!(quantize(0.25, I16_SCALE, 0.0), 8_192);
        assert_eq!(quantize(-0.25, I16_SCALE, 0.0), -8_192);
    }

    #[test]
    fn quantize_clamps_out_of_range_samples_and_dither_can_reach_minimum() {
        assert_eq!(quantize(3.0, I16_SCALE, 0.0), 32_767);
        assert_eq!(quantize(-3.0, I16_SCALE, 0.0), -32_767);
        assert_eq!(quantize(-1.0, I16_SCALE, -1.0), -32_768);
        assert_eq!(quantize(1.0, I16_SCALE, 1.0), 32_767);
    }

    #[test]
    fn dither_moves_quantized_value_by_at_most_one_lsb() {
        for step in -20..=20 {
            let sample = step as f32 / 20.0;
            for dither_step in -10..=10 {
                let dither = dither_step as f32 / 10.0;
                let plain = quantize(sample, I16_SCALE, 0.0);
                let dithered = quantize(sample, I16_SCALE, dither);
                assert!(
                    (dithered - plain).abs() <= 1,
                    "sample {sample} dither {dither}"
                );
            }
        }
    }

    #[test]
    fn tpdf_stays_within_one_lsb_and_is_deterministic() {
        let mut a = Xorshift64::new(DITHER_SEED);
        let mut b = Xorshift64::new(DITHER_SEED);

        for _ in 0..10_000 {
            let value = tpdf(&mut a);
            assert!(value.abs() <= 1.0, "dither {value} exceeds 1 LSB");
            assert_eq!(value, tpdf(&mut b));
        }
    }

    #[test]
    fn tpdf_has_near_zero_mean() {
        let mut rng = Xorshift64::new(DITHER_SEED);
        let count = 100_000;
        let mean: f32 = (0..count).map(|_| tpdf(&mut rng)).sum::<f32>() / count as f32;
        assert!(mean.abs() < 0.01, "mean {mean}");
    }

    #[test]
    fn zero_seed_does_not_stick_at_zero() {
        let mut rng = Xorshift64::new(0);
        assert_ne!(rng.next_u64(), 0);
    }

    #[test]
    fn peak_db_reports_floor_for_silence_and_zero_for_full_scale() {
        assert_eq!(peak_to_db(0.0), PEAK_DB_FLOOR);
        assert_eq!(peak_to_db(1e-30), PEAK_DB_FLOOR);
        assert!(peak_to_db(1.0).abs() < 1e-6);
        assert!((peak_to_db(0.5) - (-6.0206)).abs() < 1e-3);
    }

    #[test]
    fn bounce_writes_wav_with_requested_length_and_format() {
        let cases = [
            (BitDepth::Int16, 16, SampleFormat::Int),
            (BitDepth::Int24, 24, SampleFormat::Int),
            (BitDepth::Float32, 32, SampleFormat::Float),
        ];
        for (bit_depth, bits, format) in cases {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("out.wav");
            let graph = RenderGraph::empty(44_100);

            let report = bounce_to_wav(
                &graph,
                &options(0, 10_000, bit_depth),
                &target,
                &mut |_: f32| true,
            )
            .unwrap();

            assert_eq!(report.frames, 10_000);
            assert_eq!(report.clipped_samples, 0);
            assert_eq!(report.peak_db, PEAK_DB_FLOOR);
            let reader = hound::WavReader::open(&target).unwrap();
            let spec = reader.spec();
            assert_eq!(
                (
                    spec.channels,
                    spec.sample_rate,
                    spec.bits_per_sample,
                    spec.sample_format
                ),
                (2, 44_100, bits, format)
            );
            assert_eq!(reader.duration(), 10_000);
            assert!(!part_path(&target).exists());
        }
    }

    #[test]
    fn bounce_reports_peak_before_clamping_and_counts_clipped_samples() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("hot.wav");
        let graph = constant_graph(1.0, 2.0, 1_000);

        let report = bounce_to_wav(
            &graph,
            &options(0, 1_000, BitDepth::Int16),
            &target,
            &mut |_: f32| true,
        )
        .unwrap();

        assert!(
            (report.peak_db - 6.0206).abs() < 0.01,
            "peak {}",
            report.peak_db
        );
        assert!(report.clipped_samples > 0);
    }

    #[test]
    fn float_bounce_keeps_overs_and_counts_no_clipping() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("hot.wav");
        let graph = constant_graph(1.0, 2.0, 1_000);

        let report = bounce_to_wav(
            &graph,
            &options(0, 1_000, BitDepth::Float32),
            &target,
            &mut |_: f32| true,
        )
        .unwrap();

        assert_eq!(report.clipped_samples, 0);
        let max = hound::WavReader::open(&target)
            .unwrap()
            .into_samples::<f32>()
            .map(|s| s.unwrap().abs())
            .fold(0.0f32, f32::max);
        assert!(
            max > 1.5,
            "float output should keep values above 1.0, got {max}"
        );
    }

    #[test]
    fn progress_fraction_increases_and_ends_at_one() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.wav");
        let graph = RenderGraph::empty(48_000);
        let mut seen = Vec::new();

        bounce_to_wav(
            &graph,
            &options(0, 10_000, BitDepth::Float32),
            &target,
            &mut |fraction: f32| {
                seen.push(fraction);
                true
            },
        )
        .unwrap();

        assert!(
            seen.len() >= 2,
            "expected several progress calls, got {seen:?}"
        );
        assert!(seen.windows(2).all(|pair| pair[0] < pair[1]));
        assert!((seen[seen.len() - 1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cancelled_bounce_removes_part_file_and_leaves_no_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.wav");
        let graph = RenderGraph::empty(48_000);

        let result = bounce_to_wav(
            &graph,
            &options(0, 10_000, BitDepth::Int24),
            &target,
            &mut |_: f32| false,
        );

        assert!(matches!(result, Err(EngineError::Cancelled)));
        assert!(!part_path(&target).exists());
        assert!(!target.exists());
    }

    #[test]
    fn bounce_replaces_existing_target_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.wav");
        std::fs::write(&target, b"not a wav").unwrap();
        let graph = RenderGraph::empty(48_000);

        bounce_to_wav(
            &graph,
            &options(0, 500, BitDepth::Int16),
            &target,
            &mut |_: f32| true,
        )
        .unwrap();

        let reader = hound::WavReader::open(&target).unwrap();
        assert_eq!(reader.duration(), 500);
        assert!(!part_path(&target).exists());
    }
}
