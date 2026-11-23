use super::{SannyGitApp, WorkerResult};
use crate::git::GitContext;
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
    Branch {
        source: String,
        prepared: Option<remotes::BranchPreview>,
        action: BranchAction,
    },
    Delete {
        source: String,
        prepared: Option<branches::DeletePreview>,
    },
    DeleteTag {
        source: String,
        prepared: Option<remotes::TagDeletePreview>,
        also_remote: bool,
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

#[derive(Clone)]
pub enum BranchAction {
    Delete {
        also_remote: bool,
    },
    Push {
        custom: bool,
        destination: String,
    },
    PushLocal {
        remote: String,
        custom: bool,
        destination: String,
        choose_destination: bool,
    },
    SetTracking {
        upstream: String,
    },
    StopTracking,
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
    Branch {
        source: String,
        result: Result<remotes::BranchPreview, String>,
    },
    Delete {
        source: String,
        result: Result<branches::DeletePreview, String>,
    },
    DeleteTag {
        source: String,
        result: Result<remotes::TagDeletePreview, String>,
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
    Branch(String),
    Delete(String),
    DeleteTag(String),
    Tag(String, String),
    Reset(String),
    Remotes,
}

enum RefAction {
    Branch(remotes::BranchPreview, BranchAction),
    DeleteBranch(branches::DeletePreview),
    DeleteTag(remotes::TagDeletePreview, bool),
    PushTag(remotes::TagPushPreview),
}

fn tag_details(
    git: &GitContext,
    root: &std::path::Path,
    source: String,
    selected: String,
) -> ResultMessage {
    let names = match remote_config::load(git, root) {
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
        remotes::preview_tag_push(git, root, &source, &remote).map(Some)
    };
    ResultMessage::Tag {
        source,
        names,
        remote,
        result,
    }
}

pub struct BranchState {
    pub entries: Vec<crate::models::BranchRef>,
    pub current: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub(super) pending: bool,
    pub(super) worker: Option<std::sync::mpsc::Receiver<WorkerResult>>,
    pub(super) tag_message_reader: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    pub(super) tag_patch_reader:
        Option<std::sync::mpsc::Receiver<Result<std::path::PathBuf, String>>>,
    pub exported_tag_patch: Option<std::path::PathBuf>,
    pub branch_head_exists: bool,
    pub branch_dialog_open: bool,
    pub rename_branch_dialog: Option<super::RenameBranchDialog>,
    pub tag_dialog_open: bool,
    pub(super) last_reset_strategy: crate::branches::ResetStrategy,
    pub remote_checkout: Option<super::RemoteCheckoutDialog>,
    pub ref_dialog: Option<Dialog>,
    pub new_branch_name: String,
    pub new_tag_name: String,
    pub new_tag_message: String,
    pub switch_to_new_branch: bool,
    pub selected_branch: String,
    pub selected_ref_group: Option<crate::models::RefKind>,
    pub current_branch: String,
    /// Full HEAD commit id. Empty when the branch has no commits yet.
    pub(super) head_commit: String,
}

impl Default for BranchState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            current: false,
            loading: false,
            error: None,
            pending: false,
            worker: None,
            tag_message_reader: None,
            tag_patch_reader: None,
            exported_tag_patch: None,
            branch_head_exists: false,
            branch_dialog_open: false,
            rename_branch_dialog: None,
            tag_dialog_open: false,
            last_reset_strategy: Default::default(),
            remote_checkout: None,
            ref_dialog: None,
            new_branch_name: String::new(),
            new_tag_name: String::new(),
            new_tag_message: String::new(),
            switch_to_new_branch: true,
            selected_branch: String::new(),
            selected_ref_group: None,
            current_branch: String::new(),
            head_commit: String::new(),
        }
    }
}

