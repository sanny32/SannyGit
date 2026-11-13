use super::{SannyGitApp, WorkerResult};
use crate::{
    git,
    repository::{self, Preferences, SearchResult, Settings, Worktree},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    time::{Duration, Instant},
};

struct HostingUpdate {
    path: String,
    provider: Result<Option<crate::clone_source::Provider>, String>,
}

struct WorktreeAvailability {
    path: String,
    result: Result<(bool, bool), String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialog {
    Group,
    Clone,
    Search,
    Worktrees,
    AddWorktree,
    Prune,
    Settings,
    CloseRepository,
    RemoveRepository,
}

pub struct RepositoryManager {
    availability_receiver: Option<Receiver<WorktreeAvailability>>,
    availability_checked: Option<Instant>,
    pub worktree_availability: Option<(String, bool, bool)>,
    pub hosting: HashMap<String, crate::clone_source::Provider>,
    hosting_checked: HashMap<String, Instant>,
    hosting_receiver: Option<Receiver<HostingUpdate>>,
    pub preferences: Preferences,
    pub dialog: Option<Dialog>,
    pub highlighted_repository: Option<String>,
    pub repository_to_remove: Option<String>,
    pub clone_step: usize,
    pub clone_options: repository::CloneOptions,
    pub clone_branches: Vec<String>,
    pub clone_parent: String,
    pub clone_name: String,
    pub group_name: String,
    pub clone_source: String,
    pub clone_form: crate::clone_source::Form,
    pub destination: String,
    pub search_root: String,
    pub search_depth: usize,
    pub found: Vec<(String, String, bool)>,
    pub search_summary: String,
    pub worktrees: Vec<Worktree>,
    pub selected_worktree: Option<String>,
    pub worktree_branch: String,
    pub worktree_branches: Vec<String>,
    pub worktree_new_branch: bool,
    pub confirm_remove: bool,
    pub prune_preview: Option<String>,
    pub settings: Option<Settings>,
    pub cancel: Option<Arc<AtomicBool>>,
}
impl Default for RepositoryManager {
    fn default() -> Self {
        Self {
            availability_receiver: None,
            availability_checked: None,
            worktree_availability: None,
            hosting: HashMap::new(),
            hosting_checked: HashMap::new(),
            hosting_receiver: None,
            preferences: Preferences::default(),
            dialog: None,
            highlighted_repository: None,
            repository_to_remove: None,
            clone_step: 0,
            clone_options: repository::CloneOptions::default(),
            clone_branches: Vec::new(),
            clone_parent: "~".into(),
            clone_name: String::new(),
            group_name: String::new(),
            clone_source: String::new(),
            clone_form: crate::clone_source::Form::default(),
            destination: String::new(),
            search_root: String::new(),
            search_depth: 6,
            found: Vec::new(),
            search_summary: String::new(),
            worktrees: Vec::new(),
            selected_worktree: None,
            worktree_branch: String::new(),
            worktree_branches: Vec::new(),
            worktree_new_branch: false,
            confirm_remove: false,
            prune_preview: None,
            settings: None,
            cancel: None,
        }
    }
}

pub enum Job {
    NewWindow,
    NewRepositoryWindow(String),
    Reveal(String),
    AddOrCreate(String),
    InspectClone(String),
    Clone {
        source: String,
        destination: String,
        options: repository::CloneOptions,
    },
    Search {
        root: String,
        depth: usize,
    },
    ListWorktrees,
    PrepareAddWorktree,
    AddWorktree {
        destination: String,
        branch: String,
        new_branch: bool,
    },
    RemoveWorktree(String),
    PreviewPrune,
    Prune,
    LoadSettings,
    SaveSettings(Settings),
    EditConfig {
        global: bool,
    },
}
pub enum ResultMessage {
    WorktreeBranches(Result<Vec<String>, String>),
    CloneBranches {
        source: String,
        result: Result<Vec<String>, String>,
    },
    Window(Result<(), String>),
    Open(Result<git::Snapshot, String>),
    Search(Result<SearchResult, String>),
    Worktrees {
        result: Result<Vec<Worktree>, String>,
        snapshot: Option<Result<git::Snapshot, String>>,
    },
    Prune(Result<String, String>),
    Settings {
        result: Result<Settings, String>,
        saved: bool,
    },
    Editor(Result<(), String>),
}

impl SannyGitApp {
    pub(super) fn invalidate_worktree_availability(&mut self) {
        self.manager.availability_receiver = None;
        self.manager.availability_checked = None;
    }

