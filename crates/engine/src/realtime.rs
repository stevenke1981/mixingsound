//! Real-time playback through the default output device (cpal).
//!
//! Threading: the cpal `Stream` is built and owned by a dedicated "audio-host" thread, so
//! `start` only keeps a channel handle to it. Control → audio: `rtrb` command ring. Audio →
//! control: meter ring, retired-graph ring (old `Arc<RenderGraph>`s are sent back so they are
//! never freed on the audio thread) and atomics for position / playing.
//! The data callback never allocates, locks, blocks, does I/O or logs.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, I24, OutputCallbackInfo, Sample, SampleFormat, SizedSample, StreamConfig};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use serde::Serialize;

use crate::error::{EngineError, Result};
use crate::graph::RenderGraph;
use crate::render::{MAX_BLOCK, MeterSnapshot, Mixer};

const COMMAND_CAPACITY: usize = 256;
/// The UI polls meters at a few tens of Hz; a short ring keeps latency low and memory small.
const METER_CAPACITY: usize = 8;
/// Retired graphs are freed by `collect_garbage` on each UI tick, so this only has to cover
/// a burst of edits between two ticks.
const RETIRED_CAPACITY: usize = 64;
/// Meter cadence (about 23 Hz at 48 kHz).
const METER_INTERVAL_FRAMES: usize = 1024;
const THREAD_NAME: &str = "audio-host";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportStatus {
    pub playing: bool,
    /// Current timeline frame (start of the next block to render).
    pub position: u64,
}

/// Control-thread messages. Applied in FIFO order by the audio callback.
enum Command {
    SetGraph(Arc<RenderGraph>),
    Play,
    Pause,
    Stop,
    Seek(u64),
    SetLoop(Option<(u64, u64)>),
}

/// State shared between the control thread and the audio thread.
#[derive(Default)]
struct Shared {
    playing: AtomicBool,
    position: AtomicU64,
    /// Written by cpal's error callback, which runs outside the data callback, so a mutex is
    /// acceptable here. The data path never touches it.
    error: Mutex<Option<String>>,
}

