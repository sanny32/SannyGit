//! Repository lifecycle, discovery, worktrees and configuration. No UI rendering.
use crate::{git, models::Repository};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct RepositoryGroup {
    pub name: String,
    pub paths: Vec<String>,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Preferences {
    pub groups: Vec<RepositoryGroup>,
    pub selected_group: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub main: bool,
    pub locked: bool,
    pub prunable: bool,
    pub bare: bool,
}
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub name: String,
    pub email: String,
}

pub struct SearchResult {
    pub repositories: Vec<Repository>,
    pub limited: bool,
    pub skipped: usize,
}

fn new_destination(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    if fs::symlink_metadata(&absolute).is_ok() {
        return Err("Choose a destination that does not exist yet.".into());
    }
    let parent = absolute
        .parent()
        .ok_or("Destination has no parent directory")?;
    if !parent.is_dir() {
        return Err("The destination's parent directory must already exist.".into());
    }
    Ok(absolute)
}

pub fn init(path: &Path) -> Result<git::Snapshot, String> {
    let path = if path.exists() {
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        if !path.is_dir()
            || fs::symlink_metadata(path.join(".git")).is_ok()
            || git::run(&path, &["rev-parse", "--show-toplevel"]).is_ok()
        {
            return Err("Choose a directory outside existing Git working trees.".into());
        }
        path
    } else {
        new_destination(path)?
    };
    git::run(
        path.parent().ok_or("Missing parent")?,
        &[
            "init",
            "--initial-branch=main",
            "--",
            path.to_str().ok_or("Non-UTF-8 destination")?,
        ],
    )?;
    git::load(&path)
}

pub fn expand_path(value: &str) -> PathBuf {
    if value == "~" || value.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home).join(value.strip_prefix("~/").unwrap_or(""));
        }
    }
    PathBuf::from(value)
}

pub fn add_or_create(value: &str) -> Result<git::Snapshot, String> {
    if value.trim().is_empty() {
        return Err("Choose a repository directory.".into());
    }
    let path = expand_path(value);
    if path.exists() {
        let loaded = git::load(&path);
        if loaded.is_ok() {
            return loaded;
        }
        if !path.is_dir() || fs::symlink_metadata(path.join(".git")).is_ok() {
            return loaded;
        }
        if fs::read_dir(&path)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
        {
            return Err("To create a repository, choose an empty directory.".into());
        }
    }
    init(&path)
}

#[derive(Clone, Debug)]
pub struct CloneOptions {
    pub include_submodules: bool,
    pub fetch_all: bool,
    pub branch: String,
}
impl Default for CloneOptions {
    fn default() -> Self {
        Self {
            include_submodules: true,
            fetch_all: true,
            branch: String::new(),
        }
    }
}

pub fn clone_branches(source: &str, cancel: &AtomicBool) -> Result<Vec<String>, String> {
    let source = clone_source(source)?;
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let output = git::run_controlled(
        &root,
        &[
            "ls-remote",
            "--symref",
            "--",
            &source,
            "HEAD",
            "refs/heads/*",
        ],
        Duration::from_secs(120),
        Some(cancel),
    )?;
    parse_clone_branches(std::str::from_utf8(&output).map_err(|_| "Non-UTF-8 remote branches")?)
}

fn parse_clone_branches(output: &str) -> Result<Vec<String>, String> {
    let mut branches = Vec::new();
    let mut default = None;
    for line in output.lines() {
        if let Some(name) = line
            .strip_prefix("ref: refs/heads/")
            .and_then(|s| s.strip_suffix("\tHEAD"))
        {
            default = Some(name.to_owned());
        } else if let Some((oid, name)) = line.split_once("\trefs/heads/") {
            if !matches!(oid.len(), 40 | 64)
                || !oid.bytes().all(|b| b.is_ascii_hexdigit())
                || name.is_empty()
            {
                return Err("Invalid remote branch response".into());
            }
            branches.push(name.to_owned());
            if branches.len() > 5000 {
                return Err("Too many remote branches".into());
            }
        }
    }
    branches.sort();
    branches.dedup();
    if let Some(default) = default {
        if let Some(index) = branches.iter().position(|b| b == &default) {
            branches.swap(0, index);
        }
    }
    Ok(branches)
}