    pub(super) fn update_worktree_availability(&mut self, context: &eframe::egui::Context) {
        if let Some(receiver) = &self.manager.availability_receiver {
            match receiver.try_recv() {
                Ok(update) => {
                    self.manager.availability_receiver = None;
                    if self
                        .repositories
                        .get(self.selected_repository)
                        .is_some_and(|r| r.path == update.path)
                    {
                        match update.result {
                            Ok((remove, prune)) => {
                                self.manager.worktree_availability =
                                    Some((update.path, remove, prune))
                            }
                            Err(error) => {
                                self.manager.worktree_availability = None;
                                self.error = Some(error);
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.manager.availability_receiver = None;
                    self.manager.worktree_availability = None;
                    self.error = Some("Worktree availability worker disconnected".into());
                }
            }
        }
        if self.busy()
            || self
                .manager
                .availability_checked
                .is_some_and(|time| time.elapsed() < Duration::from_secs(30))
        {
            return;
        }
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let path = repository.path.clone();
        let git_executable = self.preferences.git_executable.clone();
        let context = context.clone();
        let (sender, receiver) = mpsc::channel();
        self.manager.availability_checked = Some(Instant::now());
        match std::thread::Builder::new()
            .name("sannygit-worktree-availability".into())
            .spawn(move || {
                git::set_executable(git_executable);
                let result = repository::worktree_availability(std::path::Path::new(&path));
                let _ = sender.send(WorktreeAvailability { path, result });
                context.request_repaint();
            }) {
            Ok(_) => self.manager.availability_receiver = Some(receiver),
            Err(error) => self.error = Some(format!("Cannot inspect worktrees: {error}")),
        }
    }

    pub(super) fn invalidate_repository_hosting(&mut self) {
        self.manager.hosting_receiver = None;
        if let Some(repository) = self.repositories.get(self.selected_repository) {
            self.manager.hosting_checked.remove(&repository.path);
        }
    }

    pub(super) fn update_repository_hosting(&mut self, context: &eframe::egui::Context) {
        if let Some(receiver) = &self.manager.hosting_receiver {
            match receiver.try_recv() {
                Ok(update) => {
                    if self.repositories.iter().any(|r| r.path == update.path) {
                        // Keep the last known icon on read errors and retry later.
                        if let Ok(provider) = update.provider {
                            if let Some(provider) = provider {
                                self.manager.hosting.insert(update.path.clone(), provider);
                            } else {
                                self.manager.hosting.remove(&update.path);
                            }
                        }
                        self.manager
                            .hosting_checked
                            .insert(update.path, Instant::now());
                    }
                    self.manager.hosting_receiver = None;
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.manager.hosting_receiver = None;
                }
            }
        }
        let Some(repository) = self.repositories.iter().find(|r| {
            self.manager
                .hosting_checked
                .get(&r.path)
                .is_none_or(|checked| checked.elapsed() >= Duration::from_secs(30))
        }) else {
            return;
        };
        let path = repository.path.clone();
        let context = context.clone();
        let (sender, receiver) = mpsc::channel();
        // Rate-limit retries even when a thread cannot be started.
        self.manager
            .hosting_checked
            .insert(path.clone(), Instant::now());
        let git_executable = self.preferences.git_executable.clone();
        if std::thread::Builder::new()
            .name("sannygit-hosting".into())
            .spawn(move || {
                git::set_executable(git_executable);
                let provider = repository::hosting_provider(std::path::Path::new(&path));
                let _ = sender.send(HostingUpdate { path, provider });
                context.request_repaint();
            })
            .is_ok()
        {
            self.manager.hosting_receiver = Some(receiver);
        }
    }

    pub fn run_repository_job(&mut self, job: Job) {
        if self.busy() {
            return;
        }
        let root = self
            .repositories
            .get(self.selected_repository)
            .map(|r| PathBuf::from(&r.path));
        let needs_root = matches!(
            job,
            Job::ListWorktrees
                | Job::PrepareAddWorktree
                | Job::AddWorktree { .. }
                | Job::RemoveWorktree(_)
                | Job::PreviewPrune
                | Job::Prune
                | Job::LoadSettings
                | Job::SaveSettings(_)
                | Job::EditConfig { global: false }
        );
        if needs_root && root.is_none() {
            return;
        }
        self.error = None;
        self.status = match &job {
            Job::NewWindow | Job::NewRepositoryWindow(_) => "Opening new window…",
            Job::Reveal(_) => "Opening repository folder…",
            Job::AddOrCreate(_) => "Opening repository…",
            Job::InspectClone(_) => "Inspecting repository…",
            Job::Clone { .. } => "Cloning repository…",
            Job::Search { .. } => "Searching repositories…",
            Job::ListWorktrees | Job::PrepareAddWorktree => "Loading worktrees…",
            Job::AddWorktree { .. } => "Adding worktree…",
            Job::RemoveWorktree(_) => "Removing worktree…",
            Job::PreviewPrune => "Inspecting obsolete worktrees…",
            Job::Prune => "Pruning obsolete worktree metadata…",
            Job::LoadSettings => "Loading repository settings…",
            Job::SaveSettings(_) => "Saving repository settings…",
            Job::EditConfig { .. } => "Opening Git config…",
        }
        .into();
        let cancel = Arc::new(AtomicBool::new(false));
        self.manager.cancel = matches!(
            job,
            Job::InspectClone(_) | Job::Clone { .. } | Job::Search { .. } | Job::AddWorktree { .. }
        )
        .then(|| cancel.clone());
        if !self.start_worker(move || {
            let result = match job {
                Job::NewWindow => ResultMessage::Window(repository::new_window()),
                Job::NewRepositoryWindow(path) => {
                    ResultMessage::Window(repository::new_repository_window(&PathBuf::from(path)))
                }
                Job::Reveal(path) => {
                    ResultMessage::Editor(repository::reveal(&PathBuf::from(path)))
                }
                Job::AddOrCreate(path) => ResultMessage::Open(repository::add_or_create(&path)),
                Job::InspectClone(source) => {
                    let result = repository::clone_branches(&source, &cancel);
                    ResultMessage::CloneBranches { source, result }
                }
                Job::Clone {
                    source,
                    destination,
                    options,
                } => ResultMessage::Open(repository::clone_with_options(
                    &source,
                    &PathBuf::from(destination),
                    &options,
                    &cancel,
                )),
                Job::Search { root, depth } => {
                    ResultMessage::Search(repository::search(&PathBuf::from(root), depth, &cancel))
                }
                other => {
                    let root = root.unwrap_or_default();
                    match other {
                        Job::PrepareAddWorktree => ResultMessage::WorktreeBranches(
                            repository::available_worktree_branches(&root),
                        ),
                        Job::ListWorktrees => ResultMessage::Worktrees {
                            result: repository::worktrees(&root),
                            snapshot: None,
                        },
                        Job::AddWorktree {
                            destination,
                            branch,
                            new_branch,
                        } => {
                            let result = repository::add_worktree(
                                &root,
                                &PathBuf::from(destination),
                                &branch,
                                new_branch,
                                &cancel,
                            );
                            ResultMessage::Worktrees {
                                result,
                                snapshot: Some(git::load(&root)),
                            }
                        }
                        Job::RemoveWorktree(path) => {
                            let result = repository::remove_worktree(&root, &PathBuf::from(path));
                            ResultMessage::Worktrees {
                                result,
                                snapshot: Some(git::load(&root)),
                            }
                        }
                        Job::PreviewPrune => ResultMessage::Prune(repository::prune_preview(&root)),
                        Job::Prune => ResultMessage::Worktrees {
                            result: repository::prune(&root),
                            snapshot: Some(git::load(&root)),
                        },
                        Job::LoadSettings => ResultMessage::Settings {
                            result: repository::settings(&root),
                            saved: false,
                        },
                        Job::SaveSettings(settings) => ResultMessage::Settings {
                            result: repository::save_settings(&root, &settings),
                            saved: true,
                        },
                        Job::EditConfig { global } => ResultMessage::Editor(
                            (if global {
                                repository::user_config_path()
                            } else {
                                repository::config_path(&root)
                            })
                            .and_then(|path| repository::edit_config(&path)),
                        ),
                        _ => unreachable!("handled above"),
                    }
                }
            };
            WorkerResult::RepositoryTask(Box::new(result))
        }) {
            self.manager.cancel = None;
        }
    }

    pub fn cancel_repository_job(&mut self) {
        if let Some(cancel) = &self.manager.cancel {
            cancel.store(true, Ordering::Relaxed);
            self.status = "Cancelling operation…".into();
        }
    }

    pub(super) fn apply_repository_result(&mut self, message: ResultMessage) {
        self.manager.cancel = None;
        self.status = "Ready".into();
        let outcome = match message {
            ResultMessage::WorktreeBranches(result) => result.map(|branches| {
                self.manager.worktree_branch = branches.first().cloned().unwrap_or_default();
                self.manager.destination = self
                    .repositories
                    .get(self.selected_repository)
                    .map(|repository| {
                        format!(
                            "{}-{}",
                            repository.path,
                            self.manager.worktree_branch.replace('/', "-")
                        )
                    })
                    .unwrap_or_default();
                self.manager.worktree_branches = branches;
                self.manager.dialog = Some(Dialog::AddWorktree);
            }),
            ResultMessage::CloneBranches { source, result } => result.map(|branches| {
                if self.manager.dialog == Some(Dialog::Clone) && self.manager.clone_source == source
                {
                    self.manager.clone_options.branch =
                        branches.first().cloned().unwrap_or_default();
                    self.manager.clone_branches = branches;
                    self.manager.clone_name = source
                        .trim_end_matches('/')
                        .rsplit(['/', ':'])
                        .next()
                        .unwrap_or("repository")
                        .trim_end_matches(".git")
                        .into();
                    self.manager.clone_step = 1;
                }
            }),
            ResultMessage::Window(result) | ResultMessage::Editor(result) => result,
            ResultMessage::Open(result) => result.map(|snapshot| {
                self.apply_snapshot(snapshot);
                self.manager.dialog = None;
            }),
            ResultMessage::Search(result) => result.map(|result| {
                self.manager.search_summary = format!(
                    "{} found; {} skipped{}",
                    result.repositories.len(),
                    result.skipped,
                    if result.limited {
                        "; search limit reached"
                    } else {
                        ""
                    }
                );
                self.manager.found = result
                    .repositories
                    .into_iter()
                    .map(|r| (r.name, r.path, true))
                    .collect();
            }),
            ResultMessage::Worktrees { result, snapshot } => {
                if let Some(snapshot) = snapshot {
                    self.manager.worktree_availability = None;
                    match snapshot {
                        Ok(snapshot) => self.apply_snapshot(snapshot),
                        Err(error) => {
                            self.state_current = false;
                            self.error = Some(format!("Refresh required: {error}"));
                        }
                    }
                }
                result.map(|trees| {
                    self.manager.worktrees = trees;
                    self.manager.confirm_remove = false;
                    self.manager.prune_preview = None;
                    if matches!(
                        self.manager.dialog,
                        Some(Dialog::Prune | Dialog::AddWorktree)
                    ) {
                        self.manager.dialog = None;
                    }
                })
            }
            ResultMessage::Prune(result) => {
                result.map(|preview| self.manager.prune_preview = Some(preview))
            }
            ResultMessage::Settings { result, saved } => result.map(|settings| {
                self.manager.settings = Some(settings);
                if saved {
                    self.manager.dialog = None;
                    self.status = "Repository settings saved".into();
                }
            }),
        };
        if let Err(error) = outcome {
            self.error = Some(error);
            self.status = "Operation failed; inspect the current state before retrying".into();
        }
    }

    pub fn add_group(&mut self) {
        let name = self.manager.group_name.trim();
        if name.is_empty()
            || name.len() > 128
            || self
                .manager
                .preferences
                .groups
                .iter()
                .any(|g| g.name == name)
        {
            self.error = Some("Enter a unique group name (1–128 bytes).".into());
            return;
        }
        self.manager
            .preferences
            .groups
            .push(repository::RepositoryGroup {
                name: name.into(),
                paths: Vec::new(),
            });
        self.manager.preferences.selected_group = Some(name.into());
        self.manager.group_name.clear();
        self.manager.dialog = None;
        self.error = None;
        self.status = "Group added".into();
    }

    pub fn assign_group(&mut self, path: &str, group: Option<String>) {
        for entry in &mut self.manager.preferences.groups {
            entry.paths.retain(|p| p != path);
        }
        if let Some(group) = group {
            if let Some(entry) = self
                .manager
                .preferences
                .groups
                .iter_mut()
                .find(|g| g.name == group)
            {
                entry.paths.push(path.into());
            }
        }
    }

    pub fn add_found_repositories(&mut self) {
        let paths: Vec<_> = self
            .manager
            .found
            .iter()
            .filter(|r| r.2)
            .map(|r| (r.0.clone(), r.1.clone()))
            .collect();
        for (name, path) in paths {
            if !self.repositories.iter().any(|r| r.path == path) {
                self.repositories.push(crate::models::Repository {
                    name,
                    path: path.clone(),
                });
            }
            self.assign_group(&path, self.manager.preferences.selected_group.clone());
        }
        self.manager.dialog = None;
        self.error = None;
        self.status = "Repositories added".into();
    }

    pub fn request_remove_repository(&mut self, path: String) {
        if self.busy() || !self.repositories.iter().any(|r| r.path == path) {
            return;
        }
        self.manager.repository_to_remove = Some(path);
        self.manager.dialog = Some(Dialog::RemoveRepository);
    }

    pub fn confirm_remove_repository(&mut self) {
        if self.busy() || self.manager.dialog != Some(Dialog::RemoveRepository) {
            return;
        }
        let Some(path) = self.manager.repository_to_remove.take() else {
            return;
        };
        self.manager.dialog = None;
        let Some(index) = self.repositories.iter().position(|r| r.path == path) else {
            return;
        };
        if self.selected_repository == index {
            self.close_repository(true);
        } else if self.selected_repository != usize::MAX && self.selected_repository > index {
            self.selected_repository -= 1;
        }
        self.repositories.remove(index);
        self.assign_group(&path, None);
        self.commit_drafts.remove(&path);
        self.manager.hosting.remove(&path);
        self.manager.hosting_checked.remove(&path);
        if self.manager.highlighted_repository.as_deref() == Some(&path) {
            self.manager.highlighted_repository = None;
        }
        self.status = "Repository removed from the list".into();
    }

    pub fn close_repository(&mut self, discard_draft: bool) {
        if self.busy() {
            return;
        }
        if !discard_draft
            && (!self.commit_message.is_empty()
                || self
                    .amend_draft
                    .as_ref()
                    .is_some_and(|draft| !draft.is_empty()))
        {
            self.manager.dialog = Some(Dialog::CloseRepository);
            return;
        }
        if let Some(repository) = self.repositories.get(self.selected_repository) {
            self.commit_drafts.remove(&repository.path);
        }
        self.amend = false;
        self.amend_head = None;
        self.amend_draft = None;
        self.open_dialog = false;
        self.selected_repository = usize::MAX;
        self.invalidate_view_reads();
        self.tracking.invalidate();
        self.selected_change = usize::MAX;
        self.current_branch.clear();
        self.changes.clear();
        self.refs.clear();
        self.commits.clear();
        self.history_edges.clear();
        self.history_max_lane = 0;
        self.history_truncated = false;
        self.history_loading = false;
        self.history_pending = false;
        self.history_session.clear_view();
        self.history_error = None;
        self.refs_current = false;
        self.refs_worker = None;
        self.refs_loading = false;
        self.refs_pending = false;
        self.refs_error = None;
        self.branch_head_exists = false;
        self.branch_dialog_open = false;
        self.rename_branch_dialog = None;
        self.tag_dialog_open = false;
        self.new_tag_name.clear();
        self.new_tag_message.clear();
        self.remote_checkout = None;
        self.network_dialog = None;
        self.stash_dialog = None;
        self.discard_dialog = None;
        self.ref_dialog = None;
        self.selected_branch.clear();
        self.selected_ref_group = None;
        self.loaded_diff = None;
        self.diff_preview = None;
        self.selected_commit = None;
        self.commit_details_open = false;
        self.clear_commit_preview();
        self.state_current = false;
        self.commit_message.clear();
        self.contents_preview_open = false;
        self.contents_preview = None;
        self.journal_filter.clear();
        self.manager.dialog = None;
        self.manager.worktrees.clear();
        self.manager.settings = None;
        self.error = None;
        self.status = "Ready".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeChoice;
    use std::{collections::HashMap, path::Path};

    #[test]
    fn worktree_menu_actions_require_current_repository_availability() {
        use crate::menu::MenuAction;
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        let path = app.repositories[0].path.clone();
        for (cached, remove, prune) in [
            (None, false, false),
            (Some((path.clone(), false, false)), false, false),
            (Some((path.clone(), true, false)), true, false),
            (Some((path.clone(), false, true)), false, true),
            (Some(("other repository".into(), true, true)), false, false),
        ] {
            app.manager.worktree_availability = cached;
            assert_eq!(app.menu_action_enabled(MenuAction::RemoveWorktree), remove);
            assert_eq!(app.menu_action_enabled(MenuAction::PruneWorktrees), prune);
        }
        app.manager.worktree_availability = Some((path, true, true));
        let (_sender, receiver) = mpsc::channel();
        app.worker = Some(receiver);
        assert!(!app.menu_action_enabled(MenuAction::RemoveWorktree));
        assert!(!app.menu_action_enabled(MenuAction::PruneWorktrees));
    }

    #[test]
    fn hosting_updates_inactive_repositories_and_retains_icons_on_errors() {
        let repo = crate::test_support::TempRepo::new();
        repo.git(&["remote", "add", "origin", "git@github.com:owner/repo.git"]);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        let path = repo.0.to_string_lossy().into_owned();
        app.repositories = vec![crate::models::Repository {
            name: "Test".into(),
            path: path.clone(),
        }];
        app.selected_repository = usize::MAX;
        let context = eframe::egui::Context::default();
        app.update_repository_hosting(&context);
        assert!(!app.busy());
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.manager.hosting_receiver.is_some() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
            app.update_repository_hosting(&context);
        }
        assert_eq!(
            app.manager.hosting.get(&path),
            Some(&crate::clone_source::Provider::GitHub)
        );
        for (provider, expected) in [
            (
                Err("Read failed".into()),
                Some(crate::clone_source::Provider::GitHub),
            ),
            (Ok(None), None),
        ] {
            let (sender, receiver) = mpsc::channel();
            sender
                .send(HostingUpdate {
                    path: path.clone(),
                    provider,
                })
                .unwrap();
            app.manager.hosting_receiver = Some(receiver);
            app.update_repository_hosting(&context);
            assert_eq!(app.manager.hosting.get(&path).copied(), expected);
        }
        assert_eq!(app.selected_repository, usize::MAX);
    }

    #[derive(Default)]
    struct Storage(HashMap<String, String>);
    impl eframe::Storage for Storage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    fn finish_restore(app: &mut SannyGitApp) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.busy() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
            app.poll_worker();
        }
        assert!(!app.busy(), "Repository restore did not finish");
    }

    #[test]
    fn clone_form_inspects_branches_and_opens_the_finished_clone() {
        let repo = crate::test_support::TempRepo::new();
        repo.write("file", "clone fixture\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "fixture"]);
        // Keep the source compatible with Git for Windows clone protection.
        repo.git(&["config", "--local", "--unset", "core.hooksPath"]);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.manager.dialog = Some(Dialog::Clone);
        *app.manager.clone_form.input() = repo.0.to_string_lossy().into_owned();
        app.manager.clone_source = app.manager.clone_form.resolve().unwrap();
        app.run_repository_job(Job::InspectClone(app.manager.clone_source.clone()));
        finish_restore(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.manager.clone_step, 1);
        assert_eq!(app.manager.clone_branches, vec!["main"]);
        let destination = repo.0.join("finished-clone");
        app.run_repository_job(Job::Clone {
            source: app.manager.clone_source.clone(),
            destination: destination.to_string_lossy().into_owned(),
            options: app.manager.clone_options.clone(),
        });
        finish_restore(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.manager.dialog, None);
        assert_eq!(
            std::fs::read_to_string(destination.join("file"))
                .unwrap()
                .replace("\r\n", "\n"),
            "clone fixture\n"
        );
        assert_eq!(
            PathBuf::from(&app.repositories[app.selected_repository].path)
                .canonicalize()
                .unwrap(),
            destination.canonicalize().unwrap()
        );
    }

    #[test]
    fn repository_llm_settings_persist_without_secrets_and_inherit_global_defaults() {
        let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        app.llm_config.instructions = "Global instructions".into();
        let custom = crate::llm::Config {
            endpoint: "http://localhost:1234/v1".into(),
            model: "local-model".into(),
            instructions: "Repository instructions".into(),
            api_key: "private-key".into(),
            ..Default::default()
        };
        app.repository_llm_configs
            .insert("/first".into(), custom.clone());
        assert_eq!(app.effective_llm_config("/first"), &custom);
        assert_eq!(app.effective_llm_config("/second"), &app.llm_config);
        app.llm_config.instructions = "Updated global instructions".into();
        assert_eq!(
            app.effective_llm_config("/first").instructions,
            "Repository instructions"
        );
        assert_eq!(
            app.effective_llm_config("/second").instructions,
            "Updated global instructions"
        );

        let mut storage = Storage::default();
        eframe::App::save(&mut app, &mut storage);
        let mut restored = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        restored.llm_config = eframe::get_value(&storage, "sannygit.llm").unwrap();
        restored.repository_llm_configs =
            eframe::get_value(&storage, "sannygit.repository_llm").unwrap();
        let config = restored.effective_llm_config("/first");
        assert_eq!(config.endpoint, custom.endpoint);
        assert_eq!(config.model, custom.model);
        assert_eq!(config.instructions, custom.instructions);
        assert!(config.api_key.is_empty());
        assert!(!storage
            .0
            .values()
            .any(|value| value.contains("private-key")));
        restored.repository_llm_configs.remove("/first");
        assert_eq!(
            restored.effective_llm_config("/first"),
            &restored.llm_config
        );
    }

    #[test]
    fn active_repository_restores_by_path_and_explicit_close_clears_it() {
        let repo = crate::test_support::TempRepo::new();
        let path = repo.0.canonicalize().unwrap().to_str().unwrap().to_owned();
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories = vec![
            crate::models::Repository {
                name: "Other".into(),
                path: "/other".into(),
            },
            crate::models::Repository {
                name: "Selected".into(),
                path: path.clone(),
            },
        ];
        app.selected_repository = 1;
        // Highlighting an inactive row must not change the restored repository.
        app.manager.highlighted_repository = Some("/other".into());
        let mut storage = Storage::default();
        eframe::App::save(&mut app, &mut storage);
        let saved: Option<String> =
            eframe::get_value(&storage, "sannygit.active_repository").unwrap();
        assert_eq!(saved.as_deref(), Some(path.as_str()));
        let mut restored = SannyGitApp::with_theme(ThemeChoice::Light);
        restored.repositories = app.repositories;
        restored.repositories.reverse();
        restored.restore_active_repository(&storage);
        assert!(restored.busy());
        finish_restore(&mut restored);
        assert!(restored.error.is_none());
        assert_eq!(
            Path::new(&restored.repositories[restored.selected_repository].path)
                .canonicalize()
                .unwrap(),
            Path::new(&path).canonicalize().unwrap()
        );
        assert_eq!(restored.selected_repository, 0);
        assert_eq!(restored.repositories.len(), 2);
        restored.close_repository(true);
        eframe::App::save(&mut restored, &mut storage);
        let saved: Option<String> =
            eframe::get_value(&storage, "sannygit.active_repository").unwrap();
        assert_eq!(saved, None);
        restored.restore_active_repository(&storage);
        assert!(!restored.busy());
        assert_eq!(restored.selected_repository, usize::MAX);
    }

    #[test]
    fn opening_an_equivalent_path_reuses_the_bookmark_and_its_settings() {
        let repo = crate::test_support::TempRepo::new();
        let stored_path = repo
            .0
            .canonicalize()
            .unwrap()
            .join(".")
            .to_str()
            .unwrap()
            .to_owned();
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories.push(crate::models::Repository {
            name: "Saved name".into(),
            path: stored_path.clone(),
        });
        app.repository_llm_configs
            .insert(stored_path.clone(), crate::llm::Config::default());
        app.manager
            .preferences
            .groups
            .push(crate::repository::RepositoryGroup {
                name: "Saved group".into(),
                paths: vec![stored_path.clone()],
            });
        app.apply_snapshot(git::load(&repo.0).unwrap());
        assert_eq!(app.repositories.len(), 1);
        assert_eq!(app.selected_repository, 0);
        assert_eq!(app.repositories[0].name, "Saved name");
        assert_eq!(app.repositories[0].path, stored_path);
        assert!(app.repository_llm_configs.contains_key(&stored_path));
        assert_eq!(app.manager.preferences.groups[0].paths, [stored_path]);
    }

    #[test]
    fn restore_missing_repository_reports_error_without_removing_bookmark() {
        let repo = crate::test_support::TempRepo::new();
        let path = repo.0.join("missing").to_str().unwrap().to_owned();
        let mut storage = Storage::default();
        eframe::set_value(&mut storage, "sannygit.active_repository", &Some(&path));
        let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
        app.repositories.push(crate::models::Repository {
            name: "Missing".into(),
            path: path.clone(),
        });
        app.restore_active_repository(&storage);
        finish_restore(&mut app);
        assert!(app.error.is_some());
        assert_eq!(app.selected_repository, usize::MAX);
        assert_eq!(app.repositories[0].path, path);
        eframe::set_value(
            &mut storage,
            "sannygit.active_repository",
            &Some("/not-bookmarked"),
        );
        app.restore_active_repository(&storage);
        assert!(!app.busy());
    }

    #[test]
    fn groups_move_repositories_without_duplicates_and_persist() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        for name in ["Work", "Personal"] {
            app.manager.group_name = name.into();
            app.add_group();
        }
        app.assign_group("/repo", Some("Work".into()));
        app.assign_group("/repo", Some("Work".into()));
        assert_eq!(app.manager.preferences.groups[0].paths, vec!["/repo"]);
        app.assign_group("/repo", Some("Personal".into()));
        assert!(app.manager.preferences.groups[0].paths.is_empty());
        assert_eq!(app.manager.preferences.groups[1].paths, vec!["/repo"]);
        app.manager.group_name = "Work".into();
        app.add_group();
        assert!(app.error.is_some());
        assert_eq!(app.manager.preferences.groups.len(), 2);
        let mut storage = Storage::default();
        eframe::App::save(&mut app, &mut storage);
        let prefs: Preferences =
            eframe::get_value(&storage, "sannygit.repository_preferences").unwrap();
        assert_eq!(prefs.groups[1].paths, vec!["/repo"]);
        assert_eq!(prefs.selected_group.as_deref(), Some("Personal"));
        app.assign_group("/repo", None);
        assert!(app
            .manager
            .preferences
            .groups
            .iter()
            .all(|g| g.paths.is_empty()));
    }

    #[test]
    fn drafts_survive_repository_switch_restart_amend_and_failed_open() {
        let first = crate::test_support::TempRepo::new();
        first.write("file", "base\n");
        crate::git::commit(&first.0, "Base", crate::git::CommitContents::All).unwrap();
        let second = crate::test_support::TempRepo::new();
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&first.0).unwrap());
        app.commit_message = "Normal draft\n\nBody".into();
        app.apply_snapshot(crate::git::load(&second.0).unwrap());
        assert!(app.commit_message.is_empty());
        app.commit_message = "Other repository".into();
        app.apply_snapshot(crate::git::load(&first.0).unwrap());
        assert_eq!(app.commit_message, "Normal draft\n\nBody");
        let (head, _) = crate::git::amend_message(&first.0).unwrap();
        app.amend = true;
        app.amend_head = Some(head.clone());
        app.amend_draft = Some(std::mem::replace(
            &mut app.commit_message,
            "Edited amend".into(),
        ));
        app.apply_snapshot(crate::git::load(&second.0).unwrap());
        app.apply_snapshot(crate::git::load(&first.0).unwrap());
        assert!(app.amend);
        assert_eq!(app.commit_message, "Edited amend");
        let mut storage = Storage::default();
        eframe::App::save(&mut app, &mut storage);
        let mut restored = SannyGitApp::with_theme(ThemeChoice::Dark);
        restored.repositories = app.repositories;
        restored.restore_commit_drafts(&storage);
        restored.restore_active_repository(&storage);
        finish_restore(&mut restored);
        assert!(restored.amend);
        assert_eq!(restored.amend_head.as_deref(), Some(head.as_str()));
        assert_eq!(restored.commit_message, "Edited amend");
        restored.open_repository(first.0.join("missing").to_string_lossy().into_owned());
        finish_restore(&mut restored);
        assert!(restored.error.is_some());
        assert_eq!(restored.commit_message, "Edited amend");
        restored.set_amend(false);
        assert_eq!(restored.commit_message, "Normal draft\n\nBody");
        restored.apply_snapshot(crate::git::load(&second.0).unwrap());
        assert_eq!(restored.commit_message, "Other repository");
        restored.close_repository(true);
        restored.apply_snapshot(crate::git::load(&second.0).unwrap());
        assert!(restored.commit_message.is_empty());
        assert!(!restored.amend);
        restored.apply_snapshot(crate::git::load(&first.0).unwrap());
        assert_eq!(restored.commit_message, "Normal draft\n\nBody");
    }

