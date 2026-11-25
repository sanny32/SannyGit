use super::*;
use crate::test_support::TempRepo;
use crate::test_support::SYSTEM_GIT;
use std::{fs, path::PathBuf};

#[test]
fn tag_delete_defaults_to_local_and_can_delete_from_all_reviewed_remotes() {
    let repo = TempRepo::new();
    repo.write("file", "base\n");
    repo.git(&["add", "file"]);
    repo.git(&["commit", "-m", "base"]);
    repo.git(&["tag", "-a", "release", "-m", "release notes"]);
    repo.git(&["branch", "release"]);
    let reference = "refs/tags/release";
    let oid = repo.git(&["rev-parse", reference]);
    let origin = repo.0.join("origin.git");
    let backup = repo.0.join("backup.git");
    for (name, remote) in [("origin", &origin), ("backup", &backup)] {
        repo.git(&["init", "--bare", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", name, remote.to_str().unwrap()]);
        repo.git(&["push", name, reference, "refs/heads/release"]);
    }
    let cancel = AtomicBool::new(false);
    let first = preview_tag_delete(&SYSTEM_GIT, &repo.0, reference).unwrap();
    delete_tag(&SYSTEM_GIT, &repo.0, &first, false, &cancel).unwrap();
    assert!(repo.git(&["tag", "--list"]).is_empty());
    for remote in [&origin, &backup] {
        assert_eq!(
            git::text(git::run(&SYSTEM_GIT, remote, &["rev-parse", reference]).unwrap()).unwrap(),
            oid
        );
    }
    repo.git(&[
        "fetch",
        "--no-tags",
        "origin",
        "refs/tags/release:refs/tags/release",
    ]);
    let reviewed = preview_tag_delete(&SYSTEM_GIT, &repo.0, reference).unwrap();
    delete_tag(&SYSTEM_GIT, &repo.0, &reviewed, true, &cancel).unwrap();
    assert!(repo.git(&["tag", "--list"]).is_empty());
    assert_eq!(repo.git(&["branch", "--list", "release"]).trim(), "release");
    for remote in [&origin, &backup] {
        assert!(git::run(&SYSTEM_GIT, remote, &["tag", "--list"])
            .unwrap()
            .is_empty());
        assert!(git::run(
            &SYSTEM_GIT,
            remote,
            &["rev-parse", "--verify", "refs/heads/release"]
        )
        .is_ok());
    }
}

#[test]
fn tag_delete_rejects_stale_local_and_remote_objects_before_removing_any_ref() {
    let repo = TempRepo::new();
    repo.git(&["commit", "--allow-empty", "-m", "base"]);
    repo.git(&["tag", "release"]);
    let reference = "refs/tags/release";
    let original = repo.git(&["rev-parse", reference]);
    let origin = repo.0.join("origin.git");
    let backup = repo.0.join("backup.git");
    for (name, remote) in [("origin", &origin), ("backup", &backup)] {
        repo.git(&["init", "--bare", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", name, remote.to_str().unwrap()]);
        repo.git(&["push", name, reference]);
    }
    let reviewed = preview_tag_delete(&SYSTEM_GIT, &repo.0, reference).unwrap();
    let cancel = AtomicBool::new(false);
    repo.git(&["commit", "--allow-empty", "-m", "different object"]);
    repo.git(&["tag", "-f", "release"]);
    assert!(delete_tag(&SYSTEM_GIT, &repo.0, &reviewed, false, &cancel)
        .unwrap_err()
        .contains("local tag changed"));
    repo.git(&["push", "--force", "origin", reference]);
    repo.git(&["tag", "-f", "release", original.trim()]);
    assert!(delete_tag(&SYSTEM_GIT, &repo.0, &reviewed, true, &cancel)
        .unwrap_err()
        .contains("different object"));
    assert_eq!(repo.git(&["rev-parse", reference]), original);
    assert_eq!(
        git::text(git::run(&SYSTEM_GIT, &backup, &["rev-parse", reference]).unwrap()).unwrap(),
        original
    );
}

fn command(root: &Path, args: &[&str]) -> Vec<u8> {
    let mut configured = vec!["-c", "core.autocrlf=false"];
    configured.extend_from_slice(args);
    git::run(&SYSTEM_GIT, root, &configured).unwrap()
}
fn identity(root: &Path) {
    command(root, &["config", "core.autocrlf", "false"]);
    command(root, &["config", "user.name", "Remote Test"]);
    command(
        root,
        &["config", "user.email", "remote-test@example.invalid"],
    );
}
fn commit(root: &Path, text: &str) {
    fs::write(root.join("file"), text).unwrap();
    command(root, &["add", "file"]);
    command(root, &["commit", "-m", text]);
}
fn fixture() -> (TempRepo, PathBuf, PathBuf) {
    let temp = TempRepo::new();
    let remote = temp.0.join("remote.git");
    let source = temp.0.join("source");
    let other = temp.0.join("other");
    command(
        &temp.0,
        &[
            "init",
            "--bare",
            "--initial-branch=main",
            remote.to_str().unwrap(),
        ],
    );
    command(
        &temp.0,
        &[
            "clone",
            "--",
            remote.to_str().unwrap(),
            source.to_str().unwrap(),
        ],
    );
    identity(&source);
    commit(&source, "base\n");
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    command(
        &temp.0,
        &[
            "clone",
            "--",
            remote.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    identity(&other);
    (temp, source, other)
}

#[test]
fn tag_push_targets_one_reviewed_object_and_ignores_unsafe_push_defaults() {
    let (temp, source, _other) = fixture();
    command(&source, &["tag", "-a", "chosen", "-m", "Chosen"]);
    command(&source, &["tag", "-a", "private", "-m", "Private"]);
    command(&source, &["branch", "private-branch"]);
    command(&source, &["config", "remote.origin.push", "+refs/*:refs/*"]);
    command(&source, &["config", "remote.origin.mirror", "true"]);
    command(&source, &["config", "push.followTags", "true"]);
    let before = command(&source, &["ls-files", "--stage"]);
    fs::write(source.join("file"), "local edits\n").unwrap();
    let preview = preview_tag_push(&SYSTEM_GIT, &source, "refs/tags/chosen", "origin").unwrap();
    push_tag(&SYSTEM_GIT, &source, &preview, &AtomicBool::new(false)).unwrap();
    let remote = temp.0.join("remote.git");
    assert_eq!(
        git::text(command(&remote, &["rev-parse", "refs/tags/chosen"]))
            .unwrap()
            .trim(),
        preview.oid
    );
    assert_eq!(
        git::text(command(&remote, &["tag", "--list"]))
            .unwrap()
            .trim(),
        "chosen"
    );
    assert!(!git::text(command(&remote, &["branch", "--list"]))
        .unwrap()
        .contains("private-branch"));
    assert_eq!(command(&source, &["ls-files", "--stage"]), before);
    assert_eq!(fs::read(source.join("file")).unwrap(), b"local edits\n");
    // An existing remote tag is never overwritten, even with forced refspec defaults.
    command(&source, &["tag", "-f", "chosen", "HEAD"]);
    let replacement = preview_tag_push(&SYSTEM_GIT, &source, "refs/tags/chosen", "origin").unwrap();
    assert!(push_tag(&SYSTEM_GIT, &source, &replacement, &AtomicBool::new(false)).is_err());
    assert_eq!(
        git::text(command(&remote, &["rev-parse", "refs/tags/chosen"]))
            .unwrap()
            .trim(),
        preview.oid
    );
}

#[test]
fn tag_push_rejects_stale_refs_endpoints_fanout_and_cancellation() {
    let (temp, source, _) = fixture();
    command(&source, &["tag", "chosen"]);
    let preview = preview_tag_push(&SYSTEM_GIT, &source, "refs/tags/chosen", "origin").unwrap();
    assert!(push_tag(&SYSTEM_GIT, &source, &preview, &AtomicBool::new(true)).is_err());
    command(
        &source,
        &[
            "config",
            "remote.origin.pushurl",
            "https://example.invalid/new.git",
        ],
    );
    assert!(
        push_tag(&SYSTEM_GIT, &source, &preview, &AtomicBool::new(false))
            .unwrap_err()
            .contains("changed")
    );
    command(
        &source,
        &[
            "config",
            "--add",
            "remote.origin.pushurl",
            "https://example.invalid/second.git",
        ],
    );
    assert!(
        preview_tag_push(&SYSTEM_GIT, &source, "refs/tags/chosen", "origin")
            .err()
            .unwrap()
            .contains("exactly one")
    );
    command(&source, &["config", "--unset-all", "remote.origin.pushurl"]);
    command(&source, &["tag", "-d", "chosen"]);
    assert!(push_tag(&SYSTEM_GIT, &source, &preview, &AtomicBool::new(false)).is_err());
    assert!(command(&temp.0.join("remote.git"), &["tag", "--list"]).is_empty());
}

#[test]
fn tag_push_local_path_cannot_be_reinterpreted_as_another_configured_remote() {
    let (_temp, source, _) = fixture();
    let destination = source.join("destination");
    let victim = source.join("victim");
    for path in [&destination, &victim] {
        command(&source, &["init", "--bare", path.to_str().unwrap()]);
    }
    command(&source, &["remote", "set-url", "origin", "destination"]);
    command(
        &source,
        &["remote", "add", "destination", victim.to_str().unwrap()],
    );
    command(&source, &["tag", "chosen"]);
    let preview = preview_tag_push(&SYSTEM_GIT, &source, "refs/tags/chosen", "origin").unwrap();
    assert!(preview.endpoint_display.starts_with("file:"));
    push_tag(&SYSTEM_GIT, &source, &preview, &AtomicBool::new(false)).unwrap();
    assert!(git::text(command(&destination, &["tag", "--list"]))
        .unwrap()
        .contains("chosen"));
    assert!(command(&victim, &["tag", "--list"]).is_empty());
}

#[test]
fn explicit_push_and_fetch_preserve_worktree_and_do_not_push_tags_or_other_branches() {
    let (temp, source, other) = fixture();
    let state = load(&SYSTEM_GIT, &other).unwrap();
    assert_eq!(state.branch.as_deref(), Some("main"));
    assert_eq!(state.upstream_remote.as_deref(), Some("origin"));
    assert_eq!(state.upstream_ref.as_deref(), Some("main"));
    command(&source, &["branch", "private"]);
    commit(&source, "next\n");
    command(&source, &["tag", "-a", "v1", "-m", "private tag"]);
    command(&source, &["config", "push.followTags", "true"]);
    command(
        &source,
        &["config", "remote.origin.push", "+refs/heads/*:refs/heads/*"],
    );
    command(&source, &["config", "remote.origin.mirror", "true"]);
    assert!(run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false)
    )
    .is_err());
    command(&source, &["config", "--unset", "remote.origin.mirror"]);
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    let remote = temp.0.join("remote.git");
    assert!(command(&remote, &["tag", "--list"]).is_empty());
    assert!(!git::text(command(
        &remote,
        &["for-each-ref", "--format=%(refname)", "refs/heads"]
    ))
    .unwrap()
    .contains("private"));
    let old = head(&SYSTEM_GIT, &other).unwrap();
    fs::write(other.join("untracked"), "keep").unwrap();
    run(
        &SYSTEM_GIT,
        &other,
        Operation::Fetch,
        "origin",
        "",
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(head(&SYSTEM_GIT, &other).unwrap(), old);
    assert_eq!(fs::read_to_string(other.join("file")).unwrap(), "base\n");
    assert_eq!(fs::read_to_string(other.join("untracked")).unwrap(), "keep");
    assert_eq!(
        command(&other, &["rev-parse", "origin/main"]),
        command(&source, &["rev-parse", "HEAD"])
    );
    assert!(run(
        &SYSTEM_GIT,
        &other,
        Operation::Fetch,
        "origin",
        "",
        None,
        &AtomicBool::new(true)
    )
    .is_err());
}

#[test]
fn sync_pushes_then_pulls_only_the_selected_branch_and_is_repeatable() {
    let (temp, source, other) = fixture();
    let cancel = AtomicBool::new(false);
    commit(&source, "remote update\n");
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &cancel,
    )
    .unwrap();
    run(
        &SYSTEM_GIT,
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &cancel,
    )
    .unwrap();
    assert_eq!(
        head(&SYSTEM_GIT, &other).unwrap(),
        head(&SYSTEM_GIT, &source).unwrap()
    );
    assert_eq!(
        fs::read_to_string(other.join("file")).unwrap(),
        "remote update\n"
    );
    commit(&other, "local update\n");
    command(&other, &["branch", "private"]);
    command(&other, &["tag", "-a", "private-tag", "-m", "private"]);
    command(&other, &["config", "push.followTags", "true"]);
    command(
        &other,
        &["config", "remote.origin.push", "+refs/heads/*:refs/heads/*"],
    );
    for _ in 0..2 {
        run(
            &SYSTEM_GIT,
            &other,
            Operation::Sync,
            "origin",
            "main",
            Some("main"),
            &cancel,
        )
        .unwrap();
        assert_eq!(
            head(&SYSTEM_GIT, &temp.0.join("remote.git")).unwrap(),
            head(&SYSTEM_GIT, &other).unwrap()
        );
    }
    assert_eq!(
        git::text(command(
            &temp.0.join("remote.git"),
            &[
                "for-each-ref",
                "--format=%(refname)",
                "refs/heads",
                "refs/tags"
            ]
        ))
        .unwrap()
        .trim(),
        "refs/heads/main"
    );
}

#[test]
fn sync_refuses_dirty_diverged_stale_and_cancelled_operations_without_pushing() {
    let (temp, source, other) = fixture();
    let cancel = AtomicBool::new(false);
    let remote = temp.0.join("remote.git");
    let base = head(&SYSTEM_GIT, &remote).unwrap();
    fs::write(other.join("file"), "keep local edits\n").unwrap();
    assert!(run(
        &SYSTEM_GIT,
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &cancel
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(other.join("file")).unwrap(),
        "keep local edits\n"
    );
    fs::write(other.join("file"), "base\n").unwrap();
    commit(&other, "local commit\n");
    let local = head(&SYSTEM_GIT, &other).unwrap();
    assert!(run(
        &SYSTEM_GIT,
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("other"),
        &cancel
    )
    .is_err());
    assert!(run(
        &SYSTEM_GIT,
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(true)
    )
    .is_err());
    assert_eq!(head(&SYSTEM_GIT, &remote).unwrap(), base);
    commit(&source, "remote commit\n");
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &cancel,
    )
    .unwrap();
    let error = run(
        &SYSTEM_GIT,
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &cancel,
    )
    .unwrap_err();
    assert!(error.contains("Sync requires a fast-forward"));
    assert_eq!(head(&SYSTEM_GIT, &other).unwrap(), local);
    assert_eq!(
        head(&SYSTEM_GIT, &remote).unwrap(),
        head(&SYSTEM_GIT, &source).unwrap()
    );
}

#[test]
fn sync_pushes_before_fetch_and_keeps_published_commits_on_fetch_failure() {
    let (temp, _, other) = fixture();
    commit(&other, "local update\n");
    command(&other, &["config", "remote.origin.uploadpack", "false"]);
    assert!(run(
        &SYSTEM_GIT,
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(
        head(&SYSTEM_GIT, &temp.0.join("remote.git")).unwrap(),
        head(&SYSTEM_GIT, &other).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn sync_does_not_pull_after_a_hook_rejects_push() {
    use std::os::unix::fs::PermissionsExt;
    let (temp, source, other) = fixture();
    commit(&source, "remote update\n");
    push_tip(
        &SYSTEM_GIT,
        &source,
        "origin",
        "refs/heads/main",
        &head(&SYSTEM_GIT, &source).unwrap(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let original = head(&SYSTEM_GIT, &other).unwrap();
    let hook = other.join(".git/hooks/pre-push");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(run(
        &SYSTEM_GIT,
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(head(&SYSTEM_GIT, &other).unwrap(), original);
    assert_eq!(
        head(&SYSTEM_GIT, &temp.0.join("remote.git")).unwrap(),
        head(&SYSTEM_GIT, &source).unwrap()
    );
}

#[test]
fn sync_does_not_hide_rejection_from_a_different_push_endpoint() {
    let (temp, source, other) = fixture();
    let destination = temp.0.join("push.git");
    command(
        &temp.0,
        &[
            "clone",
            "--bare",
            source.to_str().unwrap(),
            destination.to_str().unwrap(),
        ],
    );
    commit(&source, "push endpoint update\n");
    command(
        &source,
        &[
            "push",
            destination.to_str().unwrap(),
            "HEAD:refs/heads/main",
        ],
    );
    command(
        &other,
        &[
            "config",
            "remote.origin.pushurl",
            destination.to_str().unwrap(),
        ],
    );
    let original = head(&SYSTEM_GIT, &other).unwrap();
    let error = run(
        &SYSTEM_GIT,
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(error.contains("URLs differ"));
    assert_eq!(head(&SYSTEM_GIT, &other).unwrap(), original);
}

#[test]
fn sync_stops_on_transport_failure_before_pulling() {
    let (_temp, source, other) = fixture();
    let cancel = AtomicBool::new(false);
    commit(&source, "remote update\n");
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &cancel,
    )
    .unwrap();
    let original = head(&SYSTEM_GIT, &other).unwrap();
    // A broken push endpoint must stop Sync before fetch or local updates.
    command(
        &other,
        &[
            "config",
            "remote.origin.pushurl",
            other.join("missing.git").to_str().unwrap(),
        ],
    );
    let error = run(
        &SYSTEM_GIT,
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &cancel,
    )
    .unwrap_err();
    assert!(error.contains("push failed"));
    assert_eq!(head(&SYSTEM_GIT, &other).unwrap(), original);
}

#[test]
fn pull_fast_forwards_and_refuses_dirty_or_diverged_worktrees() {
    let (_temp, source, other) = fixture();
    commit(&source, "next\n");
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    fs::write(other.join("file"), "local edits\n").unwrap();
    let dirty = head(&SYSTEM_GIT, &other).unwrap();
    let error = run(
        &SYSTEM_GIT,
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        error.contains("fast-forward") || error.contains("merge"),
        "{error}"
    );
    assert!(!error.contains("move aside"), "{error}");
    assert_eq!(head(&SYSTEM_GIT, &other).unwrap(), dirty);
    assert_eq!(
        fs::read_to_string(other.join("file")).unwrap(),
        "local edits\n"
    );
    fs::write(other.join("file"), "base\n").unwrap();
    run(
        &SYSTEM_GIT,
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        head(&SYSTEM_GIT, &other).unwrap(),
        head(&SYSTEM_GIT, &source).unwrap()
    );
    assert_eq!(fs::read_to_string(other.join("file")).unwrap(), "next\n");
    commit(&other, "local commit\n");
    commit(&source, "remote commit\n");
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    let old = head(&SYSTEM_GIT, &other).unwrap();
    run(
        &SYSTEM_GIT,
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(head(&SYSTEM_GIT, &other).unwrap(), old);
    assert!(other.join(".git/MERGE_HEAD").is_file());
    let merged = fs::read_to_string(other.join("file")).unwrap();
    assert!(merged.contains("<<<<<<<"), "{merged}");
    assert!(merged.contains("local commit"), "{merged}");
    assert!(merged.contains("remote commit"), "{merged}");
}

#[test]
fn pull_fast_forward_keeps_unrelated_local_changes() {
    let (_temp, source, other) = fixture();
    fs::write(source.join("stable.txt"), "stable\n").unwrap();
    command(&source, &["add", "stable.txt"]);
    command(&source, &["commit", "-m", "stable"]);
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    run(
        &SYSTEM_GIT,
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    commit(&source, "next\n");
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    fs::write(other.join("stable.txt"), "local stable\n").unwrap();
    fs::write(other.join("extra.txt"), "staged extra\n").unwrap();
    command(&other, &["add", "extra.txt"]);
    fs::write(other.join("scratch.txt"), "untracked\n").unwrap();
    run(
        &SYSTEM_GIT,
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        head(&SYSTEM_GIT, &other).unwrap(),
        head(&SYSTEM_GIT, &source).unwrap()
    );
    assert_eq!(fs::read_to_string(other.join("file")).unwrap(), "next\n");
    assert_eq!(
        fs::read_to_string(other.join("stable.txt")).unwrap(),
        "local stable\n"
    );
    assert_eq!(
        fs::read_to_string(other.join("extra.txt")).unwrap(),
        "staged extra\n"
    );
    assert_eq!(
        fs::read_to_string(other.join("scratch.txt")).unwrap(),
        "untracked\n"
    );
    let status = String::from_utf8(command(&other, &["status", "--porcelain"])).unwrap();
    assert!(status.contains(" M stable.txt"), "{status}");
    assert!(status.contains("A  extra.txt"), "{status}");
    assert!(status.contains("?? scratch.txt"), "{status}");
}

#[test]
fn pull_refuses_to_overwrite_an_untracked_file() {
    let (_temp, source, other) = fixture();
    fs::write(other.join("incoming.txt"), "mine\n").unwrap();
    fs::write(source.join("incoming.txt"), "theirs\n").unwrap();
    command(&source, &["add", "incoming.txt"]);
    command(&source, &["commit", "-m", "add incoming"]);
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    let before = head(&SYSTEM_GIT, &other).unwrap();
    assert!(run(
        &SYSTEM_GIT,
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(head(&SYSTEM_GIT, &other).unwrap(), before);
    assert_eq!(
        fs::read_to_string(other.join("incoming.txt")).unwrap(),
        "mine\n"
    );
}

#[test]
fn pull_rebase_still_requires_a_clean_worktree() {
    let (_temp, source, other) = fixture();
    commit(&source, "remote commit\n");
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    fs::write(other.join("notes.txt"), "local\n").unwrap();
    let before = head(&SYSTEM_GIT, &other).unwrap();
    let error = run_with_tags(
        &SYSTEM_GIT,
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
        None,
        Some(PullStrategy::Rebase),
    )
    .unwrap_err();
    assert!(error.contains("move aside"), "{error}");
    assert_eq!(head(&SYSTEM_GIT, &other).unwrap(), before);
    assert_eq!(fs::read_to_string(other.join("notes.txt")).unwrap(), "local\n");
}

#[test]
fn pull_merges_diverged_branches_and_keeps_unrelated_local_changes() {
    let (_temp, source, other) = fixture();
    fs::write(other.join("local.txt"), "local\n").unwrap();
    command(&other, &["add", "local.txt"]);
    command(&other, &["commit", "-m", "local only"]);
    let local = head(&SYSTEM_GIT, &other).unwrap();
    fs::write(other.join("local.txt"), "local dirty\n").unwrap();
    fs::write(other.join("scratch.txt"), "untracked\n").unwrap();
    commit(&source, "remote commit\n");
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    let remote = head(&SYSTEM_GIT, &source).unwrap();
    command(&other, &["config", "merge.ff", "only"]);
    run(
        &SYSTEM_GIT,
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    let head = head(&SYSTEM_GIT, &other).unwrap();
    assert_ne!(head, local);
    assert_ne!(head, remote);
    let parents = String::from_utf8(command(
        &other,
        &["rev-list", "--parents", "-n", "1", "HEAD"],
    ))
    .unwrap();
    assert!(parents.contains(local.trim()), "{parents}");
    assert!(parents.contains(remote.trim()), "{parents}");
    let subject = String::from_utf8(command(&other, &["log", "-1", "--format=%s"])).unwrap();
    assert!(
        subject.contains("Merge remote-tracking branch 'origin/main'"),
        "{subject}"
    );
    assert_eq!(
        fs::read_to_string(other.join("file")).unwrap(),
        "remote commit\n"
    );
    assert_eq!(
        fs::read_to_string(other.join("local.txt")).unwrap(),
        "local dirty\n"
    );
    assert_eq!(
        fs::read_to_string(other.join("scratch.txt")).unwrap(),
        "untracked\n"
    );
    assert!(!other.join(".git/MERGE_HEAD").exists());
}

#[test]
fn pull_rebase_replays_local_commits_when_branches_diverged() {
    let (_temp, source, other) = fixture();
    fs::write(other.join("local.txt"), "local\n").unwrap();
    command(&other, &["add", "local.txt"]);
    command(&other, &["commit", "-m", "local only"]);
    commit(&source, "remote commit\n");
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    let remote = head(&SYSTEM_GIT, &source).unwrap();
    run_with_tags(
        &SYSTEM_GIT,
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
        None,
        Some(PullStrategy::Rebase),
    )
    .unwrap();
    assert_ne!(head(&SYSTEM_GIT, &other).unwrap(), remote);
    assert!(git::is_ancestor(
        &SYSTEM_GIT,
        &other,
        &remote,
        &head(&SYSTEM_GIT, &other).unwrap()
    )
    .unwrap());
    assert_eq!(
        fs::read_to_string(other.join("file")).unwrap(),
        "remote commit\n"
    );
    assert_eq!(
        fs::read_to_string(other.join("local.txt")).unwrap(),
        "local\n"
    );
}

#[test]
fn pull_preserves_ignored_files_and_refuses_in_progress_operations() {
    let (_temp, source, other) = fixture();
    let original = head(&SYSTEM_GIT, &other).unwrap();
    fs::write(other.join(".git/MERGE_HEAD"), format!("{original}\n")).unwrap();
    assert!(run(
        &SYSTEM_GIT,
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false)
    )
    .unwrap_err()
    .contains("in-progress"));
    fs::remove_file(other.join(".git/MERGE_HEAD")).unwrap();
    fs::write(other.join(".git/info/exclude"), "secret\n").unwrap();
    fs::write(other.join("secret"), "private local contents\n").unwrap();
    fs::write(source.join("secret"), "remote contents\n").unwrap();
    command(&source, &["add", "secret"]);
    command(&source, &["commit", "-m", "remote file"]);
    run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(run(
        &SYSTEM_GIT,
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(head(&SYSTEM_GIT, &other).unwrap(), original);
    assert_eq!(
        fs::read_to_string(other.join("secret")).unwrap(),
        "private local contents\n"
    );
}

#[test]
fn push_rejects_non_fast_forward_and_stale_or_invalid_selections() {
    let (temp, source, other) = fixture();
    commit(&other, "remote advance\n");
    run(
        &SYSTEM_GIT,
        &other,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    commit(&source, "local advance\n");
    let before = command(&temp.0.join("remote.git"), &["rev-parse", "main"]);
    assert!(run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(
        command(&temp.0.join("remote.git"), &["rev-parse", "main"]),
        before
    );
    for (remote, target, branch) in [
        ("--all", "main", Some("main")),
        ("origin", ":main", Some("main")),
        ("origin", "main", Some("other")),
    ] {
        assert!(run(
            &SYSTEM_GIT,
            &source,
            Operation::Push,
            remote,
            target,
            branch,
            &AtomicBool::new(false)
        )
        .is_err());
    }
    command(&source, &["switch", "--detach"]);
    assert!(run(
        &SYSTEM_GIT,
        &source,
        Operation::Push,
        "origin",
        "main",
        None,
        &AtomicBool::new(false)
    )
    .is_err());
}

#[test]
fn pull_tag_option_updates_only_when_requested_and_fetch_only_preserves_work() {
    let (_temp, source, other) = fixture();
    let old = head(&SYSTEM_GIT, &other).unwrap();
    command(&other, &["tag", "release"]);
    commit(&source, "next\n");
    command(&source, &["tag", "release"]);
    command(&source, &["tag", "new-tag"]);
    command(
        &source,
        &[
            "push",
            "origin",
            "main",
            "refs/tags/release",
            "refs/tags/new-tag",
        ],
    );
    run_with_tags(
        &SYSTEM_GIT,
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
        Some(false),
        None,
    )
    .unwrap();
    assert_eq!(
        command(&other, &["rev-parse", "release"]),
        format!("{old}\n").as_bytes()
    );
    assert!(git::run(
        &SYSTEM_GIT,
        &other,
        &["rev-parse", "--verify", "refs/tags/new-tag"]
    )
    .is_err());
    let before = head(&SYSTEM_GIT, &other).unwrap();
    fs::write(other.join("file"), "keep local edits\n").unwrap();
    commit(&source, "later\n");
    command(&source, &["push", "origin", "main"]);
    run_with_tags(
        &SYSTEM_GIT,
        &other,
        Operation::Fetch,
        "origin",
        "",
        None,
        &AtomicBool::new(false),
        Some(true),
        None,
    )
    .unwrap();
    assert_eq!(head(&SYSTEM_GIT, &other).unwrap(), before);
    assert_eq!(
        fs::read_to_string(other.join("file")).unwrap(),
        "keep local edits\n"
    );
    assert_eq!(
        command(&other, &["rev-parse", "release"]),
        format!("{before}\n").as_bytes()
    );
    assert_eq!(
        command(&other, &["rev-parse", "new-tag"]),
        format!("{before}\n").as_bytes()
    );
}

#[test]
fn remote_labels_mask_credentials() {
    let repo = TempRepo::new();
    repo.git(&[
        "remote",
        "add",
        "origin",
        "https://private-user:private-password@example.test/repo?private-query#private-fragment",
    ]);
    let state = load(&SYSTEM_GIT, &repo.0).unwrap();
    let label = &state.remote_urls["origin"];
    for secret in [
        "private-user",
        "private-password",
        "private-query",
        "private-fragment",
    ] {
        assert!(!label.contains(secret));
    }
}

#[test]
fn remote_branch_push_and_delete_touch_only_the_reviewed_refs() {
    let repo = TempRepo::new();
    repo.git(&["commit", "--allow-empty", "-m", "base"]);
    let oid = repo.git(&["rev-parse", "HEAD"]);
    let origin = repo.0.join("origin.git");
    let backup = repo.0.join("backup.git");
    for (name, remote) in [("origin", &origin), ("backup", &backup)] {
        repo.git(&["init", "--bare", remote.to_str().unwrap()]);
        repo.git(&["remote", "add", name, remote.to_str().unwrap()]);
        repo.git(&["push", name, "HEAD:refs/heads/keep"]);
    }
    let branch = "refs/heads/team/topic";
    let tracking = "refs/remotes/origin/team/topic";
    repo.git(&["push", "origin", &format!("HEAD:{branch}")]);
    repo.git(&["tag", "keep-local"]);
    repo.git(&["commit", "--allow-empty", "-m", "new local head"]);

    let cancel = AtomicBool::new(false);
    let preview = preview_branch(&SYSTEM_GIT, &repo.0, tracking).unwrap();
    push_branch(&SYSTEM_GIT, &repo.0, &preview, None, &cancel).unwrap();
    push_branch(
        &SYSTEM_GIT,
        &repo.0,
        &preview,
        Some("refs/heads/review/topic"),
        &cancel,
    )
    .unwrap();
    assert!(push_branch(&SYSTEM_GIT, &repo.0, &preview, Some("bad:ref"), &cancel).is_err());
    let custom = "refs/heads/review/topic";
    let read_ref = |root: &Path, name| {
        git::run(&SYSTEM_GIT, root, &["rev-parse", "--verify", name]).and_then(git::text)
    };
    assert_eq!(read_ref(&origin, custom).unwrap(), oid);
    assert!(read_ref(&backup, branch).is_err());
    assert_eq!(read_ref(&backup, "refs/heads/keep").unwrap(), oid);
    assert!(read_ref(&backup, "refs/tags/keep-local").is_err());

    delete_branch(&SYSTEM_GIT, &repo.0, &preview, false, &cancel).unwrap();
    assert!(read_ref(&repo.0, tracking).is_err());
    assert_eq!(read_ref(&origin, branch).unwrap(), oid);

    repo.git(&["fetch", "origin"]);
    repo.git(&["push", "origin", &format!("HEAD:{branch}")]);
    repo.git(&["update-ref", tracking, oid.trim()]);
    let stale = preview_branch(&SYSTEM_GIT, &repo.0, tracking).unwrap();
    assert!(delete_branch(&SYSTEM_GIT, &repo.0, &stale, true, &cancel).is_err());
    assert_eq!(read_ref(&repo.0, tracking).unwrap(), oid);
    assert_eq!(
        read_ref(&origin, branch).unwrap(),
        repo.git(&["rev-parse", "HEAD"])
    );

    repo.git(&["fetch", "origin"]);
    let fresh = preview_branch(&SYSTEM_GIT, &repo.0, tracking).unwrap();
    delete_branch(&SYSTEM_GIT, &repo.0, &fresh, true, &cancel).unwrap();
    assert!(read_ref(&repo.0, tracking).is_err());
    assert!(read_ref(&origin, branch).is_err());
    assert_eq!(read_ref(&origin, "refs/heads/keep").unwrap(), oid);
    assert_eq!(read_ref(&origin, custom).unwrap(), oid);
    assert!(read_ref(&backup, branch).is_err());
}

#[test]
fn local_branch_push_and_tracking_target_the_selected_branch() {
    let repo = TempRepo::new();
    repo.git(&["commit", "--allow-empty", "-m", "base"]);
    repo.git(&["branch", "topic"]);
    let topic = repo.git(&["rev-parse", "topic"]);
    repo.git(&["commit", "--allow-empty", "-m", "current only"]);
    let origin = repo.0.join("origin.git");
    repo.git(&["init", "--bare", origin.to_str().unwrap()]);
    repo.git(&["remote", "add", "origin", origin.to_str().unwrap()]);
    repo.git(&["push", "origin", "topic:refs/heads/upstream-topic"]);

    let selected = "refs/heads/topic";
    let prepared = preview_branch(&SYSTEM_GIT, &repo.0, selected).unwrap();
    update_branch_tracking(
        &SYSTEM_GIT,
        &repo.0,
        &prepared,
        Some("refs/remotes/origin/upstream-topic"),
    )
    .unwrap();
    let tracked = preview_branch(&SYSTEM_GIT, &repo.0, selected).unwrap();
    assert_eq!(tracked.branch, "upstream-topic");
    push_branch_to(
        &SYSTEM_GIT,
        &repo.0,
        &tracked,
        "origin",
        Some("review/topic"),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        git::text(
            git::run(
                &SYSTEM_GIT,
                &origin,
                &["rev-parse", "refs/heads/review/topic"]
            )
            .unwrap()
        )
        .unwrap(),
        topic
    );
    update_branch_tracking(&SYSTEM_GIT, &repo.0, &tracked, None).unwrap();
    assert_eq!(
        preview_branch(&SYSTEM_GIT, &repo.0, selected)
            .unwrap()
            .upstream,
        None
    );
    assert_eq!(repo.git(&["branch", "--show-current"]).trim(), "main");
}

fn remote_head(remote: &std::path::Path, reference: &str) -> String {
    git::text(git::run(&SYSTEM_GIT, remote, &["rev-parse", reference]).unwrap())
        .unwrap()
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

#[test]
fn push_up_to_moves_the_tracked_remote_without_rewinding_it() {
    let repo = TempRepo::new();
    let origin = repo.0.join("origin.git");
    repo.git(&[
        "init",
        "--bare",
        "--initial-branch=main",
        origin.to_str().unwrap(),
    ]);
    repo.git(&["remote", "add", "origin", origin.to_str().unwrap()]);
    repo.git(&["commit", "--allow-empty", "-m", "base"]);
    let base = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    repo.git(&["push", "-u", "origin", "main"]);
    repo.git(&["commit", "--allow-empty", "-m", "middle"]);
    let middle = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    repo.git(&["commit", "--allow-empty", "-m", "tip"]);
    let tip = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let cancel = AtomicBool::new(false);

    let preview = preview_push_up_to(&SYSTEM_GIT, &repo.0, &middle).unwrap();
    assert!(preview.fast_forward && preview.on_branch && !preview.already_pushed);
    assert_eq!(preview.head, tip);
    assert_eq!(preview.remote, "origin");
    assert_eq!(preview.destination, "refs/heads/main");
    push_up_to(&SYSTEM_GIT, &repo.0, &preview, &cancel).unwrap();
    assert_eq!(remote_head(&origin, "refs/heads/main"), middle);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), tip);

    let tip_preview = preview_push_up_to(&SYSTEM_GIT, &repo.0, &tip.to_ascii_uppercase()).unwrap();
    assert!(tip_preview.fast_forward);
    assert_eq!(tip_preview.commit, tip);
    push_up_to(&SYSTEM_GIT, &repo.0, &tip_preview, &cancel).unwrap();
    assert_eq!(remote_head(&origin, "refs/heads/main"), tip);

    let contained = preview_push_up_to(&SYSTEM_GIT, &repo.0, &middle).unwrap();
    assert!(contained.already_pushed && !contained.fast_forward);
    match push_up_to(&SYSTEM_GIT, &repo.0, &contained, &cancel) {
        Err(error) => assert!(error.contains("rewriting"), "{error}"),
        Ok(()) => panic!("already pushed commit was sent"),
    }
    assert_eq!(remote_head(&origin, "refs/heads/main"), tip);

    repo.git(&["branch", "side"]);
    repo.git(&["checkout", "side"]);
    repo.git(&["commit", "--allow-empty", "-m", "side"]);
    let side = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    repo.git(&["update-ref", "refs/remotes/origin/main", &side]);
    repo.git(&["checkout", "main"]);
    repo.git(&["commit", "--allow-empty", "-m", "diverged"]);
    let diverged = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let refused = preview_push_up_to(&SYSTEM_GIT, &repo.0, &diverged).unwrap();
    assert!(refused.on_branch && !refused.fast_forward && !refused.already_pushed);
    let unrelated = preview_push_up_to(&SYSTEM_GIT, &repo.0, &side).unwrap();
    assert!(!unrelated.on_branch && !unrelated.fast_forward);
    match push_up_to(&SYSTEM_GIT, &repo.0, &refused, &cancel) {
        Err(error) => assert!(error.contains("rewriting"), "{error}"),
        Ok(()) => panic!("divergent commit was sent"),
    }
    assert_eq!(remote_head(&origin, "refs/heads/main"), tip);

    let stale = preview_push_up_to(&SYSTEM_GIT, &repo.0, &base).unwrap();
    repo.git(&["commit", "--allow-empty", "-m", "after review"]);
    match push_up_to(&SYSTEM_GIT, &repo.0, &stale, &cancel) {
        Err(error) => assert!(error.contains("changed"), "{error}"),
        Ok(()) => panic!("stale preview was pushed"),
    }
    assert_eq!(remote_head(&origin, "refs/heads/main"), tip);

    repo.git(&["switch", "--detach", &middle]);
    match preview_push_up_to(&SYSTEM_GIT, &repo.0, &middle) {
        Err(error) => assert!(error.contains("current branch"), "{error}"),
        Ok(_) => panic!("detached HEAD offered a push"),
    }
    repo.git(&["switch", "main"]);
    repo.git(&["branch", "--unset-upstream"]);
    match preview_push_up_to(&SYSTEM_GIT, &repo.0, &tip) {
        Err(error) => assert!(error.contains("track a remote branch"), "{error}"),
        Ok(_) => panic!("branch without upstream offered a push"),
    }
    repo.git(&["branch", "--set-upstream-to=origin/main"]);
    repo.git(&["update-ref", "-d", "refs/remotes/origin/main"]);
    match preview_push_up_to(&SYSTEM_GIT, &repo.0, &tip) {
        Err(error) => assert!(error.contains("not available locally"), "{error}"),
        Ok(_) => panic!("missing remote-tracking ref offered a push"),
    }
    match preview_push_up_to(&SYSTEM_GIT, &repo.0, "not-a-commit") {
        Err(error) => assert!(error.contains("not a commit"), "{error}"),
        Ok(_) => panic!("non-commit text was accepted"),
    }
}
