use super::{SannyGitApp, WorkerResult};
use crate::{branches, git, models::RefKind, remote_config, remotes};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Default)]
pub struct RemoteEditor {
    pub snapshot: Option<remote_config::Snapshot>,
    pub current: bool,
    pub selected: Option<String>,
    pub name: String,
    pub url: String,
    pub push_url: String,
    pub confirm_remove: bool,
    pub keep_draft: bool,
}

impl RemoteEditor {
    pub fn select(&mut self, name: Option<String>) {
        let entry = name.as_deref().and_then(|name| {
            self.snapshot
                .as_ref()?
                .entries
                .iter()
                .find(|entry| entry.name == name)
        });
        self.name = entry.map(|entry| entry.name.clone()).unwrap_or_default();
        self.url = entry
            .map(|entry| entry.edit_url.clone())
            .unwrap_or_default();
        self.push_url = entry
            .map(|entry| entry.edit_push_url.clone())
            .unwrap_or_default();
        self.selected = name.filter(|_| entry.is_some());
        self.confirm_remove = false;
    }
}

pub enum Kind {
    Delete {
        source: String,
        prepared: Option<branches::DeletePreview>,
    },
    Tag {
        source: String,
        names: Vec<String>,
        remote: String,
        prepared: Option<remotes::TagPushPreview>,
    },
    AddTagRemote,
    Reset {
        source: String,
        prepared: Option<branches::ResetPreview>,
        choose_changes: bool,
        advanced: Option<branches::AdvancedReset>,
    },
    Remotes(RemoteEditor),
}

pub struct Dialog {
    pub root: String,
    pub kind: Kind,
    pub confirmed: bool,
    pub running: bool,
    pub reload_pending: bool,
    pub error: Option<String>,
    pub status: String,
}

pub enum ResultMessage {
    Delete {
        source: String,
        result: Result<branches::DeletePreview, String>,
    },
    Tag {
        source: String,
        names: Vec<String>,
        remote: String,
        result: Result<Option<remotes::TagPushPreview>, String>,
    },
    Reset {
        source: String,
        result: Result<branches::ResetPreview, String>,
    },
    Remotes(Result<remote_config::Snapshot, String>),
}

enum LoadRequest {
    Delete(String),
    Tag(String, String),
    Reset(String),
    Remotes,
}

fn tag_details(root: &std::path::Path, source: String, selected: String) -> ResultMessage {
    let names = match remote_config::load(root) {
        Ok(snapshot) => snapshot
            .entries
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>(),
        Err(error) => {
            return ResultMessage::Tag {
                source,
                names: Vec::new(),
                remote: selected,
                result: Err(error),
            }
        }
    };
    let remote = if names.contains(&selected) {
        selected
    } else {
        names.first().cloned().unwrap_or_default()
    };
    let result = if remote.is_empty() {
        Ok(None)
    } else {
        remotes::preview_tag_push(root, &source, &remote).map(Some)
    };
    ResultMessage::Tag {
        source,
        names,
        remote,
        result,
    }
}

