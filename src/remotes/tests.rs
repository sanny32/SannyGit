use super::*;
use crate::test_support::TempRepo;
use std::{fs, path::PathBuf};

fn command(root: &Path, args: &[&str]) -> Vec<u8> {
    let mut configured = vec!["-c", "core.autocrlf=false"];
    configured.extend_from_slice(args);
    git::run(root, &configured).unwrap()
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
    let preview = preview_tag_push(&source, "refs/tags/chosen", "origin").unwrap();
    push_tag(&source, &preview, &AtomicBool::new(false)).unwrap();
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
    let replacement = preview_tag_push(&source, "refs/tags/chosen", "origin").unwrap();
    assert!(push_tag(&source, &replacement, &AtomicBool::new(false)).is_err());
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
    let preview = preview_tag_push(&source, "refs/tags/chosen", "origin").unwrap();
    assert!(push_tag(&source, &preview, &AtomicBool::new(true)).is_err());
    command(
        &source,
        &[
            "config",
            "remote.origin.pushurl",
            "https://example.invalid/new.git",
        ],
    );
    assert!(push_tag(&source, &preview, &AtomicBool::new(false))
        .unwrap_err()
        .contains("changed"));
    command(
        &source,
        &[
            "config",
            "--add",
            "remote.origin.pushurl",
            "https://example.invalid/second.git",
        ],
    );
    assert!(preview_tag_push(&source, "refs/tags/chosen", "origin")
        .err()
        .unwrap()
        .contains("exactly one"));
    command(&source, &["config", "--unset-all", "remote.origin.pushurl"]);
    command(&source, &["tag", "-d", "chosen"]);
    assert!(push_tag(&source, &preview, &AtomicBool::new(false)).is_err());
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
    let preview = preview_tag_push(&source, "refs/tags/chosen", "origin").unwrap();
    assert!(preview.endpoint_display.starts_with("file:"));
    push_tag(&source, &preview, &AtomicBool::new(false)).unwrap();
    assert!(git::text(command(&destination, &["tag", "--list"]))
        .unwrap()
        .contains("chosen"));
    assert!(command(&victim, &["tag", "--list"]).is_empty());
}

#[test]
fn explicit_push_and_fetch_preserve_worktree_and_do_not_push_tags_or_other_branches() {
    let (temp, source, other) = fixture();
    let state = load(&other).unwrap();
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
    let old = head(&other).unwrap();
    fs::write(other.join("untracked"), "keep").unwrap();
    run(
        &other,
        Operation::Fetch,
        "origin",
        "",
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(head(&other).unwrap(), old);
    assert_eq!(fs::read_to_string(other.join("file")).unwrap(), "base\n");
    assert_eq!(fs::read_to_string(other.join("untracked")).unwrap(), "keep");
    assert_eq!(
        command(&other, &["rev-parse", "origin/main"]),
        command(&source, &["rev-parse", "HEAD"])
    );
    assert!(run(
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
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &cancel,
    )
    .unwrap();
    run(
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &cancel,
    )
    .unwrap();
    assert_eq!(head(&other).unwrap(), head(&source).unwrap());
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
            &other,
            Operation::Sync,
            "origin",
            "main",
            Some("main"),
            &cancel,
        )
        .unwrap();
        assert_eq!(
            head(&temp.0.join("remote.git")).unwrap(),
            head(&other).unwrap()
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
    let base = head(&remote).unwrap();
    fs::write(other.join("file"), "keep local edits\n").unwrap();
    assert!(run(
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
    let local = head(&other).unwrap();
    assert!(run(
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("other"),
        &cancel
    )
    .is_err());
    assert!(run(
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(true)
    )
    .is_err());
    assert_eq!(head(&remote).unwrap(), base);
    commit(&source, "remote commit\n");
    run(
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &cancel,
    )
    .unwrap();
    let error = run(
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &cancel,
    )
    .unwrap_err();
    assert!(error.contains("Sync requires a fast-forward"));
    assert_eq!(head(&other).unwrap(), local);
    assert_eq!(head(&remote).unwrap(), head(&source).unwrap());
}

#[test]
fn sync_pushes_before_fetch_and_keeps_published_commits_on_fetch_failure() {
    let (temp, _, other) = fixture();
    commit(&other, "local update\n");
    command(&other, &["config", "remote.origin.uploadpack", "false"]);
    assert!(run(
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(
        head(&temp.0.join("remote.git")).unwrap(),
        head(&other).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn sync_does_not_pull_after_a_hook_rejects_push() {
    use std::os::unix::fs::PermissionsExt;
    let (temp, source, other) = fixture();
    commit(&source, "remote update\n");
    push_tip(
        &source,
        "origin",
        "refs/heads/main",
        &head(&source).unwrap(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let original = head(&other).unwrap();
    let hook = other.join(".git/hooks/pre-push");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(run(
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(head(&other).unwrap(), original);
    assert_eq!(
        head(&temp.0.join("remote.git")).unwrap(),
        head(&source).unwrap()
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
    let original = head(&other).unwrap();
    let error = run(
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(error.contains("URLs differ"));
    assert_eq!(head(&other).unwrap(), original);
}

#[test]
fn sync_stops_on_transport_failure_before_pulling() {
    let (_temp, source, other) = fixture();
    let cancel = AtomicBool::new(false);
    commit(&source, "remote update\n");
    run(
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &cancel,
    )
    .unwrap();
    let original = head(&other).unwrap();
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
        &other,
        Operation::Sync,
        "origin",
        "main",
        Some("main"),
        &cancel,
    )
    .unwrap_err();
    assert!(error.contains("push failed"));
    assert_eq!(head(&other).unwrap(), original);
}

#[test]
fn pull_fast_forwards_and_refuses_dirty_or_diverged_worktrees() {
    let (_temp, source, other) = fixture();
    commit(&source, "next\n");
    run(
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    fs::write(other.join("file"), "local edits\n").unwrap();
    assert!(run(
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(other.join("file")).unwrap(),
        "local edits\n"
    );
    fs::write(other.join("file"), "base\n").unwrap();
    run(
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(head(&other).unwrap(), head(&source).unwrap());
    assert_eq!(fs::read_to_string(other.join("file")).unwrap(), "next\n");
    commit(&other, "local commit\n");
    commit(&source, "remote commit\n");
    run(
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    let old = head(&other).unwrap();
    assert!(run(
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false)
    )
    .unwrap_err()
    .contains("fast-forward"));
    assert_eq!(head(&other).unwrap(), old);
    assert_eq!(
        fs::read_to_string(other.join("file")).unwrap(),
        "local commit\n"
    );
}

#[test]
fn pull_preserves_ignored_files_and_refuses_in_progress_operations() {
    let (_temp, source, other) = fixture();
    let original = head(&other).unwrap();
    fs::write(other.join(".git/MERGE_HEAD"), format!("{original}\n")).unwrap();
    assert!(run(
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
        &source,
        Operation::Push,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(run(
        &other,
        Operation::Pull,
        "origin",
        "main",
        Some("main"),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(head(&other).unwrap(), original);
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
        &source,
        Operation::Push,
        "origin",
        "main",
        None,
        &AtomicBool::new(false)
    )
    .is_err());
}
