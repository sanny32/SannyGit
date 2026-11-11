//! Ahead/behind status against the exact configured upstream.
use crate::git;
use std::{path::Path, sync::atomic::AtomicBool, time::Duration};

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
    pub outgoing_files: Option<usize>,
    pub incoming_files: Option<usize>,
    pub remote: Option<String>,
    pub remote_ref: Option<String>,
    pub local_ref: Option<String>,
}

impl Status {
    pub fn badge(&self) -> String {
        match (self.ahead, self.behind) {
            (Some(a), Some(b)) if a != 0 || b != 0 => format!("↑{a} ↓{b}"),
            (Some(_), Some(_)) => "✓".into(),
            _ if self.upstream.is_some() => "?".into(),
            _ => String::new(),
        }
    }
    pub fn description(&self) -> String {
        let Some(upstream) = &self.upstream else {
            return "No upstream branch. Select ‘Set as upstream’ when pushing to track a remote branch.".into();
        };
        match (self.ahead, self.behind) {
            (Some(a), Some(b)) => format!("Upstream: {upstream}\nPush: {a} commits, {} files\nPull: {b} commits, {} files", self.outgoing_files.map(|n| n.to_string()).unwrap_or_else(|| "unknown".into()), self.incoming_files.map(|n| n.to_string()).unwrap_or_else(|| "unknown".into())),
            _ => format!("Upstream: {upstream}\nUpstream is missing or unavailable; push/pull counts are unknown."),
        }
    }
}

fn counts(output: &str) -> Result<(u64, u64), String> {
    let mut parts = output.split_whitespace();
    let a = parts
        .next()
        .ok_or("Missing ahead count")?
        .parse()
        .map_err(|_| "Invalid ahead count")?;
    let b = parts
        .next()
        .ok_or("Missing behind count")?
        .parse()
        .map_err(|_| "Invalid behind count")?;
    if parts.next().is_some() {
        return Err("Invalid ahead/behind response".into());
    }
    Ok((a, b))
}

fn files(root: &Path, range: &str, count: u64) -> Option<usize> {
    if count == 0 {
        return Some(0);
    }
    let output = git::run(
        root,
        &["diff", "--no-ext-diff", "--name-only", "-z", range, "--"],
    )
    .ok()?;
    Some(output.split(|b| *b == 0).filter(|p| !p.is_empty()).count())
}

pub fn read(root: &Path) -> Result<Status, String> {
    let output = git::text(git::run(root, &["for-each-ref", "--format=%(HEAD)%00%(refname)%00%(upstream)%00%(upstream:short)%00%(upstream:remotename)%00%(upstream:remoteref)%00", "refs/heads/"])?)?;
    let mut status = Status::default();
    for line in output.lines() {
        let fields = line.split('\0').collect::<Vec<_>>();
        if fields.len() != 7 {
            return Err("Invalid upstream response".into());
        }
        if fields[0] != "*" {
            continue;
        }
        status.branch = fields[1]
            .strip_prefix("refs/heads/")
            .ok_or("Invalid local ref")?
            .into();
        if fields[2].is_empty() {
            return Ok(status);
        }
        status.local_ref = Some(fields[2].into());
        status.upstream = Some(fields[3].into());
        status.remote = (!fields[4].is_empty()).then(|| fields[4].into());
        status.remote_ref = (!fields[5].is_empty()).then(|| fields[5].into());
        let range = format!("HEAD...{}", fields[2]);
        let Ok(output) = git::run(root, &["rev-list", "--left-right", "--count", &range, "--"])
        else {
            return Ok(status);
        };
        let (ahead, behind) = counts(&git::text(output)?)?;
        status.ahead = Some(ahead);
        status.behind = Some(behind);
        status.outgoing_files = files(root, &format!("{}...HEAD", fields[2]), ahead);
        status.incoming_files = files(root, &range, behind);
        return Ok(status);
    }
    Ok(status)
}

