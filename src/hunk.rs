//! Apply a reviewed hunk or line edit without touching the rest of the file.
use crate::diff_edit::{self, DiffEdit, PatchPlan};
use crate::git::GitContext;
use crate::models::ChangeKind;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Request {
    pub path: String,
    pub kind: ChangeKind,
    pub original_path: Option<String>,
    pub expected_diff: String,
    pub selected: Vec<usize>,
    pub edit: DiffEdit,
    pub summary: String,
}

pub fn prepare(
    path: &str,
    kind: ChangeKind,
    original_path: Option<String>,
    expected_diff: &str,
    selected: &[usize],
    edit: DiffEdit,
) -> Result<Request, String> {
    let lines = crate::diff::parse_unified_diff(expected_diff);
    let selection = selected.iter().copied().collect();
    // Reject a selection that cannot be applied before opening a confirmation.
    diff_edit::partial_patch(&lines, &selection, edit)?;
    Ok(Request {
        path: path.to_owned(),
        kind,
        original_path,
        expected_diff: expected_diff.to_owned(),
        selected: selected.to_vec(),
        edit,
        summary: diff_edit::edit_summary(path, selected.len(), edit),
    })
}

pub fn apply(git: &GitContext, root: &Path, request: &Request) -> Result<(), String> {
    let merging = crate::git::check_operation(git, root, true)?;
    if request.kind == ChangeKind::Conflicted || request.original_path.is_some() {
        return Err(
            "Stage, unstage, or discard this file as a whole. Line edits do not apply to conflicts or renames.".into(),
        );
    }
    if merging && request.edit != DiffEdit::Stage {
        return Err(
            "During a merge, stage individual lines or discard the whole file from the review dialog.".into(),
        );
    }
    let current = crate::git::file_diff(
        git,
        root,
        &request.path,
        request.kind,
        request.original_path.as_deref(),
        false,
    )?;
    if current != request.expected_diff {
        return Err("The diff changed. Refresh and select the lines again.".into());
    }
    let lines = crate::diff::parse_unified_diff(&current);
    let selected = request.selected.iter().copied().collect();
    match diff_edit::partial_patch(&lines, &selected, request.edit)? {
        PatchPlan::Apply(patch) => apply_patch(git, root, &request.path, request.edit, &patch),
        PatchPlan::RewriteUntracked { path, kept_lines } => {
            rewrite_untracked(root, &path, &lines, &kept_lines)
        }
    }
}

fn apply_patch(
    git: &GitContext,
    root: &Path,
    path: &str,
    edit: DiffEdit,
    patch: &str,
) -> Result<(), String> {
    let mut patch = patch.to_owned();
    if patch.contains("new file mode 100644") {
        #[cfg(unix)]
        if executable(root, path) {
            patch = patch.replacen("new file mode 100644", "new file mode 100755", 1);
        }
    }
    let mut args = vec!["apply", "--whitespace=nowarn"];
    if edit != DiffEdit::Stage {
        args.push("--reverse");
    }
    if edit != DiffEdit::Discard {
        args.push("--cached");
    }
    let mut check = args.clone();
    check.extend_from_slice(&["--check", "-"]);
    crate::git::run_with_input(git, root, &check, Some(patch.as_bytes())).map_err(|error| {
        format!("The selected lines no longer apply cleanly. Refresh and try again. {error}")
    })?;
    args.push("-");
    crate::git::run_with_input(git, root, &args, Some(patch.as_bytes())).map(|_| ())
}

fn rewrite_untracked(
    root: &Path,
    path: &str,
    lines: &[crate::models::DiffLine],
    kept_lines: &[String],
) -> Result<(), String> {
    let target = crate::discard::safe_path(root, path)?;
    let bytes = std::fs::read(&target).map_err(|error| format!("Cannot read {path}: {error}"))?;
    if bytes.contains(&0) {
        return Err("Binary untracked files are discarded as a whole.".into());
    }
    let text = String::from_utf8(bytes).map_err(|_| format!("{path} is not UTF-8 text."))?;
    let trailing_newline = text.ends_with('\n');
    let current: Vec<_> = text.lines().map(str::to_owned).collect();
    if current != untracked_added_text(lines) || !is_subsequence(kept_lines, &current) {
        return Err("The untracked file changed. Refresh and select the lines again.".into());
    }
    if kept_lines.is_empty() {
        std::fs::remove_file(&target)
            .map_err(|error| format!("Could not delete {path}: {error}"))?;
        return Ok(());
    }
    let mut output = kept_lines.join("\n");
    if trailing_newline {
        output.push('\n');
    }
    std::fs::write(&target, output).map_err(|error| format!("Could not update {path}: {error}"))
}

