//! Local remote configuration and credential-free URL validation.
//! Raw legacy values stay private; only redacted summaries reach rendering code.
use crate::git;
use std::path::Path;

const REMOTE_LIMIT: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub fetch_display: String,
    pub push_display: String,
    pub edit_url: String,
    pub edit_push_url: String,
    pub can_edit: bool,
    pub can_remove: bool,
    pub note: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub entries: Vec<Remote>,
    local: Vec<(String, String)>,
    effective: Vec<(String, String)>,
}

#[derive(Clone)]
pub enum Change {
    Add {
        name: String,
        url: String,
        push_url: String,
    },
    Update {
        name: String,
        url: String,
        push_url: String,
    },
    Remove {
        name: String,
    },
}

pub fn name_hint(name: &str) -> Option<&'static str> {
    if name.is_empty()
        || name.len() > 256
        || name.starts_with('-')
        || name.starts_with('.')
        || name
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "~^:?*[\\".contains(c))
        || name.contains("..")
        || name.contains("@{")
        || name.ends_with('/')
    {
        Some("Use a short remote name without whitespace, options or revision expressions")
    } else {
        None
    }
}

pub fn url_hint(value: &str) -> Option<&'static str> {
    if value.is_empty()
        || value.len() > 4096
        || value != value.trim()
        || value.starts_with('-')
        || value.chars().any(char::is_control)
    {
        return Some("Enter a URL or path without leading/trailing whitespace, control characters or options");
    }
    if value.contains("://") {
        let Ok(url) = reqwest::Url::parse(value) else {
            return Some("Enter a valid Git URL");
        };
        if !matches!(url.scheme(), "https" | "http" | "ssh" | "git" | "file") {
            return Some("Use HTTP(S), SSH, Git, file URLs or local paths; transport helpers are not supported");
        }
        if url.password().is_some()
            || (!url.username().is_empty() && url.scheme() != "ssh")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Some("Do not embed credentials, query parameters or fragments; use Git credential helpers and SSH configuration");
        }
        if url.host_str().is_some_and(|host| host.starts_with('-')) {
            return Some("Use a valid host name");
        }
        if url.scheme() != "file" && url.host_str().is_none() {
            return Some("The URL needs a host");
        }
        if url.scheme() == "file" && url.host_str().is_some_and(|host| host != "localhost") {
            return Some("Use a local file URL or a native local path");
        }
        return None;
    }
    if value.contains("::") {
        return Some("Git transport helpers are not supported");
    }
    // A native Windows drive is a path, not SCP syntax.
    let drive = value.as_bytes().get(1) == Some(&b':')
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic);
    if !drive {
        if let Some((authority, path)) = value.split_once(':') {
            if !authority.contains(['/', '\\']) {
                let host = authority.rsplit('@').next().unwrap_or_default();
                if host.starts_with('-')
                    || host.chars().any(|c| ";&|`$<>!".contains(c))
                    || authority.is_empty()
                    || path.is_empty()
                    || path.contains('@')
                    || authority.chars().any(char::is_whitespace)
                    || authority.matches('@').count() > 1
                    || authority.starts_with('@')
                    || authority.ends_with('@')
                {
                    return Some("Enter a valid SCP-style SSH address without passwords");
                }
            }
        }
    }
    None
}

pub fn redact_url(value: &str) -> String {
    if value.chars().any(char::is_control) {
        return "[unsupported URL hidden]".into();
    }
    if value.contains("://") {
        let Ok(mut url) = reqwest::Url::parse(value) else {
            return "[unsupported URL hidden]".into();
        };
        if !matches!(url.scheme(), "https" | "http" | "ssh" | "git" | "file") {
            return "[unsupported URL hidden]".into();
        }
        let secret =
            url.password().is_some() || (!url.username().is_empty() && url.scheme() != "ssh");
        if secret {
            let _ = url.set_username("");
            let _ = url.set_password(None);
        }
        let hidden = secret || url.query().is_some() || url.fragment().is_some();
        url.set_query(None);
        url.set_fragment(None);
        return format!(
            "{url}{}",
            if hidden {
                " [credentials/parameters hidden]"
            } else {
                ""
            }
        );
    }
    if url_hint(value).is_some() {
        "[unsupported URL hidden]".into()
    } else {
        value.into()
    }
}

