//! Project file (.msproj, pretty JSON) load/save.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{EngineError, Result};
use crate::model::{Clip, MAX_TRACKS, PROJECT_VERSION, Project, SourceId};

pub const PROJECT_EXTENSION: &str = "msproj";

/// Writes `project` as JSON. Source paths under the project file's directory are stored
/// relative (with `/` separators); others stay absolute. Atomic: temp file + rename.
pub fn save_project(project: &Project, path: &Path) -> Result<()> {
    let base = project_dir(path)?;
    let mut stored = project.clone();
    for source in &mut stored.sources {
        source.path = to_stored_path(&source.path, &base);
    }
    let json = serde_json::to_string_pretty(&stored).map_err(format_error)?;
    write_atomic(path, json.as_bytes())
}

/// Reads a project, resolving relative source paths against the file's directory into absolute
/// paths. Rejects unknown `version` > `PROJECT_VERSION` and broken invariants
/// (`ProjectFormat`): duplicate ids, `next_id` not above every id, clips referencing unknown
/// sources, clip geometry/fade invariants, > MAX_TRACKS tracks.
pub fn load_project(path: &Path) -> Result<Project> {
    let text = fs::read_to_string(path).map_err(|e| EngineError::io(path, e))?;
    // The version is read before the full parse so a newer file gets a version error rather
    // than whatever field the newer schema happens to break.
    let header: VersionHeader = serde_json::from_str(&text).map_err(format_error)?;
    check_version(header.version)?;
    let mut project: Project = serde_json::from_str(&text).map_err(format_error)?;

    let base = project_dir(path)?;
    for source in &mut project.sources {
        source.path = resolve_path(&source.path, &base);
    }
    validate(&project)?;
    for track in &mut project.tracks {
        track.clips.sort_by_key(|c| c.start);
    }
    Ok(project)
}

/// Sources whose file does not exist on disk.
pub fn missing_sources(project: &Project) -> Vec<SourceId> {
    project
        .sources
        .iter()
        .filter(|s| !Path::new(&s.path).exists())
        .map(|s| s.id)
        .collect()
}

/// Only the version is needed before the full schema is trusted.
#[derive(Deserialize)]
struct VersionHeader {
    version: u32,
}

fn format_error(err: serde_json::Error) -> EngineError {
    EngineError::ProjectFormat(err.to_string())
}

fn invalid(msg: impl Into<String>) -> EngineError {
    EngineError::ProjectFormat(msg.into())
}

fn check_version(version: u32) -> Result<()> {
    if version == 0 || version > PROJECT_VERSION {
        return Err(invalid(format!(
            "unsupported project version {version} (this build reads up to {PROJECT_VERSION})"
        )));
    }
    Ok(())
}

/// Directory that relative source paths are written against. Made absolute so that a bare
/// file name such as `song.msproj` still gives a comparable base.
fn project_dir(path: &Path) -> Result<PathBuf> {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    std::path::absolute(parent).map_err(|e| EngineError::io(parent, e))
}

/// Stores `source` relative to `base` when it lies inside it; otherwise keeps it unchanged.
fn to_stored_path(source: &str, base: &Path) -> String {
    let src = Path::new(source);
    if !src.is_absolute() {
        return join_with_slashes(src);
    }
    match src.strip_prefix(base) {
        Ok(rel) if !rel.as_os_str().is_empty() => join_with_slashes(rel),
        _ => source.to_string(),
    }
}

/// Inverse of `to_stored_path`: relative entries are resolved against the project directory.
fn resolve_path(stored: &str, base: &Path) -> String {
    let path = Path::new(stored);
    if path.is_absolute() {
        return stored.to_string();
    }
    base.join(path).to_string_lossy().into_owned()
}

