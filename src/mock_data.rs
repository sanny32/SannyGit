use crate::models::{ChangeFile, ChangeKind, Commit, Repository};

pub fn repositories() -> Vec<Repository> {
    vec![
        Repository {
            name: "SannyGit",
            path: "D:\\Projects\\SannyGit",
        },
        Repository {
            name: "OpenModScan",
            path: "D:\\Projects\\OpenModScan",
        },
        Repository {
            name: "OpenModSim",
            path: "D:\\Projects\\OpenModSim",
        },
        Repository {
            name: "OpenUaExplorer",
            path: "D:\\Projects\\OpenUaExplorer",
        },
    ]
}

pub fn local_branches() -> Vec<&'static str> {
    vec![
        "main",
        "develop",
        "feature/ui-redesign",
        "feature/diff-view",
        "fix/windows-build",
    ]
}

pub fn remote_branches() -> Vec<&'static str> {
    vec![
        "origin/main",
        "origin/develop",
        "origin/feature/ui-redesign",
        "origin/feature/diff-view",
        "origin/release/1.0",
    ]
}

pub fn tags() -> Vec<&'static str> {
    vec!["v1.0.0", "v0.9.0"]
}

pub fn changes() -> Vec<ChangeFile> {
    vec![
        ChangeFile {
            path: "crates/sannygit/src/main.rs",
            kind: ChangeKind::Modified,
            staged: false,
            diff: MAIN_DIFF,
        },
        ChangeFile {
            path: "crates/sannygit/src/app.rs",
            kind: ChangeKind::Modified,
            staged: false,
            diff: APP_DIFF,
        },
        ChangeFile {
            path: "crates/sannygit/src/ui/diff_view.rs",
            kind: ChangeKind::Modified,
            staged: true,
            diff: DIFF_VIEW_DIFF,
        },
        ChangeFile {
            path: "crates/sannygit/src/ui/commit_panel.rs",
            kind: ChangeKind::Added,
            staged: false,
            diff: COMMIT_PANEL_DIFF,
        },
        ChangeFile {
            path: "crates/sannygit/assets/icons/app_icon.png",
            kind: ChangeKind::Added,
            staged: false,
            diff: "Binary file added: crates/sannygit/assets/icons/app_icon.png",
        },
        ChangeFile {
            path: "crates/sannygit/Cargo.toml",
            kind: ChangeKind::Modified,
            staged: false,
            diff: CARGO_DIFF,
        },
        ChangeFile {
            path: "README.md",
            kind: ChangeKind::Deleted,
            staged: false,
            diff: README_DIFF,
        },
        ChangeFile {
            path: "src/ui/toolbar.rs",
            kind: ChangeKind::Renamed,
            staged: false,
            diff: "@@ -1 +1 @@\n-old toolbar\n+new toolbar",
        },
        ChangeFile {
            path: "src/repository.rs",
            kind: ChangeKind::Untracked,
            staged: false,
            diff: "@@ -0,0 +1 @@\n+pub struct Repository;",
        },
        ChangeFile {
            path: "src/merge.rs",
            kind: ChangeKind::Conflicted,
            staged: false,
            diff: "@@ -1 +1 @@\n-ours\n+theirs",
        },
    ]
}

pub fn commits() -> Vec<Commit> {
    vec![
        Commit {
            message: "Improve diff view highlighting",
            author: "SaNNy",
            date: "2 hours ago",
            hash: "a1b2c3d",
        },
        Commit {
            message: "Add commit panel and shortcuts",
            author: "SaNNy",
            date: "5 hours ago",
            hash: "f4e5d6c",
        },
        Commit {
            message: "Update icons and application name",
            author: "SaNNy",
            date: "1 day ago",
            hash: "9a8b7c6",
        },
        Commit {
            message: "Refactor UI layout",
            author: "SaNNy",
            date: "1 day ago",
            hash: "3c2d1e0",
        },
        Commit {
            message: "Initial egui application",
            author: "SaNNy",
            date: "2 days ago",
            hash: "7f6e5d4",
        },
        Commit {
            message: "Initial commit",
            author: "SaNNy",
            date: "2 days ago",
            hash: "1a2b3c4",
        },
    ]
}

const MAIN_DIFF: &str = r#"@@ -8,6 +8,8 @@ fn main() -> eframe::Result<()> {
     let options = eframe::NativeOptions {
         viewport: egui::ViewportBuilder::default()
             .with_title("SannyGit")
+            .with_inner_size([1440.0, 900.0])
+            .with_min_inner_size([1100.0, 700.0]),
         ..Default::default()
     };
"#;

const APP_DIFF: &str = r#"@@ -20,7 +20,7 @@ impl eframe::App for SannyGitApp {
     fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
-        self.render_toolbar(ctx);
+        ui::toolbar::show(ctx, self);
         self.render_workspace(ctx);
         self.render_status_bar(ctx);
     }
"#;

const DIFF_VIEW_DIFF: &str = r#"@@ -42,7 +42,11 @@ impl DiffView {
     fn show(&mut self, ui: &mut egui::Ui) {
         ui.horizontal(|ui| {
-            ui.label("Diff");
+            ui.label(RichText::new("Diff").strong());
+            ui.separator();
+            ui.checkbox(&mut self.show_whitespace, "Show whitespace");
+            ui.checkbox(&mut self.ignore_whitespace, "Ignore whitespace");
         });
         self.render_diff(ui);
     }
@@ -89,6 +93,10 @@ impl DiffView {
     fn render_diff(&mut self, ui: &mut egui::Ui) {
         if let Some(diff) = &self.current_diff {
+            let theme = self.theme.as_ref().unwrap_or(&DiffTheme::default());
+            let mut job = LayoutJob::default();
+            self.highlight_syntax(diff, &mut job, theme);
+            ui.label(job);
         }
     }
"#;

const COMMIT_PANEL_DIFF: &str = r#"@@ -0,0 +1,8 @@
+pub struct CommitPanel {
+    message: String,
+    amend: bool,
+}
+
+impl CommitPanel {
+    pub fn show(&mut self, ui: &mut egui::Ui) {}
+}
"#;

const CARGO_DIFF: &str = r#"@@ -10,6 +10,7 @@
 [dependencies]
 eframe = "0.31"
+egui_extras = "0.31"
 serde = { version = "1", features = ["derive"] }
"#;

const README_DIFF: &str = r#"@@ -1,3 +0,0 @@
-# SannyGit
-
-A simple, no-nonsense Git client.
"#;
