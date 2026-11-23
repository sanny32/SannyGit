use super::{SannyGitApp, WorkerResult};
use crate::git::GitContext;
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
    pub tag: Option<TagTarget>,
}

pub struct TagTarget {
    pub commit: String,
    pub fast_forward: bool,
    pub already_merged: bool,
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
    pub tag: Option<String>,
    pub fast_forward_only: bool,
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

fn load(
    git: &GitContext,
    root: &Path,
    operation: Operation,
    tag: Option<&str>,
) -> Result<Snapshot, String> {
    let head = merge::head(git, root)?;
    let branch = crate::git::load(git, root)?.branch;
    let refs = branches::load(git, root)?.refs;
    let (tag, history) = if let Some(full_name) = tag {
        let (label, object) = if let Some(reference) = refs.iter().find(|entry| {
            entry.full_name == full_name && matches!(entry.kind, RefKind::Tag | RefKind::Local)
        }) {
            (reference.name.clone(), reference.hash.clone())
        } else if branches::is_commit_id(full_name) {
            (full_name.to_owned(), full_name.to_owned())
        } else {
            return Err(
                "The selected reference no longer exists. Refresh branches and tags.".into(),
            );
        };
        let revision = format!("{object}^{{commit}}");
        let output = crate::git::run(
            git,
            root,
            &["rev-parse", "--verify", "--end-of-options", &revision],
        )
        .map_err(|error| format!("Could not resolve '{label}' to a commit.\n{error}"))?;
        let commit = crate::git::text(output)?.trim().to_owned();
        if branches::is_commit_id(full_name) && !commit.eq_ignore_ascii_case(full_name) {
            return Err("The selected object is not a commit.".into());
        }
        let already_merged = crate::git::is_ancestor(git, root, &commit, &head)?;
        let fast_forward = !already_merged && crate::git::is_ancestor(git, root, &head, &commit)?;
        (
            Some(TagTarget {
                commit,
                fast_forward,
                already_merged,
            }),
            history::prepare(Vec::new(), false),
        )
    } else {
        let page = history::reload(
            git,
            root,
            true,
            history::HISTORY_LIMIT,
            &[],
            &AtomicBool::new(false),
        )?;
        (None, page.history)
    };
    if merge::head(git, root)? != head {
        return Err(format!(
            "HEAD changed while loading. Reopen {}.",
            operation.title()
        ));
    }
    Ok(Snapshot {
        head,
        branch,
        refs,
        history,
        tag,
    })
}

impl SannyGitApp {
    pub fn abort_merge_allowed(&self) -> bool {
        self.merging
            && self.state_current
            && !self.busy()
            && self.repositories.get(self.selected_repository).is_some()
    }

    pub fn open_abort_merge(&mut self) {
        if self.abort_merge_allowed() {
            self.abort_merge_dialog =
                Some(self.repositories[self.selected_repository].path.clone());
        }
    }

    pub fn confirm_abort_merge(&mut self) {
        let Some(root) = self.abort_merge_dialog.take() else {
            return;
        };
        if !self.abort_merge_allowed() || self.repositories[self.selected_repository].path != root {
            return;
        }
        let (status, success) = match self.sequencing {
            crate::git::Sequencing::CherryPick => {
                ("Aborting cherry-pick…", "Cherry-pick aborted")
            }
            crate::git::Sequencing::Revert => ("Aborting revert…", "Revert aborted"),
            crate::git::Sequencing::Rebase => ("Aborting rebase…", "Rebase aborted"),
            _ => ("Aborting merge…", "Merge aborted"),
        };
        if self.start_worker(move |git| {
            let root = Path::new(&root);
            WorkerResult::Operation {
                committed: false,
                success,
                outcome: merge::abort(git, root),
                snapshot: crate::git::load(git, root),
            }
        }) {
            self.status = status.into();
        }
    }

    pub fn merge_allowed(&self) -> bool {
        !self.merging && self.branch_base_blocker().is_none() && self.branches.branch_head_exists
    }

    pub fn open_merge(&mut self) {
        self.open_commit_picker(Operation::Merge, None, false);
    }