fn untracked_added_text(lines: &[crate::models::DiffLine]) -> Vec<String> {
    let mut untracked = false;
    lines
        .iter()
        .filter_map(|line| {
            if line.text == "Untracked file" {
                untracked = true;
            } else if line.text.starts_with("Staged changes")
                || line.text.starts_with("Working tree changes")
                || line.text.starts_with("Conflict:")
            {
                untracked = false;
            }
            (untracked && line.kind == crate::models::DiffLineKind::Added)
                .then(|| line.text.clone())
        })
        .collect()
}

fn is_subsequence(kept: &[String], current: &[String]) -> bool {
    let mut current = current.iter();
    kept.iter()
        .all(|line| current.any(|candidate| candidate == line))
}

fn executable(root: &Path, path: &str) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        crate::discard::safe_path(root, path)
            .ok()
            .and_then(|path| path.symlink_metadata().ok())
            .is_some_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = (root, path);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TempRepo, SYSTEM_GIT};
    use std::collections::BTreeSet;

    fn commit_base(repo: &TempRepo, body: &str) {
        repo.write("file", body);
        repo.git(&["add", "file"]);
        repo.git(&["commit", "-m", "base"]);
    }

    fn change_lines(patch: &str, text: &str) -> Vec<usize> {
        crate::diff::parse_unified_diff(patch)
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                line.text == text
                    && matches!(
                        line.kind,
                        crate::models::DiffLineKind::Added | crate::models::DiffLineKind::Removed
                    )
            })
            .map(|(index, _)| index)
            .collect()
    }

    #[test]
    fn stage_unstage_and_discard_hunks_and_lines() {
        let repo = TempRepo::new();
        let mut base = String::new();
        for line in 1..=20 {
            base.push_str(&format!("line {line}\n"));
        }
        commit_base(&repo, &base);
        let mut edited = base.clone();
        edited = edited.replacen("line 2\n", "LINE 2\n", 1);
        edited = edited.replacen("line 18\n", "LINE 18\n", 1);
        repo.write("file", &edited);

        let patch = crate::git::file_diff(
            &SYSTEM_GIT,
            &repo.0,
            "file",
            ChangeKind::Modified,
            None,
            false,
        )
        .unwrap();
        let second = change_lines(&patch, "line 18");
        let second_new = change_lines(&patch, "LINE 18");
        let mut hunk = second.clone();
        hunk.extend(second_new.iter().copied());
        let request = prepare(
            "file",
            ChangeKind::Modified,
            None,
            &patch,
            &hunk,
            DiffEdit::Stage,
        )
        .unwrap();
        apply(&SYSTEM_GIT, &repo.0, &request).unwrap();
        let staged = repo.git(&["diff", "--cached", "--", "file"]);
        assert!(staged.contains("+LINE 18"), "{staged}");
        assert!(!staged.contains("LINE 2"), "{staged}");
        let unstaged = repo.git(&["diff", "--", "file"]);
        assert!(unstaged.contains("+LINE 2"), "{unstaged}");
        assert!(!unstaged.contains("LINE 18"), "{unstaged}");
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            edited
        );

        let staged_patch = crate::git::file_diff(
            &SYSTEM_GIT,
            &repo.0,
            "file",
            ChangeKind::Modified,
            None,
            false,
        )
        .unwrap();
        let mut removal = change_lines(&staged_patch, "line 18");
        removal.extend(change_lines(&staged_patch, "LINE 18"));
        assert!(!removal.is_empty(), "{staged_patch}");
        let request = prepare(
            "file",
            ChangeKind::Modified,
            None,
            &staged_patch,
            &removal,
            DiffEdit::Unstage,
        )
        .unwrap();
        apply(&SYSTEM_GIT, &repo.0, &request).unwrap();
        assert!(
            repo.git(&["diff", "--cached"]).is_empty(),
            "index should be clean"
        );
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            edited
        );

        let work = crate::git::file_diff(
            &SYSTEM_GIT,
            &repo.0,
            "file",
            ChangeKind::Modified,
            None,
            false,
        )
        .unwrap();
        let mut discard_lines = change_lines(&work, "LINE 18");
        discard_lines.extend(change_lines(&work, "line 18"));
        let request = prepare(
            "file",
            ChangeKind::Modified,
            None,
            &work,
            &discard_lines,
            DiffEdit::Discard,
        )
        .unwrap();
        apply(&SYSTEM_GIT, &repo.0, &request).unwrap();
        let restored = std::fs::read_to_string(repo.0.join("file")).unwrap();
        assert!(restored.contains("line 18\n"), "{restored}");
        assert!(restored.contains("LINE 2\n"), "{restored}");
        assert!(repo.git(&["diff", "--cached"]).is_empty());
    }

    #[test]
    fn staging_only_the_added_side_keeps_the_old_line_indexed() {
        let repo = TempRepo::new();
        commit_base(&repo, "one\ntwo\nthree\n");
        repo.write("file", "one\nTWO\nthree\n");
        let patch = crate::git::file_diff(
            &SYSTEM_GIT,
            &repo.0,
            "file",
            ChangeKind::Modified,
            None,
            false,
        )
        .unwrap();
        let added = change_lines(&patch, "TWO");
        let request = prepare(
            "file",
            ChangeKind::Modified,
            None,
            &patch,
            &added,
            DiffEdit::Stage,
        )
        .unwrap();
        apply(&SYSTEM_GIT, &repo.0, &request).unwrap();
        assert_eq!(repo.git(&["show", ":file"]), "one\ntwo\nTWO\nthree\n");
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "one\nTWO\nthree\n"
        );
    }

    #[test]
    fn untracked_lines_can_be_staged_or_discarded() {
        let repo = TempRepo::new();
        commit_base(&repo, "tracked\n");
        repo.write("new", "one\ntwo\nthree\n");
        let patch = crate::git::file_diff(
            &SYSTEM_GIT,
            &repo.0,
            "new",
            ChangeKind::Untracked,
            None,
            false,
        )
        .unwrap();
        let two = change_lines(&patch, "two");
        let request = prepare(
            "new",
            ChangeKind::Untracked,
            None,
            &patch,
            &two,
            DiffEdit::Stage,
        )
        .unwrap();
        apply(&SYSTEM_GIT, &repo.0, &request).unwrap();
        assert_eq!(repo.git(&["show", ":new"]), "two\n");
        assert_eq!(
            std::fs::read_to_string(repo.0.join("new")).unwrap(),
            "one\ntwo\nthree\n"
        );

        let patch = crate::git::file_diff(
            &SYSTEM_GIT,
            &repo.0,
            "new",
            ChangeKind::Untracked,
            None,
            false,
        )
        .unwrap();
        // The worktree file is still the full untracked file. Its preview is the
        // remaining unstaged lines plus the staged line as a separate section.
        let discard = change_lines(&patch, "three");
        assert!(!discard.is_empty(), "{patch}");
        let request = prepare(
            "new",
            ChangeKind::Untracked,
            None,
            &patch,
            &discard,
            DiffEdit::Discard,
        )
        .unwrap();
        apply(&SYSTEM_GIT, &repo.0, &request).unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.0.join("new")).unwrap(),
            "one\ntwo\n"
        );
        assert_eq!(repo.git(&["show", ":new"]), "two\n");
    }

    #[test]
    fn a_stale_diff_is_rejected_without_writes() {
        let repo = TempRepo::new();
        commit_base(&repo, "one\ntwo\n");
        repo.write("file", "one\nTWO\n");
        let patch = crate::git::file_diff(
            &SYSTEM_GIT,
            &repo.0,
            "file",
            ChangeKind::Modified,
            None,
            false,
        )
        .unwrap();
        let added = change_lines(&patch, "TWO");
        let mut prepared = prepare(
            "file",
            ChangeKind::Modified,
            None,
            &patch,
            &added,
            DiffEdit::Stage,
        )
        .unwrap();
        repo.write("file", "one\nchanged again\n");
        let error = apply(&SYSTEM_GIT, &repo.0, &prepared).unwrap_err();
        assert!(error.contains("diff changed"), "{error}");
        assert!(repo.git(&["diff", "--cached"]).is_empty());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "one\nchanged again\n"
        );
        prepared.expected_diff.clear();
        let _ = BTreeSet::<usize>::new();
    }
}
