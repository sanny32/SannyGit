pub mod discard;
pub mod ref_management;
mod remotes;
pub mod stash;
pub use remotes::RemoteDialog;
mod history;
mod llm_models;
mod repositories;
mod tracking;
mod working_tree;
pub use llm_models::ModelCatalog;
pub use repositories::{Dialog as RepositoryDialog, Job as RepositoryJob, RepositoryManager};

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
};

enum WorkerResult {
    RefDetails {
        root: String,
        result: Box<ref_management::ResultMessage>,
    },
    DiscardPrepared {
        root: String,
        path: String,
        result: Result<crate::discard::Prepared, String>,
    },
    Stash {
        path: String,
        result: Box<stash::ResultMessage>,
    },
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
        generation: u64,
        append: bool,
        result: Result<crate::history::Page, String>,
    },
    CommitPatch {
        hash: String,
        result: Result<crate::models::DiffPreview, String>,
    },
    ContentsPreview(Result<crate::models::DiffPreview, String>),
    CommitMessage(Result<String, String>),
    AmendMessage(Result<(String, String), String>),
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
    Git,
    Remote,
    Llm,
}

#[derive(Clone, Default)]
pub struct LlmSettings {
    pub config: crate::llm::Config,
    pub api_key_dirty: bool,
    pub api_key_loaded: bool,
    pub secret_error: Option<String>,
}

impl LlmSettings {
    pub fn save_secret(
        &self,
        previous: &crate::llm::Config,
        repository: Option<&str>,
    ) -> Result<(), String> {
        let config = &self.config;
        let changed = self.api_key_dirty
            || config.store_api_key != previous.store_api_key
            || (!config.api_key.is_empty()
                && (config.endpoint != previous.endpoint || config.model != previous.model));
        if changed {
            crate::llm::save_api_key(config, repository)?;
        }
        Ok(())
    }
}

pub struct SettingsWindow {
    pub open: bool,
    pub page: SettingsPage,
    pub theme: ThemeChoice,
    pub preferences: crate::preferences::Preferences,
    pub interface_style: InterfaceStyle,
    pub auto_fetch: bool,
    pub skip_push_confirmation: bool,
    pub llm: LlmSettings,
    pub llm_repositories: BTreeMap<String, LlmSettings>,
    pub llm_repository: Option<String>,
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
            llm: LlmSettings::default(),
            llm_repositories: BTreeMap::new(),
            llm_repository: None,
        }
    }
}

pub struct RemoteCheckoutDialog {
    pub remote: String,
    pub mode: u8,
    pub local: String,
    pub new_name: String,
}

pub struct RenameBranchDialog {
    pub source: String,
    pub name: String,
}

