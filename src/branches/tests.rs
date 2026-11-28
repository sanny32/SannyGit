use super::*;
use crate::test_support::TempRepo;
use crate::test_support::SYSTEM_GIT;

fn committed_repo() -> TempRepo {
    let repo = TempRepo::new();
    repo.write("file", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fixture"]);
    repo
}

#[test]
fn format_patch_exports_only_the_tagged_commit_and_replaces_the_previous_export() {
    let repo = committed_repo();
    let tagged = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["tag", "lightweight"]);
    repo.git(&["tag", "-a", "annotated", "-m", "Tag annotation"]);
    repo.write("file", "new commit\n");
    repo.git(&["add", "file"]);
    repo.git(&["commit", "-m", "Newer commit"]);
    repo.write("file", "local changes\n");
    let head = repo.git(&["rev-parse", "HEAD"]);
    let index = std::fs::read(repo.0.join(".git/index")).unwrap();
    let directory = repo.0.join("patch output");
    std::fs::create_dir(&directory).unwrap();
    let destination = directory.join("0001-fixture.patch");
    std::fs::write(&destination, "previous export").unwrap();
    let refs = load(&SYSTEM_GIT, &repo.0).unwrap().refs;

    for name in ["lightweight", "annotated"] {
        let tag = refs.iter().find(|r| r.name == name).unwrap();
        let path = format_tag_patch(&SYSTEM_GIT, &repo.0, &tag.hash, &directory).unwrap();
        assert_eq!(path, destination);
        let patch = std::fs::read_to_string(path).unwrap();
        assert!(patch.starts_with(&format!("From {} ", tagged.trim())));
        assert!(patch.contains("Subject: [PATCH] fixture"));
        assert!(patch.contains("+base"));
        assert!(!patch.contains("Newer commit") && !patch.contains("Tag annotation"));
    }
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    std::fs::remove_file(&destination).unwrap();
    std::fs::create_dir(&destination).unwrap();
    let tag = refs.iter().find(|r| r.name == "annotated").unwrap();
    assert!(format_tag_patch(&SYSTEM_GIT, &repo.0, &tag.hash, &directory).is_err());
    assert!(destination.is_dir());
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
    assert_eq!(
        std::fs::read_to_string(repo.0.join("file")).unwrap(),
        "local changes\n"
    );
}

#[test]
fn advanced_soft_and_mixed_keep_working_files_and_handle_the_index_differently() {
    for (strategy, expected_index) in [
        (ResetStrategy::Soft, "staged"),
        (ResetStrategy::Mixed, "base"),
    ] {
        let repo = committed_repo();
        repo.git(&["tag", "v1"]);
        repo.write("file", "new commit\n");
        repo.git(&["add", "file"]);
        repo.git(&["commit", "-m", "before reset"]);
        repo.write("file", "staged\n");
        repo.git(&["add", "file"]);
        repo.write("file", "unstaged\n");
        let preview = preview_reset(&SYSTEM_GIT, &repo.0, "refs/tags/v1").unwrap();
        assert!(preview.has_tracked_changes);

        reset_tag(
            &SYSTEM_GIT,
            &repo.0,
            &preview,
            ResetMode::Clean,
            Some(AdvancedReset {
                strategy,
                fix_line_endings: false,
            }),
        )
        .unwrap();

        assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), preview.target);
        assert_eq!(repo.git(&["show", ":file"]).trim(), expected_index);
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
        assert!(repo.git(&["stash", "list"]).is_empty());
    }
}

#[test]
fn advanced_keep_and_merge_refuse_conflicting_files_and_preserve_unrelated_edits() {
    for strategy in [ResetStrategy::Keep, ResetStrategy::Merge] {
        let repo = committed_repo();
        repo.write("keep", "base\n");
        repo.git(&["add", "keep"]);
        repo.git(&["commit", "-m", "add unrelated file"]);
        repo.git(&["tag", "v1"]);
        repo.write("file", "new commit\n");
        repo.git(&["add", "file"]);
        repo.git(&["commit", "-m", "before reset"]);
        let before = repo.git(&["rev-parse", "HEAD"]);
        repo.write("file", "conflict\n");
        let preview = preview_reset(&SYSTEM_GIT, &repo.0, "refs/tags/v1").unwrap();
        let options = Some(AdvancedReset {
            strategy,
            fix_line_endings: false,
        });

        assert!(reset_tag(&SYSTEM_GIT, &repo.0, &preview, ResetMode::Clean, options).is_err());
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"conflict\n");

        repo.write("file", "new commit\n");
        repo.write("keep", "unrelated edit\n");
        reset_tag(&SYSTEM_GIT, &repo.0, &preview, ResetMode::Clean, options).unwrap();
        assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), preview.target);
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"base\n");
        assert_eq!(
            std::fs::read(repo.0.join("keep")).unwrap(),
            b"unrelated edit\n"
        );
        assert!(repo.git(&["stash", "list"]).is_empty());
    }
}

