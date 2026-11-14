use super::{SannyGitApp, WorkerResult};
use crate::{
    branches, history, merge,
    models::{BranchRef, History, RefKind},
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::atomic::AtomicBool,
};

pub struct Snapshot {
    pub head: String,
    pub branch: String,
    pub refs: Vec<BranchRef>,
    pub history: History,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Merge,
    Rebase,
}

impl Operation {
    pub fn id(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Merge => "Merge",
            Self::Rebase => "Rebase",
        }
    }

    pub fn branches_id(self) -> &'static str {
        match self {
            Self::Merge => "merge.branches",
            Self::Rebase => "rebase.branches",
        }
    }
}

// Merge and Rebase share the same snapshot-based commit and branch picker.
pub struct Dialog {
    pub operation: Operation,
    pub root: String,
    pub snapshot: Option<Snapshot>,
    pub selected: Option<String>,
    pub shown: HashSet<String>,
    pub filter: String,
    pub branches_open: bool,
    pub include_tracked: bool,
    pub include_head: bool,
}

impl Dialog {
    pub fn visible(&self) -> HashSet<String> {
        let Some(snapshot) = &self.snapshot else {
            return HashSet::new();
        };
        let parents: HashMap<_, _> = snapshot
            .history
            .commits
            .iter()
            .map(|commit| (commit.hash.as_str(), &commit.parents))
            .collect();
        let mut pending = Vec::new();
        if self.include_head {
            pending.push(snapshot.head.clone());
        }
        for entry in &snapshot.refs {
            if self.shown.contains(&entry.full_name) {
                pending.push(entry.hash.clone());
                if self.include_tracked {
                    if let Some(upstream) = &entry.upstream {
                        if let Some(remote) = snapshot
                            .refs
                            .iter()
                            .find(|other| &other.name == upstream && other.kind == RefKind::Remote)
                        {
                            pending.push(remote.hash.clone());
                        }
                    }
                }
            }
        }
        let mut result = HashSet::new();
        while let Some(hash) = pending.pop() {
            if result.insert(hash.clone()) {
                if let Some(parents) = parents.get(hash.as_str()) {
                    pending.extend(parents.iter().cloned());
                }
            }
        }
        result
    }
}

fn load(root: &Path, operation: Operation) -> Result<Snapshot, String> {
    let head = merge::head(root)?;
    let branch = crate::git::load(root)?.branch;
    let refs = branches::load(root)?.refs;
    let page = history::reload(
        root,
        true,
        history::HISTORY_LIMIT,
        &[],
        &AtomicBool::new(false),
    )?;
    if merge::head(root)? != head {
        return Err(format!(
            "HEAD changed while loading. Reopen {}.",
            operation.title()
        ));
    }
    Ok(Snapshot {
        head,
        branch,
        refs,
        history: page.history,
    })
}

impl SannyGitApp {
    pub fn merge_allowed(&self) -> bool {
        self.branch_base_blocker().is_none() && self.branch_head_exists
    }

    pub fn open_merge(&mut self) {
        self.open_commit_picker(Operation::Merge);
    }

    pub fn open_rebase(&mut self) {
        self.open_commit_picker(Operation::Rebase);
    }

    fn open_commit_picker(&mut self, operation: Operation) {
        if !self.merge_allowed() {
            return;
        }
        let root = self.repositories[self.selected_repository].path.clone();
        self.merge_dialog = Some(Dialog {
            operation,
            root: root.clone(),
            snapshot: None,
            selected: None,
            shown: HashSet::new(),
            filter: String::new(),
            branches_open: false,
            include_tracked: true,
            include_head: operation == Operation::Rebase,
        });
        if !self.start_worker(move || WorkerResult::MergeDetails {
            result: load(Path::new(&root), operation),
            root,
        }) {
            self.merge_dialog = None;
        }
    }

