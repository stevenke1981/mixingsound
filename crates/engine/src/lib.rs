//! MixingSound audio engine.
//!
//! Layering (lower modules never depend on higher ones):
//!
//! ```text
//! model / edit / history / project_io   — pure data, no audio (UI-thread side)
//! buffer / decode / peaks               — audio data loading & analysis
//! graph                                 — immutable RenderGraph snapshot built from Project + SourcePool
//! dsp / render                          — real-time-safe mixing (shared by realtime & bounce)
//! realtime / bounce                     — drivers: cpal device stream, offline WAV export
//! ```
//!
//! Real-time rule: everything reachable from `render::Mixer::render` must not allocate,
//! lock, do I/O or log.

pub mod bounce;
pub mod buffer;
pub mod decode;
pub mod dsp;
pub mod edit;
pub mod error;
pub mod graph;
pub mod history;
pub mod model;
pub mod peaks;
pub mod project_io;
pub mod realtime;
pub mod render;

pub use buffer::AudioBuffer;
pub use edit::{Edit, EditOutcome, apply_edit};
pub use error::{EngineError, Result};
pub use graph::{RenderGraph, SourcePool, build_graph};
pub use history::History;
pub use model::{Clip, ClipId, MasterBus, Project, SourceId, SourceRef, Track, TrackId};
