use super::*;
use crate::test_support::TempRepo;
use crate::test_support::SYSTEM_GIT;

#[test]
fn ignore_selection_appends_literal_root_rules_without_staging() {
    let repo = TempRepo::new();
    std::fs::create_dir(repo.0.join("nested")).unwrap();
    repo.write(".gitignore", "# existing rule without final newline");
    repo.write("nested/report [1].txt", "keep these bytes");
    repo.write("nested/report 1.txt", "neighbor");
    repo.write("report [1].txt", "different directory");
    repo.write("!local.txt", "keep this too");

    let paths = ["nested/report [1].txt".into(), "!local.txt".into()];
    ignore_paths(&SYSTEM_GIT, &repo.0, &paths, Path::new(""), None).unwrap();

    let rules = std::fs::read_to_string(repo.0.join(".gitignore")).unwrap();
    assert_eq!(
        rules,
        "# existing rule without final newline\n/nested/report\\ \\[1\\].txt\n/!local.txt\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.0.join(&paths[0])).unwrap(),
        "keep these bytes"
    );
    assert!(repo.git(&["ls-files"]).is_empty());
    let remaining: BTreeSet<_> = load_changes(&SYSTEM_GIT, &repo.0)
        .unwrap()
        .into_iter()
        .map(|change| change.path)
        .collect();
    assert_eq!(
        remaining,
        BTreeSet::from([
            ".gitignore".into(),
            "nested/report 1.txt".into(),
            "report [1].txt".into(),
        ])
    );
    assert!(ignore_paths(&SYSTEM_GIT, &repo.0, &paths, Path::new(""), None).is_err());
    assert_eq!(
        std::fs::read_to_string(repo.0.join(".gitignore")).unwrap(),
        rules
    );
}

#[test]
fn ignore_pattern_is_scoped_to_the_selected_folder() {
    for destination in ["", "nested"] {
        let repo = TempRepo::new();
        std::fs::create_dir(repo.0.join("nested")).unwrap();
        repo.write("nested/input.log", "");
        repo.write("nested/output.log", "");
        repo.write("other.log", "");
        ignore_paths(
            &SYSTEM_GIT,
            &repo.0,
            &["nested/input.log".into()],
            Path::new(destination),
            Some("*.log"),
        )
        .unwrap();
        let remaining: Vec<_> = load_changes(&SYSTEM_GIT, &repo.0)
            .unwrap()
            .into_iter()
            .map(|change| change.path)
            .collect();
        assert!(remaining.iter().any(|path| path == "other.log"));
        assert!(!remaining
            .iter()
            .any(|path| path.starts_with("nested/") && path.ends_with(".log")));
    }
}

#[cfg(unix)]
#[test]
fn ignore_selection_does_not_follow_a_symlinked_gitignore() {
    let repo = TempRepo::new();
    repo.write("other", "untouched");
    repo.write("selected", "");
    std::os::unix::fs::symlink("other", repo.0.join(".gitignore")).unwrap();

    assert!(ignore_paths(
        &SYSTEM_GIT,
        &repo.0,
        &["selected".into()],
        Path::new(""),
        None
    )
    .is_err());
    assert_eq!(
        std::fs::read_to_string(repo.0.join("other")).unwrap(),
        "untouched"
    );
}

#[test]
fn staging_round_trip_command_budget() {
    let mut repo = TempRepo::new();
    // Match load() on symlinked temporary directories and Windows path prefixes.
    let root = repo.git(&["rev-parse", "--show-toplevel"]);
    repo.0 = PathBuf::from(root.strip_suffix('\n').unwrap_or(&root));
    repo.write("file", "base\n");
    repo.git(&["add", "file"]);
    repo.git(&["commit", "-m", "base"]);
    repo.write("file", "edited\n");
    let counter = crate::test_support::GitCommandCount::new(&repo.0);
    let start = Instant::now();
    for _ in 0..5 {
        for staged in [true, false] {
            set_staged(&SYSTEM_GIT, &repo.0, "file", staged).unwrap();
            let snapshot = load(&SYSTEM_GIT, &repo.0).unwrap();
            assert_eq!(snapshot.changes.len(), 1);
            assert_eq!(snapshot.changes[0].staged, staged);
            assert_eq!(snapshot.changes[0].unstaged, !staged);
        }
    }
    println!(
        "Five stage/unstage round trips including refresh: {:?}, {} Git processes",
        start.elapsed(),
        counter.get()
    );
    // Three commands to stage, four to unstage, and five for each UI refresh,
    // including HEAD's commit id and the worktree-specific merge marker.
    assert_eq!(counter.get(), 85);
    assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"edited\n");
}

#[test]
fn ignored_collisions_compare_literal_paths_and_directory_boundaries() {
    assert_eq!(path_collisions("a b\n.txt\0", "a b\n.txt\0"), ["a b\n.txt"]);
    assert_eq!(path_collisions("dir/file\0", "dir\0"), ["dir/file"]);
    assert_eq!(path_collisions("dir\0", "dir/file\0"), ["dir"]);
    assert!(path_collisions("directory/file\0", "dir\0").is_empty());
    assert!(path_collisions("dir/file\0", "dir/other\0").is_empty());
    assert!(path_collisions("", "dir\0").is_empty());
    assert_eq!(
        !path_collisions("NAME\0", "name\0").is_empty(),
        cfg!(windows)
    );
}

