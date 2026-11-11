pub mod discard;
mod errors;
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
    CommitFiles {
        path: String,
        hash: String,
        generation: u64,
        result: Result<Vec<crate::history::CommitFile>, String>,
    },
    CommitPatch {
        path: String,
        file: String,
        generation: u64,
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
    pub secret_error: Option<String>,
}

impl LlmSettings {
    pub fn save_secret(
        &self,
        previous: &crate::llm::Config,
        repository: Option<&str>,
    ) -> Result<(), String> {
        if self.secret_changed(previous) {
            crate::llm::save_api_key(&self.config, repository)?;
        }
        Ok(())
    }

    fn secret_changed(&self, previous: &crate::llm::Config) -> bool {
        let config = &self.config;
        // An unloaded key is empty in the draft. Preserve it when only the
        // model, endpoint or other preferences change; never read it to save.
        self.api_key_dirty
            || (!config.store_api_key && previous.store_api_key)
            || (!config.api_key.is_empty()
                && (config.store_api_key != previous.store_api_key
                    || config.endpoint != previous.endpoint
                    || config.model != previous.model))
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
    pub error_dialogs: errors::ErrorDialogs,
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
    pub commit_files: Option<Vec<crate::history::CommitFile>>,
    pub selected_commit_file: Option<String>,
    loaded_commit_file: Option<String>,
    commit_preview_generation: u64,
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
            error_dialogs: Default::default(),
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
            commit_files: None,
            selected_commit_file: None,
            loaded_commit_file: None,
            commit_preview_generation: 0,
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
        if !same_repository {
            self.clear_commit_preview();
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

    fn clear_commit_preview(&mut self) {
        self.commit_preview = None;
        self.commit_preview_error = None;
        self.loaded_commit = None;
        self.commit_files = None;
        self.selected_commit_file = None;
        self.loaded_commit_file = None;
        self.commit_preview_generation = self.commit_preview_generation.wrapping_add(1);
    }

    pub fn select_commit(&mut self, hash: String) {
        if !self.commits.iter().any(|c| c.hash == hash) {
            return;
        }
        if self.selected_commit.as_ref() != Some(&hash) {
            self.clear_commit_preview();
        }
        self.selected_commit = Some(hash);
        self.commit_details_open = true;
    }

    pub fn select_commit_file(&mut self, path: String) {
        if !self
            .commit_files
            .as_ref()
            .is_some_and(|files| files.iter().any(|f| f.path == path))
            || self.selected_commit_file.as_ref() == Some(&path)
        {
            return;
        }
        self.selected_commit_file = Some(path);
        self.commit_preview = None;
        self.commit_preview_error = None;
        self.loaded_commit_file = None;
        self.commit_preview_generation = self.commit_preview_generation.wrapping_add(1);
    }

    pub fn retry_commit_preview(&mut self) {
        if self.busy() {
            return;
        }
        if self.commit_files.is_none() {
            self.loaded_commit = None;
        }
        self.loaded_commit_file = None;
        self.commit_preview_error = None;
        self.commit_preview_generation = self.commit_preview_generation.wrapping_add(1);
    }

    fn load_commit_preview(&mut self) {
        if self.busy() || !self.commit_details_open {
            return;
        }
        let Some(hash) = self.selected_commit.clone() else {
            return;
        };
        let Some(commit) = self.commits.iter().find(|c| c.hash == hash) else {
            return;
        };
        let parent = commit.parents.first().cloned();
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let path = repository.path.clone();
        let root = PathBuf::from(&path);
        let generation = self.commit_preview_generation;
        if self.loaded_commit.as_ref() != Some(&hash) {
            self.loaded_commit = Some(hash.clone());
            if !self.start_worker(move || WorkerResult::CommitFiles {
                result: crate::history::files(&root, &hash, parent.as_deref()),
                path,
                hash,
                generation,
            }) {
                self.commit_preview_error = self.error.clone();
            }
            return;
        }
        let Some(file) = self
            .commit_files
            .as_ref()
            .and_then(|files| {
                files
                    .iter()
                    .find(|file| Some(&file.path) == self.selected_commit_file.as_ref())
            })
            .cloned()
        else {
            return;
        };
        if self.loaded_commit_file.as_ref() == Some(&file.path) {
            return;
        }
        self.loaded_commit_file = Some(file.path.clone());
        if !self.start_worker(move || WorkerResult::CommitPatch {
            result: crate::history::file_patch(&root, &hash, parent.as_deref(), &file)
                .map(|patch| crate::models::DiffPreview::new(&patch)),
            path,
            file: file.path,
            hash,
            generation,
        }) {
            self.commit_preview_error = self.error.clone();
        }
    }

    fn commit_preview_matches(&self, path: &str, hash: &str, generation: u64) -> bool {
        self.repositories
            .get(self.selected_repository)
            .is_some_and(|repo| repo.path == path)
            && self.selected_commit.as_deref() == Some(hash)
            && self.commit_preview_generation == generation
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
        self.commit_contents_preview_blocker().or_else(|| {
            if self.commit_contents == git::CommitContents::Staged
                && !self.changes.iter().any(|c| c.staged)
            {
                Some("There are no staged changes to describe")
            } else if self.changes.is_empty() {
                Some("There are no changes to describe")
            } else {
                None
            }
        })
    }

    pub fn commit_contents_preview_blocker(&self) -> Option<&'static str> {
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
        if self.commit_contents_preview_blocker().is_some() {
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
            WorkerResult::CommitFiles {
                path,
                hash,
                generation,
                result,
            } => {
                if self.commit_preview_matches(&path, &hash, generation) {
                    match result {
                        Ok(files) => {
                            self.selected_commit_file = files.first().map(|file| file.path.clone());
                            self.commit_files = Some(files);
                            self.commit_preview_error = None;
                        }
                        Err(error) => self.commit_preview_error = Some(error),
                    }
                }
            }
            WorkerResult::CommitPatch {
                path,
                file,
                hash,
                generation,
                result,
            } => {
                if self.commit_preview_matches(&path, &hash, generation)
                    && self.selected_commit_file.as_ref() == Some(&file)
                {
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
mod tests;