/// Only user-authored text and the expected amend target belong in draft storage.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct CommitDraft {
    message: String,
    amend_head: Option<String>,
    normal_message: Option<String>,
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
    pub repository_llm_configs: BTreeMap<String, crate::llm::Config>,
    pub llm_models: ModelCatalog,
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
    pub rename_branch_dialog: Option<RenameBranchDialog>,
    pub tag_dialog_open: bool,
    pub remote_checkout: Option<RemoteCheckoutDialog>,
    pub network_dialog: Option<remotes::RemoteDialog>,
    pub stash_dialog: Option<stash::Dialog>,
    pub discard_dialog: Option<discard::Dialog>,
    pub ref_dialog: Option<ref_management::Dialog>,
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
    pub selected_ref_group: Option<crate::models::RefKind>,
    pub change_filter: ChangeFilter,
    pub journal_filter: String,
    pub current_branch: String,
    pub commit_message: String,
    pub commit_contents: git::CommitContents,
    pub contents_preview_open: bool,
    pub contents_preview: Option<Result<crate::models::DiffPreview, String>>,
    pub generating_commit_message: bool,
    generation_cancel: Option<Arc<AtomicBool>>,
    pub amend: bool,
    amend_head: Option<String>,
    amend_draft: Option<String>,
    commit_drafts: BTreeMap<String, CommitDraft>,
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
    history_session: history::Session,
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
        app.repository_llm_configs = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, "sannygit.repository_llm"))
            .unwrap_or_default();
        if let Some(api_key) = std::env::var_os("SANNYGIT_LLM_API_KEY") {
            app.llm_config.api_key = api_key.to_string_lossy().into_owned();
        }
        app.llm_models = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, llm_models::STORAGE_KEY))
            .unwrap_or_default();
        if let Some(storage) = creation_context.storage {
            app.restore_commit_drafts(storage);
        }
        app.refresh_llm_models(&creation_context.egui_ctx);
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

    fn restore_commit_drafts(&mut self, storage: &dyn eframe::Storage) {
        self.commit_drafts =
            eframe::get_value(storage, "sannygit.commit_drafts").unwrap_or_default();
    }

    fn remember_commit_draft(&mut self) {
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let draft = CommitDraft {
            message: self.commit_message.clone(),
            amend_head: self.amend_head.clone().filter(|_| self.amend),
            normal_message: self.amend_draft.clone().filter(|_| self.amend),
        };
        if draft.message.is_empty()
            && draft
                .normal_message
                .as_deref()
                .unwrap_or_default()
                .is_empty()
        {
            self.commit_drafts.remove(&repository.path);
        } else {
            self.commit_drafts.insert(repository.path.clone(), draft);
        }
    }

    fn load_commit_draft(&mut self) {
        let draft = self
            .repositories
            .get(self.selected_repository)
            .and_then(|repository| self.commit_drafts.get(&repository.path))
            .cloned()
            .unwrap_or_default();
        self.commit_message = draft.message;
        self.amend_head = draft.amend_head;
        self.amend = self.amend_head.is_some();
        self.amend_draft = draft.normal_message;
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
            repository_llm_configs: BTreeMap::new(),
            llm_models: ModelCatalog::default(),
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
            rename_branch_dialog: None,
            tag_dialog_open: false,
            remote_checkout: None,
            network_dialog: None,
            stash_dialog: None,
            discard_dialog: None,
            ref_dialog: None,
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
            selected_ref_group: None,
            change_filter: ChangeFilter::All,
            journal_filter: String::new(),
            current_branch: String::new(),
            commit_message: String::new(),
            commit_contents: git::CommitContents::default(),
            contents_preview_open: false,
            contents_preview: None,
            generating_commit_message: false,
            generation_cancel: None,
            amend: false,
            amend_head: None,
            amend_draft: None,
            commit_drafts: BTreeMap::new(),
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
            history_session: history::Session::default(),
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

    fn refresh_llm_models(&mut self, context: &eframe::egui::Context) {
        // Cloud endpoints usually require authentication. Keep their cached
        // models until a key is available, without reading the OS key store.
        if self.llm_config.is_local() || !self.llm_config.api_key.trim().is_empty() {
            self.llm_models.load(&self.llm_config, None, context);
        }
        for (path, config) in &self.repository_llm_configs {
            if config.is_local() || !config.api_key.trim().is_empty() {
                self.llm_models.load(config, Some(path), context);
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }

    fn start_worker(&mut self, job: impl FnOnce() -> WorkerResult + Send + 'static) -> bool {
        // A user operation supersedes any in-flight automatic read.
        self.working_tree.invalidate();
        self.tracking.cancel();
        let (sender, receiver) = mpsc::channel();
        let git_executable = self.preferences.git_executable.clone();
        match std::thread::Builder::new()
            .name("sannygit-reader".into())
            .spawn(move || {
                git::set_executable(git_executable);
                let _ = sender.send(job());
            }) {
            Ok(_) => {
                self.worker = Some(receiver);
                true
            }
            Err(e) => {
                self.error = Some(format!("Cannot start background worker: {e}"));
                if self.contents_preview_open && self.contents_preview.is_none() {
                    self.contents_preview =
                        Some(Err("Background worker stopped unexpectedly".into()));
                }
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
            MenuAction::RenameBranch => self
                .rename_branch_target_blocker(&self.selected_branch)
                .is_none(),
            MenuAction::DeleteBranch => self.delete_branch_blocker().is_none(),
            MenuAction::PushTag => self.push_tag_blocker().is_none(),
            MenuAction::ManageRemotes => self.remote_allowed(),
            MenuAction::Discard => self.discard_blocker().is_none(),
            MenuAction::Stashes => self.stash_allowed(),
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
            MenuAction::DeleteBranch => self.open_delete_branch(),
            MenuAction::PushTag => self.open_push_tag(),
            MenuAction::ManageRemotes => self.open_remote_manager(),
            MenuAction::Discard => self.open_discard(),
            MenuAction::Stashes => self.open_stashes(),
            MenuAction::CreateBranch => self.branch_dialog_open = true,
            MenuAction::SwitchBranch => self.switch_branch(),
            MenuAction::RenameBranch => self.open_rename_branch(),
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
        if self.settings_window.preferences.git_executable.is_empty() {
            if let Some(path) = git::find_executable() {
                self.settings_window.preferences.git_executable =
                    path.to_string_lossy().into_owned();
            }
        }
        self.settings_window.interface_style = self.interface_style;
        self.settings_window.theme = self.theme;
        self.settings_window.auto_fetch = self.tracking.auto_fetch;
        self.settings_window.skip_push_confirmation = self.skip_push_confirmation;
        self.settings_window.llm = LlmSettings {
            config: self.llm_config.clone(),
            ..Default::default()
        };
        self.settings_window.llm_repositories = self
            .repository_llm_configs
            .iter()
            .map(|(path, config)| {
                (
                    path.clone(),
                    LlmSettings {
                        config: config.clone(),
                        ..Default::default()
                    },
                )
            })
            .collect();
        self.settings_window.llm_repository = None;
        self.settings_window.open = true;
    }

    pub fn open_repository_llm_settings(&mut self) {
        let Some(path) = self
            .repositories
            .get(self.selected_repository)
            .map(|repository| repository.path.clone())
        else {
            return;
        };
        self.open_settings();
        self.settings_window.page = SettingsPage::Llm;
        self.settings_window.llm_repository = Some(path);
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
        let existing_repository = self
            .repositories
            .iter()
            .position(|r| crate::repository::same_path(&r.path, &snapshot.repository.path));
        let new_repository = existing_repository.is_none();
        let index = existing_repository.unwrap_or_else(|| {
            self.repositories.push(snapshot.repository);
            self.repositories.len() - 1
        });
        let same_repository = self.selected_repository == index;
        if !same_repository {
            self.remember_commit_draft();
        }
        self.selected_repository = index;
        if !same_repository {
            self.manager.highlighted_repository = Some(self.repositories[index].path.clone());
        }
        if new_repository {
            let path = self.repositories[index].path.clone();
            self.assign_group(&path, self.manager.preferences.selected_group.clone());
        }
        self.contents_preview_open = false;
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
        self.queue_history_reload();
        self.commit_preview = None;
        self.commit_preview_error = None;
        self.loaded_commit = None;
        if !same_repository {
            self.manager.dialog = None;
            self.manager.worktrees.clear();
            self.manager.settings = None;
            self.selected_branch.clear();
            self.selected_ref_group = None;
            self.refs.clear();
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
            self.new_branch_name.clear();
            self.commits.clear();
            self.history_session.clear_view();
            self.history_edges.clear();
            self.history_max_lane = 0;
            self.history_truncated = false;
            self.selected_commit = None;
            self.commit_details_open = false;
            self.commit_preview_error = None;
            self.load_commit_draft();
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

    fn rename_branch_target_blocker(&self, source: &str) -> Option<&'static str> {
        self.branch_base_blocker().or_else(|| {
            (!self
                .refs
                .iter()
                .any(|r| r.full_name == source && r.kind == crate::models::RefKind::Local))
            .then_some("Select an existing local branch")
        })
    }

    pub fn open_rename_branch(&mut self) {
        if self
            .rename_branch_target_blocker(&self.selected_branch)
            .is_some()
        {
            return;
        }
        self.rename_branch_dialog = Some(RenameBranchDialog {
            source: self.selected_branch.clone(),
            name: self
                .selected_branch
                .strip_prefix("refs/heads/")
                .unwrap_or_default()
                .into(),
        });
        self.error = None;
    }

    pub fn rename_branch_blocker(&self) -> Option<&'static str> {
        let dialog = self.rename_branch_dialog.as_ref()?;
        self.rename_branch_target_blocker(&dialog.source)
            .or_else(|| crate::branches::name_hint(&dialog.name))
            .or_else(|| {
                self.refs
                    .iter()
                    .any(|r| r.full_name == format!("refs/heads/{}", dialog.name))
                    .then_some("A local branch with this name already exists")
            })
    }

    pub fn rename_branch(&mut self) {
        if self.rename_branch_blocker().is_some() {
            return;
        }
        let Some(dialog) = &self.rename_branch_dialog else {
            return;
        };
        let source = dialog.source.clone();
        let name = dialog.name.clone();
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        self.state_current = false;
        self.refs_current = false;
        self.error = None;
        self.status = "Renaming branch…".into();
        self.start_worker(move || {
            let outcome = crate::branches::rename(&root, &source, &name);
            WorkerResult::Operation {
                committed: false,
                success: "Branch renamed",
                outcome,
                snapshot: git::load(&root),
            }
        });
    }

    pub fn create_tag_blocker(&self) -> Option<&'static str> {
        self.branch_base_blocker()
            .or_else(|| {
                (!self.branch_head_exists)
                    .then_some("Create the first commit before creating a tag")
            })
            .or_else(|| crate::branches::tag_hint(&self.new_tag_name, &self.new_tag_message))
            .or_else(|| {
                self.refs
                    .iter()
                    .any(|r| r.full_name == format!("refs/tags/{}", self.new_tag_name))
                    .then_some("A tag with this name already exists")
            })
    }

    pub fn create_tag(&mut self) {
        if self.create_tag_blocker().is_some() {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let name = self.new_tag_name.clone();
        let message = self.new_tag_message.clone();
        self.state_current = false;
        self.refs_current = false;
        self.error = None;
        self.status = "Creating tag…".into();
        self.start_worker(move || WorkerResult::Operation {
            committed: false,
            success: "Tag created",
            outcome: crate::branches::create_tag(&root, &name, &message),
            snapshot: git::load(&root),
        });
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

    pub fn amend_blocker(&self) -> Option<&'static str> {
        if self.busy() {
            Some("Wait for the current operation to finish")
        } else if self.repositories.get(self.selected_repository).is_none() {
            Some("Open a repository first")
        } else if self.amend {
            None
        } else if !self.state_current || !self.refs_current {
            Some("Refresh the repository before amending")
        } else if !self.branch_head_exists {
            Some("There is no commit to amend")
        } else {
            None
        }
    }

    pub fn set_amend(&mut self, enabled: bool) {
        if self.amend_blocker().is_some() || enabled == self.amend {
            return;
        }
        self.contents_preview_open = false;
        if !enabled {
            self.amend = false;
            self.amend_head = None;
            if let Some(draft) = self.amend_draft.take() {
                self.commit_message = draft;
            }
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        self.error = None;
        self.status = "Loading last commit message…".into();
        self.start_worker(move || WorkerResult::AmendMessage(git::amend_message(&root)));
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
        } else if self.amend && self.amend_head.is_none() {
            Some("Load the last commit before amending")
        } else if self.changes.is_empty() && !self.amend {
            Some("There are no changes to commit")
        } else if self.commit_contents == git::CommitContents::Staged
            && !self.amend
            && !self.changes.iter().any(|c| c.staged)
        {
            Some("There are no staged changes to commit")
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
        let contents = self.commit_contents;
        self.contents_preview_open = false;
        self.state_current = false;
        self.error = None;
        let amend_head = self.amend_head.clone().filter(|_| self.amend);
        self.status = if self.amend {
            "Amending last commit…"
        } else {
            "Creating commit…"
        }
        .into();
        self.start_worker(move || {
            let outcome = if let Some(head) = &amend_head {
                git::amend_commit(&root, &message, head, contents)
            } else {
                git::commit(&root, &message, contents)
            };
            let committed = outcome.is_ok();
            let snapshot = git::load(&root);
            WorkerResult::Operation {
                committed,
                success: if amend_head.is_some() {
                    "Commit amended"
                } else {
                    "Commit created"
                },
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
        } else if self.amend && self.amend_head.is_none() {
            Some("Load the last commit before previewing its contents")
        } else if !self.amend
            && self.commit_contents == git::CommitContents::Staged
            && !self.changes.iter().any(|c| c.staged)
        {
            Some("There are no staged changes to describe")
        } else if self.changes.is_empty() && !self.amend {
            Some("There are no changes to describe")
        } else {
            None
        }
    }

    pub fn preview_commit_contents(&mut self) {
        if self.generate_commit_message_blocker().is_some() {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let contents = self.commit_contents;
        let head = self.amend_head.clone().filter(|_| self.amend);
        self.contents_preview_open = true;
        self.contents_preview = None;
        self.start_worker(move || {
            WorkerResult::ContentsPreview(
                git::commit_patch(&root, contents, head.as_deref())
                    .and_then(git::limit_diff)
                    .map(|patch| crate::models::DiffPreview::new(&patch)),
            )
        });
    }

    pub fn effective_llm_config(&self, path: &str) -> &crate::llm::Config {
        self.repository_llm_configs
            .get(path)
            .unwrap_or(&self.llm_config)
    }

    pub fn generate_commit_message(&mut self) {
        if self.generate_commit_message_blocker().is_some() {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let path = self.repositories[self.selected_repository].path.clone();
        let repository_scope = self
            .repository_llm_configs
            .contains_key(&path)
            .then_some(path.clone());
        let mut config = self.effective_llm_config(&path).clone();
        self.error = None;
        self.status = "Generating commit message…".into();
        let cancel = Arc::new(AtomicBool::new(false));
        self.generation_cancel = Some(cancel.clone());
        let contents = self.commit_contents;
        let head = self.amend_head.clone().filter(|_| self.amend);
        self.generating_commit_message = self.start_worker(move || {
            WorkerResult::CommitMessage(
                git::commit_patch(&root, contents, head.as_deref()).and_then(|patch| {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Commit message generation cancelled".into());
                    }
                    crate::llm::load_api_key_if_needed(&mut config, repository_scope.as_deref())?;
                    crate::llm::generate(&config, &patch, &cancel)
                }),
            )
        });
        if !self.generating_commit_message {
            self.generation_cancel = None;
        }
    }

    pub fn cancel_commit_message_generation(&mut self) {
        if !self.generating_commit_message {
            return;
        }
        if let Some(cancel) = self.generation_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        // Discard the receiver as well: a completed or late response from this
        // generation must never overwrite the draft or a subsequent operation.
        self.worker = None;
        self.generating_commit_message = false;
        self.status = "Commit message generation cancelled".into();
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
                self.generation_cancel = None;
                self.manager.cancel = None;
                self.error = Some("Background worker stopped unexpectedly".into());
                if self.refs_loading && !self.refs_pending {
                    self.refs_loading = false;
                    self.refs_current = false;
                    self.refs_error = self.error.clone();
                }
                if self.history_loading && !self.history_pending {
                    self.history_loading = false;
                    self.history_session.cancel = None;
                    self.history_error = self.error.clone();
                }
                if self.commit_details_open
                    && self.loaded_commit.is_some()
                    && self.commit_preview.is_none()
                {
                    self.commit_preview_error = self.error.clone();
                }
                if self.contents_preview_open && self.contents_preview.is_none() {
                    self.contents_preview =
                        Some(Err("Background worker stopped unexpectedly".into()));
                }
                self.status = "Operation failed".into();
                self.finish_discard(false);
                self.ref_worker_failed();
                if self.remote_confirmation_hidden() {
                    self.finish_hidden_remote();
                }
                return;
            }
        };
        self.worker = None;
        match result {
            WorkerResult::RefDetails { root, result } => self.apply_ref_details(root, *result),
            WorkerResult::DiscardPrepared { root, path, result } => {
                self.apply_discard_prepared(root, path, result)
            }
            WorkerResult::Stash { path, result } => self.apply_stash_result(path, *result),
            WorkerResult::ContentsPreview(result) => self.contents_preview = Some(result),
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
                            if self.selected_ref_group.is_some_and(|kind| {
                                kind != crate::models::RefKind::Tag
                                    && !self.refs.iter().any(|r| r.kind == kind)
                            }) {
                                self.selected_ref_group = None;
                            }
                            if self.selected_ref_group.is_none()
                                && !self
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
            WorkerResult::History {
                path,
                all,
                generation,
                append,
                result,
            } => self.apply_history(path, all, generation, append, result),
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
            WorkerResult::AmendMessage(result) => match result {
                Ok((head, message)) => {
                    self.amend_draft = Some(std::mem::replace(&mut self.commit_message, message));
                    self.amend_head = Some(head);
                    self.amend = true;
                    self.status = "Ready to amend last commit".into();
                }
                Err(error) => {
                    self.error = Some(error);
                    self.status = "Could not load last commit".into();
                }
            },
            WorkerResult::CommitMessage(result) => {
                self.generating_commit_message = false;
                self.generation_cancel = None;
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
                let managing_refs = matches!(
                    success,
                    "Branch deleted" | "Tag pushed" | "Remote configuration updated"
                );
                let discarding = matches!(
                    success,
                    "File restored from index" | "Untracked file deleted"
                );
                let operation_succeeded = outcome.is_ok();
                if success.starts_with("Stash ") {
                    if let Some(dialog) = &mut self.stash_dialog {
                        dialog.invalidate_preview();
                        dialog.entries_current = false;
                        dialog.reload_pending = true;
                    }
                }
                let hidden_remote = matches!(success, "Push complete" | "Sync complete")
                    && self.remote_confirmation_hidden();
                let cancellation_requested = self
                    .manager
                    .cancel
                    .as_ref()
                    .is_some_and(|cancel| cancel.load(Ordering::Relaxed));
                if matches!(
                    success,
                    "Fetch complete" | "Pull complete" | "Push complete" | "Sync complete"
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
                if outcome.is_ok() && success == "Branch renamed" {
                    if let Some(dialog) = self.rename_branch_dialog.take() {
                        self.selected_branch = format!("refs/heads/{}", dialog.name);
                        self.selected_ref_group = None;
                    }
                }
                if outcome.is_ok() && success == "Tag created" {
                    self.selected_branch = format!("refs/tags/{}", self.new_tag_name);
                    self.selected_ref_group = None;
                    self.tag_dialog_open = false;
                    self.new_tag_name.clear();
                    self.new_tag_message.clear();
                }
                if committed {
                    self.commit_message = self.amend_draft.take().unwrap_or_default();
                    self.amend = false;
                    self.amend_head = None;
                    self.remember_commit_draft();
                }
                if outcome.is_ok() && success.starts_with("Branch created") {
                    self.branch_dialog_open = false;
                    self.new_branch_name.clear();
                }
                if outcome.is_ok() && success == "Checkout complete" {
                    self.remote_checkout = None;
                }
                let refresh_error = match snapshot {
                    Ok(snapshot) => {
                        self.apply_snapshot(snapshot);
                        None
                    }
                    Err(error) => {
                        self.state_current = false;
                        Some(format!("Could not refresh repository: {error}. Refresh before further operations."))
                    }
                };
                match outcome {
                    Ok(()) => {
                        self.status = success.into();
                        self.error = refresh_error.map(|error| format!("{success}. {error}"));
                    }
                    Err(error) => {
                        self.status = if cancellation_requested {
                            "Cancellation requested; verify local and remote state before retrying"
                        } else {
                            "Operation failed"
                        }
                        .into();
                        let error = if cancellation_requested {
                            format!(
                                "{error}\nCancellation does not roll back local or remote changes."
                            )
                        } else {
                            error
                        };
                        self.error = Some(match refresh_error {
                            Some(refresh) => format!("{error}\n{refresh}"),
                            None => error,
                        });
                    }
                }
                if !self.state_current {
                    self.status
                        .push_str("; refresh required before further operations");
                }
                if discarding {
                    self.finish_discard(operation_succeeded);
                }
                if managing_refs {
                    self.finish_ref_operation(success, operation_succeeded);
                }
                if hidden_remote {
                    self.network_dialog = None;
                    if let Some(error) = self.error.take() {
                        self.status = format!("{}: {error}", self.status);
                    }
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
            && !self.status.starts_with("Stash ")
            && !matches!(
                self.status.as_str(),
                "Commit created"
                    | "Index updated"
                    | "File restored from index"
                    | "Untracked file deleted"
                    | "Branch deleted"
                    | "Tag pushed"
                    | "Remote configuration updated"
                    | "Branch switched"
                    | "Branch renamed"
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
    fn clear_color(&self, _visuals: &eframe::egui::Visuals) -> [f32; 4] {
        self.palette().background.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        self.llm_models.update();
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
        self.update_repository_hosting(ui.ctx());
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
            && self.stash_dialog.is_none()
            && self.discard_dialog.is_none()
            && self.ref_dialog.is_none()
            && self.remote_checkout.is_none();
        self.tracking.update(
            root,
            allow_tracking,
            &self.preferences.git_executable,
            ui.ctx(),
        );
        if self.tracking.refs_changed {
            self.tracking.refs_changed = false;
            self.refs_pending = true;
            self.refs_loading = true;
            self.refs_current = false;
            if self.history_all_branches {
                self.queue_history_reload();
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
        self.load_ref_pending();
        self.load_pending_stashes();
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
        if let Some(cancel) = &self.generation_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(cancel) = &self.history_session.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.tracking.cancel();
        if let Some(cancel) = &self.manager.cancel {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(receiver) = &self.worker {
                let _ = receiver.recv_timeout(std::time::Duration::from_secs(3));
            }
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.remember_commit_draft();
        eframe::set_value(storage, "sannygit.commit_drafts", &self.commit_drafts);
        self.llm_models.update();
        eframe::set_value(storage, llm_models::STORAGE_KEY, &self.llm_models);
        eframe::set_value(
            storage,
            "sannygit.skip_push_confirmation",
            &self.skip_push_confirmation,
        );
        eframe::set_value(storage, "sannygit.auto_fetch", &self.tracking.auto_fetch);
        eframe::set_value(storage, "sannygit.llm", &self.llm_config);
        eframe::set_value(
            storage,
            "sannygit.repository_llm",
            &self.repository_llm_configs,
        );
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

    #[test]
    fn startup_keeps_remote_model_cache_without_requesting_models_when_keys_are_missing() {
        let context = egui::Context::default();
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.llm_config.api_key = "  ".into();
        app.repository_llm_configs
            .insert("/repo".into(), app.llm_config.clone());
        for scope in [None, Some("/repo")] {
            app.llm_models.entry(&app.llm_config, scope).options = vec!["cached-model".into()];
        }
        app.refresh_llm_models(&context);
        for scope in [None, Some("/repo")] {
            let models = app.llm_models.entry(&app.llm_config, scope);
            assert_eq!(models.options, ["cached-model"]);
            assert!(!models.attempted);
            assert!(!models.loading());
            assert!(models.error.is_none());
        }
    }

    #[test]
    fn startup_refreshes_cached_models_without_loading_credentials_or_changing_selection() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
            time::{Duration, Instant},
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            for request in 0..4 {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "Startup did not request models");
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("Cannot accept test request: {error}"),
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&mut socket);
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    assert!(!line.to_ascii_lowercase().starts_with("authorization:"));
                }
                let body = format!(r#"{{"data":[{{"id":"model-{}"}}]}}"#, request / 2);
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        let context = egui::Context::default();
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.llm_config = crate::llm::Config {
            endpoint: format!("http://{address}/v1"),
            model: "my-custom-model".into(),
            store_api_key: true,
            ..Default::default()
        };
        app.repository_llm_configs
            .insert("/repo".into(), app.llm_config.clone());
        for launch in 0..2 {
            if launch == 1 {
                let saved = serde_json::to_string(&app.llm_models).unwrap();
                app.llm_models = serde_json::from_str(&saved).unwrap();
                assert_eq!(
                    app.llm_models.entry(&app.llm_config, None).options,
                    ["model-0"]
                );
            }
            app.refresh_llm_models(&context);
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                app.llm_models.update();
                let loading = [None, Some("/repo")]
                    .into_iter()
                    .any(|scope| app.llm_models.entry(&app.llm_config, scope).loading());
                if !loading {
                    break;
                }
                assert!(Instant::now() < deadline, "Model refresh did not finish");
                std::thread::sleep(Duration::from_millis(5));
            }
            for scope in [None, Some("/repo")] {
                let models = app.llm_models.entry(&app.llm_config, scope);
                assert!(models.error.is_none(), "{:?}", models.error);
                assert_eq!(models.options, [format!("model-{launch}")]);
            }
            assert_eq!(app.llm_config.model, "my-custom-model");
            assert_eq!(app.repository_llm_configs["/repo"].model, "my-custom-model");
        }
        server.join().unwrap();
    }

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
    fn cancelling_generation_preserves_draft_and_discards_late_results() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.commit_message = "Existing draft".into();
        let cancel = Arc::new(AtomicBool::new(false));
        app.generation_cancel = Some(cancel.clone());
        app.generating_commit_message = true;
        let (sender, receiver) = mpsc::channel();
        app.worker = Some(receiver);
        sender
            .send(WorkerResult::CommitMessage(Ok(
                "Already queued response".into()
            )))
            .unwrap();
        app.cancel_commit_message_generation();
        assert!(cancel.load(Ordering::Relaxed));
        assert!(!app.busy());
        assert!(!app.generating_commit_message);
        assert!(app.generation_cancel.is_none());
        assert_eq!(app.commit_message, "Existing draft");
        assert!(app.error.is_none());
        assert_eq!(app.status, "Commit message generation cancelled");
        assert!(sender
            .send(WorkerResult::CommitMessage(Ok("Late response".into())))
            .is_err());

        // A new generation can finish normally; the old sender is isolated.
        app.generating_commit_message = true;
        deliver(
            &mut app,
            WorkerResult::CommitMessage(Ok("New response".into())),
        );
        assert_eq!(app.commit_message, "New response");
    }

    #[test]
    fn cancelling_generation_does_not_cancel_other_workers() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        let (_sender, receiver) = mpsc::channel();
        app.worker = Some(receiver);
        app.status = "Loading repository".into();
        app.cancel_commit_message_generation();
        assert!(app.busy());
        assert_eq!(app.status, "Loading repository");
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
    fn staged_mode_gates_commit_preview_and_generation_but_allows_message_only_amend() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        assert_eq!(app.commit_contents, git::CommitContents::All);
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        app.state_current = true;
        app.commit_message = "Draft".into();
        app.changes = crate::mock_data::changes();
        for change in &mut app.changes {
            change.staged = false;
            change.kind = crate::models::ChangeKind::Modified;
        }
        app.commit_contents = git::CommitContents::Staged;
        assert!(app.commit_blocker().unwrap().contains("no staged"));
        assert!(app
            .generate_commit_message_blocker()
            .unwrap()
            .contains("no staged"));
        app.changes[0].staged = true;
        assert!(app.commit_blocker().is_none());
        assert!(app.generate_commit_message_blocker().is_none());
        app.changes.clear();
        app.amend = true;
        app.amend_head = Some("a".repeat(40));
        assert!(app.commit_blocker().is_none());
        assert!(app.generate_commit_message_blocker().is_none());
        assert_eq!(app.commit_message, "Draft");
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
    fn amend_loads_head_message_restores_draft_and_allows_rewording() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        assert!(app.amend_blocker().is_some());
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        app.state_current = true;
        app.refs_current = true;
        assert!(app.amend_blocker().unwrap().contains("no commit"));
        app.branch_head_exists = true;
        assert!(app.amend_blocker().is_none());
        app.commit_message = "Draft".into();
        deliver(
            &mut app,
            WorkerResult::AmendMessage(Err("Cannot read HEAD".into())),
        );
        assert_eq!(app.commit_message, "Draft");
        assert!(!app.amend);
        deliver(
            &mut app,
            WorkerResult::AmendMessage(Ok(("a".repeat(40), "Last message\n\nBody".into()))),
        );
        assert!(app.amend);
        assert_eq!(app.commit_message, "Last message\n\nBody");
        assert!(app.changes.is_empty());
        assert!(app.commit_blocker().is_none());
        app.commit_message = "Edited message".into();
        app.set_amend(false);
        assert_eq!(app.commit_message, "Draft");
        assert!(!app.amend);
        assert!(app.amend_head.is_none());
        assert!(app.commit_blocker().unwrap().contains("no changes"));
    }

    #[test]
    fn skipped_push_and_sync_report_results_without_dialogs() {
        for operation in [
            crate::remotes::Operation::Push,
            crate::remotes::Operation::Sync,
        ] {
            for skip in [false, true] {
                for failed in [false, true] {
                    for refresh_failed in [false, true] {
                        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
                        app.repositories = vec![Repository {
                            name: "test".into(),
                            path: "/test".into(),
                        }];
                        app.selected_repository = 0;
                        app.skip_push_confirmation =
                            skip && operation == crate::remotes::Operation::Push;
                        app.preferences.skip_sync_confirmation =
                            skip && operation == crate::remotes::Operation::Sync;
                        app.network_dialog = Some(RemoteDialog {
                            operation,
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
                        assert_eq!(app.remote_confirmation_hidden(), skip);
                        deliver(
                            &mut app,
                            WorkerResult::Operation {
                                committed: false,
                                success: operation.success(),
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
                        if !failed {
                            assert!(app.status.starts_with(operation.success()));
                            assert!(!app.status.contains("Push failed"));
                        }
                        if skip {
                            assert!(app.status.contains(if failed {
                                "Rejected"
                            } else if refresh_failed {
                                "Refresh failed"
                            } else {
                                operation.success()
                            }));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn network_dialogs_keep_geometry_through_loading_errors_retry_and_cancellation() {
        use eframe::egui::{Context, Id, Pos2, RawInput};
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for style in [InterfaceStyle::Classic, InterfaceStyle::Modern] {
                for size in [egui::vec2(1100.0, 700.0), egui::vec2(1536.0, 1024.0)] {
                    for operation in [
                        crate::remotes::Operation::Sync,
                        crate::remotes::Operation::Fetch,
                        crate::remotes::Operation::Pull,
                        crate::remotes::Operation::Push,
                    ] {
                        let ctx = Context::default();
                        crate::theme::install_fonts(&ctx);
                        egui_extras::install_image_loaders(&ctx);
                        theme.apply(&ctx);
                        let mut app = SannyGitApp::with_theme(theme);
                        app.interface_style = style;
                        app.network_dialog = Some(RemoteDialog {
                            operation,
                            state: None,
                            remote: String::new(),
                            branch: String::new(),
                            set_upstream: false,
                            dont_show_again: false,
                        });
                        let mut baseline = None;
                        let mut button_positions = None;
                        let (sender, receiver) = mpsc::channel();
                        app.worker = Some(receiver);
                        for stage in 0..6 {
                            match stage {
                                1 => {
                                    app.worker = None;
                                    let dialog = app.network_dialog.as_mut().unwrap();
                                    dialog.remote = "origin".into();
                                    dialog.branch = "main".into();
                                    dialog.state = Some(crate::remotes::State {
                                        remotes: vec!["origin".into()],
                                        branch: Some("main".into()),
                                        ..Default::default()
                                    });
                                }
                                2 => {
                                    app.error = Some("Network request failed. ".repeat(100));
                                }
                                3 => {
                                    app.error = None;
                                    let (_sender, receiver) = mpsc::channel();
                                    app.worker = Some(receiver);
                                    app.manager.cancel = Some(Arc::new(AtomicBool::new(false)));
                                    app.status = "Network operation in progress…".into();
                                }
                                4 => {
                                    app.cancel_repository_job();
                                    assert!(app.busy());
                                    assert!(!app.remote_allowed());
                                    assert!(app
                                        .manager
                                        .cancel
                                        .as_ref()
                                        .unwrap()
                                        .load(Ordering::Relaxed));
                                }
                                5 => {
                                    app.worker = None;
                                    app.manager.cancel = None;
                                    app.error = Some("Operation cancelled; verify local and remote state before retrying".into());
                                }
                                _ => {}
                            }
                            for frame in 0..4 {
                                let mut output = ctx.run_ui(
                                    RawInput {
                                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                        ..Default::default()
                                    },
                                    |ui| ui::show(ui, &mut app),
                                );
                                output.textures_delta.clear();
                                if frame < 3 {
                                    continue;
                                }
                                let rect = ctx
                                    .memory(|memory| {
                                        memory.area_rect(Id::unique("remote.operation"))
                                    })
                                    .unwrap();
                                assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect));
                                if let Some(before) = baseline {
                                    assert_eq!(rect, before, "Dialog changed at stage {stage}: {operation:?} {theme:?} {style:?} {size:?}");
                                } else {
                                    baseline = Some(rect);
                                }
                                let buttons = output
                                    .shapes
                                    .iter()
                                    .filter_map(|shape| match &shape.shape {
                                        egui::Shape::Text(text)
                                            if text.galley.job.text == "Cancel"
                                                || text.galley.job.text == operation.label() =>
                                        {
                                            Some((text.galley.job.text.clone(), text.pos))
                                        }
                                        _ => None,
                                    })
                                    .collect::<Vec<_>>();
                                if let Some(before) = &button_positions {
                                    assert_eq!(
                                        &buttons, before,
                                        "Buttons moved at stage {stage}: {operation:?}"
                                    );
                                } else {
                                    button_positions = Some(buttons);
                                }
                            }
                        }
                        drop(sender);
                    }
                }
            }
        }
    }

    #[test]
    fn operation_outcomes_remain_distinct_from_refresh_and_cancellation() {
        for success in [
            "Commit created",
            "Commit amended",
            "Fetch complete",
            "Pull complete",
            "Push complete",
        ] {
            for failed in [false, true] {
                for refresh_failed in [false, true] {
                    for cancelled in [false, true] {
                        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
                        app.repositories = crate::mock_data::repositories();
                        app.selected_repository = 0;
                        app.changes = crate::mock_data::changes();
                        app.selected_change = 2;
                        app.commits = crate::mock_data::commits();
                        app.selected_commit = Some(app.commits[1].hash.clone());
                        let selected_commit = app.selected_commit.clone();
                        let selected_path = app.changes[2].path.clone();
                        app.commit_message = "Keep on failure".into();
                        app.state_current = false;
                        app.manager.cancel = Some(Arc::new(AtomicBool::new(false)));
                        if cancelled {
                            app.cancel_repository_job();
                        }
                        let snapshot = if refresh_failed {
                            Err("Refresh diagnostic".into())
                        } else {
                            Ok(git::Snapshot {
                                repository: Repository {
                                    name: app.repositories[0].name.clone(),
                                    path: app.repositories[0].path.clone(),
                                },
                                branch: "main".into(),
                                changes: crate::mock_data::changes(),
                            })
                        };
                        deliver(
                            &mut app,
                            WorkerResult::Operation {
                                committed: !failed && success.starts_with("Commit"),
                                success,
                                outcome: if failed {
                                    Err("Operation diagnostic".into())
                                } else {
                                    Ok(())
                                },
                                snapshot,
                            },
                        );
                        assert_eq!(app.state_current, !refresh_failed);
                        assert_eq!(app.selected_commit, selected_commit);
                        assert_eq!(app.changes[app.selected_change].path, selected_path);
                        assert_eq!(
                            app.commit_message.is_empty(),
                            !failed && success.starts_with("Commit")
                        );
                        if failed {
                            assert!(app
                                .error
                                .as_deref()
                                .unwrap()
                                .contains("Operation diagnostic"));
                            if cancelled {
                                assert!(app.status.starts_with("Cancellation requested"));
                                assert!(app
                                    .error
                                    .as_deref()
                                    .unwrap()
                                    .contains("does not roll back"));
                            } else {
                                assert!(app.status.starts_with("Operation failed"));
                            }
                        } else {
                            assert!(app.status.starts_with(success));
                        }
                        if refresh_failed {
                            assert!(app.error.as_deref().unwrap().contains("Refresh diagnostic"));
                            assert!(app.status.contains("refresh required"));
                        } else if !failed {
                            assert!(app.error.is_none());
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn operation_errors_keep_draft_and_success_restores_normal_draft_even_if_refresh_fails() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.commit_message = "Keep draft".into();
        app.amend = true;
        app.amend_head = Some("a".repeat(40));
        app.amend_draft = Some("Original draft".into());
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
        assert!(app.amend);
        assert!(app.amend_head.is_some());
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
        assert_eq!(app.commit_message, "Original draft");
        assert!(!app.amend);
        assert!(app.amend_head.is_none());
        assert!(app.amend_draft.is_none());
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
                generation: 0,
                append: false,
                result: Err("Late history result".into()),
            },
        );
        assert_eq!(app.commits.len(), count);
        deliver(
            &mut app,
            WorkerResult::History {
                path,
                all: false,
                generation: 0,
                append: false,
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
    fn rename_requires_fresh_local_target_and_retains_draft_on_failure() {
        use crate::menu::MenuAction;
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        app.refs = crate::mock_data::refs();
        app.selected_branch = "refs/heads/develop".into();
        app.state_current = true;
        assert!(!app.menu_action_enabled(MenuAction::RenameBranch));
        app.refs_current = true;
        assert!(app.menu_action_enabled(MenuAction::RenameBranch));
        app.open_rename_branch();
        assert!(app.rename_branch_blocker().is_some()); // unchanged name
        app.rename_branch_dialog.as_mut().unwrap().name = "renamed".into();
        app.changes = crate::mock_data::changes();
        assert!(app.rename_branch_blocker().is_none()); // dirty files are allowed
        app.selected_branch = "refs/tags/v1.0.0".into();
        assert!(!app.menu_action_enabled(MenuAction::RenameBranch));
        assert!(app.rename_branch_blocker().is_none()); // dialog keeps its source
        deliver(
            &mut app,
            WorkerResult::Operation {
                committed: false,
                success: "Branch renamed",
                outcome: Err("Cannot rename".into()),
                snapshot: Err("Refresh failed".into()),
            },
        );
        assert_eq!(app.rename_branch_dialog.as_ref().unwrap().name, "renamed");
        assert!(app.rename_branch_blocker().is_some());
        deliver(
            &mut app,
            WorkerResult::Operation {
                committed: false,
                success: "Branch renamed",
                outcome: Ok(()),
                snapshot: Err("Refresh failed".into()),
            },
        );
        assert!(app.rename_branch_dialog.is_none());
        assert_eq!(app.selected_branch, "refs/heads/renamed");
    }

    #[test]
    fn tag_creation_requires_fresh_state_and_keeps_draft_on_failure() {
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        app.repositories = crate::mock_data::repositories();
        app.selected_repository = 0;
        app.new_tag_name = "release/new".into();
        app.new_tag_message = "Release details".into();
        app.tag_dialog_open = true;
        app.state_current = true;
        assert!(app.create_tag_blocker().is_some());
        app.refs_current = true;
        assert!(app.create_tag_blocker().is_some());
        app.branch_head_exists = true;
        app.changes = crate::mock_data::changes();
        assert!(app.create_tag_blocker().is_none());
        deliver(
            &mut app,
            WorkerResult::Operation {
                committed: false,
                success: "Tag created",
                outcome: Err("Tag exists".into()),
                snapshot: Err("Refresh failed".into()),
            },
        );
        assert!(app.tag_dialog_open);
        assert_eq!(app.new_tag_name, "release/new");
        assert_eq!(app.new_tag_message, "Release details");
        assert!(app.create_tag_blocker().is_some());
        deliver(
            &mut app,
            WorkerResult::Operation {
                committed: false,
                success: "Tag created",
                outcome: Ok(()),
                snapshot: Err("Refresh failed".into()),
            },
        );
        assert!(!app.tag_dialog_open);
        assert!(app.new_tag_name.is_empty());
        assert!(app.new_tag_message.is_empty());
        assert_eq!(app.selected_branch, "refs/tags/release/new");
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
                    crate::remotes::Operation::Sync,
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
                    let destination_label = if matches!(
                        operation,
                        crate::remotes::Operation::Push | crate::remotes::Operation::Sync
                    ) {
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