    #[test]
    fn restored_amend_rejects_changed_head_and_keeps_drafts_until_success() {
        let repo = crate::test_support::TempRepo::new();
        repo.write("file", "base\n");
        crate::git::commit(&repo.0, "Base", crate::git::CommitContents::All).unwrap();
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&repo.0).unwrap());
        app.commit_message = "Amend draft".into();
        app.amend = true;
        app.amend_head = Some(crate::git::amend_message(&repo.0).unwrap().0);
        app.amend_draft = Some("Normal draft".into());
        let mut storage = Storage::default();
        eframe::App::save(&mut app, &mut storage);
        repo.write("file", "external commit\n");
        crate::git::commit(&repo.0, "External", crate::git::CommitContents::All).unwrap();
        repo.write("file", "keep unstaged\n");
        let mut restored = SannyGitApp::with_theme(ThemeChoice::Dark);
        restored.repositories = app.repositories;
        restored.restore_commit_drafts(&storage);
        restored.restore_active_repository(&storage);
        finish_restore(&mut restored);
        restored.create_commit();
        finish_restore(&mut restored);
        assert!(restored
            .error
            .as_deref()
            .unwrap()
            .contains("HEAD has changed"));
        assert_eq!(restored.commit_message, "Amend draft");
        assert!(restored.amend);
        assert!(repo.git(&["diff", "--cached", "--name-only"]).is_empty());
        assert_eq!(repo.git(&["log", "-1", "--format=%s"]).trim(), "External");
        restored.set_amend(false);
        assert_eq!(restored.commit_message, "Normal draft");
        restored.create_commit();
        finish_restore(&mut restored);
        assert!(restored.error.is_none());
        assert!(restored.commit_message.is_empty());
        eframe::App::save(&mut restored, &mut storage);
        restored.restore_commit_drafts(&storage);
        restored.load_commit_draft();
        assert!(restored.commit_message.is_empty());
        assert!(restored.commit_drafts.is_empty());
        assert_eq!(
            repo.git(&["log", "-1", "--format=%s"]).trim(),
            "Normal draft"
        );
    }

    #[test]
    fn removing_active_repository_keeps_files_and_persists_the_removal() {
        let repo = crate::test_support::TempRepo::new();
        repo.write("keep.txt", "local changes");
        let path = repo.0.to_string_lossy().into_owned();
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories = vec![crate::models::Repository {
            name: "Test".into(),
            path: path.clone(),
        }];
        app.selected_repository = 0;
        app.changes = crate::mock_data::changes();
        app.commits = crate::mock_data::commits();
        app.refs = crate::mock_data::refs();
        app.commit_message = "Draft".into();
        app.manager.group_name = "Work".into();
        app.add_group();
        app.assign_group(&path, Some("Work".into()));
        app.manager.highlighted_repository = Some(path.clone());
        app.request_remove_repository(path.clone());
        assert_eq!(app.manager.dialog, Some(Dialog::RemoveRepository));
        assert_eq!(app.repositories.len(), 1);
        assert_eq!(app.commit_message, "Draft");
        app.confirm_remove_repository();
        assert!(app.repositories.is_empty());
        assert_eq!(app.selected_repository, usize::MAX);
        assert!(app.changes.is_empty() && app.commits.is_empty() && app.refs.is_empty());
        assert!(app.commit_message.is_empty());
        assert!(app.manager.preferences.groups[0].paths.is_empty());
        assert!(app.manager.highlighted_repository.is_none());
        assert!(repo.0.join(".git").is_dir());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("keep.txt")).unwrap(),
            "local changes"
        );
        let mut storage = Storage::default();
        eframe::App::save(&mut app, &mut storage);
        assert!(
            eframe::get_value::<Vec<String>>(&storage, "sannygit.repositories")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            eframe::get_value::<Option<String>>(&storage, "sannygit.active_repository"),
            Some(None)
        );
    }

    #[test]
    fn removing_inactive_repository_preserves_active_selection_and_draft() {
        for selected in [0, 2, usize::MAX] {
            let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
            app.repositories = crate::mock_data::repositories();
            app.selected_repository = selected;
            let active_path = app.repositories.get(selected).map(|r| r.path.clone());
            app.commit_message = "Keep this draft".into();
            let path = app.repositories[1].path.clone();
            app.commit_drafts.insert(
                path.clone(),
                super::super::CommitDraft {
                    message: "Inactive draft".into(),
                    ..Default::default()
                },
            );
            app.request_remove_repository(path.clone());
            app.confirm_remove_repository();
            assert_eq!(
                app.repositories
                    .get(app.selected_repository)
                    .map(|r| r.path.clone()),
                active_path
            );
            assert_eq!(app.commit_message, "Keep this draft");
            assert!(!app.commit_drafts.contains_key(&path));
            assert!(!app.repositories.iter().any(|r| r.path == path));
        }
    }

    #[test]
    fn removal_requires_a_pending_confirmation_and_an_idle_worker() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
        app.repositories = crate::mock_data::repositories();
        let path = app.repositories[0].path.clone();
        app.request_remove_repository(path.clone());
        app.manager.dialog = None; // Cancelled confirmation.
        app.confirm_remove_repository();
        assert_eq!(app.repositories.len(), 4);
        app.request_remove_repository(path);
        let (_sender, receiver) = mpsc::channel();
        app.worker = Some(receiver);
        app.confirm_remove_repository();
        assert_eq!(app.repositories.len(), 4);
        assert_eq!(app.manager.dialog, Some(Dialog::RemoveRepository));
    }

    #[test]
    fn closing_keeps_bookmarks_and_groups_and_requires_draft_confirmation() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        app.changes = crate::mock_data::changes();
        app.commits = crate::mock_data::commits();
        app.refs = crate::mock_data::refs();
        app.current_branch = "main".into();
        app.manager.group_name = "Work".into();
        app.add_group();
        let path = app.repositories[0].path.clone();
        app.assign_group(&path, Some("Work".into()));
        app.commit_message = "Unsaved message".into();
        let (sender, receiver) = mpsc::channel();
        app.refs_worker = Some(receiver);
        app.close_repository(false);
        assert_eq!(app.manager.dialog, Some(Dialog::CloseRepository));
        assert_eq!(app.selected_repository, 0);
        assert!(!app.changes.is_empty());
        assert!(app.refs_worker.is_some());
        app.close_repository(true);
        assert!(app.refs_worker.is_none());
        assert!(sender
            .send(WorkerResult::Refs {
                path: path.clone(),
                result: Err("Late read".into()),
            })
            .is_err());
        assert_eq!(app.selected_repository, usize::MAX);
        assert!(app.changes.is_empty() && app.refs.is_empty() && app.commits.is_empty());
        assert!(!app.history_pending && !app.refs_pending);
        assert!(!app.state_current);
        assert!(app.commit_message.is_empty());
        assert_eq!(app.repositories.len(), 4);
        assert_eq!(app.manager.preferences.groups[0].paths, vec![path]);
    }
}
