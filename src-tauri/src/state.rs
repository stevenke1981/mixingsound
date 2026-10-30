//! Shared app state: the open document (`Session`) and the real-time engine, behind two mutexes.
//!
//! Lock order: `session` before `engine`. Neither lock is held across decoding, bouncing or
//! `.await`; heavy work runs on `Arc` clones taken under the lock. Poisoned locks are recovered
//! (`into_inner`) because a panic in one command must not wedge every later command.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use engine::model::SourceId;
use engine::peaks::Peaks;
use engine::realtime::RealtimeEngine;
use engine::{History, Project, RenderGraph, SourcePool, build_graph};

use crate::dto::AppSnapshot;

/// Sample rate for new projects when no output device could be opened.
pub const FALLBACK_SAMPLE_RATE: u32 = 48_000;

pub type PeakMap = HashMap<SourceId, Arc<Peaks>>;

/// Everything that describes the open document. Guarded by `AppState::session`.
pub struct Session {
    pub project: Project,
    pub history: History,
    /// Decoded audio, keyed by `SourceRef::id`. Never pruned in P0/P1, so redo keeps working.
    pub pool: SourcePool,
    pub peaks: PeakMap,
    /// Last graph handed to the engine; export clones this instead of rebuilding.
    pub graph: Arc<RenderGraph>,
    pub project_path: Option<PathBuf>,
    pub dirty: bool,
    /// Bumped on every document change; lets `save_project` detect edits made while writing.
    pub revision: u64,
    pub missing_sources: Vec<SourceId>,
    /// Notices waiting for the next snapshot (each is shown once).
    pub warnings: Vec<String>,
    /// Loop region mirrored to the engine; cleared whenever the document is replaced.
    pub loop_region: Option<(u64, u64)>,
}

/// A complete document ready to become the session's current one.
pub struct Document {
    pub project: Project,
    pub pool: SourcePool,
    pub peaks: PeakMap,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub missing: Vec<SourceId>,
}

impl Document {
    pub fn blank(sample_rate: u32) -> Self {
        Self {
            project: Project::new(sample_rate),
            pool: SourcePool::new(),
            peaks: PeakMap::new(),
            path: None,
            dirty: false,
            missing: Vec::new(),
        }
    }
}

impl Session {
    pub fn new(sample_rate: u32) -> Self {
        let doc = Document::blank(sample_rate);
        Self {
            project: doc.project,
            pool: doc.pool,
            peaks: doc.peaks,
            graph: Arc::new(RenderGraph::empty(sample_rate)),
            history: History::default(),
            project_path: doc.path,
            dirty: doc.dirty,
            revision: 0,
            missing_sources: doc.missing,
            warnings: Vec::new(),
            loop_region: None,
        }
    }

    /// Swaps in a loaded document and drops the undo history of the previous one.
    /// The caller must rebuild the graph afterwards.
    pub fn replace_document(&mut self, doc: Document) {
        self.project = doc.project;
        self.pool = doc.pool;
        self.peaks = doc.peaks;
        self.project_path = doc.path;
        self.dirty = doc.dirty;
        self.missing_sources = doc.missing;
        self.history = History::default();
        self.revision += 1;
    }

    pub fn mark_changed(&mut self) {
        self.dirty = true;
        self.revision += 1;
    }
}

