use super::{SannyGitApp, WorkerResult};
use crate::{git, models::DiffPreview, stash as backend};
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub enum Action {
    Save,
    Apply,
    Pop,
    Drop,
}

pub struct Dialog {
    pub path: String,
    pub entries: Vec<backend::Entry>,
    pub selected: Option<usize>,
    pub entries_current: bool,
    pub reload_pending: bool,
    pub message: String,
    pub include_untracked: bool,
    pub restore_index: bool,
    pub confirm_drop: bool,
    pub prepared_save: Option<backend::SavePreview>,
    pub previewed_entry: Option<backend::Entry>,
    pub preview: Option<DiffPreview>,
    pub error: Option<String>,
    pub status: String,
}

impl Dialog {
    pub fn invalidate_preview(&mut self) {
        self.prepared_save = None;
        self.previewed_entry = None;
        self.preview = None;
        self.confirm_drop = false;
    }

    pub fn selected_entry(&self) -> Option<&backend::Entry> {
        self.selected.and_then(|index| self.entries.get(index))
    }
}

pub enum ResultMessage {
    List(Result<Vec<backend::Entry>, String>),
    SavePreview(Result<(backend::SavePreview, DiffPreview), String>),
    EntryPreview {
        entry: backend::Entry,
        result: Result<DiffPreview, String>,
    },
}

impl SannyGitApp {
    pub fn stash_allowed(&self) -> bool {
        !self.busy()
            && self.state_current
            && self.repositories.get(self.selected_repository).is_some()
    }

    pub fn open_stashes(&mut self) {
        if !self.stash_allowed() {
            return;
        }
        self.stash_dialog = Some(Dialog {
            path: self.repositories[self.selected_repository].path.clone(),
            entries: Vec::new(),
            selected: None,
            entries_current: false,
            reload_pending: true,
            message: String::new(),
            include_untracked: false,
            restore_index: true,
            confirm_drop: false,
            prepared_save: None,
            previewed_entry: None,
            preview: None,
            error: None,
            status: "Loading stashes…".into(),
        });
        self.load_pending_stashes();
    }

    pub fn reload_stashes(&mut self) {
        if self.busy() {
            return;
        }
        if let Some(dialog) = &mut self.stash_dialog {
            dialog.entries_current = false;
            dialog.reload_pending = true;
            dialog.error = None;
            dialog.status = "Loading stashes…".into();
            dialog.invalidate_preview();
        }
        self.load_pending_stashes();
    }

    pub(super) fn load_pending_stashes(&mut self) {
        if self.busy() {
            return;
        }
        let Some(dialog) = &mut self.stash_dialog else {
            return;
        };
        if !dialog.reload_pending {
            return;
        }
        dialog.reload_pending = false;
        let path = dialog.path.clone();
        if !self.start_worker(move || WorkerResult::Stash {
            result: Box::new(ResultMessage::List(backend::list(&PathBuf::from(&path)))),
            path,
        }) {
            if let Some(dialog) = &mut self.stash_dialog {
                dialog.error = self.error.clone();
            }
        }
    }

    pub fn preview_stash(&mut self, saving: bool) {
        if !self.stash_allowed() {
            return;
        }
        let Some(dialog) = &mut self.stash_dialog else {
            return;
        };
        let entry = dialog.selected_entry().cloned();
        if !saving && (!dialog.entries_current || entry.is_none()) {
            return;
        }
        let path = dialog.path.clone();
        let include = dialog.include_untracked;
        dialog.invalidate_preview();
        dialog.error = None;
        dialog.status = "Loading contents preview…".into();
        self.start_worker(move || {
            let root = PathBuf::from(&path);
            let result = if saving {
                ResultMessage::SavePreview(backend::preview_save(&root, include).map(|prepared| {
                    let diff = DiffPreview::new(&prepared.patch);
                    (prepared, diff)
                }))
            } else if let Some(entry) = entry {
                let result =
                    backend::preview_entry(&root, &entry).map(|patch| DiffPreview::new(&patch));
                ResultMessage::EntryPreview { entry, result }
            } else {
                ResultMessage::List(Err("Select a stash entry first.".into()))
            };
            WorkerResult::Stash {
                path,
                result: Box::new(result),
            }
        });
    }

