//! Real-time playback through the default output device (cpal).
//!
//! Threading: the cpal `Stream` is not `Send` on every backend, so `start` spawns a dedicated
//! "audio-host" thread that builds and owns the stream until `RealtimeEngine` is dropped.
//! Control → audio: `rtrb` command ring. Audio → control: meter ring, retired-graph ring
//! (old `Arc<RenderGraph>`s are sent back so they are never freed on the audio thread),
//! and atomics for position / playing.
//! The audio callback never allocates, locks, blocks, does I/O or logs.

use std::sync::Arc;

use serde::Serialize;

use crate::error::Result;
use crate::graph::RenderGraph;
use crate::render::MeterSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportStatus {
    pub playing: bool,
    /// Current timeline frame (start of the next block to render).
    pub position: u64,
}

/// `Send` handle; put it behind a `Mutex` in app state.
pub struct RealtimeEngine {
    // private fields: realtime worker
}

impl RealtimeEngine {
    /// Opens the default output device in its default shared-mode config (stereo f32 when
    /// possible; other sample formats/channel counts are converted in the callback: our stereo
    /// goes to channels 0/1, mono devices get (L+R)/2). Output is clamped to ±1.0.
    /// Starts paused at position 0 with an empty graph.
    pub fn start() -> Result<Self> {
        todo!("realtime worker")
    }

    /// Device sample rate. New projects use this rate.
    pub fn sample_rate(&self) -> u32 {
        todo!("realtime worker")
    }

    pub fn device_name(&self) -> &str {
        todo!("realtime worker")
    }

    /// Swaps the graph the callback renders (takes effect at the next block).
    pub fn set_graph(&mut self, graph: Arc<RenderGraph>) -> Result<()> {
        let _ = graph;
        todo!("realtime worker")
    }

    pub fn play(&mut self) -> Result<()> {
        todo!("realtime worker")
    }

    /// Pauses at the current position.
    pub fn pause(&mut self) -> Result<()> {
        todo!("realtime worker")
    }

    /// Pauses and returns to frame 0.
    pub fn stop(&mut self) -> Result<()> {
        todo!("realtime worker")
    }

    /// Moves the playhead (mixer smoothing is reset to avoid ramps from stale state).
    pub fn seek(&mut self, frame: u64) -> Result<()> {
        let _ = frame;
        todo!("realtime worker")
    }

    /// Loop region `start..end` (end exclusive, start < end), or `None` to disable. While
    /// playing and looping, reaching `end` jumps to `start` sample-accurately inside the block.
    pub fn set_loop(&mut self, region: Option<(u64, u64)>) -> Result<()> {
        let _ = region;
        todo!("realtime worker")
    }

    pub fn status(&self) -> TransportStatus {
        todo!("realtime worker")
    }

    /// Latest meter snapshot since the previous call (drains the ring), or `None`.
    pub fn poll_meters(&mut self) -> Option<MeterSnapshot> {
        todo!("realtime worker")
    }

    /// Last asynchronous device/stream error (e.g. device unplugged), cleared on read.
    pub fn take_error(&mut self) -> Option<String> {
        todo!("realtime worker")
    }

    /// Drops graphs retired by the audio thread. Call regularly (each UI tick) from the
    /// control thread.
    pub fn collect_garbage(&mut self) {
        todo!("realtime worker")
    }
}