#[test]
fn exited_parent_pipes_obey_timeout_cancellation_and_normal_completion() {
    let repo = TempRepo::new();
    let source = repo.0.join("pipe_fixture.rs");
    let executable = repo.0.join(if cfg!(windows) {
        "pipe_fixture.exe"
    } else {
        "pipe_fixture"
    });
    std::fs::write(
        &source,
        r#"
use std::{io::Write, path::PathBuf, process::{Command, Stdio}, thread, time::{Duration, Instant}};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--descendant") {
        let directory = PathBuf::from(&args[2]);
        println!("descendant output");
        eprintln!("descendant diagnostic");
        std::io::stdout().flush().unwrap();
        std::fs::write(directory.join("ready"), b"").unwrap();
        let start = Instant::now();
        while !directory.join("release").exists() && start.elapsed() < Duration::from_secs(6) {
            thread::sleep(Duration::from_millis(10));
        }
        return;
    }
    let directory = args.last().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.arg("--descendant").arg(directory).stdin(Stdio::inherit())
        .stdout(Stdio::inherit()).stderr(Stdio::inherit());
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let _child = command.spawn().unwrap();
    // Deliberately exit without waiting: all three pipes stay in the descendant.
}
"#,
    )
    .unwrap();
    let mut compiler = Command::new("rustc");
    compiler
        .arg("--edition=2021")
        .arg(&source)
        .arg("-o")
        .arg(&executable);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        compiler.creation_flags(0x08000000);
    }
    let compiled = compiler.output().unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );

    thread::spawn(move || {
        let git = &GitContext::new(executable.to_string_lossy().into_owned());
        for scenario in ["timeout", "cancel", "blocked-input", "success"] {
            let directory = repo.0.join(scenario);
            std::fs::create_dir(&directory).unwrap();
            let cancel = AtomicBool::new(false);
            let start = Instant::now();
            let result = thread::scope(|scope| {
                if matches!(scenario, "cancel" | "success") {
                    let directory = &directory;
                    let cancel = &cancel;
                    scope.spawn(move || {
                        let deadline = Instant::now() + Duration::from_secs(3);
                        while !directory.join("ready").exists() && Instant::now() < deadline {
                            thread::sleep(Duration::from_millis(10));
                        }
                        // Let the wrapper exit before cancelling/releasing its descendant.
                        thread::sleep(Duration::from_millis(100));
                        if scenario == "cancel" {
                            signal_cancel(cancel);
                        } else {
                            std::fs::write(directory.join("release"), b"").unwrap();
                        }
                    });
                }
                let input = vec![b'x'; 2 * 1024 * 1024];
                execute(
                    git,
                    &repo.0,
                    &["status", directory.to_str().unwrap()],
                    (scenario == "blocked-input").then_some(input.as_slice()),
                    if matches!(scenario, "cancel" | "success") {
                        Duration::from_secs(5)
                    } else {
                        Duration::from_millis(500)
                    },
                    Some(&cancel),
                    CommandMode::Normal,
                )
            });
            // Release an orphan on Windows even if an assertion below fails.
            std::fs::write(directory.join("release"), b"").unwrap();
            assert!(
                directory.join("ready").exists(),
                "{scenario}: fixture did not start"
            );
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "{scenario}: pipes exceeded deadline"
            );
            if scenario == "success" {
                let output = result.unwrap();
                assert!(output.status.success());
                assert!(String::from_utf8(output.stdout)
                    .unwrap()
                    .contains("descendant output"));
            } else {
                let error = result.unwrap_err();
                assert_eq!(
                    error.kind,
                    if scenario == "cancel" {
                        FailureKind::Cancelled
                    } else {
                        FailureKind::TimedOut
                    },
                    "{scenario}"
                );
                assert!(!error.to_string().contains("descendant diagnostic"));
            }
        }
    })
    .join()
    .unwrap();
}

#[test]
fn command_results_keep_exit_codes_and_distinguish_cancellation_from_git_failure() {
    let repo = TempRepo::new();
    let output = execute(
        &SYSTEM_GIT,
        &repo.0,
        &["--version"],
        None,
        TIMEOUT,
        None,
        CommandMode::Normal,
    )
    .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.starts_with(b"git version"));
    assert!(!format!("{output:?}").contains("git version"));
    for (args, expected) in [
        (vec!["config", "--local", "--get", "sannygit.missing"], 1),
        (vec!["rev-parse", "--verify", "HEAD"], 128),
    ] {
        let error = execute(
            &SYSTEM_GIT,
            &repo.0,
            &args,
            None,
            TIMEOUT,
            None,
            CommandMode::Normal,
        )
        .unwrap_err();
        assert_eq!(error.kind, FailureKind::Exit);
        assert_eq!(error.exit_code, Some(expected));
        assert!(error
            .to_string()
            .contains(&format!("exit code: {expected}")));
    }
    let cancelled = AtomicBool::new(true);
    let error = execute(
        &SYSTEM_GIT,
        &repo.0,
        &["status"],
        None,
        TIMEOUT,
        Some(&cancelled),
        CommandMode::Normal,
    )
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::Cancelled);
    assert_eq!(error.exit_code, None);
}