impl BranchState {
    fn apply(&mut self, result: Result<crate::models::RefSnapshot, String>) {
        self.loading = false;
        match result {
            Ok(snapshot) => {
                self.entries = snapshot.refs;
                self.branch_head_exists = snapshot.has_head;
                self.current = true;
                self.error = None;
                if self.selected_ref_group.is_some_and(|kind| {
                    kind != crate::models::RefKind::Tag
                        && !self.entries.iter().any(|r| r.kind == kind)
                }) {
                    self.selected_ref_group = None;
                }
                if self.selected_ref_group.is_none()
                    && !self
                        .entries
                        .iter()
                        .any(|r| r.full_name == self.selected_branch)
                {
                    self.selected_branch = self
                        .entries
                        .iter()
                        .find(|r| r.current)
                        .map(|r| r.full_name.clone())
                        .unwrap_or_default();
                }
            }
            Err(error) => {
                self.current = false;
                self.error = Some(error);
            }
        }
    }
}

impl SannyGitApp {
    pub(super) fn apply_refs(
        &mut self,
        path: String,
        result: Result<crate::models::RefSnapshot, String>,
    ) {
        if self
            .repositories
            .get(self.selected_repository)
            .is_some_and(|r| r.path == path)
        {
            self.branches.apply(result);
        }
    }

