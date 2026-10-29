use crate::{
    mock_data,
    models::{ChangeFile, Commit, Repository},
    theme::{ThemeChoice, THEME_STORAGE_KEY},
    ui,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeFilter {
    All,
    Modified,
    Added,
    Deleted,
}

pub struct SannyGitApp {
    pub theme: ThemeChoice,
    applied_theme: ThemeChoice,
    pub repositories: Vec<Repository>,
    pub local_branches: Vec<&'static str>,
    pub remote_branches: Vec<&'static str>,
    pub tags: Vec<&'static str>,
    pub changes: Vec<ChangeFile>,
    pub commits: Vec<Commit>,
    pub selected_repository: usize,
    pub selected_change: usize,
    pub selected_branch: String,
    pub branch_filter: String,
    pub change_filter: ChangeFilter,
    pub journal_filter: String,
    pub current_branch: &'static str,
    pub commit_message: String,
    pub amend: bool,
    pub push_after_commit: bool,
    pub ahead: u32,
    pub behind: u32,
}

impl SannyGitApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&creation_context.egui_ctx);
        let system_theme = if creation_context.egui_ctx.style().visuals.dark_mode {
            ThemeChoice::Dark
        } else {
            ThemeChoice::Light
        };
        let theme = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, THEME_STORAGE_KEY))
            .unwrap_or(system_theme);
        theme.apply(&creation_context.egui_ctx);

        Self {
            theme,
            applied_theme: theme,
            repositories: mock_data::repositories(),
            local_branches: mock_data::local_branches(),
            remote_branches: mock_data::remote_branches(),
            tags: mock_data::tags(),
            changes: mock_data::changes(),
            commits: mock_data::commits(),
            selected_repository: 0,
            selected_change: 3,
            selected_branch: "main".to_owned(),
            branch_filter: String::new(),
            change_filter: ChangeFilter::All,
            journal_filter: String::new(),
            current_branch: "main",
            commit_message: String::new(),
            amend: false,
            push_after_commit: false,
            ahead: 2,
            behind: 0,
        }
    }
}

impl eframe::App for SannyGitApp {
    fn update(&mut self, context: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        if self.applied_theme != self.theme {
            self.theme.apply(context);
            self.applied_theme = self.theme;
        }
        ui::show(context, self);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, THEME_STORAGE_KEY, &self.theme);
    }
}