#[cfg(unix)]
#[test]
fn structured_timeouts_output_limits_and_failed_symbolic_refs_keep_their_meaning() {
    use std::os::unix::fs::PermissionsExt;
    let repo = TempRepo::new();
    let timeout = execute(
        &SYSTEM_GIT,
        &repo.0,
        &["-c", "alias.wait=!sleep 30", "wait"],
        None,
        Duration::from_millis(100),
        None,
        CommandMode::Normal,
    )
    .unwrap_err();
    assert_eq!(timeout.kind, FailureKind::TimedOut);
    assert_eq!(timeout.exit_code, None);
    let executable = repo.0.join("mock-git");
    std::fs::write(&executable, "#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do\ncase \"$1\" in\nrev-parse) pwd; exit 0;;\nsymbolic-ref) echo 'fatal: repository not found' >&2; exit 128;;\nesac\nshift\ndone\nexit 1\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    // Thread-local configuration cannot affect other tests.
    thread::scope(|scope| {
        scope.spawn(|| {
            let git = &GitContext::new(executable.to_string_lossy().into_owned());
            let error = load(git, &repo.0).err().unwrap();
            assert!(error.contains("symbolic-ref exit code: 128"));
            assert!(!error.contains("Detached at"));
            // Failed stdout and arbitrary stderr from hook-capable operations
            // must not enter either Display or Debug diagnostics.
            std::fs::write(&executable, "#!/bin/sh\necho 'raw-stdout-private-value'\necho 'raw-stderr-private-value' >&2\nexit 7\n").unwrap();
            let error = execute(git, &repo.0, &["push"], None, TIMEOUT, None, CommandMode::Normal).unwrap_err();
            assert_eq!(error.exit_code, Some(7));
            assert!(!format!("{error:?} {error}").contains("private-value"));
            std::fs::write(&executable, "#!/bin/sh\nhead -c 2097153 /dev/zero\n").unwrap();
            let error = execute(git, &repo.0, &["status"], None, TIMEOUT, None, CommandMode::Normal).unwrap_err();
            assert_eq!(error.kind, FailureKind::OutputLimit);
            assert_eq!(error.exit_code, Some(0));
            assert!(error.to_string().contains("2 MiB"));
            std::fs::write(&executable, "#!/bin/sh\necho 'partial private-value' >&2\nsleep 30\n").unwrap();
            let cancel = AtomicBool::new(false);
            thread::scope(|scope| {
                scope.spawn(|| {
                    thread::sleep(Duration::from_millis(50));
                    signal_cancel(&cancel);
                });
                let error = execute(git, &repo.0, &["status"], None, TIMEOUT, Some(&cancel), CommandMode::Normal).unwrap_err();
                assert_eq!(error.kind, FailureKind::Cancelled);
                assert_eq!(error.exit_code, None);
                assert!(!error.to_string().contains("private-value"));
            });
        }).join().unwrap();
    });
}

#[test]
fn sync_accepts_only_a_single_non_fast_forward_porcelain_rejection() {
    for reason in ["fetch first", "non-fast-forward"] {
        assert!(push_needs_pull(
            format!("To hidden\n!\t123:refs/heads/main\t[rejected] ({reason})\nDone\n").as_bytes()
        ));
    }
    for output in [
        "!\t123:refs/heads/main\t[remote rejected] (pre-receive hook declined)\n",
        "!\t123:refs/heads/main\t[rejected] (stale info)\n",
        "!\t123:refs/heads/main\t[rejected] (fetch first)\n!\t456:refs/heads/other\t[rejected] (fetch first)\n",
        "fatal: authentication failed", "", "=\t123:refs/heads/main\t[up to date]\n",
    ] {
        assert!(!push_needs_pull(output.as_bytes()), "{output}");
    }
}

#[test]
fn path_discovery_returns_the_first_executable_as_an_absolute_path() {
    let repo = TempRepo::new();
    let first = repo.0.join("first Git directory");
    let second = repo.0.join("second");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let candidate = second.join(name);
    std::fs::write(&candidate, "test executable").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // A non-executable earlier PATH entry must be skipped.
        std::fs::write(first.join(name), "not executable").unwrap();
        std::fs::set_permissions(first.join(name), std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let search =
        std::env::join_paths([Path::new("first Git directory"), Path::new("second")]).unwrap();
    assert_eq!(find_executable_in(&search, &repo.0), Some(candidate));
    assert_eq!(find_executable_in(first.as_os_str(), &repo.0), None);
}

#[test]
fn configured_executable_reports_missing_file_without_falling_back() {
    thread::spawn(|| {
        let directory = std::env::temp_dir();
        let missing = directory.join(format!("missing Git executable {}", std::process::id()));
        let git = &GitContext::new(missing.to_string_lossy().into_owned());
        let typed = execute(
            git,
            &directory,
            &["--version"],
            None,
            TIMEOUT,
            None,
            CommandMode::Normal,
        )
        .unwrap_err();
        assert_eq!(typed.kind, FailureKind::Start);
        assert_eq!(typed.exit_code, None);
        let error = run(git, &directory, &["--version"]).unwrap_err();
        assert!(error.contains(&*missing.to_string_lossy()));
        assert!(error.contains("Preferences > Git > Executable"));
        let git = &SYSTEM_GIT;
        assert!(run(git, &directory, &["--version"])
            .unwrap()
            .starts_with(b"git version"));
    })
    .join()
    .unwrap();
}

#[cfg(unix)]
#[test]
fn configured_executable_supports_spaces_and_preserves_arguments() {
    thread::spawn(|| {
        use std::os::unix::fs::PermissionsExt;
        let repo = TempRepo::new();
        let executable = repo.0.join("custom Git executable");
        std::fs::write(&executable, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let git = &GitContext::new(executable.to_string_lossy().into_owned());
        let output = run(git, &repo.0, &["--version", "argument with spaces", "$(literal)"]).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(),
            "--no-pager\n--no-optional-locks\n-c\ncolor.ui=false\n--version\nargument with spaces\n$(literal)\n");
    }).join().unwrap();
}

#[test]
fn staged_commit_preview_and_commit_preserve_partial_file_and_untracked_contents() {
    for unborn in [true, false] {
        let repo = TempRepo::new();
        if !unborn {
            repo.write("file", "base\n");
            commit(&SYSTEM_GIT, &repo.0, "Base", CommitContents::All).unwrap();
        }
        repo.write("file", "selected\n");
        repo.git(&["add", "file"]);
        repo.write("file", "excluded\n");
        repo.write("new", "private untracked\n");
        let index = std::fs::read(repo.0.join(".git/index")).unwrap();
        let patch = commit_patch(
            &SYSTEM_GIT,
            &repo.0,
            CommitContents::Staged,
            None,
            CommitPatchOptions {
                max_new_file_content_bytes: usize::MAX,
                ..CommitPatchOptions::default()
            },
        )
        .unwrap();
        assert!(patch.contains("+selected"));
        assert!(!patch.contains("excluded"));
        assert!(!patch.contains("private untracked"));
        let all = commit_patch(
            &SYSTEM_GIT,
            &repo.0,
            CommitContents::All,
            None,
            CommitPatchOptions {
                max_new_file_content_bytes: usize::MAX,
                ..CommitPatchOptions::default()
            },
        )
        .unwrap();
        assert!(all.contains("+excluded"));
        assert!(!all.contains("+selected"));
        assert!(all.contains("private untracked"));
        assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
        commit(&SYSTEM_GIT, &repo.0, "Selected", CommitContents::Staged).unwrap();
        assert_eq!(repo.git(&["show", "HEAD:file"]), "selected\n");
        assert_eq!(repo.git(&["ls-tree", "--name-only", "HEAD"]), "file\n");
        assert!(repo.git(&["diff", "--cached"]).is_empty());
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file")).unwrap(),
            "excluded\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.0.join("new")).unwrap(),
            "private untracked\n"
        );
        let head = repo.git(&["rev-parse", "HEAD"]);
        assert!(
            commit(&SYSTEM_GIT, &repo.0, "Empty", CommitContents::Staged)
                .unwrap_err()
                .contains("no staged")
        );
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    }
}

#[test]
fn staged_commit_handles_deletion_rename_and_recreated_working_file() {
    let repo = TempRepo::new();
    repo.write("old", "rename contents\n");
    repo.write("deleted", "delete contents\n");
    commit(&SYSTEM_GIT, &repo.0, "Base", CommitContents::All).unwrap();
    repo.git(&["mv", "old", "renamed"]);
    repo.git(&["rm", "deleted"]);
    repo.write("deleted", "recreated unstaged\n");
    repo.write("renamed", "changed after rename\n");
    let patch = commit_patch(
        &SYSTEM_GIT,
        &repo.0,
        CommitContents::Staged,
        None,
        CommitPatchOptions {
            max_new_file_content_bytes: usize::MAX,
            ..CommitPatchOptions::default()
        },
    )
    .unwrap();
    assert!(patch.contains("deleted"));
    assert!(patch.contains("renamed"));
    assert!(!patch.contains("recreated unstaged"));
    assert!(!patch.contains("changed after rename"));
    commit(
        &SYSTEM_GIT,
        &repo.0,
        "Rename and delete",
        CommitContents::Staged,
    )
    .unwrap();
    assert_eq!(repo.git(&["ls-tree", "--name-only", "HEAD"]), "renamed\n");
    assert_eq!(repo.git(&["show", "HEAD:renamed"]), "rename contents\n");
    assert!(repo.git(&["diff", "--cached"]).is_empty());
    assert_eq!(
        std::fs::read_to_string(repo.0.join("deleted")).unwrap(),
        "recreated unstaged\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.0.join("renamed")).unwrap(),
        "changed after rename\n"
    );
}

#[test]
fn staged_amend_preview_includes_original_commit_and_preserves_unstaged_changes() {
    for root_commit in [true, false] {
        let repo = TempRepo::new();
        if !root_commit {
            repo.write("parent", "parent content\n");
            commit(&SYSTEM_GIT, &repo.0, "Parent", CommitContents::All).unwrap();
        }
        repo.write("original", "original commit content\n");
        commit(&SYSTEM_GIT, &repo.0, "Original", CommitContents::All).unwrap();
        let (head, _) = amend_message(&SYSTEM_GIT, &repo.0).unwrap();
        let parents = repo.git(&["show", "-s", "--format=%P", "HEAD"]);
        repo.write("original", "unstaged private content\n");
        repo.write("new", "selected addition\n");
        repo.git(&["add", "new"]);
        repo.write("new", "excluded addition\n");
        let index = std::fs::read(repo.0.join(".git/index")).unwrap();
        let patch = commit_patch(
            &SYSTEM_GIT,
            &repo.0,
            CommitContents::Staged,
            Some(&head),
            CommitPatchOptions {
                max_new_file_content_bytes: usize::MAX,
                ..CommitPatchOptions::default()
            },
        )
        .unwrap();
        assert!(patch.contains("+original commit content"));
        assert!(patch.contains("+selected addition"));
        assert!(!patch.contains("unstaged private"));
        assert!(!patch.contains("excluded addition"));
        assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
        amend_commit(
            &SYSTEM_GIT,
            &repo.0,
            "Amended",
            &head,
            CommitContents::Staged,
        )
        .unwrap();
        assert_eq!(repo.git(&["show", "-s", "--format=%P", "HEAD"]), parents);
        assert_eq!(
            repo.git(&["show", "HEAD:original"]),
            "original commit content\n"
        );
        assert_eq!(repo.git(&["show", "HEAD:new"]), "selected addition\n");
        let (next, _) = amend_message(&SYSTEM_GIT, &repo.0).unwrap();
        let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
        assert!(repo.git(&["diff", "--cached"]).is_empty());
        amend_commit(
            &SYSTEM_GIT,
            &repo.0,
            "Message only",
            &next,
            CommitContents::Staged,
        )
        .unwrap();
        assert_eq!(repo.git(&["rev-parse", "HEAD^{tree}"]), tree);
        assert_eq!(
            std::fs::read_to_string(repo.0.join("original")).unwrap(),
            "unstaged private content\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.0.join("new")).unwrap(),
            "excluded addition\n"
        );
        let before = std::fs::read(repo.0.join(".git/index")).unwrap();
        for mode in [CommitContents::All, CommitContents::Staged] {
            assert!(amend_commit(&SYSTEM_GIT, &repo.0, "Stale", &head, mode)
                .unwrap_err()
                .contains("HEAD has changed"));
            assert!(commit_patch(
                &SYSTEM_GIT,
                &repo.0,
                mode,
                Some(&head),
                CommitPatchOptions {
                    max_new_file_content_bytes: usize::MAX,
                    ..CommitPatchOptions::default()
                },
            )
            .unwrap_err()
            .contains("HEAD has changed"));
        }
        assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), before);
    }
}