#[test]
fn advanced_hard_refreshes_line_endings_even_for_an_unchanged_file() {
    let repo = committed_repo();
    repo.write(".gitattributes", "eol.txt text eol=crlf\n");
    repo.write("eol.txt", "first\nsecond\n");
    repo.git(&["add", ".gitattributes", "eol.txt"]);
    repo.git(&["commit", "-m", "line ending rules"]);
    repo.git(&["tag", "v1"]);
    repo.git(&["commit", "--allow-empty", "-m", "same tree"]);
    repo.git(&["switch", "--detach"]);
    repo.write("eol.txt", "first\nsecond\n");
    repo.git(&["update-index", "-q", "--refresh"]);
    repo.write("untracked", "keep\n");
    let preview = preview_reset(&SYSTEM_GIT, &repo.0, "refs/tags/v1").unwrap();
    assert!(!preview.has_tracked_changes);
    reset_tag(
        &SYSTEM_GIT,
        &repo.0,
        &preview,
        ResetMode::Clean,
        Some(AdvancedReset {
            strategy: ResetStrategy::Hard,
            fix_line_endings: true,
        }),
    )
    .unwrap();
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), preview.target);
    assert_eq!(
        repo.git(&["rev-parse", "--symbolic-full-name", "HEAD"])
            .trim(),
        "HEAD"
    );
    assert_eq!(
        std::fs::read(repo.0.join("eol.txt")).unwrap(),
        b"first\r\nsecond\r\n"
    );
    assert_eq!(std::fs::read(repo.0.join("untracked")).unwrap(), b"keep\n");
}

#[test]
fn reset_handles_tracked_changes_and_preserves_unrelated_untracked_files() {
    for mode in [ResetMode::SaveStash, ResetMode::Discard] {
        let repo = committed_repo();
        repo.git(&["commit", "--allow-empty", "-m", "before reset"]);
        let before = repo.git(&["rev-parse", "HEAD"]);
        repo.git(&["switch", "-c", "release", "HEAD~"]);
        repo.write("collision", "target file\n");
        repo.git(&["add", "collision"]);
        repo.git(&["commit", "-m", "release"]);
        repo.git(&["tag", "-a", "-m", "release tag", "v1"]);
        let tagged = repo.git(&["rev-parse", "HEAD"]);
        repo.git(&["switch", "main"]);
        repo.write("file", "staged\n");
        repo.git(&["add", "file"]);
        repo.write("file", "unstaged\n");
        repo.write("collision", "an untracked file in the way\n");
        repo.write("keep", "unrelated\n");

        let preview = preview_reset(&SYSTEM_GIT, &repo.0, "refs/tags/v1").unwrap();
        assert!(preview.has_tracked_changes);
        assert_eq!(preview.target, tagged.trim());
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
        reset_tag(&SYSTEM_GIT, &repo.0, &preview, mode, None).unwrap();

        assert_eq!(repo.git(&["rev-parse", "HEAD"]), tagged);
        assert_eq!(
            repo.git(&["symbolic-ref", "--short", "HEAD"]).trim(),
            "main"
        );
        assert_eq!(repo.git(&["status", "--porcelain"]).trim(), "?? keep");
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"base\n");
        assert_eq!(
            std::fs::read(repo.0.join("collision")).unwrap(),
            b"target file\n"
        );
        assert_eq!(std::fs::read(repo.0.join("keep")).unwrap(), b"unrelated\n");
        if mode == ResetMode::SaveStash {
            assert_eq!(repo.git(&["show", "stash@{0}:file"]).trim(), "unstaged");
            assert_eq!(repo.git(&["show", "stash@{0}^2:file"]).trim(), "staged");
            assert!(
                repo.git(&["stash", "show", "--include-untracked", "--name-only"])
                    .trim()
                    == "file"
            );
        } else {
            assert!(repo.git(&["stash", "list"]).is_empty());
        }
    }
}

#[test]
fn reset_rejects_stale_tag_branch_and_new_changes_before_moving_head() {
    let repo = committed_repo();
    repo.git(&["tag", "v1"]);
    repo.git(&["commit", "--allow-empty", "-m", "before reset"]);
    let before = repo.git(&["rev-parse", "HEAD"]);
    let preview = preview_reset(&SYSTEM_GIT, &repo.0, "refs/tags/v1").unwrap();
    assert!(!preview.has_tracked_changes);
    repo.git(&["switch", "-c", "other"]);
    assert!(
        reset_tag(&SYSTEM_GIT, &repo.0, &preview, ResetMode::Clean, None)
            .unwrap_err()
            .contains("changed")
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
    repo.git(&["switch", "main"]);
    repo.git(&["tag", "-f", "v1"]);
    assert!(
        reset_tag(&SYSTEM_GIT, &repo.0, &preview, ResetMode::Clean, None)
            .unwrap_err()
            .contains("changed")
    );
    repo.git(&["tag", "-f", "v1", "HEAD~"]);

    repo.write("file", "new work\n");
    assert!(
        reset_tag(&SYSTEM_GIT, &repo.0, &preview, ResetMode::Clean, None)
            .unwrap_err()
            .contains("Tracked changes appeared")
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
    assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"new work\n");
    repo.write("file", "base\n");
    reset_tag(&SYSTEM_GIT, &repo.0, &preview, ResetMode::Clean, None).unwrap();
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), preview.target);
}