/// `Send` handle; put it behind a `Mutex` in app state.
pub struct RealtimeEngine {
    commands: Producer<Command>,
    meters: Consumer<MeterSnapshot>,
    retired: Consumer<Arc<RenderGraph>>,
    shared: Arc<Shared>,
    sample_rate: u32,
    device_name: String,
    /// Dropping the sender wakes the audio thread, which then releases the stream.
    shutdown: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl RealtimeEngine {
    /// Opens the default output device in its default shared-mode config (stereo f32 when
    /// possible; other sample formats/channel counts are converted in the callback: our stereo
    /// goes to channels 0/1, mono devices get (L+R)/2). Output is clamped to ±1.0.
    /// Starts paused at position 0 with an empty graph.
    pub fn start() -> Result<Self> {
        let (command_tx, command_rx) = RingBuffer::new(COMMAND_CAPACITY);
        let (meter_tx, meter_rx) = RingBuffer::new(METER_CAPACITY);
        let (retired_tx, retired_rx) = RingBuffer::new(RETIRED_CAPACITY);
        let shared = Arc::new(Shared::default());
        let (ready_tx, ready_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let setup = HostSetup {
            commands: command_rx,
            meters: meter_tx,
            retired: retired_tx,
            shared: Arc::clone(&shared),
            ready: ready_tx,
            shutdown: shutdown_rx,
        };
        let thread = std::thread::Builder::new()
            .name(THREAD_NAME.into())
            .spawn(move || host_thread(setup))
            .map_err(|e| EngineError::Device(format!("cannot start audio thread: {e}")))?;

        let ready = match ready_rx.recv() {
            Ok(Ok(ready)) => ready,
            Ok(Err(message)) => {
                join_quietly(thread);
                return Err(EngineError::Device(message));
            }
            Err(_) => {
                join_quietly(thread);
                return Err(EngineError::Device(
                    "audio thread exited during startup".into(),
                ));
            }
        };
        Ok(Self {
            commands: command_tx,
            meters: meter_rx,
            retired: retired_rx,
            shared,
            sample_rate: ready.sample_rate,
            device_name: ready.device_name,
            shutdown: Some(shutdown_tx),
            thread: Some(thread),
        })
    }

    /// Device sample rate. New projects use this rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Swaps the graph the callback renders (takes effect at the next block).
    pub fn set_graph(&mut self, graph: Arc<RenderGraph>) -> Result<()> {
        push_command(&mut self.commands, Command::SetGraph(graph))
    }

    pub fn play(&mut self) -> Result<()> {
        push_command(&mut self.commands, Command::Play)
    }

    /// Pauses at the current position.
    pub fn pause(&mut self) -> Result<()> {
        push_command(&mut self.commands, Command::Pause)
    }

    /// Pauses and returns to frame 0.
    pub fn stop(&mut self) -> Result<()> {
        push_command(&mut self.commands, Command::Stop)
    }

    /// Moves the playhead (mixer smoothing is reset to avoid ramps from stale state).
    pub fn seek(&mut self, frame: u64) -> Result<()> {
        push_command(&mut self.commands, Command::Seek(frame))
    }

    /// Loop region `start..end` (end exclusive, start < end), or `None` to disable. While
    /// playing and looping, reaching `end` jumps to `start` sample-accurately inside the block.
    pub fn set_loop(&mut self, region: Option<(u64, u64)>) -> Result<()> {
        let region = validate_loop(region)?;
        push_command(&mut self.commands, Command::SetLoop(region))
    }

    /// Values published by the audio thread at the end of its last callback, so they lag the
    /// control calls by at most one device buffer.
    pub fn status(&self) -> TransportStatus {
        TransportStatus {
            playing: self.shared.playing.load(Ordering::Relaxed),
            position: self.shared.position.load(Ordering::Relaxed),
        }
    }

    /// Latest meter snapshot since the previous call (drains the ring), or `None`.
    pub fn poll_meters(&mut self) -> Option<MeterSnapshot> {
        let mut latest = None;
        while let Ok(snapshot) = self.meters.pop() {
            latest = Some(snapshot);
        }
        latest
    }

    /// Last asynchronous device/stream error (e.g. device unplugged), cleared on read.
    pub fn take_error(&mut self) -> Option<String> {
        let mut slot = self
            .shared
            .error
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        slot.take()
    }

    /// Drops graphs retired by the audio thread. Call regularly (each UI tick) from the
    /// control thread.
    pub fn collect_garbage(&mut self) {
        while self.retired.pop().is_ok() {}
    }
}

impl Drop for RealtimeEngine {
    fn drop(&mut self) {
        drop(self.shutdown.take());
        if let Some(thread) = self.thread.take() {
            join_quietly(thread);
        }
    }
}

/// A panicked audio thread is ignored at shutdown: there is nothing left for the caller to
/// recover, and dropping an engine must not panic.
fn join_quietly(thread: JoinHandle<()>) {
    let _ = thread.join();
}

fn push_command(commands: &mut Producer<Command>, command: Command) -> Result<()> {
    commands.push(command).map_err(|_| EngineError::QueueFull)
}

/// Rejects empty or inverted loop ranges on the control side, so the audio side only sees
/// valid regions.
fn validate_loop(region: Option<(u64, u64)>) -> Result<Option<(u64, u64)>> {
    match region {
        None => Ok(None),
        Some((start, end)) if start < end => Ok(Some((start, end))),
        Some((start, end)) => Err(EngineError::InvalidEdit(format!(
            "loop start {start} must be before loop end {end}"
        ))),
    }
}

/// Everything the audio-host thread needs, moved into it at spawn.
struct HostSetup {
    commands: Consumer<Command>,
    meters: Producer<MeterSnapshot>,
    retired: Producer<Arc<RenderGraph>>,
    shared: Arc<Shared>,
    ready: mpsc::Sender<std::result::Result<DeviceReady, String>>,
    shutdown: mpsc::Receiver<()>,
}

struct DeviceReady {
    sample_rate: u32,
    device_name: String,
}

/// Owns the stream for its whole life: the stream is created and dropped on this thread.
fn host_thread(setup: HostSetup) {
    let HostSetup {
        commands,
        meters,
        retired,
        shared,
        ready,
        shutdown,
    } = setup;
    match open_output(commands, meters, retired, Arc::clone(&shared)) {
        Ok((stream, info)) => {
            let _ = ready.send(Ok(info));
            // Blocks while the engine lives; the sender is dropped in `RealtimeEngine::drop`.
            let _ = shutdown.recv();
            drop(stream);
        }
        Err(message) => {
            let _ = ready.send(Err(message));
        }
    }
}

fn open_output(
    commands: Consumer<Command>,
    meters: Producer<MeterSnapshot>,
    retired: Producer<Arc<RenderGraph>>,
    shared: Arc<Shared>,
) -> std::result::Result<(cpal::Stream, DeviceReady), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no default output device".to_owned())?;
    // A device that cannot report its name can still play, so fall back to a generic label.
    let device_name = device
        .description()
        .map(|description| description.name().to_owned())
        .unwrap_or_else(|_| "unknown output device".to_owned());
    let supported = device
        .default_output_config()
        .map_err(|e| format!("no output configuration: {e}"))?;
    let sample_format = supported.sample_format();
    let config = supported.config();
    if config.channels == 0 {
        return Err("output device reports zero channels".to_owned());
    }
    let sample_rate = config.sample_rate;
    let callback = RenderCallback::new(
        sample_rate,
        usize::from(config.channels),
        commands,
        meters,
        retired,
        Arc::clone(&shared),
    );
    let stream = build_for_format(&device, config, sample_format, callback, shared)?;
    stream
        .play()
        .map_err(|e| format!("cannot start output stream: {e}"))?;
    Ok((
        stream,
        DeviceReady {
            sample_rate,
            device_name,
        },
    ))
}

fn build_for_format(
    device: &cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    callback: RenderCallback,
    errors: Arc<Shared>,
) -> std::result::Result<cpal::Stream, String> {
    let built = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(device, config, callback, errors),
        SampleFormat::F64 => build_stream::<f64>(device, config, callback, errors),
        SampleFormat::I8 => build_stream::<i8>(device, config, callback, errors),
        SampleFormat::I16 => build_stream::<i16>(device, config, callback, errors),
        SampleFormat::I24 => build_stream::<I24>(device, config, callback, errors),
        SampleFormat::I32 => build_stream::<i32>(device, config, callback, errors),
        SampleFormat::U8 => build_stream::<u8>(device, config, callback, errors),
        SampleFormat::U16 => build_stream::<u16>(device, config, callback, errors),
        SampleFormat::U32 => build_stream::<u32>(device, config, callback, errors),
        other => return Err(format!("unsupported output sample format {other}")),
    };
    built.map_err(|e| format!("cannot build output stream: {e}"))
}

