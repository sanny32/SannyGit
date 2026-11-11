use super::*;
use crate::test_support::TempRepo;

fn committed_repo() -> TempRepo {
    let repo = TempRepo::new();
    repo.write("file", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fixture"]);
    repo
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
    let preview = preview_delete(&repo.0, "refs/heads/merged").unwrap();
    assert!(repo.git(&["branch", "--list", "merged"]).contains("merged"));
    delete(&repo.0, &preview).unwrap();
    assert!(repo.git(&["branch", "--list", "merged"]).is_empty());
    assert!(repo.git(&["branch", "--list", "keep"]).contains("keep"));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["ls-files", "--stage"]), index);
    assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
    assert_eq!(std::fs::read(repo.0.join("new")).unwrap(), b"keep new\n");
}

#[test]
fn delete_rejects_current_unmerged_worktree_and_stale_branch_targets() {
    let repo = committed_repo();
    repo.git(&["branch", "other"]);
    assert!(preview_delete(&repo.0, "refs/heads/main")
        .unwrap_err()
        .contains("current"));
    let preview = preview_delete(&repo.0, "refs/heads/other").unwrap();
    repo.git(&["commit", "--allow-empty", "-m", "HEAD advanced"]);
    assert!(delete(&repo.0, &preview).unwrap_err().contains("changed"));
    repo.git(&["switch", "other"]);
    repo.write("file", "unmerged\n");
    repo.git(&["add", "file"]);
    repo.git(&["commit", "-m", "Unique"]);
    repo.git(&["switch", "main"]);
    assert!(preview_delete(&repo.0, "refs/heads/other")
        .unwrap_err()
        .contains("not fully merged"));
    repo.git(&["branch", "worktree-branch"]);
    let linked = repo.0.join("linked");
    repo.git(&[
        "worktree",
        "add",
        linked.to_str().unwrap(),
        "worktree-branch",
    ]);
    assert!(preview_delete(&repo.0, "refs/heads/worktree-branch")
        .unwrap_err()
        .contains("worktree"));
    repo.git(&["tag", "tag"]);
    for name in [
        "refs/tags/tag",
        "refs/remotes/origin/main",
        "other",
        "--all",
    ] {
        assert!(preview_delete(&repo.0, name).is_err());
    }
    assert!(repo.git(&["branch", "--list", "other"]).contains("other"));
}

#[test]
fn delete_rechecks_tip_worktree_occupancy_and_unfinished_operations_after_review() {
    let repo = committed_repo();
    repo.git(&["commit", "--allow-empty", "-m", "Next"]);
    repo.git(&["branch", "merged", "HEAD^"]);
    let preview = preview_delete(&repo.0, "refs/heads/merged").unwrap();
    repo.git(&["branch", "-f", "merged", "HEAD"]);
    assert!(delete(&repo.0, &preview).unwrap_err().contains("changed"));
    let preview = preview_delete(&repo.0, "refs/heads/merged").unwrap();
    repo.write(".git/MERGE_HEAD", "busy");
    assert!(delete(&repo.0, &preview).unwrap_err().contains("Finish"));
    std::fs::remove_file(repo.0.join(".git/MERGE_HEAD")).unwrap();
    let path = repo.0.join("linked");
    repo.git(&["worktree", "add", path.to_str().unwrap(), "merged"]);
    assert!(delete(&repo.0, &preview).unwrap_err().contains("worktree"));
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
    create_tag(&repo.0, "release/версия", " \n").unwrap();
    assert_eq!(
        repo.git(&["cat-file", "-t", "refs/tags/release/версия"])
            .trim(),
        "commit"
    );
    repo.git(&["config", "tag.gpgSign", "false"]);
    create_tag(&repo.0, "v1.0", "Release\n\nDetails #1\n").unwrap();
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
    create_tag(&repo.0, "detached", "").unwrap();
    assert_eq!(repo.git(&["rev-parse", "refs/tags/detached"]), head);
}

#[test]
fn tag_creation_rejects_invalid_names_duplicates_messages_and_unborn_head() {
    let empty = TempRepo::new();
    assert!(create_tag(&empty.0, "v1", "")
        .unwrap_err()
        .contains("first commit"));
    let repo = committed_repo();
    create_tag(&repo.0, "v1", "Original").unwrap();
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
            create_tag(&repo.0, name, "replacement").is_err(),
            "{name:?}"
        );
    }
    assert!(create_tag(&repo.0, "bad-message", "bad\0message").is_err());
    assert!(create_tag(&repo.0, "huge", &"x".repeat(1024 * 1024 + 1)).is_err());
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
    rename(&repo.0, "refs/heads/main", "feature/новая").unwrap();
    assert_eq!(git::load(&repo.0).unwrap().branch, "feature/новая");
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
    assert!(!load(&repo.0)
        .unwrap()
        .refs
        .iter()
        .any(|r| r.full_name == "refs/heads/main"));
    create(&repo.0, "other", false).unwrap();
    rename(&repo.0, "refs/heads/other", "renamed-other").unwrap();
    assert_eq!(git::load(&repo.0).unwrap().branch, "feature/новая");
    assert_eq!(repo.git(&["rev-parse", "renamed-other"]), head);
}

