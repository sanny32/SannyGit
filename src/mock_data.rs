use crate::models::{BranchRef, ChangeFile, ChangeKind, Commit, RefKind, Repository};

pub fn repositories() -> Vec<Repository> {
    vec![
        Repository {
            name: "SannyGit".into(),
            path: "D:\\Projects\\SannyGit".into(),
        },
        Repository {
            name: "OpenModScan".into(),
            path: "D:\\Projects\\OpenModScan".into(),
        },
        Repository {
            name: "OpenModSim".into(),
            path: "D:\\Projects\\OpenModSim".into(),
        },
        Repository {
            name: "OpenUaExplorer".into(),
            path: "D:\\Projects\\OpenUaExplorer".into(),
        },
    ]
}

pub fn refs() -> Vec<BranchRef> {
    [
        (
            RefKind::Local,
            "refs/heads/",
            vec![
                "main",
                "develop",
                "feature/ui-redesign",
                "feature/diff-view",
                "fix/windows-build",
            ],
        ),
        (
            RefKind::Remote,
            "refs/remotes/",
            vec![
                "origin/main",
                "origin/develop",
                "origin/feature/ui-redesign",
                "origin/feature/diff-view",
                "origin/release/1.0",
            ],
        ),
        (RefKind::Tag, "refs/tags/", vec!["v1.0.0", "v0.9.0"]),
    ]
    .into_iter()
    .flat_map(|(kind, prefix, names)| {
        names.into_iter().map(move |name| BranchRef {
            name: name.into(),
            full_name: format!("{prefix}{name}"),
            hash: "a".repeat(40),
            upstream: if kind == RefKind::Local {
                Some(format!("origin/{name}"))
            } else {
                None
            },
            current: kind == RefKind::Local && name == "main",
            kind,
        })
    })
    .collect()
}

pub fn changes() -> Vec<ChangeFile> {
    vec![
        ChangeFile {
            original_path: None,
            path: "crates/sannygit/src/main.rs".into(),
            kind: ChangeKind::Modified,
            unstaged: true,
            staged: false,
            diff: MAIN_DIFF.into(),
        },
        ChangeFile {
            original_path: None,
            path: "crates/sannygit/src/app.rs".into(),
            kind: ChangeKind::Modified,
            unstaged: true,
            staged: false,
            diff: APP_DIFF.into(),
        },
        ChangeFile {
            original_path: None,
            path: "crates/sannygit/src/ui/diff_view.rs".into(),
            kind: ChangeKind::Modified,
            unstaged: true,
            staged: true,
            diff: DIFF_VIEW_DIFF.into(),
        },
        ChangeFile {
            original_path: None,
            path: "crates/sannygit/src/ui/commit_panel.rs".into(),
            kind: ChangeKind::Added,
            unstaged: true,
            staged: false,
            diff: COMMIT_PANEL_DIFF.into(),
        },
        ChangeFile {
            original_path: None,
            path: "crates/sannygit/assets/icons/app_icon.png".into(),
            kind: ChangeKind::Added,
            unstaged: true,
            staged: false,
            diff: "Binary file added: crates/sannygit/assets/icons/app_icon.png".into(),
        },
        ChangeFile {
            original_path: None,
            path: "crates/sannygit/Cargo.toml".into(),
            kind: ChangeKind::Modified,
            unstaged: true,
            staged: false,
            diff: CARGO_DIFF.into(),
        },
        ChangeFile {
            original_path: None,
            path: "README.md".into(),
            kind: ChangeKind::Deleted,
            unstaged: true,
            staged: false,
            diff: README_DIFF.into(),
        },
    ]
}

pub fn long_history() -> Vec<Commit> {
    (0..20)
        .flat_map(|batch| {
            commits().into_iter().map(move |mut commit| {
                commit.hash = format!("{batch:02}{}", commit.hash);
                commit.message = format!("History {batch}: {}", commit.message);
                commit
            })
        })
        .collect()
}

pub fn commits() -> Vec<Commit> {
    vec![
        Commit {
            email: "test@example.invalid".into(),
            message: "Improve diff view highlighting".into(),
            author: "SaNNy".into(),
            date: "2 hours ago".into(),
            hash: "a1b2c3d".into(),
            parents: vec!["f4e5d6c".into(), "9a8b7c6".into()],
            lane: 0,
        },
        Commit {
            email: "test@example.invalid".into(),
            message: "Add commit panel and shortcuts".into(),
            author: "SaNNy".into(),
            date: "5 hours ago".into(),
            hash: "f4e5d6c".into(),
            parents: vec!["3c2d1e0".into()],
            lane: 1,
        },
        Commit {
            email: "test@example.invalid".into(),
            message: "Update icons and application name".into(),
            author: "SaNNy".into(),
            date: "1 day ago".into(),
            hash: "9a8b7c6".into(),
            parents: vec!["1a2b3c4".into()],
            lane: 0,
        },
        Commit {
            email: "test@example.invalid".into(),
            message: "Refactor UI layout".into(),
            author: "SaNNy".into(),
            date: "1 day ago".into(),
            hash: "3c2d1e0".into(),
            parents: vec!["7f6e5d4".into()],
            lane: 1,
        },
        Commit {
            email: "test@example.invalid".into(),
            message: "Initial egui application".into(),
            author: "SaNNy".into(),
            date: "2 days ago".into(),
            hash: "7f6e5d4".into(),
            parents: vec!["1a2b3c4".into()],
            lane: 1,
        },
        Commit {
            email: "test@example.invalid".into(),
            message: "Initial commit".into(),
            author: "SaNNy".into(),
            date: "2 days ago".into(),
            hash: "1a2b3c4".into(),
            parents: vec![],
            lane: 0,
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

pub fn stash_entries() -> Vec<crate::stash::Entry> {
    (0..12)
        .map(|index| crate::stash::Entry {
            oid: format!("{index:040x}"),
            message: format!("On main: Work in progress {index}"),
        })
        .collect()
}

pub fn stash_dialog() -> crate::app::stash::Dialog {
    crate::app::stash::Dialog {
        path: "/test/repository".into(),
        entries: stash_entries(),
        selected: Some(0),
        entries_current: true,
        reload_pending: false,
        message: String::new(),
        include_untracked: false,
        restore_index: true,
        confirm_drop: false,
        prepared_save: None,
        previewed_entry: None,
        preview: None,
        error: None,
        status: "Preview changes or select a saved stash.".into(),
    }
}