    pub(super) fn apply_merge_details(&mut self, root: String, result: Result<Snapshot, String>) {
        let Some(dialog) = self
            .merge_dialog
            .as_mut()
            .filter(|dialog| dialog.root == root)
        else {
            return;
        };
        match result {
            Ok(snapshot) => {
                dialog.shown = snapshot
                    .refs
                    .iter()
                    .filter(|entry| {
                        entry.full_name == self.selected_branch && entry.kind != RefKind::Tag
                    })
                    .map(|entry| entry.full_name.clone())
                    .collect();
                if dialog.operation == Operation::Rebase && dialog.shown.is_empty() {
                    dialog.shown.extend(
                        snapshot
                            .refs
                            .iter()
                            .filter(|entry| entry.current)
                            .map(|entry| entry.full_name.clone()),
                    );
                }
                dialog.selected = snapshot
                    .refs
                    .iter()
                    .find(|entry| {
                        entry.full_name == self.selected_branch
                            && entry.hash != snapshot.head
                            && entry.kind != RefKind::Tag
                            && snapshot
                                .history
                                .commits
                                .iter()
                                .any(|commit| commit.hash == entry.hash)
                    })
                    .map(|entry| entry.hash.clone());
                dialog.snapshot = Some(snapshot);
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn submit_merge(&mut self, mode: merge::Mode) {
        self.submit_commit_operation(Some(mode));
    }

    pub fn submit_rebase(&mut self) {
        self.submit_commit_operation(None);
    }

    fn submit_commit_operation(&mut self, mode: Option<merge::Mode>) {
        if self.busy() {
            return;
        }
        let Some(dialog) = &self.merge_dialog else {
            return;
        };
        if !matches!(
            (dialog.operation, mode),
            (Operation::Merge, Some(_)) | (Operation::Rebase, None)
        ) {
            return;
        }
        let (Some(snapshot), Some(target)) = (&dialog.snapshot, &dialog.selected) else {
            return;
        };
        if target == &snapshot.head
            || !snapshot
                .history
                .commits
                .iter()
                .any(|commit| &commit.hash == target)
        {
            return;
        }
        let root = dialog.root.clone();
        let head = snapshot.head.clone();
        let branch = snapshot.branch.clone();
        let target = target.clone();
        let operation = dialog.operation;
        if self.start_worker(move || WorkerResult::Operation {
            committed: false,
            success: if operation == Operation::Rebase {
                "Rebase complete"
            } else {
                "Merge complete"
            },
            outcome: match mode {
                Some(mode) => merge::execute(Path::new(&root), &head, &branch, &target, mode),
                None => crate::rebase::execute(Path::new(&root), &head, &branch, &target),
            },
            snapshot: crate::git::load(Path::new(&root)),
        }) {
            self.status = if operation == Operation::Rebase {
                "Rebasing…"
            } else {
                "Merging…"
            }
            .into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(app: &mut SannyGitApp) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while app.busy() {
            app.poll_worker();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn rebase_dialog_loads_without_writes_keeps_failed_selection_and_closes_after_success() {
        let repo = crate::test_support::TempRepo::new();
        repo.write("base", "base\n");
        crate::git::commit(&repo.0, "base", crate::git::CommitContents::All).unwrap();
        repo.git(&["checkout", "-b", "target"]);
        repo.write("target", "target\n");
        crate::git::commit(&repo.0, "target", crate::git::CommitContents::All).unwrap();
        let target = merge::head(&repo.0).unwrap();
        repo.git(&["checkout", "main"]);
        repo.write("local", "local\n");
        crate::git::commit(&repo.0, "local", crate::git::CommitContents::All).unwrap();
        let before = merge::head(&repo.0).unwrap();
        let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&repo.0).unwrap());
        app.refs_current = true;
        app.branch_head_exists = true;
        assert!(app.menu_action_enabled(crate::menu::MenuAction::Rebase));
        app.open_rebase();
        wait(&mut app);
        let dialog = app.merge_dialog.as_mut().unwrap();
        assert_eq!(dialog.operation, Operation::Rebase);
        assert!(dialog.include_head && dialog.shown.contains("refs/heads/main"));
        assert_eq!(dialog.snapshot.as_ref().unwrap().head, before);
        assert_eq!(merge::head(&repo.0).unwrap(), before);
        dialog.selected = Some(target.clone());
        repo.write("untracked", "keep\n");
        app.submit_rebase();
        wait(&mut app);
        assert!(app.error.as_ref().unwrap().contains("local changes"));
        assert_eq!(
            app.merge_dialog.as_ref().unwrap().selected.as_ref(),
            Some(&target)
        );
        assert_eq!(merge::head(&repo.0).unwrap(), before);
        // The failed operation preserved the new file; remove our fixture file
        // to retry the same reviewed selection with a clean working tree.
        std::fs::remove_file(repo.0.join("untracked")).unwrap();
        app.error = None;
        app.submit_rebase();
        wait(&mut app);
        assert!(app.error.is_none());
        assert!(app.merge_dialog.is_none());
        assert_eq!(app.status, "Rebase complete");
        assert_eq!(
            repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(),
            target
        );
    }

    #[test]
    fn branch_scope_includes_ancestors_and_optional_tracked_remote() {
        let mut dialog = crate::mock_data::merge_dialog();
        assert_eq!(dialog.visible().len(), 6);
        dialog.include_tracked = false;
        assert_eq!(dialog.visible().len(), 4);
        dialog.shown.clear();
        assert_eq!(dialog.visible().len(), 4);
        dialog.include_head = false;
        assert!(dialog.visible().is_empty());
        dialog.shown.insert("refs/heads/feature".into());
        assert_eq!(dialog.visible().len(), 2);
    }
}