#[test]
fn commit_patch_honors_path_excludes_for_untracked_files() {
    let repo = TempRepo::new();
    repo.write("keep.txt", "keep\n");
    repo.write("skip.txt", "skip\n");
    let patch = commit_patch(
        &SYSTEM_GIT,
        &repo.0,
        CommitContents::All,
        None,
        CommitPatchOptions {
            path_excludes: vec!["skip.txt".into()],
            ..CommitPatchOptions::default()
        },
    )
    .unwrap();
    assert!(patch.contains("keep.txt"));
    assert!(!patch.contains("skip.txt"));
}

#[test]
fn commit_patch_omits_untracked_binary_and_oversized_file_contents() {
    let repo = TempRepo::new();
    repo.write("note.txt", "describe the font\n");
    repo.write("Noto.otf", "\0binary");
    let large = repo.0.join("large.bin");
    std::fs::write(&large, vec![b'a'; 2 * 1024 * 1024 + 1]).unwrap();
    let limit = crate::llm::DEFAULT_MAX_NEW_FILE_CONTENT_BYTES as usize;
    let patch = commit_patch(
        &SYSTEM_GIT,
        &repo.0,
        CommitContents::All,
        None,
        CommitPatchOptions {
            max_new_file_content_bytes: limit,
            ..CommitPatchOptions::default()
        },
    )
    .unwrap();
    assert!(patch.contains("New file: Noto.otf (content omitted"));
    assert!(!patch.contains("\0binary"));
    assert!(patch.contains("New file: large.bin (content omitted"));
    assert!(patch.contains("+describe the font"));
}

#[test]
fn commit_patch_includes_new_files_without_changing_the_index() {
    let repo = TempRepo::new();
    repo.write("file.txt", "base\n");
    repo.git(&["add", "file.txt"]);
    repo.git(&["commit", "-m", "base"]);
    repo.write("file.txt", "unstaged\n");
    repo.write("untracked.txt", "new contents\n");
    repo.write(".gitignore", "ignored.txt\n");
    repo.write("ignored.txt", "ignored contents\n");
    let before = repo.git(&["status", "--porcelain=v1", "-z"]);

    let patch = commit_patch(
        &SYSTEM_GIT,
        &repo.0,
        CommitContents::All,
        None,
        CommitPatchOptions {
            max_new_file_content_bytes: usize::MAX,
            ..CommitPatchOptions::default()
        },
    )
    .unwrap();
    assert!(patch.contains("+unstaged"));
    assert!(patch.contains("untracked.txt"));
    assert!(patch.contains("+new contents"));
    assert!(!patch.contains("ignored contents"));
    assert_eq!(repo.git(&["status", "--porcelain=v1", "-z"]), before);
}

