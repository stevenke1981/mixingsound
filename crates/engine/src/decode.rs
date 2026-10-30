//! File decoding (symphonia) + resampling to the project rate (rubato).

use std::io::ErrorKind;
use std::path::Path;

use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Resampler, WindowFunction};
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::buffer::AudioBuffer;
use crate::error::{EngineError, Result};

/// Engine audio is mono or stereo; wider sources keep their first two channels.
const MAX_OUTPUT_CHANNELS: usize = 2;
/// Requested block sizes, tried in order. rubato rounds the block to a multiple of the rate
/// ratio; when either resulting FFT size is odd, the reported delay (`fft_size_out / 2`) is
/// rounded down and the output lands half a frame early. Measured on 44.1/48/96 kHz pairs.
const RESAMPLE_CHUNK_CANDIDATES: [usize; 4] = [4096, 8192, 2048, 16384];
const RESAMPLE_SUB_CHUNKS: usize = 1;
const RESAMPLE_WINDOW: WindowFunction = WindowFunction::BlackmanHarris2;

#[derive(Debug, Clone)]
pub struct Decoded {
    /// 1 (mono) or 2 (stereo) channels at the requested sample rate.
    /// Sources with > 2 channels keep only the first two.
    pub buffer: AudioBuffer,
    pub original_sample_rate: u32,
    pub original_channels: u16,
}

/// Decodes the whole file (WAV, FLAC, MP3, OGG/Vorbis, AIFF, … whatever symphonia "all"
/// supports) and resamples to `target_sample_rate` with rubato's fixed-ratio FFT resampler.
/// Output length = round(in_frames · target / original) frames (no resampler delay padding).
/// Errors: unreadable file → `Io`; unknown format/codec → `Unsupported`; corrupt data →
/// `Decode` (recoverable packet errors are skipped, not fatal).
pub fn decode_file(path: &Path, target_sample_rate: u32) -> Result<Decoded> {
    if target_sample_rate == 0 {
        return Err(EngineError::Unsupported(
            "target sample rate must be greater than zero".into(),
        ));
    }
    let file = std::fs::File::open(path).map_err(|e| EngineError::io(path, e))?;
    let mut format = probe(path, file)?;

    let track = format
        .first_track(TrackType::Audio)
        .ok_or_else(|| EngineError::Unsupported("no audio track in file".into()))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .cloned()
        .ok_or_else(|| EngineError::Unsupported("audio track has no codec parameters".into()))?;

    let original_rate = params
        .sample_rate
        .filter(|rate| *rate > 0)
        .ok_or_else(|| EngineError::Unsupported("unknown sample rate".into()))?;
    let channel_count = params
        .channels
        .as_ref()
        .map(|c| c.count())
        .filter(|count| *count > 0)
        .ok_or_else(|| EngineError::Unsupported("unknown channel layout".into()))?;
    let original_channels = u16::try_from(channel_count)
        .map_err(|_| EngineError::Unsupported(format!("{channel_count} channels")))?;

    let planes = decode_planes(path, format.as_mut(), track_id, &params, channel_count)?;
    let planes = if original_rate == target_sample_rate {
        planes
    } else {
        resample(&planes, original_rate, target_sample_rate)?
    };

    Ok(Decoded {
        buffer: AudioBuffer::new(planes),
        original_sample_rate: original_rate,
        original_channels,
    })
}

/// Opens the container. Probe failures on unrecognised data are `Unsupported`; a file that ends
/// inside the probe is corrupt rather than unknown, so it is `Decode`.
fn probe(path: &Path, file: std::fs::File) -> Result<Box<dyn FormatReader + 'static>> {
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|err| match err {
            SymphoniaError::IoError(io) if io.kind() == ErrorKind::UnexpectedEof => {
                EngineError::Decode {
                    path: path.to_path_buf(),
                    reason: "file ends before a complete audio header".into(),
                }
            }
            other => map_symphonia_error(path, other),
        })
}