fn records(bytes: &[u8]) -> Result<Vec<(String, String)>, String> {
    if !bytes.is_empty() && !bytes.ends_with(&[0]) {
        return Err("Truncated Git configuration output".into());
    }
    let mut result = Vec::new();
    for record in bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let split = record
            .iter()
            .position(|byte| *byte == b'\n')
            .unwrap_or(record.len());
        let key = std::str::from_utf8(&record[..split])
            .map_err(|_| "Unsupported Git configuration key")?;
        if !(key.starts_with("remote.") || key.starts_with("url.")) {
            continue;
        }
        let value = if split < record.len() {
            &record[split + 1..]
        } else {
            b""
        };
        let value = std::str::from_utf8(value)
            .map_err(|_| "Non-UTF-8 remote configuration is not supported")?;
        result.push((key.into(), value.into()));
    }
    Ok(result)
}

fn remote_records(records: &[(String, String)], name: &str) -> Vec<(String, String)> {
    records
        .iter()
        .filter(|(key, _)| {
            key.strip_prefix("remote.")
                .and_then(|key| key.rsplit_once('.'))
                .is_some_and(|(remote, _)| remote == name)
        })
        .cloned()
        .collect()
}

fn values<'a>(records: &'a [(String, String)], key: &str) -> Vec<&'a str> {
    records
        .iter()
        .filter(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
        .collect()
}

pub fn load(root: &Path) -> Result<Snapshot, String> {
    let local = records(&git::run(
        root,
        &["config", "--local", "--no-includes", "--null", "--list"],
    )?)?;
    let effective = records(&git::run(root, &["config", "--null", "--list"])?)?;
    let mut names: Vec<_> = effective
        .iter()
        .filter_map(|(key, _)| {
            key.strip_prefix("remote.")?
                .rsplit_once('.')
                .map(|(name, _)| name.to_owned())
        })
        .collect();
    names.sort();
    names.dedup();
    if names.len() > REMOTE_LIMIT {
        return Err("Remote list exceeds the 100-entry limit".into());
    }
    let mut entries = Vec::new();
    for name in names {
        if name_hint(&name).is_some() {
            return Err(
                "Unsupported remote name; edit Git configuration outside this dialog.".into(),
            );
        }
        let owned = remote_records(&local, &name) == remote_records(&effective, &name);
        let fetch = values(&effective, &format!("remote.{name}.url"));
        let push = values(&effective, &format!("remote.{name}.pushurl"));
        let specs = values(&effective, &format!("remote.{name}.fetch"));
        let default_spec = format!("+refs/heads/*:refs/remotes/{name}/*");
        let safe_remove = specs.iter().all(|spec| *spec == default_spec);
        let can_edit = owned && fetch.len() == 1 && push.len() <= 1;
        let can_remove = owned && safe_remove;
        let note = if !owned {
            "Remote uses inherited, included or worktree-specific settings; edit its source file."
        } else if fetch.len() != 1 || push.len() > 1 {
            "Multiple or missing URLs: edit the Git configuration to simplify them before using this editor."
        } else if fetch.iter().chain(&push).any(|url| url_hint(url).is_some()) {
            "Existing URL has credentials or unsupported syntax; enter new credential-free URLs to replace it."
        } else if !safe_remove {
            "Custom fetch destinations: removal is disabled to protect other refs."
        } else {
            "Empty push URL uses the fetch URL. Removal also removes cached remote-tracking refs and local upstream settings."
        };
        entries.push(Remote {
            name: name.clone(),
            fetch_display: fetch
                .iter()
                .map(|url| redact_url(url))
                .collect::<Vec<_>>()
                .join("; "),
            push_display: if push.is_empty() {
                "Same as fetch".into()
            } else {
                push.iter()
                    .map(|url| redact_url(url))
                    .collect::<Vec<_>>()
                    .join("; ")
            },
            edit_url: fetch
                .first()
                .filter(|url| url_hint(url).is_none())
                .map(|url| (*url).to_owned())
                .unwrap_or_default(),
            edit_push_url: push
                .first()
                .filter(|url| url_hint(url).is_none())
                .map(|url| (*url).to_owned())
                .unwrap_or_default(),
            can_edit,
            can_remove,
            note: note.into(),
        });
    }
    Ok(Snapshot {
        entries,
        local,
        effective,
    })
}

