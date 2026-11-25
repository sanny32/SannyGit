//! Shared fixtures for tests that use isolated temporary Git repositories.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) fn git_command() -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new("git");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command
}

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

fn configured_template() -> &'static Path {
    static TEMPLATE: OnceLock<PathBuf> = OnceLock::new();
    TEMPLATE.get_or_init(|| {
        let path =
            std::env::temp_dir().join(format!("sannygit-test-template-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        run_git(&path, &["init", "--initial-branch=main"]);
        // Isolate backend write tests from the developer's identity, hooks and signing.
        for (key, value) in [
            ("user.name", "SannyGit Test"),
            ("user.email", "test@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", "disabled-test-hooks"),
            ("core.autocrlf", "false"),
        ] {
            run_git(&path, &["config", "--local", key, value]);
        }
        path
    })
}

fn run_git(path: &Path, args: &[&str]) {
    let output = git_command().current_dir(path).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
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
        // Copy one configured repository instead of spawning `git init`
        // and five `git config` processes for every test.
        copy_dir(configured_template(), &path);
        Self(path)
    }
    pub(crate) fn git(&self, args: &[&str]) -> String {
        let output = git_command()
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
    crate::git::run_with_input(
        &SYSTEM_GIT,
        &repo.0,
        &["fast-import", "--quiet"],
        Some(input.as_bytes()),
    )
    .unwrap();
    repo.write("untracked", "keep local bytes\n");
    repo
}

/// In-memory [`eframe::Storage`] that lets UI tests round-trip persisted state
/// through the same RON encoding eframe uses for the app storage file.
#[derive(Default)]
pub(crate) struct MemoryStorage(std::collections::BTreeMap<String, String>);

impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }

    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }

    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }

    fn flush(&mut self) {}
}

/// Serialize the egui memory like `eframe` does on exit and read it back, as
/// the native runner does on startup after a restart.
pub(crate) fn restart_memory(ctx: &eframe::egui::Context) -> eframe::egui::Memory {
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, "egui", &ctx.memory(|memory| memory.clone()));
    eframe::get_value(&storage, "egui").expect("egui memory must round-trip")
}

pub(crate) static SYSTEM_GIT: std::sync::LazyLock<crate::git::GitContext> =
    std::sync::LazyLock::new(|| crate::git::GitContext::new(String::new()));

#[test]
fn fresh_repositories_are_independent_copies() {
    let first = TempRepo::new();
    let second = TempRepo::new();
    assert_ne!(first.0, second.0);
    for repo in [&first, &second] {
        assert_eq!(
            repo.git(&["symbolic-ref", "--short", "HEAD"]).trim(),
            "main"
        );
        assert_eq!(
            repo.git(&["config", "--local", "user.name"]).trim(),
            "SannyGit Test"
        );
        assert_eq!(
            repo.git(&["config", "--local", "user.email"]).trim(),
            "test@example.invalid"
        );
        assert_eq!(
            repo.git(&["config", "--local", "commit.gpgsign"]).trim(),
            "false"
        );
        assert_eq!(
            repo.git(&["config", "--local", "core.hooksPath"]).trim(),
            "disabled-test-hooks"
        );
        assert_eq!(
            repo.git(&["config", "--local", "core.autocrlf"]).trim(),
            "false"
        );
    }
    first.git(&["config", "--local", "user.name", "Changed"]);
    first.write("only-first", "a");
    assert_eq!(
        second.git(&["config", "--local", "user.name"]).trim(),
        "SannyGit Test"
    );
    assert!(!second.0.join("only-first").exists());
}