#[test]
fn rename_rejects_invalid_missing_nonlocal_and_existing_targets_without_writes() {
    let repo = committed_repo();
    create(&repo.0, "existing", false).unwrap();
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
            rename(&repo.0, "refs/heads/main", name).is_err(),
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
        assert!(rename(&repo.0, source, "new-name").is_err());
    }
    assert_eq!(repo.git(&["show-ref"]), before);
    assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
    assert_eq!(git::load(&repo.0).unwrap().branch, "main");
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
    repo.git(&["tag", "-a", "release", "-m", "Release"]);
    let index = std::fs::read(repo.0.join(".git/index")).unwrap();
    let snapshot = load(&repo.0).unwrap();
    assert!(snapshot.has_head);
    assert_eq!(snapshot.refs.len(), 5);
    let current = snapshot.refs.iter().find(|r| r.current).unwrap();
    assert_eq!(current.full_name, "refs/heads/main");
    assert_eq!(current.upstream.as_deref(), Some("origin/main"));
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
    assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
}

#[test]
fn creates_branches_with_and_without_switching_and_handles_detached_head() {
    let repo = committed_repo();
    let head = repo.git(&["rev-parse", "HEAD"]);
    create(&repo.0, "feature/новая", false).unwrap();
    assert_eq!(git::load(&repo.0).unwrap().branch, "main");
    assert_eq!(repo.git(&["rev-parse", "feature/новая"]), head);
    create(&repo.0, "another", true).unwrap();
    repo.git(&["tag", "main"]);
    assert_eq!(git::load(&repo.0).unwrap().branch, "another");
    switch(&repo.0, "refs/heads/main").unwrap();
    assert_eq!(git::load(&repo.0).unwrap().branch, "main");
    assert!(switch(&repo.0, "refs/heads/main").is_err());
    repo.git(&["checkout", "--detach"]);
    assert!(load(&repo.0).unwrap().refs.iter().all(|r| !r.current));
    create(&repo.0, "from-detached", true).unwrap();
    assert_eq!(git::load(&repo.0).unwrap().branch, "from-detached");
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
}

#[test]
fn refuses_dirty_switches_and_preserves_index_and_worktree() {
    let repo = committed_repo();
    create(&repo.0, "other", false).unwrap();
    for staged in [false, true] {
        repo.write("file", "unsaved\n");
        if staged {
            repo.git(&["add", "file"]);
        }
        let index = std::fs::read(repo.0.join(".git/index")).unwrap();
        assert!(switch(&repo.0, "refs/heads/other").is_err());
        assert_eq!(git::load(&repo.0).unwrap().branch, "main");
        assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "unsaved\n"
        );
        repo.git(&["restore", "--staged", "--worktree", "--", "file"]);
    }
    repo.write("untracked", "keep\n");
    assert!(switch(&repo.0, "refs/heads/other").is_err());
    assert!(create(&repo.0, "must-not-exist", true).is_err());
    assert!(!load(&repo.0)
        .unwrap()
        .refs
        .iter()
        .any(|r| r.name == "must-not-exist"));
    create(&repo.0, "without-switch", false).unwrap();
    assert_eq!(git::load(&repo.0).unwrap().branch, "main");
    assert_eq!(
        std::fs::read_to_string(repo.0.join("untracked")).unwrap(),
        "keep\n"
    );
}

#[test]
fn never_overwrites_ignored_files_when_switching() {
    let repo = committed_repo();
    repo.write(".gitignore", "ignored.data\n");
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-m", "ignore"]);
    create(&repo.0, "target", true).unwrap();
    repo.write("ignored.data", "tracked target\n");
    repo.git(&["add", "-f", "ignored.data"]);
    repo.git(&["commit", "-m", "target file"]);
    switch(&repo.0, "refs/heads/main").unwrap();
    repo.write("ignored.data", "private local contents\n");
    assert!(git::load(&repo.0).unwrap().changes.is_empty());
    assert!(switch(&repo.0, "refs/heads/target").is_err());
    assert_eq!(
        std::fs::read_to_string(repo.0.join("ignored.data")).unwrap(),
        "private local contents\n"
    );
    assert_eq!(git::load(&repo.0).unwrap().branch, "main");
}