fn build_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut callback: RenderCallback,
    errors: Arc<Shared>,
) -> std::result::Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    device.build_output_stream(
        config,
        move |out: &mut [T], _: &OutputCallbackInfo| callback.process(out),
        move |err: cpal::Error| report_error(&errors, &err),
        None,
    )
}

fn report_error(shared: &Shared, err: &cpal::Error) {
    let mut slot = shared.error.lock().unwrap_or_else(PoisonError::into_inner);
    *slot = Some(err.to_string());
}

/// One render step inside a block: render `frames` frames starting at timeline frame `from`,
/// then continue from `next` (already wrapped to the loop start when the loop end was reached).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Step {
    from: u64,
    frames: usize,
    next: u64,
}

/// Plans the next render step for a block with `remaining` frames left. A loop end splits the
/// block so the wrap is sample-accurate. A playhead at or past the loop end jumps to the loop
/// start; an invalid (empty or inverted) region is treated as no loop.
fn next_step(position: u64, remaining: usize, loop_region: Option<(u64, u64)>) -> Step {
    let Some((start, end)) = loop_region.filter(|(start, end)| start < end) else {
        return Step {
            from: position,
            frames: remaining,
            next: position.saturating_add(remaining as u64),
        };
    };
    // `from` is always below `end`, so `to_end` is at least one frame.
    let from = if position >= end { start } else { position };
    let to_end = usize::try_from(end - from).unwrap_or(usize::MAX);
    let frames = remaining.min(to_end);
    let reached = from + frames as u64;
    let next = if reached >= end { start } else { reached };
    Step { from, frames, next }
}

