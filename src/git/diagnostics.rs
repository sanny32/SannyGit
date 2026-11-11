//! Bounded display diagnostics. Raw command arguments, stdin and failed stdout
//! never enter an error; hook-capable operations also withhold their stderr.
use std::fmt;

const DISPLAY_LIMIT: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FailureKind {
    Start,
    Io,
    Worker,
    Exit,
    Cancelled,
    TimedOut,
    OutputLimit,
}

#[derive(Debug)]
pub(crate) struct CommandError {
    pub kind: FailureKind,
    pub exit_code: Option<i32>,
    operation: &'static str,
    message: String,
    diagnostic: String,
}

impl CommandError {
    pub fn with_exit_code(mut self, code: Option<i32>) -> Self {
        self.exit_code = code;
        self
    }

    pub fn new(operation: &'static str, kind: FailureKind, message: impl AsRef<str>) -> Self {
        Self {
            kind,
            exit_code: None,
            operation,
            message: redact(message.as_ref()),
            diagnostic: String::new(),
        }
    }

    pub fn exited(operation: &'static str, code: Option<i32>, stderr: &[u8]) -> Self {
        let diagnostic = if stderr.is_empty() {
            String::new()
        } else if stderr.len() > super::OUTPUT_LIMIT {
            "[Git diagnostic hidden: output exceeds the capture limit]".into()
        } else if !matches!(
            operation,
            "cat-file"
                | "check-ref-format"
                | "config"
                | "diff"
                | "diff-tree"
                | "for-each-ref"
                | "log"
                | "ls-files"
                | "rev-list"
                | "rev-parse"
                | "show"
                | "status"
                | "symbolic-ref"
        ) {
            "[Operation output hidden: it may contain hook output or credentials]".into()
        } else {
            match std::str::from_utf8(stderr) {
                Ok(text) => redact(text),
                Err(_) => "[Non-UTF-8 Git diagnostic hidden]".into(),
            }
        };
        Self {
            kind: FailureKind::Exit,
            exit_code: code,
            operation,
            message: super::command_error(operation),
            diagnostic,
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if self.kind == FailureKind::Start
            && !self.message.contains("Preferences > Git > Executable")
        {
            write!(
                f,
                "\nCannot start Git. Check Preferences > Git > Executable."
            )?;
        }
        if let Some(code) = self.exit_code {
            write!(f, "\nGit {} exit code: {code}.", self.operation)?;
        } else if self.kind == FailureKind::Exit {
            write!(
                f,
                "\nGit {} terminated without an exit code.",
                self.operation
            )?;
        }
        if !self.diagnostic.is_empty() {
            write!(f, "\n\nGit diagnostic:\n{}", self.diagnostic)?;
        }
        Ok(())
    }
}

impl std::error::Error for CommandError {}

/// Only known subcommands may be included in an error, never arbitrary argv.
pub(super) fn operation(mut args: &[&str]) -> &'static str {
    while args.first() == Some(&"-c") && args.len() >= 2 {
        args = &args[2..];
    }
    match args.first().copied() {
        Some("add") => "add",
        Some("branch") => "branch",
        Some("cat-file") => "cat-file",
        Some("check-ref-format") => "check-ref-format",
        Some("checkout") => "checkout",
        Some("clone") => "clone",
        Some("commit") => "commit",
        Some("config") => "config",
        Some("diff") => "diff",
        Some("diff-tree") => "diff-tree",
        Some("fetch") => "fetch",
        Some("for-each-ref") => "for-each-ref",
        Some("init") => "init",
        Some("log") => "log",
        Some("ls-files") => "ls-files",
        Some("ls-remote") => "ls-remote",
        Some("push") => "push",
        Some("restore") => "restore",
        Some("rev-list") => "rev-list",
        Some("rev-parse") => "rev-parse",
        Some("show") => "show",
        Some("stash") => "stash",
        Some("status") => "status",
        Some("switch") => "switch",
        Some("symbolic-ref") => "symbolic-ref",
        Some("tag") => "tag",
        Some("update-index") => "update-index",
        Some("update-ref") => "update-ref",
        Some("worktree") => "worktree",
        _ => "command",
    }
}

