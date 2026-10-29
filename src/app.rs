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
        crate::theme::install_fonts(&creation_context.egui_ctx);
        egui_extras::install_image_loaders(&creation_context.egui_ctx);
        let system_theme = if creation_context.egui_ctx.global_style().visuals.dark_mode {
            ThemeChoice::Dark
        } else {
            ThemeChoice::Light
        };
        let theme = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, THEME_STORAGE_KEY))
            .unwrap_or(system_theme);
        theme.apply(&creation_context.egui_ctx);

        Self::with_theme(theme)
    }

    fn with_theme(theme: ThemeChoice) -> Self {
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
            selected_change: 2,
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
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        if self.applied_theme != self.theme {
            self.theme.apply(ui.ctx());
            self.applied_theme = self.theme;
        }
        ui::show(ui, self);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, THEME_STORAGE_KEY, &self.theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{self, epaint::ClippedShape, Rect, Vec2};

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
                        visible_text(&output.shapes, file.path),
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
            }
        }
    }
}