/// Reads every packet of `track_id` and returns the first `min(channels, 2)` planes.
fn decode_planes(
    path: &Path,
    format: &mut dyn FormatReader,
    track_id: u32,
    params: &AudioCodecParameters,
    channel_count: usize,
) -> Result<Vec<Vec<f32>>> {
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|err| map_symphonia_error(path, err))?;
    let kept = channel_count.min(MAX_OUTPUT_CHANNELS);
    let mut planes: Vec<Vec<f32>> = vec![Vec::new(); kept];
    let mut scratch: Vec<Vec<f32>> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            // A truncated container ends the stream; keep what was decoded so far.
            Err(SymphoniaError::IoError(io)) if io.kind() == ErrorKind::UnexpectedEof => break,
            Err(err) => return Err(map_symphonia_error(path, err)),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(buffer) => buffer,
            // Corrupt or unreadable packet: skip it and keep going with the next one.
            Err(SymphoniaError::IoError(_) | SymphoniaError::DecodeError(_)) => continue,
            Err(err) => return Err(map_symphonia_error(path, err)),
        };
        if decoded.num_planes() != channel_count {
            return Err(EngineError::Decode {
                path: path.to_path_buf(),
                reason: "channel count changed mid-stream".into(),
            });
        }
        decoded.copy_to_vecs_planar(&mut scratch);
        for (plane, samples) in planes.iter_mut().zip(&scratch) {
            plane.extend_from_slice(samples);
        }
    }
    Ok(planes)
}

/// Resamples each plane with rubato's fixed-ratio FFT resampler. `process_all_into_buffer`
/// drops the startup delay, so output sample 0 lines up with input sample 0, and it flushes
/// the tail. The result is trimmed to round(frames · to / from), which is never longer than
/// the ceil(...) frames rubato reports.
fn resample(planes: &[Vec<f32>], from: u32, to: u32) -> Result<Vec<Vec<f32>>> {
    let channels = planes.len();
    let frames = planes.first().map_or(0, Vec::len);
    let target_frames = rounded_frame_count(frames, from, to);
    if frames == 0 {
        return Ok(vec![Vec::new(); channels]);
    }

    let mut resampler = new_resampler(from, to, channels)?;
    let input = SequentialSliceOfVecs::new(planes, channels, frames)
        .map_err(|err| EngineError::Resample(err.to_string()))?;
    let needed = resampler.process_all_needed_output_len(frames);
    let mut output: Vec<Vec<f32>> = vec![vec![0.0; needed]; channels];
    let produced = {
        let mut output_adapter = SequentialSliceOfVecs::new_mut(&mut output, channels, needed)
            .map_err(|err| EngineError::Resample(err.to_string()))?;
        let (_, produced) = resampler
            .process_all_into_buffer(&input, &mut output_adapter, frames, None)
            .map_err(|err| EngineError::Resample(err.to_string()))?;
        produced
    };
    if produced < target_frames {
        return Err(EngineError::Resample(format!(
            "resampler produced {produced} frames, expected {target_frames}"
        )));
    }
    for plane in &mut output {
        plane.truncate(target_frames);
    }
    Ok(output)
}

/// Fixed-ratio FFT resampler with the first candidate block size that gives even FFT sizes,
/// falling back to the first candidate when none does.
fn new_resampler(from: u32, to: u32, channels: usize) -> Result<Fft<f32>> {
    let mut fallback = None;
    for chunk in RESAMPLE_CHUNK_CANDIDATES {
        let resampler = Fft::new_custom(
            from as usize,
            to as usize,
            chunk,
            RESAMPLE_SUB_CHUNKS,
            channels,
            RESAMPLE_WINDOW,
            FixedSync::Both,
        )
        .map_err(|err| EngineError::Resample(err.to_string()))?;
        if resampler.fft_size_in() % 2 == 0 && resampler.fft_size_out() % 2 == 0 {
            return Ok(resampler);
        }
        fallback.get_or_insert(resampler);
    }
    fallback.ok_or_else(|| EngineError::Resample("no resampler configuration".into()))
}

/// round(frames · to / from) with halves rounded up, computed in integers.
fn rounded_frame_count(frames: usize, from: u32, to: u32) -> usize {
    let frames = frames as u128;
    let from = u128::from(from);
    let to = u128::from(to);
    let rounded = (frames * to * 2 + from) / (from * 2);
    usize::try_from(rounded).unwrap_or(usize::MAX)
}

