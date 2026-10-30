//! Multi-resolution waveform overview, computed once at import and queried by the UI.

use crate::buffer::AudioBuffer;

/// Frames per peak for each precomputed level, finest first.
pub const PEAK_LEVELS: [u32; 4] = [256, 1024, 4096, 16384];

#[derive(Debug, Clone, PartialEq)]
pub struct PeakLevel {
    pub frames_per_peak: u32,
    /// Per bucket: min / max over all channels and frames in the bucket.
    pub min: Vec<f32>,
    pub max: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Peaks {
    pub frames: u64,
    /// One entry per `PEAK_LEVELS` value, same order.
    pub levels: Vec<PeakLevel>,
}

/// Builds every level (coarser levels may be derived from finer ones).
pub fn build_peaks(buffer: &AudioBuffer) -> Peaks {
    let frames = buffer.frames();
    let mut levels: Vec<PeakLevel> = Vec::with_capacity(PEAK_LEVELS.len());
    for &frames_per_peak in &PEAK_LEVELS {
        let level = match levels.last() {
            // Coarser levels reduce the finer level's buckets: one pass over the audio total.
            Some(finer) if frames_per_peak % finer.frames_per_peak == 0 => {
                derive_level(finer, frames_per_peak, frames)
            }
            _ => scan_level(buffer, frames_per_peak),
        };
        levels.push(level);
    }
    Peaks {
        frames: frames as u64,
        levels,
    }
}

impl Peaks {
    /// Waveform for source frames `start..end` split into `columns` equal columns.
    /// Returns `[min, max]` per column (exactly `columns` entries; columns past the end of the
    /// audio are `[0, 0]`). Uses the coarsest level whose `frames_per_peak` ≤ frames per
    /// column; when a column spans fewer frames than the finest level, reads `buffer` directly.
    /// Each column covers at least one frame, so a zoom past one frame per column still shows
    /// every source sample. `columns == 0` returns an empty Vec; `end <= start` returns
    /// `columns` zero pairs.
    pub fn query(
        &self,
        buffer: &AudioBuffer,
        start: u64,
        end: u64,
        columns: usize,
    ) -> Vec<[f32; 2]> {
        if columns == 0 {
            return Vec::new();
        }
        if end <= start {
            return vec![[0.0; 2]; columns];
        }
        let audio_len = self.frames.min(buffer.frames() as u64);
        let span = end - start;
        let frames_per_column = span as f64 / columns as f64;
        let level = self
            .levels
            .iter()
            .rev()
            .find(|level| f64::from(level.frames_per_peak) <= frames_per_column);

        (0..columns)
            .map(|index| {
                let a = column_edge(start, span, columns, index);
                let b = column_edge(start, span, columns, index + 1).max(a + 1);
                if a >= audio_len {
                    return [0.0; 2];
                }
                let b = b.min(audio_len);
                match level {
                    Some(level) => query_level(level, a, b),
                    None => scan_range(buffer, a, b),
                }
            })
            .collect()
    }
}

/// Number of buckets of `frames_per_peak` frames needed to cover `frames` (last one partial).
fn bucket_count(frames: usize, frames_per_peak: u32) -> usize {
    frames.div_ceil(frames_per_peak.max(1) as usize)
}

/// Start of column `index` (`index == columns` gives `start + span` exactly). Computed in f64
/// from the full span so that rounding does not accumulate across columns.
fn column_edge(start: u64, span: u64, columns: usize, index: usize) -> u64 {
    let offset = span as f64 * index as f64 / columns as f64;
    start + offset.floor() as u64
}

fn min_max(samples: &[f32]) -> [f32; 2] {
    samples
        .iter()
        .fold([f32::INFINITY, f32::NEG_INFINITY], |[lo, hi], &s| {
            [lo.min(s), hi.max(s)]
        })
}

/// Combines per-bucket min/max arrays. An empty range gives `[0, 0]`.
fn reduce(mins: &[f32], maxs: &[f32]) -> [f32; 2] {
    let [lo, hi] = mins
        .iter()
        .zip(maxs)
        .fold([f32::INFINITY, f32::NEG_INFINITY], |[lo, hi], (&m, &x)| {
            [lo.min(m), hi.max(x)]
        });
    if lo > hi { [0.0; 2] } else { [lo, hi] }
}

fn scan_level(buffer: &AudioBuffer, frames_per_peak: u32) -> PeakLevel {
    let chunk = frames_per_peak.max(1) as usize;
    let buckets = bucket_count(buffer.frames(), frames_per_peak);
    let mut min = vec![f32::INFINITY; buckets];
    let mut max = vec![f32::NEG_INFINITY; buckets];
    for channel in &buffer.channels {
        for (bucket, samples) in channel.chunks(chunk).enumerate() {
            let [lo, hi] = min_max(samples);
            if let (Some(m), Some(x)) = (min.get_mut(bucket), max.get_mut(bucket)) {
                *m = m.min(lo);
                *x = x.max(hi);
            }
        }
    }
    // Buckets that received no samples keep the sentinels; report silence for them.
    for (m, x) in min.iter_mut().zip(max.iter_mut()) {
        if *m > *x {
            *m = 0.0;
            *x = 0.0;
        }
    }
    PeakLevel {
        frames_per_peak,
        min,
        max,
    }
}

fn derive_level(finer: &PeakLevel, frames_per_peak: u32, frames: usize) -> PeakLevel {
    let ratio = (frames_per_peak / finer.frames_per_peak.max(1)) as usize;
    let buckets = bucket_count(frames, frames_per_peak);
    let mut min = Vec::with_capacity(buckets);
    let mut max = Vec::with_capacity(buckets);
    for bucket in 0..buckets {
        let lo = (bucket * ratio).min(finer.min.len());
        let hi = (lo + ratio).min(finer.min.len());
        let [m, x] = reduce(
            finer.min.get(lo..hi).unwrap_or_default(),
            finer.max.get(lo..hi).unwrap_or_default(),
        );
        min.push(m);
        max.push(x);
    }
    PeakLevel {
        frames_per_peak,
        min,
        max,
    }
}

/// Min/max over the buckets of `level` that intersect source frames `a..b`.
fn query_level(level: &PeakLevel, a: u64, b: u64) -> [f32; 2] {
    let frames_per_peak = u64::from(level.frames_per_peak.max(1));
    let lo = usize::try_from(a / frames_per_peak).unwrap_or(usize::MAX);
    let hi = usize::try_from(b.div_ceil(frames_per_peak))
        .unwrap_or(usize::MAX)
        .min(level.min.len());
    reduce(
        level.min.get(lo..hi).unwrap_or_default(),
        level.max.get(lo..hi).unwrap_or_default(),
    )
}

/// Min/max over raw source frames `a..b`, across all channels.
fn scan_range(buffer: &AudioBuffer, a: u64, b: u64) -> [f32; 2] {
    let lo = usize::try_from(a).unwrap_or(usize::MAX);
    let hi = usize::try_from(b).unwrap_or(usize::MAX);
    let mut out = [f32::INFINITY, f32::NEG_INFINITY];
    for samples in buffer.channels.iter().filter_map(|ch| ch.get(lo..hi)) {
        let [m, x] = min_max(samples);
        out = [out[0].min(m), out[1].max(x)];
    }
    if out[0] > out[1] { [0.0; 2] } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mono(samples: Vec<f32>) -> AudioBuffer {
        AudioBuffer::new(vec![samples])
    }

    fn ramp(frames: usize) -> AudioBuffer {
        mono((0..frames).map(|i| i as f32).collect())
    }

    fn level(peaks: &Peaks, frames_per_peak: u32) -> &PeakLevel {
        peaks
            .levels
            .iter()
            .find(|l| l.frames_per_peak == frames_per_peak)
            .expect("level exists")
    }

    #[test]
    fn build_peaks_ramp_gives_exact_min_max_at_every_level() {
        let frames = 40_000;
        let peaks = build_peaks(&ramp(frames));

        for (&fpp, l) in PEAK_LEVELS.iter().zip(&peaks.levels) {
            let fpp = fpp as usize;
            assert_eq!(l.min.len(), frames.div_ceil(fpp), "bucket count at {fpp}");
            for (j, (&m, &x)) in l.min.iter().zip(&l.max).enumerate() {
                let first = (j * fpp) as f32;
                let last = ((j * fpp + fpp).min(frames) - 1) as f32;
                assert_eq!(m, first, "min of bucket {j} at {fpp}");
                assert_eq!(x, last, "max of bucket {j} at {fpp}");
            }
        }
    }

    #[test]
    fn build_peaks_last_bucket_is_partial() {
        let peaks = build_peaks(&ramp(300));

        let finest = level(&peaks, 256);
        assert_eq!(finest.min.len(), 2);
        assert_eq!((finest.min[1], finest.max[1]), (256.0, 299.0));
    }

    #[test]
    fn build_peaks_combines_channels_into_one_min_max() {
        let buffer = AudioBuffer::new(vec![vec![0.1, 0.9], vec![-0.7, 0.2]]);

        let peaks = build_peaks(&buffer);

        let finest = level(&peaks, 256);
        assert_eq!(finest.min, vec![-0.7]);
        assert_eq!(finest.max, vec![0.9]);
    }

    #[test]
    fn build_peaks_empty_buffer_has_no_buckets() {
        let peaks = build_peaks(&AudioBuffer::default());

        assert_eq!(peaks.frames, 0);
        assert_eq!(peaks.levels.len(), PEAK_LEVELS.len());
        assert!(
            peaks
                .levels
                .iter()
                .all(|l| l.min.is_empty() && l.max.is_empty())
        );
    }

    #[test]
    fn query_returns_exactly_requested_column_count() {
        let buffer = ramp(10_000);
        let peaks = build_peaks(&buffer);

        for columns in [1, 7, 100, 333] {
            let cols = peaks.query(&buffer, 0, 10_000, columns);
            assert_eq!(cols.len(), columns, "columns {columns}");
        }
    }

    #[test]
    fn query_zero_columns_returns_empty() {
        let buffer = ramp(100);
        let peaks = build_peaks(&buffer);

        assert!(peaks.query(&buffer, 0, 100, 0).is_empty());
    }

    #[test]
    fn query_empty_range_returns_zero_pairs() {
        let buffer = ramp(100);
        let peaks = build_peaks(&buffer);

        assert_eq!(peaks.query(&buffer, 50, 50, 3), vec![[0.0, 0.0]; 3]);
        assert_eq!(peaks.query(&buffer, 60, 10, 2), vec![[0.0, 0.0]; 2]);
    }

    #[test]
    fn query_columns_past_end_of_audio_are_zero() {
        let buffer = ramp(1000);
        let peaks = build_peaks(&buffer);

        // 200 frames per column: columns 0..5 cover the audio, 5..10 lie past its end.
        let cols = peaks.query(&buffer, 0, 2000, 10);

        assert_eq!(cols.len(), 10);
        assert_eq!(cols[0], [0.0, 199.0]);
        assert_eq!(cols[4], [800.0, 999.0]);
        assert!(cols[5..].iter().all(|c| *c == [0.0, 0.0]));
    }

    #[test]
    fn query_zoomed_in_reads_raw_samples_and_finds_single_spike() {
        let mut samples = vec![0.0_f32; 10_000];
        samples[5000] = 0.9;
        let buffer = mono(samples);
        let peaks = build_peaks(&buffer);

        // 20 frames over 20 columns: one frame per column, below the finest level (256).
        let cols = peaks.query(&buffer, 4990, 5010, 20);

        // A one-frame column has min == max == the sample.
        assert_eq!(cols[10], [0.9, 0.9]);
        let hits = cols.iter().filter(|c| c[1] > 0.0).count();
        assert_eq!(hits, 1);
    }

    #[test]
    fn query_column_edges_follow_half_open_f64_split() {
        let mut samples = vec![0.0_f32; 10];
        samples[6] = 0.5;
        let buffer = mono(samples);
        let peaks = build_peaks(&buffer);

        // Three columns over frames 0..10: [0,3) [3,6) [6,10).
        let cols = peaks.query(&buffer, 0, 10, 3);

        assert_eq!(cols[1], [0.0, 0.0]);
        assert_eq!(cols[2], [0.0, 0.5]);
    }

    #[test]
    fn query_zoomed_out_uses_level_and_finds_single_sample_spike() {
        let mut samples = vec![0.0_f32; 200_000];
        samples[100_000] = -0.7;
        let buffer = mono(samples);
        let peaks = build_peaks(&buffer);

        // 2000 frames per column: coarsest level with frames_per_peak <= 2000 is 1024.
        // Buckets intersecting a column may reach into its neighbour, so allow one extra hit.
        let cols = peaks.query(&buffer, 0, 200_000, 100);

        assert_eq!(cols[50][0], -0.7);
        assert!(cols.iter().filter(|c| c[0] < 0.0).count() <= 2);
    }

    #[test]
    fn query_very_coarse_level_still_finds_single_spike() {
        let mut samples = vec![0.0_f32; 200_000];
        samples[100_000] = 0.6;
        let buffer = mono(samples);
        let peaks = build_peaks(&buffer);

        // 20000 frames per column selects the 16384 level.
        let cols = peaks.query(&buffer, 0, 200_000, 10);

        assert_eq!(cols[5][1], 0.6);
        assert!(cols.iter().filter(|c| c[1] > 0.0).count() <= 2);
    }
}