fn clone_source(source: &str) -> Result<String, String> {
    if source.is_empty() || source.starts_with('-') || source.chars().any(char::is_control) {
        return Err("Enter a valid repository URL or local path.".into());
    }
    if source.split_once("::").is_some_and(|(prefix, _)| {
        prefix
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }) {
        return Err("Unsupported clone transport".into());
    }
    if let Some((scheme, rest)) = source.split_once("://") {
        if !["https", "http", "ssh", "git", "file"].contains(&scheme) {
            return Err("Unsupported clone URL scheme".into());
        }
        let authority = rest.split('/').next().unwrap_or("");
        if let Some((userinfo, _)) = authority.rsplit_once('@') {
            if matches!(scheme, "http" | "https") || userinfo.contains(':') {
                return Err(
                    "Use Git credential helpers or SSH instead of credentials in the URL.".into(),
                );
            }
        }
    }
    let source = if expand_path(source).exists() {
        expand_path(source)
            .canonicalize()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned()
    } else {
        source.into()
    };
    Ok(source)
}

#[cfg(test)]
pub fn clone_repository(
    source: &str,
    path: &Path,
    cancel: &AtomicBool,
) -> Result<git::Snapshot, String> {
    clone_with_options(source, path, &CloneOptions::default(), cancel)
}

pub fn clone_with_options(
    source: &str,
    path: &Path,
    options: &CloneOptions,
    cancel: &AtomicBool,
) -> Result<git::Snapshot, String> {
    let source = clone_source(source)?;
    let path = new_destination(path)?;
    let mut args = vec![
        "clone",
        if options.include_submodules {
            "--recurse-submodules"
        } else {
            "--no-recurse-submodules"
        },
    ];
    if !options.fetch_all {
        args.extend(["--single-branch", "--no-tags"]);
    }
    if !options.branch.is_empty() {
        args.extend(["--branch", &options.branch]);
    }
    args.extend(["--", &source, path.to_str().ok_or("Non-UTF-8 destination")?]);
    git::run_controlled(
        path.parent().ok_or("Missing parent")?,
        &args,
        Duration::from_secs(600),
        Some(cancel),
    )?;
    git::load(&path)
}

pub fn search(root: &Path, depth: usize, cancel: &AtomicBool) -> Result<SearchResult, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("Cannot search directory: {e}"))?;
    if !root.is_dir() {
        return Err("Choose a directory to search.".into());
    }
    let mut pending = vec![(root, 0)];
    let mut seen = HashSet::new();
    let mut result = SearchResult {
        repositories: Vec::new(),
        limited: false,
        skipped: 0,
    };
    let start = Instant::now();
    let mut visited = 0;
    let mut examined = 0;
    while let Some((path, level)) = pending.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Search cancelled".into());
        }
        if visited >= 20_000
            || examined >= 100_000
            || result.repositories.len() >= 200
            || start.elapsed() > Duration::from_secs(30)
        {
            result.limited = true;
            break;
        }
        visited += 1;
        if path.join(".git").exists() {
            match git::run(&path, &["rev-parse", "--show-toplevel"]).and_then(git::text) {
                Ok(root) => {
                    let root = root.trim_end_matches('\n').to_owned();
                    if seen.insert(root.clone()) {
                        result.repositories.push(Repository {
                            name: Path::new(&root)
                                .file_name()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_else(|| root.clone()),
                            path: root,
                        });
                    }
                }
                Err(_) => result.skipped += 1,
            }
        }
        if level >= depth.min(16) {
            result.limited = true;
            continue;
        }
        match fs::read_dir(&path) {
            Ok(entries) => {
                for entry in entries {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Search cancelled".into());
                    }
                    if examined >= 100_000
                        || visited + pending.len() >= 20_000
                        || start.elapsed() > Duration::from_secs(30)
                    {
                        result.limited = true;
                        break;
                    }
                    examined += 1;
                    match entry {
                        Ok(entry)
                            if entry.file_type().is_ok_and(|t| t.is_dir())
                                && !skip_dir(&entry.file_name().to_string_lossy()) =>
                        {
                            pending.push((entry.path(), level + 1))
                        }
                        Err(_) => result.skipped += 1,
                        _ => {}
                    }
                }
            }
            Err(_) => result.skipped += 1,
        }
    }
    result.repositories.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}
fn skip_dir(name: &str) -> bool {
    matches!(name, ".git" | "target" | "node_modules")
}

pub fn worktrees(root: &Path) -> Result<Vec<Worktree>, String> {
    // -z is available since Git 2.36; paths can contain newlines.
    parse_worktrees(&git::run(root, &["worktree", "list", "--porcelain", "-z"])?)
}

