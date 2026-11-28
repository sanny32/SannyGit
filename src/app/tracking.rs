use crate::{
    logging::{self, Level},
    tracking::{self, Status},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::AtomicBool,
        mpsc::{self, Receiver},
        Arc,
    },
    time::{Duration, Instant},
};

type Update = (Result<Status, String>, Option<String>, bool);
pub struct Monitor {
    pub status: Option<Status>,
    pub error: Option<String>,
    pub auto_fetch: bool,
    pub checked: bool,
    pub checking: bool,
    pub refs_changed: bool,
    root: Option<PathBuf>,
    receiver: Option<Receiver<Update>>,
    cancel: Option<Arc<AtomicBool>>,
    next_read: Instant,
    next_fetch: Instant,
    fast_pending: bool,
    fast_worker: bool,
}
impl Default for Monitor {
    fn default() -> Self {
        Self {
            status: None,
            error: None,
            auto_fetch: true,
            checked: false,
            checking: false,
            refs_changed: false,
            root: None,
            receiver: None,
            cancel: None,
            next_read: Instant::now(),
            next_fetch: Instant::now(),
            fast_pending: true,
            fast_worker: false,
        }
    }
}
impl Monitor {
    pub fn cancel(&mut self) {
        if self.checking {
            self.next_fetch = Instant::now();
            self.next_read = Instant::now();
        }
        if let Some(cancel) = self.cancel.take() {
            crate::git::signal_cancel(&cancel);
        }
        if self.fast_worker {
            self.fast_pending = true;
        }
        self.receiver = None;
        self.fast_worker = false;
        self.checking = false;
    }
    pub fn invalidate(&mut self) {
        self.cancel();
        self.status = None;
        self.next_read = Instant::now();
        self.fast_pending = true;
    }
    /// Refresh the current branch without hiding its last known badge.
    pub fn refresh(&mut self) {
        self.cancel();
        self.next_read = Instant::now();
        self.fast_pending = true;
    }
    pub fn branch_changed(&mut self) {
        self.invalidate();
        self.checked = false;
        self.error = None;
        self.next_fetch = Instant::now();
    }
    pub fn set_auto_fetch(&mut self, enabled: bool) {
        self.cancel();
        self.auto_fetch = enabled;
        self.next_fetch = Instant::now();
        self.next_read = Instant::now();
    }
    pub fn badge(&self) -> String {
        let Some(status) = &self.status else {
            return String::new();
        };
        let mut badge = status.badge();
        if status.upstream.is_some() && (!self.checked || self.error.is_some()) {
            if badge == "✓" {
                badge = "?".into();
            } else if !badge.contains('?') {
                badge.push_str(" ?");
            }
        }
        badge
    }
    pub fn description(&self) -> String {
        let mut text = self
            .status
            .as_ref()
            .map(Status::description)
            .unwrap_or_else(|| "Checking upstream status…".into());
        if self.error.is_some() {
            text.push_str("\nCounts use the last fetched refs; retry with Fetch.");
        } else if !self.checked {
            text.push_str("\nRemote not checked yet. Counts use cached refs; use Fetch or enable automatic fetch.");
        }
        if self.checking {
            text.push_str("\nChecking remote…");
        }
        text
    }
    pub fn update(
        &mut self,
        root: Option<PathBuf>,
        allow_start: bool,
        allow_fast: bool,
        git_executable: &str,
        context: &eframe::egui::Context,
    ) {
        if self.root != root {
            self.invalidate();
            self.root = root;
            self.error = None;
            self.checked = false;
            self.refs_changed = false;
            self.next_fetch = Instant::now();
        }
        if let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok((result, error, fetched)) => {
                    let fast = self.fast_worker;
                    self.receiver = None;
                    self.cancel = None;
                    self.checking = false;
                    self.fast_worker = false;
                    if fast {
                        // Fill in file counts and optionally fetch after publishing
                        // the inexpensive local counts.
                        self.next_read = Instant::now();
                    }
                    if error.is_some() {
                        logging::record(Level::Warn, format_args!("Automatic fetch failed"));
                        self.error = error;
                    }
                    if fetched {
                        self.refs_changed = true;
                        self.error = None;
                        self.checked = true;
                    }
                    match result {
                        Ok(status) => {
                            if !fetched
                                && self.status.as_ref().is_some_and(|old| {
                                    old.branch != status.branch || old.upstream != status.upstream
                                })
                            {
                                self.checked = false;
                                self.error = None;
                                self.next_fetch = Instant::now();
                                self.next_read = Instant::now();
                            }
                            if status.upstream.is_none() {
                                self.error = None;
                            }
                            self.status = Some(status);
                        }
                        Err(error) => {
                            self.error = Some(error);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.cancel = None;
                    self.checking = false;
                    self.fast_worker = false;
                    self.error = Some("Remote status worker stopped".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.receiver.is_some() || Instant::now() < self.next_read {
            return;
        }
        let fast = self.fast_pending;
        if if fast { !allow_fast } else { !allow_start } {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let fetch = !fast && self.auto_fetch && Instant::now() >= self.next_fetch;
        self.next_read = Instant::now() + Duration::from_secs(5);
        if fetch {
            self.next_fetch = Instant::now() + Duration::from_secs(60);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (sender, receiver) = mpsc::channel();
        let context = context.clone();
        let git_context = crate::git::GitContext::new(git_executable.to_owned());
        match std::thread::Builder::new()
            .name("sannygit-upstream".into())
            .spawn(move || {
                let git = &git_context;
                let mut error = None;
                let mut fetched = false;
                let result = if fast {
                    tracking::read_counts(git, &root)
                } else {
                    tracking::read(git, &root).and_then(|status| {
                        if fetch {
                            match tracking::fetch_upstream(git, &root, &status, &flag) {
                                Ok(checked) => {
                                    fetched = checked || status.remote.as_deref() == Some(".")
                                }
                                Err(e) => error = Some(e),
                            }
                        }
                        if fetch {
                            tracking::read(git, &root)
                        } else {
                            Ok(status)
                        }
                    })
                };
                let _ = sender.send((result, error, fetched));
                context.request_repaint();
            }) {
            Ok(_) => {
                self.receiver = Some(receiver);
                self.cancel = Some(cancel);
                self.checking = fetch;
                self.fast_pending = false;
                self.fast_worker = fast;
            }
            Err(error) => self.error = Some(format!("Cannot check upstream: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{GitCommandCount, TempRepo, SYSTEM_GIT};
    fn status() -> Status {
        Status {
            branch: "main".into(),
            upstream: Some("origin/main".into()),
            ahead: Some(0),
            behind: Some(0),
            ..Default::default()
        }
    }
    fn deliver(monitor: &mut Monitor, update: Update) {
        let (sender, receiver) = mpsc::channel();
        sender.send(update).unwrap();
        monitor.receiver = Some(receiver);
        monitor.update(
            monitor.root.clone(),
            false,
            false,
            "",
            &eframe::egui::Context::default(),
        );
    }
    #[test]
    fn failed_remote_check_stays_unknown_until_a_successful_fetch() {
        let mut monitor = Monitor {
            checked: true,
            status: Some(status()),
            ..Default::default()
        };
        deliver(
            &mut monitor,
            (Ok(status()), Some("Network unavailable".into()), false),
        );
        assert_eq!(monitor.badge(), "?");
        deliver(&mut monitor, (Ok(status()), None, false));
        assert_eq!(monitor.badge(), "?");
        assert!(monitor.error.is_some());
        deliver(&mut monitor, (Ok(status()), None, true));
        assert_eq!(monitor.badge(), "✓");
        assert!(monitor.error.is_none());
        monitor.branch_changed();
        assert!(monitor.status.is_none());
        assert!(!monitor.checked);
    }
    #[test]
    fn cancellation_discards_late_results_and_root_switch_clears_status() {
        let mut monitor = Monitor {
            checked: true,
            status: Some(status()),
            ..Default::default()
        };
        let (sender, receiver) = mpsc::channel();
        monitor.receiver = Some(receiver);
        monitor.checking = true;
        monitor.next_fetch = Instant::now() + Duration::from_secs(60);
        monitor.cancel();
        assert!(
            monitor.next_fetch <= Instant::now(),
            "Interrupted first fetch must retry without a one-minute delay"
        );
        assert!(sender.send((Ok(status()), None, true)).is_err());
        monitor.update(
            Some(PathBuf::from("/other-repository")),
            false,
            false,
            "",
            &eframe::egui::Context::default(),
        );
        assert!(monitor.status.is_none());
        assert!(!monitor.checked);
        monitor.set_auto_fetch(false);
        assert!(!monitor.auto_fetch);
    }

    #[test]
    fn refreshing_discards_an_outdated_fast_read_without_clearing_the_badge() {
        let (sender, receiver) = mpsc::channel();
        let mut monitor = Monitor {
            checked: true,
            status: Some(status()),
            receiver: Some(receiver),
            fast_worker: true,
            ..Default::default()
        };
        monitor.refresh();
        assert!(sender.send((Ok(status()), None, false)).is_err());
        assert!(monitor.fast_pending);
        assert_eq!(monitor.badge(), "✓");
    }

    #[test]
    fn failed_local_read_retains_previous_counts() {
        let mut monitor = Monitor {
            checked: true,
            status: Some(Status {
                ahead: Some(2),
                ..status()
            }),
            fast_worker: true,
            ..Default::default()
        };
        deliver(&mut monitor, (Err("read failed".into()), None, false));
        assert_eq!(
            monitor.status.as_ref().and_then(|status| status.ahead),
            Some(2)
        );
        assert_eq!(monitor.badge(), "↑2 ↓0 ?");
    }

    #[test]
    fn local_counts_refresh_before_fetch_after_commit_and_push() {
        let repo = TempRepo::new();
        repo.git(&["commit", "--allow-empty", "-m", "base"]);
        let origin = repo.0.join("origin.git");
        repo.git(&["init", "--bare", origin.to_str().unwrap()]);
        repo.git(&["remote", "add", "origin", origin.to_str().unwrap()]);
        repo.git(&["push", "--set-upstream", "origin", "main"]);
        let original = tracking::read(&SYSTEM_GIT, &repo.0).unwrap();
        let mut monitor = Monitor {
            root: Some(repo.0.clone()),
            status: Some(original),
            checked: true,
            next_read: Instant::now() + Duration::from_secs(5),
            next_fetch: Instant::now(),
            fast_pending: false,
            ..Default::default()
        };
        let ctx = eframe::egui::Context::default();
        let counter = GitCommandCount::new(&repo.0);
        let refresh_counts = |monitor: &mut Monitor, expected: (u64, u64)| {
            let before = counter.get();
            let badge = monitor.badge();
            monitor.refresh();
            assert_eq!(
                monitor.badge(),
                badge,
                "refresh must retain the previous badge"
            );
            // History and ref loading may still be active; only the fast read is allowed.
            monitor.update(Some(repo.0.clone()), false, true, "", &ctx);
            assert!(monitor.fast_worker && !monitor.checking);
            let deadline = Instant::now() + Duration::from_secs(10);
            while monitor.receiver.is_some() && Instant::now() < deadline {
                monitor.update(Some(repo.0.clone()), false, false, "", &ctx);
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(monitor.receiver.is_none(), "local counts did not finish");
            let current = monitor.status.as_ref().unwrap();
            assert_eq!(
                (current.ahead, current.behind),
                (Some(expected.0), Some(expected.1))
            );
            assert_eq!(
                counter.get() - before,
                2,
                "only ref lookup and rev-list should run"
            );
            assert!(
                monitor.next_read <= Instant::now(),
                "full refresh should remain scheduled"
            );
            assert!(
                monitor.next_fetch <= Instant::now(),
                "fast read must not delay auto-fetch"
            );
        };
        repo.git(&["commit", "--allow-empty", "-m", "local"]);
        refresh_counts(&mut monitor, (1, 0));
        assert_eq!(monitor.badge(), "↑1 ↓0");
        repo.git(&["push", "origin", "main"]);
        refresh_counts(&mut monitor, (0, 0));
        assert_eq!(monitor.badge(), "✓");
    }
}