fn redact(text: &str) -> String {
    // Inspect the complete captured message before truncation. Suppress the
    // whole diagnostic on credential markers, including wrapped/PEM values.
    let compact: String = text
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if [
        "password",
        "passwd",
        "token",
        "secret",
        "authorization",
        "credential",
        "apikey",
        "accesskey",
        "privatekey",
        "bearer",
        "beginrsaprivate",
        "beginopenssh",
        "beginecprivate",
        "sshrsa",
        "sshed25519",
        "ghp",
        "gho",
        "ghu",
        "ghs",
        "githubpat",
        "glpat",
        "akia",
        "eyj",
    ]
    .iter()
    .any(|marker| compact.contains(marker))
        || text.contains("sk-")
    {
        return "[Diagnostic hidden: potentially sensitive data]".into();
    }
    let text = text.replace("\r\n", "\n");
    if text.chars().any(|c| {
        c.is_control() && !matches!(c, '\n' | '\t')
            || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    }) {
        return "[Diagnostic hidden: unsupported control characters]".into();
    }
    let mut display = String::new();
    for line in text.lines() {
        if !display.is_empty() {
            display.push('\n');
        }
        for word in line.split_whitespace() {
            if !display.is_empty() && !display.ends_with('\n') {
                display.push(' ');
            }
            if word.contains("://") || word.contains('@') && word.contains(':') {
                // Hide the complete address, including malformed URLs and
                // credentials encoded in paths, query strings or fragments.
                display.push_str("[URL hidden]");
            } else {
                display.push_str(word);
            }
        }
    }
    if display.len() > DISPLAY_LIMIT {
        let mut end = DISPLAY_LIMIT;
        while !display.is_char_boundary(end) {
            end -= 1;
        }
        display.truncate(end);
        display.push_str("\n[Diagnostic truncated]");
    }
    display
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_diagnostics_hide_complete_addresses_even_when_malformed_or_encoded() {
        for url in [
            "https://alice:hunter2@example.invalid/repo?auth=abc123#fragment",
            "ssh://alice:hunter2@example.invalid/repo",
            "https://example.invalid/abc123/repo",
            "https://alice%3Ahunter2@example.invalid/%61%62%63",
            "https://[malformed/alice:hunter2",
            "alice:hunter2@example.invalid:repo",
        ] {
            let error = CommandError::exited(
                "status",
                Some(128),
                format!("fatal: cannot access '{url}': connection refused").as_bytes(),
            );
            let shown = error.to_string();
            assert!(shown.contains("connection refused"));
            assert!(shown.contains("exit code: 128"));
            for hidden in [
                "alice",
                "hunter2",
                "abc123",
                "fragment",
                "%61",
                "example.invalid",
            ] {
                assert!(!shown.contains(hidden));
                assert!(!format!("{error:?}").contains(hidden));
            }
        }
    }

    #[test]
    fn credential_markers_hide_the_entire_diagnostic_before_truncation() {
        for text in [
            "Authorization: Basic YWxhZGRpbjpvcGVuIHNlc2FtZQ==",
            "access_token=unlabelled-value",
            "API-Key: unlabelled-value",
            "client-secret: unlabelled-value",
            "pass\nword=unlabelled-value",
            "-----BEGIN OPENSSH PRIVATE KEY-----\nunlabelled-value\n-----END OPENSSH PRIVATE KEY-----",
            "ghp_unlabelled-value", "glpat-unlabelled-value", "sk-unlabelled-value",
            "eyJhbGciOiJIUzI1NiJ9.unlabelled-value.signature",
        ] {
            let raw = format!("{}\n{text}", "ordinary text ".repeat(DISPLAY_LIMIT));
            let error = CommandError::exited("config", Some(1), raw.as_bytes());
            let shown = error.to_string();
            assert!(shown.contains("potentially sensitive data"));
            assert!(!shown.contains("unlabelled-value"));
            assert!(!shown.contains("YWxh"));
            assert!(!shown.contains("ordinary text"));
            assert!(!format!("{error:?}").contains("unlabelled-value"));
        }
    }

    #[test]
    fn hook_network_and_unknown_operations_never_display_unlabelled_output() {
        for operation in [
            "commit",
            "stash",
            "switch",
            "checkout",
            "branch",
            "tag",
            "restore",
            "update-ref",
            "worktree",
            "fetch",
            "push",
            "clone",
            "ls-remote",
            "command",
        ] {
            let error = CommandError::exited(operation, Some(1), b"unlabelled-private-content");
            assert!(error.to_string().contains("Operation output hidden"));
            assert!(!format!("{error:?}").contains("unlabelled-private-content"));
        }
        assert_eq!(
            operation(&[
                "-c",
                "http.extraHeader=Authorization: Bearer hidden",
                "push"
            ]),
            "push"
        );
        assert_eq!(
            operation(&["https://alice:password@example.invalid"]),
            "command"
        );
        assert_eq!(operation(&["-c", "status"]), "command");
    }

    #[test]
    fn localized_diagnostics_are_bounded_and_control_or_binary_output_is_hidden() {
        let message = "fatal: dépôt introuvable\r\n";
        let error = CommandError::exited("status", Some(128), message.as_bytes());
        assert!(error.to_string().contains("fatal: dépôt introuvable"));
        assert_eq!(error.kind, FailureKind::Exit);
        assert_eq!(error.exit_code, Some(128));
        let long = CommandError::exited("status", None, "é".repeat(DISPLAY_LIMIT).as_bytes());
        assert!(long.diagnostic.len() < DISPLAY_LIMIT + 100);
        assert!(long.to_string().contains("without an exit code"));
        assert!(long.diagnostic.ends_with("[Diagnostic truncated]"));
        for bytes in [
            b"\xffprivate".as_slice(),
            b"\x1b[31mprivate",
            b"private\0rest",
            "\u{202e}private".as_bytes(),
        ] {
            let error = CommandError::exited("status", Some(1), bytes);
            assert!(!error.to_string().contains("private"));
        }
        let huge = CommandError::exited(
            "status",
            Some(1),
            &vec![b'x'; super::super::OUTPUT_LIMIT + 1],
        );
        assert!(huge.diagnostic.contains("capture limit"));
    }
}
