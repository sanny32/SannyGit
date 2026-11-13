//! Opt-in C3 measurements through the real backend and polling coordinators.
use super::*;
use crate::{
    history,
    models::{ChangeKind, DiffPreview},
    test_support::{GitCommandCount, TempRepo},
    theme::ThemeChoice,
    tracking,
};
use std::{fmt::Write, sync::atomic::AtomicBool};

const SAMPLES: usize = 5;

fn fixture(files: usize, commits: usize) -> TempRepo {
    let mut repo = TempRepo::new();
    // Match Git's canonical --show-toplevel path, including macOS /private/var.
    repo.0 = repo.0.canonicalize().unwrap();
    let mut input = String::new();
    for commit in 1..=commits {
        writeln!(input, "commit refs/heads/main\nmark :{commit}\ncommitter Performance Test <test@example.invalid> {} +0000\ndata 6\nbench", 1_700_000_000 + commit).unwrap();
        if commit > 1 {
            writeln!(input, "from :{}", commit - 1).unwrap();
        }
        for file in 0..if commit == 1 { files } else { 1 } {
            let content = format!("base {commit}\n");
            writeln!(
                input,
                "M 100644 inline file-{file:05}\ndata {}\n{content}",
                content.len()
            )
            .unwrap();
        }
        input.push('\n');
    }
    git::run_with_input(&repo.0, &["fast-import", "--quiet"], Some(input.as_bytes())).unwrap();
    // All writes are confined to this disposable repository.
    repo.git(&["restore", "--source=HEAD", "--staged", "--worktree", "."]);
    repo.git(&["branch", "baseline", "HEAD~1"]);
    repo.git(&["config", "branch.main.remote", "."]);
    repo.git(&["config", "branch.main.merge", "refs/heads/baseline"]);
    repo
}

fn measure<T>(repo: &TempRepo, name: &str, mut operation: impl FnMut() -> T) {
    drop(operation()); // Warm caches; fixture construction is excluded.
    let counter = GitCommandCount::new(&repo.0);
    let mut elapsed = Vec::new();
    for _ in 0..SAMPLES {
        let start = Instant::now();
        let result = operation();
        elapsed.push(start.elapsed().as_secs_f64() * 1000.0);
        drop(result);
    }
    elapsed.sort_by(f64::total_cmp);
    assert!(
        counter.get() > 0,
        "Measurement did not observe Git commands"
    );
    assert_eq!(counter.get() % SAMPLES, 0);
    println!(
        "{}",
        serde_json::json!({
            "operation": name, "samples": SAMPLES,
            "median_ms": elapsed[SAMPLES / 2], "min_ms": elapsed[0],
            "max_ms": elapsed[SAMPLES - 1],
            "git_processes_total": counter.get(),
        })
    );
}

fn polling(repo: &TempRepo, dirty: bool) {
    let context = eframe::egui::Context::default();
    let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
    app.apply_snapshot(git::load(&repo.0).unwrap());
    // Exercise the real monitors without auto-fetch or interactive history work.
    // The local upstream is one commit behind and needs an outgoing-file read.
    app.tracking.set_auto_fetch(false);
    let counter = GitCommandCount::new(&repo.0);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(12) {
        app.poll_working_tree();
        app.check_working_tree(&context);
        app.tracking
            .update(Some(repo.0.clone()), true, "", &context);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.tracking.error.is_none(), "{:?}", app.tracking.error);
        std::thread::sleep(Duration::from_millis(10));
    }
    // Drain a read started just before the observation window ended.
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        app.poll_working_tree();
        app.tracking
            .update(Some(repo.0.clone()), false, "", &context);
        if app.working_tree.receiver.is_none() && !app.tracking.has_pending_read() {
            break;
        }
        assert!(Instant::now() < deadline, "Polling worker did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
    let seconds = start.elapsed().as_secs_f64();
    println!(
        "{}",
        serde_json::json!({
            "operation": if dirty { "poll_dirty" } else { "poll_clean" },
            "seconds": seconds, "git_processes_total": counter.get(),
            "git_processes_per_minute": counter.get() as f64 * 60.0 / seconds,
        })
    );
    app.tracking.cancel();
}

#[test]
#[ignore = "opt-in performance measurements; creates temporary repositories and observes polling"]
fn backend_measurements() {
    println!(
        "{}",
        serde_json::json!({"os": std::env::consts::OS,
        "arch": std::env::consts::ARCH, "debug_assertions": cfg!(debug_assertions),
        "git": String::from_utf8(crate::test_support::git_command().arg("--version").output().unwrap().stdout).unwrap().trim()})
    );
    for (files, commits) in [(100, 100), (1_000, 1_000), (5_000, 10_000)] {
        let repo = fixture(files, commits);
        println!(
            "{}",
            serde_json::json!({"files": files, "commits": commits})
        );
        let cancel = AtomicBool::new(false);
        measure(&repo, "status_clean", || git::load(&repo.0).unwrap());
        measure(&repo, "tracking", || tracking::read(&repo.0).unwrap());
        measure(&repo, "history_first", || {
            history::reload(&repo.0, false, history::PAGE_SIZE, &[], &cancel).unwrap()
        });
        let page = history::reload(&repo.0, false, history::PAGE_SIZE, &[], &cancel).unwrap();
        if page.cursor.has_more() {
            measure(&repo, "history_more", || {
                history::more(&repo.0, &page.cursor, page.history.commits.clone(), &cancel).unwrap()
            });
        }
        let root = repo.git(&["rev-list", "--max-parents=0", "HEAD"]);
        measure(&repo, "commit_files_root", || {
            history::files(&repo.0, root.trim(), None).unwrap()
        });
        let selected = history::files(&repo.0, root.trim(), None)
            .unwrap()
            .remove(0);
        measure(&repo, "commit_file_diff", || {
            let patch = history::file_patch(&repo.0, root.trim(), None, &selected).unwrap();
            DiffPreview::new(&patch)
        });
        polling(&repo, false);
        for index in 0..(files / 100).max(1) {
            repo.write(&format!("file-{index:05}"), &"changed\n".repeat(1_000));
            repo.write(&format!("new-{index:05}"), "untracked\n");
        }
        repo.git(&["add", "file-00000"]);
        repo.write("file-00000", &"worktree\n".repeat(1_000));
        measure(&repo, "status_dirty", || git::load(&repo.0).unwrap());
        measure(&repo, "working_file_diff", || {
            let patch = git::diff(&repo.0, "file-00000", ChangeKind::Modified, None).unwrap();
            DiffPreview::new(&patch)
        });
        polling(&repo, true);
    }
}
