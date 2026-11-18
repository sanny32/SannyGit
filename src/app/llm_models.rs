use crate::llm::Config;
use eframe::egui::Context;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
};

pub const STORAGE_KEY: &str = "sannygit.llm_models";

type ApiKeyLoader = fn(&mut Config, Option<&str>) -> Result<(), String>;

enum ModelLoad {
    Startup,
    InMemory,
    SavedKey(ApiKeyLoader),
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ModelCatalog {
    entries: BTreeMap<String, ModelList>,
    #[serde(skip)]
    pub(super) log_path: Option<PathBuf>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ModelList {
    pub options: Vec<String>,
    #[serde(skip)]
    pub error: Option<String>,
    #[serde(skip)]
    pub attempted: bool,
    #[serde(skip)]
    receiver: Option<Receiver<Result<Vec<String>, String>>>,
}

impl ModelCatalog {
    pub fn entry(&mut self, config: &Config, repository: Option<&str>) -> &mut ModelList {
        let endpoint = crate::llm::models_endpoint(&config.endpoint)
            .unwrap_or_else(|_| config.endpoint.trim().to_owned());
        let scope = repository.unwrap_or("");
        let key = format!("{}:{scope}:{endpoint}", scope.len());
        self.entries.entry(key).or_default()
    }

    pub fn update(&mut self) {
        for models in self.entries.values_mut() {
            models.update(self.log_path.as_deref());
        }
    }

    pub fn load(&mut self, config: &Config, repository: Option<&str>, context: &Context) {
        self.start_load(config, repository, context, ModelLoad::InMemory);
    }

    pub fn load_from_dropdown(
        &mut self,
        config: &Config,
        repository: Option<&str>,
        context: &Context,
    ) {
        self.start_load(
            config,
            repository,
            context,
            ModelLoad::SavedKey(crate::llm::load_api_key_if_needed),
        );
    }

    pub(super) fn load_at_startup(
        &mut self,
        config: &Config,
        repository: Option<&str>,
        context: &Context,
    ) {
        self.start_load(config, repository, context, ModelLoad::Startup);
    }

    fn start_load(
        &mut self,
        config: &Config,
        repository: Option<&str>,
        context: &Context,
        load: ModelLoad,
    ) {
        let log_path = self.log_path.clone();
        let models = self.entry(config, repository);
        // Replace the receiver so a response using older credentials cannot
        // overwrite a newer refresh. Keep cached options until success.
        models.receiver = None;
        models.attempted = !matches!(load, ModelLoad::Startup)
            || !config.store_api_key
            || !config.api_key.trim().is_empty();
        models.error = None;
        let mut config = config.clone();
        let repository = repository.map(str::to_owned);
        let context = context.clone();
        let (sender, receiver) = mpsc::channel();
        match std::thread::Builder::new()
            .name("llm-models".into())
            .spawn(move || {
                let result = (|| {
                    if let ModelLoad::SavedKey(read_key) = load {
                        read_key(&mut config, repository.as_deref())?;
                    }
                    crate::llm::list_models(&config)
                })();
                let _ = sender.send(result);
                context.request_repaint();
            }) {
            Ok(_) => models.receiver = Some(receiver),
            Err(error) => models.record_error(
                log_path.as_deref(),
                format!("Cannot start model list worker: {error}"),
            ),
        }
    }
}

impl ModelList {
    pub fn loading(&self) -> bool {
        self.receiver.is_some()
    }

    fn update(&mut self, log_path: Option<&Path>) {
        let Some(receiver) = &self.receiver else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Model list worker stopped unexpectedly".into())
            }
        };
        self.receiver = None;
        match result {
            Ok(models) => {
                self.options = models;
                self.error = None;
            }
            Err(error) => self.record_error(log_path, error),
        }
    }

    fn record_error(&mut self, log_path: Option<&Path>, error: String) {
        let line = format!(
            "{} [WARN] Model list: {}\n",
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            error.replace(['\r', '\n'], " ")
        );
        match log_path {
            Some(path) => {
                if let Err(log_error) = append_log(path, &line) {
                    let _ = write!(
                        io::stderr().lock(),
                        "Cannot write SannyGit log: {log_error}\n{line}"
                    );
                }
            }
            None => {
                let _ = io::stderr().lock().write_all(line.as_bytes());
            }
        }
        self.error = Some(error);
    }
}