impl SannyGitApp {
    pub fn delete_branch_blocker(&self) -> Option<&'static str> {
        self.branch_base_blocker().or_else(|| {
            match self.refs.iter().find(|entry| {
                entry.full_name == self.selected_branch && entry.kind == RefKind::Local
            }) {
                None => Some("Select a local branch"),
                Some(entry) if entry.current => Some("The current branch cannot be deleted"),
                Some(_) => None,
            }
        })
    }

    pub fn push_tag_blocker(&self) -> Option<&'static str> {
        self.branch_base_blocker().or_else(|| {
            (!self
                .refs
                .iter()
                .any(|entry| entry.full_name == self.selected_branch && entry.kind == RefKind::Tag))
            .then_some("Select a tag to push")
        })
    }

    pub fn open_delete_branch(&mut self) {
        if self.delete_branch_blocker().is_some() {
            return;
        }
        self.open_ref_dialog(Kind::Delete {
            source: self.selected_branch.clone(),
            prepared: None,
        });
    }

    pub fn open_push_tag(&mut self) {
        if self.push_tag_blocker().is_some() {
            return;
        }
        self.open_tag_push_to(self.selected_branch.clone(), String::new());
    }

    fn open_tag_push_to(&mut self, source: String, remote: String) {
        self.open_ref_dialog(Kind::Tag {
            source,
            names: Vec::new(),
            remote,
            prepared: None,
        });
    }

    pub fn push_selected_tag(&mut self) {
        if self.branch_base_blocker().is_some() {
            return;
        }
        let Some(tag) = self
            .refs
            .iter()
            .find(|entry| entry.full_name == self.selected_branch && entry.kind == RefKind::Tag)
        else {
            return;
        };
        let source = tag.full_name.clone();
        let oid = tag.hash.clone();
        let root = self.repositories[self.selected_repository].path.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        self.error = None;
        if self.start_worker(move || {
            let path = PathBuf::from(&root);
            let outcome = match remotes::load(&path) {
                Ok(state) => {
                    let remote = match state.remotes.as_slice() {
                        [only] => Some(only),
                        names => names.iter().find(|name| *name == "origin"),
                    };
                    let Some(remote) = remote else {
                        let preferred_remote = (!state.remotes.is_empty())
                            .then(|| state.upstream_or_origin().unwrap_or_default().to_owned());
                        return WorkerResult::TagPushSelection {
                            root,
                            source,
                            preferred_remote,
                        };
                    };
                    remotes::preview_tag_push(&path, &source, remote).and_then(|preview| {
                        if preview.oid != oid {
                            return Err(
                                "The selected tag changed. Refresh branches and retry.".into()
                            );
                        }
                        remotes::push_tag(&path, &preview, &worker_cancel)
                    })
                }
                Err(error) => Err(error),
            };
            WorkerResult::Operation {
                committed: false,
                success: "Tag pushed",
                outcome,
                snapshot: git::load(&path),
            }
        }) {
            self.manager.cancel = Some(cancel);
            self.status = "Pushing tag…".into();
        }
    }

    pub(super) fn apply_tag_push_selection(
        &mut self,
        root: String,
        source: String,
        preferred_remote: Option<String>,
    ) {
        if self
            .manager
            .cancel
            .take()
            .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
        {
            self.status = "Tag push cancelled".into();
            return;
        }
        if !self
            .repositories
            .get(self.selected_repository)
            .is_some_and(|repo| repo.path == root)
        {
            return;
        }
        self.status = match &preferred_remote {
            Some(_) => "Choose a remote for the tag",
            None => "No configured remotes",
        }
        .into();
        if let Some(remote) = preferred_remote {
            self.open_tag_push_to(source, remote);
        } else {
            self.open_ref_dialog(Kind::AddTagRemote);
        }
    }

    pub fn open_remote_manager(&mut self) {
        if !self.remote_allowed() {
            return;
        }
        self.open_ref_dialog(Kind::Remotes(RemoteEditor {
            snapshot: None,
            current: false,
            selected: None,
            name: String::new(),
            url: String::new(),
            push_url: String::new(),
            confirm_remove: false,
            keep_draft: false,
        }));
    }

    pub fn tag_reset_allowed(&self) -> bool {
        self.branch_head_exists && !self.merging && self.branch_base_blocker().is_none()
    }

    pub fn open_tag_reset(&mut self, source: String) {
        self.open_tag_reset_dialog(source, None);
    }

    pub fn open_advanced_tag_reset(&mut self, source: String) {
        self.open_tag_reset_dialog(
            source,
            Some(branches::AdvancedReset {
                strategy: self.last_reset_strategy,
                fix_line_endings: false,
            }),
        );
    }

    fn open_tag_reset_dialog(&mut self, source: String, advanced: Option<branches::AdvancedReset>) {
        if !self.tag_reset_allowed()
            || !self
                .refs
                .iter()
                .any(|entry| entry.kind == RefKind::Tag && entry.full_name == source)
        {
            return;
        }
        self.open_ref_dialog(Kind::Reset {
            source,
            prepared: None,
            choose_changes: false,
            advanced,
        });
    }

    fn open_ref_dialog(&mut self, kind: Kind) {
        let reload_pending = !matches!(kind, Kind::AddTagRemote);
        self.ref_dialog = Some(Dialog {
            root: self.repositories[self.selected_repository].path.clone(),
            kind,
            confirmed: false,
            running: false,
            reload_pending,
            error: None,
            status: "Loading details…".into(),
        });
        self.error = None;
        self.load_ref_pending();
    }

    pub fn reload_ref_dialog(&mut self) {
        if self.busy() {
            return;
        }
        if let Some(dialog) = &mut self.ref_dialog {
            dialog.confirmed = false;
            dialog.error = None;
            dialog.reload_pending = true;
            dialog.status = "Loading details…".into();
            match &mut dialog.kind {
                Kind::Delete { prepared, .. } => *prepared = None,
                Kind::Tag { prepared, .. } => *prepared = None,
                Kind::Reset {
                    prepared,
                    choose_changes,
                    ..
                } => {
                    *prepared = None;
                    *choose_changes = false;
                }
                Kind::AddTagRemote => dialog.reload_pending = false,
                Kind::Remotes(editor) => {
                    editor.current = false;
                    editor.confirm_remove = false;
                    editor.keep_draft = false;
                }
            }
        }
        self.load_ref_pending();
    }

    pub(super) fn load_ref_pending(&mut self) {
        if self.busy() {
            return;
        }
        let Some(dialog) = &mut self.ref_dialog else {
            return;
        };
        if !dialog.reload_pending {
            return;
        }
        dialog.reload_pending = false;
        let root = dialog.root.clone();
        let target = match &dialog.kind {
            Kind::Delete { source, .. } => LoadRequest::Delete(source.clone()),
            Kind::Tag { source, remote, .. } => LoadRequest::Tag(source.clone(), remote.clone()),
            Kind::Reset { source, .. } => LoadRequest::Reset(source.clone()),
            Kind::AddTagRemote => return,
            Kind::Remotes(_) => LoadRequest::Remotes,
        };
        if !self.start_worker(move || {
            let path = PathBuf::from(&root);
            let result = match target {
                LoadRequest::Delete(source) => ResultMessage::Delete {
                    result: branches::preview_delete(&path, &source),
                    source,
                },
                LoadRequest::Tag(source, remote) => tag_details(&path, source, remote),
                LoadRequest::Reset(source) => ResultMessage::Reset {
                    result: branches::preview_reset(&path, &source),
                    source,
                },
                LoadRequest::Remotes => ResultMessage::Remotes(remote_config::load(&path)),
            };
            WorkerResult::RefDetails {
                root,
                result: Box::new(result),
            }
        }) {
            self.ref_worker_failed();
        }
    }

    pub fn run_ref_action(&mut self) {
        if self.busy() || !self.state_current {
            return;
        }
        let Some(dialog) = &mut self.ref_dialog else {
            return;
        };
        if !dialog.confirmed
            || dialog.running
            || !self
                .repositories
                .get(self.selected_repository)
                .is_some_and(|repo| repo.path == dialog.root)
        {
            return;
        }
        let root = PathBuf::from(&dialog.root);
        let (deletion, tag) = match &dialog.kind {
            Kind::Delete {
                prepared: Some(prepared),
                ..
            } => (Some(prepared.clone()), None),
            Kind::Tag {
                prepared: Some(prepared),
                remote,
                ..
            } if prepared.remote == *remote => (None, Some(prepared.clone())),
            _ => return,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        if tag.is_some() {
            self.manager.cancel = Some(cancel.clone());
        }
        dialog.running = true;
        dialog.error = None;
        self.state_current = false;
        self.error = None;
        self.status = "Ref operation in progress…".into();
        if !self.start_worker(move || {
            let (outcome, success) = if let Some(prepared) = deletion {
                (branches::delete(&root, &prepared), "Branch deleted")
            } else if let Some(prepared) = tag {
                (remotes::push_tag(&root, &prepared, &cancel), "Tag pushed")
            } else {
                (
                    Err("Reload details before retrying.".into()),
                    "Ref operation",
                )
            };
            WorkerResult::Operation {
                committed: false,
                success,
                outcome,
                snapshot: git::load(&root),
            }
        }) {
            self.manager.cancel = None;
            self.ref_worker_failed();
        }
    }

    pub fn confirm_tag_reset(&mut self, mode: branches::ResetMode) {
        if self.busy() || !self.state_current {
            return;
        }
        let Some(dialog) = &mut self.ref_dialog else {
            return;
        };
        if dialog.running
            || !self
                .repositories
                .get(self.selected_repository)
                .is_some_and(|repo| repo.path == dialog.root)
        {
            return;
        }
        let Kind::Reset {
            prepared: Some(prepared),
            choose_changes,
            advanced,
            ..
        } = &mut dialog.kind
        else {
            return;
        };
        if mode != branches::ResetMode::Clean && !*choose_changes
            || mode == branches::ResetMode::Clean && *choose_changes
        {
            return;
        }
        if let Some(options) = advanced {
            self.last_reset_strategy = options.strategy;
        }
        let hard = advanced.is_none_or(|options| options.strategy == branches::ResetStrategy::Hard);
        if hard && mode == branches::ResetMode::Clean && prepared.has_tracked_changes {
            *choose_changes = true;
            return;
        }
        let root = PathBuf::from(&dialog.root);
        let prepared = prepared.clone();
        let advanced = *advanced;
        dialog.running = true;
        dialog.error = None;
        self.state_current = false;
        self.error = None;
        self.status = "Resetting HEAD…".into();
        if !self.start_worker(move || WorkerResult::Operation {
            committed: false,
            success: "Reset complete",
            outcome: branches::reset_tag(&root, &prepared, mode, advanced),
            snapshot: git::load(&root),
        }) {
            self.state_current = true;
            self.ref_worker_failed();
        }
    }

    pub fn change_remote(&mut self, remove: bool) {
        if self.busy() || !self.state_current {
            return;
        }
        let Some(dialog) = &mut self.ref_dialog else {
            return;
        };
        if !self
            .repositories
            .get(self.selected_repository)
            .is_some_and(|repo| repo.path == dialog.root)
        {
            return;
        }
        let Kind::Remotes(editor) = &mut dialog.kind else {
            return;
        };
        if !editor.current || dialog.running || (remove && !editor.confirm_remove) {
            return;
        }
        let Some(snapshot) = editor.snapshot.clone() else {
            return;
        };
        let name = editor.selected.as_ref().unwrap_or(&editor.name).clone();
        let change = if remove {
            if editor.selected.is_none() {
                return;
            }
            remote_config::Change::Remove { name }
        } else if editor.selected.is_some() {
            remote_config::Change::Update {
                name,
                url: editor.url.clone(),
                push_url: editor.push_url.clone(),
            }
        } else {
            remote_config::Change::Add {
                name,
                url: editor.url.clone(),
                push_url: editor.push_url.clone(),
            }
        };
        let root = PathBuf::from(&dialog.root);
        editor.current = false;
        editor.confirm_remove = false;
        dialog.running = true;
        dialog.error = None;
        self.state_current = false;
        self.error = None;
        self.status = "Updating remote configuration…".into();
        if !self.start_worker(move || WorkerResult::Operation {
            committed: false,
            success: "Remote configuration updated",
            outcome: remote_config::change(&root, &snapshot, &change),
            snapshot: git::load(&root),
        }) {
            self.ref_worker_failed();
        }
    }

    pub(super) fn apply_ref_details(&mut self, root: String, result: ResultMessage) {
        let Some(dialog) = self
            .ref_dialog
            .as_mut()
            .filter(|dialog| dialog.root == root)
        else {
            return;
        };
        match (&mut dialog.kind, result) {
            (
                Kind::Delete { source, prepared },
                ResultMessage::Delete {
                    source: requested,
                    result,
                },
            ) if *source == requested => match result {
                Ok(details) => {
                    *prepared = Some(details);
                    dialog.status = "Merged into current HEAD. No force deletion; working files and the index are kept.".into();
                }
                Err(error) => dialog.error = Some(error),
            },
            (
                Kind::Tag {
                    source,
                    names,
                    remote,
                    prepared,
                },
                ResultMessage::Tag {
                    source: requested,
                    names: loaded,
                    remote: selected,
                    result,
                },
            ) if *source == requested => {
                *names = loaded;
                *remote = selected;
                match result {
                    Ok(details) => {
                        *prepared = details;
                        dialog.status = if remote.is_empty() { "No configured remotes. Add one in Remote → Manage Remotes." } else { "Only this tag object will be pushed. Existing remote tags are not replaced." }.into();
                    }
                    Err(error) => dialog.error = Some(error),
                }
            }
            (Kind::Remotes(editor), ResultMessage::Remotes(result)) => match result {
                Ok(snapshot) => {
                    let selected = editor.selected.clone();
                    editor.snapshot = Some(snapshot);
                    editor.current = true;
                    if !editor.keep_draft {
                        editor.select(selected);
                    }
                    editor.keep_draft = false;
                    dialog.status.clear();
                }
                Err(error) => {
                    editor.current = false;
                    dialog.error = Some(error);
                }
            },
            (
                Kind::Reset {
                    source, prepared, ..
                },
                ResultMessage::Reset {
                    source: requested,
                    result,
                },
            ) if *source == requested => match result {
                Ok(details) => *prepared = Some(details),
                Err(error) => dialog.error = Some(error),
            },
            _ => {}
        }
    }

    pub(super) fn finish_ref_operation(&mut self, success: &str, succeeded: bool) {
        if success == "Tag pushed" {
            self.manager.cancel = None;
        }
        if success == "Remote configuration updated" {
            self.tracking.branch_changed();
            self.invalidate_repository_hosting();
            if let Some(dialog) = &mut self.ref_dialog {
                if let Kind::Remotes(editor) = &mut dialog.kind {
                    editor.keep_draft = !succeeded;
                    if succeeded && editor.selected.is_none() {
                        editor.selected = Some(editor.name.clone());
                    }
                }
                dialog.running = false;
                dialog.error = self.error.clone();
                dialog.reload_pending = true;
            }
        } else if succeeded {
            self.ref_dialog = None;
        } else {
            self.ref_worker_failed();
        }
    }

    pub(super) fn ref_worker_failed(&mut self) {
        if let Some(dialog) = &mut self.ref_dialog {
            dialog.running = false;
            dialog.confirmed = false;
            dialog.error = self.error.clone();
            match &mut dialog.kind {
                Kind::Delete { prepared, .. } => *prepared = None,
                Kind::Tag { prepared, .. } => *prepared = None,
                Kind::Reset {
                    prepared,
                    choose_changes,
                    ..
                } => {
                    *prepared = None;
                    *choose_changes = false;
                }
                Kind::AddTagRemote => {}
                Kind::Remotes(editor) => {
                    editor.current = false;
                    editor.confirm_remove = false;
                }
            }
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
            app.load_ref_pending();
            if !app.busy() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn opened() -> (TempRepo, SannyGitApp) {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        git::commit(&repo.0, "Base", git::CommitContents::All).unwrap();
        repo.git(&["branch", "merged"]);
        repo.git(&["tag", "chosen"]);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(git::load(&repo.0).unwrap());
        let refs = branches::load(&repo.0).unwrap();
        app.branch_head_exists = refs.has_head;
        app.refs = refs.refs;
        app.refs_current = true;
        app.commit_message = "Keep commit draft".into();
        (repo, app)
    }

    #[test]
    fn advanced_tag_reset_remembers_the_confirmed_strategy() {
        let (repo, mut app) = opened();
        repo.git(&["commit", "--allow-empty", "-m", "before reset"]);
        repo.write("file", "dirty\n");
        app.open_advanced_tag_reset("refs/tags/chosen".into());
        wait(&mut app);
        let Kind::Reset {
            advanced: Some(options),
            ..
        } = &mut app.ref_dialog.as_mut().unwrap().kind
        else {
            panic!("Expected Reset Advanced");
        };
        assert_eq!(options.strategy, branches::ResetStrategy::Mixed);
        options.strategy = branches::ResetStrategy::Soft;
        app.confirm_tag_reset(branches::ResetMode::Clean);
        assert!(app.busy());
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.ref_dialog.is_none());
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"dirty\n");

        app.refs_current = true;
        app.open_advanced_tag_reset("refs/tags/chosen".into());
        wait(&mut app);
        let Kind::Reset {
            advanced: Some(options),
            ..
        } = &app.ref_dialog.as_ref().unwrap().kind
        else {
            panic!("Expected Reset Advanced");
        };
        assert_eq!(options.strategy, branches::ResetStrategy::Soft);
    }

    #[test]
    fn tag_reset_waits_for_the_dirty_worktree_decision() {
        let (repo, mut app) = opened();
        repo.git(&["commit", "--allow-empty", "-m", "before reset"]);
        let before = repo.git(&["rev-parse", "HEAD"]);
        repo.write("file", "dirty\n");
        app.open_tag_reset("refs/tags/chosen".into());
        wait(&mut app);
        app.confirm_tag_reset(branches::ResetMode::Discard);
        assert!(!app.busy());
        app.confirm_tag_reset(branches::ResetMode::Clean);
        assert!(!app.busy());
        assert!(matches!(
            app.ref_dialog.as_ref().unwrap().kind,
            Kind::Reset {
                choose_changes: true,
                ..
            }
        ));
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"dirty\n");

        app.confirm_tag_reset(branches::ResetMode::Discard);
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.ref_dialog.is_none());
        assert_eq!(app.status, "Reset complete");
        assert_eq!(
            repo.git(&["rev-parse", "HEAD"]),
            repo.git(&["rev-parse", "refs/tags/chosen"])
        );
        assert_eq!(app.commit_message, "Keep commit draft");
    }

    #[test]
    fn branch_deletion_needs_confirmation_and_errors_reset_it_without_losing_drafts() {
        let (repo, mut app) = opened();
        app.selected_branch = "refs/heads/merged".into();
        app.open_delete_branch();
        wait(&mut app);
        app.run_ref_action();
        assert!(!app.busy());
        assert!(repo.git(&["branch", "--list", "merged"]).contains("merged"));
        repo.git(&["commit", "--allow-empty", "-m", "External HEAD"]);
        app.ref_dialog.as_mut().unwrap().confirmed = true;
        app.run_ref_action();
        wait(&mut app);
        assert!(app.error.as_deref().unwrap().contains("changed"));
        assert!(!app.ref_dialog.as_ref().unwrap().confirmed);
        assert_eq!(app.commit_message, "Keep commit draft");
        app.reload_ref_dialog();
        wait(&mut app);
        app.ref_dialog.as_mut().unwrap().confirmed = true;
        app.run_ref_action();
        wait(&mut app);
        assert!(app.error.is_none());
        assert!(app.ref_dialog.is_none());
        assert!(repo.git(&["branch", "--list", "merged"]).is_empty());
        assert_eq!(app.status, "Branch deleted");
        assert_eq!(app.commit_message, "Keep commit draft");
    }

    #[test]
    fn remote_editor_adds_updates_and_requires_removal_confirmation_while_preserving_failed_input()
    {
        let (repo, mut app) = opened();
        app.open_remote_manager();
        wait(&mut app);
        if let Kind::Remotes(editor) = &mut app.ref_dialog.as_mut().unwrap().kind {
            editor.name = "origin".into();
            editor.url = "https://example.test/repo.git".into();
        }
        app.change_remote(false);
        wait(&mut app);
        assert!(app.error.is_none());
        if let Kind::Remotes(editor) = &mut app.ref_dialog.as_mut().unwrap().kind {
            assert_eq!(editor.selected.as_deref(), Some("origin"));
            editor.url = "https://dummy-token@example.test/repo".into();
        }
        app.change_remote(false);
        wait(&mut app);
        assert!(app.error.is_some());
        if let Kind::Remotes(editor) = &mut app.ref_dialog.as_mut().unwrap().kind {
            assert!(editor.url.contains("dummy-token"));
            editor.url = "ssh://git@example.test/new.git".into();
        }
        app.change_remote(false);
        wait(&mut app);
        assert!(app.error.is_none());
        app.change_remote(true);
        assert!(!app.busy());
        assert!(repo.git(&["remote"]).contains("origin"));
        if let Kind::Remotes(editor) = &mut app.ref_dialog.as_mut().unwrap().kind {
            editor.confirm_remove = true;
        }
        app.change_remote(true);
        wait(&mut app);
        assert!(app.error.is_none());
        assert!(repo.git(&["remote"]).is_empty());
        assert_eq!(app.commit_message, "Keep commit draft");
    }

    #[test]
    fn tag_details_keep_other_remote_choices_when_the_default_url_is_unsupported() {
        let (repo, mut app) = opened();
        repo.git(&[
            "remote",
            "add",
            "a-bad",
            "https://dummy-token@example.test/repo",
        ]);
        let remote = repo.0.join("remote.git");
        repo.git(&["init", "--bare", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", "z-good", remote.to_str().unwrap()]);
        app.selected_branch = "refs/tags/chosen".into();
        app.open_push_tag();
        wait(&mut app);
        assert!(app.ref_dialog.as_ref().unwrap().error.is_some());
        if let Kind::Tag { names, remote, .. } = &mut app.ref_dialog.as_mut().unwrap().kind {
            assert_eq!(names.len(), 2);
            *remote = "z-good".into();
        }
        app.reload_ref_dialog();
        wait(&mut app);
        assert!(app.ref_dialog.as_ref().unwrap().error.is_none());
        app.run_ref_action();
        assert!(!app.busy());
        app.ref_dialog.as_mut().unwrap().confirmed = true;
        app.run_ref_action();
        wait(&mut app);
        assert!(app.ref_dialog.is_none());
        assert!(app.error.is_none());
        assert_eq!(app.status, "Tag pushed");
        assert!(git::text(git::run(&remote, &["tag", "--list"]).unwrap())
            .unwrap()
            .contains("chosen"));
        assert_eq!(app.commit_message, "Keep commit draft");
    }

    #[test]
    fn direct_tag_push_prefers_origin_and_can_use_a_sole_remote() {
        let (repo, mut app) = opened();
        let origin = repo.0.join("origin.git");
        let team = repo.0.join("team.git");
        for (name, path) in [("origin", &origin), ("team", &team)] {
            repo.git(&["init", "--bare", path.to_str().unwrap()]);
            repo.git(&["remote", "add", name, path.to_str().unwrap()]);
        }
        repo.git(&["push", "-u", "team", "HEAD"]);
        repo.git(&["config", "remote.pushDefault", "team"]);
        repo.git(&["tag", "other"]);
        app.selected_branch = "refs/tags/chosen".into();

        app.push_selected_tag();
        assert!(app.busy());
        assert!(app.ref_dialog.is_none());
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.ref_dialog.is_none());
        assert_eq!(app.status, "Tag pushed");
        assert_eq!(
            git::text(git::run(&origin, &["for-each-ref", "--format=%(refname)"]).unwrap())
                .unwrap()
                .trim(),
            "refs/tags/chosen"
        );
        assert!(git::run(&team, &["tag", "--list"]).unwrap().is_empty());

        repo.git(&["remote", "remove", "origin"]);
        app.refs_current = true;
        app.push_selected_tag();
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.ref_dialog.is_none());
        assert_eq!(
            git::text(git::run(&team, &["tag", "--list"]).unwrap())
                .unwrap()
                .trim(),
            "chosen"
        );
        assert_eq!(app.commit_message, "Keep commit draft");
    }

    #[test]
    fn direct_tag_push_requests_configuration_or_remote_choice_when_needed() {
        let (repo, mut app) = opened();
        app.selected_branch = "refs/tags/chosen".into();
        app.push_selected_tag();
        wait(&mut app);
        assert!(matches!(
            app.ref_dialog.as_ref().unwrap().kind,
            Kind::AddTagRemote
        ));
        app.ref_dialog = None;

        let remote = repo.0.join("team.git");
        repo.git(&["init", "--bare", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", "team", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", "archive", remote.to_str().unwrap()]);
        repo.git(&["push", "-u", "team", "HEAD"]);
        app.refs_current = true;
        app.push_selected_tag();
        wait(&mut app);
        let dialog = app.ref_dialog.as_ref().unwrap();
        let Kind::Tag {
            remote: selected,
            prepared,
            ..
        } = &dialog.kind
        else {
            panic!("Expected the tag destination dialog");
        };
        assert_eq!(selected, "team");
        assert_eq!(prepared.as_ref().unwrap().full_name, "refs/tags/chosen");
        assert!(!dialog.confirmed);
        assert!(git::run(&remote, &["tag", "--list"]).unwrap().is_empty());
    }

    #[test]
    fn add_tag_and_push_sends_only_the_new_tag_to_the_upstream_remote() {
        let (repo, mut app) = opened();
        let remote = repo.0.join("remote.git");
        repo.git(&["init", "--bare", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", "origin", "https://example.test/repo"]);
        repo.git(&["remote", "add", "team", remote.to_str().unwrap()]);
        repo.git(&["push", "-u", "team", "HEAD"]);
        app.tag_dialog_open = true;
        app.new_tag_name = "release".into();
        app.new_tag_message = "Release notes".into();
        app.create_tag(true);
        assert!(!app.tag_dialog_open);
        assert!(app.busy());
        wait(&mut app);
        assert!(app.error.is_none());
        assert_eq!(
            repo.git(&["cat-file", "-t", "refs/tags/release"]).trim(),
            "tag"
        );
        let remote_tags = || git::text(git::run(&remote, &["tag", "--list"]).unwrap()).unwrap();
        assert!(app.ref_dialog.is_none());
        assert_eq!(remote_tags().trim(), "release");
    }

    #[test]
    fn add_tag_and_push_requires_an_upstream_or_origin() {
        let (repo, mut app) = opened();
        let remote = repo.0.join("remote.git");
        repo.git(&["init", "--bare", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", "team", remote.to_str().unwrap()]);
        app.tag_dialog_open = true;
        app.new_tag_name = "release".into();
        app.create_tag(true);
        wait(&mut app);
        assert!(app
            .error
            .as_deref()
            .unwrap()
            .contains("Configure an upstream"));
        assert!(!app.tag_dialog_open);
        assert_eq!(app.new_tag_name, "release");
        assert!(repo.git(&["tag", "--list", "release"]).is_empty());
        assert!(git::text(git::run(&remote, &["tag", "--list"]).unwrap())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn closed_dialogs_and_other_repositories_ignore_late_details_and_actions() {
        let (repo, mut app) = opened();
        app.open_remote_manager();
        wait(&mut app);
        app.selected_repository = usize::MAX;
        if let Kind::Remotes(editor) = &mut app.ref_dialog.as_mut().unwrap().kind {
            editor.name = "unsafe".into();
            editor.url = "https://example.test/repo".into();
        }
        app.change_remote(false);
        assert!(!app.busy());
        assert!(repo.git(&["remote"]).is_empty());
        app.apply_ref_details(
            "/another-repository".into(),
            ResultMessage::Remotes(Err("Late error".into())),
        );
        assert!(app.ref_dialog.as_ref().unwrap().error.is_none());
        let root = app.ref_dialog.as_ref().unwrap().root.clone();
        app.ref_dialog = None;
        app.apply_ref_details(root, ResultMessage::Remotes(Err("Late error".into())));
        assert!(app.ref_dialog.is_none());
    }
}
