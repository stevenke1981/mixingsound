//! Offline render ("bounce") of a `RenderGraph` to a WAV file, as fast as possible, using the
//! same `render::Mixer` as real-time playback.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::graph::RenderGraph;

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
    /// Highest absolute sample in dBFS (before clamping).
    pub peak_db: f32,
    /// Samples that exceeded ±1.0 and were clamped (integer formats only).
    pub clipped_samples: u64,
}

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
    let _ = (graph, options, path, progress);
    todo!("io worker")
}
