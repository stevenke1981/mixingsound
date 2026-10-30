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
    let _ = buffer;
    todo!("decode worker")
}

impl Peaks {
    /// Waveform for source frames `start..end` split into `columns` equal columns.
    /// Returns `[min, max]` per column (exactly `columns` entries; columns past the end of the
    /// audio are `[0, 0]`). Uses the coarsest level whose `frames_per_peak` ≤ frames per
    /// column; when a column spans fewer frames than the finest level, reads `buffer` directly.
    pub fn query(
        &self,
        buffer: &AudioBuffer,
        start: u64,
        end: u64,
        columns: usize,
    ) -> Vec<[f32; 2]> {
        let _ = (buffer, start, end, columns);
        todo!("decode worker")
    }
}
