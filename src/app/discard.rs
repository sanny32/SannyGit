use super::{SannyGitApp, WorkerResult};
use crate::{discard as backend, git, models::ChangeKind};
use std::path::PathBuf;

pub struct Dialog {
    pub root: String,
    pub path: String,
    pub action: backend::Action,
    pub prepared: Option<backend::Prepared>,
    pub confirmed: bool,
    pub running: bool,
    pub error: Option<String>,
}

impl SannyGitApp {
    pub fn discard_blocker(&self) -> Option<&'static str> {
        if self.busy() {
            Some("Wait for the current operation to finish")
        } else if self.repositories.get(self.selected_repository).is_none() {
            Some("Open a repository first")
        } else if !self.state_current {
            Some("Refresh the repository before discarding changes")
        } else if let Some(change) = self.changes.get(self.selected_change) {
            if change.kind == ChangeKind::Conflicted {
                Some("Resolve conflicts outside SannyGit")
            } else if !change.unstaged && change.kind != ChangeKind::Untracked {
                Some("Only staged changes remain; use Unstage separately before discarding")
            } else {
                None
            }
        } else {
            Some("Select one changed file first")
        }
    }

    pub fn open_discard(&mut self) {
        if self.discard_blocker().is_some() {
            return;
        }
        let change = &self.changes[self.selected_change];
        self.discard_dialog = Some(Dialog {
            root: self.repositories[self.selected_repository].path.clone(),
            path: change.path.clone(),
            action: if change.kind == ChangeKind::Untracked {
                backend::Action::DeleteUntracked
            } else {
                backend::Action::Restore
            },
            prepared: None,
            confirmed: false,
            running: false,
            error: None,
        });
        self.reload_discard();
    }

    pub fn reload_discard(&mut self) {
        if self.busy() {
            return;
        }
        let Some(dialog) = &mut self.discard_dialog else {
            return;
        };
        dialog.prepared = None;
        dialog.confirmed = false;
        dialog.error = None;
        let root = dialog.root.clone();
        let path = dialog.path.clone();
        if !self.start_worker(move || WorkerResult::DiscardPrepared {
            result: backend::prepare(&PathBuf::from(&root), &path),
            root,
            path,
        }) {
            if let Some(dialog) = &mut self.discard_dialog {
                dialog.error = self.error.clone();
            }
        }
    }

    pub fn confirm_discard(&mut self) {
        if self.busy() || !self.state_current {
            return;
        }
        let Some(dialog) = &mut self.discard_dialog else {
            return;
        };
        if !dialog.confirmed || dialog.running || dialog.prepared.is_none() {
            return;
        }
        if !self
            .repositories
            .get(self.selected_repository)
            .is_some_and(|repo| repo.path == dialog.root)
        {
            return;
        }
        let Some(prepared) = dialog.prepared.clone() else {
            return;
        };
        if prepared.action != dialog.action || prepared.path != dialog.path {
            return;
        }
        let root = PathBuf::from(&dialog.root);
        dialog.running = true;
        dialog.error = None;
        self.state_current = false;
        self.error = None;
        self.status = "Discard operation in progress…".into();
        if !self.start_worker(move || WorkerResult::Operation {
            committed: false,
            success: match prepared.action {
                backend::Action::Restore => "File restored from index",
                backend::Action::DeleteUntracked => "Untracked file deleted",
            },
            outcome: backend::execute(&root, &prepared),
            snapshot: git::load(&root),
        }) {
            self.finish_discard(false);
        }
    }

    pub(super) fn apply_discard_prepared(
        &mut self,
        root: String,
        path: String,
        result: Result<backend::Prepared, String>,
    ) {
        let Some(dialog) = self
            .discard_dialog
            .as_mut()
            .filter(|dialog| dialog.root == root && dialog.path == path)
        else {
            return;
        };
        match result {
            Ok(prepared) if prepared.action == dialog.action => dialog.prepared = Some(prepared),
            Ok(_) => dialog.error = Some("The file's tracking state changed. Cancel and reopen Discard to review the correct action.".into()),
            Err(error) => dialog.error = Some(error),
        }
    }

    pub(super) fn finish_discard(&mut self, succeeded: bool) {
        if succeeded {
            self.discard_dialog = None;
        } else if let Some(dialog) = &mut self.discard_dialog {
            dialog.running = false;
            dialog.prepared = None;
            dialog.confirmed = false;
            dialog.error = self.error.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{git::tests::TempRepo, theme::ThemeChoice};
    use std::time::{Duration, Instant};

    fn wait(app: &mut SannyGitApp) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.busy() {
            assert!(Instant::now() < deadline);
            app.poll_worker();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn opened() -> (TempRepo, SannyGitApp) {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        git::commit(&repo.0, "Base", git::CommitContents::All).unwrap();
        repo.write("file", "staged\n");
        repo.git(&["add", "file"]);
        repo.write("file", "unstaged\n");
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(git::load(&repo.0).unwrap());
        app.commit_message = "Keep commit draft".into();
        (repo, app)
    }

    #[test]
    fn cancellation_and_unchecked_confirmation_cannot_modify_the_file_or_index() {
        let (repo, mut app) = opened();
        let index = repo.git(&["ls-files", "--stage"]);
        app.open_discard();
        app.discard_dialog = None;
        wait(&mut app);
        assert!(app.discard_dialog.is_none());
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
        app.open_discard();
        wait(&mut app);
        app.confirm_discard();
        assert!(!app.busy());
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
        assert_eq!(repo.git(&["ls-files", "--stage"]), index);
        app.discard_dialog.as_mut().unwrap().confirmed = true;
        app.confirm_discard();
        wait(&mut app);
        assert!(app.error.is_none());
        assert!(app.discard_dialog.is_none());
        assert_eq!(app.status, "File restored from index");
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"staged\n");
        assert_eq!(repo.git(&["ls-files", "--stage"]), index);
        assert_eq!(app.commit_message, "Keep commit draft");
    }

    #[test]
    fn stale_review_keeps_new_work_and_resets_confirmation_until_reload() {
        let (repo, mut app) = opened();
        app.open_discard();
        wait(&mut app);
        repo.write("file", "external new work\n");
        app.discard_dialog.as_mut().unwrap().confirmed = true;
        app.confirm_discard();
        wait(&mut app);
        assert!(app
            .error
            .as_deref()
            .unwrap()
            .contains("changed after review"));
        let dialog = app.discard_dialog.as_ref().unwrap();
        assert!(!dialog.confirmed && !dialog.running && dialog.prepared.is_none());
        assert_eq!(
            std::fs::read(repo.0.join("file")).unwrap(),
            b"external new work\n"
        );
        app.confirm_discard();
        assert!(!app.busy());
        app.reload_discard();
        wait(&mut app);
        assert!(!app.discard_dialog.as_ref().unwrap().confirmed);
        app.discard_dialog.as_mut().unwrap().confirmed = true;
        app.confirm_discard();
        wait(&mut app);
        assert!(app.error.is_none());
        assert!(app.discard_dialog.is_none());
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"staged\n");
    }

    #[test]
    fn staged_only_changes_require_a_separate_unstage_action() {
        let (repo, mut app) = opened();
        repo.git(&["add", "file"]);
        app.apply_snapshot(git::load(&repo.0).unwrap());
        assert!(app.discard_blocker().unwrap().contains("Unstage"));
        app.open_discard();
        assert!(app.discard_dialog.is_none());
        app.set_staged(0, false);
        wait(&mut app);
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
        assert!(app.discard_blocker().is_none());
        app.open_discard();
        wait(&mut app);
        app.discard_dialog.as_mut().unwrap().confirmed = true;
        app.confirm_discard();
        wait(&mut app);
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"base\n");
        assert!(repo.git(&["diff", "--cached", "--name-only"]).is_empty());
        assert_eq!(app.commit_message, "Keep commit draft");
    }

    #[test]
    fn an_untracked_file_staged_after_review_is_not_deleted() {
        let repo = TempRepo::new();
        repo.write("new", "keep new\n");
        let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
        app.apply_snapshot(git::load(&repo.0).unwrap());
        app.open_discard();
        wait(&mut app);
        assert_eq!(
            app.discard_dialog.as_ref().unwrap().action,
            backend::Action::DeleteUntracked
        );
        repo.git(&["add", "new"]);
        app.discard_dialog.as_mut().unwrap().confirmed = true;
        app.confirm_discard();
        wait(&mut app);
        assert!(app.error.is_some());
        assert_eq!(std::fs::read(repo.0.join("new")).unwrap(), b"keep new\n");
        assert_eq!(repo.git(&["show", ":new"]), "keep new\n");
        assert!(!app.discard_dialog.as_ref().unwrap().confirmed);
    }

    #[test]
    fn repository_changes_and_late_reviews_cannot_target_another_file() {
        let (repo, mut app) = opened();
        app.open_discard();
        wait(&mut app);
        let prepared = app
            .discard_dialog
            .as_ref()
            .unwrap()
            .prepared
            .clone()
            .unwrap();
        app.apply_discard_prepared(
            "/another-repository".into(),
            "file".into(),
            Err("Late error".into()),
        );
        assert!(app.discard_dialog.as_ref().unwrap().error.is_none());
        app.discard_dialog.as_mut().unwrap().confirmed = true;
        app.selected_repository = usize::MAX;
        app.confirm_discard();
        assert!(!app.busy());
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
        let root = app.discard_dialog.as_ref().unwrap().root.clone();
        app.discard_dialog = None;
        app.apply_discard_prepared(root, "file".into(), Ok(prepared));
        assert!(app.discard_dialog.is_none());
    }
    #[test]
    fn disconnected_worker_resets_the_dialog_without_changing_files() {
        let (repo, mut app) = opened();
        app.open_discard();
        wait(&mut app);
        let dialog = app.discard_dialog.as_mut().unwrap();
        dialog.confirmed = true;
        dialog.running = true;
        app.state_current = false;
        let (sender, receiver) = std::sync::mpsc::channel();
        app.worker = Some(receiver);
        drop(sender);
        app.poll_worker();
        assert!(!app.busy());
        let dialog = app.discard_dialog.as_ref().unwrap();
        assert!(!dialog.running && !dialog.confirmed && dialog.prepared.is_none());
        assert!(dialog
            .error
            .as_deref()
            .unwrap()
            .contains("stopped unexpectedly"));
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
        assert_eq!(repo.git(&["show", ":file"]), "staged\n");
    }
}