pub fn fetch_upstream(root: &Path, status: &Status, cancel: &AtomicBool) -> Result<bool, String> {
    let (Some(remote), Some(source), Some(destination)) =
        (&status.remote, &status.remote_ref, &status.local_ref)
    else {
        return Ok(false);
    };
    if remote == "." {
        return Ok(false);
    }
    if !source.starts_with("refs/heads/") || !destination.starts_with("refs/remotes/") {
        return Err("Automatic fetch requires a remote-tracking upstream ref.".into());
    }
    // Update only this remote-tracking ref, never local branches or FETCH_HEAD.
    git::run_controlled(
        root,
        &[
            "fetch",
            "--no-write-fetch-head",
            "--no-tags",
            "--no-recurse-submodules",
            "--refmap=",
            "--",
            remote,
            &format!("+{source}:{destination}"),
        ],
        Duration::from_secs(30),
        Some(cancel),
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempRepo;
    use std::{fs, path::PathBuf};

    fn command(path: &Path, args: &[&str]) -> Vec<u8> {
        git::run(path, args).unwrap()
    }
    fn identity(path: &Path) {
        command(path, &["config", "user.name", "Tracking Test"]);
        command(path, &["config", "user.email", "test@example.invalid"]);
    }
    fn commit(path: &Path, file: &str, content: &str) {
        fs::write(path.join(file), content).unwrap();
        command(path, &["add", file]);
        command(path, &["commit", "-m", content]);
    }
    fn fixture() -> (TempRepo, PathBuf, PathBuf) {
        let temp = TempRepo::new();
        let source = temp.0.join("source");
        let client = temp.0.join("client");
        let remote = temp.0.join("remote.git");
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
            &["clone", remote.to_str().unwrap(), source.to_str().unwrap()],
        );
        identity(&source);
        commit(&source, "file", "base\n");
        command(&source, &["push", "-u", "origin", "main"]);
        command(
            &temp.0,
            &["clone", remote.to_str().unwrap(), client.to_str().unwrap()],
        );
        identity(&client);
        (temp, source, client)
    }

    #[test]
    fn fetch_reveals_divergence_and_counts_files_without_changing_head_or_fetch_head() {
        let (_temp, source, client) = fixture();
        let initial = read(&client).unwrap();
        assert_eq!(initial.badge(), "✓");
        assert_eq!(initial.upstream.as_deref(), Some("origin/main"));
        commit(&client, "local.txt", "local\n");
        commit(&source, "remote.txt", "remote 1\n");
        commit(&source, "remote.txt", "remote 2\n");
        command(&source, &["push", "origin", "main"]);
        let cached = read(&client).unwrap();
        assert_eq!((cached.ahead, cached.behind), (Some(1), Some(0)));
        let head = command(&client, &["rev-parse", "HEAD"]);
        let index = fs::read(client.join(".git/index")).unwrap();
        fs::write(client.join(".git/FETCH_HEAD"), "keep existing fetch head\n").unwrap();
        fetch_upstream(&client, &cached, &AtomicBool::new(false)).unwrap();
        let status = read(&client).unwrap();
        assert_eq!((status.ahead, status.behind), (Some(1), Some(2)));
        assert_eq!(
            (status.outgoing_files, status.incoming_files),
            (Some(1), Some(1))
        );
        assert_eq!(status.badge(), "↑1 ↓2");
        assert_eq!(command(&client, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(client.join(".git/index")).unwrap(), index);
        assert!(!client.join("remote.txt").exists());
        assert_eq!(
            fs::read_to_string(client.join(".git/FETCH_HEAD")).unwrap(),
            "keep existing fetch head\n"
        );
        assert!(fetch_upstream(&client, &status, &AtomicBool::new(true)).is_err());
    }

    #[test]
    fn distinguishes_missing_upstream_no_upstream_detached_and_empty_repository() {
        let (temp, _source, client) = fixture();
        command(&client, &["update-ref", "-d", "refs/remotes/origin/main"]);
        let missing = read(&client).unwrap();
        assert!(missing.upstream.is_some());
        assert_eq!(missing.ahead, None);
        assert_eq!(missing.badge(), "?");
        command(&client, &["branch", "--unset-upstream"]);
        assert!(read(&client).unwrap().upstream.is_none());
        command(&client, &["switch", "--detach"]);
        assert!(read(&client).unwrap().branch.is_empty());
        assert!(read(&temp.0).unwrap().upstream.is_none());
        for invalid in ["", "1", "-1 2", "a 0", "1 2 3"] {
            assert!(counts(invalid).is_err());
        }
    }

    #[test]
    fn explicit_upstream_setup_enables_tracking_after_push() {
        let (_temp, source, _client) = fixture();
        command(&source, &["branch", "--unset-upstream"]);
        assert!(read(&source).unwrap().upstream.is_none());
        crate::remotes::set_upstream(&source, "main", "origin", "main").unwrap();
        assert_eq!(
            read(&source).unwrap().upstream.as_deref(),
            Some("origin/main")
        );
    }
}
