use crate::llm::Config;
use eframe::egui::Context;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver},
};

pub const STORAGE_KEY: &str = "sannygit.llm_models";

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ModelCatalog {
    entries: BTreeMap<String, ModelList>,
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

    pub(super) fn errors(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .filter_map(|(key, models)| models.error.as_deref().map(|error| (key.as_str(), error)))
    }

    pub fn update(&mut self) {
        for models in self.entries.values_mut() {
            models.update();
        }
    }

    pub fn load(&mut self, config: &Config, repository: Option<&str>, context: &Context) {
        let models = self.entry(config, repository);
        // Replace the receiver so a response using older credentials cannot
        // overwrite a newer refresh. Keep cached options until success.
        models.receiver = None;
        models.attempted = true;
        models.error = None;
        // Model discovery uses only credentials already available in memory.
        // In particular, startup refresh must never prompt the OS key store.
        let config = config.clone();
        let context = context.clone();
        let (sender, receiver) = mpsc::channel();
        match std::thread::Builder::new()
            .name("llm-models".into())
            .spawn(move || {
                let result = crate::llm::list_models(&config);
                let _ = sender.send(result);
                context.request_repaint();
            }) {
            Ok(_) => models.receiver = Some(receiver),
            Err(error) => models.error = Some(format!("Cannot start model list worker: {error}")),
        }
    }
}

impl ModelList {
    pub fn loading(&self) -> bool {
        self.receiver.is_some()
    }

    fn update(&mut self) {
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
            Err(error) => self.error = Some(error),
        }
    }
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
        let models = restored.entry(&config, None);
        assert_eq!(models.options, ["cached-model"]);
        assert!(!models.attempted);
        let (sender, receiver) = mpsc::channel();
        models.receiver = Some(receiver);
        models.update();
        assert!(models.loading());
        assert_eq!(models.options, ["cached-model"]);
        sender.send(Err("HTTP 401".into())).unwrap();
        models.update();
        assert!(!models.loading());
        assert_eq!(models.options, ["cached-model"]);
        assert_eq!(models.error.as_deref(), Some("HTTP 401"));
        let (sender, receiver) = mpsc::channel();
        models.receiver = Some(receiver);
        sender.send(Ok(vec!["updated-model".into()])).unwrap();
        models.update();
        assert_eq!(models.options, ["updated-model"]);
        assert!(models.error.is_none());
        assert_eq!(config.model, crate::llm::DEFAULT_MODEL);
    }
}