pub fn change(root: &Path, expected: &Snapshot, change: &Change) -> Result<(), String> {
    if load(root)? != *expected {
        return Err(
            "Remote configuration changed after loading. Reload before editing or removing it."
                .into(),
        );
    }
    let (name, urls) = match change {
        Change::Add {
            name,
            url,
            push_url,
        }
        | Change::Update {
            name,
            url,
            push_url,
        } => (name, Some((url, push_url))),
        Change::Remove { name } => (name, None),
    };
    if let Some(reason) = name_hint(name) {
        return Err(reason.into());
    }
    git::run(
        root,
        &[
            "check-ref-format",
            &format!("refs/remotes/{name}/validation"),
        ],
    )?;
    if let Some((url, push)) = urls {
        if let Some(reason) =
            url_hint(url).or_else(|| (!push.is_empty()).then(|| url_hint(push)).flatten())
        {
            return Err(reason.into());
        }
    }
    let existing = expected.entries.iter().find(|entry| entry.name == *name);
    match change {
        Change::Add { url, push_url, .. } => {
            if existing.is_some() {
                return Err("A remote with this name already exists.".into());
            }
            git::run(root, &["remote", "add", "--", name, url])?;
            if !push_url.is_empty() {
                git::run(root, &["config", "--local", "--replace-all", &format!("remote.{name}.pushurl"), push_url])
                    .map_err(|_| "Remote added, but its push URL could not be set. Reload and inspect its configuration before retrying.".to_owned())?;
            }
        }
        Change::Update { url, push_url, .. } => {
            if !existing.is_some_and(|entry| entry.can_edit) {
                return Err(
                    "This remote cannot be edited safely here. Use Edit Git Config.".into(),
                );
            }
            git::run(root, &["remote", "set-url", "--", name, url])?;
            if !push_url.is_empty() {
                git::run(root, &["config", "--local", "--replace-all", &format!("remote.{name}.pushurl"), push_url])
                    .map_err(|_| "Fetch URL updated, but push URL update failed. Reload and inspect the remote before retrying.".to_owned())?;
            } else if !values(&expected.local, &format!("remote.{name}.pushurl")).is_empty() {
                git::run(root, &["config", "--local", "--unset-all", &format!("remote.{name}.pushurl")])
                    .map_err(|_| "Fetch URL updated, but the old push URL could not be removed. Reload before retrying.".to_owned())?;
            }
        }
        Change::Remove { .. } => {
            if !existing.is_some_and(|entry| entry.can_remove) {
                return Err("Remote removal is disabled for inherited settings or custom fetch destinations.".into());
            }
            git::run(root, &["remote", "remove", "--", name])?;
        }
    }
    Ok(())
}