/// Counts rendered frames and reports when a meter snapshot is due.
#[derive(Default)]
struct MeterClock {
    frames_since: usize,
}

impl MeterClock {
    fn advance(&mut self, frames: usize) -> bool {
        self.frames_since = self.frames_since.saturating_add(frames);
        if self.frames_since >= METER_INTERVAL_FRAMES {
            self.frames_since = 0;
            true
        } else {
            false
        }
    }
}

/// Holds the graph being rendered. A swap never drops an `Arc` on the audio thread: the old
/// graph goes to the retired ring, and if that ring is full the swap is deferred to a later
/// callback.
struct GraphSlot {
    current: Arc<RenderGraph>,
    pending: Option<Arc<RenderGraph>>,
    retired: Producer<Arc<RenderGraph>>,
}

impl GraphSlot {
    fn new(current: Arc<RenderGraph>, retired: Producer<Arc<RenderGraph>>) -> Self {
        Self {
            current,
            pending: None,
            retired,
        }
    }

    /// The caller must not request a second graph while one is pending; the command drain
    /// guarantees this, so `pending` is never overwritten (which would drop an `Arc` here).
    fn request(&mut self, next: Arc<RenderGraph>) {
        self.pending = Some(next);
        self.flush_pending();
    }

    fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    fn flush_pending(&mut self) {
        let Some(next) = self.pending.take() else {
            return;
        };
        let old = std::mem::replace(&mut self.current, next);
        if let Err(PushError::Full(old)) = self.retired.push(old) {
            // Undo the swap: the old graph is still rendered and the new one waits.
            let next = std::mem::replace(&mut self.current, old);
            self.pending = Some(next);
        }
    }
}

/// Everything the audio callback reads and writes. Built on the audio-host thread before the
/// stream exists, so the callback itself never allocates.
struct RenderCallback {
    slot: GraphSlot,
    commands: Consumer<Command>,
    meters: Producer<MeterSnapshot>,
    shared: Arc<Shared>,
    mixer: Mixer,
    channels: usize,
    playing: bool,
    position: u64,
    loop_region: Option<(u64, u64)>,
    meter_clock: MeterClock,
    meter_buf: MeterSnapshot,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl RenderCallback {
    /// `channels` must be at least 1.
    fn new(
        sample_rate: u32,
        channels: usize,
        commands: Consumer<Command>,
        meters: Producer<MeterSnapshot>,
        retired: Producer<Arc<RenderGraph>>,
        shared: Arc<Shared>,
    ) -> Self {
        Self {
            slot: GraphSlot::new(Arc::new(RenderGraph::empty(sample_rate)), retired),
            commands,
            meters,
            shared,
            mixer: Mixer::new(sample_rate),
            channels,
            playing: false,
            position: 0,
            loop_region: None,
            meter_clock: MeterClock::default(),
            meter_buf: MeterSnapshot::default(),
            left: vec![0.0; MAX_BLOCK],
            right: vec![0.0; MAX_BLOCK],
        }
    }

    /// Data callback. Splits the device buffer into chunks of at most `MAX_BLOCK` frames.
    fn process<T>(&mut self, out: &mut [T])
    where
        T: SizedSample + FromSample<f32>,
    {
        self.slot.flush_pending();
        self.drain_commands();
        let channels = self.channels;
        let total_frames = out.len() / channels;
        let mut done = 0;
        while done < total_frames {
            let frames = (total_frames - done).min(MAX_BLOCK);
            let Some(window) = out.get_mut(done * channels..(done + frames) * channels) else {
                break;
            };
            if self.playing {
                self.render_chunk(frames);
                let left = self.left.get(..frames).unwrap_or_default();
                let right = self.right.get(..frames).unwrap_or_default();
                write_interleaved(window, channels, left, right);
            } else {
                fill_silence(window);
            }
            self.tick_meters(frames);
            done += frames;
        }
        self.publish();
    }

    fn drain_commands(&mut self) {
        loop {
            // While a swap is deferred, leave further graphs queued. Other commands still run,
            // and FIFO order is kept because only the queue head is ever popped.
            if self.slot.has_pending() && matches!(self.commands.peek(), Ok(Command::SetGraph(_))) {
                break;
            }
            let Ok(command) = self.commands.pop() else {
                break;
            };
            self.apply(command);
        }
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::SetGraph(graph) => self.slot.request(graph),
            Command::Play => self.playing = true,
            Command::Pause => self.playing = false,
            Command::Stop => {
                self.playing = false;
                self.rewind(0);
            }
            Command::Seek(frame) => self.rewind(frame),
            Command::SetLoop(region) => self.loop_region = region,
        }
    }