fn parse_worktrees(bytes: &[u8]) -> Result<Vec<Worktree>, String> {
    if !bytes.is_empty() && !bytes.ends_with(&[0, 0]) {
        return Err("Truncated worktree output".into());
    }
    let mut result = Vec::new();
    let mut current: Option<Worktree> = None;
    for field in bytes.split(|b| *b == 0) {
        if field.is_empty() {
            if let Some(tree) = current.take() {
                result.push(tree);
            }
            continue;
        }
        let value =
            std::str::from_utf8(field).map_err(|_| "Non-UTF-8 worktree paths are not supported")?;
        if let Some(path) = value.strip_prefix("worktree ") {
            if current.is_some() || path.is_empty() {
                return Err("Malformed worktree output".into());
            }
            current = Some(Worktree {
                path: path.into(),
                branch: None,
                main: result.is_empty(),
                locked: false,
                prunable: false,
                bare: false,
            });
        } else {
            let tree = current.as_mut().ok_or("Missing worktree path")?;
            if let Some(branch) = value.strip_prefix("branch ") {
                tree.branch = Some(branch.trim_start_matches("refs/heads/").into());
            } else if value == "locked" || value.starts_with("locked ") {
                tree.locked = true;
            } else if value == "prunable" || value.starts_with("prunable ") {
                tree.prunable = true;
            } else if value == "bare" {
                tree.bare = true;
            } else if value != "detached" && !value.starts_with("HEAD ") {
                return Err("Unexpected worktree field".into());
            }
        }
        if result.len() >= 500 {
            return Err("Worktree list exceeds the 500-entry limit".into());
        }
    }
    if current.is_some() {
        return Err("Truncated worktree record".into());
    }
    Ok(result)
}

pub fn add_worktree(
    root: &Path,
    destination: &Path,
    branch: &str,
    new_branch: bool,
    cancel: &AtomicBool,
) -> Result<Vec<Worktree>, String> {
    let path = new_destination(destination)?;
    if let Some(reason) = crate::branches::name_hint(branch) {
        return Err(reason.into());
    }
    git::run(root, &["check-ref-format", "--branch", branch])?;
    let refs = crate::branches::load(root)?;
    let existing = refs
        .refs
        .iter()
        .any(|r| r.full_name == format!("refs/heads/{branch}"));
    if existing == new_branch {
        return Err(if new_branch {
            "Branch already exists"
        } else {
            "Select an existing local branch"
        }
        .into());
    }
    let path = path.to_str().ok_or("Non-UTF-8 destination")?;
    let args = if new_branch {
        vec!["worktree", "add", "-b", branch, "--", path, "HEAD"]
    } else {
        vec!["worktree", "add", "--", path, branch]
    };
    git::run_controlled(root, &args, Duration::from_secs(120), Some(cancel))?;
    worktrees(root)
}

pub fn remove_worktree(root: &Path, path: &Path) -> Result<Vec<Worktree>, String> {
    let trees = worktrees(root)?;
    let target = trees
        .iter()
        .find(|t| {
            Path::new(&t.path) == path
                || path.canonicalize().ok().is_some_and(|canonical| {
                    Path::new(&t.path).canonicalize().ok().as_ref() == Some(&canonical)
                })
        })
        .ok_or("Select a registered worktree")?;
    let path = Path::new(&target.path);
    if target.main
        || target.bare
        || target.locked
        || path.canonicalize().ok() == root.canonicalize().ok()
    {
        return Err("The main, active or locked worktree cannot be removed.".into());
    }
    if !git::load(path)?.changes.is_empty() {
        return Err("The worktree contains local changes; removal was refused.".into());
    }
    // Include ignored files: Git's normal remove would allow losing those.
    if !git::run(path, &["ls-files", "--others", "-z"])?.is_empty() {
        return Err(
            "The worktree contains untracked or ignored files; removal was refused.".into(),
        );
    }
    git::run(
        root,
        &[
            "worktree",
            "remove",
            "--",
            path.to_str().ok_or("Non-UTF-8 worktree path")?,
        ],
    )?;
    worktrees(root)
}

pub fn prune_preview(root: &Path) -> Result<String, String> {
    git::text(git::run_diagnostic(
        root,
        &[
            "worktree",
            "prune",
            "--dry-run",
            "--verbose",
            "--expire=now",
        ],
    )?)
}
pub fn prune(root: &Path) -> Result<Vec<Worktree>, String> {
    git::run(root, &["worktree", "prune", "--expire=now"])?;
    worktrees(root)
}

