//! IPC commands. Names/arguments are the contract with `ui/src/lib/api.ts`
//! (Tauri maps camelCase JS argument keys to these snake_case parameters).
//!
//! After any project change: rebuild the graph (`engine::build_graph`) and push it to the
//! real-time engine, then return a fresh `AppSnapshot`.

use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use engine::bounce::{BounceOptions, BounceReport, bounce_to_wav};
use engine::decode::decode_file;
use engine::model::{MAX_TRACKS, SourceId};
use engine::peaks::{Peaks, build_peaks};
use engine::project_io;
use engine::realtime::RealtimeEngine;
use engine::{
    AudioBuffer, Edit, EditOutcome, EngineError, Project, SourcePool, SourceRef,
    apply_edit as apply_engine_edit,
};
use tauri::{AppHandle, Emitter, State};

use crate::dto::{AppSnapshot, EVENT_EXPORT_PROGRESS, ExportProgressEvent};
use crate::state::{AppState, Document, FALLBACK_SAMPLE_RATE, PeakMap, Session, lock_recovering};

type CmdResult<T> = Result<T, String>;

const NO_DEVICE: &str = "No audio output device";
/// Upper bound for one waveform request; a screen never needs more columns than this.
const MAX_WAVEFORM_COLUMNS: usize = 16_384;
/// Export progress is pushed at most every 50 ms (about 20 events per second).
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(50);

/// First call from the UI: starts the audio engine if not started (failure is non-fatal →
/// `device_name: None` + a warning), creates an empty project at the device rate
/// (48000 Hz without a device) and returns the snapshot.
#[tauri::command]
pub fn init(state: State<'_, AppState>) -> CmdResult<AppSnapshot> {
    let mut guard = state.lock_session();
    let s = &mut *guard;
    if state.first_init() {
        match RealtimeEngine::start() {
            Ok(engine) => *state.lock_engine() = Some(engine),
            Err(err) => s.warnings.push(device_warning(&err.to_string())),
        }
        let rate = state.device_rate().unwrap_or(FALLBACK_SAMPLE_RATE);
        s.replace_document(Document::blank(rate));
        state.rebuild_graph(s)?;
    }
    Ok(state.snapshot_of(s))
}

#[tauri::command]
pub fn new_project(state: State<'_, AppState>) -> CmdResult<AppSnapshot> {
    let mut guard = state.lock_session();
    let s = &mut *guard;
    let rate = state.device_rate().unwrap_or(FALLBACK_SAMPLE_RATE);
    install_document(&state, s, Document::blank(rate))
}

