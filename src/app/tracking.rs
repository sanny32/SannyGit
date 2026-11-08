use crate::tracking::{self, Status};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
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
            cancel.store(true, Ordering::Relaxed);
        }
        self.receiver = None;
        self.checking = false;
    }
    pub fn invalidate(&mut self) {
        self.cancel();
        self.status = None;
        self.next_read = Instant::now();
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
        if let Some(error) = &self.error {
            text.push_str(&format!(
                "\nRemote check failed: {error}\nCounts use the last fetched refs."
            ));
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
                    self.receiver = None;
                    self.cancel = None;
                    self.checking = false;
                    if error.is_some() {
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
                            self.status = None;
                            self.error = Some(error);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.cancel = None;
                    self.checking = false;
                    self.error = Some("Remote status worker stopped".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !allow_start || self.receiver.is_some() || Instant::now() < self.next_read {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let fetch = self.auto_fetch && Instant::now() >= self.next_fetch;
        self.next_read = Instant::now() + Duration::from_secs(5);
        if fetch {
            self.next_fetch = Instant::now() + Duration::from_secs(60);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (sender, receiver) = mpsc::channel();
        let context = context.clone();
        match std::thread::Builder::new()
            .name("sannygit-upstream".into())
            .spawn(move || {
                let mut error = None;
                let mut fetched = false;
                let result = tracking::read(&root).and_then(|status| {
                    if fetch {
                        match tracking::fetch_upstream(&root, &status, &flag) {
                            Ok(checked) => {
                                fetched = checked || status.remote.as_deref() == Some(".")
                            }
                            Err(e) => error = Some(e),
                        }
                    }
                    if fetch {
                        tracking::read(&root)
                    } else {
                        Ok(status)
                    }
                });
                let _ = sender.send((result, error, fetched));
                context.request_repaint();
            }) {
            Ok(_) => {
                self.receiver = Some(receiver);
                self.cancel = Some(cancel);
                self.checking = fetch;
            }
            Err(error) => self.error = Some(format!("Cannot check upstream: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            &eframe::egui::Context::default(),
        );
        assert!(monitor.status.is_none());
        assert!(!monitor.checked);
        monitor.set_auto_fetch(false);
        assert!(!monitor.auto_fetch);
    }
}