#[test]
fn new_files_can_generate_a_patch_and_create_the_initial_commit() {
    let repo = TempRepo::new();
    repo.write("new file.txt", "initial contents\n");
    repo.write(".gitignore", "ignored\n");
    repo.write("ignored", "keep private\n");
    let patch = commit_patch(
        &SYSTEM_GIT,
        &repo.0,
        CommitContents::All,
        None,
        CommitPatchOptions {
            max_new_file_content_bytes: usize::MAX,
            ..CommitPatchOptions::default()
        },
    )
    .unwrap();
    assert!(patch.contains("new file.txt"));
    assert!(patch.contains("+initial contents"));
    assert!(!patch.contains("keep private"));
    assert!(load(&SYSTEM_GIT, &repo.0)
        .unwrap()
        .changes
        .iter()
        .all(|c| !c.staged));
    commit(
        &SYSTEM_GIT,
        &repo.0,
        "Add initial files",
        CommitContents::All,
    )
    .unwrap();
    assert_eq!(
        repo.git(&["show", "HEAD:new file.txt"]),
        "initial contents\n"
    );
    assert!(!repo
        .git(&["ls-tree", "--name-only", "HEAD"])
        .lines()
        .any(|path| path == "ignored"));
    assert!(load(&SYSTEM_GIT, &repo.0).unwrap().changes.is_empty());
}

#[test]
fn parses_nul_paths_renames_mixed_and_conflicted_status() {
    let files = parse_status(b"MM a file\0R  new\nname\0old name\0?? unicode-\xc3\xa9\0UU conflict\0 D deleted\0AA both\0").unwrap();
    assert_eq!(files.len(), 6);
    assert_eq!(files[0].kind, ChangeKind::Modified);
    assert!(files[0].staged);
    assert_eq!(files[1].path, "new\nname");
    assert_eq!(files[1].kind, ChangeKind::Renamed);
    assert_eq!(files[1].original_path.as_deref(), Some("old name"));
    assert_eq!(files[2].kind, ChangeKind::Untracked);
    assert!(!files[2].staged);
    assert_eq!(files[3].kind, ChangeKind::Conflicted);
    assert!(!files[3].staged);
    assert_eq!(files[4].kind, ChangeKind::Deleted);
    assert_eq!(files[5].kind, ChangeKind::Conflicted);
}

#[test]
fn labels_each_unmerged_status() {
    let files = parse_status(b"DD a\0AU b\0UD c\0UA d\0DU e\0AA f\0UU g\0 M h\0").unwrap();
    assert_eq!(
        files
            .iter()
            .map(|file| file.conflict_status)
            .collect::<Vec<_>>(),
        [
            Some("Both deleted"),
            Some("Added by us"),
            Some("Deleted by them"),
            Some("Added by them"),
            Some("Deleted by us"),
            Some("Both added"),
            Some("Both modified"),
            None,
        ],
    );
}

#[test]
fn rejects_malformed_oversized_and_non_utf8_status() {
    for input in [
        b"M\0".as_slice(),
        b"M  missing terminator",
        b"R  new\0",
        b"ZZ file\0",
        b"?? \xff\0",
    ] {
        assert!(parse_status(input).is_err());
    }
    assert!(parse_status(&b"?? file\0".repeat(FILE_LIMIT + 1)).is_err());
    assert!(limit_diff("+line\n".repeat(10_001)).is_err());
    assert!(limit_diff("a".repeat(OUTPUT_LIMIT + 1)).is_err());
}

#[test]
fn loads_unborn_repository_and_previews_untracked_files() {
    let repo = TempRepo::new();
    repo.write("hello é.txt", "hello\nworld\n");
    repo.write("binary", "a\0b");
    let snapshot = load(&SYSTEM_GIT, &repo.0).unwrap();
    assert_eq!(snapshot.branch, "main");
    assert_eq!(snapshot.changes.len(), 2);
    let patch = diff(
        &SYSTEM_GIT,
        &repo.0,
        "hello é.txt",
        ChangeKind::Untracked,
        None,
    )
    .unwrap();
    assert!(patch.contains("+hello\n+world"));
    assert!(
        diff(&SYSTEM_GIT, &repo.0, "binary", ChangeKind::Untracked, None)
            .unwrap()
            .contains("Binary file")
    );
    repo.git(&["add", "--", "hello é.txt"]);
    assert!(
        diff(&SYSTEM_GIT, &repo.0, "hello é.txt", ChangeKind::Added, None)
            .unwrap()
            .contains("Staged changes")
    );
}

#[test]
fn loads_real_index_worktree_rename_deletion_and_detached_head() {
    let repo = TempRepo::new();
    repo.write("file.txt", "original\n");
    repo.write("delete.txt", "delete\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fixture"]);
    repo.write("file.txt", "staged\n");
    repo.git(&["add", "file.txt"]);
    repo.write("file.txt", "unstaged\n");
    repo.git(&["mv", "delete.txt", "renamed.txt"]);
    let snapshot = load(&SYSTEM_GIT, &repo.0).unwrap();
    assert!(snapshot
        .changes
        .iter()
        .any(|c| c.kind == ChangeKind::Renamed));
    let rename_patch = diff(
        &SYSTEM_GIT,
        &repo.0,
        "renamed.txt",
        ChangeKind::Renamed,
        Some("delete.txt"),
    )
    .unwrap();
    assert!(rename_patch.contains("rename from delete.txt"));
    assert!(rename_patch.contains("rename to renamed.txt"));
    let patch = diff(&SYSTEM_GIT, &repo.0, "file.txt", ChangeKind::Modified, None).unwrap();
    assert!(patch.contains("+staged"));
    assert!(patch.contains("+unstaged"));
    assert!(patch.contains("Staged changes"));
    assert!(patch.contains("Working tree changes"));
    std::fs::remove_file(repo.0.join("file.txt")).unwrap();
    assert!(load(&SYSTEM_GIT, &repo.0)
        .unwrap()
        .changes
        .iter()
        .any(|c| c.kind == ChangeKind::Deleted));
    repo.git(&["checkout", "--detach"]);
    assert!(load(&SYSTEM_GIT, &repo.0)
        .unwrap()
        .branch
        .starts_with("Detached at "));
}

#[test]
fn literal_bracket_path_does_not_include_other_files() {
    let repo = TempRepo::new();
    repo.write("[x].txt", "one\n");
    repo.write("x.txt", "two\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fixture"]);
    repo.write("[x].txt", "literal\n");
    repo.write("x.txt", "unrelated\n");
    let patch = diff(&SYSTEM_GIT, &repo.0, "[x].txt", ChangeKind::Modified, None).unwrap();
    assert!(patch.contains("+literal"));
    assert!(!patch.contains("unrelated"));
}

