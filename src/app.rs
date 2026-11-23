use crate::git::GitContext;
mod commit;
mod commit_ops;
mod compare;
mod sequencer;
#[cfg(test)]
use commit::CommitDraft;
use commit::CommitState;
pub mod conflict;
pub mod discard;
mod errors;
mod file_selection;
pub mod merge;
pub mod ref_management;
mod remotes;
pub mod stash;
pub use remotes::RemoteDialog;
pub mod diff_actions;
mod history;
mod llm_models;
mod push_up_to;
mod repositories;
mod tracking;
mod working_tree;
pub use llm_models::ModelCatalog;
pub use repositories::{Dialog as RepositoryDialog, Job as RepositoryJob, RepositoryManager};

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
};

enum WorkerResult {
    ConflictReview(Result<Box<crate::conflict::Review>, String>),
    Index {
        staged: bool,
        outcome: Result<(), String>,
        update: working_tree::Update,
    },
    DiscardBatchPrepared {
        root: String,
        paths: Vec<String>,
        result: Result<Vec<crate::discard::Prepared>, String>,
    },
    MergeDetails {
        root: String,
        result: Result<merge::Snapshot, String>,
    },
    PushUpToDetails {
        root: String,
        result: Result<crate::remotes::PushUpToPreview, String>,
    },
    RefDetails {
        root: String,
        result: Box<ref_management::ResultMessage>,
    },
    TagPushSelection {
        root: String,
        source: String,
        preferred_remote: Option<String>,
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
    CommitMessage {
        result: Result<String, String>,
        config: crate::llm::Config,
        repository: Option<String>,
    },
    AiConnection {
        sign_in: bool,
        result: Result<String, String>,
    },
    AmendMessage(Result<(String, String), String>),
    Repository(Result<crate::git::Snapshot, String>),
    Operation {
        committed: bool,
        success: &'static str,
        outcome: Result<(), String>,
        snapshot: Result<crate::git::Snapshot, String>,
    },
    RevertMergeParents(commit_ops::RevertMergeDialog),
    CommitCompare {
        path: String,
        from: String,
        to: String,
        generation: u64,
        result: Result<crate::models::DiffPreview, String>,
    },
    Diff {
        index: usize,
        result: Result<(String, crate::models::DiffPreview), String>,
    },
}

use crate::{
    git, logging,
    models::{ChangeFile, Repository},
    theme::{InterfaceStyle, Palette, ThemeChoice, INTERFACE_STYLE_STORAGE_KEY, THEME_STORAGE_KEY},
    ui,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeFilter {
    All,
    Modified,
    Added,
    Deleted,
    Conflicted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsPage {
    General,
    Appearance,
    DateTime,
    Logging,
    Git,
    Remote,
    Llm,
    CommitMessages,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepositoryLlmTab {
    #[default]
    Model,
    Prompt,
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChatGptStatus {
    Unknown,
    Checking,
    SignedOut,
    ChangingSession,
    Connected,
    Failed,
}

pub struct SettingsWindow {
    pub open: bool,
    resume_after_ai_action: bool,
    pub initial_position_set: bool,
    pub initial_position: Option<eframe::egui::Pos2>,
    pub needs_reveal: bool,
    pub page: SettingsPage,
    pub theme: ThemeChoice,
    pub preferences: crate::preferences::Preferences,
    pub interface_style: InterfaceStyle,
    pub auto_fetch: bool,
    pub skip_push_confirmation: bool,
    pub llm: LlmSettings,
    pub llm_repositories: BTreeMap<String, LlmSettings>,
    pub llm_instructions: BTreeMap<String, String>,
    pub llm_repository: Option<String>,
    pub llm_repository_tab: RepositoryLlmTab,
    pub chatgpt_status: ChatGptStatus,
    chatgpt_status_reader: Option<Receiver<Result<bool, String>>>,
    pub chatgpt_models: Vec<crate::llm::chatgpt::Model>,
    chatgpt_models_reader: Option<Receiver<Result<Vec<crate::llm::chatgpt::Model>, String>>>,
    pub chatgpt_models_attempted: bool,
}

impl SettingsWindow {
    fn new(theme: ThemeChoice) -> Self {
        Self {
            open: false,
            resume_after_ai_action: false,
            initial_position_set: false,
            initial_position: None,
            needs_reveal: false,
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
            llm_instructions: BTreeMap::new(),
            llm_repository: None,
            llm_repository_tab: RepositoryLlmTab::Model,
            chatgpt_status: ChatGptStatus::Unknown,
            chatgpt_status_reader: None,
            chatgpt_models: Vec::new(),
            chatgpt_models_reader: None,
            chatgpt_models_attempted: false,
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WelcomeAction {
    Add,
    Clone,
    Reopen,
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
    pub repository_llm_instructions: BTreeMap<String, String>,
    pub llm_models: ModelCatalog,
    pub about_dialog: bool,
    pub welcome_open: bool,
    pub(crate) welcome_action: WelcomeAction,
    pub(crate) welcome_repository: Option<String>,
    welcome_pending: bool,
    applied_interface_style: InterfaceStyle,
    pub repositories: Vec<Repository>,
    pub branches: ref_management::BranchState,
    pub network_dialog: Option<remotes::RemoteDialog>,
    pub background_progress_only: bool,
    pub stash_dialog: Option<stash::Dialog>,
    pub ignore_dialog: Option<file_selection::IgnoreDialog>,
    pub discard_dialog: Option<discard::Dialog>,
    pub merge_dialog: Option<merge::Dialog>,
    pub push_up_to: Option<push_up_to::Dialog>,
    pub conflict_dialog: Option<conflict::Dialog>,
    pub abort_merge_dialog: Option<String>,
    pub revert_merge_dialog: Option<commit_ops::RevertMergeDialog>,
    pub skip_push_confirmation: bool,
    pub changes: Vec<ChangeFile>,
    pub history: history::View,
    tag_log: Option<history::View>,
    pub selected_repository: usize,
    pub selection: file_selection::FileSelection,
    pub change_filter: ChangeFilter,
    pub commit: CommitState,
    pub repository_path: String,
    pub open_dialog: bool,
    pub error: Option<String>,
    pub error_dialogs: errors::ErrorDialogs,
    pub status: String,
    worker: Option<Receiver<WorkerResult>>,
    diff_worker: Option<Receiver<WorkerResult>>,
    diff_refresh_pending: bool,
    working_tree: working_tree::Monitor,
    loaded_diff: Option<usize>,
    pub diff_preview: Option<crate::models::DiffPreview>,
    pub diff_line_selection: BTreeSet<usize>,
    pub diff_discard: Option<crate::hunk::Request>,
    pub rename_dialog: Option<diff_actions::RenamePathDialog>,
    pub state_current: bool,
    pub merging: bool,
    pub sequencing: crate::git::Sequencing,
}

pub(crate) fn clipboard_message(message: String) -> String {
    #[cfg(windows)]
    {
        message.replace("\r\n", "\n").replace('\n', "\r\n")
    }
    #[cfg(not(windows))]
    {
        message
    }
}

pub(crate) fn copy_to_clipboard(ctx: &eframe::egui::Context, message: String) {
    let message = clipboard_message(message);
    if !message.is_empty() {
        ctx.copy_text(message);
    }
}

impl SannyGitApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install_fonts(&creation_context.egui_ctx);
        egui_extras::install_image_loaders(&creation_context.egui_ctx);
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        crate::native_window::install_system_theme(&creation_context.egui_ctx);
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
        let interface_style: InterfaceStyle = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, INTERFACE_STYLE_STORAGE_KEY))
            .unwrap_or_default();
        interface_style.apply(&creation_context.egui_ctx);
        theme.select(&creation_context.egui_ctx, preferences.follow_system_theme);

        let mut app = Self::with_theme(theme);
        app.preferences = preferences;
        if let Err(error) = logging::configure(app.preferences.log_level) {
            app.error = Some(error);
        }
        logging::record(
            logging::Level::Info,
            format_args!("SannyGit {} started", env!("CARGO_PKG_VERSION")),
        );
        app.welcome_pending = app.preferences.show_welcome;
        app.interface_style = interface_style;
        app.applied_interface_style = interface_style;
        app.skip_push_confirmation = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, "sannygit.skip_push_confirmation"))
            .unwrap_or(false);
        app.branches.last_reset_strategy = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, "sannygit.reset_strategy"))
            .unwrap_or_default();
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
        app.repository_llm_instructions = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, "sannygit.repository_llm_instructions"))
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
            app.apply_bookmark_names();
        }
        if let Some(path) = std::env::var_os("SANNYGIT_OPEN_REPOSITORY") {
            app.open_repository(PathBuf::from(path).to_string_lossy().into_owned());
        } else if let Some(storage) = creation_context.storage {
            app.restore_active_repository(storage);
        }
        app
    }

    fn restore_commit_drafts(&mut self, storage: &dyn eframe::Storage) {
        self.commit.commit_drafts =
            eframe::get_value(storage, "sannygit.commit_drafts").unwrap_or_default();
    }

    fn remember_commit_draft(&mut self) {
        if let Some(repo) = self.repositories.get(self.selected_repository) {
            self.commit.remember(&repo.path);
        }
    }

    fn load_commit_draft(&mut self) {
        let path = self
            .repositories
            .get(self.selected_repository)
            .map(|repo| repo.path.as_str());
        self.commit.load(path);
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
            repository_llm_instructions: BTreeMap::new(),
            llm_models: ModelCatalog::default(),
            about_dialog: false,
            welcome_open: false,
            welcome_action: WelcomeAction::Reopen,
            welcome_repository: None,
            welcome_pending: false,
            applied_interface_style: InterfaceStyle::default(),
            repositories: Vec::new(),
            branches: Default::default(),
            network_dialog: None,
            background_progress_only: false,
            stash_dialog: None,
            ignore_dialog: None,
            discard_dialog: None,
            merge_dialog: None,
            push_up_to: None,
            conflict_dialog: None,
            abort_merge_dialog: None,
            revert_merge_dialog: None,
            skip_push_confirmation: false,
            changes: Vec::new(),
            history: history::View::default(),
            tag_log: None,
            selected_repository: usize::MAX,
            selection: Default::default(),
            change_filter: ChangeFilter::All,
            commit: CommitState::default(),
            repository_path: String::new(),
            open_dialog: false,
            error: None,
            error_dialogs: Default::default(),
            status: "Ready".into(),
            worker: None,
            diff_worker: None,
            diff_refresh_pending: false,
            working_tree: working_tree::Monitor::default(),
            loaded_diff: None,
            diff_preview: None,
            diff_line_selection: BTreeSet::new(),
            diff_discard: None,
            rename_dialog: None,
            state_current: false,
            merging: false,
            sequencing: crate::git::Sequencing::None,
        }
    }
    pub fn selected_diff(&self) -> Option<&crate::models::DiffPreview> {
        (self.loaded_diff == Some(self.selection.active))
            .then_some(self.diff_preview.as_ref())
            .flatten()
    }

    fn refresh_llm_models(&mut self, context: &eframe::egui::Context) {
        // Cloud endpoints usually require authentication. Keep their cached
        // models until a key is available, without reading the OS key store.
        if !self.llm_config.chatgpt_subscription
            && (self.llm_config.is_local() || !self.llm_config.api_key.trim().is_empty())
        {
            self.llm_models
                .load_at_startup(&self.llm_config, None, context);
        }
        for (path, config) in &self.repository_llm_configs {
            if !config.chatgpt_subscription
                && (config.is_local() || !config.api_key.trim().is_empty())
            {
                self.llm_models.load_at_startup(config, Some(path), context);
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }

    /// A modal dialog owns interaction with the main window. Secondary windows
    /// like commit details and Conflict Solver remain independent.
    pub fn modal_dialog_open(&self) -> bool {
        self.settings_window.open
            || self.welcome_open
            || self.about_dialog
            || (!self.background_progress_only && self.operation_dialog_open())
            || self.error.is_some()
            || !self.error_dialogs.pending.is_empty()
    }

    fn operation_dialog_open(&self) -> bool {
        self.open_dialog
            || self.manager.dialog.is_some()
            || (self.network_dialog.is_some() && !self.remote_confirmation_hidden())
            || self.stash_dialog.is_some()
            || self.discard_dialog.is_some()
            || self.diff_discard.is_some()
            || self.rename_dialog.is_some()
            || self.ignore_dialog.is_some()
            || self.branches.ref_dialog.is_some()
            || self.merge_dialog.is_some()
            || self.push_up_to.is_some()
            || self
                .conflict_dialog
                .as_ref()
                .is_some_and(|dialog| dialog.action != crate::conflict::Action::Edit)
            || self.abort_merge_dialog.is_some()
            || self.revert_merge_dialog.is_some()
            || self.branches.remote_checkout.is_some()
            || self.branches.branch_dialog_open
            || self.branches.rename_branch_dialog.is_some()
            || self.branches.tag_dialog_open
    }

    fn start_background_operation(
        &mut self,
        job: impl FnOnce(&GitContext) -> WorkerResult + Send + 'static,
    ) -> bool {
        let started = self.start_worker(job);
        if started {
            self.background_progress_only = true;
        }
        started
    }

    fn start_worker(
        &mut self,
        job: impl FnOnce(&GitContext) -> WorkerResult + Send + 'static,
    ) -> bool {
        self.invalidate_view_reads();
        // An operation supersedes a ref read; reload after the operation completes.
        if self.branches.worker.take().is_some() {
            self.branches.pending = true;
        }
        // A user operation supersedes any in-flight automatic read.
        self.working_tree.invalidate();
        self.tracking.cancel();
        let (sender, receiver) = mpsc::channel();
        let git_context = crate::git::GitContext::new(self.preferences.git_executable.clone());
        match std::thread::Builder::new()
            .name("sannygit-reader".into())
            .spawn(move || {
                let git = &git_context;
                let _ = sender.send(job(git));
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

    fn spawn_reader<T: Send + 'static>(
        &self,
        repaint: Option<eframe::egui::Context>,
        job: impl FnOnce(&GitContext) -> T + Send + 'static,
    ) -> Result<Receiver<T>, String> {
        let (sender, receiver) = mpsc::channel();
        let git_context = crate::git::GitContext::new(self.preferences.git_executable.clone());
        std::thread::Builder::new()
            .name("sannygit-view".into())
            .spawn(move || {
                let git = &git_context;
                let _ = sender.send(job(git));
                if let Some(context) = repaint {
                    context.request_repaint();
                }
            })
            .map_err(|error| format!("Cannot start background reader: {error}"))?;
        Ok(receiver)
    }

    fn invalidate_view_reads(&mut self) {
        self.history.invalidate_reads();
        if let Some(view) = &mut self.tag_log {
            view.invalidate_reads();
        }
        if self.diff_worker.take().is_some() {
            if self.selected_diff().is_some() {
                self.diff_refresh_pending = true;
            } else {
                self.loaded_diff = None;
            }
        }
    }

    fn poll_reader<T>(receiver: &mut Option<Receiver<T>>) -> Option<Result<T, String>> {
        let result = match receiver.as_ref()?.try_recv() {
            Ok(result) => Ok(result),
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Background reader stopped unexpectedly".into())
            }
        };
        *receiver = None;
        Some(result)
    }

    fn poll_view_reads(&mut self) {
        self.poll_history_reads();
        if self.tag_log.is_some() {
            self.with_history_window(|app| app.poll_history_reads());
        }
        if let Some(result) = Self::poll_reader(&mut self.diff_worker) {
            match result {
                Ok(result) => self.apply_worker_result(result),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn poll_history_reads(&mut self) {
        if let Some(result) = Self::poll_reader(&mut self.history.history_worker) {
            match result {
                Ok(result) => self.apply_worker_result(result),
                Err(error) => {
                    self.history.history_loading = false;
                    self.history.history_session.cancel = None;
                    self.history.history_error = Some(error);
                }
            }
        }
        if let Some(result) = Self::poll_reader(&mut self.history.commit_preview_worker) {
            match result {
                Ok(result) => self.apply_worker_result(result),
                Err(error) => self.history.commit_preview_error = Some(error),
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
        self.start_worker(move |git| {
            WorkerResult::Repository(git::load(git, &PathBuf::from(path)))
        });
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
            | MenuAction::RepositorySettings
            | MenuAction::EditRepositoryConfig => {
                !self.busy() && self.repositories.get(self.selected_repository).is_some()
            }
            MenuAction::RemoveWorktree | MenuAction::PruneWorktrees => {
                !self.busy()
                    && self.manager.worktree_availability.as_ref().is_some_and(
                        |(path, remove, prune)| {
                            self.repositories
                                .get(self.selected_repository)
                                .is_some_and(|r| r.path == *path)
                                && if action == MenuAction::RemoveWorktree {
                                    *remove
                                } else {
                                    *prune
                                }
                        },
                    )
            }
            MenuAction::Open
            | MenuAction::Preferences
            | MenuAction::LightTheme
            | MenuAction::DarkTheme
            | MenuAction::MaximizeWindow
            | MenuAction::MinimizeWindow
            | MenuAction::FullScreen
            | MenuAction::Close
            | MenuAction::Exit
            | MenuAction::About => true,
            MenuAction::Refresh | MenuAction::ReloadHistory => {
                !self.busy() && self.repositories.get(self.selected_repository).is_some()
            }
            MenuAction::Stage => self.selected_staging_allowed(true),
            MenuAction::Unstage => self.selected_staging_allowed(false),
            MenuAction::Commit => self.commit_blocker().is_none(),
            MenuAction::CreateBranch => {
                !self.busy()
                    && self.state_current
                    && self.branches.current
                    && self.branches.branch_head_exists
            }
            MenuAction::SwitchBranch => self.switch_branch_blocker().is_none(),
            MenuAction::RenameBranch => self
                .rename_branch_target_blocker(&self.branches.selected_branch)
                .is_none(),
            MenuAction::DeleteBranch => self.delete_branch_blocker().is_none(),
            MenuAction::PushTag => self.push_tag_blocker().is_none(),
            MenuAction::ManageRemotes => self.remote_allowed(),
            MenuAction::Discard => self.discard_blocker().is_none(),
            MenuAction::Stashes => self.stash_allowed(),
            MenuAction::Merge => self.merge_allowed(),
            MenuAction::Rebase => self.merge_allowed(),
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
            MenuAction::MaximizeWindow => {
                context.send_viewport_cmd(eframe::egui::ViewportCommand::Maximized(true))
            }
            MenuAction::MinimizeWindow => {
                context.send_viewport_cmd(eframe::egui::ViewportCommand::Minimized(true))
            }
            MenuAction::FullScreen => {
                let fullscreen =
                    context.input(|input| input.viewport().fullscreen.unwrap_or(false));
                context.send_viewport_cmd(eframe::egui::ViewportCommand::Fullscreen(!fullscreen));
            }
            MenuAction::AddGroup => self.manager.dialog = Some(RepositoryDialog::Group),
            MenuAction::Clone => {
                self.manager.clone_step = 0;
                self.manager.dialog = Some(RepositoryDialog::Clone);
            }
            MenuAction::SearchRepositories => self.manager.dialog = Some(RepositoryDialog::Search),
            MenuAction::CloseRepository => self.close_repository(false),
            MenuAction::AddWorktree => {
                self.run_repository_job(RepositoryJob::PrepareAddWorktree);
            }
            MenuAction::RemoveWorktree => {
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
            MenuAction::Stage => self.set_selected_staged(true),
            MenuAction::Unstage => self.set_selected_staged(false),
            MenuAction::Commit => self.create_commit(),
            MenuAction::DeleteBranch => self.open_delete_branch(),
            MenuAction::PushTag => self.open_push_tag(),
            MenuAction::ManageRemotes => self.open_remote_manager(),
            MenuAction::Discard => self.open_discard(),
            MenuAction::Stashes => self.open_stashes(),
            MenuAction::Merge => self.open_merge(),
            MenuAction::Rebase => self.open_rebase(),
            MenuAction::CreateBranch => self.branches.branch_dialog_open = true,
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
            MenuAction::Close | MenuAction::Exit => {
                context.send_viewport_cmd(eframe::egui::ViewportCommand::Close)
            }
            MenuAction::About => self.about_dialog = true,
        }
        context.request_repaint();
    }

    #[cfg(not(target_os = "macos"))]
    fn handle_shortcuts(&mut self, context: &eframe::egui::Context) {
        use crate::menu::{MenuAction, SHORTCUTS};
        use eframe::egui::Event;

        if self.modal_dialog_open()
            || self.history_window().commit_details_open
            || context.memory(|memory| memory.top_modal_layer().is_some())
            || eframe::egui::Popup::is_any_open(context)
        {
            return;
        }
        let editing = context.text_edit_focused();
        let commit_editor = context
            .memory(|memory| memory.focused() == Some(eframe::egui::Id::unique("commit.message")));
        let mut selected = None;
        context.input_mut(|input| {
            input.events.retain(|event| {
                let Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    repeat,
                    ..
                } = event
                else {
                    return true;
                };
                let Some((action, _)) = SHORTCUTS.iter().find(|(_, shortcut)| {
                    shortcut.logical_key == *key && modifiers.matches_exact(shortcut.modifiers)
                }) else {
                    return true;
                };
                if editing
                    && *action != MenuAction::FullScreen
                    && !(*action == MenuAction::Commit && commit_editor)
                {
                    return true;
                }
                if !repeat && selected.is_none() && self.menu_action_enabled(*action) {
                    selected = Some(*action);
                }
                false
            });
        });
        if let Some(action) = selected {
            self.handle_menu_action(action, context);
        }
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
        if self.settings_window.chatgpt_models_reader.is_none() {
            self.settings_window.chatgpt_models_attempted = false;
        }
        if !std::mem::take(&mut self.settings_window.resume_after_ai_action) {
            self.load_settings_draft();
        }
        self.settings_window.llm_repository = None;
        self.settings_window.initial_position_set = false;
        self.settings_window.initial_position = None;
        self.settings_window.needs_reveal = true;
        self.settings_window.open = true;
    }

    fn load_settings_draft(&mut self) {
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
        self.settings_window.llm_instructions = self.repository_llm_instructions.clone();
        // The prompt tab tracks overrides by their dedicated map entry; mirror
        // per-repository configs saved before the instructions were split out.
        for (path, config) in &self.repository_llm_configs {
            if config.instructions != self.llm_config.instructions {
                self.settings_window
                    .llm_instructions
                    .entry(path.clone())
                    .or_insert_with(|| config.instructions.clone());
            }
        }
    }

    pub fn open_repository_llm_settings(&mut self) {
        let Some(path) = self
            .repositories
            .get(self.selected_repository)
            .map(|repository| repository.path.clone())
        else {
            return;
        };
        self.settings_window.resume_after_ai_action = false;
        self.open_settings();
        self.settings_window.page = SettingsPage::Llm;
        self.settings_window.llm_repository = Some(path);
        self.settings_window.llm_repository_tab = RepositoryLlmTab::Model;
    }

    pub fn refresh(&mut self) {
        self.refresh_working_tree();
    }

    fn apply_snapshot(&mut self, snapshot: git::Snapshot) {
        self.invalidate_worktree_availability();
        self.invalidate_view_reads();
        self.branches.worker = None;
        let same_branch = self.branches.current_branch == snapshot.branch;
        if !same_branch {
            self.tracking.branch_changed();
        } else {
            self.tracking.invalidate();
        }
        self.state_current = true;
        let selected = self
            .changes
            .get(self.selection.active)
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
            self.working_tree.refs_token = None;
        }
        self.selected_repository = index;
        if !same_repository {
            self.manager.highlighted_repository = Some(self.repositories[index].path.clone());
        }
        if new_repository {
            let path = self.repositories[index].path.clone();
            self.assign_group(&path, self.manager.preferences.selected_group.clone());
        }
        self.branches.current_branch = snapshot.branch;
        self.branches.head_commit = snapshot.head;
        let retained_diff = if same_repository && same_branch && self.selected_diff().is_some() {
            self.changes
                .get_mut(self.selection.active)
                .map(|change| std::mem::take(&mut change.diff))
        } else {
            None
        };
        self.changes = snapshot.changes;
        self.update_commit_contents_after_refresh(!same_repository);
        let retained_selection = if same_repository {
            selected.and_then(|path| self.changes.iter().position(|c| c.path == path))
        } else {
            None
        };
        self.selection.active = retained_selection
            .or_else(|| (!self.changes.is_empty()).then_some(0))
            .unwrap_or(usize::MAX);
        if let (Some(index), Some(diff)) = (retained_selection, retained_diff) {
            self.loaded_diff = Some(index);
            self.changes[index].diff = diff;
            self.diff_refresh_pending = true;
        } else {
            self.loaded_diff = None;
            self.diff_preview = None;
            self.diff_refresh_pending = false;
            self.diff_line_selection.clear();
        }
        self.open_dialog = false;
        if !same_repository {
            self.selection.paths.clear();
            self.selection.anchor = None;
        }
        self.retain_change_selection();
        self.branches.current = false;
        self.branches.pending = true;
        self.branches.loading = true;
        self.branches.error = None;
        self.queue_history_reload();
        if !same_repository {
            self.close_tag_log();
            self.clear_commit_preview();
            self.manager.dialog = None;
            self.manager.worktrees.clear();
            self.manager.settings = None;
            self.branches.selected_branch.clear();
            self.branches.selected_ref_group = None;
            self.branches.entries.clear();
            self.branches.branch_head_exists = false;
            self.branches.branch_dialog_open = false;
            self.branches.rename_branch_dialog = None;
            self.branches.tag_dialog_open = false;
            self.branches.new_tag_name.clear();
            self.branches.new_tag_message.clear();
            self.branches.remote_checkout = None;
            self.network_dialog = None;
            self.stash_dialog = None;
            self.discard_dialog = None;
            self.diff_discard = None;
            self.rename_dialog = None;
            self.ignore_dialog = None;
            self.branches.ref_dialog = None;
            self.merge_dialog = None;
            self.push_up_to = None;
            if !self.conflict_solver_open() {
                self.conflict_dialog = None;
            }
            self.abort_merge_dialog = None;
            self.branches.new_branch_name.clear();
            self.history.commits.clear();
            self.history.history_session.clear_view();
            self.history.history_edges.clear();
            self.history.history_max_lane = 0;
            self.history.history_truncated = false;
            self.history.selected_commit = None;
            self.history.commit_details_open = false;
            self.history.commit_preview_error = None;
            self.load_commit_draft();
            self.history.journal_filter.clear();
            self.history.reveal_head_on_reload();
        } else if self.tag_log.is_some() {
            self.with_history_window(|app| app.queue_history_reload());
        }
        self.sequencing = snapshot.sequencing;
        self.update_merge_message(snapshot.merging, snapshot.merge_message, same_repository);
        if self.preferences.show_ignored_files {
            self.working_tree.read_soon();
        }
        self.status = if self.changes.is_empty() {
            "Working tree clean"
        } else {
            "Ready"
        }
        .into();
    }

    fn update_merge_message(
        &mut self,
        merging: bool,
        message: Option<String>,
        same_repository: bool,
    ) {
        if !same_repository {
            self.commit.merge_default_message = None;
        } else if self.merging
            && !merging
            && self.commit.merge_default_message.take().as_ref() == Some(&self.commit.message)
        {
            self.commit.message.clear();
        }
        if merging && (!self.merging || !same_repository) && self.commit.message.is_empty() {
            if let Some(message) = message {
                self.commit.message.clone_from(&message);
                self.commit.merge_default_message = Some(message);
            }
        }
        self.merging = merging;
    }

    fn update_commit_contents_after_refresh(&mut self, opening_repository: bool) {
        if !self.preferences.staged_only_after_staging {
            return;
        }
        if !self.changes.iter().any(|change| change.staged) {
            self.commit.contents = git::CommitContents::All;
        } else if opening_repository {
            self.commit.contents = git::CommitContents::Staged;
        }
    }
    pub fn reload_refs(&mut self) {
        if self.busy() || self.repositories.get(self.selected_repository).is_none() {
            return;
        }
        // Keep the last successful snapshot usable during this read-only refresh.
        self.branches.pending = true;
        self.branches.loading = true;
        self.branches.error = None;
    }

    fn load_refs(&mut self) {
        if self.busy()
            || self.history.history_pending
            || self.branches.worker.is_some()
            || !self.branches.pending
        {
            return;
        }
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let path = repository.path.clone();
        self.branches.pending = false;
        let (sender, receiver) = mpsc::channel();
        let git_context = crate::git::GitContext::new(self.preferences.git_executable.clone());
        match std::thread::Builder::new()
            .name("sannygit-refs".into())
            .spawn(move || {
                let git = &git_context;
                let _ = sender.send(WorkerResult::Refs {
                    result: crate::branches::load(git, &PathBuf::from(&path)),
                    path,
                });
            }) {
            Ok(_) => self.branches.worker = Some(receiver),
            Err(error) => self.ref_read_failed(format!("Cannot start background worker: {error}")),
        }
    }

    fn ref_read_failed(&mut self, error: String) {
        self.branches.loading = false;
        self.branches.current = false;
        self.branches.error = Some(error);
    }

    pub fn copy_commit_message(&mut self, ctx: &eframe::egui::Context, hash: String) {
        if let Some(message) = self
            .history
            .commits
            .iter()
            .chain(self.tag_log.iter().flat_map(|view| view.commits.iter()))
            .find(|commit| commit.hash == hash)
            .map(|commit| commit.message.clone())
        {
            copy_to_clipboard(ctx, message);
            return;
        }
        if !crate::branches::is_commit_id(&hash) {
            return;
        }
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let root = PathBuf::from(&repository.path);
        match self.spawn_reader(Some(ctx.clone()), move |git| {
            crate::branches::read_commit_message(git, &root, &hash)
        }) {
            Ok(reader) => self.branches.commit_message_reader = Some(reader),
            Err(error) => {
                self.branches.commit_message_reader = None;
                self.error = Some(error);
            }
        }
    }

    pub fn copy_tag_message(&mut self, ctx: &eframe::egui::Context, oid: String) {
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let root = PathBuf::from(&repository.path);
        match self.spawn_reader(Some(ctx.clone()), move |git| {
            crate::branches::read_tag_message(git, &root, &oid)
        }) {
            Ok(reader) => self.branches.tag_message_reader = Some(reader),
            Err(error) => {
                self.branches.tag_message_reader = None;
                self.error = Some(error);
            }
        }
    }

    fn poll_tag_message(&mut self, ctx: &eframe::egui::Context) {
        if let Some(result) = Self::poll_reader(&mut self.branches.tag_message_reader) {
            self.finish_copied_text(ctx, result);
        }
        if let Some(result) = Self::poll_reader(&mut self.branches.commit_message_reader) {
            self.finish_copied_text(ctx, result);
        }
    }

    fn finish_copied_text(
        &mut self,
        ctx: &eframe::egui::Context,
        result: Result<Result<String, String>, String>,
    ) {
        match result {
            Ok(Ok(message)) => copy_to_clipboard(ctx, message),
            Ok(Err(error)) | Err(error) => self.error = Some(error),
        }
    }

    pub fn tag_patch_pending(&self) -> bool {
        self.branches.tag_patch_reader.is_some()
    }

    pub fn format_tag_patch(
        &mut self,
        ctx: &eframe::egui::Context,
        oid: String,
        directory: PathBuf,
    ) {
        if self.tag_patch_pending() {
            return;
        }
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let root = PathBuf::from(&repository.path);
        match self.spawn_reader(Some(ctx.clone()), move |git| {
            crate::branches::format_tag_patch(git, &root, &oid, &directory)
        }) {
            Ok(reader) => {
                self.branches.tag_patch_reader = Some(reader);
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn poll_tag_patch(&mut self) {
        match Self::poll_reader(&mut self.branches.tag_patch_reader) {
            Some(Ok(Ok(path))) => {
                self.status = format!("Patch exported to {}", path.display());
            }
            Some(Ok(Err(error)) | Err(error)) => {
                self.status = "Patch export failed".into();
                self.error = Some(error);
            }
            None => {}
        }
    }

    fn poll_refs(&mut self) {
        let Some(receiver) = &self.branches.worker else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.branches.worker = None;
                self.ref_read_failed("Background worker stopped unexpectedly".into());
                return;
            }
        };
        self.branches.worker = None;
        self.apply_worker_result(result);
    }

    fn branch_base_blocker(&self) -> Option<&'static str> {
        if self.busy() {
            Some("Wait for the current operation to finish")
        } else if self.repositories.get(self.selected_repository).is_none() {
            Some("Open a repository first")
        } else if !self.state_current || !self.branches.current {
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
            .branches
            .entries
            .iter()
            .find(|r| r.full_name == self.branches.selected_branch)
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
        None
    }

    fn rename_branch_target_blocker(&self, source: &str) -> Option<&'static str> {
        self.branch_base_blocker().or_else(|| {
            (!self
                .branches
                .entries
                .iter()
                .any(|r| r.full_name == source && r.kind == crate::models::RefKind::Local))
            .then_some("Select an existing local branch")
        })
    }

    pub fn open_rename_branch(&mut self) {
        if self
            .rename_branch_target_blocker(&self.branches.selected_branch)
            .is_some()
        {
            return;
        }
        self.branches.rename_branch_dialog = Some(RenameBranchDialog {
            source: self.branches.selected_branch.clone(),
            name: self
                .branches
                .selected_branch
                .strip_prefix("refs/heads/")
                .unwrap_or_default()
                .into(),
        });
        self.error = None;
    }

    pub fn rename_branch_blocker(&self) -> Option<&'static str> {
        let dialog = self.branches.rename_branch_dialog.as_ref()?;
        self.rename_branch_target_blocker(&dialog.source)
            .or_else(|| crate::branches::name_hint(&dialog.name))
            .or_else(|| {
                self.branches
                    .entries
                    .iter()
                    .any(|r| r.full_name == format!("refs/heads/{}", dialog.name))
                    .then_some("A local branch with this name already exists")
            })
    }

    pub fn rename_branch(&mut self) {
        if self.rename_branch_blocker().is_some() {
            return;
        }
        let Some(dialog) = &self.branches.rename_branch_dialog else {
            return;
        };
        let source = dialog.source.clone();
        let name = dialog.name.clone();
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        self.state_current = false;
        self.branches.current = false;
        self.error = None;
        self.status = "Renaming branch…".into();
        self.start_background_operation(move |git| {
            let outcome = crate::branches::rename(git, &root, &source, &name);
            WorkerResult::Operation {
                committed: false,
                success: "Branch renamed",
                outcome,
                snapshot: git::load(git, &root),
            }
        });
    }

    pub fn create_tag_blocker(&self) -> Option<&'static str> {
        self.branch_base_blocker()
            .or_else(|| {
                (!self.branches.branch_head_exists)
                    .then_some("Create the first commit before creating a tag")
            })
            .or_else(|| {
                crate::branches::tag_hint(
                    &self.branches.new_tag_name,
                    &self.branches.new_tag_message,
                )
            })
            .or_else(|| {
                self.branches
                    .entries
                    .iter()
                    .any(|r| r.full_name == format!("refs/tags/{}", self.branches.new_tag_name))
                    .then_some("A tag with this name already exists")
            })
    }

    pub fn create_tag(&mut self, push: bool) {
        if self.create_tag_blocker().is_some() {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let name = self.branches.new_tag_name.clone();
        let message = self.branches.new_tag_message.clone();
        self.state_current = false;
        self.branches.current = false;
        self.error = None;
        let cancel = Arc::new(AtomicBool::new(false));
        self.status = if push {
            "Creating and pushing tag…"
        } else {
            "Creating tag…"
        }
        .into();
        if push {
            self.manager.cancel = Some(cancel.clone());
        }
        if self.start_background_operation(move |git| WorkerResult::Operation {
            committed: false,
            success: if push {
                "Tag created and pushed"
            } else {
                "Tag created"
            },
            outcome: if push {
                crate::remotes::create_and_push_tag(git, &root, &name, &message, &cancel)
            } else {
                crate::branches::create_tag(git, &root, &name, &message)
            },
            snapshot: git::load(git, &root),
        }) {
            self.branches.tag_dialog_open = false;
        } else if push {
            self.manager.cancel = None;
        }
    }

    pub fn create_branch_blocker(&self) -> Option<&'static str> {
        if let Some(reason) = self.branch_base_blocker() {
            return Some(reason);
        }
        if !self.branches.branch_head_exists {
            return Some("Create the first commit before creating another branch");
        }
        if let Some(reason) = crate::branches::name_hint(&self.branches.new_branch_name) {
            return Some(reason);
        }
        if self
            .branches
            .entries
            .iter()
            .any(|r| r.full_name == format!("refs/heads/{}", self.branches.new_branch_name))
        {
            return Some("A local branch with this name already exists");
        }
        None
    }

    pub fn switch_branch(&mut self) {
        if self.switch_branch_blocker().is_some() {
            return;
        }
        if let Some(remote) = self.branches.entries.iter().find(|r| {
            r.full_name == self.branches.selected_branch && r.kind == crate::models::RefKind::Remote
        }) {
            if let Some(local) = crate::branches::local_for_remote(&self.branches.entries, remote) {
                let current = local.current;
                self.branches.selected_branch = local.full_name.clone();
                if current {
                    self.status = "Local branch already checked out".into();
                    return;
                }
            } else {
                self.branches.remote_checkout = Some(RemoteCheckoutDialog {
                    remote: remote.full_name.clone(),
                    mode: 1,
                    local: self
                        .branches
                        .entries
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
        let target = self.branches.selected_branch.clone();
        self.state_current = false;
        self.branches.current = false;
        self.error = None;
        self.status = "Switching branch or tag…".into();
        self.start_worker(move |git| {
            let outcome = crate::branches::switch(git, &root, &target);
            let snapshot = git::load(git, &root);
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
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        self.state_current = false;
        self.branches.current = false;
        self.error = None;
        self.status = "Checking out remote branch…".into();
        self.start_background_operation(move |git| {
            let outcome = crate::branches::checkout_remote(git, &root, &remote, &choice);
            WorkerResult::Operation {
                committed: false,
                success: "Checkout complete",
                outcome,
                snapshot: git::load(git, &root),
            }
        })
    }

    pub fn create_branch(&mut self) {
        if self.create_branch_blocker().is_some() {
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        let name = self.branches.new_branch_name.clone();
        let switch_to = self.branches.switch_to_new_branch;
        self.state_current = false;
        self.branches.current = false;
        self.error = None;
        self.status = "Creating branch…".into();
        let start = self.branches.create_from_commit.take();
        self.start_background_operation(move |git| {
            let outcome = crate::branches::create(git, &root, &name, switch_to, start.as_deref());
            let snapshot = git::load(git, &root);
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
        self.history.commit_preview_worker = None;
        self.history.commit_preview = None;
        self.history.commit_preview_error = None;
        self.history.loaded_commit = None;
        self.history.commit_files = None;
        self.history.selected_commit_file = None;
        self.history.loaded_commit_file = None;
        self.history.comparing_with = None;
        self.history.commit_preview_generation =
            self.history.commit_preview_generation.wrapping_add(1);
    }

    pub(crate) fn load_commit_preview(&mut self) {
        if self.busy()
            || self.history.commit_preview_worker.is_some()
            || !self.history.commit_details_open
            || self.history.comparing_with.is_some()
        {
            return;
        }
        self.load_commit_preview_inner();
    }

    pub fn select_commit(&mut self, hash: String) {
        if !self.history.commits.iter().any(|c| c.hash == hash) {
            return;
        }
        if self.history.selected_commit.as_ref() != Some(&hash) {
            self.clear_commit_preview();
        }
        self.history.selected_commit = Some(hash);
        self.history.commit_details_open = true;
    }

    pub fn select_commit_file(&mut self, path: String) {
        if !self
            .history
            .commit_files
            .as_ref()
            .is_some_and(|files| files.iter().any(|f| f.path == path))
            || self.history.selected_commit_file.as_ref() == Some(&path)
        {
            return;
        }
        self.history.selected_commit_file = Some(path);
        self.history.commit_preview_worker = None;
        self.history.commit_preview = None;
        self.history.commit_preview_error = None;
        self.history.loaded_commit_file = None;
        self.history.commit_preview_generation =
            self.history.commit_preview_generation.wrapping_add(1);
    }

    pub fn retry_commit_preview(&mut self) {
        if self.busy() {
            return;
        }
        self.history.commit_preview_worker = None;
        if self.history.commit_files.is_none() {
            self.history.loaded_commit = None;
        }
        self.history.loaded_commit_file = None;
        self.history.commit_preview_error = None;
        self.history.commit_preview_generation =
            self.history.commit_preview_generation.wrapping_add(1);
    }

    fn load_commit_preview_inner(&mut self) {
        let Some(hash) = self.history.selected_commit.clone() else {
            return;
        };
        let Some(commit) = self.history.commits.iter().find(|c| c.hash == hash) else {
            return;
        };
        let parent = commit.parents.first().cloned();
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let path = repository.path.clone();
        let root = PathBuf::from(&path);
        let generation = self.history.commit_preview_generation;
        if self.history.loaded_commit.as_ref() != Some(&hash) {
            self.history.loaded_commit = Some(hash.clone());
            match self.spawn_reader(None, move |git| WorkerResult::CommitFiles {
                result: crate::history::files(git, &root, &hash, parent.as_deref()),
                path,
                hash,
                generation,
            }) {
                Ok(receiver) => self.history.commit_preview_worker = Some(receiver),
                Err(error) => self.history.commit_preview_error = Some(error),
            }
            return;
        }
        let Some(file) = self
            .history
            .commit_files
            .as_ref()
            .and_then(|files| {
                files
                    .iter()
                    .find(|file| Some(&file.path) == self.history.selected_commit_file.as_ref())
            })
            .cloned()
        else {
            return;
        };
        if self.history.loaded_commit_file.as_ref() == Some(&file.path) {
            return;
        }
        self.history.loaded_commit_file = Some(file.path.clone());
        match self.spawn_reader(None, move |git| WorkerResult::CommitPatch {
            result: crate::history::file_patch(git, &root, &hash, parent.as_deref(), &file)
                .map(|patch| crate::models::DiffPreview::new(&patch)),
            path,
            file: file.path,
            hash,
            generation,
        }) {
            Ok(receiver) => self.history.commit_preview_worker = Some(receiver),
            Err(error) => self.history.commit_preview_error = Some(error),
        }
    }

    pub fn staging_allowed(&self, index: usize, staged: bool) -> bool {
        !self.busy()
            && self.state_current
            && self.repositories.get(self.selected_repository).is_some()
            && self.changes.get(index).is_some_and(|c| {
                if c.kind == crate::models::ChangeKind::Conflicted {
                    self.merging && staged
                } else if staged {
                    c.unstaged
                } else {
                    !self.merging && c.staged
                }
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
        let selected = self
            .changes
            .get(self.selection.active)
            .map(|c| c.path.clone());
        let ignore_whitespace = self.preferences.diff_ignore_whitespace;
        let show_ignored = self.preferences.show_ignored_files;
        self.start_index_worker(move |git| {
            let outcome = git::set_staged(git, &root, &path, staged);
            WorkerResult::Index {
                staged,
                outcome,
                update: working_tree::Update::read(
                    git,
                    root.to_string_lossy().into_owned(),
                    selected,
                    ignore_whitespace,
                    show_ignored,
                    false,
                ),
            }
        });
    }

    pub fn amend_blocker(&self) -> Option<&'static str> {
        if self.busy() {
            Some("Wait for the current operation to finish")
        } else if self.repositories.get(self.selected_repository).is_none() {
            Some("Open a repository first")
        } else if self.commit.amend {
            None
        } else if !self.state_current || !self.branches.current {
            Some("Refresh the repository before amending")
        } else if self.merging {
            Some("Finish the merge before amending")
        } else if !self.branches.branch_head_exists {
            Some("There is no commit to amend")
        } else {
            None
        }
    }

    pub fn set_amend(&mut self, enabled: bool) {
        if self.amend_blocker().is_some() || enabled == self.commit.amend {
            return;
        }
        if !enabled {
            self.commit.cancel_amend();
            return;
        }
        let root = PathBuf::from(&self.repositories[self.selected_repository].path);
        self.error = None;
        self.status = "Loading last commit message…".into();
        self.start_worker(move |git| WorkerResult::AmendMessage(git::amend_message(git, &root)));
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
        } else if self.commit.amend && self.commit.amend_head.is_none() {
            Some("Load the last commit before amending")
        } else if self.merging && self.commit.amend {
            Some("Turn off Amend to finish the merge")
        } else if self.changes.is_empty() && !self.commit.amend && !self.merging {
            Some("There are no changes to commit")
        } else if self.commit.contents == git::CommitContents::Staged
            && !self.commit.amend
            && !self.merging
            && !self.changes.iter().any(|c| c.staged)
        {
            Some("There are no staged changes to commit")
        } else if self.commit.message.trim().is_empty() {
            Some("Enter a commit message")
        } else if self.commit.message.contains('\0') || self.commit.message.len() > 65_536 {
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
        let message = self.commit.message.clone();
        let contents = self.commit.contents;
        self.state_current = false;
        self.error = None;
        let amend_head = self.commit.amend_head.clone().filter(|_| self.commit.amend);
        self.status = if self.commit.amend {
            "Amending last commit…"
        } else {
            "Creating commit…"
        }
        .into();
        self.start_worker(move |git| {
            let outcome = if let Some(head) = &amend_head {
                git::amend_commit(git, &root, &message, head, contents)
            } else {
                git::commit(git, &root, &message, contents)
            };
            let committed = outcome.is_ok();
            let snapshot = git::load(git, &root);
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
        } else if self.commit.amend && self.commit.amend_head.is_none() {
            Some("Load the last commit before generating a message")
        } else if self.commit.contents == git::CommitContents::Staged
            && !self.changes.iter().any(|c| c.staged)
        {
            Some("There are no staged changes to describe")
        } else if self.changes.is_empty() {
            Some("There are no changes to describe")
        } else {
            None
        }
    }

    pub fn effective_llm_config(&self, path: &str) -> &crate::llm::Config {
        self.repository_llm_configs
            .get(path)
            .unwrap_or(&self.llm_config)
    }

    pub fn effective_generation_config(&self, path: &str) -> crate::llm::Config {
        let mut config = self.effective_llm_config(path).clone();
        if let Some(instructions) = self.repository_llm_instructions.get(path) {
            config.instructions.clone_from(instructions);
        }
        config.chatgpt_subscription = self.llm_config.chatgpt_subscription;
        if config.chatgpt_subscription {
            if config.subscription_models.is_empty() {
                config
                    .subscription_models
                    .clone_from(&self.llm_config.subscription_models);
            }
        } else {
            config.subscription_models.clear();
        }
        config
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
        let mut config = self.effective_generation_config(&path);
        let endpoint = (!config.chatgpt_subscription)
            .then(|| crate::llm::request_endpoint(&config))
            .transpose();
        if let Err(error) = endpoint {
            self.error = Some(error);
            return;
        }
        let contents = self.commit.contents;
        let head = self.commit.amend_head.clone().filter(|_| self.commit.amend);
        self.error = None;
        self.status = "Generating commit message…".into();
        let cancel = Arc::new(AtomicBool::new(false));
        self.commit.generation_cancel = Some(cancel.clone());
        self.commit.generating_message = self.start_worker(move |git| {
            let result =
                crate::llm::load_api_key_if_needed(&mut config, repository_scope.as_deref())
                    .and_then(|()| config.validate())
                    .and_then(|()| git::commit_patch(git, &root, contents, head.as_deref()))
                    .and_then(git::limit_diff)
                    .and_then(|patch| {
                        if patch.trim().is_empty() {
                            Err("The commit diff is empty".into())
                        } else if patch.len() > crate::llm::MAX_DIFF_BYTES {
                            Err("The commit diff exceeds the LLM request limit".into())
                        } else {
                            crate::llm::generate(&config, &patch, &cancel)
                        }
                    });
            WorkerResult::CommitMessage {
                result,
                config,
                repository: repository_scope,
            }
        });
        if !self.commit.generating_message {
            self.commit.generation_cancel = None;
        }
    }

    pub fn change_chatgpt_connection(&mut self, ctx: &eframe::egui::Context, sign_in: bool) {
        let (status, job): (_, fn() -> Result<String, String>) = if sign_in {
            (
                "Complete ChatGPT sign-in in your browser",
                crate::llm::chatgpt::sign_in,
            )
        } else {
            ("Disconnecting ChatGPT…", crate::llm::chatgpt::sign_out)
        };
        if self.busy() {
            return;
        }
        let ctx = ctx.clone();
        if self.start_worker(move |_| {
            let result = job();
            ctx.request_repaint();
            WorkerResult::AiConnection { sign_in, result }
        }) {
            self.settings_window.chatgpt_status_reader = None;
            self.settings_window.chatgpt_models.clear();
            self.settings_window.chatgpt_models_reader = None;
            self.settings_window.chatgpt_models_attempted = false;
            self.settings_window.chatgpt_status = ChatGptStatus::ChangingSession;
            self.settings_window.open = false;
            self.settings_window.resume_after_ai_action = true;
            self.status = status.into();
        }
    }

    fn update_chatgpt_status(&mut self, ctx: &eframe::egui::Context) {
        let result = match self.settings_window.chatgpt_status {
            ChatGptStatus::Unknown => {
                match self.spawn_reader(Some(ctx.clone()), |_| {
                    crate::llm::chatgpt::has_saved_session()
                }) {
                    Ok(reader) => {
                        self.settings_window.chatgpt_status_reader = Some(reader);
                        self.settings_window.chatgpt_status = ChatGptStatus::Checking;
                        return;
                    }
                    Err(error) => Err(error),
                }
            }
            ChatGptStatus::Checking => {
                let Some(reader) = &self.settings_window.chatgpt_status_reader else {
                    return;
                };
                match reader.try_recv() {
                    Ok(result) => result,
                    Err(mpsc::TryRecvError::Empty) => return,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Err("Could not check the saved ChatGPT session".into())
                    }
                }
            }
            _ => return,
        };
        self.settings_window.chatgpt_status_reader = None;
        self.settings_window.chatgpt_status = match result {
            Ok(true) => ChatGptStatus::Connected,
            Ok(false) => ChatGptStatus::SignedOut,
            Err(error) => {
                self.error = Some(error);
                ChatGptStatus::Failed
            }
        };
    }

    pub fn update_chatgpt_models(&mut self, ctx: &eframe::egui::Context) {
        self.update_chatgpt_status(ctx);
        if self.settings_window.chatgpt_status != ChatGptStatus::Connected {
            return;
        }
        if let Some(reader) = &self.settings_window.chatgpt_models_reader {
            let result = match reader.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err("Could not load ChatGPT models".into())
                }
            };
            self.settings_window.chatgpt_models_reader = None;
            match result {
                Ok(models) => self.settings_window.chatgpt_models = models,
                Err(error) => self.error = Some(error),
            }
        } else if !self.settings_window.chatgpt_models_attempted {
            self.settings_window.chatgpt_models_attempted = true;
            match self.spawn_reader(Some(ctx.clone()), |_| crate::llm::chatgpt::list_models()) {
                Ok(reader) => self.settings_window.chatgpt_models_reader = Some(reader),
                Err(error) => self.error = Some(error),
            }
        }
    }

    pub fn cancel_commit_message_generation(&mut self) {
        if !self.commit.generating_message {
            return;
        }
        if let Some(cancel) = self.commit.generation_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        // Discard the receiver as well: a completed or late response from this
        // generation must never overwrite the draft or a subsequent operation.
        self.worker = None;
        self.commit.generating_message = false;
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
                if self.settings_window.chatgpt_status == ChatGptStatus::ChangingSession {
                    self.settings_window.chatgpt_status = ChatGptStatus::Unknown;
                }
                self.background_progress_only = false;
                self.commit.generating_message = false;
                self.commit.generation_cancel = None;
                self.manager.cancel = None;
                self.error = Some("Background worker stopped unexpectedly".into());
                if self.branches.loading && !self.branches.pending {
                    self.branches.loading = false;
                    self.branches.current = false;
                    self.branches.error = self.error.clone();
                }
                if self.history.history_loading && !self.history.history_pending {
                    self.history.history_loading = false;
                    self.history.history_session.cancel = None;
                    self.history.history_error = self.error.clone();
                }
                if self.history.commit_details_open
                    && self.history.loaded_commit.is_some()
                    && self.history.commit_preview.is_none()
                {
                    self.history.commit_preview_error = self.error.clone();
                }
                self.status = "Operation failed".into();
                self.finish_discard(false);
                if self
                    .conflict_dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.review.is_none())
                {
                    self.conflict_dialog = None;
                }
                self.ref_worker_failed();
                if self.remote_confirmation_hidden() {
                    self.finish_hidden_remote();
                }
                return;
            }
        };
        self.worker = None;
        self.background_progress_only = false;
        self.apply_worker_result(result);
    }

    fn apply_worker_result(&mut self, result: WorkerResult) {
        match result {
            WorkerResult::ConflictReview(result) => self.finish_conflict_review(result),
            WorkerResult::Index {
                staged,
                outcome,
                update,
            } => {
                if outcome.is_ok() {
                    let event = if staged {
                        "Index staged"
                    } else {
                        "Index unstaged"
                    };
                    logging::record(logging::Level::Info, format_args!("{event}"));
                }
                self.apply_index_update(update, staged, outcome);
            }
            WorkerResult::DiscardBatchPrepared {
                root,
                paths,
                result,
            } => self.apply_discard_batch(root, paths, result),
            WorkerResult::MergeDetails { root, result } => self.apply_merge_details(root, result),
            WorkerResult::RevertMergeParents(dialog) => self.apply_revert_merge_parents(dialog),
            WorkerResult::CommitCompare {
                path,
                from,
                to,
                generation,
                result,
            } => self.apply_commit_compare(path, from, to, generation, result),
            WorkerResult::PushUpToDetails { root, result } => self.apply_push_up_to(root, result),
            WorkerResult::RefDetails { root, result } => self.apply_ref_details(root, *result),
            WorkerResult::TagPushSelection {
                root,
                source,
                preferred_remote,
            } => self.apply_tag_push_selection(root, source, preferred_remote),
            WorkerResult::DiscardPrepared { root, path, result } => {
                self.apply_discard_prepared(root, path, result)
            }
            WorkerResult::Stash { path, result } => self.apply_stash_result(path, *result),
            WorkerResult::RemoteInfo { operation, result } => {
                self.apply_remote_info(operation, result)
            }
            WorkerResult::RepositoryTask(result) => self.apply_repository_result(*result),
            WorkerResult::Refs { path, result } => self.apply_refs(path, result),
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
            } => self.apply_commit_files(path, hash, generation, result),
            WorkerResult::CommitPatch {
                path,
                file,
                hash,
                generation,
                result,
            } => self.apply_commit_patch(path, file, hash, generation, result),
            WorkerResult::AmendMessage(result) => match result {
                Ok((head, message)) => {
                    self.commit.begin_amend(head, message);
                    self.status = "Ready to amend last commit".into();
                }
                Err(error) => {
                    self.error = Some(error);
                    self.status = "Could not load last commit".into();
                }
            },
            WorkerResult::CommitMessage {
                result,
                config,
                repository,
            } => {
                self.commit.generating_message = false;
                self.commit.generation_cancel = None;
                let current = match repository.as_deref() {
                    Some(path) => self.repository_llm_configs.get_mut(path),
                    None => Some(&mut self.llm_config),
                };
                if let Some(current) = current {
                    // Keep an unlocked key for this session, including after network
                    // errors. Never overwrite a newer key or a different endpoint.
                    if current.endpoint == config.endpoint
                        && current.store_api_key
                        && current.api_key.is_empty()
                        && !config.api_key.is_empty()
                    {
                        current.api_key = config.api_key;
                        if current.model.trim().is_empty() {
                            current.model = config.model;
                        }
                    }
                }
                match result {
                    Ok(message) => {
                        logging::record(
                            logging::Level::Info,
                            format_args!("Commit message generated"),
                        );
                        self.commit.message = message;
                        self.status = "Commit message generated".into();
                    }
                    Err(error) => {
                        self.error = Some(error);
                        self.status = "Could not generate commit message".into();
                    }
                }
            }
            WorkerResult::AiConnection { sign_in, result } => match result {
                Ok(status) => {
                    self.settings_window.chatgpt_status = if sign_in {
                        ChatGptStatus::Connected
                    } else {
                        ChatGptStatus::SignedOut
                    };
                    self.status = status;
                }
                Err(error) => {
                    self.settings_window.chatgpt_status = ChatGptStatus::Unknown;
                    self.error = Some(error);
                    self.status = "AI connection failed".into();
                }
            },
            WorkerResult::Repository(Ok(snapshot)) => {
                logging::record(logging::Level::Info, format_args!("Repository opened"));
                self.apply_snapshot(snapshot);
                self.history.reveal_head_on_reload();
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
                    "Branch deleted"
                        | "Tag deleted"
                        | "Tag pushed"
                        | "Remote branch pushed"
                        | "Remote branch deleted"
                        | "Local branch pushed"
                        | "Commits pushed"
                        | "Branch tracking updated"
                        | "Branch tracking stopped"
                        | "Reset complete"
                        | "Remote configuration updated"
                );
                let discarding = matches!(
                    success,
                    "File restored from index"
                        | "Untracked file deleted"
                        | "Files discarded"
                        | "Files removed"
                        | "Merge changes discarded"
                );
                let operation_succeeded = outcome.is_ok();
                if operation_succeeded {
                    logging::record(logging::Level::Info, format_args!("{success}"));
                }
                if (matches!(
                    success,
                    "Merge complete"
                        | "Merge started. Resolve conflicts in the working tree"
                        | "Merge ready to review and commit"
                        | "Rebase complete"
                )) && operation_succeeded
                {
                    self.merge_dialog = None;
                }
                if operation_succeeded
                    && success == "File staged"
                    && self.preferences.staged_only_after_staging
                {
                    self.commit.contents = git::CommitContents::Staged;
                }
                if operation_succeeded && success.starts_with("Stash ") {
                    self.stash_dialog = None;
                } else if success.starts_with("Stash ") {
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
                    if let Some(dialog) = self.branches.rename_branch_dialog.take() {
                        self.branches.selected_branch = format!("refs/heads/{}", dialog.name);
                        self.branches.selected_ref_group = None;
                    }
                }
                if success == "Tag created and pushed" {
                    self.manager.cancel = None;
                }
                if outcome.is_ok() && matches!(success, "Tag created" | "Tag created and pushed") {
                    self.branches.selected_branch =
                        format!("refs/tags/{}", self.branches.new_tag_name);
                    self.branches.selected_ref_group = None;
                    self.branches.tag_dialog_open = false;
                    self.branches.new_tag_name.clear();
                    self.branches.new_tag_message.clear();
                }
                if committed {
                    self.commit.finish_commit();
                    self.remember_commit_draft();
                }
                if outcome.is_ok() && success.starts_with("Branch created") {
                    self.branches.branch_dialog_open = false;
                    self.branches.new_branch_name.clear();
                }
                if outcome.is_ok() && success == "Checkout complete" {
                    self.branches.remote_checkout = None;
                }
                let refresh_error = match snapshot {
                    Ok(snapshot) => {
                        self.apply_snapshot(snapshot);
                        if operation_succeeded {
                            self.history.reveal_head_on_reload();
                        }
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
                if operation_succeeded && success == ".gitignore updated" {
                    self.ignore_dialog = None;
                }
                if discarding {
                    self.finish_discard(operation_succeeded);
                }
                if operation_succeeded
                    && matches!(success, "Conflict resolved" | "Conflict recreated")
                {
                    self.conflict_dialog = None;
                }
                if managing_refs {
                    self.finish_ref_operation(success, operation_succeeded);
                }
                if hidden_remote {
                    self.network_dialog = None;
                }
            }
            WorkerResult::Diff { index, result } => self.apply_selected_diff(index, result),
        }
    }

    fn load_selected_diff(&mut self, context: &eframe::egui::Context) {
        if self.busy()
            || (!self.diff_refresh_pending && self.loaded_diff == Some(self.selection.active))
        {
            return;
        }
        let Some(change) = self.changes.get(self.selection.active) else {
            return;
        };
        let Some(repository) = self.repositories.get(self.selected_repository) else {
            return;
        };
        let root = PathBuf::from(&repository.path);
        let path = change.path.clone();
        let kind = change.kind;
        let original_path = change.original_path.clone();
        let index = self.selection.active;
        let retain_preview = self.selected_diff().is_some();
        self.diff_refresh_pending = false;
        if !retain_preview {
            self.diff_preview = None;
            if let Some(previous) = self.loaded_diff.and_then(|i| self.changes.get_mut(i)) {
                previous.diff = String::new();
            }
        }
        self.loaded_diff = Some(index);
        self.diff_worker = None;
        if !retain_preview
            && self.error.is_none()
            && !self.status.starts_with("Push ")
            && !self.status.starts_with("Stash ")
            && !matches!(
                self.status.as_str(),
                "Commit created"
                    | "Pull complete"
                    | "Index updated"
                    | "File staged"
                    | "File restored from index"
                    | "Untracked file deleted"
                    | "Branch deleted"
                    | "Tag deleted"
                    | "Reset complete"
                    | "Tag pushed"
                    | "Remote branch pushed"
                    | "Remote branch deleted"
                    | "Local branch pushed"
                    | "Branch tracking updated"
                    | "Branch tracking stopped"
                    | "Remote configuration updated"
                    | "Branch switched"
                    | "Branch renamed"
                    | "Branch created"
                    | "Branch created and switched"
                    | "Hunk staged"
                    | "Lines staged"
                    | "Hunk unstaged"
                    | "Lines unstaged"
                    | "Hunk discarded"
                    | "Lines discarded"
                    | "File renamed"
            )
        {
            self.status = "Loading diff…".into();
        }
        let ignore_whitespace = self.preferences.diff_ignore_whitespace;
        match self.spawn_reader(Some(context.clone()), move |git| WorkerResult::Diff {
            index,
            result: git::file_diff(
                git,
                &root,
                &path,
                kind,
                original_path.as_deref(),
                ignore_whitespace,
            )
            .map(|patch| {
                let preview = crate::models::DiffPreview::new(&patch);
                (patch, preview)
            }),
        }) {
            Ok(receiver) => self.diff_worker = Some(receiver),
            Err(error) => self.error = Some(error),
        }
    }
}

impl eframe::App for SannyGitApp {
    fn clear_color(&self, _visuals: &eframe::egui::Visuals) -> [f32; 4] {
        self.palette().background.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "windows")]
        if let Err(error) = crate::native_window::finish_first_presentation(ui.ctx()) {
            self.error = Some(error.to_string());
        }
        self.llm_models.update();
        if self.preferences.follow_system_theme {
            self.theme = ThemeChoice::from_system(ui.ctx(), self.theme);
        }
        if self.applied_interface_style != self.interface_style {
            self.interface_style.apply(ui.ctx());
            self.applied_interface_style = self.interface_style;
        }
        self.theme
            .select(ui.ctx(), self.preferences.follow_system_theme);
        ui.set_style(ui.ctx().global_style());
        self.poll_worker();
        self.poll_refs();
        self.poll_tag_message(ui.ctx());
        self.poll_tag_patch();
        self.poll_view_reads();
        self.finish_startup();
        self.poll_working_tree();
        self.update_repository_hosting(ui.ctx());
        self.update_worktree_availability(ui.ctx());
        let root = self
            .repositories
            .get(self.selected_repository)
            .map(|r| PathBuf::from(&r.path));
        let allow_tracking = !self.busy()
            && !self.branches.pending
            && !self.branches.loading
            && !self.history.history_pending
            && !self.history.history_loading
            && self.state_current
            && !self.open_dialog
            && !self.welcome_open
            && self.manager.dialog.is_none()
            && self.network_dialog.is_none()
            && self.stash_dialog.is_none()
            && self.discard_dialog.is_none()
            && self.diff_discard.is_none()
            && self.rename_dialog.is_none()
            && self.ignore_dialog.is_none()
            && self.branches.ref_dialog.is_none()
            && self.merge_dialog.is_none()
            && self.push_up_to.is_none()
            && self.conflict_dialog.is_none()
            && self.abort_merge_dialog.is_none()
            && self.branches.remote_checkout.is_none();
        self.tracking.update(
            root,
            allow_tracking,
            &self.preferences.git_executable,
            ui.ctx(),
        );
        if self.tracking.refs_changed {
            self.tracking.refs_changed = false;
            self.branches.pending = true;
            self.branches.loading = true;
            self.branches.current = false;
            if self.history.history_all_branches {
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
            // The OS preferences window needs the edit events forwarded; the
            // in-app repository LLM dialog reads them from the main viewport.
            if self.settings_window.open && self.settings_window.llm_repository.is_none() {
                self.pending_native_edit_events.extend(edit_events);
            } else {
                ui.input_mut(|input| input.events.extend(edit_events));
            }
            for action in actions {
                self.handle_menu_action(action, ui.ctx());
            }
        }
        #[cfg(not(target_os = "macos"))]
        self.handle_shortcuts(ui.ctx());
        ui::handle_branch_shortcuts(ui.ctx(), self);
        ui::show(ui, self);
        self.load_ref_pending();
        self.load_pending_stashes();
        self.load_selected_diff(ui.ctx());
        self.load_refs();
        self.load_history();
        self.load_commit_preview();
        if self.tag_log.is_some() {
            self.with_history_window(|app| {
                app.load_history();
                app.load_commit_preview();
            });
        }
        self.check_working_tree(ui.ctx());
        #[cfg(target_os = "macos")]
        if let Some(menu) = &self.native_menu {
            menu.update_enabled(|action| self.menu_action_enabled(action));
        }
        if self.busy()
            || self.branches.worker.is_some()
            || self.history.history_worker.is_some()
            || self.diff_worker.is_some()
            || self.history.commit_preview_worker.is_some()
            || self.tag_log.as_ref().is_some_and(|view| {
                view.history_worker.is_some() || view.commit_preview_worker.is_some()
            })
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn on_exit(
        &mut self,
        #[cfg(any(target_os = "linux", target_os = "windows"))] _gl: Option<&eframe::glow::Context>,
    ) {
        #[cfg(target_os = "linux")]
        crate::native_window::shutdown();
        self.close_tag_log();
        if let Some(cancel) = &self.commit.generation_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(cancel) = &self.history.history_session.cancel {
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
        eframe::set_value(
            storage,
            "sannygit.commit_drafts",
            &self.commit.commit_drafts,
        );
        self.llm_models.update();
        eframe::set_value(storage, llm_models::STORAGE_KEY, &self.llm_models);
        eframe::set_value(
            storage,
            "sannygit.skip_push_confirmation",
            &self.skip_push_confirmation,
        );
        eframe::set_value(storage, "sannygit.auto_fetch", &self.tracking.auto_fetch);
        eframe::set_value(
            storage,
            "sannygit.reset_strategy",
            &self.branches.last_reset_strategy,
        );
        eframe::set_value(storage, "sannygit.llm", &self.llm_config);
        eframe::set_value(
            storage,
            "sannygit.repository_llm_instructions",
            &self.repository_llm_instructions,
        );
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