    pub fn run_stash(&mut self, action: Action) {
        if !self.stash_allowed() {
            return;
        }
        let Some(dialog) = &mut self.stash_dialog else {
            return;
        };
        let entry_ready = dialog.entries_current
            && dialog.selected_entry().is_some()
            && dialog.previewed_entry.as_ref() == dialog.selected_entry();
        match action {
            Action::Save if dialog.prepared_save.is_none() => return,
            Action::Apply | Action::Pop | Action::Drop if !entry_ready => return,
            Action::Drop if !dialog.confirm_drop => return,
            _ => {}
        }
        let root = PathBuf::from(&dialog.path);
        let prepared = dialog.prepared_save.clone();
        let entries = dialog.entries.clone();
        let selected = dialog.selected.unwrap_or(usize::MAX);
        let message = dialog.message.clone();
        let restore_index = dialog.restore_index;
        dialog.error = None;
        dialog.status = "Stash operation in progress…".into();
        dialog.invalidate_preview();
        dialog.entries_current = false;
        self.error = None;
        self.state_current = false;
        self.status = "Stash operation in progress…".into();
        self.start_worker(move || {
            let (outcome, success) = match action {
                Action::Save => (
                    prepared
                        .as_ref()
                        .ok_or_else(|| "Preview changes before saving.".to_owned())
                        .and_then(|preview| backend::save(&root, &message, preview)),
                    "Stash saved",
                ),
                Action::Apply => (
                    backend::apply(&root, &entries, selected, restore_index, false),
                    "Stash applied; entry retained",
                ),
                Action::Pop => (
                    backend::apply(&root, &entries, selected, restore_index, true),
                    "Stash applied and removed",
                ),
                Action::Drop => (
                    backend::drop_entry(&root, &entries, selected),
                    "Stash dropped",
                ),
            };
            WorkerResult::Operation {
                committed: false,
                success,
                outcome,
                snapshot: git::load(&root),
            }
        });
    }