#[test]
fn loads_linked_worktree_from_subdirectory() {
    let repo = TempRepo::new();
    repo.write("file", "fixture");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fixture"]);
    let linked = repo.0.join("linked");
    repo.git(&["worktree", "add", "--detach", linked.to_str().unwrap()]);
    std::fs::create_dir(linked.join("nested")).unwrap();
    let snapshot = load(&SYSTEM_GIT, &linked.join("nested")).unwrap();
    assert_eq!(
        PathBuf::from(snapshot.repository.path)
            .canonicalize()
            .unwrap(),
        linked.canonicalize().unwrap()
    );
    assert!(snapshot.changes.is_empty());
    assert!(snapshot.branch.starts_with("Detached at "));
}

#[test]
fn previews_conflict_against_ours_without_combined_diff() {
    let repo = TempRepo::new();
    repo.write("file", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.git(&["checkout", "-b", "other"]);
    repo.write("file", "theirs\n");
    repo.git(&["commit", "-am", "theirs"]);
    repo.git(&["checkout", "main"]);
    repo.write("file", "ours\n");
    repo.git(&["commit", "-am", "ours"]);
    let output = crate::test_support::git_command()
        .current_dir(&repo.0)
        .args([
            "-c",
            "core.hooksPath=disabled-test-hooks",
            "merge",
            "--no-commit",
            "other",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        load(&SYSTEM_GIT, &repo.0).unwrap().changes[0].kind,
        ChangeKind::Conflicted
    );
    let index_before = std::fs::read(repo.0.join(".git/index")).unwrap();
    assert!(commit(
        &SYSTEM_GIT,
        &repo.0,
        "Must not commit conflict",
        CommitContents::All
    )
    .is_err());
    assert_eq!(
        std::fs::read(repo.0.join(".git/index")).unwrap(),
        index_before
    );
    let patch = diff(&SYSTEM_GIT, &repo.0, "file", ChangeKind::Conflicted, None).unwrap();
    assert!(patch.contains("Conflict: working tree compared with ours"));
    assert!(patch.contains("+<<<<<<<"));
    assert!(!patch.contains("@@@"));
}

#[test]
fn stage_and_unstage_preserve_worktree_before_and_after_first_commit() {
    let repo = TempRepo::new();
    let path = "[x] é.txt";
    repo.write(path, "new\n");
    repo.write("x é.txt", "unrelated\n");
    set_staged(&SYSTEM_GIT, &repo.0, path, true).unwrap();
    let snapshot = load(&SYSTEM_GIT, &repo.0).unwrap();
    assert!(
        snapshot
            .changes
            .iter()
            .find(|c| c.path == path)
            .unwrap()
            .staged
    );
    assert!(
        !snapshot
            .changes
            .iter()
            .find(|c| c.path == "x é.txt")
            .unwrap()
            .staged
    );
    repo.write(path, "new and unstaged\n");
    set_staged(&SYSTEM_GIT, &repo.0, path, false).unwrap();
    assert!(load(&SYSTEM_GIT, &repo.0)
        .unwrap()
        .changes
        .iter()
        .all(|c| !c.staged));
    assert_eq!(
        std::fs::read_to_string(repo.0.join(path)).unwrap(),
        "new and unstaged\n"
    );
    set_staged(&SYSTEM_GIT, &repo.0, path, true).unwrap();
    commit(&SYSTEM_GIT, &repo.0, "First commit", CommitContents::All).unwrap();
    repo.write(path, "index version\n");
    set_staged(&SYSTEM_GIT, &repo.0, path, true).unwrap();
    repo.write(path, "working version\n");
    let snapshot = load(&SYSTEM_GIT, &repo.0).unwrap();
    let mixed = snapshot.changes.iter().find(|c| c.path == path).unwrap();
    assert!(mixed.staged && mixed.unstaged);
    set_staged(&SYSTEM_GIT, &repo.0, path, false).unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.0.join(path)).unwrap(),
        "working version\n"
    );
    assert!(
        !load(&SYSTEM_GIT, &repo.0)
            .unwrap()
            .changes
            .iter()
            .find(|c| c.path == path)
            .unwrap()
            .staged
    );
}

#[test]
fn commit_includes_tracked_unstaged_changes_and_new_files() {
    let repo = TempRepo::new();
    repo.write("file", "base\n");
    repo.git(&["add", "file"]);
    repo.git(&["commit", "-m", "base"]);
    repo.write("file", "working tree\n");
    repo.write("other", "untracked\n");
    let message = "Subject é\n\nBody with `text` and $(touch SHOULD_NOT_EXIST)\n# preserved";
    commit(&SYSTEM_GIT, &repo.0, message, CommitContents::All).unwrap();
    assert_eq!(repo.git(&["show", "HEAD:file"]), "working tree\n");
    assert_eq!(repo.git(&["log", "-1", "--format=%B"]).trim_end(), message);
    assert_eq!(
        repo.git(&["ls-tree", "--name-only", "HEAD"]),
        "file\nother\n"
    );
    assert_eq!(repo.git(&["show", "HEAD:other"]), "untracked\n");
    assert_eq!(
        std::fs::read_to_string(repo.0.join("other")).unwrap(),
        "untracked\n"
    );
    assert!(!repo.0.join("SHOULD_NOT_EXIST").exists());
    let changes = load(&SYSTEM_GIT, &repo.0).unwrap().changes;
    assert!(changes.is_empty());
}

#[test]
fn amend_replaces_root_and_allows_message_only_changes() {
    let repo = TempRepo::new();
    assert!(amend_message(&SYSTEM_GIT, &repo.0)
        .unwrap_err()
        .contains("no commit"));
    repo.write("file", "original\n");
    commit(
        &SYSTEM_GIT,
        &repo.0,
        "Original\n\nBody é",
        CommitContents::All,
    )
    .unwrap();
    let (head, message) = amend_message(&SYSTEM_GIT, &repo.0).unwrap();
    assert_eq!(message, "Original\n\nBody é");
    let author = repo.git(&["show", "-s", "--format=%an <%ae> %aI", "HEAD"]);
    amend_commit(
        &SYSTEM_GIT,
        &repo.0,
        "Reworded\n\nNew body",
        &head,
        CommitContents::All,
    )
    .unwrap();
    assert_ne!(repo.git(&["rev-parse", "HEAD"]).trim(), head);
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]).trim(), "1");
    assert_eq!(repo.git(&["show", "-s", "--format=%P", "HEAD"]).trim(), "");
    assert_eq!(
        repo.git(&["show", "-s", "--format=%an <%ae> %aI", "HEAD"]),
        author
    );
    assert_eq!(
        repo.git(&["log", "-1", "--format=%B"]).trim_end(),
        "Reworded\n\nNew body"
    );
    assert_eq!(repo.git(&["show", "HEAD:file"]), "original\n");
}