#[test]
fn delete_removes_only_a_reviewed_merged_branch_and_preserves_dirty_index_and_files() {
    let repo = committed_repo();
    repo.git(&["branch", "merged"]);
    repo.git(&["branch", "keep"]);
    repo.write("file", "staged\n");
    repo.git(&["add", "file"]);
    repo.write("file", "unstaged\n");
    repo.write("new", "keep new\n");
    let index = repo.git(&["ls-files", "--stage"]);
    let head = repo.git(&["rev-parse", "HEAD"]);
    let preview = preview_delete(&SYSTEM_GIT, &repo.0, "refs/heads/merged").unwrap();
    assert!(repo.git(&["branch", "--list", "merged"]).contains("merged"));
    delete(&SYSTEM_GIT, &repo.0, &preview).unwrap();
    assert!(repo.git(&["branch", "--list", "merged"]).is_empty());
    assert!(repo.git(&["branch", "--list", "keep"]).contains("keep"));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["ls-files", "--stage"]), index);
    assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
    assert_eq!(std::fs::read(repo.0.join("new")).unwrap(), b"keep new\n");
}

#[test]
fn delete_rejects_current_worktree_and_stale_targets_and_removes_an_unmerged_branch() {
    let repo = committed_repo();
    repo.git(&["branch", "other"]);
    assert!(preview_delete(&SYSTEM_GIT, &repo.0, "refs/heads/main")
        .unwrap_err()
        .contains("current"));
    let preview = preview_delete(&SYSTEM_GIT, &repo.0, "refs/heads/other").unwrap();
    repo.git(&["commit", "--allow-empty", "-m", "HEAD advanced"]);
    assert!(delete(&SYSTEM_GIT, &repo.0, &preview)
        .unwrap_err()
        .contains("changed"));
    repo.git(&["switch", "other"]);
    repo.write("file", "unmerged\n");
    repo.git(&["add", "file"]);
    repo.git(&["commit", "-m", "Unique"]);
    repo.git(&["switch", "main"]);
    let unmerged = preview_delete(&SYSTEM_GIT, &repo.0, "refs/heads/other").unwrap();
    assert!(!unmerged.merged);
    delete(&SYSTEM_GIT, &repo.0, &unmerged).unwrap();
    assert!(repo.git(&["branch", "--list", "other"]).is_empty());
    assert!(repo.git(&["branch", "--list", "main"]).contains("main"));
    repo.git(&["branch", "worktree-branch"]);
    let linked = repo.0.join("linked");
    repo.git(&[
        "worktree",
        "add",
        linked.to_str().unwrap(),
        "worktree-branch",
    ]);
    assert!(
        preview_delete(&SYSTEM_GIT, &repo.0, "refs/heads/worktree-branch")
            .unwrap_err()
            .contains("worktree")
    );
    repo.git(&["tag", "tag"]);
    for name in [
        "refs/tags/tag",
        "refs/remotes/origin/main",
        "other",
        "--all",
    ] {
        assert!(preview_delete(&SYSTEM_GIT, &repo.0, name).is_err());
    }
}

#[test]
fn delete_rechecks_tip_worktree_occupancy_and_unfinished_operations_after_review() {
    let repo = committed_repo();
    repo.git(&["commit", "--allow-empty", "-m", "Next"]);
    repo.git(&["branch", "merged", "HEAD^"]);
    let preview = preview_delete(&SYSTEM_GIT, &repo.0, "refs/heads/merged").unwrap();
    repo.git(&["branch", "-f", "merged", "HEAD"]);
    assert!(delete(&SYSTEM_GIT, &repo.0, &preview)
        .unwrap_err()
        .contains("changed"));
    let preview = preview_delete(&SYSTEM_GIT, &repo.0, "refs/heads/merged").unwrap();
    repo.write(".git/MERGE_HEAD", "busy");
    assert!(delete(&SYSTEM_GIT, &repo.0, &preview)
        .unwrap_err()
        .contains("Finish"));
    std::fs::remove_file(repo.0.join(".git/MERGE_HEAD")).unwrap();
    let path = repo.0.join("linked");
    repo.git(&["worktree", "add", path.to_str().unwrap(), "merged"]);
    assert!(delete(&SYSTEM_GIT, &repo.0, &preview)
        .unwrap_err()
        .contains("worktree"));
    assert!(repo.git(&["branch", "--list", "merged"]).contains("merged"));
}

#[test]
fn creates_lightweight_and_annotated_tags_without_touching_dirty_files() {
    let repo = committed_repo();
    repo.write("file", "staged\n");
    repo.git(&["add", "file"]);
    repo.write("file", "unstaged\n");
    repo.write("new", "untracked\n");
    let index = std::fs::read(repo.0.join(".git/index")).unwrap();
    let status = repo.git(&["status", "--porcelain=v1"]);
    let head = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["config", "tag.gpgSign", "true"]);
    create_tag(&SYSTEM_GIT, &repo.0, "release/версия", " \n").unwrap();
    assert_eq!(
        repo.git(&["cat-file", "-t", "refs/tags/release/версия"])
            .trim(),
        "commit"
    );
    repo.git(&["config", "tag.gpgSign", "false"]);
    create_tag(&SYSTEM_GIT, &repo.0, "v1.0", "Release\n\nDetails #1\n").unwrap();
    assert_eq!(
        repo.git(&["cat-file", "-t", "refs/tags/v1.0"]).trim(),
        "tag"
    );
    assert!(repo
        .git(&["cat-file", "-p", "refs/tags/v1.0"])
        .ends_with("Release\n\nDetails #1\n"));
    assert_eq!(repo.git(&["rev-parse", "refs/tags/v1.0^{commit}"]), head);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["status", "--porcelain=v1"]), status);
    assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
    assert_eq!(
        std::fs::read_to_string(repo.0.join("file")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.0.join("new")).unwrap(),
        "untracked\n"
    );
    repo.git(&["checkout", "--detach"]);
    create_tag(&SYSTEM_GIT, &repo.0, "detached", "").unwrap();
    assert_eq!(repo.git(&["rev-parse", "refs/tags/detached"]), head);
}

