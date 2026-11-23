use super::{SannyGitApp, WorkerResult};
use crate::{branches, models::RefKind, remotes};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

pub struct Dialog {
    pub root: String,
    pub commit: String,
    pub subject: String,
    pub branch_name: String,
    pub upstream: String,
    pub preview: Option<remotes::PushUpToPreview>,
    pub error: Option<String>,
}

impl SannyGitApp {
    pub fn open_push_up_to(&mut self, commit: String) {
        if self.busy()
            || !self.state_current
            || !self.branches.current
            || !branches::is_commit_id(&commit)
        {
            return;
        }
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let Some(branch) = self.branches.entries.iter().find(|entry| {
            entry.kind == RefKind::Local && entry.current && entry.upstream.is_some()
        }) else {
            return;
        };
        let root = repository.path.clone();
        let subject = self
            .history
            .commits
            .iter()
            .find(|entry| entry.hash == commit)
            .map(|entry| entry.subject().to_owned())
            .unwrap_or_else(|| commit.chars().take(12).collect());
        self.push_up_to = Some(Dialog {
            root: root.clone(),
            commit: commit.clone(),
            subject,
            branch_name: branch.name.clone(),
            upstream: branch.upstream.clone().unwrap_or_default(),
            preview: None,
            error: None,
        });
        if !self.start_worker(move |git| WorkerResult::PushUpToDetails {
            root: root.clone(),
            result: remotes::preview_push_up_to(git, std::path::Path::new(&root), &commit),
        }) {
            self.push_up_to = None;
        }
    }

    pub(super) fn apply_push_up_to(
        &mut self,
        root: String,
        result: Result<remotes::PushUpToPreview, String>,
    ) {
        let Some(dialog) = self
            .push_up_to
            .as_mut()
            .filter(|dialog| dialog.root == root)
        else {
            return;
        };
        match result {
            Ok(preview) if preview.commit.eq_ignore_ascii_case(&dialog.commit) => {
                dialog.branch_name.clone_from(&preview.branch_name);
                dialog.upstream = format!(
                    "{}/{}",
                    preview.remote,
                    preview
                        .destination
                        .strip_prefix("refs/heads/")
                        .unwrap_or(&preview.destination)
                );
                dialog.preview = Some(preview);
                dialog.error = None;
            }
            Ok(_) => {
                dialog.preview = None;
                dialog.error = Some("The selected commit changed. Close and reopen Push.".into());
            }
            Err(error) => {
                dialog.preview = None;
                dialog.error = Some(error);
            }
        }
    }

    pub fn submit_push_up_to(&mut self) {
        if self.busy() || !self.state_current {
            return;
        }
        let Some(dialog) = &self.push_up_to else {
            return;
        };
        if !self
            .repositories
            .get(self.selected_repository)
            .is_some_and(|repository| repository.path == dialog.root)
        {
            return;
        }
        let Some(preview) = dialog
            .preview
            .clone()
            .filter(|preview| preview.fast_forward)
        else {
            return;
        };
        let root = PathBuf::from(&dialog.root);
        let cancel = Arc::new(AtomicBool::new(false));
        self.manager.cancel = Some(cancel.clone());
        self.state_current = false;
        self.error = None;
        self.status = "Pushing commits…".into();
        if self.start_background_operation(move |git| WorkerResult::Operation {
            committed: false,
            success: "Commits pushed",
            outcome: remotes::push_up_to(git, &root, &preview, &cancel),
            snapshot: crate::git::load(git, &root),
        }) {
            self.push_up_to = None;
        } else {
            self.state_current = true;
            self.manager.cancel = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TempRepo, SYSTEM_GIT};
    use std::time::{Duration, Instant};

    fn wait(app: &mut SannyGitApp) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while app.busy() {
            app.poll_worker();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn pushing_up_to_a_commit_stops_at_that_commit() {
        let repo = TempRepo::new();
        let origin = repo.0.join("origin.git");
        repo.git(&[
            "init",
            "--bare",
            "--initial-branch=main",
            origin.to_str().unwrap(),
        ]);
        repo.git(&["remote", "add", "origin", origin.to_str().unwrap()]);
        repo.git(&["commit", "--allow-empty", "-m", "base"]);
        repo.git(&["push", "-u", "origin", "main"]);
        repo.git(&["commit", "--allow-empty", "-m", "middle"]);
        let middle = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
        repo.git(&["commit", "--allow-empty", "-m", "tip"]);
        let tip = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();

        let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&SYSTEM_GIT, &repo.0).unwrap());
        app.branches.current = true;
        app.branches.entries = crate::branches::load(&SYSTEM_GIT, &repo.0).unwrap().refs;
        app.history.commits = vec![crate::models::Commit {
            message: "middle".into(),
            author: String::new(),
            email: String::new(),
            date: String::new(),
            hash: middle.clone(),
            parents: Vec::new(),
            lane: 0,
        }];
        app.open_push_up_to("abc".into());
        assert!(app.push_up_to.is_none() && !app.busy());
        app.open_push_up_to(middle.clone());
        wait(&mut app);
        let Some(dialog) = app.push_up_to.as_ref() else {
            panic!("push dialog did not open");
        };
        assert!(dialog.error.is_none(), "{:?}", dialog.error);
        assert_eq!(dialog.subject, "middle");
        let Some(preview) = dialog.preview.as_ref() else {
            panic!("push preview did not load");
        };
        assert!(preview.fast_forward);
        app.submit_push_up_to();
        assert!(app.push_up_to.is_none() && app.busy());
        assert_eq!(app.status, "Pushing commits…");
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.status, "Commits pushed");
        assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), tip);
        assert_eq!(
            repo.git(&["rev-parse", "refs/remotes/origin/main"]).trim(),
            middle
        );
        assert_eq!(
            crate::git::text(
                crate::git::run(&SYSTEM_GIT, &origin, &["rev-parse", "refs/heads/main"]).unwrap()
            )
            .unwrap()
            .trim(),
            middle
        );

        app.branches.current = true;
        app.branches.entries = crate::branches::load(&SYSTEM_GIT, &repo.0).unwrap().refs;
        app.open_push_up_to(middle.clone());
        wait(&mut app);
        let Some(dialog) = app.push_up_to.as_ref() else {
            panic!("push dialog did not open for a contained commit");
        };
        assert!(dialog.error.is_none(), "{:?}", dialog.error);
        assert!(dialog
            .preview
            .as_ref()
            .is_some_and(|preview| preview.already_pushed && !preview.fast_forward));
        app.submit_push_up_to();
        assert!(app.push_up_to.is_some() && !app.busy());
    }
}
