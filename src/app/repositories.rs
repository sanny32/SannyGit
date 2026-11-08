use super::{SannyGitApp, WorkerResult};
use crate::{
    git,
    repository::{self, Preferences, SearchResult, Settings, Worktree},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialog {
    Group,
    Clone,
    Search,
    Worktrees,
    Prune,
    Settings,
    CloseRepository,
}

pub struct RepositoryManager {
    pub preferences: Preferences,
    pub dialog: Option<Dialog>,
    pub highlighted_repository: Option<String>,
    pub clone_step: usize,
    pub clone_options: repository::CloneOptions,
    pub clone_branches: Vec<String>,
    pub clone_parent: String,
    pub clone_name: String,
    pub group_name: String,
    pub clone_source: String,
    pub destination: String,
    pub search_root: String,
    pub search_depth: usize,
    pub found: Vec<(String, String, bool)>,
    pub search_summary: String,
    pub worktrees: Vec<Worktree>,
    pub selected_worktree: Option<String>,
    pub worktree_branch: String,
    pub worktree_new_branch: bool,
    pub confirm_remove: bool,
    pub prune_preview: Option<String>,
    pub settings: Option<Settings>,
    pub cancel: Option<Arc<AtomicBool>>,
}
impl Default for RepositoryManager {
    fn default() -> Self {
        Self {
            preferences: Preferences::default(),
            dialog: None,
            highlighted_repository: None,
            clone_step: 0,
            clone_options: repository::CloneOptions::default(),
            clone_branches: Vec::new(),
            clone_parent: "~".into(),
            clone_name: String::new(),
            group_name: String::new(),
            clone_source: String::new(),
            destination: String::new(),
            search_root: String::new(),
            search_depth: 6,
            found: Vec::new(),
            search_summary: String::new(),
            worktrees: Vec::new(),
            selected_worktree: None,
            worktree_branch: String::new(),
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
            Job::NewWindow => "Opening new window…",
            Job::AddOrCreate(_) => "Opening repository…",
            Job::InspectClone(_) => "Inspecting repository…",
            Job::Clone { .. } => "Cloning repository…",
            Job::Search { .. } => "Searching repositories…",
            Job::ListWorktrees => "Loading worktrees…",
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
                    if self.manager.dialog == Some(Dialog::Prune) {
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

    pub fn close_repository(&mut self, discard_draft: bool) {
        if self.busy() {
            return;
        }
        if !discard_draft && !self.commit_message.is_empty() {
            self.manager.dialog = Some(Dialog::CloseRepository);
            return;
        }
        self.open_dialog = false;
        self.selected_repository = usize::MAX;
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
        self.history_error = None;
        self.refs_current = false;
        self.refs_loading = false;
        self.refs_pending = false;
        self.refs_error = None;
        self.branch_head_exists = false;
        self.branch_dialog_open = false;
        self.remote_checkout = None;
        self.network_dialog = None;
        self.selected_branch.clear();
        self.loaded_diff = None;
        self.diff_preview = None;
        self.selected_commit = None;
        self.commit_details_open = false;
        self.commit_preview = None;
        self.commit_preview_error = None;
        self.loaded_commit = None;
        self.state_current = false;
        self.commit_message.clear();
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
    use std::collections::HashMap;

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
        app.close_repository(false);
        assert_eq!(app.manager.dialog, Some(Dialog::CloseRepository));
        assert_eq!(app.selected_repository, 0);
        assert!(!app.changes.is_empty());
        app.close_repository(true);
        assert_eq!(app.selected_repository, usize::MAX);
        assert!(app.changes.is_empty() && app.refs.is_empty() && app.commits.is_empty());
        assert!(!app.history_pending && !app.refs_pending);
        assert!(!app.state_current);
        assert!(app.commit_message.is_empty());
        assert_eq!(app.repositories.len(), 4);
        assert_eq!(app.manager.preferences.groups[0].paths, vec![path]);
    }
}