#[test]
fn tag_creation_rejects_invalid_names_duplicates_messages_and_unborn_head() {
    let empty = TempRepo::new();
    assert!(create_tag(&SYSTEM_GIT, &empty.0, "v1", "")
        .unwrap_err()
        .contains("first commit"));
    let repo = committed_repo();
    create_tag(&SYSTEM_GIT, &repo.0, "v1", "Original").unwrap();
    let before = repo.git(&["show-ref"]);
    for name in [
        "",
        "-bad",
        "refs/tags/bad",
        "a..b",
        "a.lock",
        "space name",
        "bad\0name",
        "a@{b",
        "a^b",
        "a//b",
        ".hidden",
        "a.",
        "v1",
    ] {
        assert!(
            create_tag(&SYSTEM_GIT, &repo.0, name, "replacement").is_err(),
            "{name:?}"
        );
    }
    assert!(create_tag(&SYSTEM_GIT, &repo.0, "bad-message", "bad\0message").is_err());
    assert!(create_tag(&SYSTEM_GIT, &repo.0, "huge", &"x".repeat(1024 * 1024 + 1)).is_err());
    assert_eq!(repo.git(&["show-ref"]), before);
}

#[test]
fn rename_preserves_dirty_files_index_head_and_tracking_configuration() {
    let repo = committed_repo();
    repo.git(&["config", "branch.main.remote", "origin"]);
    repo.git(&["config", "branch.main.merge", "refs/heads/main"]);
    let head = repo.git(&["rev-parse", "HEAD"]);
    repo.write("file", "staged\n");
    repo.git(&["add", "file"]);
    repo.write("file", "unstaged\n");
    repo.write("new", "untracked\n");
    let index = std::fs::read(repo.0.join(".git/index")).unwrap();
    rename(&SYSTEM_GIT, &repo.0, "refs/heads/main", "feature/новая").unwrap();
    assert_eq!(
        git::load(&SYSTEM_GIT, &repo.0).unwrap().branch,
        "feature/новая"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
    assert_eq!(
        std::fs::read_to_string(repo.0.join("file")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.0.join("new")).unwrap(),
        "untracked\n"
    );
    assert_eq!(
        repo.git(&["config", "branch.feature/новая.remote"]).trim(),
        "origin"
    );
    assert_eq!(
        repo.git(&["config", "branch.feature/новая.merge"]).trim(),
        "refs/heads/main"
    );
    assert!(!load(&SYSTEM_GIT, &repo.0)
        .unwrap()
        .refs
        .iter()
        .any(|r| r.full_name == "refs/heads/main"));
    create(&SYSTEM_GIT, &repo.0, "other", false, None).unwrap();
    rename(&SYSTEM_GIT, &repo.0, "refs/heads/other", "renamed-other").unwrap();
    assert_eq!(
        git::load(&SYSTEM_GIT, &repo.0).unwrap().branch,
        "feature/новая"
    );
    assert_eq!(repo.git(&["rev-parse", "renamed-other"]), head);
}

#[test]
fn rename_rejects_invalid_missing_nonlocal_and_existing_targets_without_writes() {
    let repo = committed_repo();
    create(&SYSTEM_GIT, &repo.0, "existing", false, None).unwrap();
    repo.git(&["tag", "release"]);
    repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    let before = repo.git(&["show-ref"]);
    let index = std::fs::read(repo.0.join(".git/index")).unwrap();
    for name in [
        "",
        "HEAD",
        "-bad",
        "refs/heads/bad",
        "@{-1}",
        "a..b",
        "a.lock",
        "space name",
        "bad\0name",
        "existing",
        "main",
    ] {
        assert!(
            rename(&SYSTEM_GIT, &repo.0, "refs/heads/main", name).is_err(),
            "{name:?}"
        );
    }
    for source in [
        "main",
        "refs/heads/missing",
        "refs/tags/release",
        "refs/remotes/origin/main",
        "--all",
    ] {
        assert!(rename(&SYSTEM_GIT, &repo.0, source, "new-name").is_err());
    }
    assert_eq!(repo.git(&["show-ref"]), before);
    assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
    assert_eq!(git::load(&SYSTEM_GIT, &repo.0).unwrap().branch, "main");
}

#[test]
fn lists_current_local_remote_upstream_and_annotated_tags_without_writes() {
    let repo = committed_repo();
    repo.git(&["branch", "feature/ветка"]);
    repo.git(&[
        "config",
        "--local",
        "remote.origin.url",
        "https://example.invalid/fixture.git",
    ]);
    repo.git(&[
        "config",
        "--local",
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/origin/*",
    ]);
    repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    repo.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ]);
    repo.git(&["branch", "--set-upstream-to=origin/main", "main"]);
    repo.git(&["tag", "lightweight"]);
    repo.git(&[
        "tag",
        "-a",
        "release",
        "-m",
        "Версия 1.0\ncontinued\n\nDetails",
    ]);
    let index = std::fs::read(repo.0.join(".git/index")).unwrap();
    let snapshot = load(&SYSTEM_GIT, &repo.0).unwrap();
    assert!(snapshot.has_head);
    assert_eq!(snapshot.refs.len(), 5);
    let current = snapshot.refs.iter().find(|r| r.current).unwrap();
    assert_eq!(current.full_name, "refs/heads/main");
    assert_eq!(current.upstream.as_deref(), Some("origin/main"));
    assert_eq!(current.upstream_remote.as_deref(), Some("origin"));
    assert!(!current.upstream_gone);
    assert!(snapshot.refs.iter().any(|r| r.name == "feature/ветка"));
    assert!(snapshot
        .refs
        .iter()
        .any(|r| r.kind == RefKind::Remote && r.name == "origin/main"));
    assert_eq!(
        snapshot
            .refs
            .iter()
            .filter(|r| r.kind == RefKind::Tag)
            .count(),
        2
    );
    assert_eq!(
        snapshot
            .refs
            .iter()
            .find(|r| r.name == "release")
            .unwrap()
            .tag_subject
            .as_deref(),
        Some("Версия 1.0 continued")
    );
    assert!(snapshot
        .refs
        .iter()
        .filter(|r| r.name != "release")
        .all(|r| r.tag_subject.is_none()));
    let tag = snapshot.refs.iter().find(|r| r.name == "release").unwrap();
    assert_eq!(
        read_tag_message(&SYSTEM_GIT, &repo.0, &tag.hash).unwrap(),
        "Версия 1.0\ncontinued\n\nDetails\n"
    );
    let lightweight = snapshot
        .refs
        .iter()
        .find(|r| r.name == "lightweight")
        .unwrap();
    assert!(read_tag_message(&SYSTEM_GIT, &repo.0, &lightweight.hash).is_err());
    assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
}

#[test]
fn marks_only_configured_remote_upstreams_missing_from_loaded_remote_refs() {
    let repo = committed_repo();
    for name in ["kept", "deleted", "local-track"] {
        repo.git(&["branch", name]);
    }
    repo.git(&[
        "config",
        "--local",
        "remote.origin.url",
        "https://example.invalid/fixture.git",
    ]);
    repo.git(&[
        "config",
        "--local",
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/origin/*",
    ]);
    repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    repo.git(&["update-ref", "refs/remotes/origin/kept", "HEAD"]);
    repo.git(&["branch", "--set-upstream-to=origin/main", "main"]);
    repo.git(&["branch", "--set-upstream-to=origin/kept", "kept"]);
    repo.git(&["config", "branch.deleted.remote", "origin"]);
    repo.git(&["config", "branch.deleted.merge", "refs/heads/deleted"]);
    repo.git(&["branch", "--set-upstream-to=main", "local-track"]);

    let refs = load(&SYSTEM_GIT, &repo.0).unwrap().refs;
    let find = |name: &str| {
        refs.iter()
            .find(|entry| entry.kind == RefKind::Local && entry.name == name)
            .unwrap()
    };
    assert_eq!(find("main").upstream_remote.as_deref(), Some("origin"));
    assert!(!find("main").upstream_gone);
    assert!(!find("kept").upstream_gone);
    let deleted = find("deleted");
    assert_eq!(deleted.upstream.as_deref(), Some("origin/deleted"));
    assert_eq!(deleted.upstream_remote.as_deref(), Some("origin"));
    assert!(deleted.upstream_gone);
    let local = find("local-track");
    assert_eq!(local.upstream.as_deref(), Some("main"));
    assert_eq!(local.upstream_remote, None);
    assert!(!local.upstream_gone);
}

#[test]
fn never_marks_upstreams_gone_when_the_remote_was_not_fetched() {
    let repo = committed_repo();
    repo.git(&["branch", "topic"]);
    repo.git(&[
        "config",
        "--local",
        "remote.origin.url",
        "https://example.invalid/fixture.git",
    ]);
    repo.git(&[
        "config",
        "--local",
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/origin/*",
    ]);
    repo.git(&["config", "branch.topic.remote", "origin"]);
    repo.git(&["config", "branch.topic.merge", "refs/heads/topic"]);

    let refs = load(&SYSTEM_GIT, &repo.0).unwrap().refs;
    let topic = refs
        .iter()
        .find(|entry| entry.kind == RefKind::Local && entry.name == "topic")
        .unwrap();
    assert_eq!(topic.upstream.as_deref(), Some("origin/topic"));
    assert_eq!(topic.upstream_remote.as_deref(), Some("origin"));
    assert!(!topic.upstream_gone);
}

#[test]
fn creates_branches_with_and_without_switching_and_handles_detached_head() {
    let repo = committed_repo();
    let head = repo.git(&["rev-parse", "HEAD"]);
    create(&SYSTEM_GIT, &repo.0, "feature/новая", false, None).unwrap();
    assert_eq!(git::load(&SYSTEM_GIT, &repo.0).unwrap().branch, "main");
    assert_eq!(repo.git(&["rev-parse", "feature/новая"]), head);
    create(&SYSTEM_GIT, &repo.0, "another", true, None).unwrap();
    repo.git(&["tag", "main"]);
    assert_eq!(git::load(&SYSTEM_GIT, &repo.0).unwrap().branch, "another");
    switch(&SYSTEM_GIT, &repo.0, "refs/heads/main").unwrap();
    assert_eq!(git::load(&SYSTEM_GIT, &repo.0).unwrap().branch, "main");
    assert!(switch(&SYSTEM_GIT, &repo.0, "refs/heads/main").is_err());
    repo.git(&["checkout", "--detach"]);
    assert!(load(&SYSTEM_GIT, &repo.0)
        .unwrap()
        .refs
        .iter()
        .all(|r| !r.current));
    create(&SYSTEM_GIT, &repo.0, "from-detached", true, None).unwrap();
    assert_eq!(
        git::load(&SYSTEM_GIT, &repo.0).unwrap().branch,
        "from-detached"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
}

#[test]
fn carries_staged_unstaged_and_untracked_changes_across_branch_switches() {
    let repo = committed_repo();
    create(&SYSTEM_GIT, &repo.0, "other", true, None).unwrap();
    repo.write("target-only", "from other\n");
    repo.git(&["add", "target-only"]);
    repo.git(&["commit", "-m", "Add target file"]);
    switch(&SYSTEM_GIT, &repo.0, "refs/heads/main").unwrap();

    repo.write("file", "staged\n");
    repo.git(&["add", "file"]);
    repo.write("file", "unstaged\n");
    repo.write("untracked", "keep\n");
    let status = repo.git(&["status", "--porcelain"]);
    let staged = repo.git(&["show", ":file"]);

    switch(&SYSTEM_GIT, &repo.0, "refs/heads/other").unwrap();
    assert_eq!(git::load(&SYSTEM_GIT, &repo.0).unwrap().branch, "other");
    assert_eq!(
        std::fs::read_to_string(repo.0.join("target-only")).unwrap(),
        "from other\n"
    );
    create(&SYSTEM_GIT, &repo.0, "new", true, None).unwrap();
    assert_eq!(git::load(&SYSTEM_GIT, &repo.0).unwrap().branch, "new");
    assert_eq!(repo.git(&["status", "--porcelain"]), status);
    assert_eq!(repo.git(&["show", ":file"]), staged);
    for (path, contents) in [("file", "unstaged\n"), ("untracked", "keep\n")] {
        assert_eq!(
            std::fs::read_to_string(repo.0.join(path)).unwrap(),
            contents
        );
    }
}

#[test]
fn refuses_switch_that_would_overwrite_untracked_file() {
    let repo = committed_repo();
    create(&SYSTEM_GIT, &repo.0, "other", true, None).unwrap();
    repo.write("collision", "tracked\n");
    repo.git(&["add", "collision"]);
    repo.git(&["commit", "-m", "Track collision"]);
    switch(&SYSTEM_GIT, &repo.0, "refs/heads/main").unwrap();

    repo.write("collision", "untracked\n");
    assert!(switch(&SYSTEM_GIT, &repo.0, "refs/heads/other").is_err());
    assert_eq!(git::load(&SYSTEM_GIT, &repo.0).unwrap().branch, "main");
    assert_eq!(
        std::fs::read_to_string(repo.0.join("collision")).unwrap(),
        "untracked\n"
    );
}

#[test]
fn never_overwrites_ignored_files_when_switching() {
    let repo = committed_repo();
    repo.write(".gitignore", "ignored.data\n");
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-m", "ignore"]);
    create(&SYSTEM_GIT, &repo.0, "target", true, None).unwrap();
    repo.write("ignored.data", "tracked target\n");
    repo.git(&["add", "-f", "ignored.data"]);
    repo.git(&["commit", "-m", "target file"]);
    switch(&SYSTEM_GIT, &repo.0, "refs/heads/main").unwrap();
    repo.write("ignored.data", "private local contents\n");
    assert!(git::load(&SYSTEM_GIT, &repo.0).unwrap().changes.is_empty());
    assert!(switch(&SYSTEM_GIT, &repo.0, "refs/heads/target").is_err());
    assert_eq!(
        std::fs::read_to_string(repo.0.join("ignored.data")).unwrap(),
        "private local contents\n"
    );
    assert_eq!(git::load(&SYSTEM_GIT, &repo.0).unwrap().branch, "main");
}

#[test]
fn refuses_branch_in_another_worktree_and_missing_local_targets() {
    let repo = committed_repo();
    create(&SYSTEM_GIT, &repo.0, "occupied", false, None).unwrap();
    repo.git(&[
        "worktree",
        "add",
        repo.0.join("linked").to_str().unwrap(),
        "occupied",
    ]);
    repo.write(".git/info/exclude", "linked/\n");
    assert!(git::load(&SYSTEM_GIT, &repo.0).unwrap().changes.is_empty());
    assert!(switch(&SYSTEM_GIT, &repo.0, "refs/heads/occupied").is_err());
    repo.git(&["tag", "main"]);
    repo.git(&["update-ref", "refs/remotes/origin/remote-only", "HEAD"]);
    assert!(switch(&SYSTEM_GIT, &repo.0, "refs/heads/remote-only").is_err());
    assert_eq!(git::load(&SYSTEM_GIT, &repo.0).unwrap().branch, "main");
}

#[test]
fn switches_tags_and_remote_refs_to_detached_commits_without_creating_branches() {
    let repo = committed_repo();
    let original = git::run(&SYSTEM_GIT, &repo.0, &["rev-parse", "HEAD"]).unwrap();
    repo.git(&["tag", "lightweight"]);
    repo.git(&["tag", "-a", "release", "-m", "Release"]);
    repo.git(&["update-ref", "refs/remotes/origin/remote-only", "HEAD"]);
    repo.write("file", "next\n");
    repo.git(&["commit", "-am", "next"]);
    for target in [
        "refs/tags/lightweight",
        "refs/tags/release",
        "refs/remotes/origin/remote-only",
    ] {
        switch(&SYSTEM_GIT, &repo.0, target).unwrap();
        assert_eq!(
            git::run(&SYSTEM_GIT, &repo.0, &["rev-parse", "HEAD"]).unwrap(),
            original
        );
        assert!(git::run(&SYSTEM_GIT, &repo.0, &["symbolic-ref", "-q", "HEAD"]).is_err());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "base\n"
        );
        switch(&SYSTEM_GIT, &repo.0, "refs/heads/main").unwrap();
    }
    assert_eq!(
        load(&SYSTEM_GIT, &repo.0)
            .unwrap()
            .refs
            .iter()
            .filter(|r| r.kind == RefKind::Local)
            .count(),
        1
    );
    repo.write("file", "staged edits\n");
    repo.git(&["add", "file"]);
    repo.write("file", "local edits\n");
    assert!(switch(&SYSTEM_GIT, &repo.0, "refs/tags/release").is_err());
    assert_eq!(
        std::fs::read_to_string(repo.0.join("file")).unwrap(),
        "local edits\n"
    );
    assert_eq!(repo.git(&["show", ":file"]), "staged edits\n");
    assert_eq!(git::load(&SYSTEM_GIT, &repo.0).unwrap().branch, "main");
}

#[test]
fn remote_checkout_creates_tracking_branch_at_remote_commit_and_preserves_existing() {
    let repo = committed_repo();
    repo.git(&[
        "config",
        "remote.origin.url",
        "https://example.invalid/repo",
    ]);
    repo.git(&[
        "config",
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/origin/*",
    ]);
    repo.git(&["update-ref", "refs/remotes/origin/topic", "HEAD"]);
    let remote = "refs/remotes/origin/topic";
    let original = git::run(&SYSTEM_GIT, &repo.0, &["rev-parse", "HEAD"]).unwrap();
    repo.write("file", "next\n");
    repo.git(&["commit", "-am", "next"]);
    checkout_remote(
        &SYSTEM_GIT,
        &repo.0,
        remote,
        &RemoteChoice::Create("topic".into()),
    )
    .unwrap();
    assert_eq!(
        git::run(&SYSTEM_GIT, &repo.0, &["rev-parse", "HEAD"]).unwrap(),
        original
    );
    let refs = load(&SYSTEM_GIT, &repo.0).unwrap().refs;
    let target = refs.iter().find(|r| r.full_name == remote).unwrap();
    let local = local_for_remote(&refs, target).unwrap();
    assert_eq!(local.name, "topic");
    assert_eq!(local.upstream.as_deref(), Some("origin/topic"));
    checkout_remote(
        &SYSTEM_GIT,
        &repo.0,
        remote,
        &RemoteChoice::Local("refs/heads/main".into()),
    )
    .unwrap();
    assert_eq!(git::load(&SYSTEM_GIT, &repo.0).unwrap().branch, "main");
    assert!(checkout_remote(
        &SYSTEM_GIT,
        &repo.0,
        remote,
        &RemoteChoice::Create("main".into())
    )
    .is_err());
    checkout_remote(&SYSTEM_GIT, &repo.0, remote, &RemoteChoice::Detached).unwrap();
    assert!(git::run(&SYSTEM_GIT, &repo.0, &["symbolic-ref", "-q", "HEAD"]).is_err());
    repo.write("file", "local\n");
    checkout_remote(
        &SYSTEM_GIT,
        &repo.0,
        remote,
        &RemoteChoice::Create("safe".into()),
    )
    .unwrap();
    assert_eq!(git::load(&SYSTEM_GIT, &repo.0).unwrap().branch, "safe");
    assert_eq!(
        std::fs::read_to_string(repo.0.join("file")).unwrap(),
        "local\n"
    );
}

#[test]
fn remote_local_matching_avoids_other_remotes_and_ambiguous_tracking() {
    let repo = committed_repo();
    repo.git(&["branch", "topic"]);
    repo.git(&[
        "config",
        "remote.origin.url",
        "https://example.invalid/repo",
    ]);
    repo.git(&[
        "config",
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/origin/*",
    ]);
    repo.git(&["update-ref", "refs/remotes/origin/topic", "HEAD"]);
    let mut refs = load(&SYSTEM_GIT, &repo.0).unwrap().refs;
    let remote_index = refs.iter().position(|r| r.kind == RefKind::Remote).unwrap();
    assert_eq!(
        local_for_remote(&refs, &refs[remote_index]).unwrap().name,
        "topic"
    );
    let topic = refs.iter_mut().find(|r| r.name == "topic").unwrap();
    topic.upstream = Some("other/topic".into());
    assert!(local_for_remote(&refs, &refs[remote_index]).is_none());
    for local in refs.iter_mut().filter(|r| r.kind == RefKind::Local) {
        local.upstream = Some("origin/topic".into());
    }
    assert!(local_for_remote(&refs, &refs[remote_index]).is_none());
}

#[test]
fn validates_names_rejects_duplicates_and_handles_unborn_head() {
    let repo = TempRepo::new();
    let snapshot = load(&SYSTEM_GIT, &repo.0).unwrap();
    assert!(!snapshot.has_head);
    assert!(snapshot.refs.is_empty());
    assert!(create(&SYSTEM_GIT, &repo.0, "new", false, None)
        .unwrap_err()
        .contains("first commit"));
    repo.write("file", "fixture\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fixture"]);
    for name in [
        "",
        "HEAD",
        "-option",
        "refs/heads/nested",
        "@{-1}",
        "a b",
        "a..b",
        ".hidden",
        "a.lock",
        "a\0b",
    ] {
        assert!(
            create(&SYSTEM_GIT, &repo.0, name, false, None).is_err(),
            "accepted {name:?}"
        );
    }
    assert!(create(&SYSTEM_GIT, &repo.0, "main", false, None)
        .unwrap_err()
        .contains("already exists"));
    assert_eq!(load(&SYSTEM_GIT, &repo.0).unwrap().refs.len(), 1);
}

#[test]
fn rejects_malformed_ref_records_and_bounds_ref_count() {
    let record = format!("refs/heads/main\0{}\0\0\0*\0\0\0\0\n", "a".repeat(40));
    assert_eq!(parse(record.as_bytes()).unwrap()[0].name, "main");
    assert!(parse(&record.as_bytes()[..record.len() - 1]).is_err());
    assert!(parse(b"short\0\n").is_err());
    assert!(parse(format!("{record}{record}").as_bytes()).is_err());
    assert!(parse(record.replace(&"a".repeat(40), "bad").as_bytes()).is_err());
    let mut bytes = record.into_bytes();
    bytes[12] = 255;
    assert!(parse(&bytes).is_err());
    let many: String = (0..=REF_LIMIT)
        .map(|i| format!("refs/heads/b{i}\0{}\0\0\0 \0\0\0\0\n", "a".repeat(40)))
        .collect();
    assert!(parse(many.as_bytes()).is_err_and(|error| error.contains("5,000")));
}

#[test]
fn reset_to_a_commit_id_moves_head_and_rejects_other_text() {
    let repo = committed_repo();
    let first = repo
        .git(&["rev-parse", "--verify", "HEAD"])
        .trim()
        .to_owned();
    repo.write("file", "next\n");
    repo.git(&["add", "file"]);
    repo.git(&["commit", "-m", "next"]);
    let preview = preview_reset(&SYSTEM_GIT, &repo.0, &first).unwrap();
    assert_eq!(preview.target, first);
    assert_eq!(preview.tag, first);
    reset_tag(&SYSTEM_GIT, &repo.0, &preview, ResetMode::Clean, None).unwrap();
    assert_eq!(repo.git(&["rev-parse", "--verify", "HEAD"]).trim(), first);
    assert_eq!(
        std::fs::read_to_string(repo.0.join("file")).unwrap(),
        "base\n"
    );
    assert!(preview_reset(&SYSTEM_GIT, &repo.0, "abcdef").is_err());
    assert!(preview_reset(&SYSTEM_GIT, &repo.0, &"0".repeat(40)).is_err());
}

#[test]
fn reads_the_commit_message_without_git_log_newline() {
    let repo = committed_repo();
    repo.write("file", "next\n");
    repo.git(&["add", "file"]);
    repo.git(&["commit", "-m", "subject\n\nbody line"]);
    let oid = repo.git(&["rev-parse", "--verify", "HEAD"]);
    assert_eq!(
        read_commit_message(&SYSTEM_GIT, &repo.0, oid.trim()).unwrap(),
        "subject\n\nbody line"
    );
    assert!(read_commit_message(&SYSTEM_GIT, &repo.0, "abc").is_err());
}

#[test]
fn upstream_delta_preserves_both_sides_of_divergence() {
    assert_eq!(parse_delta("ahead 2, behind 5"), Some((2, 5)));
    assert_eq!(parse_delta("behind 46"), Some((0, 46)));
    assert_eq!(parse_delta(""), Some((0, 0)));
    assert_eq!(parse_delta("gone"), None);
}
