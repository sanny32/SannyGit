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

pub struct Dialog {
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

fn load(root: &Path) -> Result<Snapshot, String> {
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
        return Err("HEAD changed while loading. Reopen Merge.".into());
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
        if !self.merge_allowed() {
            return;
        }
        let root = self.repositories[self.selected_repository].path.clone();
        self.merge_dialog = Some(Dialog {
            root: root.clone(),
            snapshot: None,
            selected: None,
            shown: HashSet::new(),
            filter: String::new(),
            branches_open: false,
            include_tracked: true,
            include_head: false,
        });
        if !self.start_worker(move || WorkerResult::MergeDetails {
            result: load(Path::new(&root)),
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
        if self.busy() {
            return;
        }
        let Some(dialog) = &self.merge_dialog else {
            return;
        };
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
        if self.start_worker(move || WorkerResult::Operation {
            committed: false,
            success: "Merge complete",
            outcome: merge::execute(Path::new(&root), &head, &branch, &target, mode),
            snapshot: crate::git::load(Path::new(&root)),
        }) {
            self.status = "Merging…".into();
        }
    }
}

#[cfg(test)]
mod tests {
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