#[test]
fn amend_includes_all_changes_and_preserves_parents() {
    let repo = TempRepo::new();
    repo.write("file", "base\n");
    commit(&SYSTEM_GIT, &repo.0, "Base", CommitContents::All).unwrap();
    let parent = repo.git(&["rev-parse", "HEAD"]);
    repo.write("file", "second\n");
    commit(&SYSTEM_GIT, &repo.0, "Second", CommitContents::All).unwrap();
    let (head, _) = amend_message(&SYSTEM_GIT, &repo.0).unwrap();
    repo.write("file", "replacement\n");
    repo.write("new", "new file\n");
    amend_commit(
        &SYSTEM_GIT,
        &repo.0,
        "Replacement",
        &head,
        CommitContents::All,
    )
    .unwrap();
    assert_eq!(repo.git(&["rev-parse", "HEAD^"]), parent);
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]).trim(), "2");
    assert_eq!(repo.git(&["show", "HEAD:file"]), "replacement\n");
    assert_eq!(repo.git(&["show", "HEAD:new"]), "new file\n");
    assert!(load(&SYSTEM_GIT, &repo.0).unwrap().changes.is_empty());
}

#[test]
fn amend_rejects_stale_head_invalid_message_and_in_progress_operations_before_staging() {
    let repo = TempRepo::new();
    repo.write("file", "base\n");
    commit(&SYSTEM_GIT, &repo.0, "Base", CommitContents::All).unwrap();
    let (old_head, _) = amend_message(&SYSTEM_GIT, &repo.0).unwrap();
    repo.write("file", "second\n");
    commit(&SYSTEM_GIT, &repo.0, "Second", CommitContents::All).unwrap();
    let (head, _) = amend_message(&SYSTEM_GIT, &repo.0).unwrap();
    repo.write("new", "not staged\n");
    assert!(amend_commit(
        &SYSTEM_GIT,
        &repo.0,
        "Stale",
        &old_head,
        CommitContents::All
    )
    .unwrap_err()
    .contains("HEAD has changed"));
    assert!(amend_commit(&SYSTEM_GIT, &repo.0, " ", &head, CommitContents::All).is_err());
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        let path = repo.0.join(".git").join(marker);
        std::fs::write(&path, &head).unwrap();
        assert!(
            amend_commit(&SYSTEM_GIT, &repo.0, "Blocked", &head, CommitContents::All)
                .unwrap_err()
                .contains("Finish")
        );
        std::fs::remove_file(path).unwrap();
    }
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), head);
    assert!(repo.git(&["diff", "--cached", "--name-only"]).is_empty());
    assert_eq!(repo.git(&["status", "--porcelain"]).trim(), "?? new");
}

#[test]
fn unfinished_operations_preserve_index_worktree_and_head_in_linked_worktrees() {
    let repo = TempRepo::new();
    repo.write("file", "base\n");
    commit(&SYSTEM_GIT, &repo.0, "Base", CommitContents::All).unwrap();
    repo.git(&["branch", "other"]);
    let linked = repo.0.join("linked");
    repo.git(&["worktree", "add", "--detach", linked.to_str().unwrap()]);
    for root in [&repo.0, &linked] {
        std::fs::write(root.join("file"), "staged\n").unwrap();
        set_staged(&SYSTEM_GIT, root, "file", true).unwrap();
        std::fs::write(root.join("file"), "unstaged\n").unwrap();
        std::fs::write(root.join("new"), "untracked\n").unwrap();
        let head = text(run(&SYSTEM_GIT, root, &["rev-parse", "HEAD"]).unwrap()).unwrap();
        let index = run(&SYSTEM_GIT, root, &["ls-files", "--stage", "-z"]).unwrap();
        for marker in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
            "sequencer",
        ] {
            let marker_path =
                text(run(&SYSTEM_GIT, root, &["rev-parse", "--git-path", marker]).unwrap())
                    .unwrap();
            let marker_path = root.join(marker_path.trim_end_matches('\n'));
            let directory = matches!(marker, "rebase-merge" | "rebase-apply" | "sequencer");
            if directory {
                std::fs::create_dir(&marker_path).unwrap();
            } else {
                std::fs::write(&marker_path, &head).unwrap();
            }
            for mode in [CommitContents::All, CommitContents::Staged] {
                if !matches!(marker, "MERGE_HEAD" | "CHERRY_PICK_HEAD" | "REVERT_HEAD") {
                    assert!(commit(&SYSTEM_GIT, root, "Blocked", mode)
                        .unwrap_err()
                        .contains("Finish"));
                }
                assert!(
                    amend_commit(&SYSTEM_GIT, root, "Blocked", head.trim(), mode)
                        .unwrap_err()
                        .contains("Finish")
                );
            }
            assert!(amend_message(&SYSTEM_GIT, root)
                .unwrap_err()
                .contains("Finish"));
            if !matches!(marker, "MERGE_HEAD" | "CHERRY_PICK_HEAD" | "REVERT_HEAD") {
                assert!(set_staged(&SYSTEM_GIT, root, "file", true)
                    .unwrap_err()
                    .contains("Finish"));
            }
            assert!(set_staged(&SYSTEM_GIT, root, "file", false)
                .unwrap_err()
                .contains("Finish"));
            assert!(
                crate::branches::switch(&SYSTEM_GIT, root, "refs/heads/other")
                    .unwrap_err()
                    .contains("Finish")
            );
            assert_eq!(
                run(&SYSTEM_GIT, root, &["ls-files", "--stage", "-z"]).unwrap(),
                index
            );
            assert_eq!(
                text(run(&SYSTEM_GIT, root, &["rev-parse", "HEAD"]).unwrap()).unwrap(),
                head
            );
            assert_eq!(std::fs::read(root.join("file")).unwrap(), b"unstaged\n");
            assert_eq!(std::fs::read(root.join("new")).unwrap(), b"untracked\n");
            if directory {
                std::fs::remove_dir(marker_path).unwrap();
            } else {
                std::fs::remove_file(marker_path).unwrap();
            }
        }
        require_no_operation(&SYSTEM_GIT, root).unwrap();
    }
}