    pub(super) fn apply_stash_result(&mut self, path: String, result: ResultMessage) {
        let Some(dialog) = self
            .stash_dialog
            .as_mut()
            .filter(|dialog| dialog.path == path)
        else {
            return;
        };
        match result {
            ResultMessage::List(result) => match result {
                Ok(entries) => {
                    let selected = dialog.selected_entry().cloned();
                    dialog.entries = entries;
                    dialog.selected = selected
                        .and_then(|entry| dialog.entries.iter().position(|item| *item == entry));
                    dialog.entries_current = true;
                    dialog.status = format!("{} stashes. Apply retains the entry; Pop removes it only after a successful apply.", dialog.entries.len());
                }
                Err(error) => {
                    dialog.entries_current = false;
                    dialog.error = Some(error);
                }
            },
            ResultMessage::SavePreview(result) => match result {
                Ok((prepared, diff)) if prepared.include_untracked == dialog.include_untracked => {
                    dialog.prepared_save = Some(prepared);
                    dialog.preview = Some(diff);
                    dialog.status = "Preview: local changes to save. Save rechecks these contents before changing files.".into();
                }
                Ok(_) => {}
                Err(error) => dialog.error = Some(error),
            },
            ResultMessage::EntryPreview { entry, result }
                if dialog.selected_entry() == Some(&entry) =>
            {
                match result {
                    Ok(diff) => {
                        dialog.previewed_entry = Some(entry);
                        dialog.preview = Some(diff);
                        dialog.status =
                            "Preview: selected stash. Apply and Pop require a clean working tree."
                                .into();
                    }
                    Err(error) => dialog.error = Some(error),
                }
            }
            ResultMessage::EntryPreview { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_support::TempRepo, theme::ThemeChoice};
    use std::time::{Duration, Instant};

    fn wait(app: &mut SannyGitApp) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            app.poll_worker();
            app.load_pending_stashes();
            if !app.busy() {
                break;
            }
            assert!(Instant::now() < deadline, "Stash worker timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn stash_workflow_preserves_commit_draft_requires_preview_and_confirms_drop() {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        git::commit(&repo.0, "Base", git::CommitContents::All).unwrap();
        repo.write("file", "staged\n");
        repo.git(&["add", "file"]);
        repo.write("file", "unstaged\n");
        repo.write("new", "new\n");
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(git::load(&repo.0).unwrap());
        app.commit_message = "Commit draft remains".into();
        app.open_stashes();
        wait(&mut app);
        app.run_stash(Action::Save);
        assert!(!app.busy());
        assert!(backend::list(&repo.0).unwrap().is_empty());
        app.stash_dialog.as_mut().unwrap().include_untracked = true;
        app.preview_stash(true);
        wait(&mut app);
        assert!(app.stash_dialog.as_ref().unwrap().prepared_save.is_some());
        app.run_stash(Action::Save);
        wait(&mut app);
        assert!(app.error.is_none());
        assert_eq!(app.status, "Stash saved");
        assert!(app.changes.is_empty());
        assert_eq!(app.commit_message, "Commit draft remains");
        assert_eq!(app.stash_dialog.as_ref().unwrap().entries.len(), 1);
        app.stash_dialog.as_mut().unwrap().selected = Some(0);
        app.run_stash(Action::Apply);
        assert!(!app.busy());
        assert!(app.changes.is_empty());
        app.preview_stash(false);
        wait(&mut app);
        let selected = app
            .stash_dialog
            .as_ref()
            .unwrap()
            .selected_entry()
            .unwrap()
            .clone();
        app.reload_stashes();
        wait(&mut app);
        assert_eq!(
            app.stash_dialog.as_ref().unwrap().selected_entry(),
            Some(&selected)
        );
        assert!(app.stash_dialog.as_ref().unwrap().previewed_entry.is_none());
        app.preview_stash(false);
        wait(&mut app);
        app.run_stash(Action::Apply);
        wait(&mut app);
        assert!(app.error.is_none());
        assert_eq!(app.status, "Stash applied; entry retained");
        assert_eq!(app.commit_message, "Commit draft remains");
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
        assert_eq!(repo.git(&["show", ":file"]), "staged\n");
        app.preview_stash(false);
        wait(&mut app);
        app.run_stash(Action::Drop);
        assert!(!app.busy());
        assert_eq!(backend::list(&repo.0).unwrap().len(), 1);
        app.stash_dialog.as_mut().unwrap().confirm_drop = true;
        app.run_stash(Action::Drop);
        wait(&mut app);
        assert!(backend::list(&repo.0).unwrap().is_empty());
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
        assert_eq!(app.commit_message, "Commit draft remains");
    }

    #[test]
    fn conflict_results_keep_entry_and_actual_conflicts_and_require_a_new_preview() {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        git::commit(&repo.0, "Base", git::CommitContents::All).unwrap();
        repo.write("file", "stashed\n");
        backend::save(
            &repo.0,
            "Conflict",
            &backend::preview_save(&repo.0, false).unwrap(),
        )
        .unwrap();
        repo.write("file", "diverged\n");
        git::commit(&repo.0, "Diverged", git::CommitContents::All).unwrap();
        let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
        app.apply_snapshot(git::load(&repo.0).unwrap());
        app.commit_message = "Keep draft".into();
        app.open_stashes();
        wait(&mut app);
        let dialog = app.stash_dialog.as_mut().unwrap();
        dialog.selected = Some(0);
        dialog.restore_index = false;
        app.preview_stash(false);
        wait(&mut app);
        app.run_stash(Action::Pop);
        wait(&mut app);
        assert!(app.error.as_deref().unwrap().contains("retained"));
        assert!(!app.status.contains("applied and removed"));
        assert_eq!(app.stash_dialog.as_ref().unwrap().entries.len(), 1);
        assert!(app.stash_dialog.as_ref().unwrap().previewed_entry.is_none());
        assert!(app
            .changes
            .iter()
            .any(|change| change.kind == crate::models::ChangeKind::Conflicted));
        assert_eq!(app.commit_message, "Keep draft");
    }

    #[test]
    fn late_stash_previews_cannot_replace_another_selection_or_repository() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        let repo = TempRepo::new();
        app.apply_snapshot(git::load(&repo.0).unwrap());
        app.open_stashes();
        wait(&mut app);
        let path = app.stash_dialog.as_ref().unwrap().path.clone();
        app.apply_stash_result(
            path.clone(),
            ResultMessage::EntryPreview {
                entry: backend::Entry {
                    oid: "a".repeat(40),
                    message: "No longer selected".into(),
                },
                result: Ok(DiffPreview::new("+late\n")),
            },
        );
        assert!(app.stash_dialog.as_ref().unwrap().preview.is_none());
        app.apply_stash_result(
            "/another-repository".into(),
            ResultMessage::List(Ok(Vec::new())),
        );
        assert_eq!(app.stash_dialog.as_ref().unwrap().path, path);
        app.stash_dialog = None;
        app.apply_stash_result(path, ResultMessage::List(Err("Late error".into())));
        assert!(app.stash_dialog.is_none());
    }
}