    fn rewind(&mut self, frame: u64) {
        self.position = frame;
        self.mixer.reset();
    }

    /// Renders `frames` frames into the scratch buffers, following the loop region.
    fn render_chunk(&mut self, frames: usize) {
        let mut written = 0;
        while written < frames {
            let step = next_step(self.position, frames - written, self.loop_region);
            if step.frames == 0 {
                break;
            }
            let window = written..written + step.frames;
            if let (Some(left), Some(right)) = (
                self.left.get_mut(window.clone()),
                self.right.get_mut(window),
            ) {
                self.mixer
                    .render(&self.slot.current, step.from, left, right);
            }
            self.position = step.next;
            written += step.frames;
        }
    }

    /// Pushes a meter snapshot when due. If the ring is full the snapshot is skipped and the
    /// mixer keeps accumulating, so peaks are reported late instead of lost.
    fn tick_meters(&mut self, frames: usize) {
        if !self.meter_clock.advance(frames) || self.meters.is_full() {
            return;
        }
        if self.playing {
            self.mixer.take_meters(&mut self.meter_buf);
            let _ = self.meters.push(self.meter_buf);
        } else {
            let _ = self.meters.push(MeterSnapshot::default());
        }
    }

    fn publish(&self) {
        self.shared.position.store(self.position, Ordering::Relaxed);
        self.shared.playing.store(self.playing, Ordering::Relaxed);
    }
}

/// Keeps device output inside ±1.0. NaN (never produced by a finite graph) becomes silence.
fn clamp_sample(sample: f32) -> f32 {
    if sample.is_nan() {
        0.0
    } else {
        sample.clamp(-1.0, 1.0)
    }
}

/// Writes stereo into the device's interleaved layout: channels 0/1 get L/R, extra channels
/// are silent, and a mono device gets (L+R)/2.
fn write_interleaved<T>(out: &mut [T], channels: usize, left: &[f32], right: &[f32])
where
    T: SizedSample + FromSample<f32>,
{
    for (frame, (&l, &r)) in out.chunks_exact_mut(channels).zip(left.iter().zip(right)) {
        match frame {
            [mono] => *mono = T::from_sample(clamp_sample((l + r) * 0.5)),
            [a, b, rest @ ..] => {
                *a = T::from_sample(clamp_sample(l));
                *b = T::from_sample(clamp_sample(r));
                fill_silence(rest);
            }
            _ => {}
        }
    }
}

