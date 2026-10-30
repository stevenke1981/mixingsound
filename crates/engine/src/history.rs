//! Undo/redo. Edits are commands (`edit::Edit`); undo restores project snapshots
//! (projects hold no audio, so snapshots are cheap).

use crate::edit::{CoalesceKey, Edit, EditOutcome, apply_edit};
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
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            last_key: None,
            limit,
        }
    }

    /// Applies `edit` via `apply_edit`. On success pushes the pre-edit snapshot (unless it
    /// coalesces with the previous edit: same `Some(coalesce_key)`), clears redo, and trims
    /// undo to `limit`. On error nothing changes.
    pub fn apply(&mut self, project: &mut Project, edit: &Edit) -> Result<EditOutcome> {
        let key = edit.coalesce_key();
        // A coalesced edit keeps the snapshot of the gesture's first edit, so the copy is skipped.
        let coalesces = key.is_some() && key == self.last_key;
        let snapshot = (!coalesces).then(|| project.clone());

        let outcome = apply_edit(project, edit)?;

        if let Some(snapshot) = snapshot {
            self.push_undo(snapshot);
        }
        self.redo.clear();
        self.last_key = key;
        Ok(outcome)
    }

    /// Runs several edits as ONE undo step (e.g. import = AddSource + AddTrack + AddClip per
    /// file). `f` calls `edit::apply_edit` on the project as often as it likes. If `f` errors,
    /// the project is restored to its pre-group state and nothing is recorded. Never coalesces.
    pub fn apply_group<T>(
        &mut self,
        project: &mut Project,
        f: impl FnOnce(&mut Project) -> Result<T>,
    ) -> Result<T> {
        let snapshot = project.clone();
        match f(project) {
            Ok(value) => {
                self.push_undo(snapshot);
                self.redo.clear();
                self.last_key = None;
                Ok(value)
            }
            Err(err) => {
                *project = snapshot;
                Err(err)
            }
        }
    }

    /// Returns false if there is nothing to undo.
    pub fn undo(&mut self, project: &mut Project) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        let current = std::mem::replace(project, previous);
        self.redo.push(current);
        self.last_key = None;
        true
    }

    pub fn redo(&mut self, project: &mut Project) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let current = std::mem::replace(project, next);
        self.push_undo(current);
        self.last_key = None;
        true
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

    /// Pushes a snapshot and drops the oldest ones beyond `limit`, so memory stays bounded.
    fn push_undo(&mut self, snapshot: Project) {
        self.undo.push(snapshot);
        let excess = self.undo.len().saturating_sub(self.limit);
        self.undo.drain(..excess);
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new(Self::DEFAULT_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::EngineError;
    use crate::model::TrackId;

    /// Project with one track named "base" (id 1).
    fn project_with_track() -> (Project, TrackId) {
        let mut p = Project::new(48_000);
        let t = apply_edit(
            &mut p,
            &Edit::AddTrack {
                name: "base".into(),
                index: None,
            },
        )
        .unwrap()
        .created
        .unwrap();
        (p, t)
    }

    fn names(p: &Project) -> Vec<&str> {
        p.tracks.iter().map(|t| t.name.as_str()).collect()
    }

    fn add(history: &mut History, p: &mut Project, name: &str) {
        history
            .apply(
                p,
                &Edit::AddTrack {
                    name: name.into(),
                    index: None,
                },
            )
            .expect("add track");
    }

    fn set_gain(history: &mut History, p: &mut Project, track: TrackId, gain_db: f32) {
        history
            .apply(p, &Edit::SetTrackGain { track, gain_db })
            .expect("set gain");
    }

    fn gain(p: &Project, track: TrackId) -> f32 {
        p.track(track).unwrap().gain_db
    }

    #[test]
    fn apply_undo_redo_walks_back_and_forth() {
        let mut h = History::default();
        let (mut p, _) = project_with_track();
        add(&mut h, &mut p, "a");
        add(&mut h, &mut p, "b");

        assert!(h.undo(&mut p));
        assert_eq!(names(&p), vec!["base", "a"]);
        assert!(h.undo(&mut p));
        assert_eq!(names(&p), vec!["base"]);
        assert!(h.redo(&mut p));
        assert_eq!(names(&p), vec!["base", "a"]);
        assert!(h.redo(&mut p));
        assert_eq!(names(&p), vec!["base", "a", "b"]);
        assert!(!h.redo(&mut p), "nothing left to redo");
    }

    #[test]
    fn undo_on_empty_history_returns_false() {
        let mut h = History::default();
        let (mut p, _) = project_with_track();
        let before = p.clone();

        assert!(!h.can_undo());
        assert!(!h.undo(&mut p));
        assert!(!h.redo(&mut p));
        assert_eq!(p, before);
    }

    #[test]
    fn new_edit_clears_redo_stack() {
        let mut h = History::default();
        let (mut p, _) = project_with_track();
        add(&mut h, &mut p, "a");
        assert!(h.undo(&mut p));
        assert!(h.can_redo());

        add(&mut h, &mut p, "b");

        assert!(!h.can_redo());
        assert!(!h.redo(&mut p));
        assert_eq!(names(&p), vec!["base", "b"]);
    }

    #[test]
    fn failed_edit_changes_neither_project_nor_history() {
        let mut h = History::default();
        let (mut p, _) = project_with_track();
        add(&mut h, &mut p, "a");
        assert!(h.undo(&mut p));
        let before = p.clone();

        let err = h
            .apply(&mut p, &Edit::RemoveTrack { track: 999 })
            .expect_err("unknown track must fail");

        assert!(matches!(err, EngineError::NotFound(_)));
        assert_eq!(p, before);
        assert!(h.can_redo(), "a failed edit must not clear redo");
        assert!(!h.can_undo());
    }

    #[test]
    fn repeated_gain_changes_coalesce_into_one_undo_step() {
        let mut h = History::default();
        let (mut p, t) = project_with_track();

        set_gain(&mut h, &mut p, t, -1.0);
        set_gain(&mut h, &mut p, t, -2.0);
        set_gain(&mut h, &mut p, t, -3.0);
        assert_eq!(gain(&p, t), -3.0);

        assert!(h.undo(&mut p));
        assert_eq!(
            gain(&p, t),
            0.0,
            "undo returns to the value before the whole gesture"
        );
        assert!(!h.can_undo(), "the gesture was a single step");
    }

    #[test]
    fn break_coalescing_starts_a_new_undo_step() {
        let mut h = History::default();
        let (mut p, t) = project_with_track();

        set_gain(&mut h, &mut p, t, -1.0);
        h.break_coalescing();
        set_gain(&mut h, &mut p, t, -2.0);

        assert!(h.undo(&mut p));
        assert_eq!(gain(&p, t), -1.0);
        assert!(h.undo(&mut p));
        assert_eq!(gain(&p, t), 0.0);
    }

    #[test]
    fn different_coalesce_keys_do_not_merge() {
        let mut h = History::default();
        let (mut p, t) = project_with_track();
        let other = add_and_get_id(&mut h, &mut p, "other");

        set_gain(&mut h, &mut p, t, -1.0);
        set_gain(&mut h, &mut p, other, -1.0);
        h.apply(&mut p, &Edit::SetTrackPan { track: t, pan: 0.5 })
            .unwrap();

        assert!(h.undo(&mut p));
        assert_eq!(p.track(t).unwrap().pan, 0.0, "pan step undone alone");
        assert!(h.undo(&mut p));
        assert_eq!(gain(&p, other), 0.0, "second track gain undone alone");
        assert_eq!(gain(&p, t), -1.0);
        assert!(h.undo(&mut p));
        assert_eq!(gain(&p, t), 0.0);
    }

    fn add_and_get_id(h: &mut History, p: &mut Project, name: &str) -> TrackId {
        h.apply(
            p,
            &Edit::AddTrack {
                name: name.into(),
                index: None,
            },
        )
        .unwrap()
        .created
        .unwrap()
    }

    #[test]
    fn undo_resets_coalescing_so_next_gesture_is_a_new_step() {
        let mut h = History::default();
        let (mut p, t) = project_with_track();
        set_gain(&mut h, &mut p, t, -1.0);
        assert!(h.undo(&mut p));

        set_gain(&mut h, &mut p, t, -2.0);

        assert!(h.undo(&mut p));
        assert_eq!(gain(&p, t), 0.0);
        assert!(!h.can_undo());
    }

    #[test]
    fn limit_drops_oldest_undo_steps() {
        let mut h = History::new(2);
        let (mut p, _) = project_with_track();
        add(&mut h, &mut p, "a");
        add(&mut h, &mut p, "b");
        add(&mut h, &mut p, "c");

        assert!(h.undo(&mut p));
        assert_eq!(names(&p), vec!["base", "a", "b"]);
        assert!(h.undo(&mut p));
        assert_eq!(names(&p), vec!["base", "a"]);
        assert!(
            !h.undo(&mut p),
            "the state before the first add was dropped by the limit"
        );
    }

    #[test]
    fn apply_group_is_a_single_undo_step() {
        let mut h = History::default();
        let mut p = Project::new(48_000);

        h.apply_group(&mut p, |p| {
            apply_edit(
                p,
                &Edit::AddTrack {
                    name: "x".into(),
                    index: None,
                },
            )?;
            apply_edit(
                p,
                &Edit::AddTrack {
                    name: "y".into(),
                    index: None,
                },
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(names(&p), vec!["x", "y"]);

        assert!(h.undo(&mut p));
        assert!(p.tracks.is_empty());
        assert!(!h.can_undo());
    }

    #[test]
    fn apply_group_error_restores_project_and_records_nothing() {
        let mut h = History::default();
        let (mut p, _) = project_with_track();
        add(&mut h, &mut p, "a");
        assert!(h.undo(&mut p));
        let before = p.clone();

        let result: Result<()> = h.apply_group(&mut p, |p| {
            apply_edit(
                p,
                &Edit::AddTrack {
                    name: "partial".into(),
                    index: None,
                },
            )?;
            apply_edit(p, &Edit::RemoveTrack { track: 999 })?;
            Ok(())
        });

        assert!(matches!(result, Err(EngineError::NotFound(_))));
        assert_eq!(p, before, "partial group edits are rolled back");
        assert!(h.can_redo(), "a failed group must not clear redo");
        assert!(!h.can_undo());
    }

    #[test]
    fn apply_group_returns_closure_value() {
        let mut h = History::default();
        let mut p = Project::new(48_000);

        let id = h
            .apply_group(&mut p, |p| {
                let outcome = apply_edit(
                    p,
                    &Edit::AddTrack {
                        name: "x".into(),
                        index: None,
                    },
                )?;
                Ok(outcome.created)
            })
            .unwrap();

        assert_eq!(id, Some(p.tracks[0].id));
    }

    #[test]
    fn apply_group_breaks_coalescing_of_surrounding_gestures() {
        let mut h = History::default();
        let (mut p, t) = project_with_track();
        set_gain(&mut h, &mut p, t, -1.0);
        h.apply_group(&mut p, |p| {
            apply_edit(
                p,
                &Edit::AddTrack {
                    name: "g".into(),
                    index: None,
                },
            )?;
            Ok(())
        })
        .unwrap();

        set_gain(&mut h, &mut p, t, -2.0);

        assert!(h.undo(&mut p));
        assert_eq!(gain(&p, t), -1.0);
        assert!(h.undo(&mut p));
        assert_eq!(names(&p), vec!["base"]);
        assert_eq!(gain(&p, t), -1.0);
        assert!(h.undo(&mut p));
        assert_eq!(gain(&p, t), 0.0);
    }

    #[test]
    fn clear_drops_all_history() {
        let mut h = History::default();
        let (mut p, _) = project_with_track();
        add(&mut h, &mut p, "a");
        assert!(h.undo(&mut p));

        h.clear();

        assert!(!h.can_undo());
        assert!(!h.can_redo());
    }
}
