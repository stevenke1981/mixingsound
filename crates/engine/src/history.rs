//! Undo/redo. Edits are commands (`edit::Edit`); undo restores project snapshots
//! (projects hold no audio, so snapshots are cheap).

use crate::edit::{CoalesceKey, Edit, EditOutcome};
use crate::error::Result;
use crate::model::Project;

pub struct History {
    undo: Vec<Project>,
    redo: Vec<Project>,
    last_key: Option<CoalesceKey>,
    limit: usize,
}

impl History {
    pub const DEFAULT_LIMIT: usize = 200;

    pub fn new(limit: usize) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), last_key: None, limit }
    }

    /// Applies `edit` via `apply_edit`. On success pushes the pre-edit snapshot (unless it
    /// coalesces with the previous edit: same `Some(coalesce_key)`), clears redo, and trims
    /// undo to `limit`. On error nothing changes.
    pub fn apply(&mut self, project: &mut Project, edit: &Edit) -> Result<EditOutcome> {
        let _ = (project, edit, &self.undo, &self.redo, self.last_key, self.limit);
        todo!("history worker")
    }

    /// Runs several edits as ONE undo step (e.g. import = AddSource + AddTrack + AddClip per
    /// file). `f` calls `edit::apply_edit` on the project as often as it likes. If `f` errors,
    /// the project is restored to its pre-group state and nothing is recorded. Never coalesces.
    pub fn apply_group<T>(
        &mut self,
        project: &mut Project,
        f: impl FnOnce(&mut Project) -> Result<T>,
    ) -> Result<T> {
        let _ = (project, f);
        todo!("history worker")
    }

    /// Returns false if there is nothing to undo.
    pub fn undo(&mut self, project: &mut Project) -> bool {
        let _ = project;
        todo!("history worker")
    }

    pub fn redo(&mut self, project: &mut Project) -> bool {
        let _ = project;
        todo!("history worker")
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// End of a continuous gesture (fader release, drag end): the next edit starts a new step.
    pub fn break_coalescing(&mut self) {
        self.last_key = None;
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last_key = None;
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new(Self::DEFAULT_LIMIT)
    }
}