/// Decodes files in parallel (one thread per file, capped at available_parallelism), builds
/// peaks, then — as ONE undo step — adds per file: source, a new track named after the file
/// stem, and a clip at frame 0. Files that fail are skipped and reported in `warnings`.
#[tauri::command]
pub async fn import_files(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> CmdResult<AppSnapshot> {
    let rate = state.lock_session().project.sample_rate;
    let files: Vec<PathBuf> = paths.iter().map(absolute_path).collect();
    let decoded = tauri::async_runtime::spawn_blocking(move || decode_all(files, rate))
        .await
        .map_err(msg)?;

    let mut guard = state.lock_session();
    let s = &mut *guard;
    let mut ready = Vec::new();
    for (path, result) in decoded {
        let name = track_name_for(&path);
        match result {
            Ok(loaded) => ready.push(ImportedFile {
                source: source_ref(name, &path, &loaded),
                loaded,
            }),
            Err(reason) => s.warnings.push(file_warning(&name, &reason)),
        }
    }
    if ready.is_empty() {
        return Ok(state.snapshot_of(s));
    }
    if s.project.sample_rate != rate {
        s.warnings
            .push("The project changed while files were loading; nothing was imported".into());
        return Ok(state.snapshot_of(s));
    }
    let room = MAX_TRACKS.saturating_sub(s.project.tracks.len());
    let overflow = ready.split_off(room.min(ready.len()));
    for file in &overflow {
        let reason = format!("the {MAX_TRACKS}-track limit is reached");
        s.warnings.push(file_warning(&file.source.name, &reason));
    }
    if ready.is_empty() {
        return Ok(state.snapshot_of(s));
    }

    let rejected = {
        let Session {
            project,
            history,
            pool,
            peaks,
            ..
        } = &mut *s;
        history.apply_group(project, |p| Ok(add_imported(p, pool, peaks, ready)))
    }
    .map_err(msg)?;
    s.warnings.extend(rejected);
    state.finish_change(s)
}

#[tauri::command]
pub fn apply_edit(state: State<'_, AppState>, edit: Edit) -> CmdResult<AppSnapshot> {
    // A source without decoded audio would play silence, so sources only enter via import.
    if matches!(edit, Edit::AddSource { .. }) {
        return Err("Sources are added by importing files".to_owned());
    }
    let mut guard = state.lock_session();
    let s = &mut *guard;
    s.history.apply(&mut s.project, &edit).map_err(msg)?;
    state.finish_change(s)
}

/// Ends a continuous gesture (fader/knob release, drag end): breaks undo coalescing.
#[tauri::command]
pub fn end_gesture(state: State<'_, AppState>) -> CmdResult<()> {
    state.lock_session().history.break_coalescing();
    Ok(())
}

#[tauri::command]
pub fn undo(state: State<'_, AppState>) -> CmdResult<AppSnapshot> {
    let mut guard = state.lock_session();
    let s = &mut *guard;
    if s.history.undo(&mut s.project) {
        state.finish_change(s)
    } else {
        Ok(state.snapshot_of(s))
    }
}

#[tauri::command]
pub fn redo(state: State<'_, AppState>) -> CmdResult<AppSnapshot> {
    let mut guard = state.lock_session();
    let s = &mut *guard;
    if s.history.redo(&mut s.project) {
        state.finish_change(s)
    } else {
        Ok(state.snapshot_of(s))
    }
}

#[tauri::command]
pub fn transport_play(state: State<'_, AppState>) -> CmdResult<()> {
    let mut guard = state.lock_engine();
    let engine = guard.as_mut().ok_or_else(|| NO_DEVICE.to_owned())?;
    engine.play().map_err(msg)
}

#[tauri::command]
pub fn transport_pause(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(engine) = state.lock_engine().as_mut() {
        engine.pause().map_err(msg)?;
    }
    Ok(())
}

#[tauri::command]
pub fn transport_stop(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(engine) = state.lock_engine().as_mut() {
        engine.stop().map_err(msg)?;
    }
    Ok(())
}

#[tauri::command]
pub fn transport_seek(state: State<'_, AppState>, frame: u64) -> CmdResult<()> {
    if let Some(engine) = state.lock_engine().as_mut() {
        engine.seek(frame).map_err(msg)?;
    }
    Ok(())
}

/// `start`/`end` both `None` disables looping; otherwise both required, start < end.
#[tauri::command]
pub fn transport_set_loop(
    state: State<'_, AppState>,
    start: Option<u64>,
    end: Option<u64>,
) -> CmdResult<()> {
    let region = validate_loop(start, end)?;
    let mut session = state.lock_session();
    if let Some(engine) = state.lock_engine().as_mut() {
        engine.set_loop(region).map_err(msg)?;
    }
    session.loop_region = region;
    Ok(())
}

/// `[min, max]` per column for source frames `start..end` (see `engine::peaks::Peaks::query`).
#[tauri::command]
pub fn get_waveform(
    state: State<'_, AppState>,
    source_id: u64,
    start: u64,
    end: u64,
    columns: usize,
) -> CmdResult<Vec<[f32; 2]>> {
    validate_waveform_request(start, end, columns)?;
    let (peaks, buffer) = {
        let s = state.lock_session();
        let peaks = s
            .peaks
            .get(&source_id)
            .cloned()
            .ok_or_else(|| format!("Unknown source {source_id}"))?;
        let buffer = s
            .pool
            .get(&source_id)
            .cloned()
            .ok_or_else(|| format!("Source {source_id} has no audio data"))?;
        (peaks, buffer)
    };
    Ok(peaks.query(&buffer, start, end, columns))
}

/// `path: None` saves to the current project path (error if there is none).
/// Appends ".msproj" if missing. Clears `dirty`.
#[tauri::command]
pub fn save_project(state: State<'_, AppState>, path: Option<String>) -> CmdResult<AppSnapshot> {
    let (project, target, revision) = {
        let s = state.lock_session();
        let target = match path {
            Some(p) => PathBuf::from(p),
            None => s
                .project_path
                .clone()
                .ok_or_else(|| "The project has no file yet; choose where to save".to_owned())?,
        };
        (
            s.project.clone(),
            with_project_extension(&target),
            s.revision,
        )
    };
    // Written without the lock so UI commands keep flowing; edits made meanwhile keep `dirty`.
    project_io::save_project(&project, &target).map_err(msg)?;

    let mut guard = state.lock_session();
    let s = &mut *guard;
    s.project_path = Some(target);
    if s.revision == revision {
        s.dirty = false;
    }
    Ok(state.snapshot_of(s))
}

/// Loads, converts to the device rate with `Project::with_sample_rate` if needed (warning),
/// decodes all present sources in parallel, rebuilds peaks, resets history, stops transport.
#[tauri::command]
pub async fn open_project(state: State<'_, AppState>, path: String) -> CmdResult<AppSnapshot> {
    let path = absolute_path(&path);
    let target = {
        let s = state.lock_session();
        state.device_rate().unwrap_or(s.project.sample_rate)
    };
    let worker_path = path.clone();
    let opened = tauri::async_runtime::spawn_blocking(move || open_document(&worker_path, target))
        .await
        .map_err(msg)??;

    let mut guard = state.lock_session();
    let s = &mut *guard;
    s.warnings.extend(opened.warnings);
    let doc = Document {
        project: opened.project,
        pool: opened.pool,
        peaks: opened.peaks,
        path: Some(path),
        // A converted project differs from the file on disk, so it counts as unsaved.
        dirty: opened.converted,
        missing: opened.missing,
    };
    install_document(&state, s, doc)
}

/// Bounces on a blocking worker thread, emitting `export-progress`. Does not hold the state
/// lock while rendering (clones the `Arc<RenderGraph>` first).
#[tauri::command]
pub async fn export_wav(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    options: BounceOptions,
) -> CmdResult<BounceReport> {
    let graph = Arc::clone(&state.lock_session().graph);
    tauri::async_runtime::spawn_blocking(move || {
        let mut throttle = ProgressThrottle::default();
        bounce_to_wav(&graph, &options, Path::new(&path), &mut |fraction: f32| {
            if throttle.should_emit(fraction, Instant::now()) {
                let _ = app.emit(EVENT_EXPORT_PROGRESS, ExportProgressEvent { fraction });
            }
            true
        })
    })
    .await
    .map_err(msg)?
    .map_err(msg)
}

/// Replaces the open document: stops playback (and any loop), swaps the document, rebuilds the
/// graph and returns the snapshot. Shared by `new_project` and `open_project`.
fn install_document(state: &AppState, s: &mut Session, doc: Document) -> CmdResult<AppSnapshot> {
    let had_loop = s.loop_region.take().is_some();
    state.reset_transport(had_loop)?;
    s.replace_document(doc);
    state.rebuild_graph(s)?;
    Ok(state.snapshot_of(s))
}

// ---------------------------------------------------------------------------------------------
// Import / open helpers (run outside the session lock)
// ---------------------------------------------------------------------------------------------

/// A decoded file, resampled to the project rate, with its waveform overview.
struct Loaded {
    buffer: AudioBuffer,
    peaks: Peaks,
    original_sample_rate: u32,
}

struct ImportedFile {
    source: SourceRef,
    loaded: Loaded,
}

struct OpenedDocument {
    project: Project,
    pool: SourcePool,
    peaks: PeakMap,
    missing: Vec<SourceId>,
    warnings: Vec<String>,
    converted: bool,
}

fn decode_all(paths: Vec<PathBuf>, rate: u32) -> Vec<(PathBuf, CmdResult<Loaded>)> {
    let results = parallel_map(&paths, |p| load_audio_guarded(p, rate));
    paths.into_iter().zip(results).collect()
}

fn load_audio_guarded(path: &Path, rate: u32) -> CmdResult<Loaded> {
    guarded(|| load_audio(path, rate))
}

fn load_audio(path: &Path, rate: u32) -> CmdResult<Loaded> {
    let decoded = decode_file(path, rate).map_err(msg)?;
    let peaks = build_peaks(&decoded.buffer);
    Ok(Loaded {
        buffer: decoded.buffer,
        peaks,
        original_sample_rate: decoded.original_sample_rate,
    })
}

fn source_ref(name: String, path: &Path, loaded: &Loaded) -> SourceRef {
    SourceRef {
        // Ignored by `Edit::AddSource`, which allocates the real id.
        id: 0,
        name,
        path: path.to_string_lossy().into_owned(),
        channels: loaded.buffer.channel_count() as u16,
        frames: loaded.buffer.frames() as u64,
        original_sample_rate: loaded.original_sample_rate,
    }
}

/// Adds every file as source + track + clip in the project, then records their audio.
/// Runs inside `History::apply_group`, so the whole batch is one undo step. Each file is
/// rolled back on its own if it fails, so one bad file cannot leave a half-added track.
fn add_imported(
    project: &mut Project,
    pool: &mut SourcePool,
    peaks: &mut PeakMap,
    files: Vec<ImportedFile>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for file in files {
        let before = project.clone();
        match add_source_track(project, &file.source) {
            Ok(source_id) => {
                let Loaded {
                    buffer,
                    peaks: file_peaks,
                    ..
                } = file.loaded;
                pool.insert(source_id, Arc::new(buffer));
                peaks.insert(source_id, Arc::new(file_peaks));
            }
            Err(err) => {
                *project = before;
                warnings.push(file_warning(&file.source.name, &err.to_string()));
            }
        }
    }
    warnings
}

fn add_source_track(project: &mut Project, source: &SourceRef) -> engine::Result<SourceId> {
    let source_id = created_id(apply_engine_edit(
        project,
        &Edit::AddSource {
            source: source.clone(),
        },
    )?)?;
    let track = created_id(apply_engine_edit(
        project,
        &Edit::AddTrack {
            name: source.name.clone(),
            index: None,
        },
    )?)?;
    apply_engine_edit(
        project,
        &Edit::AddClip {
            track,
            source: source_id,
            start: 0,
        },
    )?;
    Ok(source_id)
}

fn created_id(outcome: EditOutcome) -> engine::Result<u64> {
    outcome
        .created
        .ok_or_else(|| EngineError::InvalidEdit("edit did not allocate an id".to_owned()))
}

/// Loads a project file, converts it to `target_rate` if needed, and decodes its sources in
/// parallel. Sources that are absent or fail to decode end up in `missing`.
fn open_document(path: &Path, target_rate: u32) -> CmdResult<OpenedDocument> {
    let mut project = project_io::load_project(path).map_err(msg)?;
    let mut warnings = Vec::new();
    let converted = project.sample_rate != target_rate;
    if converted {
        warnings.push(conversion_warning(project.sample_rate, target_rate));
        project = project.with_sample_rate(target_rate);
    }

    let absent = project_io::missing_sources(&project);
    let (pool, peaks, failed) = {
        let present: Vec<&SourceRef> = project
            .sources
            .iter()
            .filter(|source| !absent.contains(&source.id))
            .collect();
        let paths: Vec<PathBuf> = present.iter().map(|s| PathBuf::from(&s.path)).collect();
        let results = parallel_map(&paths, |p| load_audio_guarded(p, target_rate));

        let mut pool = SourcePool::new();
        let mut peaks = PeakMap::new();
        let mut failed = Vec::new();
        for (source, result) in present.iter().zip(results) {
            match result {
                Ok(loaded) => {
                    pool.insert(source.id, Arc::new(loaded.buffer));
                    peaks.insert(source.id, Arc::new(loaded.peaks));
                }
                Err(reason) => {
                    failed.push(source.id);
                    warnings.push(file_warning(&source.name, &reason));
                }
            }
        }
        (pool, peaks, failed)
    };

    let missing = project
        .sources
        .iter()
        .map(|source| source.id)
        .filter(|id| absent.contains(id) || failed.contains(id))
        .collect();
    Ok(OpenedDocument {
        project,
        pool,
        peaks,
        missing,
        warnings,
        converted,
    })
}

// ---------------------------------------------------------------------------------------------
// Pure helpers (unit-tested)
// ---------------------------------------------------------------------------------------------

fn msg(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn absolute_path(raw: impl AsRef<Path>) -> PathBuf {
    let path = raw.as_ref().to_path_buf();
    std::path::absolute(&path).unwrap_or(path)
}

/// Appends `.msproj` unless the name already ends with it (case-insensitive). The existing
/// extension is kept, so `take.v2` becomes `take.v2.msproj` rather than `take.msproj`.
fn with_project_extension(path: &Path) -> PathBuf {
    let ext = project_io::PROJECT_EXTENSION;
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(ext);
    PathBuf::from(name)
}

/// Track and display name for an imported file: its stem, or "Track" when nothing is left.
fn track_name_for(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().trim().to_owned())
        .unwrap_or_default();
    if stem.is_empty() {
        "Track".to_owned()
    } else {
        stem
    }
}

/// `start` and `end` must be both cleared or both set with start < end.
fn validate_loop(start: Option<u64>, end: Option<u64>) -> CmdResult<Option<(u64, u64)>> {
    match (start, end) {
        (None, None) => Ok(None),
        (Some(s), Some(e)) if s < e => Ok(Some((s, e))),
        (Some(_), Some(_)) => Err("Loop end must be after loop start".to_owned()),
        _ => Err("Loop start and end must both be set or both be cleared".to_owned()),
    }
}

fn validate_waveform_request(start: u64, end: u64, columns: usize) -> CmdResult<()> {
    if end < start {
        return Err("Waveform range ends before it starts".to_owned());
    }
    if columns > MAX_WAVEFORM_COLUMNS {
        return Err(format!(
            "Too many waveform columns (limit {MAX_WAVEFORM_COLUMNS})"
        ));
    }
    Ok(())
}

fn file_warning(name: &str, reason: &str) -> String {
    format!("{name}: {reason}")
}

fn conversion_warning(from: u32, to: u32) -> String {
    format!("Project converted from {from} Hz to {to} Hz")
}

fn device_warning(reason: &str) -> String {
    format!("{NO_DEVICE} ({reason}). Editing and export still work.")
}

/// Decides whether a progress update is sent: at most one per interval, but the final
/// fraction (1.0) always goes out so the UI can finish its bar.
#[derive(Default)]
struct ProgressThrottle {
    last_sent: Option<Instant>,
}

impl ProgressThrottle {
    fn should_emit(&mut self, fraction: f32, now: Instant) -> bool {
        let due = self
            .last_sent
            .is_none_or(|sent| now.duration_since(sent) >= PROGRESS_MIN_INTERVAL);
        if due || fraction >= 1.0 {
            self.last_sent = Some(now);
            true
        } else {
            false
        }
    }
}

/// Runs `f` for every item on up to `available_parallelism` scoped threads. Results keep the
/// input order. Workers pull indices from a shared counter, so uneven file sizes balance out.
fn parallel_map<T, R>(items: &[T], f: impl Fn(&T) -> CmdResult<R> + Sync) -> Vec<CmdResult<R>>
where
    T: Sync,
    R: Send,
{
    let workers = thread::available_parallelism()
        .map_or(1, NonZeroUsize::get)
        .min(items.len());
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<CmdResult<R>>>> = items.iter().map(|_| Mutex::new(None)).collect();
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(index) else {
                        break;
                    };
                    if let Some(slot) = slots.get(index) {
                        *lock_recovering(slot) = Some(f(item));
                    }
                }
            });
        }
    });
    slots
        .into_iter()
        .map(|slot| {
            lock_recovering(&slot)
                .take()
                .unwrap_or_else(|| Err("internal error: decode worker did not finish".to_owned()))
        })
        .collect()
}