/// Uses `/` regardless of platform so a project file moves cleanly between Windows and macOS.
fn join_with_slashes(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

/// Writes to a sibling temp file and renames it over the target, so a crash mid-write never
/// leaves a truncated project behind.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = temp_path(path);
    let result = write_and_sync(&tmp, bytes)
        .and_then(|()| fs::rename(&tmp, path).map_err(|e| EngineError::io(path, e)));
    if result.is_err() {
        // Best-effort cleanup: the write error is the one worth reporting.
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_and_sync(tmp: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = File::create(tmp).map_err(|e| EngineError::io(tmp, e))?;
    file.write_all(bytes).map_err(|e| EngineError::io(tmp, e))?;
    file.sync_all().map_err(|e| EngineError::io(tmp, e))
}

fn validate(project: &Project) -> Result<()> {
    if project.sample_rate == 0 {
        return Err(invalid("sample rate must be positive"));
    }
    if project.tracks.len() > MAX_TRACKS {
        return Err(invalid(format!(
            "project has more than {MAX_TRACKS} tracks"
        )));
    }
    check_finite("master gain", project.master.gain_db)?;
    check_ids(project)?;
    for source in &project.sources {
        check_source(source.channels, source.frames)?;
    }
    for track in &project.tracks {
        check_track(track.gain_db, track.pan)?;
        for clip in &track.clips {
            check_clip(project, clip)?;
        }
    }
    Ok(())
}

fn check_finite(what: &str, value: f32) -> Result<()> {
    if !value.is_finite() {
        return Err(invalid(format!("{what} must be a finite number")));
    }
    Ok(())
}

fn check_ids(project: &Project) -> Result<()> {
    let track_ids = project.tracks.iter().map(|t| t.id);
    let clip_ids = project
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter().map(|c| c.id));
    let source_ids = project.sources.iter().map(|s| s.id);
    let mut seen = HashSet::new();
    for id in track_ids.chain(clip_ids).chain(source_ids) {
        if !seen.insert(id) {
            return Err(invalid(format!("duplicate id {id}")));
        }
        if id >= project.next_id {
            return Err(invalid(format!(
                "nextId {} must be above every id (found {id})",
                project.next_id
            )));
        }
    }
    Ok(())
}

fn check_source(channels: u16, frames: u64) -> Result<()> {
    if !matches!(channels, 1 | 2) {
        return Err(invalid(format!(
            "source has unsupported channel count {channels}"
        )));
    }
    if frames == 0 {
        return Err(invalid("source has no audio frames"));
    }
    Ok(())
}

fn check_track(gain_db: f32, pan: f32) -> Result<()> {
    check_finite("track gain", gain_db)?;
    // Written as a containment test so NaN fails too.
    if !(-1.0..=1.0).contains(&pan) {
        return Err(invalid(format!("track pan {pan} is outside -1..=1")));
    }
    Ok(())
}

fn check_clip(project: &Project, clip: &Clip) -> Result<()> {
    let source = project.source(clip.source_id).ok_or_else(|| {
        invalid(format!(
            "clip {} references unknown source {}",
            clip.id, clip.source_id
        ))
    })?;
    if clip.length == 0 {
        return Err(invalid(format!("clip {} has zero length", clip.id)));
    }
    if clip
        .offset
        .checked_add(clip.length)
        .is_none_or(|end| end > source.frames)
    {
        return Err(invalid(format!(
            "clip {} reads past the end of its source",
            clip.id
        )));
    }
    if clip.start.checked_add(clip.length).is_none() {
        return Err(invalid(format!("clip {} overflows the timeline", clip.id)));
    }
    if clip
        .fade_in
        .checked_add(clip.fade_out)
        .is_none_or(|sum| sum > clip.length)
    {
        return Err(invalid(format!(
            "clip {} fades are longer than the clip",
            clip.id
        )));
    }
    check_finite("clip gain", clip.gain_db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{Edit, apply_edit};
    use crate::model::{SourceRef, Track};
    use tempfile::TempDir;

    /// Valid project: one 1000-frame stereo source `kick.wav` in `dir`, one track with a clip.
    fn base_project(dir: &Path) -> Project {
        let mut p = Project::new(48_000);
        let source = SourceRef {
            id: 0,
            name: "kick".into(),
            path: dir.join("kick.wav").to_string_lossy().into_owned(),
            channels: 2,
            frames: 1_000,
            original_sample_rate: 48_000,
        };
        apply_edit(&mut p, &Edit::AddSource { source }).unwrap();
        let track = apply_edit(
            &mut p,
            &Edit::AddTrack {
                name: "drums".into(),
                index: None,
            },
        )
        .unwrap()
        .created
        .unwrap();
        let clip = apply_edit(
            &mut p,
            &Edit::AddClip {
                track,
                source: 1,
                start: 0,
            },
        )
        .unwrap()
        .created
        .unwrap();
        apply_edit(
            &mut p,
            &Edit::SetClipFades {
                clip,
                fade_in: 10,
                fade_out: 10,
            },
        )
        .unwrap();
        p
    }

    fn first_clip(p: &mut Project) -> &mut Clip {
        &mut p.tracks[0].clips[0]
    }

    /// Saves `p` (save does not validate) and returns the load error, expecting one.
    fn load_error(dir: &TempDir, p: &Project) -> String {
        let file = dir.path().join("broken.msproj");
        save_project(p, &file).unwrap();
        match load_project(&file) {
            Err(EngineError::ProjectFormat(msg)) => msg,
            other => panic!("expected ProjectFormat error, got {other:?}"),
        }
    }

    #[test]
    fn save_then_load_round_trips_project() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("song.msproj");
        let original = base_project(dir.path());

        save_project(&original, &file).unwrap();
        let loaded = load_project(&file).unwrap();

        assert_eq!(loaded, original);
    }

    #[test]
    fn round_trip_keeps_inside_source_relative_and_outside_source_absolute() {
        let dir = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        let file = dir.path().join("song.msproj");
        let mut original = base_project(dir.path());
        let outside = elsewhere.path().join("pad.wav");
        let mut pad = original.sources[0].clone();
        pad.id = 100;
        pad.name = "pad".into();
        pad.path = outside.to_string_lossy().into_owned();
        original.sources.push(pad);
        original.next_id = 101;

        save_project(&original, &file).unwrap();
        let loaded = load_project(&file).unwrap();

        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(json["sources"][0]["path"], "kick.wav");
        assert_eq!(
            Path::new(&loaded.sources[0].path),
            Path::new(&original.sources[0].path)
        );
        assert_eq!(Path::new(&loaded.sources[1].path), outside.as_path());
        assert_eq!(loaded.sources.len(), 2);
    }

    #[test]
    fn save_stores_source_inside_project_dir_as_relative_path() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("song.msproj");
        let mut p = base_project(dir.path());
        let nested = dir.path().join("audio").join("snare.wav");
        p.sources[0].path = nested.to_string_lossy().into_owned();

        save_project(&p, &file).unwrap();

        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(json["sources"][0]["path"], "audio/snare.wav");
    }

    #[test]
    fn save_keeps_absolute_path_for_source_outside_project_dir() {
        let dir = TempDir::new().unwrap();
        let elsewhere = TempDir::new().unwrap();
        let file = dir.path().join("song.msproj");
        let mut p = base_project(dir.path());
        let outside = elsewhere
            .path()
            .join("pad.wav")
            .to_string_lossy()
            .into_owned();
        p.sources[0].path = outside.clone();

        save_project(&p, &file).unwrap();

        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(json["sources"][0]["path"].as_str(), Some(outside.as_str()));
        let loaded = load_project(&file).unwrap();
        assert_eq!(loaded.sources[0].path, outside);
    }

    #[test]
    fn load_resolves_relative_source_path_against_project_dir() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("song.msproj");
        let mut p = base_project(dir.path());
        p.sources[0].path = dir
            .path()
            .join("audio")
            .join("hat.wav")
            .to_string_lossy()
            .into_owned();
        save_project(&p, &file).unwrap();

        let loaded = load_project(&file).unwrap();

        let resolved = Path::new(&loaded.sources[0].path);
        assert!(resolved.is_absolute());
        assert_eq!(resolved, dir.path().join("audio").join("hat.wav"));
    }

    #[test]
    fn save_leaves_no_temp_file_behind() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("song.msproj");

        save_project(&base_project(dir.path()), &file).unwrap();

        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"song.msproj".to_string()));
        assert!(
            !names.iter().any(|n| n.ends_with(".tmp")),
            "found temp file in {names:?}"
        );
    }

    #[test]
    fn save_overwrites_existing_project_file() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("song.msproj");
        save_project(&Project::new(44_100), &file).unwrap();
        let second = base_project(dir.path());

        save_project(&second, &file).unwrap();

        assert_eq!(load_project(&file).unwrap(), second);
    }

    #[test]
    fn save_into_missing_directory_fails_and_creates_nothing() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("no-such-dir").join("song.msproj");

        let err = save_project(&base_project(dir.path()), &missing).unwrap_err();

        assert!(matches!(err, EngineError::Io { .. }), "got {err:?}");
        assert!(!dir.path().join("no-such-dir").exists());
    }

    #[test]
    fn load_missing_file_is_io_error() {
        let dir = TempDir::new().unwrap();

        let err = load_project(&dir.path().join("absent.msproj")).unwrap_err();

        assert!(matches!(err, EngineError::Io { .. }), "got {err:?}");
    }

    #[test]
    fn load_rejects_invalid_json() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("bad.msproj");
        fs::write(&file, "{ not json").unwrap();

        let err = load_project(&file).unwrap_err();

        assert!(matches!(err, EngineError::ProjectFormat(_)), "got {err:?}");
    }

    #[test]
    fn load_rejects_non_numeric_gain() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("bad.msproj");
        let mut json = serde_json::to_value(base_project(dir.path())).unwrap();
        json["tracks"][0]["gainDb"] = serde_json::Value::Null;
        fs::write(&file, json.to_string()).unwrap();

        assert!(matches!(
            load_project(&file),
            Err(EngineError::ProjectFormat(_))
        ));
    }

    #[test]
    fn load_rejects_future_version() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("future.msproj");
        let mut json = serde_json::to_value(base_project(dir.path())).unwrap();
        json["version"] = serde_json::json!(PROJECT_VERSION + 1);
        fs::write(&file, json.to_string()).unwrap();

        let err = load_project(&file).unwrap_err();

        match err {
            EngineError::ProjectFormat(msg) => assert!(msg.contains("version"), "{msg}"),
            other => panic!("expected ProjectFormat, got {other:?}"),
        }
    }

    #[test]
    fn load_rejects_version_zero() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        p.version = 0;

        assert!(load_error(&dir, &p).contains("version"));
    }

    #[test]
    fn load_rejects_clip_referencing_missing_source() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        first_clip(&mut p).source_id = 999;

        assert!(load_error(&dir, &p).contains("unknown source"));
    }

    #[test]
    fn load_rejects_clip_reading_past_source_end() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        first_clip(&mut p).length = 1_001;

        assert!(load_error(&dir, &p).contains("past the end"));
    }

    #[test]
    fn load_rejects_offset_that_pushes_clip_past_source_end() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        let clip = first_clip(&mut p);
        clip.offset = 900;
        clip.length = 200;

        assert!(load_error(&dir, &p).contains("past the end"));
    }

    #[test]
    fn load_rejects_zero_length_clip() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        first_clip(&mut p).length = 0;

        assert!(load_error(&dir, &p).contains("zero length"));
    }

    #[test]
    fn load_rejects_fades_longer_than_clip() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        let clip = first_clip(&mut p);
        clip.fade_in = 600;
        clip.fade_out = 500;

        assert!(load_error(&dir, &p).contains("fades"));
    }

    #[test]
    fn load_rejects_duplicate_ids_across_tracks_clips_and_sources() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        // Track id 2 collides with the source id 1 only if we set it so; use the source id.
        p.tracks[0].id = p.sources[0].id;

        assert!(load_error(&dir, &p).contains("duplicate id"));
    }

    #[test]
    fn load_rejects_next_id_not_above_existing_ids() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        let max_id = p.tracks[0].clips[0].id;
        p.next_id = max_id;

        assert!(load_error(&dir, &p).contains("nextId"));
    }

    #[test]
    fn load_rejects_more_tracks_than_limit() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        for i in 0..MAX_TRACKS {
            p.tracks.push(Track {
                id: 10_000 + i as u64,
                name: format!("extra {i}"),
                gain_db: 0.0,
                pan: 0.0,
                mute: false,
                solo: false,
                color: "#000000".into(),
                clips: Vec::new(),
            });
        }
        p.next_id = 20_000;

        assert!(load_error(&dir, &p).contains("tracks"));
    }

    #[test]
    fn load_rejects_pan_outside_unit_range() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        p.tracks[0].pan = 1.5;

        assert!(load_error(&dir, &p).contains("pan"));
    }

    #[test]
    fn load_rejects_unsupported_channel_count() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        p.sources[0].channels = 6;

        assert!(load_error(&dir, &p).contains("channel"));
    }

    #[test]
    fn load_rejects_zero_sample_rate() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        p.sample_rate = 0;

        assert!(load_error(&dir, &p).contains("sample rate"));
    }

    #[test]
    fn load_sorts_clips_by_start_within_each_track() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        let mut late = p.tracks[0].clips[0].clone();
        late.id = 50;
        late.start = 500;
        let mut early = late.clone();
        early.id = 51;
        early.start = 100;
        p.tracks[0].clips = vec![late, early];
        p.next_id = 60;
        let file = dir.path().join("unsorted.msproj");
        save_project(&p, &file).unwrap();

        let loaded = load_project(&file).unwrap();

        let starts: Vec<u64> = loaded.tracks[0].clips.iter().map(|c| c.start).collect();
        assert_eq!(starts, vec![100, 500]);
    }

    #[test]
    fn missing_sources_lists_only_absent_files() {
        let dir = TempDir::new().unwrap();
        let mut p = base_project(dir.path());
        fs::write(dir.path().join("kick.wav"), b"").unwrap();
        let absent = SourceRef {
            id: 100,
            name: "gone".into(),
            path: dir.path().join("gone.wav").to_string_lossy().into_owned(),
            channels: 1,
            frames: 10,
            original_sample_rate: 48_000,
        };
        p.sources.push(absent);

        assert_eq!(missing_sources(&p), vec![100]);
    }

    #[test]
    fn missing_sources_is_empty_when_all_files_exist() {
        let dir = TempDir::new().unwrap();
        let p = base_project(dir.path());
        fs::write(dir.path().join("kick.wav"), b"").unwrap();

        assert!(missing_sources(&p).is_empty());
    }
}