fn append_log(path: &Path, line: &str) -> io::Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_are_scoped_to_endpoint_and_repository() {
        let config = Config::default();
        let mut catalogue = ModelCatalog::default();
        catalogue.entry(&config, None).options = vec!["global".into()];
        catalogue.entry(&config, Some("/repo")).options = vec!["repo".into()];
        let other = Config {
            endpoint: "http://localhost:1234/v1".into(),
            ..config.clone()
        };
        assert!(catalogue.entry(&other, None).options.is_empty());
        assert_eq!(catalogue.entry(&config, None).options, ["global"]);
        assert_eq!(catalogue.entry(&config, Some("/repo")).options, ["repo"]);
        let base = Config {
            endpoint: "https://api.openai.com/v1/".into(),
            ..config.clone()
        };
        assert_eq!(catalogue.entry(&base, None).options, ["global"]);
    }

    #[test]
    fn dropdown_request_uses_the_saved_key_for_its_repository() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
            time::{Duration, Instant},
        };

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let config = Config {
            endpoint: format!("http://{}/v1", listener.local_addr().unwrap()),
            ..Config::default()
        };
        let mut catalog = ModelCatalog::default();
        catalog.start_load(
            &config,
            Some("/repo"),
            &Context::default(),
            ModelLoad::SavedKey(|config, repository| {
                assert_eq!(repository, Some("/repo"));
                assert!(config.api_key.is_empty());
                config.api_key = "saved-test-key".into();
                Ok(())
            }),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "No model list request received");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("Cannot accept model list request: {error}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut headers = BufReader::new(&mut socket).lines();
        assert_eq!(headers.next().unwrap().unwrap(), "GET /v1/models HTTP/1.1");
        assert!(headers
            .map(Result::unwrap)
            .take_while(|line| !line.is_empty())
            .any(|line| line == "authorization: Bearer saved-test-key"));
        let body = r#"{"data":[{"id":"saved-key-model"}]}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let models = catalog.entry(&config, Some("/repo"));
        let result = models
            .receiver
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(result.unwrap(), ["saved-key-model"]);
        assert!(config.api_key.is_empty());
    }

    #[test]
    fn cache_survives_restart_and_refresh_failure() {
        let config = Config {
            api_key: "secret-not-to-save".into(),
            ..Config::default()
        };
        let mut catalogue = ModelCatalog::default();
        let models = catalogue.entry(&config, None);
        models.options = vec!["cached-model".into()];
        models.attempted = true;
        models.error = Some("transient error".into());
        let saved = serde_json::to_string(&catalogue).unwrap();
        assert!(!saved.contains(&config.api_key));
        assert!(!saved.contains("transient error"));
        let mut restored: ModelCatalog = serde_json::from_str(&saved).unwrap();
        let log_path = std::env::temp_dir().join(format!(
            "sannygit-model-catalog-{}-{}.log",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap(),
        ));
        let models = restored.entry(&config, None);
        assert_eq!(models.options, ["cached-model"]);
        assert!(!models.attempted);
        let (sender, receiver) = mpsc::channel();
        models.receiver = Some(receiver);
        models.update(Some(&log_path));
        assert!(models.loading());
        assert_eq!(models.options, ["cached-model"]);
        sender.send(Err("HTTP 401".into())).unwrap();
        models.update(Some(&log_path));
        models.update(Some(&log_path));
        assert!(!models.loading());
        assert_eq!(models.options, ["cached-model"]);
        assert_eq!(models.error.as_deref(), Some("HTTP 401"));
        let log = fs::read_to_string(&log_path).unwrap();
        fs::remove_file(log_path).unwrap();
        assert_eq!(log.lines().count(), 1);
        assert!(log.contains("[WARN] Model list: HTTP 401"));
        assert!(!log.contains(&config.api_key));
        let (sender, receiver) = mpsc::channel();
        models.receiver = Some(receiver);
        sender.send(Ok(vec!["updated-model".into()])).unwrap();
        models.update(None);
        assert_eq!(models.options, ["updated-model"]);
        assert!(models.error.is_none());
        assert_eq!(config.model, crate::llm::DEFAULT_MODEL);
    }
}