    pub fn open_merge_tag(&mut self, tag: String) {
        self.open_commit_picker(Operation::Merge, Some(tag), false);
    }

    pub fn open_fast_forward(&mut self, reference: String) {
        self.open_commit_picker(Operation::Merge, Some(reference), true);
    }

    pub fn open_rebase(&mut self) {
        self.open_commit_picker(Operation::Rebase, None, false);
    }

    pub fn open_merge_commit(&mut self, commit: String) {
        if branches::is_commit_id(&commit) {
            self.open_commit_picker(Operation::Merge, Some(commit), false);
        }
    }

    pub fn open_rebase_commit(&mut self, commit: String) {
        if branches::is_commit_id(&commit) {
            self.open_commit_picker(Operation::Rebase, Some(commit), false);
        }
    }

    fn open_commit_picker(
        &mut self,
        operation: Operation,
        tag: Option<String>,
        fast_forward_only: bool,
    ) {
        if !self.merge_allowed() {
            return;
        }
        let root = self.repositories[self.selected_repository].path.clone();
        self.merge_dialog = Some(Dialog {
            operation,
            root: root.clone(),
            snapshot: None,
            tag: tag.clone(),
            fast_forward_only,
            selected: None,
            shown: HashSet::new(),
            filter: String::new(),
            branches_open: false,
            include_tracked: true,
            include_head: true,
        });
        if !self.start_worker(move |git| WorkerResult::MergeDetails {
            result: load(git, Path::new(&root), operation, tag.as_deref()),
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
            Ok(snapshot) if snapshot.tag.is_some() => {
                dialog.snapshot = Some(snapshot);
            }
            Ok(snapshot) => {
                dialog.shown = snapshot
                    .refs
                    .iter()
                    .filter(|entry| {
                        entry.kind != RefKind::Tag
                            && (dialog.operation == Operation::Merge
                                || entry.full_name == self.branches.selected_branch)
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
                        entry.full_name == self.branches.selected_branch
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
                if dialog.operation == Operation::Merge && dialog.selected.is_none() {
                    let visible = dialog.visible();
                    dialog.selected = dialog.snapshot.as_ref().and_then(|snapshot| {
                        snapshot
                            .history
                            .commits
                            .iter()
                            .find(|commit| visible.contains(&commit.hash))
                            .map(|commit| commit.hash.clone())
                    });
                }
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
        let Some(snapshot) = &dialog.snapshot else {
            return;
        };
        let fast_forward_only = dialog.fast_forward_only;
        // A chosen commit is already confirmed. Close that form when Git starts
        // and leave progress in the status bar. The branch picker stays open so
        // a refused rebase can keep its selection.
        let close_when_started = fast_forward_only
            || matches!(
                mode,
                Some(merge::Mode::FastForward) | Some(merge::Mode::Squash)
            )
            || (dialog.operation == Operation::Rebase && dialog.tag.is_some());
        if fast_forward_only && mode != Some(merge::Mode::FastForward) {
            return;
        }
        let target = if let Some(tag) = &snapshot.tag {
            let allowed = match mode {
                Some(merge::Mode::FastForward) => !tag.already_merged && tag.fast_forward,
                Some(_) => !tag.already_merged,
                None => dialog.operation == Operation::Rebase && tag.commit != snapshot.head,
            };
            allowed.then_some(&tag.commit)
        } else {
            dialog.selected.as_ref().filter(|target| {
                *target != &snapshot.head
                    && snapshot
                        .history
                        .commits
                        .iter()
                        .any(|commit| &commit.hash == *target)
            })
        };
        let Some(target) = target else {
            return;
        };
        let root = dialog.root.clone();
        let head = snapshot.head.clone();
        let branch = snapshot.branch.clone();
        let target = target.clone();
        let preferred_ref = self.branches.selected_branch.clone();
        let operation = dialog.operation;
        if self.start_background_operation(move |git| {
            let root = Path::new(&root);
            let result = match mode {
                Some(mode) => {
                    merge::execute(git, root, &head, &branch, &target, &preferred_ref, mode)
                }
                None => crate::rebase::execute(git, root, &head, &branch, &target).map(|()| false),
            };
            let snapshot = crate::git::load(git, root);
            let success = match (mode, &result) {
                (None, _) => "Rebase complete",
                (_, Ok(true)) => "Merge started. Resolve conflicts in the working tree",
                (Some(merge::Mode::WorkingTree), _)
                    if snapshot.as_ref().is_ok_and(|s| s.merging) =>
                {
                    "Merge ready to review and commit"
                }
                (Some(merge::Mode::Squash), Ok(_)) => {
                    "Squash merge staged. Review and commit"
                }
                (Some(merge::Mode::FastForward), Ok(_)) => "Fast-forward merge complete",
                _ => "Merge complete",
            };
            WorkerResult::Operation {
                committed: false,
                success,
                outcome: result.map(|_| ()),
                snapshot,
            }
        }) {
            if close_when_started {
                self.merge_dialog = None;
            }
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
    use crate::test_support::SYSTEM_GIT;

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
        crate::git::commit(
            &SYSTEM_GIT,
            &repo.0,
            "base",
            crate::git::CommitContents::All,
        )
        .unwrap();
        repo.git(&["checkout", "-b", "target"]);
        repo.write("target", "target\n");
        crate::git::commit(
            &SYSTEM_GIT,
            &repo.0,
            "target",
            crate::git::CommitContents::All,
        )
        .unwrap();
        let target = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["checkout", "main"]);
        repo.write("local", "local\n");
        crate::git::commit(
            &SYSTEM_GIT,
            &repo.0,
            "local",
            crate::git::CommitContents::All,
        )
        .unwrap();
        let before = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&SYSTEM_GIT, &repo.0).unwrap());
        app.branches.current = true;
        app.branches.branch_head_exists = true;
        assert!(app.menu_action_enabled(crate::menu::MenuAction::Rebase));
        app.open_rebase();
        wait(&mut app);
        let dialog = app.merge_dialog.as_mut().unwrap();
        assert_eq!(dialog.operation, Operation::Rebase);
        assert!(dialog.include_head && dialog.shown.contains("refs/heads/main"));
        assert_eq!(dialog.snapshot.as_ref().unwrap().head, before);
        assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);
        dialog.selected = Some(target.clone());
        repo.write("untracked", "keep\n");
        app.submit_rebase();
        wait(&mut app);
        assert!(app.error.as_ref().unwrap().contains("local changes"));
        assert_eq!(
            app.merge_dialog.as_ref().unwrap().selected.as_ref(),
            Some(&target)
        );
        assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);
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
    fn tag_merge_reviews_the_tag_commit_and_keeps_it_pinned() {
        let repo = crate::test_support::TempRepo::new();
        repo.write("base", "base\n");
        crate::git::commit(
            &SYSTEM_GIT,
            &repo.0,
            "base",
            crate::git::CommitContents::All,
        )
        .unwrap();
        let before = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["checkout", "-b", "feature"]);
        repo.write("feature", "feature\n");
        crate::git::commit(
            &SYSTEM_GIT,
            &repo.0,
            "feature",
            crate::git::CommitContents::All,
        )
        .unwrap();
        let target = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["tag", "lightweight"]);
        repo.git(&["tag", "-a", "-m", "Release", "annotated"]);
        repo.git(&["checkout", "main"]);
        repo.git(&["branch", "-D", "feature"]);

        let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&SYSTEM_GIT, &repo.0).unwrap());
        app.branches.current = true;
        app.branches.branch_head_exists = true;
        for name in ["lightweight", "annotated"] {
            app.open_merge_tag(format!("refs/tags/{name}"));
            wait(&mut app);
            assert!(app.error.is_none(), "{:?}", app.error);
            let dialog = app.merge_dialog.as_ref().unwrap();
            let tag = dialog.snapshot.as_ref().unwrap().tag.as_ref().unwrap();
            assert_eq!(tag.commit, target);
            assert!(tag.fast_forward);
            assert!(!tag.already_merged);
            assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);
        }

        repo.git(&["tag", "-f", "annotated", &before]);
        app.submit_merge(merge::Mode::Commit);
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(
            repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(),
            format!("{before} {target}")
        );
    }

    #[test]
    fn tag_fast_forward_only_moves_head_to_the_reviewed_commit() {
        let repo = crate::test_support::TempRepo::new();
        repo.git(&["commit", "--allow-empty", "-m", "base"]);
        let before = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["checkout", "-b", "feature"]);
        repo.write("feature", "feature\n");
        repo.git(&["add", "feature"]);
        repo.git(&["commit", "-m", "feature"]);
        repo.git(&["tag", "-a", "-m", "Release", "release"]);
        let target = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["checkout", "main"]);
        repo.write("untracked", "keep\n");

        let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&SYSTEM_GIT, &repo.0).unwrap());
        app.branches.current = true;
        app.branches.branch_head_exists = true;
        app.open_fast_forward("refs/tags/release".into());
        wait(&mut app);
        let dialog = app.merge_dialog.as_ref().unwrap();
        assert!(dialog.fast_forward_only);
        assert!(
            dialog
                .snapshot
                .as_ref()
                .unwrap()
                .tag
                .as_ref()
                .unwrap()
                .fast_forward
        );
        for mode in [merge::Mode::Commit, merge::Mode::WorkingTree] {
            app.submit_merge(mode);
            assert!(!app.busy());
            assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);
        }
        app.submit_merge(merge::Mode::FastForward);
        assert!(app.merge_dialog.is_none());
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), target);
        assert_eq!(repo.git(&["branch", "--show-current"]).trim(), "main");
        assert_eq!(std::fs::read(repo.0.join("untracked")).unwrap(), b"keep\n");
    }

    #[test]
    fn merge_opens_with_other_branches_visible_and_executes_both_modes() {
        for mode in [merge::Mode::Commit, merge::Mode::WorkingTree] {
            let repo = crate::test_support::TempRepo::new();
            repo.write("base", "base\n");
            crate::git::commit(
                &SYSTEM_GIT,
                &repo.0,
                "base",
                crate::git::CommitContents::All,
            )
            .unwrap();
            let before = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
            repo.git(&["checkout", "-b", "feature"]);
            repo.write("feature", "feature\n");
            crate::git::commit(
                &SYSTEM_GIT,
                &repo.0,
                "feature",
                crate::git::CommitContents::All,
            )
            .unwrap();
            let target = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
            repo.git(&["checkout", "main"]);
            repo.git(&["update-ref", "refs/remotes/origin/dev", &target]);

            let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
            app.apply_snapshot(crate::git::load(&SYSTEM_GIT, &repo.0).unwrap());
            app.branches.current = true;
            app.branches.branch_head_exists = true;
            app.branches.selected_branch = "refs/remotes/origin/dev".into();
            app.open_merge();
            wait(&mut app);
            let dialog = app.merge_dialog.as_ref().unwrap();
            assert!(dialog.visible().contains(&target));
            assert_eq!(dialog.selected.as_ref(), Some(&target));
            assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);
            app.submit_merge(mode);
            wait(&mut app);

            assert!(app.error.is_none(), "{:?}", app.error);
            assert!(app.merge_dialog.is_none());
            match mode {
                merge::Mode::Commit => {
                    assert_eq!(
                        repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(),
                        format!("{before} {target}")
                    );
                    assert_eq!(
                        repo.git(&["show", "-s", "--format=%B", "HEAD"]).trim(),
                        "Merge remote-tracking branch 'origin/dev'"
                    );
                }
                merge::Mode::WorkingTree => {
                    assert_eq!(
                        app.commit.message,
                        "Merge remote-tracking branch 'origin/dev'"
                    );
                    assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);
                    assert!(app
                        .changes
                        .iter()
                        .any(|file| file.path == "feature" && file.staged));
                }
                merge::Mode::FastForward => unreachable!(),
                merge::Mode::Squash => {
                    assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);
                    assert!(app
                        .changes
                        .iter()
                        .any(|file| file.path == "feature" && file.staged));
                }
            }
        }
    }

    #[test]
    fn conflicted_merge_can_discard_remove_and_commit_from_the_working_tree() {
        let repo = crate::test_support::TempRepo::new();
        for path in ["discard", "remove", "edit"] {
            repo.write(path, "base\n");
        }
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "base"]);
        repo.git(&["checkout", "-b", "feature"]);
        for path in ["discard", "remove", "edit", "clean"] {
            repo.write(path, "theirs\n");
        }
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "theirs"]);
        let target = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["checkout", "main"]);
        for path in ["discard", "remove", "edit"] {
            repo.write(path, "ours\n");
        }
        repo.git(&["commit", "-am", "ours"]);
        let before = merge::head(&SYSTEM_GIT, &repo.0).unwrap();

        let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&SYSTEM_GIT, &repo.0).unwrap());
        app.branches.current = true;
        app.branches.branch_head_exists = true;
        app.open_merge();
        wait(&mut app);
        app.merge_dialog.as_mut().unwrap().selected = Some(target.clone());
        app.submit_merge(merge::Mode::WorkingTree);
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.merge_dialog.is_none() && app.merging);
        let message = "Merge branch 'feature'";
        let draft = format!("{message}\n\n# Conflicts:\n#\tdiscard\n#\tedit\n#\tremove");
        assert_eq!(app.commit.message, draft);
        assert!(app
            .changes
            .iter()
            .any(|file| file.path == "clean" && file.staged));
        assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);

        for (path, remove) in [("discard", false), ("remove", true)] {
            app.selection.paths.clear();
            app.selection.active = app
                .changes
                .iter()
                .position(|file| file.path == path)
                .unwrap();
            if remove {
                app.open_remove();
            } else {
                app.open_discard();
            }
            wait(&mut app);
            let dialog = app.discard_dialog.as_mut().unwrap();
            assert!(dialog.prepared.is_some(), "{:?}", dialog.error);
            dialog.confirmed = true;
            app.confirm_discard();
            wait(&mut app);
            assert!(app.error.is_none(), "{:?}", app.error);
            assert!(app.discard_dialog.is_none());
        }
        assert_eq!(
            std::fs::read_to_string(repo.0.join("discard")).unwrap(),
            "ours\n"
        );
        assert!(!repo.0.join("remove").exists());
        repo.write("edit", "resolved\n");
        app.commit.message.push_str("\n\nResolved merge");
        app.selection.paths.clear();
        app.selection.active = app
            .changes
            .iter()
            .position(|file| file.path == "edit")
            .unwrap();
        app.set_selected_staged(true);
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.commit.message, format!("{draft}\n\nResolved merge"));
        app.create_commit();
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(!app.merging);
        assert_eq!(
            repo.git(&["show", "-s", "--format=%B", "HEAD"]).trim(),
            format!("{message}\n\nResolved merge")
        );
        assert_eq!(
            repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(),
            format!("{before} {target}")
        );
    }

    #[test]
    fn abort_merge_restores_the_working_tree_after_confirmation() {
        let repo = crate::test_support::TempRepo::new();
        repo.write("base", "base\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "base"]);
        let before = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["checkout", "-b", "feature"]);
        repo.write("feature", "feature\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "feature"]);
        repo.git(&["checkout", "main"]);
        repo.git(&["merge", "--no-ff", "--no-commit", "feature"]);

        let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&SYSTEM_GIT, &repo.0).unwrap());
        assert_eq!(app.commit.message, "Merge branch 'feature'");
        app.remember_commit_draft();
        assert!(app.commit.commit_drafts.is_empty());
        app.open_abort_merge();
        assert!(app.modal_dialog_open());
        assert!(crate::git::load(&SYSTEM_GIT, &repo.0).unwrap().merging);
        app.confirm_abort_merge();
        wait(&mut app);

        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.abort_merge_dialog.is_none() && !app.merging);
        assert!(app.commit.message.is_empty());
        assert!(app.changes.is_empty());
        assert_eq!(app.status, "Merge aborted");
        assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);
        assert!(!repo.0.join("feature").exists());
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

    #[test]
    fn merging_a_commit_uses_the_reviewed_id_and_refuses_a_missing_one() {
        let repo = crate::test_support::TempRepo::new();
        repo.write("base", "base\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "base"]);
        repo.git(&["checkout", "-b", "feature"]);
        repo.write("feature", "feature\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "feature"]);
        let feature = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["checkout", "main"]);
        repo.write("local", "local\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "local"]);
        let before = merge::head(&SYSTEM_GIT, &repo.0).unwrap();

        let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&SYSTEM_GIT, &repo.0).unwrap());
        app.branches.current = true;
        app.branches.branch_head_exists = true;
        app.open_merge_commit("abc".into());
        assert!(app.merge_dialog.is_none() && !app.busy());
        app.open_merge_commit("a".repeat(40));
        wait(&mut app);
        assert!(app.error.as_deref().unwrap().contains("Could not resolve"));
        app.error = None;
        app.merge_dialog = None;

        app.open_merge_commit(before.clone());
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(
            app.merge_dialog
                .as_ref()
                .unwrap()
                .snapshot
                .as_ref()
                .unwrap()
                .tag
                .as_ref()
                .unwrap()
                .already_merged
        );
        app.submit_merge(merge::Mode::Commit);
        assert!(!app.busy());
        assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);
        app.merge_dialog = None;

        app.open_merge_commit(feature.clone());
        wait(&mut app);
        let tag = app
            .merge_dialog
            .as_ref()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .tag
            .as_ref()
            .unwrap();
        assert_eq!(tag.commit, feature);
        assert!(!tag.fast_forward && !tag.already_merged);
        app.submit_merge(merge::Mode::Commit);
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.merge_dialog.is_none());
        assert_eq!(
            repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(),
            format!("{before} {feature}")
        );
        assert!(repo
            .git(&["show", "-s", "--format=%B", "HEAD"])
            .contains("Merge branch 'feature'"));
    }

    #[test]
    fn rebasing_onto_a_commit_closes_the_confirmation_and_replays() {
        let repo = crate::test_support::TempRepo::new();
        repo.write("base", "base\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "base"]);
        repo.git(&["checkout", "-b", "upstream"]);
        repo.write("upstream", "upstream\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "upstream"]);
        let target = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        repo.git(&["checkout", "main"]);
        repo.write("local", "local\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "local"]);
        let before = merge::head(&SYSTEM_GIT, &repo.0).unwrap();
        let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        app.apply_snapshot(crate::git::load(&SYSTEM_GIT, &repo.0).unwrap());
        app.branches.current = true;
        app.branches.branch_head_exists = true;

        app.open_rebase_commit(before.clone());
        wait(&mut app);
        app.submit_rebase();
        assert!(!app.busy());
        assert!(app.merge_dialog.is_some());
        assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);
        app.merge_dialog = None;

        app.open_rebase_commit(target.clone());
        wait(&mut app);
        assert_eq!(
            app.merge_dialog
                .as_ref()
                .unwrap()
                .snapshot
                .as_ref()
                .unwrap()
                .tag
                .as_ref()
                .unwrap()
                .commit,
            target
        );
        repo.write("dirty", "keep\n");
        app.submit_rebase();
        assert!(app.merge_dialog.is_none() && app.busy());
        wait(&mut app);
        assert!(app.error.as_deref().unwrap().contains("local changes"));
        assert_eq!(merge::head(&SYSTEM_GIT, &repo.0).unwrap(), before);
        std::fs::remove_file(repo.0.join("dirty")).unwrap();
        app.error = None;
        // The refused rebase refreshed the working tree and marked refs stale.
        app.branches.current = true;

        app.open_rebase_commit(target.clone());
        wait(&mut app);
        app.submit_rebase();
        wait(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.merge_dialog.is_none());
        assert_eq!(
            repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(),
            target
        );
        assert_eq!(
            std::fs::read_to_string(repo.0.join("local")).unwrap(),
            "local\n"
        );
    }
}
