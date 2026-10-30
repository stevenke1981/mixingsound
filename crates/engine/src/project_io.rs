//! Project file (.msproj, pretty JSON) load/save.

use std::path::Path;

use crate::error::Result;
use crate::model::{Project, SourceId};

pub const PROJECT_EXTENSION: &str = "msproj";

/// Writes `project` as JSON. Source paths under the project file's directory are stored
/// relative (with `/` separators); others stay absolute. Atomic: temp file + rename.
pub fn save_project(project: &Project, path: &Path) -> Result<()> {
    let _ = (project, path);
    todo!("io worker")
}

/// Reads a project, resolving relative source paths against the file's directory into absolute
/// paths. Rejects unknown `version` > `PROJECT_VERSION` and broken invariants
/// (`ProjectFormat`): duplicate ids, `next_id` not above every id, clips referencing unknown
/// sources, clip geometry/fade invariants, > MAX_TRACKS tracks.
pub fn load_project(path: &Path) -> Result<Project> {
    let _ = path;
    todo!("io worker")
}

/// Sources whose file does not exist on disk.
pub fn missing_sources(project: &Project) -> Vec<SourceId> {
    let _ = project;
    todo!("io worker")
}