#[test]
fn stages_deletion_and_unstages_both_sides_of_rename() {
    let repo = TempRepo::new();
    repo.write(
        "old",
        "unchanged content line one\nunchanged content line two\n",
    );
    set_staged(&SYSTEM_GIT, &repo.0, "old", true).unwrap();
    commit(&SYSTEM_GIT, &repo.0, "initial", CommitContents::All).unwrap();
    repo.git(&["mv", "old", "new"]);
    repo.write(
        "new",
        "unchanged content line one\nunchanged content line two\nupdated\n",
    );
    set_staged(&SYSTEM_GIT, &repo.0, "new", true).unwrap();
    set_staged(&SYSTEM_GIT, &repo.0, "new", false).unwrap();
    assert!(load(&SYSTEM_GIT, &repo.0)
        .unwrap()
        .changes
        .iter()
        .all(|c| !c.staged));
    assert!(!repo.0.join("old").exists());
    assert_eq!(
        std::fs::read_to_string(repo.0.join("new")).unwrap(),
        "unchanged content line one\nunchanged content line two\nupdated\n"
    );
    set_staged(&SYSTEM_GIT, &repo.0, "old", true).unwrap();
    assert!(load(&SYSTEM_GIT, &repo.0)
        .unwrap()
        .changes
        .iter()
        .any(|c| c.kind == ChangeKind::Deleted && c.staged));
    set_staged(&SYSTEM_GIT, &repo.0, "old", false).unwrap();
    assert!(!repo.0.join("old").exists());
}

#[test]
fn scoped_staging_rejects_a_rename_source_and_checks_literal_ignored_paths() {
    let repo = TempRepo::new();
    repo.write("old", "rename contents\n");
    repo.write("ordinary", "base\n");
    repo.write(".gitignore", "*.ignored\n");
    commit(&SYSTEM_GIT, &repo.0, "base", CommitContents::All).unwrap();
    repo.git(&["mv", "old", "new"]);
    repo.write("ordinary", "edited\n");
    repo.git(&["add", "ordinary"]);
    let index_before = repo.git(&["diff", "--cached", "--name-status"]);
    assert!(set_staged_paths(
        &SYSTEM_GIT,
        &repo.0,
        &["ordinary".into(), "old".into()],
        false
    )
    .is_err());
    assert_eq!(
        repo.git(&["diff", "--cached", "--name-status"]),
        index_before
    );

    repo.write("[x].ignored", "selected\n");
    repo.write("x.ignored", "unrelated\n");
    set_staged(&SYSTEM_GIT, &repo.0, "[x].ignored", true).unwrap();
    let staged = repo.git(&["diff", "--cached", "--name-only"]);
    assert!(staged.lines().any(|path| path == "[x].ignored"));
    assert!(!staged.lines().any(|path| path == "x.ignored"));
}

#[test]
fn refuses_empty_commit_and_reports_index_lock_without_touching_worktree() {
    let repo = TempRepo::new();
    for message in ["", " \n\t", "bad\0message"] {
        assert!(validate_message(message).is_err());
    }
    assert!(validate_message(&"a".repeat(65_537)).is_err());
    assert!(
        commit(&SYSTEM_GIT, &repo.0, "Nothing changed", CommitContents::All)
            .unwrap_err()
            .contains("no changes")
    );
    repo.write("file", "content\n");
    std::fs::write(repo.0.join(".git/index.lock"), "locked").unwrap();
    assert!(
        commit(&SYSTEM_GIT, &repo.0, "Add new file", CommitContents::All)
            .unwrap_err()
            .contains("locked")
    );
    assert!(set_staged(&SYSTEM_GIT, &repo.0, "file", true)
        .unwrap_err()
        .contains("locked"));
    assert_eq!(
        std::fs::read_to_string(repo.0.join("file")).unwrap(),
        "content\n"
    );
}

#[cfg(unix)]
#[test]
fn commit_respects_rejecting_hook_and_keeps_index() {
    use std::os::unix::fs::PermissionsExt;
    let repo = TempRepo::new();
    repo.write("file", "content\n");
    commit(&SYSTEM_GIT, &repo.0, "Base", CommitContents::All).unwrap();
    let (head, _) = amend_message(&SYSTEM_GIT, &repo.0).unwrap();
    repo.write("file", "updated\n");
    set_staged(&SYSTEM_GIT, &repo.0, "file", true).unwrap();
    let hooks = repo.0.join("test-hooks");
    std::fs::create_dir(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'unlabelled-hook-content' >&2\necho 'unlabelled-hook-stdout'\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    repo.git(&[
        "config",
        "--local",
        "core.hooksPath",
        hooks.to_str().unwrap(),
    ]);
    repo.write("file", "unstaged after selection\n");
    repo.write("untracked", "not selected\n");
    let index = repo.git(&["ls-files", "--stage", "-z"]);
    let error = commit(
        &SYSTEM_GIT,
        &repo.0,
        "Rejected staged",
        CommitContents::Staged,
    )
    .unwrap_err();
    assert!(error.contains("exit code: 1"));
    assert!(!error.contains("unlabelled-hook"));
    assert!(amend_commit(
        &SYSTEM_GIT,
        &repo.0,
        "Rejected staged amend",
        &head,
        CommitContents::Staged
    )
    .is_err());
    assert_eq!(repo.git(&["ls-files", "--stage", "-z"]), index);
    assert_eq!(repo.git(&["show", ":file"]), "updated\n");
    assert_eq!(
        std::fs::read_to_string(repo.0.join("file")).unwrap(),
        "unstaged after selection\n"
    );
    assert!(
        commit(&SYSTEM_GIT, &repo.0, "Rejected", CommitContents::All)
            .unwrap_err()
            .contains("hooks")
    );
    assert!(load(&SYSTEM_GIT, &repo.0).unwrap().changes[0].staged);
    assert!(amend_commit(
        &SYSTEM_GIT,
        &repo.0,
        "Rejected amend",
        &head,
        CommitContents::All
    )
    .unwrap_err()
    .contains("hooks"));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), head);
    assert!(load(&SYSTEM_GIT, &repo.0).unwrap().changes[0].staged);
}

#[test]
fn rejects_non_repository_and_bare_repository() {
    let repo = TempRepo::new();
    let missing = repo.0.join("missing");
    assert!(load(&SYSTEM_GIT, &missing).is_err());
    let bare = repo.0.join("bare.git");
    repo.git(&["init", "--bare", bare.to_str().unwrap()]);
    assert!(load(&SYSTEM_GIT, &bare).is_err());
}