pub struct AppState {
    session: Mutex<Session>,
    engine: Mutex<Option<RealtimeEngine>>,
    /// Set by the first `init`. The device is opened once; a failure is a warning, not a retry.
    engine_started: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(Session::new(FALLBACK_SAMPLE_RATE)),
            engine: Mutex::new(None),
            engine_started: AtomicBool::new(false),
        }
    }

    pub fn lock_session(&self) -> MutexGuard<'_, Session> {
        lock_recovering(&self.session)
    }

    pub fn lock_engine(&self) -> MutexGuard<'_, Option<RealtimeEngine>> {
        lock_recovering(&self.engine)
    }

    /// True exactly once: the first `init` call owns device start-up.
    pub fn first_init(&self) -> bool {
        !self.engine_started.swap(true, Ordering::AcqRel)
    }

    pub fn device_name(&self) -> Option<String> {
        self.lock_engine()
            .as_ref()
            .map(|e| e.device_name().to_owned())
    }

    pub fn device_rate(&self) -> Option<u32> {
        self.lock_engine().as_ref().map(RealtimeEngine::sample_rate)
    }

    /// Resolves the document into a graph, keeps it in the session and pushes it to the engine.
    /// Call with the session lock held (session → engine order).
    pub fn rebuild_graph(&self, s: &mut Session) -> Result<(), String> {
        let graph = Arc::new(build_graph(&s.project, &s.pool).map_err(|e| e.to_string())?);
        s.graph = Arc::clone(&graph);
        if let Some(engine) = self.lock_engine().as_mut() {
            engine.set_graph(graph).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn snapshot_of(&self, s: &mut Session) -> AppSnapshot {
        snapshot(s, self.device_name())
    }

    /// Common tail of every edit: mark dirty, rebuild the graph, return the new snapshot.
    pub fn finish_change(&self, s: &mut Session) -> Result<AppSnapshot, String> {
        s.mark_changed();
        self.rebuild_graph(s)?;
        Ok(self.snapshot_of(s))
    }

    /// Pauses and returns to frame 0 (no-op without a device). Optionally clears the engine loop.
    pub fn reset_transport(&self, clear_loop: bool) -> Result<(), String> {
        let mut guard = self.lock_engine();
        let Some(engine) = guard.as_mut() else {
            return Ok(());
        };
        engine.stop().map_err(|e| e.to_string())?;
        if clear_loop {
            engine.set_loop(None).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// Builds the UI payload. Drains pending warnings, so each one is delivered exactly once.
pub fn snapshot(s: &mut Session, device_name: Option<String>) -> AppSnapshot {
    AppSnapshot {
        project: s.project.clone(),
        can_undo: s.history.can_undo(),
        can_redo: s.history.can_redo(),
        project_path: s.project_path.as_deref().map(path_to_string),
        dirty: s.dirty,
        device_name,
        missing_sources: s.missing_sources.clone(),
        warnings: std::mem::take(&mut s.warnings),
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Locks a mutex, recovering the guard if a previous holder panicked.
pub fn lock_recovering<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::model::{Clip, TRACK_COLORS, Track};
    use engine::{AudioBuffer, SourceRef};

    fn session_with_warnings(warnings: &[&str]) -> Session {
        let mut s = Session::new(48_000);
        s.warnings = warnings.iter().map(|w| (*w).to_owned()).collect();
        s
    }

    #[test]
    fn snapshot_drains_warnings_so_each_is_delivered_once() {
        let mut s = session_with_warnings(&["first", "second"]);

        let first = snapshot(&mut s, None);
        let second = snapshot(&mut s, None);

        assert_eq!(first.warnings, ["first", "second"]);
        assert!(second.warnings.is_empty());
    }

    #[test]
    fn snapshot_passes_device_name_and_clean_state_through() {
        let mut s = Session::new(44_100);

        let snap = snapshot(&mut s, Some("Speakers".into()));

        assert_eq!(snap.device_name.as_deref(), Some("Speakers"));
        assert_eq!(snap.project.sample_rate, 44_100);
        assert!(!snap.dirty);
        assert!(snap.project_path.is_none());
        assert!(!snap.can_undo && !snap.can_redo);
    }

    #[test]
    fn mark_changed_sets_dirty_and_bumps_revision() {
        let mut s = Session::new(48_000);
        let before = s.revision;

        s.mark_changed();

        assert!(s.dirty);
        assert_eq!(s.revision, before + 1);
    }

    #[test]
    fn replace_document_clears_dirty_history_and_bumps_revision() {
        let mut s = Session::new(48_000);
        s.mark_changed();
        let before = s.revision;
        let path = PathBuf::from("C:/songs/demo.msproj");

        s.replace_document(Document {
            project: Project::new(44_100),
            pool: SourcePool::new(),
            peaks: PeakMap::new(),
            path: Some(path.clone()),
            dirty: false,
            missing: vec![7],
        });

        assert_eq!(s.project.sample_rate, 44_100);
        assert_eq!(s.project_path, Some(path));
        assert!(!s.dirty);
        assert_eq!(s.missing_sources, [7]);
        assert!(!s.history.can_undo());
        assert!(s.revision > before);
    }

    #[test]
    fn first_init_is_true_only_for_the_first_call() {
        let state = AppState::new();

        assert!(state.first_init());
        assert!(!state.first_init());
        assert!(!state.first_init());
    }

    #[test]
    fn rebuild_graph_without_engine_resolves_only_decoded_sources() {
        let state = AppState::new();
        let mut s = Session::new(48_000);
        s.project.sources.push(SourceRef {
            id: 1,
            name: "kick".into(),
            path: "kick.wav".into(),
            channels: 1,
            frames: 10,
            original_sample_rate: 48_000,
        });
        s.project.tracks.push(Track {
            id: 2,
            name: "kick".into(),
            gain_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            color: TRACK_COLORS[0].into(),
            clips: vec![Clip {
                id: 3,
                source_id: 1,
                start: 0,
                offset: 0,
                length: 10,
                gain_db: 0.0,
                fade_in: 0,
                fade_out: 0,
            }],
        });

        state
            .rebuild_graph(&mut s)
            .expect("graph builds without an engine");
        assert!(
            s.graph.tracks[0].clips.is_empty(),
            "offline source is skipped"
        );

        s.pool
            .insert(1, Arc::new(AudioBuffer::new(vec![vec![0.0; 10]])));
        state
            .rebuild_graph(&mut s)
            .expect("graph builds with the buffer present");
        assert_eq!(s.graph.tracks[0].clips.len(), 1);
        assert_eq!(s.graph.length, 10);
    }

    #[test]
    fn reset_transport_without_engine_is_a_no_op() {
        let state = AppState::new();

        assert_eq!(state.reset_transport(true), Ok(()));
        assert_eq!(state.device_name(), None);
    }

    #[test]
    fn poisoned_session_lock_is_recovered() {
        let state = std::sync::Arc::new(AppState::new());
        let poisoner = std::sync::Arc::clone(&state);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock_session();
            panic!("poison the session mutex");
        })
        .join();

        let guard = state.lock_session();

        assert_eq!(guard.revision, 0);
    }
}
