mod remotes;
pub use remotes::RemoteDialog;
mod repositories;
mod tracking;
mod working_tree;
pub use repositories::{Dialog as RepositoryDialog, Job as RepositoryJob, RepositoryManager};

use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

enum WorkerResult {
    RemoteInfo {
        operation: crate::remotes::Operation,
        result: Result<crate::remotes::State, String>,
    },
    RepositoryTask(Box<repositories::ResultMessage>),
    Refs {
        path: String,
        result: Result<crate::models::RefSnapshot, String>,
    },
    History {
        path: String,
        all: bool,
        result: Result<crate::models::History, String>,
    },
    CommitPatch {
        hash: String,
        result: Result<crate::models::DiffPreview, String>,
    },
    CommitMessage(Result<String, String>),
    Repository(Result<crate::git::Snapshot, String>),
    Operation {
        committed: bool,
        success: &'static str,
        outcome: Result<(), String>,
        snapshot: Result<crate::git::Snapshot, String>,
    },
    Diff {
        index: usize,
        result: Result<(String, crate::models::DiffPreview), String>,
    },
}

use crate::{
    git,
    models::{ChangeFile, Commit, Repository},
    theme::{InterfaceStyle, Palette, ThemeChoice, INTERFACE_STYLE_STORAGE_KEY, THEME_STORAGE_KEY},
    ui,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeFilter {
    All,
    Modified,
    Added,
    Deleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsPage {
    General,
    Appearance,
    DateTime,
    Remote,
    Llm,
}

pub struct SettingsWindow {
    pub open: bool,
    pub page: SettingsPage,
    pub theme: ThemeChoice,
    pub preferences: crate::preferences::Preferences,
    pub interface_style: InterfaceStyle,
    pub auto_fetch: bool,
    pub skip_push_confirmation: bool,
    pub llm: crate::llm::Config,
    pub llm_api_key_dirty: bool,
    pub llm_api_key_loaded: bool,
    pub llm_secret_error: Option<String>,
}

impl SettingsWindow {
    fn new(theme: ThemeChoice) -> Self {
        Self {
            open: false,
            page: SettingsPage::Appearance,
            theme,
            preferences: crate::preferences::Preferences {
                follow_system_theme: false,
                ..Default::default()
            },
            interface_style: InterfaceStyle::default(),
            auto_fetch: true,
            skip_push_confirmation: false,
            llm: crate::llm::Config::default(),
            llm_api_key_dirty: false,
            llm_api_key_loaded: false,
            llm_secret_error: None,
        }
    }
}

pub struct RemoteCheckoutDialog {
    pub remote: String,
    pub mode: u8,
    pub local: String,
    pub new_name: String,
}

pub struct SannyGitApp {
    pub manager: RepositoryManager,
    pub tracking: tracking::Monitor,
    #[cfg(target_os = "macos")]
    native_menu: Option<crate::native_menu::NativeMenu>,
    #[cfg(target_os = "macos")]
    pub(crate) pending_native_edit_events: Vec<eframe::egui::Event>,
    pub theme: ThemeChoice,
    pub preferences: crate::preferences::Preferences,
    pub interface_style: InterfaceStyle,
    pub settings_window: SettingsWindow,
    pub llm_config: crate::llm::Config,
    pub about_dialog: bool,
    pub welcome_open: bool,
    welcome_pending: bool,
    applied_theme: ThemeChoice,
    applied_interface_style: InterfaceStyle,
    pub repositories: Vec<Repository>,
    pub refs: Vec<crate::models::BranchRef>,
    pub refs_current: bool,
    pub refs_loading: bool,
    pub refs_error: Option<String>,
    refs_pending: bool,
    pub branch_head_exists: bool,
    pub branch_dialog_open: bool,
    pub tag_dialog_open: bool,
    pub remote_checkout: Option<RemoteCheckoutDialog>,
    pub network_dialog: Option<remotes::RemoteDialog>,
    pub skip_push_confirmation: bool,
    pub new_branch_name: String,
    pub new_tag_name: String,
    pub new_tag_message: String,
    pub switch_to_new_branch: bool,
    pub changes: Vec<ChangeFile>,
    pub commits: Vec<Commit>,
    pub selected_repository: usize,
    pub selected_change: usize,
    pub selected_branch: String,
    pub change_filter: ChangeFilter,
    pub journal_filter: String,
    pub current_branch: String,
    pub commit_message: String,
    pub generating_commit_message: bool,
    pub amend: bool,
    pub repository_path: String,
    pub open_dialog: bool,
    pub error: Option<String>,
    pub status: String,
    worker: Option<Receiver<WorkerResult>>,
    working_tree: working_tree::Monitor,
    loaded_diff: Option<usize>,
    pub diff_preview: Option<crate::models::DiffPreview>,
    pub state_current: bool,
    pub history_all_branches: bool,
    pub history_edges: Vec<crate::models::HistoryEdge>,
    pub history_max_lane: usize,
    pub history_truncated: bool,
    pub history_loading: bool,
    pub history_error: Option<String>,
    history_pending: bool,
    pub selected_commit: Option<String>,
    pub commit_details_open: bool,
    pub commit_preview: Option<crate::models::DiffPreview>,
    pub commit_preview_error: Option<String>,
    loaded_commit: Option<String>,
}

impl SannyGitApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install_fonts(&creation_context.egui_ctx);
        egui_extras::install_image_loaders(&creation_context.egui_ctx);
        let system_theme = if creation_context.egui_ctx.global_style().visuals.dark_mode {
            ThemeChoice::Dark
        } else {
            ThemeChoice::Light
        };
        let saved_theme = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, THEME_STORAGE_KEY));
        let preferences: crate::preferences::Preferences = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, crate::preferences::STORAGE_KEY))
            .unwrap_or_else(|| crate::preferences::Preferences {
                follow_system_theme: saved_theme.is_none(),
                ..Default::default()
            });
        let theme = if preferences.follow_system_theme {
            system_theme
        } else {
            saved_theme.unwrap_or(system_theme)
        };
        let interface_style = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, INTERFACE_STYLE_STORAGE_KEY))
            .unwrap_or_default();
        theme.apply_style(&creation_context.egui_ctx, interface_style);

        let mut app = Self::with_theme(theme);
        app.preferences = preferences;
        app.welcome_pending = app.preferences.show_welcome;
        app.interface_style = interface_style;
        app.applied_interface_style = interface_style;
        app.skip_push_confirmation = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, "sannygit.skip_push_confirmation"))
            .unwrap_or(false);
        app.tracking.auto_fetch = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, "sannygit.auto_fetch"))
            .unwrap_or(true);
        app.llm_config = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, "sannygit.llm"))
            .unwrap_or_default();
        if let Some(api_key) = std::env::var_os("SANNYGIT_LLM_API_KEY") {
            app.llm_config.api_key = api_key.to_string_lossy().into_owned();
        }
        #[cfg(target_os = "macos")]
        {
            app.native_menu = crate::native_menu::NativeMenu::new(&creation_context.egui_ctx);
        }
        if let Some(paths) = creation_context
            .storage
            .and_then(|storage| eframe::get_value::<Vec<String>>(storage, "sannygit.repositories"))
        {
            app.repositories = paths
                .into_iter()
                .map(|path| Repository {
                    name: PathBuf::from(&path)
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.clone()),
                    path,
                })
                .collect();
        }
        if let Some(preferences) = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, "sannygit.repository_preferences"))
        {
            app.manager.preferences = preferences;
        }
        if let Some(path) = std::env::var_os("SANNYGIT_OPEN_REPOSITORY") {
            app.open_repository(PathBuf::from(path).to_string_lossy().into_owned());
        } else if let Some(storage) = creation_context.storage {
            app.restore_active_repository(storage);
        }
        app
    }

    fn restore_active_repository(&mut self, storage: &dyn eframe::Storage) {
        let Some(Some(path)) =
            eframe::get_value::<Option<String>>(storage, "sannygit.active_repository")
        else {
            return;
        };
        if self
            .repositories
            .iter()
            .any(|repository| repository.path == path)
        {
            self.open_repository(path);
        }
    }

    pub(crate) fn with_theme(theme: ThemeChoice) -> Self {
        Self {
            manager: RepositoryManager::default(),
            tracking: tracking::Monitor::default(),
            #[cfg(target_os = "macos")]
            native_menu: None,
            #[cfg(target_os = "macos")]
            pending_native_edit_events: Vec::new(),
            theme,
            preferences: crate::preferences::Preferences {
                follow_system_theme: false,
                ..Default::default()
            },
            interface_style: InterfaceStyle::default(),
            settings_window: SettingsWindow::new(theme),
            llm_config: crate::llm::Config::default(),
            about_dialog: false,
            welcome_open: false,
            welcome_pending: false,
            applied_theme: theme,
            applied_interface_style: InterfaceStyle::default(),
            repositories: Vec::new(),
            refs: Vec::new(),
            refs_current: false,
            refs_loading: false,
            refs_error: None,
            refs_pending: false,
            branch_head_exists: false,
            branch_dialog_open: false,
            tag_dialog_open: false,
            remote_checkout: None,
            network_dialog: None,
            skip_push_confirmation: false,
            new_branch_name: String::new(),
            new_tag_name: String::new(),
            new_tag_message: String::new(),
            switch_to_new_branch: true,
            changes: Vec::new(),
            commits: Vec::new(),
            selected_repository: usize::MAX,
            selected_change: usize::MAX,
            selected_branch: String::new(),
            change_filter: ChangeFilter::All,
            journal_filter: String::new(),
            current_branch: String::new(),
            commit_message: String::new(),
            generating_commit_message: false,
            amend: false,
            repository_path: String::new(),
            open_dialog: false,
            error: None,
            status: "Ready".into(),
            worker: None,
            working_tree: working_tree::Monitor::default(),
            loaded_diff: None,
            diff_preview: None,
            state_current: false,
            history_all_branches: false,
            history_edges: Vec::new(),
            history_max_lane: 0,
            history_truncated: false,
            history_loading: false,
            history_error: None,
            history_pending: false,
            selected_commit: None,
            commit_details_open: false,
            commit_preview: None,
            commit_preview_error: None,
            loaded_commit: None,
        }
    }
    pub fn selected_diff(&self) -> Option<&crate::models::DiffPreview> {
        (self.loaded_diff == Some(self.selected_change))
            .then_some(self.diff_preview.as_ref())
            .flatten()
    }

    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }

    fn start_worker(&mut self, job: impl FnOnce() -> WorkerResult + Send + 'static) -> bool {
        // A user operation supersedes any in-flight automatic read.
        self.working_tree.invalidate();
        self.tracking.cancel();
        let (sender, receiver) = mpsc::channel();
        match std::thread::Builder::new()
            .name("sannygit-reader".into())
            .spawn(move || {
                let _ = sender.send(job());
            }) {
            Ok(_) => {
                self.worker = Some(receiver);
                true
            }
            Err(e) => {
                self.error = Some(format!("Cannot start background worker: {e}"));
                self.status = "Operation failed".into();
                false
            }
        }
    }

    pub fn open_repository(&mut self, path: String) {
        if self.busy() {
            return;
        }
        self.error = None;
        self.state_current = false;
        self.status = "Loading repository…".into();
        self.start_worker(move || WorkerResult::Repository(git::load(&PathBuf::from(path))));
    }

    pub fn menu_action_enabled(&self, action: crate::menu::MenuAction) -> bool {
        use crate::menu::MenuAction;
        match action {
            MenuAction::NewWindow
            | MenuAction::AddGroup
            | MenuAction::Clone
            | MenuAction::SearchRepositories
            | MenuAction::EditUserConfig => !self.busy(),
            MenuAction::CloseRepository
            | MenuAction::AddWorktree
            | MenuAction::RemoveWorktree
            | MenuAction::PruneWorktrees
            | MenuAction::RepositorySettings
            | MenuAction::EditRepositoryConfig => {
                !self.busy() && self.repositories.get(self.selected_repository).is_some()
            }
            MenuAction::Open
            | MenuAction::Preferences
            | MenuAction::LightTheme
            | MenuAction::DarkTheme
            | MenuAction::Close
            | MenuAction::About => true,
            MenuAction::Refresh | MenuAction::ReloadHistory => {
                !self.busy() && self.repositories.get(self.selected_repository).is_some()
            }
            MenuAction::Stage => self.staging_allowed(self.selected_change, true),
            MenuAction::Unstage => self.staging_allowed(self.selected_change, false),
            MenuAction::Commit => self.commit_blocker().is_none(),
            MenuAction::CreateBranch => {
                !self.busy() && self.state_current && self.refs_current && self.branch_head_exists
            }
            MenuAction::SwitchBranch => self.switch_branch_blocker().is_none(),
            MenuAction::Fetch | MenuAction::Pull | MenuAction::Push => self.remote_allowed(),
        }
    }

    pub fn handle_menu_action(
        &mut self,
        action: crate::menu::MenuAction,
        context: &eframe::egui::Context,
    ) {
        if !self.menu_action_enabled(action) {
            return;
        }
        use crate::menu::MenuAction;
        match action {
            MenuAction::NewWindow => self.run_repository_job(RepositoryJob::NewWindow),
            MenuAction::AddGroup => self.manager.dialog = Some(RepositoryDialog::Group),
            MenuAction::Clone => {
                self.manager.clone_step = 0;
                self.manager.dialog = Some(RepositoryDialog::Clone);
            }
            MenuAction::SearchRepositories => self.manager.dialog = Some(RepositoryDialog::Search),
            MenuAction::CloseRepository => self.close_repository(false),
            MenuAction::AddWorktree | MenuAction::RemoveWorktree => {
                self.manager.dialog = Some(RepositoryDialog::Worktrees);
                self.manager.confirm_remove = false;
                self.run_repository_job(RepositoryJob::ListWorktrees);
            }
            MenuAction::PruneWorktrees => {
                self.manager.dialog = Some(RepositoryDialog::Prune);
                self.manager.prune_preview = None;
                self.run_repository_job(RepositoryJob::PreviewPrune);
            }
            MenuAction::RepositorySettings => {
                self.manager.dialog = Some(RepositoryDialog::Settings);
                self.manager.settings = None;
                self.run_repository_job(RepositoryJob::LoadSettings);
            }
            MenuAction::EditRepositoryConfig => {
                self.run_repository_job(RepositoryJob::EditConfig { global: false })
            }
            MenuAction::EditUserConfig => {
                self.run_repository_job(RepositoryJob::EditConfig { global: true })
            }
            MenuAction::Open => self.open_dialog = true,
            MenuAction::Refresh => self.refresh(),
            MenuAction::Stage => self.set_staged(self.selected_change, true),
            MenuAction::Unstage => self.set_staged(self.selected_change, false),
            MenuAction::Commit => self.create_commit(),
            MenuAction::CreateBranch => self.branch_dialog_open = true,
            MenuAction::SwitchBranch => self.switch_branch(),
            MenuAction::Fetch => self.open_remote(crate::remotes::Operation::Fetch),
            MenuAction::Pull => self.open_remote(crate::remotes::Operation::Pull),
            MenuAction::Push => self.open_remote(crate::remotes::Operation::Push),
            MenuAction::ReloadHistory => self.reload_history(),
            MenuAction::Preferences => self.open_settings(),
            MenuAction::LightTheme => {
                self.preferences.follow_system_theme = false;
                self.theme = ThemeChoice::Light;
            }
            MenuAction::DarkTheme => {
                self.preferences.follow_system_theme = false;
                self.theme = ThemeChoice::Dark;
            }
            MenuAction::Close => context.send_viewport_cmd(eframe::egui::ViewportCommand::Close),
            MenuAction::About => self.about_dialog = true,
        }
        context.request_repaint();
    }

    fn finish_startup(&mut self) {
        if self.welcome_pending && !self.busy() && self.error.is_none() {
            self.welcome_open = self.preferences.show_welcome
                && self.repositories.get(self.selected_repository).is_none();
            self.welcome_pending = false;
        }
    }

    pub fn palette(&self) -> Palette {
        self.interface_style.palette(self.theme)
    }

    pub fn open_settings(&mut self) {
        self.settings_window.preferences = self.preferences.clone();
        self.settings_window.interface_style = self.interface_style;
        self.settings_window.theme = self.theme;
        self.settings_window.auto_fetch = self.tracking.auto_fetch;
        self.settings_window.skip_push_confirmation = self.skip_push_confirmation;
        self.settings_window.llm = self.llm_config.clone();
        self.settings_window.llm_api_key_dirty = false;
        self.settings_window.llm_api_key_loaded = false;
        self.settings_window.llm_secret_error = None;
        self.settings_window.open = true;
    }

    pub fn refresh(&mut self) {
        if let Some(repository) = self.repositories.get(self.selected_repository) {
            self.open_repository(repository.path.clone());
        }
    }

    fn apply_snapshot(&mut self, snapshot: git::Snapshot) {
        if self.current_branch != snapshot.branch {
            self.tracking.branch_changed();
        } else {
            self.tracking.invalidate();
        }
        self.state_current = true;
        let selected = self
            .changes
            .get(self.selected_change)
            .map(|c| c.path.clone());
        let new_repository = !self
            .repositories
            .iter()
            .any(|r| r.path == snapshot.repository.path);
        let index = self
            .repositories
            .iter()
            .position(|r| r.path == snapshot.repository.path)
            .unwrap_or_else(|| {
                self.repositories.push(snapshot.repository);
                self.repositories.len() - 1
            });
        let same_repository = self.selected_repository == index;
        self.selected_repository = index;
        if !same_repository {
            self.manager.highlighted_repository = Some(self.repositories[index].path.clone());
        }
        if new_repository {
            let path = self.repositories[index].path.clone();
            self.assign_group(&path, self.manager.preferences.selected_group.clone());
        }
        self.current_branch = snapshot.branch;
        self.changes = snapshot.changes;
        self.selected_change = if same_repository {
            selected.and_then(|path| self.changes.iter().position(|c| c.path == path))
        } else {
            None
        }
        .or_else(|| (!self.changes.is_empty()).then_some(0))
        .unwrap_or(usize::MAX);
        self.loaded_diff = None;
        self.diff_preview = None;
        self.open_dialog = false;
        self.refs_current = false;
        self.refs_pending = true;
        self.refs_loading = true;
        self.refs_error = None;
        self.history_pending = true;
        self.history_loading = true;
        self.history_error = None;
        self.commit_preview = None;
        self.commit_preview_error = None;
        self.loaded_commit = None;
        if !same_repository {
            self.manager.dialog = None;
            self.manager.worktrees.clear();
            self.manager.settings = None;
            self.selected_branch.clear();
            self.refs.clear();
            self.branch_head_exists = false;
            self.branch_dialog_open = false;
            self.remote_checkout = None;
            self.network_dialog = None;
            self.new_branch_name.clear();
            self.commits.clear();
            self.history_edges.clear();
            self.history_max_lane = 0;
            self.history_truncated = false;
            self.selected_commit = None;
            self.commit_details_open = false;
            self.commit_preview_error = None;
            self.commit_message.clear();
            self.journal_filter.clear();
        }
        self.status = if self.changes.is_empty() {
            "Working tree clean"
        } else {
            "Ready"
        }
        .into();
    }

    fn load_refs(&mut self) {
        if self.busy() || !self.refs_pending {
            return;
        }
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let path = repository.path.clone();
        self.refs_pending = false;
        if !self.start_worker(move || WorkerResult::Refs {
            result: crate::branches::load(&PathBuf::from(&path)),
            path,
        }) {
            self.refs_loading = false;
            self.refs_error = self.error.clone();
        }
    }

    fn branch_base_blocker(&self) -> Option<&'static str> {
        if self.busy() {
            Some("Wait for the current operation to finish")
        } else if self.repositories.get(self.selected_repository).is_none() {
            Some("Open a repository first")
        } else if !self.state_current || !self.refs_current {
            Some("Refresh the repository and branches before continuing")
        } else {
            None
        }
    }

    pub fn switch_branch_blocker(&self) -> Option<&'static str> {
        if let Some(reason) = self.branch_base_blocker() {
            return Some(reason);
        }
        let Some(target) = self
            .refs
            .iter()
            .find(|r| r.full_name == self.selected_branch)
        else {
            return Some("Select a branch or tag");
        };
        if target.current {
            return Some("This branch is already checked out");
        }
        if target.kind == crate::models::RefKind::Local {
            if let Some(reason) = crate::branches::name_hint(&target.name) {
                return Some(reason);
            }
        }
        if !self.changes.is_empty() {
            return Some("Commit or move aside local changes before switching branches");
        }
        None
    }

    pub fn create_branch_blocker(&self) -> Option<&'static str> {
        if let Some(reason) = self.branch_base_blocker() {
            return Some(reason);
        }
        if !self.branch_head_exists {
            return Some("Create the first commit before creating another branch");
        }
        if let Some(reason) = crate::branches::name_hint(&self.new_branch_name) {
            return Some(reason);
        }
        if self
            .refs
            .iter()
            .any(|r| r.full_name == format!("refs/heads/{}", self.new_branch_name))
        {
            return Some("A local branch with this name already exists");
        }
        if self.switch_to_new_branch && !self.changes.is_empty() {
            return Some("Commit or move aside changes, or disable switching to the new branch");
        }
        None
    }

    pub fn switch_branch(&mut self) {
        if self.switch_branch_blocker().is_some() {
            return;
        }
        if let Some(remote) = self.refs.iter().find(|r| {
            r.full_name == self.selected_branch && r.kind == crate::models::RefKind::Remote
        }) {
            if let Some(local) = crate::branches::local_for_remote(&self.refs, remote) {
                let current = local.current;
                self.selected_branch = local.full_name.clone();
                if current {
                    self.status = "Local branch already checked out".into();
                    return;
                }
            } else {
                self.remote_checkout = Some(RemoteCheckoutDialog {
                    remote: remote.full_name.clone(),
                    mode: 1,
                    local: self
                        .refs
                        .iter()
                        .find(|r| r.kind == crate::models::RefKind::Local)
                        .map(|r| r.full_name.clone())
                        .unwrap_or_default(),
                    new_name: remote
                        .name
                        .split_once('/')
                        .map(|(_, name)| name)
                        .unwrap_or(&remote.name)
                        .into(),
                });
                self.error = None;
                return;
            }
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let target = self.selected_branch.clone();
        self.state_current = false;
        self.refs_current = false;
        self.error = None;
        self.status = "Switching branch or tag…".into();
        self.start_worker(move || {
            let outcome = crate::branches::switch(&root, &target);
            let snapshot = git::load(&root);
            WorkerResult::Operation {
                committed: false,
                success: "Branch or tag switched",
                outcome,
                snapshot,
            }
        });
    }

    pub fn checkout_remote(
        &mut self,
        remote: String,
        choice: crate::branches::RemoteChoice,
    ) -> bool {
        if let Some(reason) = self.branch_base_blocker() {
            self.error = Some(reason.into());
            return false;
        }
        if !self.changes.is_empty() {
            self.error = Some("Commit or move aside local changes before checking out.".into());
            return false;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        self.state_current = false;
        self.refs_current = false;
        self.error = None;
        self.status = "Checking out remote branch…".into();
        self.start_worker(move || {
            let outcome = crate::branches::checkout_remote(&root, &remote, &choice);
            WorkerResult::Operation {
                committed: false,
                success: "Checkout complete",
                outcome,
                snapshot: git::load(&root),
            }
        })
    }

    pub fn create_branch(&mut self) {
        if self.create_branch_blocker().is_some() {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let name = self.new_branch_name.clone();
        let switch_to = self.switch_to_new_branch;
        self.state_current = false;
        self.refs_current = false;
        self.error = None;
        self.status = "Creating branch…".into();
        self.start_worker(move || {
            let outcome = crate::branches::create(&root, &name, switch_to);
            let snapshot = git::load(&root);
            WorkerResult::Operation {
                committed: false,
                success: if switch_to {
                    "Branch created and switched"
                } else {
                    "Branch created"
                },
                outcome,
                snapshot,
            }
        });
    }

    pub fn reload_history(&mut self) {
        if self.busy() || self.repositories.get(self.selected_repository).is_none() {
            return;
        }
        self.history_pending = true;
        self.history_loading = true;
        self.history_error = None;
    }

    pub fn select_commit(&mut self, hash: String) {
        if !self.commits.iter().any(|c| c.hash == hash) {
            return;
        }
        if self.selected_commit.as_ref() != Some(&hash) {
            self.commit_preview = None;
            self.commit_preview_error = None;
            self.loaded_commit = None;
        }
        self.selected_commit = Some(hash);
        self.commit_details_open = true;
    }

    pub fn retry_commit_preview(&mut self) {
        if self.busy() {
            return;
        }
        self.loaded_commit = None;
        self.commit_preview_error = None;
    }

    fn load_history(&mut self) {
        if self.busy() || !self.history_pending {
            return;
        }
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let path = repository.path.clone();
        let all = self.history_all_branches;
        self.history_pending = false;
        if !self.start_worker(move || WorkerResult::History {
            result: crate::history::load(&PathBuf::from(&path), all),
            path,
            all,
        }) {
            self.history_loading = false;
            self.history_error = self.error.clone();
        }
    }

    fn load_commit_preview(&mut self) {
        if self.busy() || !self.commit_details_open {
            return;
        }
        let Some(hash) = self.selected_commit.clone() else {
            return;
        };
        if self.loaded_commit.as_ref() == Some(&hash) {
            return;
        }
        let Some(commit) = self.commits.iter().find(|c| c.hash == hash) else {
            return;
        };
        let parent = commit.parents.first().cloned();
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let root = PathBuf::from(&repository.path);
        self.commit_preview = None;
        self.loaded_commit = Some(hash.clone());
        if !self.start_worker(move || WorkerResult::CommitPatch {
            result: crate::history::patch(&root, &hash, parent.as_deref())
                .map(|patch| crate::models::DiffPreview::new(&patch)),
            hash,
        }) {
            self.commit_preview_error = self.error.clone();
        }
    }

    pub fn staging_allowed(&self, index: usize, staged: bool) -> bool {
        !self.busy()
            && self.state_current
            && self.repositories.get(self.selected_repository).is_some()
            && self.changes.get(index).is_some_and(|c| {
                c.kind != crate::models::ChangeKind::Conflicted
                    && if staged { c.unstaged } else { c.staged }
            })
    }

    pub fn set_staged(&mut self, index: usize, staged: bool) {
        if !self.staging_allowed(index, staged) {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let path = self.changes[index].path.clone();
        self.state_current = false;
        self.error = None;
        self.status = if staged {
            "Staging file…"
        } else {
            "Unstaging file…"
        }
        .into();
        self.start_worker(move || {
            let outcome = git::set_staged(&root, &path, staged);
            let snapshot = git::load(&root);
            WorkerResult::Operation {
                committed: false,
                success: "Index updated",
                outcome,
                snapshot,
            }
        });
    }

    pub fn commit_blocker(&self) -> Option<&'static str> {
        if self.busy() {
            Some("Wait for the current operation to finish")
        } else if self.repositories.get(self.selected_repository).is_none() {
            Some("Open a repository first")
        } else if !self.state_current {
            Some("Refresh the repository before committing")
        } else if self
            .changes
            .iter()
            .any(|c| c.kind == crate::models::ChangeKind::Conflicted)
        {
            Some("Resolve all conflicts before committing")
        } else if self.changes.is_empty() {
            Some("There are no changes to commit")
        } else if self.commit_message.trim().is_empty() {
            Some("Enter a commit message")
        } else if self.commit_message.contains('\0') || self.commit_message.len() > 65_536 {
            Some("Use a message without NUL characters and no larger than 64 KiB")
        } else {
            None
        }
    }

    pub fn create_commit(&mut self) {
        if self.commit_blocker().is_some() {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let message = self.commit_message.clone();
        self.state_current = false;
        self.error = None;
        self.status = "Creating commit…".into();
        self.start_worker(move || {
            let outcome = git::commit(&root, &message);
            let committed = outcome.is_ok();
            let snapshot = git::load(&root);
            WorkerResult::Operation {
                committed,
                success: "Commit created",
                outcome,
                snapshot,
            }
        });
    }

    pub fn generate_commit_message_blocker(&self) -> Option<&'static str> {
        if self.busy() {
            Some("Wait for the current operation to finish")
        } else if self.repositories.get(self.selected_repository).is_none() {
            Some("Open a repository first")
        } else if !self.state_current {
            Some("Refresh the repository before generating a message")
        } else if self.changes.is_empty() {
            Some("There are no changes to describe")
        } else {
            None
        }
    }

    pub fn generate_commit_message(&mut self) {
        if self.generate_commit_message_blocker().is_some() {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let mut config = self.llm_config.clone();
        self.error = None;
        self.status = "Generating commit message…".into();
        self.generating_commit_message = self.start_worker(move || {
            WorkerResult::CommitMessage(git::commit_patch(&root).and_then(|patch| {
                crate::llm::load_api_key_if_needed(&mut config)?;
                crate::llm::generate(&config, &patch)
            }))
        });
    }

    fn poll_worker(&mut self) {
        let Some(receiver) = &self.worker else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.worker = None;
                self.generating_commit_message = false;
                self.manager.cancel = None;
                self.error = Some("Background worker stopped unexpectedly".into());
                if self.refs_loading && !self.refs_pending {
                    self.refs_loading = false;
                    self.refs_current = false;
                    self.refs_error = self.error.clone();
                }
                if self.history_loading && !self.history_pending {
                    self.history_loading = false;
                    self.history_error = self.error.clone();
                }
                if self.commit_details_open
                    && self.loaded_commit.is_some()
                    && self.commit_preview.is_none()
                {
                    self.commit_preview_error = self.error.clone();
                }
                self.status = "Operation failed".into();
                if self.push_dialog_hidden() {
                    self.finish_hidden_push();
                }
                return;
            }
        };
        self.worker = None;
        match result {
            WorkerResult::RemoteInfo { operation, result } => {
                self.apply_remote_info(operation, result)
            }
            WorkerResult::RepositoryTask(result) => self.apply_repository_result(*result),
            WorkerResult::Refs { path, result } => {
                if self
                    .repositories
                    .get(self.selected_repository)
                    .is_some_and(|r| r.path == path)
                {
                    self.refs_loading = false;
                    match result {
                        Ok(snapshot) => {
                            self.refs = snapshot.refs;
                            self.branch_head_exists = snapshot.has_head;
                            self.refs_current = true;
                            self.refs_error = None;
                            if !self
                                .refs
                                .iter()
                                .any(|r| r.full_name == self.selected_branch)
                            {
                                self.selected_branch = self
                                    .refs
                                    .iter()
                                    .find(|r| r.current)
                                    .map(|r| r.full_name.clone())
                                    .unwrap_or_default();
                            }
                        }
                        Err(error) => {
                            self.refs_current = false;
                            self.refs_error = Some(error);
                        }
                    }
                }
            }
            WorkerResult::History { path, all, result } => {
                if self
                    .repositories
                    .get(self.selected_repository)
                    .is_some_and(|r| r.path == path)
                    && self.history_all_branches == all
                {
                    self.history_loading = false;
                    match result {
                        Ok(history) => {
                            self.commits = history.commits;
                            self.history_edges = history.edges;
                            self.history_max_lane = history.max_lane;
                            self.history_truncated = history.truncated;
                            self.history_error = None;
                            if !self
                                .commits
                                .iter()
                                .any(|c| Some(&c.hash) == self.selected_commit.as_ref())
                            {
                                self.selected_commit = None;
                                self.commit_details_open = false;
                                self.commit_preview = None;
                                self.loaded_commit = None;
                            }
                        }
                        Err(error) => self.history_error = Some(error),
                    }
                }
            }
            WorkerResult::CommitPatch { hash, result } => {
                if self.selected_commit.as_ref() == Some(&hash) {
                    match result {
                        Ok(preview) => {
                            self.commit_preview = Some(preview);
                            self.commit_preview_error = None;
                        }
                        Err(error) => self.commit_preview_error = Some(error),
                    }
                }
            }
            WorkerResult::CommitMessage(result) => {
                self.generating_commit_message = false;
                match result {
                    Ok(message) => {
                        self.commit_message = message;
                        self.status = "Commit message generated".into();
                    }
                    Err(error) => {
                        self.error = Some(error);
                        self.status = "Could not generate commit message".into();
                    }
                }
            }
            WorkerResult::Repository(Ok(snapshot)) => {
                self.apply_snapshot(snapshot);
            }
            WorkerResult::Repository(Err(error)) => {
                self.error = Some(error);
                self.status = "Could not load repository; previous view retained".into();
            }
            WorkerResult::Operation {
                committed,
                success,
                outcome,
                snapshot,
            } => {
                let hidden_push = success == "Push complete" && self.push_dialog_hidden();
                if matches!(
                    success,
                    "Fetch complete" | "Pull complete" | "Push complete"
                ) {
                    self.manager.cancel = None;
                    if outcome.is_ok() {
                        let checked_upstream = self.network_dialog.as_ref().is_some_and(|dialog| {
                            dialog.set_upstream
                                || self.tracking.status.as_ref().is_some_and(|status| {
                                    status.remote.as_deref() == Some(dialog.remote.as_str())
                                        && (success == "Fetch complete"
                                            || status.remote_ref.as_deref()
                                                == Some(
                                                    format!("refs/heads/{}", dialog.branch)
                                                        .as_str(),
                                                ))
                                })
                        });
                        if checked_upstream {
                            self.tracking.checked = true;
                            self.tracking.error = None;
                        }
                        self.network_dialog = None;
                    }
                }
                if committed {
                    self.commit_message.clear();
                }
                if outcome.is_ok() && success.starts_with("Branch created") {
                    self.branch_dialog_open = false;
                    self.new_branch_name.clear();
                }
                if outcome.is_ok() && success == "Checkout complete" {
                    self.remote_checkout = None;
                }
                let refreshed = snapshot.is_ok();
                match snapshot {
                    Ok(snapshot) => self.apply_snapshot(snapshot),
                    Err(error) => {
                        self.state_current = false;
                        self.error =
                            Some(format!("Could not refresh after the operation: {error}"));
                        self.status = "Refresh required before further operations".into();
                    }
                }
                match outcome {
                    Ok(()) => {
                        if refreshed {
                            self.status = success.into();
                        }
                    }
                    Err(error) => {
                        self.error = Some(error);
                        self.status =
                            "Operation failed; check the refreshed repository state".into();
                    }
                }
                if hidden_push {
                    self.finish_hidden_push();
                }
            }
            WorkerResult::Diff { index, result } => {
                if self.selected_change == index {
                    self.loaded_diff = Some(index);
                    match result {
                        Ok((diff, preview)) => {
                            self.diff_preview = Some(preview);
                            if let Some(change) = self.changes.get_mut(index) {
                                change.diff = diff;
                            }
                            if self.status == "Loading diff…" {
                                self.status = "Ready".into();
                            }
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.status = "Could not load diff".into();
                        }
                    }
                }
            }
        }
    }

    fn load_selected_diff(&mut self) {
        if self.busy() || self.loaded_diff == Some(self.selected_change) {
            return;
        }
        let Some(change) = self.changes.get(self.selected_change) else {
            return;
        };
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let root = PathBuf::from(&repository.path);
        let path = change.path.clone();
        let kind = change.kind;
        let original_path = change.original_path.clone();
        let index = self.selected_change;
        self.diff_preview = None;
        if let Some(previous) = self.loaded_diff.and_then(|i| self.changes.get_mut(i)) {
            previous.diff = String::new();
        }
        self.loaded_diff = Some(index);
        if self.error.is_none()
            && !self.status.starts_with("Push ")
            && !matches!(
                self.status.as_str(),
                "Commit created"
                    | "Index updated"
                    | "Branch switched"
                    | "Branch created"
                    | "Branch created and switched"
            )
        {
            self.status = "Loading diff…".into();
        }
        self.start_worker(move || WorkerResult::Diff {
            index,
            result: git::diff(&root, &path, kind, original_path.as_deref()).map(|patch| {
                let preview = crate::models::DiffPreview::new(&patch);
                (patch, preview)
            }),
        });
    }
}

impl eframe::App for SannyGitApp {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        if self.preferences.follow_system_theme {
            self.theme = ThemeChoice::from_system(ui.ctx(), self.theme);
        }
        if self.applied_theme != self.theme || self.applied_interface_style != self.interface_style
        {
            self.theme.apply_style(ui.ctx(), self.interface_style);
            self.applied_interface_style = self.interface_style;
            self.applied_theme = self.theme;
        }
        self.poll_worker();
        self.finish_startup();
        self.poll_working_tree();
        let root = self
            .repositories
            .get(self.selected_repository)
            .map(|r| PathBuf::from(&r.path));
        let allow_tracking = !self.busy()
            && !self.refs_pending
            && !self.refs_loading
            && !self.history_pending
            && !self.history_loading
            && self.state_current
            && !self.open_dialog
            && !self.welcome_open
            && self.manager.dialog.is_none()
            && self.network_dialog.is_none()
            && self.remote_checkout.is_none();
        self.tracking.update(root, allow_tracking, ui.ctx());
        if self.tracking.refs_changed {
            self.tracking.refs_changed = false;
            self.refs_pending = true;
            self.refs_loading = true;
            self.refs_current = false;
            if self.history_all_branches {
                self.history_pending = true;
                self.history_loading = true;
            }
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));
        #[cfg(target_os = "macos")]
        {
            let (actions, edit_events) = self.native_menu.as_ref().map_or_else(
                || (Vec::new(), Vec::new()),
                |menu| (menu.take_actions(), menu.take_edit_events()),
            );
            if self.settings_window.open {
                self.pending_native_edit_events.extend(edit_events);
            } else {
                ui.input_mut(|input| input.events.extend(edit_events));
            }
            for action in actions {
                self.handle_menu_action(action, ui.ctx());
            }
        }
        ui::show(ui, self);
        self.load_selected_diff();
        self.load_refs();
        self.load_history();
        self.load_commit_preview();
        self.check_working_tree(ui.ctx());
        #[cfg(target_os = "macos")]
        if let Some(menu) = &self.native_menu {
            menu.update_enabled(|action| self.menu_action_enabled(action));
        }
        if self.busy() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.tracking.cancel();
        if let Some(cancel) = &self.manager.cancel {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(receiver) = &self.worker {
                let _ = receiver.recv_timeout(std::time::Duration::from_secs(3));
            }
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            "sannygit.skip_push_confirmation",
            &self.skip_push_confirmation,
        );
        eframe::set_value(storage, "sannygit.auto_fetch", &self.tracking.auto_fetch);
        eframe::set_value(storage, "sannygit.llm", &self.llm_config);
        eframe::set_value(storage, THEME_STORAGE_KEY, &self.theme);
        eframe::set_value(storage, crate::preferences::STORAGE_KEY, &self.preferences);
        eframe::set_value(storage, INTERFACE_STYLE_STORAGE_KEY, &self.interface_style);
        let paths: Vec<_> = self.repositories.iter().map(|r| &r.path).collect();
        eframe::set_value(storage, "sannygit.repositories", &paths);
        let active = self
            .repositories
            .get(self.selected_repository)
            .map(|r| &r.path);
        eframe::set_value(storage, "sannygit.active_repository", &active);
        eframe::set_value(
            storage,
            "sannygit.repository_preferences",
            &self.manager.preferences,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{self, epaint::ClippedShape, Rect, Vec2};

    fn deliver(app: &mut SannyGitApp, result: WorkerResult) {
        let (sender, receiver) = mpsc::channel();
        app.worker = Some(receiver);
        sender.send(result).ok().unwrap();
        app.poll_worker();
    }

    #[test]
    fn repository_error_retains_view_and_refresh_preserves_selection() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories = crate::mock_data::repositories();
        app.changes = crate::mock_data::changes();
        app.selected_repository = 0;
        app.selected_change = 2;
        let selected = app.changes[2].path.clone();
        deliver(
            &mut app,
            WorkerResult::Repository(Err("Cannot read repository".into())),
        );
        assert_eq!(app.changes[app.selected_change].path, selected);
        assert!(app.error.is_some());
        let mut changes = crate::mock_data::changes();
        changes.reverse();
        let snapshot = git::Snapshot {
            repository: Repository {
                name: app.repositories[0].name.clone(),
                path: app.repositories[0].path.clone(),
            },
            branch: "real-branch".into(),
            changes,
        };
        deliver(&mut app, WorkerResult::Repository(Ok(snapshot)));
        assert_eq!(app.changes[app.selected_change].path, selected);
        assert_eq!(app.current_branch, "real-branch");
        assert!(app.commits.is_empty());
    }

    #[test]
    fn changing_selection_hides_previous_diff_and_discards_late_result() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Dark);
        app.changes = crate::mock_data::changes();
        app.selected_change = 0;
        app.loaded_diff = Some(0);
        app.diff_preview = Some(crate::models::DiffPreview::new("+old"));
        assert!(app.selected_diff().is_some());
        app.selected_change = 1;
        assert!(app.selected_diff().is_none());
        deliver(
            &mut app,
            WorkerResult::Diff {
                index: 0,
                result: Err("stale error".into()),
            },
        );
        assert!(app.error.is_none());
        assert!(app.selected_diff().is_none());
    }

    #[test]
    fn generated_commit_message_updates_draft_and_errors_preserve_it() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.commit_message = "Existing draft".into();
        app.generating_commit_message = true;
        deliver(
            &mut app,
            WorkerResult::CommitMessage(Ok("Generated message".into())),
        );
        assert_eq!(app.commit_message, "Generated message");
        assert_eq!(app.status, "Commit message generated");
        assert!(!app.generating_commit_message);

        app.generating_commit_message = true;
        deliver(
            &mut app,
            WorkerResult::CommitMessage(Err("Provider unavailable".into())),
        );
        assert_eq!(app.commit_message, "Generated message");
        assert_eq!(app.error.as_deref(), Some("Provider unavailable"));
        assert!(!app.generating_commit_message);
    }

    #[test]
    fn commit_and_generation_accept_unstaged_and_untracked_files() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        assert!(app.commit_blocker().is_some());
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        app.changes = crate::mock_data::changes();
        app.commit_message = "Message".into();
        assert!(app.commit_blocker().unwrap().contains("Refresh"));
        app.state_current = true;
        for change in &mut app.changes {
            change.staged = false;
        }
        assert!(app.commit_blocker().is_none());
        assert!(app.generate_commit_message_blocker().is_none());
        for change in &mut app.changes {
            change.kind = crate::models::ChangeKind::Untracked;
        }
        assert!(app.commit_blocker().is_none());
        assert!(app.generate_commit_message_blocker().is_none());
        let changes = std::mem::take(&mut app.changes);
        assert!(app.commit_blocker().is_some());
        assert!(app.generate_commit_message_blocker().is_some());
        app.changes = changes;
        app.changes[0].kind = crate::models::ChangeKind::Modified;
        app.commit_message = " \n\t".into();
        assert!(app.commit_blocker().unwrap().contains("message"));
        app.create_commit();
        assert!(!app.busy());
        app.commit_message = "Valid message".into();
        assert!(app.commit_blocker().is_none());
        app.changes[0].staged = true;
        assert!(app.staging_allowed(0, false));
        app.changes[0].kind = crate::models::ChangeKind::Conflicted;
        assert!(app.commit_blocker().unwrap().contains("conflicts"));
        assert!(!app.staging_allowed(0, true));
        assert!(!app.staging_allowed(0, false));
    }

    #[test]
    fn skipped_push_reports_results_without_dialogs() {
        for skip in [false, true] {
            for failed in [false, true] {
                for refresh_failed in [false, true] {
                    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
                    app.repositories = vec![Repository {
                        name: "test".into(),
                        path: "/test".into(),
                    }];
                    app.selected_repository = 0;
                    app.skip_push_confirmation = skip;
                    app.network_dialog = Some(RemoteDialog {
                        operation: crate::remotes::Operation::Push,
                        state: Some(crate::remotes::State {
                            remotes: vec!["origin".into()],
                            branch: Some("main".into()),
                            upstream_remote: Some("origin".into()),
                            upstream_ref: Some("main".into()),
                        }),
                        remote: "origin".into(),
                        branch: "main".into(),
                        set_upstream: false,
                        dont_show_again: false,
                    });
                    assert_eq!(app.push_dialog_hidden(), skip);
                    deliver(
                        &mut app,
                        WorkerResult::Operation {
                            committed: false,
                            success: "Push complete",
                            outcome: if failed {
                                Err("Rejected".into())
                            } else {
                                Ok(())
                            },
                            snapshot: if refresh_failed {
                                Err("Refresh failed".into())
                            } else {
                                Ok(git::Snapshot {
                                    repository: Repository {
                                        name: "test".into(),
                                        path: "/test".into(),
                                    },
                                    branch: "main".into(),
                                    changes: vec![],
                                })
                            },
                        },
                    );
                    assert_eq!(app.network_dialog.is_some(), !skip && failed);
                    assert_eq!(app.error.is_some(), !skip && (failed || refresh_failed));
                    if skip {
                        assert!(app.status.contains(if failed {
                            "Rejected"
                        } else if refresh_failed {
                            "Refresh failed"
                        } else {
                            "Push complete"
                        }));
                    }
                }
            }
        }
    }

    #[test]
    fn operation_errors_keep_draft_and_success_clears_it_even_if_refresh_fails() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.commit_message = "Keep draft".into();
        app.state_current = false;
        deliver(
            &mut app,
            WorkerResult::Operation {
                committed: false,
                success: "Index updated",
                outcome: Err("Hook rejected commit".into()),
                snapshot: Err("Refresh failed".into()),
            },
        );
        assert_eq!(app.commit_message, "Keep draft");
        assert!(app.error.as_deref().unwrap().contains("Hook"));
        assert!(!app.state_current);
        deliver(
            &mut app,
            WorkerResult::Operation {
                committed: true,
                success: "Commit created",
                outcome: Ok(()),
                snapshot: Err("Refresh failed".into()),
            },
        );
        assert!(app.commit_message.is_empty());
        assert!(app.error.as_deref().unwrap().contains("refresh"));
        assert!(!app.state_current);
        assert!(app.commit_blocker().is_some());
    }

    #[test]
    fn history_errors_retain_rows_and_late_commit_previews_are_discarded() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        app.commits = crate::mock_data::commits();
        app.state_current = true;
        let selected = app.commits[0].hash.clone();
        app.select_commit(selected.clone());
        let path = app.repositories[0].path.clone();
        let count = app.commits.len();
        deliver(
            &mut app,
            WorkerResult::History {
                path: "other-repository".into(),
                all: false,
                result: Ok(crate::history::prepare(Vec::new(), false)),
            },
        );
        assert_eq!(app.commits.len(), count);
        deliver(
            &mut app,
            WorkerResult::History {
                path,
                all: false,
                result: Err("Cannot read history".into()),
            },
        );
        assert_eq!(app.commits.len(), count);
        assert!(app.history_error.is_some());
        assert!(app.state_current);
        deliver(
            &mut app,
            WorkerResult::CommitPatch {
                hash: "old selection".into(),
                result: Err("Stale preview".into()),
            },
        );
        assert!(app.commit_preview_error.is_none());
        assert!(app.commit_preview.is_none());
        deliver(
            &mut app,
            WorkerResult::CommitPatch {
                hash: selected,
                result: Ok(crate::models::DiffPreview::new(
                    "@@ -0,0 +1 @@\n+commit preview",
                )),
            },
        );
        assert_eq!(app.commit_preview.as_ref().unwrap().added, 1);
    }

    #[test]
    fn branch_actions_require_fresh_refs_and_clean_switches() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        app.refs = crate::mock_data::refs();
        app.selected_branch = "refs/heads/develop".into();
        app.state_current = true;
        app.branch_head_exists = true;
        app.new_branch_name = "feature/new".into();
        assert!(app.switch_branch_blocker().is_some());
        app.refs_current = true;
        assert!(app.switch_branch_blocker().is_none());
        assert!(app.create_branch_blocker().is_none());
        app.changes = crate::mock_data::changes();
        assert!(app.switch_branch_blocker().is_some());
        assert!(app.create_branch_blocker().is_some());
        app.switch_branch();
        app.create_branch();
        assert!(!app.busy());
        app.switch_to_new_branch = false;
        assert!(app.create_branch_blocker().is_none());
        app.branch_head_exists = false;
        assert!(app
            .create_branch_blocker()
            .unwrap()
            .contains("first commit"));
        app.changes.clear();
        app.selected_branch = "refs/tags/v1.0.0".into();
        assert!(app.switch_branch_blocker().is_none());
    }

    #[test]
    fn remote_checkout_uses_current_tracking_branch_or_shows_choices() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        app.refs = crate::mock_data::refs();
        app.state_current = true;
        app.refs_current = true;
        let remote = app
            .refs
            .iter()
            .find(|r| r.kind == crate::models::RefKind::Remote)
            .unwrap();
        let remote_name = remote.name.clone();
        app.selected_branch = remote.full_name.clone();
        let local = app
            .refs
            .iter_mut()
            .find(|r| r.kind == crate::models::RefKind::Local)
            .unwrap();
        local.upstream = Some(remote_name);
        local.current = true;
        let local_ref = local.full_name.clone();
        app.switch_branch();
        assert_eq!(app.selected_branch, local_ref);
        assert!(app.remote_checkout.is_none() && !app.busy());
        app.selected_branch = "refs/remotes/origin/new-topic".into();
        app.refs.push(crate::models::BranchRef {
            full_name: app.selected_branch.clone(),
            name: "origin/new-topic".into(),
            hash: "a".repeat(40),
            upstream: None,
            kind: crate::models::RefKind::Remote,
            current: false,
        });
        app.switch_branch();
        let dialog = app.remote_checkout.as_ref().unwrap();
        assert_eq!(dialog.new_name, "new-topic");
        assert_eq!(dialog.mode, 1);
        assert!(!app.busy());
    }

    #[test]
    fn branch_load_errors_keep_rows_but_disable_branch_actions() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        app.refs = crate::mock_data::refs();
        app.state_current = true;
        app.refs_current = true;
        let count = app.refs.len();
        deliver(
            &mut app,
            WorkerResult::Refs {
                path: "other repo".into(),
                result: Ok(crate::models::RefSnapshot {
                    refs: Vec::new(),
                    has_head: false,
                }),
            },
        );
        assert_eq!(app.refs.len(), count);
        assert!(app.refs_current);
        let path = app.repositories[0].path.clone();
        deliver(
            &mut app,
            WorkerResult::Refs {
                path,
                result: Err("Cannot read refs".into()),
            },
        );
        assert_eq!(app.refs.len(), count);
        assert!(app.state_current);
        assert!(!app.refs_current);
        assert!(app.refs_error.is_some());
        assert!(app.switch_branch_blocker().is_some());
        assert!(app.create_branch_blocker().is_some());
    }

    fn visible_text_count(shapes: &[ClippedShape], expected: &str) -> usize {
        shapes
            .iter()
            .filter(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    text.galley.job.text == expected
                        && shape
                            .clip_rect
                            .intersect(text.galley.rect.translate(text.pos.to_vec2()))
                            .height()
                            > 8.0
                } else {
                    false
                }
            })
            .count()
    }

    fn visible_text(shapes: &[ClippedShape], expected: &str) -> bool {
        visible_text_count(shapes, expected) > 0
    }

    #[test]
    fn welcome_waits_for_restore_and_only_opens_without_an_active_repository() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.welcome_pending = true;
        let (_sender, receiver) = mpsc::channel();
        app.worker = Some(receiver);
        app.finish_startup();
        assert!(app.welcome_pending);
        assert!(!app.welcome_open);
        app.worker = None;
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        app.finish_startup();
        assert!(!app.welcome_open);
        app.welcome_pending = true;
        app.selected_repository = usize::MAX;
        app.finish_startup();
        assert!(app.welcome_open);
        app.welcome_open = false;
        app.finish_startup();
        assert!(!app.welcome_open, "Welcome must not reopen after closing");
        app.welcome_pending = true;
        app.preferences.show_welcome = false;
        app.finish_startup();
        assert!(!app.welcome_open);
    }

    #[test]
    fn double_click_staging_setting_changes_only_the_index() {
        use crate::preferences::FileDoubleClick;
        use egui::{Event, Modifiers, PointerButton};
        for mode in [FileDoubleClick::ShowDiff, FileDoubleClick::ToggleStage] {
            let repo = crate::git::tests::TempRepo::new();
            std::fs::write(repo.0.join("example.txt"), "Keep working-tree content\n").unwrap();
            let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
            app.apply_snapshot(git::load(&repo.0).unwrap());
            app.preferences.file_double_click = mode;
            let context = egui::Context::default();
            crate::theme::install_fonts(&context);
            egui_extras::install_image_loaders(&context);
            ThemeChoice::Light.apply(&context);
            let render = |app: &mut SannyGitApp, time, events| {
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(1100.0, 700.0),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ui| ui::show(ui, app),
                );
                output.textures_delta.clear();
                output
            };
            for round in 0..2 {
                let start = 1.0 + round as f64;
                let mut output = render(&mut app, start - 0.5, vec![]);
                for _ in 0..2 {
                    output = render(&mut app, start - 0.4, vec![]);
                }
                let target = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == "example.txt" => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .unwrap();
                for (step, pressed) in [true, false, true, false].into_iter().enumerate() {
                    render(
                        &mut app,
                        start + step as f64 * 0.05,
                        vec![
                            Event::PointerMoved(target),
                            Event::PointerButton {
                                pos: target,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                    );
                }
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while app.busy() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    app.poll_worker();
                }
                assert!(!app.busy());
                assert!(app.error.is_none(), "{:?}", app.error);
                let snapshot = git::load(&repo.0).unwrap();
                assert_eq!(
                    snapshot.changes[0].staged,
                    mode == FileDoubleClick::ToggleStage && round == 0
                );
                assert_eq!(
                    std::fs::read_to_string(repo.0.join("example.txt")).unwrap(),
                    "Keep working-tree content\n"
                );
            }
        }
    }

    #[test]
    fn modern_layout_fits_both_sizes_and_color_themes_without_loading_jumps() {
        for size in [Vec2::new(1100.0, 700.0), Vec2::new(1536.0, 1024.0)] {
            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                let context = egui::Context::default();
                crate::theme::install_fonts(&context);
                egui_extras::install_image_loaders(&context);
                theme.apply_style(&context, InterfaceStyle::Modern);
                let mut app = SannyGitApp::with_theme(theme);
                app.interface_style = InterfaceStyle::Modern;
                app.repositories = crate::mock_data::repositories();
                app.changes = crate::mock_data::changes();
                app.commits = crate::mock_data::commits();
                app.refs = crate::mock_data::refs();
                app.current_branch = "main".into();
                app.selected_change = 2;
                app.loaded_diff = Some(2);
                app.diff_preview = Some(crate::models::DiffPreview::new(&app.changes[2].diff));
                let render = |app: &mut SannyGitApp| {
                    let mut output = context.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ui| ui::show(ui, app),
                    );
                    output.textures_delta.clear();
                    output
                };
                for _ in 0..3 {
                    render(&mut app);
                }
                let output = render(&mut app);
                for label in [
                    "Repositories",
                    "Branches",
                    "Settings",
                    "Commit",
                    "Journal",
                    "diff_view.rs",
                    "Initial commit",
                ] {
                    assert!(
                        visible_text(&output.shapes, label),
                        "Missing {label} at {size:?} in {theme:?}"
                    );
                }
                let label_rect = |label: &str| {
                    output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.job.text == label => {
                                Some(text.galley.rect.translate(text.pos.to_vec2()))
                            }
                            _ => None,
                        })
                        .unwrap()
                };
                assert!(label_rect("diff_view.rs").left() > label_rect("File").right());
                assert!(label_rect("Journal").left() > label_rect("Commit").right());
                assert!(label_rect("Commit").top() > label_rect("File").bottom());
                let panels = || {
                    context.data(|data| {
                        let mut rects: Vec<_> = data
                            .iter_temp_by_type::<egui::PanelState>()
                            .map(|panel| panel.outer_rect)
                            .collect();
                        rects.sort_by(|a, b| {
                            a.left()
                                .total_cmp(&b.left())
                                .then(a.top().total_cmp(&b.top()))
                        });
                        rects
                    })
                };
                let before = panels();
                app.changes.clear();
                app.commits.clear();
                app.refs_loading = true;
                app.history_loading = true;
                for _ in 0..3 {
                    render(&mut app);
                }
                assert_eq!(
                    before,
                    panels(),
                    "Loading moved panels at {size:?} in {theme:?}"
                );
            }
        }
    }

    #[test]
    fn empty_state_and_open_dialog_fit_in_both_themes() {
        for size in [Vec2::new(1100.0, 700.0), Vec2::new(1536.0, 1024.0)] {
            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                let context = egui::Context::default();
                crate::theme::install_fonts(&context);
                egui_extras::install_image_loaders(&context);
                theme.apply(&context);
                let mut app = SannyGitApp::with_theme(theme);
                app.open_dialog = true;
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                let mut output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                output.textures_delta.clear();
                for _ in 0..2 {
                    output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                    output.textures_delta.clear();
                }
                for text in ["OK", "Settings", "Add existing or create new repository"] {
                    assert!(
                        visible_text(&output.shapes, text),
                        "{text} clipped at {size:?} in {theme:?}"
                    );
                }
                assert!(app.repositories.is_empty());
                assert!(app.worker.is_none());
            }
        }
    }

    #[test]
    fn all_changed_files_fit_at_reference_and_small_window_sizes() {
        for size in [
            Vec2::new(1536.0, 1024.0),
            Vec2::new(1244.0, 720.0),
            Vec2::new(1100.0, 700.0),
        ] {
            for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
                let context = egui::Context::default();
                crate::theme::install_fonts(&context);
                egui_extras::install_image_loaders(&context);
                theme.apply(&context);
                let mut app = SannyGitApp::with_theme(theme);
                app.repositories = crate::mock_data::repositories();
                app.changes = crate::mock_data::changes();
                let history = crate::history::prepare(crate::mock_data::commits(), false);
                app.commits = history.commits;
                app.history_edges = history.edges;
                app.history_max_lane = history.max_lane;
                app.refs = crate::mock_data::refs();
                app.refs_current = true;
                app.branch_head_exists = true;
                app.selected_repository = 0;
                app.state_current = true;
                app.commit_message = "Layout check".into();
                app.selected_change = 2;
                app.loaded_diff = Some(2);
                app.diff_preview = Some(crate::models::DiffPreview::new(&app.changes[2].diff));
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                let mut output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                // Headless layout checks do not upload textures to a renderer.
                output.textures_delta.clear();
                for _ in 0..2 {
                    output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                    output.textures_delta.clear();
                }
                for file in &app.changes {
                    assert!(
                        visible_text(&output.shapes, &file.path),
                        "File {} is clipped at {size:?} in {theme:?}",
                        file.path
                    );
                }
                assert!(
                    visible_text_count(&output.shapes, "Commit") >= 2,
                    "Commit control is clipped at {size:?}"
                );
                assert!(
                    visible_text(&output.shapes, "Initial commit"),
                    "History is clipped at {size:?}"
                );
                app.select_commit(app.commits.last().unwrap().hash.clone());
                app.commit_preview = Some(crate::models::DiffPreview::new(
                    "@@ -0,0 +1 @@\n+Read-only commit preview",
                ));
                for _ in 0..3 {
                    output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                    output.textures_delta.clear();
                }
                for text in [
                    "Commit details",
                    "Copy hash",
                    "Commit changes",
                    "Read-only commit preview",
                ] {
                    assert!(
                        visible_text(&output.shapes, text),
                        "Commit details {text} clipped at {size:?} in {theme:?}"
                    );
                }
                assert!(
                    app.worker.is_none(),
                    "Rendering history must not run Git commands"
                );
                app.commit_details_open = false;
                app.current_branch = "main".into();
                app.branch_dialog_open = true;
                app.new_branch_name = "feature/new-ui".into();
                for _ in 0..3 {
                    output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                    output.textures_delta.clear();
                }
                for text in ["Create branch", "Switch to the new branch", "Create"] {
                    assert!(
                        visible_text(&output.shapes, text),
                        "Branch control {text} clipped at {size:?} in {theme:?}"
                    );
                }
                assert!(
                    app.worker.is_none(),
                    "Rendering branch controls must not run Git commands"
                );
                app.branch_dialog_open = false;
                app.remote_checkout = Some(RemoteCheckoutDialog {
                    remote: "refs/remotes/origin/topic".into(),
                    mode: 1,
                    local: "refs/heads/main".into(),
                    new_name: "topic".into(),
                });
                for _ in 0..3 {
                    output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                    output.textures_delta.clear();
                }
                for label in [
                    "Check out commit",
                    "Checkout local branch:",
                    "Create local branch:",
                    "Checkout",
                    "Cancel",
                ] {
                    assert!(
                        visible_text(&output.shapes, label),
                        "Remote checkout {label} clipped at {size:?} in {theme:?}"
                    );
                }
                app.remote_checkout = None;
                for operation in [
                    crate::remotes::Operation::Fetch,
                    crate::remotes::Operation::Pull,
                    crate::remotes::Operation::Push,
                ] {
                    app.network_dialog = Some(remotes::RemoteDialog {
                        operation,
                        state: Some(crate::remotes::State {
                            remotes: vec!["origin".into()],
                            branch: Some("main".into()),
                            ..Default::default()
                        }),
                        remote: "origin".into(),
                        branch: "main".into(),
                        set_upstream: false,
                        dont_show_again: false,
                    });
                    for _ in 0..3 {
                        output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                        output.textures_delta.clear();
                    }
                    let destination_label = if operation == crate::remotes::Operation::Push {
                        "Options"
                    } else {
                        "Remote:"
                    };
                    for label in [destination_label, "Cancel"] {
                        assert!(
                            visible_text(&output.shapes, label),
                            "Remote operation {label} clipped at {size:?} in {theme:?}"
                        );
                    }
                    if operation == crate::remotes::Operation::Pull {
                        assert!(visible_text(&output.shapes, "Remote branch:"));
                    }
                    assert!(app.worker.is_none());
                }
                app.network_dialog = None;
                app.manager.settings = Some(crate::repository::Settings::default());
                app.manager.prune_preview = Some("Preview of obsolete metadata".into());
                app.manager.found = vec![("Found repo".into(), "/tmp/found-repo".into(), true)];
                app.manager.worktrees = vec![crate::repository::Worktree {
                    path: "/tmp/linked-tree".into(),
                    branch: Some("feature".into()),
                    main: false,
                    locked: false,
                    prunable: false,
                    bare: false,
                }];
                app.manager.selected_worktree = Some("/tmp/linked-tree".into());
                app.manager.confirm_remove = true;
                for (dialog, button) in [
                    (RepositoryDialog::Group, "Add Group"),
                    (RepositoryDialog::Clone, "Continue"),
                    (RepositoryDialog::Search, "Add Selected"),
                    (RepositoryDialog::Worktrees, "Remove Worktree"),
                    (RepositoryDialog::Prune, "Prune Obsolete Metadata"),
                    (RepositoryDialog::Settings, "OK"),
                    (RepositoryDialog::CloseRepository, "Close Repository"),
                ] {
                    app.manager.dialog = Some(dialog);
                    for _ in 0..3 {
                        output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                        output.textures_delta.clear();
                    }
                    assert!(
                        visible_text(&output.shapes, button),
                        "Repository control {button} clipped at {size:?} in {theme:?}"
                    );
                    assert!(
                        app.worker.is_none(),
                        "Rendering repository dialogs must not execute Git operations"
                    );
                }
                app.manager.dialog = Some(RepositoryDialog::Clone);
                app.manager.clone_branches = vec!["main".into(), "feature".into()];
                app.manager.clone_options.branch = "main".into();
                for (step, label) in [(1, "Check Out Branch:"), (2, "Finish")] {
                    app.manager.clone_step = step;
                    for _ in 0..3 {
                        output = context.run_ui(input.clone(), |ui| ui::show(ui, &mut app));
                        output.textures_delta.clear();
                    }
                    assert!(
                        visible_text(&output.shapes, label),
                        "Clone step {step} clipped at {size:?} in {theme:?}"
                    );
                    assert!(app.worker.is_none());
                }
            }
        }
    }
}