/// Turns a panic inside a decoder into an ordinary per-file error.
fn guarded<R>(f: impl FnOnce() -> CmdResult<R>) -> CmdResult<R> {
    catch_unwind(AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err("the decoder failed unexpectedly".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_extension_is_appended_when_missing() {
        assert_eq!(
            with_project_extension(Path::new("song")),
            PathBuf::from("song.msproj")
        );
    }

    #[test]
    fn project_extension_keeps_existing_msproj_case_insensitively() {
        assert_eq!(
            with_project_extension(Path::new("song.msproj")),
            PathBuf::from("song.msproj")
        );
        assert_eq!(
            with_project_extension(Path::new("song.MsProj")),
            PathBuf::from("song.MsProj")
        );
    }

    #[test]
    fn project_extension_keeps_dotted_names_intact() {
        assert_eq!(
            with_project_extension(Path::new("take.v2")),
            PathBuf::from("take.v2.msproj")
        );
    }

    #[test]
    fn track_name_is_the_file_stem() {
        assert_eq!(track_name_for(Path::new("C:/music/kick.wav")), "kick");
        assert_eq!(track_name_for(Path::new("bass.line.flac")), "bass.line");
    }

    #[test]
    fn track_name_handles_odd_names() {
        assert_eq!(track_name_for(Path::new("drums")), "drums");
        assert_eq!(track_name_for(Path::new(".wav")), ".wav");
        assert_eq!(track_name_for(Path::new("kick .wav")), "kick");
        assert_eq!(track_name_for(Path::new("鼓 組.wav")), "鼓 組");
        assert_eq!(track_name_for(Path::new("   .wav")), "Track");
        assert_eq!(track_name_for(Path::new("")), "Track");
    }

    #[test]
    fn loop_region_accepts_both_none_or_ordered_pair() {
        assert_eq!(validate_loop(None, None), Ok(None));
        assert_eq!(validate_loop(Some(0), Some(10)), Ok(Some((0, 10))));
    }

    #[test]
    fn loop_region_rejects_empty_reversed_and_half_set_pairs() {
        assert!(validate_loop(Some(5), Some(5)).is_err());
        assert!(validate_loop(Some(9), Some(3)).is_err());
        assert!(validate_loop(Some(1), None).is_err());
        assert!(validate_loop(None, Some(1)).is_err());
    }

    #[test]
    fn waveform_request_rejects_reversed_range_and_huge_column_count() {
        assert!(validate_waveform_request(10, 5, 100).is_err());
        assert!(validate_waveform_request(0, 10, MAX_WAVEFORM_COLUMNS + 1).is_err());
        assert!(validate_waveform_request(0, 0, 0).is_ok());
        assert!(validate_waveform_request(0, 10, MAX_WAVEFORM_COLUMNS).is_ok());
    }

    #[test]
    fn warnings_read_as_user_facing_sentences() {
        assert_eq!(
            file_warning("kick", "cannot decode x"),
            "kick: cannot decode x"
        );
        assert_eq!(
            conversion_warning(44_100, 48_000),
            "Project converted from 44100 Hz to 48000 Hz"
        );
        assert!(device_warning("busy").starts_with("No audio output device (busy)"));
    }

    #[test]
    fn progress_throttle_sends_first_then_waits_for_interval() {
        let mut throttle = ProgressThrottle::default();
        let t0 = Instant::now();

        assert!(throttle.should_emit(0.1, t0));
        assert!(!throttle.should_emit(0.2, t0 + Duration::from_millis(10)));
        assert!(throttle.should_emit(0.3, t0 + PROGRESS_MIN_INTERVAL));
    }

    #[test]
    fn progress_throttle_always_sends_completion() {
        let mut throttle = ProgressThrottle::default();
        let t0 = Instant::now();
        assert!(throttle.should_emit(0.5, t0));

        assert!(throttle.should_emit(1.0, t0 + Duration::from_millis(1)));
    }

    #[test]
    fn parallel_map_preserves_input_order() {
        let items: Vec<u32> = (0..200).collect();

        let results = parallel_map(&items, |n| Ok(n * 2));

        let values: Vec<u32> = results.into_iter().map(|r| r.unwrap_or(0)).collect();
        assert_eq!(values, items.iter().map(|n| n * 2).collect::<Vec<_>>());
    }

    #[test]
    fn parallel_map_reports_errors_per_item_and_handles_empty_input() {
        let items = ["ok", "bad", "ok"];
        let results = parallel_map(&items, |s| {
            if *s == "bad" {
                Err("broken".to_owned())
            } else {
                Ok(s.len())
            }
        });
        assert_eq!(results[0], Ok(2));
        assert_eq!(results[1], Err("broken".to_owned()));
        assert_eq!(results[2], Ok(2));

        let empty: Vec<CmdResult<u8>> = parallel_map(&[] as &[u8], |_| Ok(0));
        assert!(empty.is_empty());
    }

    #[test]
    fn guarded_turns_a_panic_into_an_error() {
        let result: CmdResult<()> = guarded(|| panic!("decoder bug"));

        assert_eq!(result, Err("the decoder failed unexpectedly".to_owned()));
    }
}