#[test]
fn refuses_branch_in_another_worktree_and_missing_local_targets() {
    let repo = committed_repo();
    create(&repo.0, "occupied", false).unwrap();
    repo.git(&[
        "worktree",
        "add",
        repo.0.join("linked").to_str().unwrap(),
        "occupied",
    ]);
    repo.write(".git/info/exclude", "linked/\n");
    assert!(git::load(&repo.0).unwrap().changes.is_empty());
    assert!(switch(&repo.0, "refs/heads/occupied").is_err());
    repo.git(&["tag", "main"]);
    repo.git(&["update-ref", "refs/remotes/origin/remote-only", "HEAD"]);
    assert!(switch(&repo.0, "refs/heads/remote-only").is_err());
    assert_eq!(git::load(&repo.0).unwrap().branch, "main");
}

#[test]
fn switches_tags_and_remote_refs_to_detached_commits_without_creating_branches() {
    let repo = committed_repo();
    let original = git::run(&repo.0, &["rev-parse", "HEAD"]).unwrap();
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
        switch(&repo.0, target).unwrap();
        assert_eq!(git::run(&repo.0, &["rev-parse", "HEAD"]).unwrap(), original);
        assert!(git::run(&repo.0, &["symbolic-ref", "-q", "HEAD"]).is_err());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "base\n"
        );
        switch(&repo.0, "refs/heads/main").unwrap();
    }
    assert_eq!(
        load(&repo.0)
            .unwrap()
            .refs
            .iter()
            .filter(|r| r.kind == RefKind::Local)
            .count(),
        1
    );
    repo.write("file", "local edits\n");
    assert!(switch(&repo.0, "refs/tags/release").is_err());
    assert_eq!(
        std::fs::read_to_string(repo.0.join("file")).unwrap(),
        "local edits\n"
    );
    assert_eq!(git::load(&repo.0).unwrap().branch, "main");
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
    let original = git::run(&repo.0, &["rev-parse", "HEAD"]).unwrap();
    repo.write("file", "next\n");
    repo.git(&["commit", "-am", "next"]);
    checkout_remote(&repo.0, remote, &RemoteChoice::Create("topic".into())).unwrap();
    assert_eq!(git::run(&repo.0, &["rev-parse", "HEAD"]).unwrap(), original);
    let refs = load(&repo.0).unwrap().refs;
    let target = refs.iter().find(|r| r.full_name == remote).unwrap();
    let local = local_for_remote(&refs, target).unwrap();
    assert_eq!(local.name, "topic");
    assert_eq!(local.upstream.as_deref(), Some("origin/topic"));
    checkout_remote(
        &repo.0,
        remote,
        &RemoteChoice::Local("refs/heads/main".into()),
    )
    .unwrap();
    assert_eq!(git::load(&repo.0).unwrap().branch, "main");
    assert!(checkout_remote(&repo.0, remote, &RemoteChoice::Create("main".into())).is_err());
    checkout_remote(&repo.0, remote, &RemoteChoice::Detached).unwrap();
    assert!(git::run(&repo.0, &["symbolic-ref", "-q", "HEAD"]).is_err());
    repo.write("file", "local\n");
    assert!(checkout_remote(&repo.0, remote, &RemoteChoice::Create("safe".into())).is_err());
    assert!(!load(&repo.0).unwrap().refs.iter().any(|r| r.name == "safe"));
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
    let mut refs = load(&repo.0).unwrap().refs;
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
    let snapshot = load(&repo.0).unwrap();
    assert!(!snapshot.has_head);
    assert!(snapshot.refs.is_empty());
    assert!(create(&repo.0, "new", false)
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
        assert!(create(&repo.0, name, false).is_err(), "accepted {name:?}");
    }
    assert!(create(&repo.0, "main", false)
        .unwrap_err()
        .contains("already exists"));
    assert_eq!(load(&repo.0).unwrap().refs.len(), 1);
}

#[test]
fn rejects_malformed_ref_records_and_bounds_ref_count() {
    let record = format!("refs/heads/main\0{}\0\0*\0\0\n", "a".repeat(40));
    assert_eq!(parse(record.as_bytes()).unwrap()[0].name, "main");
    assert!(parse(&record.as_bytes()[..record.len() - 1]).is_err());
    assert!(parse(b"short\0\n").is_err());
    assert!(parse(format!("{record}{record}").as_bytes()).is_err());
    assert!(parse(record.replace(&"a".repeat(40), "bad").as_bytes()).is_err());
    let mut bytes = record.into_bytes();
    bytes[12] = 255;
    assert!(parse(&bytes).is_err());
    let many: String = (0..=REF_LIMIT)
        .map(|i| format!("refs/heads/b{i}\0{}\0\0 \0\0\n", "a".repeat(40)))
        .collect();
    assert!(parse(many.as_bytes()).is_err_and(|error| error.contains("5,000")));
}
