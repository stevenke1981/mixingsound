//! File decoding (symphonia) + resampling to the project rate (rubato).

use std::path::Path;

use crate::buffer::AudioBuffer;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct Decoded {
    /// 1 (mono) or 2 (stereo) channels at the requested sample rate.
    /// Sources with > 2 channels keep only the first two.
    pub buffer: AudioBuffer,
    pub original_sample_rate: u32,
    pub original_channels: u16,
}

/// Decodes the whole file (WAV, FLAC, MP3, OGG/Vorbis, AIFF, … whatever symphonia "all"
/// supports) and resamples to `target_sample_rate` with a high-quality sinc resampler.
/// Output length = round(in_frames · target / original) frames (no resampler delay padding).
/// Errors: unreadable file → `Io`; unknown format/codec → `Unsupported`; corrupt data →
/// `Decode` (recoverable packet errors are skipped, not fatal).
pub fn decode_file(path: &Path, target_sample_rate: u32) -> Result<Decoded> {
    let _ = (path, target_sample_rate);
    todo!("decode worker")
}
