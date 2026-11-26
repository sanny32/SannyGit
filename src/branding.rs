//! Product strings referenced across the UI.

use crate::models::Repository;
use eframe::egui::{self, ViewportCommand};

pub const APP_NAME: &str = "SannyGit";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PREFERENCES_WINDOW_TITLE: &str = "SannyGit Preferences";
pub const SIGN_IN_CHATGPT: &str = "Sign in to ChatGPT in your browser. SannyGit uses your account and sends the selected diff when you generate a message.";
pub const SIGN_IN_COPILOT: &str = "Sign in to GitHub in your browser. SannyGit sends the selected diff to Copilot when you generate a message.";

/// Main window title: `name - [path] - SannyGit version` (SmartGit layout).
pub fn main_window_title(repository: Option<&Repository>) -> String {
    let app = format!("{APP_NAME} {APP_VERSION}");
    match repository {
        Some(repository) => format!("{} - [{}] - {}", repository.name, repository.path, app),
        None => app,
    }
}

pub fn sync_main_window_title(ctx: &egui::Context, repository: Option<&Repository>) {
    let title = main_window_title(repository);
    let id = egui::Id::unique("sannygit.main_window_title");
    let previous: Option<String> = ctx.data(|data| data.get_temp(id));
    if previous.as_deref() == Some(title.as_str()) {
        return;
    }
    ctx.data_mut(|data| data.insert_temp(id, title.clone()));
    ctx.send_viewport_cmd(ViewportCommand::Title(title));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_window_title_matches_smartgit_layout() {
        let repository = Repository {
            name: "granata".into(),
            path: r"D:\Work\granata".into(),
        };
        assert_eq!(
            main_window_title(Some(&repository)),
            format!("granata - [D:\\Work\\granata] - {APP_NAME} {APP_VERSION}")
        );
    }

    #[test]
    fn main_window_title_without_repository() {
        assert_eq!(
            main_window_title(None),
            format!("{APP_NAME} {APP_VERSION}")
        );
    }
}