fn map_symphonia_error(path: &Path, err: SymphoniaError) -> EngineError {
    match err {
        SymphoniaError::IoError(io) => EngineError::io(path, io),
        SymphoniaError::Unsupported(what) => EngineError::Unsupported(what.to_string()),
        SymphoniaError::ResetRequired => {
            EngineError::Unsupported("track list changed mid-stream".into())
        }
        other => EngineError::Decode {
            path: path.to_path_buf(),
            reason: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;
    use std::path::{Path, PathBuf};

    use super::*;

    const SINE_HZ: f64 = 1000.0;
    const SINE_AMP: f64 = 0.5;

    fn sine(rate: u32, frames: usize, amp: f64) -> Vec<f32> {
        (0..frames)
            .map(|i| (amp * (TAU * SINE_HZ * i as f64 / f64::from(rate)).sin()) as f32)
            .collect()
    }

    /// Writes interleaved float samples as a 32-bit IEEE float WAV.
    fn write_float_wav(path: &Path, channels: u16, rate: u32, interleaved: &[f32]) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("create float wav");
        for &sample in interleaved {
            writer.write_sample(sample).expect("write float sample");
        }
        writer.finalize().expect("finalize float wav");
    }

    /// Writes interleaved integer samples (already scaled to `bits`) as a PCM WAV.
    fn write_int_wav(path: &Path, channels: u16, rate: u32, bits: u16, interleaved: &[i32]) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: bits,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("create int wav");
        for &sample in interleaved {
            writer.write_sample(sample).expect("write int sample");
        }
        writer.finalize().expect("finalize int wav");
    }

    /// Quantises to 16-bit exactly as the fixture writer does, so references stay exact.
    fn to_i16(samples: &[f32]) -> Vec<i32> {
        samples
            .iter()
            .map(|&s| (f64::from(s) * 32767.0).round() as i32)
            .collect()
    }

    fn from_i16(samples: &[i32]) -> Vec<f32> {
        samples.iter().map(|&s| s as f32 / 32767.0).collect()
    }

    /// Rising zero crossings with linear interpolation, as fractional frame indices.
    fn rising_crossings(samples: &[f32]) -> Vec<f64> {
        samples
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0)
            .map(|(i, w)| {
                let (a, b) = (f64::from(w[0]), f64::from(w[1]));
                i as f64 + (-a) / (b - a)
            })
            .collect()
    }

    fn expect_decoded(path: &Path, rate: u32) -> Decoded {
        decode_file(path, rate).expect("decode should succeed")
    }

    fn write_noise_file(dir: &tempfile::TempDir) -> PathBuf {
        let path = dir.path().join("noise.bin");
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let bytes: Vec<u8> = (0..4096)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect();
        std::fs::write(&path, bytes).expect("write noise");
        path
    }

    #[test]
    fn decode_resamples_44100_mono_to_48000_with_expected_frame_count() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("one_second.wav");
        write_int_wav(&path, 1, 44100, 16, &to_i16(&sine(44100, 44100, SINE_AMP)));

        let decoded = expect_decoded(&path, 48000);

        assert_eq!(decoded.buffer.frames(), 48000);
        assert_eq!(decoded.buffer.channel_count(), 1);
        assert_eq!(decoded.original_sample_rate, 44100);
        assert_eq!(decoded.original_channels, 1);
    }

    #[test]
    fn decode_frame_count_rounds_half_up_for_non_integer_ratio() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("short.wav");
        write_int_wav(&path, 1, 44100, 16, &to_i16(&sine(44100, 1000, SINE_AMP)));

        let decoded = expect_decoded(&path, 48000);

        // round(1000 · 48000 / 44100) = round(1088.435…) = 1088
        assert_eq!(decoded.buffer.frames(), 1088);
    }

    #[test]
    fn resampled_1khz_sine_keeps_amplitude_and_phase() {
        let dir = tempfile::tempdir().expect("tempdir");
        let frames = 44100 * 2;
        let path = dir.path().join("sine.wav");
        let source = to_i16(&sine(44100, frames, SINE_AMP));
        write_int_wav(&path, 1, 44100, 16, &source);

        let decoded = expect_decoded(&path, 48000);
        let out = &decoded.buffer.channels[0];
        let margin = out.len() / 10;
        let middle = &out[margin..out.len() - margin];
        let start = margin;
        let reference: Vec<f32> =
            sine(48000, out.len(), SINE_AMP)[start..start + middle.len()].to_vec();

        let max_err = middle
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        let rms = |s: &[f32]| {
            (s.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / s.len() as f64).sqrt()
        };
        let rms_ratio = rms(middle) / rms(&reference);

        assert!(
            max_err < 1e-3,
            "max sample error {max_err} too large (delay or filter bug)"
        );
        assert!(
            (rms_ratio - 1.0).abs() < 0.01,
            "RMS ratio {rms_ratio} outside 1%"
        );
    }

    #[test]
    fn resampled_sine_is_aligned_for_common_rate_pairs() {
        let pairs = [
            (44100, 48000),
            (48000, 44100),
            (32000, 48000),
            (96000, 44100),
            (22050, 48000),
            (44100, 96000),
        ];
        for (from, to) in pairs {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("pair.wav");
            let source_frames = from as usize * 2;
            write_int_wav(
                &path,
                1,
                from,
                16,
                &to_i16(&sine(from, source_frames, SINE_AMP)),
            );

            let decoded = expect_decoded(&path, to);

            let out = &decoded.buffer.channels[0];
            let margin = out.len() / 10;
            let reference = sine(to, out.len(), SINE_AMP);
            let max_err = out[margin..out.len() - margin]
                .iter()
                .zip(&reference[margin..out.len() - margin])
                .map(|(a, b)| (a - b).abs())
                .fold(0.0_f32, f32::max);
            assert_eq!(out.len(), rounded_frame_count(source_frames, from, to));
            assert!(max_err < 1e-3, "{from}->{to}: max error {max_err}");
        }
    }

    #[test]
    fn resampled_sine_zero_crossings_match_analytic_positions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let frames = 44100 * 2;
        let path = dir.path().join("sine_zc.wav");
        write_int_wav(&path, 1, 44100, 16, &to_i16(&sine(44100, frames, SINE_AMP)));

        let decoded = expect_decoded(&path, 48000);
        let out = &decoded.buffer.channels[0];
        let found = rising_crossings(out);
        // A 1 kHz sine has a rising crossing every 48 frames at 48 kHz, starting at frame 0.
        let inner: Vec<f64> = found
            .iter()
            .copied()
            .filter(|&t| t > 4800.0 && t < out.len() as f64 - 4800.0)
            .collect();

        assert!(inner.len() > 150, "too few crossings: {}", inner.len());
        for t in inner {
            let k = (t / 48.0).round();
            let err = (t - k * 48.0).abs();
            assert!(
                err < 0.05,
                "crossing at {t} is {err} frames off the analytic grid"
            );
        }
    }

    #[test]
    fn decode_same_rate_stereo_float32_is_identical_within_tolerance() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("stereo_f32.wav");
        let frames = 4800;
        let left = sine(48000, frames, 0.8);
        let right = sine(48000, frames, -0.3);
        let interleaved: Vec<f32> = left
            .iter()
            .zip(&right)
            .flat_map(|(&l, &r)| [l, r])
            .collect();
        write_float_wav(&path, 2, 48000, &interleaved);

        let decoded = expect_decoded(&path, 48000);

        assert_eq!(decoded.buffer.channel_count(), 2);
        assert_eq!(decoded.buffer.frames(), frames);
        assert_eq!(decoded.original_sample_rate, 48000);
        let max_err = decoded.buffer.channels[0]
            .iter()
            .zip(&left)
            .chain(decoded.buffer.channels[1].iter().zip(&right))
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(max_err < 1e-6, "max error {max_err}");
    }

    #[test]
    fn decode_24bit_integer_samples_scale_to_unit_range() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("pcm24.wav");
        let full_scale = 8_388_607.0_f64;
        let source: Vec<i32> = sine(48000, 4800, SINE_AMP)
            .iter()
            .map(|&s| (f64::from(s) * full_scale).round() as i32)
            .collect();
        write_int_wav(&path, 1, 48000, 24, &source);

        let decoded = expect_decoded(&path, 48000);

        let out = &decoded.buffer.channels[0];
        let max_err = out
            .iter()
            .zip(&source)
            .map(|(&a, &b)| (f64::from(a) - b as f64 / full_scale).abs())
            .fold(0.0_f64, f64::max);
        let peak = out.iter().fold(0.0_f32, |p, &x| p.max(x.abs()));
        assert!(max_err < 1e-6, "24-bit scaling error {max_err}");
        assert!((f64::from(peak) - SINE_AMP).abs() < 1e-3, "peak {peak}");
    }

    #[test]
    fn decode_four_channel_file_keeps_first_two_channels() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("quad.wav");
        let frames = 480;
        let levels = [0.25_f32, -0.5, 0.9, 0.1];
        let interleaved: Vec<i32> = (0..frames)
            .flat_map(|_| {
                levels
                    .iter()
                    .map(|&v| (f64::from(v) * 32767.0).round() as i32)
            })
            .collect();
        write_int_wav(&path, 4, 48000, 16, &interleaved);

        let decoded = expect_decoded(&path, 48000);

        assert_eq!(decoded.buffer.channel_count(), 2);
        assert_eq!(decoded.original_channels, 4);
        assert_eq!(decoded.buffer.frames(), frames);
        assert!((decoded.buffer.channels[0][0] - 0.25).abs() < 1e-4);
        assert!((decoded.buffer.channels[1][0] + 0.5).abs() < 1e-4);
    }

    #[test]
    fn decode_missing_file_returns_io_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("does_not_exist.wav");

        let err = decode_file(&path, 48000).expect_err("missing file must fail");

        assert!(matches!(err, EngineError::Io { .. }), "got {err:?}");
    }

    #[test]
    fn decode_random_bytes_returns_error_without_panicking() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_noise_file(&dir);

        let err = decode_file(&path, 48000).expect_err("noise must not decode");

        assert!(
            matches!(
                err,
                EngineError::Unsupported(_) | EngineError::Decode { .. }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn decode_text_file_returns_unsupported_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "this is not audio at all\n".repeat(50)).expect("write text");

        let err = decode_file(&path, 48000).expect_err("text must not decode");

        assert!(
            matches!(
                err,
                EngineError::Unsupported(_) | EngineError::Decode { .. }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn decode_zero_byte_file_returns_error_without_panicking() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("empty.mp3");
        std::fs::write(&path, b"").expect("write empty");

        let err = decode_file(&path, 48000).expect_err("empty file must fail");

        assert!(
            matches!(
                err,
                EngineError::Unsupported(_) | EngineError::Decode { .. }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn decode_empty_data_wav_returns_zero_frames() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("empty.wav");
        write_int_wav(&path, 1, 44100, 16, &[]);

        let decoded = expect_decoded(&path, 48000);

        assert_eq!(decoded.buffer.frames(), 0);
    }

    #[test]
    fn decode_zero_target_rate_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("any.wav");
        write_int_wav(&path, 1, 44100, 16, &[0, 1, 2]);

        let err = decode_file(&path, 0).expect_err("zero rate must fail");

        assert!(matches!(err, EngineError::Unsupported(_)), "got {err:?}");
    }

    #[test]
    fn decode_truncated_wav_keeps_decoded_prefix() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("full.wav");
        let samples = to_i16(&sine(48000, 48000, SINE_AMP));
        write_int_wav(&path, 1, 48000, 16, &samples);
        let bytes = std::fs::read(&path).expect("read fixture");
        let cut = bytes.len() / 2;
        std::fs::write(&path, bytes.get(..cut).unwrap_or(&bytes)).expect("truncate fixture");

        let decoded = expect_decoded(&path, 48000);

        let frames = decoded.buffer.frames();
        assert!(frames > 0 && frames < 48000, "frames {frames}");
        let reference = from_i16(&samples);
        let max_err = decoded.buffer.channels[0]
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        // 16-bit decoding is not bit-exact against our float reference: tolerate one LSB.
        assert!(max_err < 1e-4, "prefix mismatch {max_err}");
    }
}
