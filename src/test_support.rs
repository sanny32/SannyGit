//! Shared fixtures for tests that use isolated temporary Git repositories.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex, Weak,
    },
};

static NEXT: AtomicU64 = AtomicU64::new(0);

// Opt-in, repository-scoped counters also observe background workers without
// counting other tests' temporary repositories. Never compiled into the app.
static COMMAND_COUNTS: Mutex<Vec<(PathBuf, Weak<AtomicUsize>)>> = Mutex::new(Vec::new());

pub(crate) struct GitCommandCount(Arc<AtomicUsize>);

impl GitCommandCount {
    pub(crate) fn new(root: &Path) -> Self {
        let counter = Self(Arc::new(AtomicUsize::new(0)));
        let mut counters = COMMAND_COUNTS.lock().unwrap();
        counters.retain(|(_, count)| count.strong_count() > 0);
        counters.push((root.to_owned(), Arc::downgrade(&counter.0)));
        counter
    }

    pub(crate) fn get(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

pub(crate) fn record_git_command(root: &Path) {
    for (path, counter) in COMMAND_COUNTS.lock().unwrap().iter() {
        if path == root {
            if let Some(counter) = counter.upgrade() {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

pub(crate) struct TempRepo(pub(crate) PathBuf);
impl TempRepo {
    pub(crate) fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "sannygit-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        let repo = Self(path);
        repo.git(&["init", "--initial-branch=main"]);
        // Isolate backend write tests from the developer's identity, hooks and signing.
        for (key, value) in [
            ("user.name", "SannyGit Test"),
            ("user.email", "test@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", "disabled-test-hooks"),
            ("core.autocrlf", "false"),
        ] {
            repo.git(&["config", "--local", key, value]);
        }
        repo
    }
    pub(crate) fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(&self.0)
            .args([
                "-c",
                "user.name=SannyGit Test",
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=disabled-test-hooks",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
    pub(crate) fn write(&self, path: &str, content: &str) {
        std::fs::write(self.0.join(path), content).unwrap();
    }
}
impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn linear_repo(count: usize) -> TempRepo {
    let repo = TempRepo::new();
    let mut input = String::new();
    for index in 1..=count {
        let message = format!("History commit {index}\n");
        let contents = format!("Contents {index}\n");
        input.push_str(&format!("commit refs/heads/main\nmark :{index}\ncommitter History Test <history@example.invalid> {} +0000\ndata {}\n{message}", 1_700_000_000 + index, message.len()));
        if index > 1 {
            input.push_str(&format!("from :{}\n", index - 1));
        }
        input.push_str(&format!(
            "M 100644 inline file\ndata {}\n{contents}\n",
            contents.len()
        ));
    }
    crate::git::run_with_input(&repo.0, &["fast-import", "--quiet"], Some(input.as_bytes()))
        .unwrap();
    repo.write("untracked", "keep local bytes\n");
    repo
}