/// Resolve one actual push endpoint; reject fan-out and unsafe/custom transports.
pub fn push_endpoint(root: &Path, name: &str) -> Result<(String, Snapshot), String> {
    let snapshot = load(root)?;
    if !snapshot.entries.iter().any(|entry| entry.name == name) {
        return Err("Choose a configured remote.".into());
    }
    if !values(&snapshot.effective, &format!("remote.{name}.vcs")).is_empty() {
        return Err("Custom remote helpers are not supported for tag pushes.".into());
    }
    let output = git::text(git::run(
        root,
        &["remote", "get-url", "--push", "--all", "--", name],
    )?)?;
    let urls: Vec<_> = output
        .strip_suffix('\n')
        .unwrap_or(&output)
        .split('\n')
        .collect();
    if urls.len() != 1 {
        return Err(
            "Tag push requires exactly one push URL; multiple destinations are not supported."
                .into(),
        );
    }
    let url = urls[0];
    if url_hint(url).is_some() {
        return Err("Push URL contains credentials or unsupported syntax. Use Git credential helpers and a supported URL before pushing tags.".into());
    }
    let drive = url.as_bytes().get(1) == Some(&b':')
        && url.as_bytes().first().is_some_and(u8::is_ascii_alphabetic);
    let scp = !drive
        && url
            .split_once(':')
            .is_some_and(|(authority, _)| !authority.contains(['/', '\\']));
    let endpoint = if url.contains("://") || scp {
        url.to_owned()
    } else {
        let path = root.join(url);
        reqwest::Url::from_file_path(path)
            .map_err(|_| "Cannot convert the local push path to a file URL")?
            .into()
    };
    Ok((endpoint, snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::TempRepo;

    #[test]
    fn url_validation_and_redaction_never_return_embedded_credentials() {
        for value in [
            "https://example.test/team/repo.git",
            "ssh://git@example.test/team/repo.git",
            "git@example.test:team/repo.git",
            "/with spaces/repo.git",
            "../remote.git",
            "file:///tmp/remote.git",
        ] {
            assert!(url_hint(value).is_none(), "{value}");
        }
        for value in [
            "https://token@example.test/repo",
            "https://user:password@example.test/repo?key=secret#fragment",
            "ssh://git:password@example.test/repo",
            "ext::run-command",
            "--all",
            " user@host:repo",
            "ssh://git@-host/repo",
            "git@-host:repo",
        ] {
            assert!(url_hint(value).is_some(), "{value}");
        }
        for value in [
            "https://token@example.test/repo",
            "https://user:password@example.test/repo?key=secret#fragment",
            "ssh://git:password@example.test/repo",
            "ext::password",
            "invalid://password@host",
            "user:password@host:repo",
        ] {
            let display = redact_url(value);
            for secret in ["password", "token", "secret", "fragment"] {
                assert!(!display.contains(secret), "Redaction failed");
            }
        }
        assert!(records(b"remote.origin.url\ntruncated").is_err());
        assert!(records(b"remote.origin.url\n\xff\0").is_err());
    }

    #[test]
    fn add_update_and_remove_are_local_preserve_other_remotes_refs_and_dirty_files() {
        let repo = TempRepo::new();
        repo.write("file", "base\n");
        repo.git(&["add", "file"]);
        repo.git(&["commit", "-m", "Base"]);
        change(
            &repo.0,
            &load(&repo.0).unwrap(),
            &Change::Add {
                name: "origin".into(),
                url: "../remote.git".into(),
                push_url: "ssh://git@example.test/push.git".into(),
            },
        )
        .unwrap();
        repo.git(&["remote", "add", "keep", "https://example.test/keep.git"]);
        repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        repo.git(&["update-ref", "refs/remotes/keep/main", "HEAD"]);
        repo.git(&["branch", "local"]);
        repo.git(&["config", "branch.local.remote", "origin"]);
        repo.git(&["config", "branch.local.merge", "refs/heads/main"]);
        repo.write("file", "staged\n");
        repo.git(&["add", "file"]);
        repo.write("file", "unstaged\n");
        let index = repo.git(&["ls-files", "--stage"]);
        let head = repo.git(&["rev-parse", "HEAD"]);
        let snapshot = load(&repo.0).unwrap();
        change(
            &repo.0,
            &snapshot,
            &Change::Update {
                name: "origin".into(),
                url: "https://example.test/new.git".into(),
                push_url: String::new(),
            },
        )
        .unwrap();
        let updated = load(&repo.0).unwrap();
        let origin = updated
            .entries
            .iter()
            .find(|entry| entry.name == "origin")
            .unwrap();
        assert_eq!(origin.fetch_display, "https://example.test/new.git");
        assert_eq!(origin.push_display, "Same as fetch");
        assert!(change(
            &repo.0,
            &snapshot,
            &Change::Remove {
                name: "origin".into()
            }
        )
        .unwrap_err()
        .contains("changed"));
        change(
            &repo.0,
            &updated,
            &Change::Remove {
                name: "origin".into(),
            },
        )
        .unwrap();
        assert_eq!(load(&repo.0).unwrap().entries.len(), 1);
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(repo.git(&["ls-files", "--stage"]), index);
        assert_eq!(std::fs::read(repo.0.join("file")).unwrap(), b"unstaged\n");
        assert!(repo.git(&["branch", "--list", "local"]).contains("local"));
        assert!(repo
            .git(&[
                "for-each-ref",
                "--format=%(refname)",
                "refs/remotes/origin/"
            ])
            .is_empty());
        assert!(repo
            .git(&["for-each-ref", "--format=%(refname)", "refs/remotes/keep/"])
            .contains("keep/main"));
    }

    #[test]
    fn legacy_secrets_are_hidden_and_complex_or_inherited_remotes_are_not_overwritten() {
        let repo = TempRepo::new();
        repo.git(&[
            "remote",
            "add",
            "origin",
            "https://token@example.test/repo?key=secret",
        ]);
        let snapshot = load(&repo.0).unwrap();
        let entry = &snapshot.entries[0];
        assert!(!entry.fetch_display.contains("token") && !entry.fetch_display.contains("secret"));
        assert!(entry.edit_url.is_empty());
        let before = repo.git(&["config", "--local", "--get", "remote.origin.url"]);
        assert!(change(
            &repo.0,
            &snapshot,
            &Change::Update {
                name: "origin".into(),
                url: "https://password@example.test/repo".into(),
                push_url: String::new()
            }
        )
        .is_err());
        assert_eq!(
            repo.git(&["config", "--local", "--get", "remote.origin.url"]),
            before
        );
        repo.git(&[
            "config",
            "--add",
            "remote.origin.url",
            "https://example.test/second",
        ]);
        assert!(!load(&repo.0).unwrap().entries[0].can_edit);
        repo.git(&["config", "remote.origin.fetch", "+refs/*:refs/*"]);
        let snapshot = load(&repo.0).unwrap();
        assert!(!snapshot.entries[0].can_remove);
        assert!(change(
            &repo.0,
            &snapshot,
            &Change::Remove {
                name: "origin".into()
            }
        )
        .is_err());
        let included = repo.0.join("included-config");
        std::fs::write(&included, "[remote \"inherited\"]\nurl = https://example.test/inherited.git\nfetch = +refs/heads/*:refs/remotes/inherited/*\n").unwrap();
        repo.git(&["config", "include.path", included.to_str().unwrap()]);
        let snapshot = load(&repo.0).unwrap();
        let inherited = snapshot
            .entries
            .iter()
            .find(|entry| entry.name == "inherited")
            .unwrap();
        assert!(!inherited.can_edit && !inherited.can_remove);
        assert!(change(
            &repo.0,
            &snapshot,
            &Change::Remove {
                name: "inherited".into()
            }
        )
        .is_err());
        assert!(std::fs::read_to_string(included)
            .unwrap()
            .contains("inherited.git"));
    }

    #[test]
    fn rewritten_push_credentials_are_rejected_without_echoing_secrets() {
        let repo = TempRepo::new();
        repo.git(&["remote", "add", "origin", "safe:repo"]);
        repo.git(&[
            "config",
            "url.https://dummy-token@example.test/.insteadOf",
            "safe:",
        ]);
        let error = push_endpoint(&repo.0, "origin").err().unwrap();
        assert!(error.contains("credentials"));
        assert!(!error.contains("dummy-token"));
    }

    #[cfg(unix)]
    #[test]
    fn partial_url_updates_are_reported_and_are_not_rolled_back() {
        std::thread::spawn(|| {
            use std::os::unix::fs::PermissionsExt;
            let repo = TempRepo::new(); repo.git(&["remote", "add", "origin", "https://example.test/old.git"]);
            let snapshot = load(&repo.0).unwrap();
            let wrapper = repo.0.join(".git/failing-config-git");
            std::fs::write(&wrapper, "#!/bin/sh\nfor arg in \"$@\"; do\n  if [ \"$arg\" = remote.origin.pushurl ]; then exit 1; fi\ndone\nexec git \"$@\"\n").unwrap();
            std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
            git::set_executable(wrapper.to_string_lossy().into_owned());
            let error = change(&repo.0, &snapshot, &Change::Update { name: "origin".into(), url: "https://example.test/new.git".into(), push_url: "https://example.test/push.git".into() }).unwrap_err();
            assert!(error.contains("Fetch URL updated") && error.contains("push URL update failed"));
            assert_eq!(load(&repo.0).unwrap().entries[0].fetch_display, "https://example.test/new.git");
        }).join().unwrap();
    }
}
