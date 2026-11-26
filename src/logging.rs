use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
};

const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024;
static LOGGER: Mutex<Option<Logger>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum Level {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}

impl Level {
    pub const ALL: [Self; 5] = [Self::Off, Self::Error, Self::Warn, Self::Info, Self::Debug];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Error => "Errors",
            Self::Warn => "Warnings and errors",
            Self::Info => "Information",
            Self::Debug => "Debug",
        }
    }

    fn records(self, event: Self) -> bool {
        event != Self::Off && event <= self
    }
}

struct Logger {
    level: Level,
    sender: mpsc::SyncSender<String>,
    worker: JoinHandle<()>,
    error: Arc<Mutex<Option<String>>>,
}

pub fn path() -> Option<PathBuf> {
    crate::persistence::data_dir().map(|dir| dir.join("sannygit.log"))
}

pub fn configure(level: Level) -> Result<(), String> {
    let mut logger = LOGGER.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(logger) = logger.as_mut() {
        logger.level = level;
    } else if level != Level::Off {
        let path = path().ok_or("Cannot determine the application log location.")?;
        *logger = Some(
            Logger::start(path, level)
                .map_err(|error| format!("Cannot start the application log writer: {error}"))?,
        );
    }
    Ok(())
}

pub fn record(level: Level, message: fmt::Arguments<'_>) {
    let logger = LOGGER.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(logger) = logger.as_ref() {
        logger.record(level, message);
    }
}

pub fn take_error() -> Option<String> {
    let logger = LOGGER.lock().unwrap_or_else(|error| error.into_inner());
    let logger = logger.as_ref()?;
    let mut error = logger
        .error
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    error.take()
}

pub fn shutdown() {
    let logger = LOGGER
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    if let Some(logger) = logger {
        logger.finish();
    }
}

impl Logger {
    fn start(path: PathBuf, level: Level) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<String>(1024);
        let error = Arc::new(Mutex::new(None));
        let worker_error = Arc::clone(&error);
        let worker = thread::Builder::new()
            .name("application-log".into())
            .spawn(move || {
                let mut failed = false;
                for line in receiver {
                    match append(&path, &line, MAX_FILE_SIZE) {
                        Ok(()) => failed = false,
                        Err(error) if !failed => {
                            let mut pending = worker_error
                                .lock()
                                .unwrap_or_else(|error| error.into_inner());
                            *pending = Some(format!("Cannot write the application log: {error}"));
                            failed = true;
                        }
                        Err(_) => {}
                    }
                }
            })?;
        Ok(Self {
            level,
            sender,
            worker,
            error,
        })
    }

    fn record(&self, level: Level, message: fmt::Arguments<'_>) {
        if self.level.records(level) {
            let tag = match level {
                Level::Off => return,
                Level::Error => "ERROR",
                Level::Warn => "WARN",
                Level::Info => "INFO",
                Level::Debug => "DEBUG",
            };
            let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
            // Logging must not block Git workers or the UI when the disk is busy.
            let _ = self
                .sender
                .try_send(format!("{timestamp} [{tag}] {message}\n"));
        }
    }

    fn finish(self) {
        drop(self.sender);
        let _ = self.worker.join();
    }
}

fn append(path: &Path, line: &str, max_size: u64) -> io::Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    // Separate windows can run in separate processes using the same log.
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("log.lock"))?;
    lock.lock()?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let size = file.metadata()?.len();
    if size > 0 && size.saturating_add(line.len() as u64) > max_size {
        drop(file);
        fs::rename(path, path.with_extension("log.1"))?;
        file = OpenOptions::new().create(true).append(true).open(path)?;
    }
    file.write_all(line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "sannygit-log-test-{}-{}",
                std::process::id(),
                Utc::now().timestamp_nanos_opt().unwrap()
            ))
            .join("sannygit.log")
    }

    #[test]
    fn levels_filter_records_and_shutdown_drains_the_queue() {
        let path = log_path();
        let mut logger = Logger::start(path.clone(), Level::Warn).unwrap();
        logger.record(Level::Info, format_args!("filtered information"));
        logger.record(Level::Warn, format_args!("model list failed"));
        logger.record(Level::Error, format_args!("operation failed"));
        logger.level = Level::Off;
        logger.record(Level::Error, format_args!("filtered error"));
        logger.finish();

        let log = fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = log.lines().collect();
        assert_eq!(lines.len(), 2);
        let (timestamp, warning) = lines[0].split_once(' ').unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(timestamp).is_ok());
        assert_eq!(warning, "[WARN] model list failed");
        assert!(lines[1].ends_with("[ERROR] operation failed"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn rotation_keeps_only_the_latest_backup() {
        let path = log_path();
        for line in ["first\n", "second\n", "third\n"] {
            append(&path, line, 8).unwrap();
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), "third\n");
        assert_eq!(
            fs::read_to_string(path.with_extension("log.1")).unwrap(),
            "second\n"
        );
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn failed_writes_are_reported_to_the_application() {
        let path = log_path();
        let directory = path.parent().unwrap();
        fs::write(directory, "blocks creation of the log directory").unwrap();
        let logger = Logger::start(path.clone(), Level::Error).unwrap();
        let pending = Arc::clone(&logger.error);
        logger.record(Level::Error, format_args!("First event"));
        logger.record(Level::Error, format_args!("Second event"));
        logger.finish();
        assert!(pending
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .starts_with("Cannot write the application log:"));
        fs::remove_file(directory).unwrap();
    }
}