pub fn settings(root: &Path) -> Result<Settings, String> {
    let output = git::text(git::run(root, &["config", "--local", "--null", "--list"])?)?;
    let mut settings = Settings::default();
    for item in output.split('\0') {
        if let Some((key, value)) = item.split_once('\n') {
            match key {
                "user.name" => settings.name = value.into(),
                "user.email" => settings.email = value.into(),
                _ => {}
            }
        }
    }
    Ok(settings)
}
pub fn save_settings(root: &Path, values: &Settings) -> Result<Settings, String> {
    if [&values.name, &values.email]
        .iter()
        .any(|v| v.len() > 4096 || v.chars().any(char::is_control))
    {
        return Err("Identity values must be single lines up to 4 KiB.".into());
    }
    let existing = git::text(git::run(root, &["config", "--local", "--null", "--list"])?)?;
    for (key, value) in [("user.name", &values.name), ("user.email", &values.email)] {
        if value.is_empty() {
            if existing
                .split('\0')
                .any(|item| item.split_once('\n').is_some_and(|(name, _)| name == key))
            {
                git::run(root, &["config", "--local", "--unset-all", key])?;
            }
        } else {
            git::run(root, &["config", "--local", "--replace-all", key, value])?;
        }
    }
    settings(root)
}

pub fn config_path(root: &Path) -> Result<PathBuf, String> {
    let common = git::text(git::run(root, &["rev-parse", "--git-common-dir"])?)?;
    let common = PathBuf::from(common.trim_end_matches('\n'));
    Ok(if common.is_absolute() {
        common
    } else {
        root.join(common)
    }
    .join("config"))
}
pub fn user_config_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("GIT_CONFIG_GLOBAL") {
        return Ok(path.into());
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or("Cannot find the home directory")?;
    let main = PathBuf::from(home).join(".gitconfig");
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| main.parent().unwrap_or(Path::new(".")).join(".config"))
        .join("git/config");
    Ok(if main.exists() || !xdg.exists() {
        main
    } else {
        xdg
    })
}
pub fn edit_config(path: &Path) -> Result<(), String> {
    let absolute;
    let path = if path.is_absolute() {
        path
    } else {
        absolute = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path);
        &absolute
    };
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
    }
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut cmd = Command::new("open");
        cmd.arg("-t");
        cmd
    };
    #[cfg(target_os = "windows")]
    let mut command = Command::new("notepad.exe");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = Command::new("xdg-open");
    let mut child = command
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Cannot open text editor: {e}"))?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err("The system could not open a text editor for this config file.".into())
                }
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
pub fn new_window() -> Result<(), String> {
    let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .env("SANNYGIT_SECONDARY_WINDOW", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Cannot open new window: {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::TempRepo;

    #[test]
    fn init_creates_new_or_existing_directory_without_reinitializing_repositories() {
        let container = TempRepo::new();
        let base = container.0.join("outside");
        fs::create_dir(&base).unwrap();
        // Use a separate parent outside the fixture working tree.
        let outside = std::env::temp_dir().join(format!(
            "sannygit-init-{}",
            container.0.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir(&outside).unwrap();
        let snapshot = init(&outside.join("new é")).unwrap();
        assert_eq!(snapshot.branch, "main");
        assert!(snapshot.changes.is_empty());
        assert!(init(Path::new(&snapshot.repository.path)).is_err());
        let existing = outside.join("existing");
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("keep"), "user bytes").unwrap();
        assert_eq!(init(&existing).unwrap().changes.len(), 1);
        assert_eq!(
            fs::read_to_string(existing.join("keep")).unwrap(),
            "user bytes"
        );
        assert!(init(&base).is_err());
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn clone_uses_local_source_preserves_existing_destinations_and_supports_cancel() {
        let repo = TempRepo::new();
        repo.write("file", "fixture\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "fixture"]);
        let destination = repo.0.join("clone é");
        let cancel = AtomicBool::new(false);
        let snapshot = clone_repository(repo.0.to_str().unwrap(), &destination, &cancel).unwrap();
        assert!(snapshot.changes.is_empty());
        assert_eq!(
            fs::read_to_string(destination.join("file")).unwrap(),
            "fixture\n"
        );
        assert!(clone_repository(repo.0.to_str().unwrap(), &destination, &cancel).is_err());
        cancel.store(true, Ordering::Relaxed);
        assert!(
            clone_repository(repo.0.to_str().unwrap(), &repo.0.join("cancelled"), &cancel).is_err()
        );
        assert!(!repo.0.join("cancelled").exists());
        for source in [
            "--help",
            "https://token@example.invalid/repo",
            "ssh://git:password@example.invalid/repo",
            "ext::not-supported",
        ] {
            assert!(
                clone_repository(source, &repo.0.join("invalid"), &AtomicBool::new(false)).is_err()
            );
            assert!(!repo.0.join("invalid").exists());
        }
    }

    #[test]
    fn add_or_create_preserves_nonempty_directories() {
        let repo = TempRepo::new();
        let empty = repo.0.join("empty");
        fs::create_dir(&empty).unwrap();
        // An empty child of an existing repository opens that repository.
        assert!(add_or_create(empty.to_str().unwrap()).is_ok());
        let outside = std::env::temp_dir().join(format!("sannygit-empty-{}", std::process::id()));
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), "contents").unwrap();
        assert!(add_or_create(outside.to_str().unwrap()).is_err());
        assert!(!outside.join(".git").exists());
        assert_eq!(
            fs::read_to_string(outside.join("keep")).unwrap(),
            "contents"
        );
        fs::remove_file(outside.join("keep")).unwrap();
        assert!(add_or_create(outside.to_str().unwrap()).is_ok());
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn clone_selects_branch_and_limits_fetch() {
        let repo = TempRepo::new();
        repo.write("file", "main");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "main"]);
        repo.git(&["branch", "feature"]);
        repo.git(&["tag", "v1"]);
        let cancel = AtomicBool::new(false);
        let branches = clone_branches(repo.0.to_str().unwrap(), &cancel).unwrap();
        assert_eq!(branches.first().map(String::as_str), Some("main"));
        assert!(branches.contains(&"feature".into()));
        let destination = repo.0.join("limited");
        clone_with_options(
            repo.0.to_str().unwrap(),
            &destination,
            &CloneOptions {
                branch: "feature".into(),
                include_submodules: false,
                fetch_all: false,
            },
            &cancel,
        )
        .unwrap();
        assert_eq!(
            git::run(&destination, &["branch", "--show-current"]).unwrap(),
            b"feature\n"
        );
        assert!(git::run(&destination, &["tag", "--list"])
            .unwrap()
            .is_empty());
        let refs = git::run(
            &destination,
            &["for-each-ref", "--format=%(refname)", "refs/remotes/origin"],
        )
        .unwrap();
        assert!(!String::from_utf8(refs).unwrap().contains("origin/main"));
        assert!(parse_clone_branches("bad\trefs/heads/main").is_err());
    }

    #[test]
    fn search_discovers_worktrees_respects_depth_and_skips_symlinks() {
        let repo = TempRepo::new();
        repo.write("file", "fixture");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "fixture"]);
        let linked = repo.0.join("nested");
        add_worktree(&repo.0, &linked, "linked", true, &AtomicBool::new(false)).unwrap();
        let shallow = search(&repo.0, 0, &AtomicBool::new(false)).unwrap();
        assert_eq!(shallow.repositories.len(), 1);
        assert!(shallow.limited);
        let deep = search(&repo.0, 4, &AtomicBool::new(false)).unwrap();
        assert_eq!(deep.repositories.len(), 2);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&repo.0, repo.0.join("loop")).unwrap();
            assert_eq!(
                search(&repo.0, 4, &AtomicBool::new(false))
                    .unwrap()
                    .repositories
                    .len(),
                2
            );
        }
        assert!(search(&repo.0, 4, &AtomicBool::new(true)).is_err());
    }

    #[test]
    fn worktree_lifecycle_preserves_ignored_dirty_active_and_locked_directories() {
        let repo = TempRepo::new();
        repo.write("file", "fixture\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "fixture"]);
        let linked = repo.0.join("linked");
        let trees =
            add_worktree(&repo.0, &linked, "feature", true, &AtomicBool::new(false)).unwrap();
        assert_eq!(trees.len(), 2);
        assert!(trees[0].main);
        assert!(remove_worktree(&repo.0, &repo.0).is_err());
        assert!(remove_worktree(&linked, &linked).is_err());
        repo.git(&["worktree", "lock", linked.to_str().unwrap()]);
        assert!(remove_worktree(&repo.0, &linked).is_err());
        repo.git(&["worktree", "unlock", linked.to_str().unwrap()]);
        fs::write(linked.join("file"), "dirty").unwrap();
        assert!(remove_worktree(&repo.0, &linked).is_err());
        repo.git(&["-C", linked.to_str().unwrap(), "restore", "file"]);
        fs::write(repo.0.join(".git/info/exclude"), "ignored\nlinked/\n").unwrap();
        fs::write(linked.join("ignored"), "valuable bytes").unwrap();
        assert!(remove_worktree(&repo.0, &linked).is_err());
        assert_eq!(
            fs::read_to_string(linked.join("ignored")).unwrap(),
            "valuable bytes"
        );
        fs::remove_file(linked.join("ignored")).unwrap();
        assert_eq!(remove_worktree(&repo.0, &linked).unwrap().len(), 1);
        assert!(!linked.exists());
        assert!(crate::branches::load(&repo.0)
            .unwrap()
            .refs
            .iter()
            .any(|r| r.name == "feature"));
    }

    #[test]
    fn prune_previews_metadata_and_preserves_locked_records() {
        let repo = TempRepo::new();
        repo.write("file", "fixture");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "fixture"]);
        let linked = repo.0.join("gone");
        add_worktree(&repo.0, &linked, "gone", true, &AtomicBool::new(false)).unwrap();
        repo.git(&["worktree", "lock", linked.to_str().unwrap()]);
        fs::remove_dir_all(&linked).unwrap();
        assert!(prune_preview(&repo.0).unwrap().is_empty());
        assert_eq!(prune(&repo.0).unwrap().len(), 2);
        repo.git(&["worktree", "unlock", linked.to_str().unwrap()]);
        fs::File::options()
            .write(true)
            .open(repo.0.join(".git/worktrees/gone/gitdir"))
            .unwrap()
            .set_modified(std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1000))
            .unwrap();
        assert!(!prune_preview(&repo.0).unwrap().is_empty());
        assert_eq!(worktrees(&repo.0).unwrap().len(), 2);
        assert_eq!(prune(&repo.0).unwrap().len(), 1);
        assert!(repo.0.join("file").exists());
    }

    #[test]
    fn settings_are_local_and_config_path_resolves_common_git_directory() {
        let repo = TempRepo::new();
        let values = Settings {
            name: "Name é".into(),
            email: "person@example.invalid".into(),
        };
        assert_eq!(save_settings(&repo.0, &values).unwrap(), values);
        assert_eq!(settings(&repo.0).unwrap(), values);
        assert!(save_settings(
            &repo.0,
            &Settings {
                name: "bad\nname".into(),
                email: String::new()
            }
        )
        .is_err());
        assert_eq!(
            save_settings(&repo.0, &Settings::default()).unwrap(),
            Settings::default()
        );
        repo.git(&["config", "--local", "user.name", ""]);
        save_settings(&repo.0, &Settings::default()).unwrap();
        assert!(!repo
            .git(&["config", "--local", "--name-only", "--list"])
            .lines()
            .any(|key| key == "user.name"));
        assert_eq!(
            config_path(&repo.0).unwrap().canonicalize().unwrap(),
            repo.0.join(".git/config").canonicalize().unwrap()
        );
        repo.write("file", "fixture");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "fixture"]);
        let linked = repo.0.join("linked");
        add_worktree(&repo.0, &linked, "linked", true, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            config_path(&linked).unwrap().canonicalize().unwrap(),
            repo.0.join(".git/config").canonicalize().unwrap()
        );
    }

    #[test]
    fn parses_worktree_paths_with_newlines_and_rejects_truncation() {
        let trees = parse_worktrees(b"worktree /root\0HEAD abc\0branch refs/heads/main\0\0worktree /with\nnewline\0HEAD def\0detached\0locked reason\0prunable missing\0\0").unwrap();
        assert_eq!(trees[1].path, "/with\nnewline");
        assert!(trees[1].locked && trees[1].prunable);
        assert!(parse_worktrees(b"worktree /truncated\0HEAD abc\0").is_err());
        assert!(parse_worktrees(b"HEAD abc\0\0").is_err());
        assert!(parse_worktrees(b"worktree /bad\xff\0\0").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_descendants_so_pipe_readers_cannot_hang() {
        let repo = TempRepo::new();
        let start = Instant::now();
        assert!(git::run_controlled(
            &repo.0,
            &["-c", "alias.wait=!sleep 30", "wait"],
            Duration::from_millis(100),
            None
        )
        .is_err());
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