    pub fn delete_branch_blocker(&self) -> Option<&'static str> {
        self.branch_base_blocker().or_else(|| {
            match self.branches.entries.iter().find(|entry| {
                entry.full_name == self.branches.selected_branch && entry.kind == RefKind::Local
            }) {
                None => Some("Select a local branch"),
                Some(entry) if entry.current => Some("The current branch cannot be deleted"),
                Some(_) => None,
            }
        })
    }

    pub fn push_tag_blocker(&self) -> Option<&'static str> {
        self.branch_base_blocker().or_else(|| {
            (!self.branches.entries.iter().any(|entry| {
                entry.full_name == self.branches.selected_branch && entry.kind == RefKind::Tag
            }))
            .then_some("Select a tag to push")
        })
    }

    pub fn open_delete_branch(&mut self) {
        if self.delete_branch_blocker().is_some() {
            return;
        }
        self.open_ref_dialog(Kind::Delete {
            source: self.branches.selected_branch.clone(),
            prepared: None,
        });
    }

    pub fn open_delete_tag(&mut self) {
        if self.push_tag_blocker().is_none() {
            self.open_ref_dialog(Kind::DeleteTag {
                source: self.branches.selected_branch.clone(),
                prepared: None,
                also_remote: false,
            });
        }
    }

    pub fn open_push_tag(&mut self) {
        if self.push_tag_blocker().is_some() {
            return;
        }
        self.open_tag_push_to(self.branches.selected_branch.clone(), String::new());
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
        let Some(tag) = self.branches.entries.iter().find(|entry| {
            entry.full_name == self.branches.selected_branch && entry.kind == RefKind::Tag
        }) else {
            return;
        };
        let source = tag.full_name.clone();
        let oid = tag.hash.clone();
        let root = self.repositories[self.selected_repository].path.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        self.error = None;
        if self.start_worker(move |git| {
            let path = PathBuf::from(&root);
            let outcome = match remotes::load(git, &path) {
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
                    remotes::preview_tag_push(git, &path, &source, remote).and_then(|preview| {
                        if preview.oid != oid {
                            return Err(
                                "The selected tag changed. Refresh branches and retry.".into()
                            );
                        }
                        remotes::push_tag(git, &path, &preview, &worker_cancel)
                    })
                }
                Err(error) => Err(error),
            };
            WorkerResult::Operation {
                committed: false,
                success: "Tag pushed",
                outcome,
                snapshot: git::load(git, &path),
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

    pub fn ref_reset_allowed(&self) -> bool {
        self.branches.branch_head_exists && !self.merging && self.branch_base_blocker().is_none()
    }

    pub fn open_ref_reset(&mut self, source: String) {
        self.open_ref_reset_dialog(source, None);
    }

    pub fn open_advanced_ref_reset(&mut self, source: String) {
        self.open_ref_reset_dialog(
            source,
            Some(branches::AdvancedReset {
                strategy: self.branches.last_reset_strategy,
                fix_line_endings: false,
            }),
        );
    }

    fn open_ref_reset_dialog(&mut self, source: String, advanced: Option<branches::AdvancedReset>) {
        if !self.ref_reset_allowed()
            || !self
                .branches
                .entries
                .iter()
                .any(|entry| entry.full_name == source)
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

    pub fn open_branch_action(&mut self, source: String, action: BranchAction) {
        if self.branch_base_blocker().is_some()
            || !self.branches.entries.iter().any(|entry| {
                entry.full_name == source
                    && match &action {
                        BranchAction::Push { .. } | BranchAction::Delete { .. } => {
                            entry.kind == RefKind::Remote
                        }
                        _ => entry.kind == RefKind::Local,
                    }
            })
        {
            return;
        }
        self.open_ref_dialog(Kind::Branch {
            source,
            prepared: None,
            action,
        });
    }

    fn open_ref_dialog(&mut self, kind: Kind) {
        let reload_pending = !matches!(kind, Kind::AddTagRemote);
        self.branches.ref_dialog = Some(Dialog {
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
        if let Some(dialog) = &mut self.branches.ref_dialog {
            dialog.confirmed = false;
            dialog.error = None;
            dialog.reload_pending = true;
            dialog.status = "Loading details…".into();
            match &mut dialog.kind {
                Kind::Delete { prepared, .. } => *prepared = None,
                Kind::Branch { prepared, .. } => *prepared = None,
                Kind::DeleteTag {
                    prepared,
                    also_remote,
                    ..
                } => {
                    *prepared = None;
                    *also_remote = false;
                }
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
        let Some(dialog) = &mut self.branches.ref_dialog else {
            return;
        };
        if !dialog.reload_pending {
            return;
        }
        dialog.reload_pending = false;
        let root = dialog.root.clone();
        let target = match &dialog.kind {
            Kind::Delete { source, .. } => LoadRequest::Delete(source.clone()),
            Kind::Branch { source, .. } => LoadRequest::Branch(source.clone()),
            Kind::DeleteTag { source, .. } => LoadRequest::DeleteTag(source.clone()),
            Kind::Tag { source, remote, .. } => LoadRequest::Tag(source.clone(), remote.clone()),
            Kind::Reset { source, .. } => LoadRequest::Reset(source.clone()),
            Kind::AddTagRemote => return,
            Kind::Remotes(_) => LoadRequest::Remotes,
        };
        if !self.start_worker(move |git| {
            let path = PathBuf::from(&root);
            let result = match target {
                LoadRequest::Branch(source) => ResultMessage::Branch {
                    result: remotes::preview_branch(git, &path, &source),
                    source,
                },
                LoadRequest::Delete(source) => ResultMessage::Delete {
                    result: branches::preview_delete(git, &path, &source),
                    source,
                },
                LoadRequest::DeleteTag(source) => ResultMessage::DeleteTag {
                    result: remotes::preview_tag_delete(git, &path, &source),
                    source,
                },
                LoadRequest::Tag(source, remote) => tag_details(git, &path, source, remote),
                LoadRequest::Reset(source) => ResultMessage::Reset {
                    result: branches::preview_reset(git, &path, &source),
                    source,
                },
                LoadRequest::Remotes => ResultMessage::Remotes(remote_config::load(git, &path)),
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
        let Some(dialog) = &mut self.branches.ref_dialog else {
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
        let action = match &dialog.kind {
            Kind::Branch {
                prepared: Some(prepared),
                action,
                ..
            } => RefAction::Branch(prepared.clone(), action.clone()),
            Kind::Delete {
                prepared: Some(prepared),
                ..
            } => RefAction::DeleteBranch(prepared.clone()),
            Kind::DeleteTag {
                prepared: Some(prepared),
                also_remote,
                ..
            } => RefAction::DeleteTag(prepared.clone(), *also_remote),
            Kind::Tag {
                prepared: Some(prepared),
                remote,
                ..
            } if prepared.remote == *remote => RefAction::PushTag(prepared.clone()),
            _ => return,
        };
        let remote_branch_progress = match &action {
            RefAction::Branch(_, BranchAction::Push { .. } | BranchAction::PushLocal { .. }) => {
                Some("Pushing branch…")
            }
            RefAction::Branch(_, BranchAction::SetTracking { .. } | BranchAction::StopTracking) => {
                Some("Updating branch tracking…")
            }
            RefAction::Branch(_, BranchAction::Delete { .. }) => Some("Deleting branch…"),
            _ => None,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        if matches!(
            action,
            RefAction::PushTag(_)
                | RefAction::DeleteTag(_, true)
                | RefAction::Branch(
                    _,
                    BranchAction::Push { .. } | BranchAction::PushLocal { .. }
                )
                | RefAction::Branch(_, BranchAction::Delete { also_remote: true })
        ) {
            self.manager.cancel = Some(cancel.clone());
        }
        dialog.running = true;
        dialog.error = None;
        self.state_current = false;
        self.error = None;
        self.status = remote_branch_progress
            .unwrap_or("Ref operation in progress…")
            .into();
        if !self.start_worker(move |git| {
            let (outcome, success) = match action {
                RefAction::Branch(
                    prepared,
                    BranchAction::Push {
                        custom,
                        destination,
                    },
                ) => (
                    remotes::push_branch(
                        git,
                        &root,
                        &prepared,
                        custom.then_some(destination.as_str()),
                        &cancel,
                    ),
                    "Remote branch pushed",
                ),
                RefAction::Branch(
                    prepared,
                    BranchAction::PushLocal {
                        remote,
                        custom,
                        destination,
                        ..
                    },
                ) => (
                    remotes::push_branch_to(
                        git,
                        &root,
                        &prepared,
                        &remote,
                        custom.then_some(destination.as_str()),
                        &cancel,
                    ),
                    "Local branch pushed",
                ),
                RefAction::Branch(prepared, BranchAction::SetTracking { upstream }) => (
                    remotes::update_branch_tracking(git, &root, &prepared, Some(&upstream)),
                    "Branch tracking updated",
                ),
                RefAction::Branch(prepared, BranchAction::StopTracking) => (
                    remotes::update_branch_tracking(git, &root, &prepared, None),
                    "Branch tracking stopped",
                ),
                RefAction::Branch(prepared, BranchAction::Delete { also_remote }) => (
                    remotes::delete_branch(git, &root, &prepared, also_remote, &cancel),
                    "Remote branch deleted",
                ),
                RefAction::DeleteBranch(prepared) => {
                    (branches::delete(git, &root, &prepared), "Branch deleted")
                }
                RefAction::DeleteTag(prepared, also_remote) => (
                    remotes::delete_tag(git, &root, &prepared, also_remote, &cancel),
                    "Tag deleted",
                ),
                RefAction::PushTag(prepared) => (
                    remotes::push_tag(git, &root, &prepared, &cancel),
                    "Tag pushed",
                ),
            };
            WorkerResult::Operation {
                committed: false,
                success,
                outcome,
                snapshot: git::load(git, &root),
            }
        }) {
            self.manager.cancel = None;
            self.ref_worker_failed();
        } else if remote_branch_progress.is_some() {
            self.branches.ref_dialog = None;
        }
    }

    pub fn confirm_tag_reset(&mut self, mode: branches::ResetMode) {
        if self.busy() || !self.state_current {
            return;
        }
        let Some(dialog) = &mut self.branches.ref_dialog else {
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
            self.branches.last_reset_strategy = options.strategy;
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
        if !self.start_worker(move |git| WorkerResult::Operation {
            committed: false,
            success: "Reset complete",
            outcome: branches::reset_tag(git, &root, &prepared, mode, advanced),
            snapshot: git::load(git, &root),
        }) {
            self.state_current = true;
            self.ref_worker_failed();
        }
    }

    pub fn change_remote(&mut self, remove: bool) {
        if self.busy() || !self.state_current {
            return;
        }
        let Some(dialog) = &mut self.branches.ref_dialog else {
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
        if !self.start_worker(move |git| WorkerResult::Operation {
            committed: false,
            success: "Remote configuration updated",
            outcome: remote_config::change(git, &root, &snapshot, &change),
            snapshot: git::load(git, &root),
        }) {
            self.ref_worker_failed();
        }
    }

    pub(super) fn apply_ref_details(&mut self, root: String, result: ResultMessage) {
        let Some(dialog) = self
            .branches
            .ref_dialog
            .as_mut()
            .filter(|dialog| dialog.root == root)
        else {
            return;
        };
        match (&mut dialog.kind, result) {
            (
                Kind::Branch {
                    source,
                    prepared,
                    action,
                },
                ResultMessage::Branch {
                    source: requested,
                    result,
                },
            ) if *source == requested => match result {
                Ok(details) => {
                    match action {
                        BranchAction::PushLocal { remote, .. }
                            if !details.destinations.contains(remote) =>
                        {
                            *remote = details.owner.clone();
                        }
                        BranchAction::SetTracking { upstream }
                            if !details.tracking_refs.contains(upstream) =>
                        {
                            upstream.clear();
                        }
                        BranchAction::Delete { also_remote } if !details.can_delete_remote => {
                            *also_remote = false;
                        }
                        _ => {}
                    }
                    *prepared = Some(details);
                }
                Err(error) => dialog.error = Some(error),
            },
            (
                Kind::DeleteTag {
                    source, prepared, ..
                },
                ResultMessage::DeleteTag {
                    source: requested,
                    result,
                },
            ) if *source == requested => match result {
                Ok(details) => *prepared = Some(details),
                Err(error) => dialog.error = Some(error),
            },
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
        if succeeded
            && matches!(
                success,
                "Branch tracking updated" | "Branch tracking stopped"
            )
        {
            self.tracking.branch_changed();
        }
        if matches!(
            success,
            "Remote branch pushed"
                | "Remote branch deleted"
                | "Local branch pushed"
                | "Branch tracking updated"
                | "Branch tracking stopped"
        ) {
            self.manager.cancel = None;
            return;
        }
        if matches!(success, "Tag pushed" | "Tag deleted") {
            self.manager.cancel = None;
        }
        if success == "Remote configuration updated" {
            self.tracking.branch_changed();
            self.invalidate_repository_hosting();
            if let Some(dialog) = &mut self.branches.ref_dialog {
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
            self.branches.ref_dialog = None;
        } else {
            self.ref_worker_failed();
        }
    }

    pub(super) fn ref_worker_failed(&mut self) {
        if let Some(dialog) = &mut self.branches.ref_dialog {
            dialog.running = false;
            dialog.confirmed = false;
            dialog.error = self.error.clone();
            match &mut dialog.kind {
                Kind::Delete { prepared, .. } => *prepared = None,
                Kind::Branch { prepared, .. } => *prepared = None,
                Kind::DeleteTag {
                    prepared,
                    also_remote,
                    ..
                } => {
                    *prepared = None;
                    *also_remote = false;
                }
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
    use crate::test_support::SYSTEM_GIT;
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
        git::commit(&SYSTEM_GIT, &repo.0, "Base", git::CommitContents::All).unwrap();
        repo.git(&["branch", "merged"]);
        repo.git(&["tag", "chosen"]);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(git::load(&SYSTEM_GIT, &repo.0).unwrap());
        let refs = branches::load(&SYSTEM_GIT, &repo.0).unwrap();
        app.branches.branch_head_exists = refs.has_head;
        app.branches.entries = refs.refs;
        app.branches.current = true;
        app.commit.message = "Keep commit draft".into();
        (repo, app)
    }

    #[test]
    fn remote_branch_operations_close_confirmation_when_the_worker_starts() {
        let (repo, mut app) = opened();
        let remote = repo.0.join("origin.git");
        repo.git(&["init", "--bare", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        let source = "refs/remotes/origin/topic";
        repo.git(&["update-ref", source, "HEAD"]);
        app.branches.entries = branches::load(&SYSTEM_GIT, &repo.0).unwrap().refs;

        for (action, progress, success) in [
            (
                BranchAction::Push {
                    custom: false,
                    destination: String::new(),
                },
                "Pushing branch…",
                "Remote branch pushed",
            ),
            (
                BranchAction::Delete { also_remote: false },
                "Deleting branch…",
                "Remote branch deleted",
            ),
        ] {
            app.branches.current = true;
            app.open_branch_action(source.into(), action);
            wait(&mut app);
            app.run_ref_action();
            assert!(!app.busy());
            app.branches.ref_dialog.as_mut().unwrap().confirmed = true;
            app.run_ref_action();
            assert!(app.busy());
            assert!(app.branches.ref_dialog.is_none());
            assert_eq!(app.status, progress);
            wait(&mut app);
            assert!(app.error.is_none(), "{:?}", app.error);
            assert!(app.branches.ref_dialog.is_none());
            assert_eq!(app.status, success);
        }
    }

    #[test]
    fn delete_tag_waits_for_confirmation_and_keeps_the_commit_draft() {
        let (repo, mut app) = opened();
        app.branches.selected_branch = "refs/tags/chosen".into();
        app.open_delete_tag();
        wait(&mut app);
        app.run_ref_action();
        assert!(!app.busy());
        assert_eq!(repo.git(&["tag", "--list", "chosen"]).trim(), "chosen");
        assert!(matches!(
            app.branches.ref_dialog.as_ref().unwrap().kind,
            Kind::DeleteTag {
                also_remote: false,
                ..
            }
        ));
        app.branches.ref_dialog.as_mut().unwrap().confirmed = true;
        app.run_ref_action();
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.status, "Tag deleted");
        assert!(app.branches.ref_dialog.is_none());
        assert!(repo.git(&["tag", "--list", "chosen"]).is_empty());
        assert_eq!(app.commit.message, "Keep commit draft");
    }

    #[test]
    fn reset_rejects_a_remote_tracking_ref_that_moved_after_preview() {
        let (repo, _) = opened();
        let reference = "refs/remotes/origin/topic";
        repo.git(&["update-ref", reference, "HEAD"]);
        repo.git(&["commit", "--allow-empty", "-m", "before reset"]);
        let head = repo.git(&["rev-parse", "HEAD"]);
        let prepared = branches::preview_reset(&SYSTEM_GIT, &repo.0, reference).unwrap();
        assert_eq!(prepared.target, repo.git(&["rev-parse", "HEAD^"]).trim());

        repo.git(&["update-ref", reference, "HEAD"]);
        let error = branches::reset_tag(
            &SYSTEM_GIT,
            &repo.0,
            &prepared,
            branches::ResetMode::Clean,
            None,
        )
        .unwrap_err();
        assert!(error.contains("changed"), "{error}");
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    }

    #[test]
    fn advanced_tag_reset_remembers_the_confirmed_strategy() {
        let (repo, mut app) = opened();
        repo.git(&["commit", "--allow-empty", "-m", "before reset"]);
        repo.write("file", "dirty\n");
        app.open_advanced_ref_reset("refs/tags/chosen".into());
        wait(&mut app);
        let Kind::Reset {
            advanced: Some(options),
            ..
        } = &mut app.branches.ref_dialog.as_mut().unwrap().kind
        else {
            panic!("Expected Reset Advanced");
        };
        assert_eq!(options.strategy, branches::ResetStrategy::Mixed);
        options.strategy = branches::ResetStrategy::Soft;
        app.confirm_tag_reset(branches::ResetMode::Clean);
        assert!(app.busy());
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.branches.ref_dialog.is_none());
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"dirty\n");

        app.branches.current = true;
        app.open_advanced_ref_reset("refs/tags/chosen".into());
        wait(&mut app);
        let Kind::Reset {
            advanced: Some(options),
            ..
        } = &app.branches.ref_dialog.as_ref().unwrap().kind
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
        app.open_ref_reset("refs/tags/chosen".into());
        wait(&mut app);
        app.confirm_tag_reset(branches::ResetMode::Discard);
        assert!(!app.busy());
        app.confirm_tag_reset(branches::ResetMode::Clean);
        assert!(!app.busy());
        assert!(matches!(
            app.branches.ref_dialog.as_ref().unwrap().kind,
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
        assert!(app.branches.ref_dialog.is_none());
        assert_eq!(app.status, "Reset complete");
        assert_eq!(
            repo.git(&["rev-parse", "HEAD"]),
            repo.git(&["rev-parse", "refs/tags/chosen"])
        );
        assert_eq!(app.commit.message, "Keep commit draft");
    }

    #[test]
    fn branch_deletion_needs_confirmation_and_errors_reset_it_without_losing_drafts() {
        let (repo, mut app) = opened();
        app.branches.selected_branch = "refs/heads/merged".into();
        app.open_delete_branch();
        wait(&mut app);
        app.run_ref_action();
        assert!(!app.busy());
        assert!(repo.git(&["branch", "--list", "merged"]).contains("merged"));
        repo.git(&["commit", "--allow-empty", "-m", "External HEAD"]);
        app.branches.ref_dialog.as_mut().unwrap().confirmed = true;
        app.run_ref_action();
        wait(&mut app);
        assert!(app.error.as_deref().unwrap().contains("changed"));
        assert!(!app.branches.ref_dialog.as_ref().unwrap().confirmed);
        assert_eq!(app.commit.message, "Keep commit draft");
        app.reload_ref_dialog();
        wait(&mut app);
        app.branches.ref_dialog.as_mut().unwrap().confirmed = true;
        app.run_ref_action();
        wait(&mut app);
        assert!(app.error.is_none());
        assert!(app.branches.ref_dialog.is_none());
        assert!(repo.git(&["branch", "--list", "merged"]).is_empty());
        assert_eq!(app.status, "Branch deleted");
        assert_eq!(app.commit.message, "Keep commit draft");
    }

    #[test]
    fn remote_editor_adds_updates_and_requires_removal_confirmation_while_preserving_failed_input()
    {
        let (repo, mut app) = opened();
        app.open_remote_manager();
        wait(&mut app);
        if let Kind::Remotes(editor) = &mut app.branches.ref_dialog.as_mut().unwrap().kind {
            editor.name = "origin".into();
            editor.url = "https://example.test/repo.git".into();
        }
        app.change_remote(false);
        wait(&mut app);
        assert!(app.error.is_none());
        if let Kind::Remotes(editor) = &mut app.branches.ref_dialog.as_mut().unwrap().kind {
            assert_eq!(editor.selected.as_deref(), Some("origin"));
            editor.url = "https://dummy-token@example.test/repo".into();
        }
        app.change_remote(false);
        wait(&mut app);
        assert!(app.error.is_some());
        if let Kind::Remotes(editor) = &mut app.branches.ref_dialog.as_mut().unwrap().kind {
            assert!(editor.url.contains("dummy-token"));
            editor.url = "ssh://git@example.test/new.git".into();
        }
        app.change_remote(false);
        wait(&mut app);
        assert!(app.error.is_none());
        app.change_remote(true);
        assert!(!app.busy());
        assert!(repo.git(&["remote"]).contains("origin"));
        if let Kind::Remotes(editor) = &mut app.branches.ref_dialog.as_mut().unwrap().kind {
            editor.confirm_remove = true;
        }
        app.change_remote(true);
        wait(&mut app);
        assert!(app.error.is_none());
        assert!(repo.git(&["remote"]).is_empty());
        assert_eq!(app.commit.message, "Keep commit draft");
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
        app.branches.selected_branch = "refs/tags/chosen".into();
        app.open_push_tag();
        wait(&mut app);
        assert!(app.branches.ref_dialog.as_ref().unwrap().error.is_some());
        if let Kind::Tag { names, remote, .. } = &mut app.branches.ref_dialog.as_mut().unwrap().kind
        {
            assert_eq!(names.len(), 2);
            *remote = "z-good".into();
        }
        app.reload_ref_dialog();
        wait(&mut app);
        assert!(app.branches.ref_dialog.as_ref().unwrap().error.is_none());
        app.run_ref_action();
        assert!(!app.busy());
        app.branches.ref_dialog.as_mut().unwrap().confirmed = true;
        app.run_ref_action();
        wait(&mut app);
        assert!(app.branches.ref_dialog.is_none());
        assert!(app.error.is_none());
        assert_eq!(app.status, "Tag pushed");
        assert!(
            git::text(git::run(&SYSTEM_GIT, &remote, &["tag", "--list"]).unwrap())
                .unwrap()
                .contains("chosen")
        );
        assert_eq!(app.commit.message, "Keep commit draft");
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
        app.branches.selected_branch = "refs/tags/chosen".into();

        app.push_selected_tag();
        assert!(app.busy());
        assert!(app.branches.ref_dialog.is_none());
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.branches.ref_dialog.is_none());
        assert_eq!(app.status, "Tag pushed");
        assert_eq!(
            git::text(
                git::run(
                    &SYSTEM_GIT,
                    &origin,
                    &["for-each-ref", "--format=%(refname)"]
                )
                .unwrap()
            )
            .unwrap()
            .trim(),
            "refs/tags/chosen"
        );
        assert!(git::run(&SYSTEM_GIT, &team, &["tag", "--list"])
            .unwrap()
            .is_empty());

        repo.git(&["remote", "remove", "origin"]);
        app.branches.current = true;
        app.push_selected_tag();
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.branches.ref_dialog.is_none());
        assert_eq!(
            git::text(git::run(&SYSTEM_GIT, &team, &["tag", "--list"]).unwrap())
                .unwrap()
                .trim(),
            "chosen"
        );
        assert_eq!(app.commit.message, "Keep commit draft");
    }

    #[test]
    fn direct_tag_push_requests_configuration_or_remote_choice_when_needed() {
        let (repo, mut app) = opened();
        app.branches.selected_branch = "refs/tags/chosen".into();
        app.push_selected_tag();
        wait(&mut app);
        assert!(matches!(
            app.branches.ref_dialog.as_ref().unwrap().kind,
            Kind::AddTagRemote
        ));
        app.branches.ref_dialog = None;

        let remote = repo.0.join("team.git");
        repo.git(&["init", "--bare", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", "team", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", "archive", remote.to_str().unwrap()]);
        repo.git(&["push", "-u", "team", "HEAD"]);
        app.branches.current = true;
        app.push_selected_tag();
        wait(&mut app);
        let dialog = app.branches.ref_dialog.as_ref().unwrap();
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
        assert!(git::run(&SYSTEM_GIT, &remote, &["tag", "--list"])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn add_tag_and_push_sends_only_the_new_tag_to_the_upstream_remote() {
        let (repo, mut app) = opened();
        let remote = repo.0.join("remote.git");
        repo.git(&["init", "--bare", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", "origin", "https://example.test/repo"]);
        repo.git(&["remote", "add", "team", remote.to_str().unwrap()]);
        repo.git(&["push", "-u", "team", "HEAD"]);
        app.branches.tag_dialog_open = true;
        app.branches.new_tag_name = "release".into();
        app.branches.new_tag_message = "Release notes".into();
        app.create_tag(true);
        assert!(!app.branches.tag_dialog_open);
        assert!(app.busy());
        wait(&mut app);
        assert!(app.error.is_none());
        assert_eq!(
            repo.git(&["cat-file", "-t", "refs/tags/release"]).trim(),
            "tag"
        );
        let remote_tags =
            || git::text(git::run(&SYSTEM_GIT, &remote, &["tag", "--list"]).unwrap()).unwrap();
        assert!(app.branches.ref_dialog.is_none());
        assert_eq!(remote_tags().trim(), "release");
    }

    #[test]
    fn add_tag_and_push_requires_an_upstream_or_origin() {
        let (repo, mut app) = opened();
        let remote = repo.0.join("remote.git");
        repo.git(&["init", "--bare", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", "team", remote.to_str().unwrap()]);
        app.branches.tag_dialog_open = true;
        app.branches.new_tag_name = "release".into();
        app.create_tag(true);
        wait(&mut app);
        assert!(app
            .error
            .as_deref()
            .unwrap()
            .contains("Configure an upstream"));
        assert!(!app.branches.tag_dialog_open);
        assert_eq!(app.branches.new_tag_name, "release");
        assert!(repo.git(&["tag", "--list", "release"]).is_empty());
        assert!(
            git::text(git::run(&SYSTEM_GIT, &remote, &["tag", "--list"]).unwrap())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn closed_dialogs_and_other_repositories_ignore_late_details_and_actions() {
        let (repo, mut app) = opened();
        app.open_remote_manager();
        wait(&mut app);
        app.selected_repository = usize::MAX;
        if let Kind::Remotes(editor) = &mut app.branches.ref_dialog.as_mut().unwrap().kind {
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
        assert!(app.branches.ref_dialog.as_ref().unwrap().error.is_none());
        let root = app.branches.ref_dialog.as_ref().unwrap().root.clone();
        app.branches.ref_dialog = None;
        app.apply_ref_details(root, ResultMessage::Remotes(Err("Late error".into())));
        assert!(app.branches.ref_dialog.is_none());
    }
}