fn fill_silence<T: Sample>(out: &mut [T]) {
    for sample in out.iter_mut() {
        *sample = T::EQUILIBRIUM;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn assert_send<T: Send>() {}

    fn graph(sample_rate: u32) -> Arc<RenderGraph> {
        Arc::new(RenderGraph::empty(sample_rate))
    }

    /// Expands a block into the (from, frames) steps the callback would render.
    fn steps(
        mut position: u64,
        mut remaining: usize,
        region: Option<(u64, u64)>,
    ) -> Vec<(u64, usize)> {
        let mut out = Vec::new();
        while remaining > 0 {
            let step = next_step(position, remaining, region);
            assert!(step.frames > 0, "a step must render at least one frame");
            out.push((step.from, step.frames));
            remaining -= step.frames;
            position = step.next;
        }
        out
    }

    #[test]
    fn realtime_engine_is_send() {
        assert_send::<RealtimeEngine>();
    }

    #[test]
    fn validate_loop_accepts_none_and_ordered_range() {
        assert_eq!(validate_loop(None).unwrap(), None);
        assert_eq!(validate_loop(Some((0, 1))).unwrap(), Some((0, 1)));
    }

    #[test]
    fn validate_loop_rejects_empty_or_inverted_range() {
        assert!(matches!(
            validate_loop(Some((5, 5))),
            Err(EngineError::InvalidEdit(_))
        ));
        assert!(matches!(
            validate_loop(Some((9, 5))),
            Err(EngineError::InvalidEdit(_))
        ));
    }

    #[test]
    fn push_command_reports_queue_full_when_ring_is_full() {
        let (mut tx, _rx) = RingBuffer::new(1);
        assert!(push_command(&mut tx, Command::Play).is_ok());
        assert!(matches!(
            push_command(&mut tx, Command::Pause),
            Err(EngineError::QueueFull)
        ));
    }

    #[test]
    fn next_step_without_loop_renders_whole_block() {
        let step = next_step(100, 64, None);
        assert_eq!(
            step,
            Step {
                from: 100,
                frames: 64,
                next: 164
            }
        );
    }

    #[test]
    fn next_step_without_loop_saturates_at_u64_max() {
        assert_eq!(next_step(u64::MAX - 1, 10, None).next, u64::MAX);
    }

    #[test]
    fn next_step_zero_remaining_renders_nothing() {
        assert_eq!(next_step(10, 0, Some((0, 100))).frames, 0);
    }

    #[test]
    fn next_step_inside_loop_renders_whole_block() {
        assert_eq!(
            next_step(10, 32, Some((0, 100))),
            Step {
                from: 10,
                frames: 32,
                next: 42
            }
        );
    }

    #[test]
    fn next_step_block_exactly_reaching_loop_end_wraps_to_start() {
        assert_eq!(
            next_step(90, 10, Some((0, 100))),
            Step {
                from: 90,
                frames: 10,
                next: 0
            }
        );
    }

    #[test]
    fn next_step_block_crossing_loop_end_splits_at_end() {
        assert_eq!(
            next_step(95, 10, Some((0, 100))),
            Step {
                from: 95,
                frames: 5,
                next: 0
            }
        );
        assert_eq!(steps(95, 10, Some((0, 100))), vec![(95, 5), (0, 5)]);
    }

    #[test]
    fn loop_shorter_than_block_wraps_repeatedly_within_one_block() {
        assert_eq!(
            steps(15, 30, Some((10, 20))),
            vec![(15, 5), (10, 10), (10, 10), (10, 5)]
        );
    }

    #[test]
    fn position_past_loop_end_jumps_to_loop_start() {
        assert_eq!(
            next_step(150, 10, Some((0, 100))),
            Step {
                from: 0,
                frames: 10,
                next: 10
            }
        );
    }

    #[test]
    fn position_exactly_at_loop_end_jumps_to_loop_start() {
        assert_eq!(
            next_step(100, 5, Some((0, 100))),
            Step {
                from: 0,
                frames: 5,
                next: 5
            }
        );
    }

    #[test]
    fn position_before_loop_start_plays_until_loop_end() {
        assert_eq!(
            next_step(10, 200, Some((50, 100))),
            Step {
                from: 10,
                frames: 90,
                next: 50
            }
        );
    }

    #[test]
    fn invalid_loop_region_is_treated_as_no_loop() {
        assert_eq!(
            next_step(10, 5, Some((20, 20))),
            Step {
                from: 10,
                frames: 5,
                next: 15
            }
        );
    }

    #[test]
    fn loop_segments_cover_each_block_exactly_and_never_cross_loop_end() {
        let regions = [Some((5, 9)), Some((0, 7)), Some((3, 100)), None];
        for region in regions {
            for position in 0..50 {
                for remaining in 1..60 {
                    let segments = steps(position, remaining, region);
                    let total: usize = segments.iter().map(|(_, frames)| frames).sum();
                    assert_eq!(total, remaining, "region {region:?} at {position}");
                    if let Some((_, end)) = region {
                        for (from, frames) in segments {
                            assert!(from < end, "segment starts before loop end");
                            assert!(
                                from + frames as u64 <= end,
                                "segment must not cross loop end"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn meter_clock_fires_once_per_interval() {
        let mut clock = MeterClock::default();
        assert!(!clock.advance(METER_INTERVAL_FRAMES - 1));
        assert!(clock.advance(1));
        assert!(!clock.advance(512));
        assert!(clock.advance(512));
    }

    #[test]
    fn request_swaps_graph_and_retires_previous_one() {
        let first = graph(1);
        let second = graph(2);
        let (retired_tx, mut retired_rx) = RingBuffer::new(4);
        let mut slot = GraphSlot::new(Arc::clone(&first), retired_tx);

        slot.request(Arc::clone(&second));

        assert!(Arc::ptr_eq(&slot.current, &second));
        assert!(!slot.has_pending());
        let retired = retired_rx.pop().unwrap();
        assert!(Arc::ptr_eq(&retired, &first));
    }

    #[test]
    fn full_retired_ring_defers_swap_and_keeps_both_graphs_alive() {
        let first = graph(1);
        let second = graph(2);
        let third = graph(3);
        let (retired_tx, mut retired_rx) = RingBuffer::new(1);
        let mut slot = GraphSlot::new(Arc::clone(&first), retired_tx);
        slot.request(Arc::clone(&second));

        slot.request(Arc::clone(&third));

        assert!(
            Arc::ptr_eq(&slot.current, &second),
            "old graph keeps rendering"
        );
        assert!(slot.has_pending());
        assert_eq!(
            Arc::strong_count(&third),
            2,
            "pending graph is held, not dropped"
        );
        slot.flush_pending();
        assert!(slot.has_pending(), "still full, so still deferred");

        let _ = retired_rx.pop().unwrap();
        slot.flush_pending();

        assert!(Arc::ptr_eq(&slot.current, &third));
        assert!(!slot.has_pending());
        let retired = retired_rx.pop().unwrap();
        assert!(Arc::ptr_eq(&retired, &second));
    }

    #[test]
    fn interleaved_stereo_maps_left_right_and_silences_extra_channels() {
        let mut out = [9.0f32; 8];

        write_interleaved(&mut out, 4, &[0.5, -0.25], &[0.1, 0.2]);

        assert_eq!(out, [0.5, 0.1, 0.0, 0.0, -0.25, 0.2, 0.0, 0.0]);
    }

    #[test]
    fn mono_device_receives_average_of_left_and_right() {
        let mut out = [0.0f32; 2];

        write_interleaved(&mut out, 1, &[1.0, 0.5], &[0.0, 0.5]);

        assert_eq!(out, [0.5, 0.5]);
    }

    #[test]
    fn device_output_is_clamped_and_nan_becomes_silence() {
        let mut out = [0.0f32; 6];

        write_interleaved(&mut out, 2, &[3.0, -3.0, f32::NAN], &[0.0, 0.0, 0.0]);

        assert_eq!(out, [1.0, 0.0, -1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn integer_output_keeps_sign_and_silence_maps_to_equilibrium() {
        let mut out = [0i16; 4];
        write_interleaved(&mut out, 2, &[1.0, -1.0], &[0.0, 0.0]);
        assert!(out[0] > 0);
        assert!(out[2] < 0);
        assert_eq!(out[1], 0);

        let mut unsigned = [0u16; 2];
        fill_silence(&mut unsigned);
        assert_eq!(unsigned, [u16::EQUILIBRIUM; 2]);
    }

    #[test]
    fn validate_loop_error_names_both_bounds() {
        let message = validate_loop(Some((9, 5))).unwrap_err().to_string();
        assert!(message.contains('9') && message.contains('5'), "{message}");
    }

    /// Needs a real output device, so it is ignored by default. Run with
    /// `cargo test -p engine realtime::tests::plays_on_default_device -- --ignored`.
    #[test]
    #[ignore = "needs a real audio output device"]
    fn plays_on_default_device() {
        let mut engine = RealtimeEngine::start().expect("default output device");
        assert!(engine.sample_rate() > 0);
        assert!(!engine.device_name().is_empty());

        engine.play().unwrap();
        std::thread::sleep(Duration::from_millis(300));

        let status = engine.status();
        assert!(status.playing);
        assert!(status.position > 0, "position should advance while playing");
        drop(engine);
    }
}
