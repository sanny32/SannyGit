//! Application settings stored as pretty JSON under OS-appropriate directories.
//!
//! eframe still persists window geometry and egui memory in `frame.ron` under the data
//! directory. Everything else lives in human-readable JSON files.

use crate::{
    app::{ModelCatalog, PersistedCommitDraft, LLM_MODELS_STORAGE_KEY},
    branches::ResetStrategy,
    llm::Config,
    preferences::Preferences as AppPreferences,
    repository::Preferences as RepositoryPreferences,
    theme::{InterfaceStyle, ThemeChoice, INTERFACE_STYLE_STORAGE_KEY, THEME_STORAGE_KEY},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::Write, path::PathBuf};

pub const PREFERENCES_FILE: &str = "preferences.json";
pub const APPEARANCE_FILE: &str = "appearance.json";
pub const REPOSITORIES_FILE: &str = "repositories.json";
pub const LLM_FILE: &str = "llm.json";
pub const WORKSPACE_FILE: &str = "workspace.json";
pub const COMMIT_DRAFTS_FILE: &str = "commit_drafts.json";
pub const FRAME_FILE: &str = "frame.ron";

#[derive(Debug, Serialize, Deserialize)]
pub struct AppearanceDocument {
    pub theme: ThemeChoice,
    pub interface_style: InterfaceStyle,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RepositoriesDocument {
    #[serde(default)]
    pub paths: Vec<String>,
    pub active: Option<String>,
    #[serde(default)]
    pub bookmarks: RepositoryPreferences,
}

#[derive(Default, Serialize, Deserialize)]
pub struct LlmDocument {
    #[serde(default)]
    pub global: Config,
    #[serde(default)]
    pub repository_llm: BTreeMap<String, Config>,
    #[serde(default)]
    pub repository_llm_instructions: BTreeMap<String, String>,
    #[serde(default)]
    pub models: ModelCatalog,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceDocument {
    #[serde(default)]
    pub skip_push_confirmation: bool,
    #[serde(default = "default_auto_fetch")]
    pub auto_fetch: bool,
    #[serde(default)]
    pub reset_strategy: ResetStrategy,
}

impl Default for WorkspaceDocument {
    fn default() -> Self {
        Self {
            skip_push_confirmation: false,
            auto_fetch: true,
            reset_strategy: ResetStrategy::default(),
        }
    }
}

fn default_auto_fetch() -> bool {
    true
}

/// User settings and bookmarks (small, may roam on Windows).
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| std::env::home_dir().map(|home| home.join(".config")))?;
        Some(base.join("sannygit"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::home_dir().map(|home| {
            home.join("Library")
                .join("Application Support")
                .join("SannyGit")
        })
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|roaming| PathBuf::from(roaming).join("SannyGit"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Local state: commit drafts, frame persistence, logs, OAuth locks.
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| std::env::home_dir().map(|home| home.join(".local").join("share")))?;
        Some(base.join("sannygit"))
    }
    #[cfg(target_os = "macos")]
    {
        config_dir().map(|root| root.join("Data"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join("SannyGit"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

pub fn frame_storage_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join(FRAME_FILE))
}

pub struct Loaded {
    pub preferences: AppPreferences,
    pub saved_theme: Option<ThemeChoice>,
    pub interface_style: InterfaceStyle,
    pub skip_push_confirmation: bool,
    pub auto_fetch: bool,
    pub reset_strategy: ResetStrategy,
    pub llm_config: Config,
    pub repository_llm_configs: BTreeMap<String, Config>,
    pub repository_llm_instructions: BTreeMap<String, String>,
    pub llm_models: ModelCatalog,
    pub repository_paths: Vec<String>,
    pub repository_preferences: RepositoryPreferences,
    pub active_repository: Option<String>,
    pub commit_drafts: BTreeMap<String, PersistedCommitDraft>,
}

impl Default for Loaded {
    fn default() -> Self {
        Self {
            preferences: AppPreferences::default(),
            saved_theme: None,
            interface_style: InterfaceStyle::default(),
            skip_push_confirmation: false,
            auto_fetch: true,
            reset_strategy: ResetStrategy::default(),
            llm_config: Config::default(),
            repository_llm_configs: BTreeMap::new(),
            repository_llm_instructions: BTreeMap::new(),
            llm_models: ModelCatalog::default(),
            repository_paths: Vec::new(),
            repository_preferences: RepositoryPreferences::default(),
            active_repository: None,
            commit_drafts: BTreeMap::new(),
        }
    }
}

pub fn load(_storage: Option<&dyn eframe::Storage>) -> Loaded {
    if cfg!(test) {
        return load_from_storage(_storage);
    }
    load_from_disk().unwrap_or_default()
}

pub fn save(app: &crate::app::SannyGitApp, storage: &mut dyn eframe::Storage) {
    if cfg!(test) {
        save_to_storage(app, storage);
        return;
    }
    let _ = save_to_disk(app);
    strip_app_keys_from_frame_storage(storage);
}

/// Keep eframe `frame.ron` limited to window geometry and egui memory.
fn strip_app_keys_from_frame_storage(storage: &mut dyn eframe::Storage) {
    const APP_KEYS: &[&str] = &[
        crate::preferences::STORAGE_KEY,
        THEME_STORAGE_KEY,
        INTERFACE_STYLE_STORAGE_KEY,
        LLM_MODELS_STORAGE_KEY,
        "sannygit.commit_drafts",
        "sannygit.skip_push_confirmation",
        "sannygit.auto_fetch",
        "sannygit.reset_strategy",
        "sannygit.llm",
        "sannygit.repository_llm_instructions",
        "sannygit.repository_llm",
        "sannygit.repositories",
        "sannygit.active_repository",
        "sannygit.repository_preferences",
    ];
    for key in APP_KEYS {
        storage.remove_string(key);
    }
}

fn load_from_disk() -> Option<Loaded> {
    let preferences = read_json(config_dir()?.join(PREFERENCES_FILE))?;
    let appearance: AppearanceDocument =
        read_json(config_dir()?.join(APPEARANCE_FILE)).unwrap_or(AppearanceDocument {
            theme: ThemeChoice::Light,
            interface_style: InterfaceStyle::default(),
        });
    let repositories: RepositoriesDocument =
        read_json(config_dir()?.join(REPOSITORIES_FILE)).unwrap_or_default();
    let llm: LlmDocument = read_json(config_dir()?.join(LLM_FILE)).unwrap_or_default();
    let workspace: WorkspaceDocument =
        read_json(config_dir()?.join(WORKSPACE_FILE)).unwrap_or_default();
    let commit_drafts: BTreeMap<String, PersistedCommitDraft> =
        read_json(data_dir()?.join(COMMIT_DRAFTS_FILE)).unwrap_or_default();

    Some(Loaded {
        preferences,
        saved_theme: Some(appearance.theme),
        interface_style: appearance.interface_style,
        skip_push_confirmation: workspace.skip_push_confirmation,
        auto_fetch: workspace.auto_fetch,
        reset_strategy: workspace.reset_strategy,
        llm_config: llm.global,
        repository_llm_configs: llm.repository_llm,
        repository_llm_instructions: llm.repository_llm_instructions,
        llm_models: llm.models,
        repository_paths: repositories.paths,
        repository_preferences: repositories.bookmarks,
        active_repository: repositories.active,
        commit_drafts,
    })
}

fn save_to_disk(app: &crate::app::SannyGitApp) -> Result<(), String> {
    save_loaded(&app.persistable_state())
}

fn save_loaded(loaded: &Loaded) -> Result<(), String> {
    let config = config_dir().ok_or("Cannot determine config directory")?;
    fs::create_dir_all(&config).map_err(|error| error.to_string())?;
    let data = data_dir().ok_or("Cannot determine data directory")?;
    fs::create_dir_all(&data).map_err(|error| error.to_string())?;

    write_json(config.join(PREFERENCES_FILE), &loaded.preferences)?;
    write_json(
        config.join(APPEARANCE_FILE),
        &AppearanceDocument {
            theme: loaded.saved_theme.unwrap_or(ThemeChoice::Dark),
            interface_style: loaded.interface_style,
        },
    )?;
    write_json(
        config.join(REPOSITORIES_FILE),
        &RepositoriesDocument {
            paths: loaded.repository_paths.clone(),
            active: loaded.active_repository.clone(),
            bookmarks: loaded.repository_preferences.clone(),
        },
    )?;
    write_json(
        config.join(LLM_FILE),
        &LlmDocument {
            global: loaded.llm_config.clone(),
            repository_llm: loaded.repository_llm_configs.clone(),
            repository_llm_instructions: loaded.repository_llm_instructions.clone(),
            models: loaded.llm_models.clone(),
        },
    )?;
    write_json(
        config.join(WORKSPACE_FILE),
        &WorkspaceDocument {
            skip_push_confirmation: loaded.skip_push_confirmation,
            auto_fetch: loaded.auto_fetch,
            reset_strategy: loaded.reset_strategy,
        },
    )?;
    write_json(data.join(COMMIT_DRAFTS_FILE), &loaded.commit_drafts)?;
    Ok(())
}

fn load_from_storage(storage: Option<&dyn eframe::Storage>) -> Loaded {
    let Some(storage) = storage else {
        return Loaded::default();
    };
    let mut loaded = Loaded::default();
    loaded.saved_theme = get_value(storage, THEME_STORAGE_KEY);
    loaded.preferences =
        get_value(storage, crate::preferences::STORAGE_KEY).unwrap_or_else(|| AppPreferences {
            follow_system_theme: loaded.saved_theme.is_none(),
            ..Default::default()
        });
    loaded.interface_style = get_value(storage, INTERFACE_STYLE_STORAGE_KEY).unwrap_or_default();
    loaded.skip_push_confirmation =
        get_value(storage, "sannygit.skip_push_confirmation").unwrap_or_default();
    loaded.auto_fetch = get_value(storage, "sannygit.auto_fetch").unwrap_or(true);
    loaded.reset_strategy = get_value(storage, "sannygit.reset_strategy").unwrap_or_default();
    loaded.llm_config = get_value(storage, "sannygit.llm").unwrap_or_default();
    loaded.repository_llm_configs =
        get_value(storage, "sannygit.repository_llm").unwrap_or_default();
    loaded.repository_llm_instructions =
        get_value(storage, "sannygit.repository_llm_instructions").unwrap_or_default();
    loaded.llm_models = get_value(storage, LLM_MODELS_STORAGE_KEY).unwrap_or_default();
    loaded.repository_paths = get_value(storage, "sannygit.repositories").unwrap_or_default();
    loaded.active_repository = get_value(storage, "sannygit.active_repository").unwrap_or(None);
    loaded.repository_preferences =
        get_value(storage, "sannygit.repository_preferences").unwrap_or_default();
    loaded.commit_drafts = get_value(storage, "sannygit.commit_drafts").unwrap_or_default();
    loaded
}

fn save_to_storage(app: &crate::app::SannyGitApp, storage: &mut dyn eframe::Storage) {
    let loaded = app.persistable_state();
    set_value(
        storage,
        crate::preferences::STORAGE_KEY,
        &loaded.preferences,
    );
    set_value(
        storage,
        THEME_STORAGE_KEY,
        &loaded.saved_theme.unwrap_or(ThemeChoice::Dark),
    );
    set_value(
        storage,
        INTERFACE_STYLE_STORAGE_KEY,
        &loaded.interface_style,
    );
    set_value(
        storage,
        "sannygit.skip_push_confirmation",
        &loaded.skip_push_confirmation,
    );
    set_value(storage, "sannygit.auto_fetch", &loaded.auto_fetch);
    set_value(storage, "sannygit.reset_strategy", &loaded.reset_strategy);
    set_value(storage, "sannygit.llm", &loaded.llm_config);
    set_value(
        storage,
        "sannygit.repository_llm_instructions",
        &loaded.repository_llm_instructions,
    );
    set_value(
        storage,
        "sannygit.repository_llm",
        &loaded.repository_llm_configs,
    );
    set_value(storage, LLM_MODELS_STORAGE_KEY, &loaded.llm_models);
    set_value(storage, "sannygit.repositories", &loaded.repository_paths);
    set_value(
        storage,
        "sannygit.active_repository",
        &loaded.active_repository,
    );
    set_value(
        storage,
        "sannygit.repository_preferences",
        &loaded.repository_preferences,
    );
    set_value(storage, "sannygit.commit_drafts", &loaded.commit_drafts);
}

pub fn get_value<T: DeserializeOwned>(storage: &dyn eframe::Storage, key: &str) -> Option<T> {
    let value = storage.get_string(key)?;
    serde_json::from_str(&value).ok()
}

pub fn set_value<T: Serialize>(storage: &mut dyn eframe::Storage, key: &str, value: &T) {
    match serde_json::to_string(value) {
        Ok(string) => storage.set_string(key, string),
        Err(error) => crate::logging::record(
            crate::logging::Level::Error,
            format_args!("Failed to encode JSON for {key}: {error}"),
        ),
    }
}

fn read_json<T: DeserializeOwned>(path: PathBuf) -> Option<T> {
    if !path.is_file() {
        return None;
    }
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_json<T: Serialize>(path: PathBuf, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    let temp = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&temp).map_err(|error| error.to_string())?;
        file.write_all(text.as_bytes())
            .map_err(|error| error.to_string())?;
        file.write_all(b"\n").map_err(|error| error.to_string())?;
    }
    fs::rename(temp, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        preferences::{FileDoubleClick, Preferences as AppPreferences},
        repository::{Preferences as RepositoryPreferences, RepositoryGroup},
    };

    #[test]
    fn json_round_trip_preferences() {
        let preferences = AppPreferences {
            file_double_click: FileDoubleClick::ToggleStage,
            git_executable: "git".into(),
            ..Default::default()
        };
        let dir = std::env::temp_dir().join(format!(
            "sannygit-persist-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(PREFERENCES_FILE);
        write_json(path.clone(), &preferences).unwrap();
        let restored: AppPreferences = read_json(path).unwrap();
        assert_eq!(restored, preferences);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_storage_uses_json_encoding() {
        #[derive(Default)]
        struct MemoryStorage(std::collections::BTreeMap<String, String>);
        impl eframe::Storage for MemoryStorage {
            fn get_string(&self, key: &str) -> Option<String> {
                self.0.get(key).cloned()
            }
            fn set_string(&mut self, key: &str, value: String) {
                self.0.insert(key.to_owned(), value);
            }
            fn remove_string(&mut self, _: &str) {}
            fn flush(&mut self) {}
        }

        let mut storage = MemoryStorage::default();
        set_value(&mut storage, "sannygit.auto_fetch", &false);
        let value: bool = get_value(&storage, "sannygit.auto_fetch").unwrap();
        assert!(!value);
        assert!(storage
            .0
            .get("sannygit.auto_fetch")
            .unwrap()
            .starts_with("false"));
    }

    #[test]
    fn repositories_document_keeps_bookmarks() {
        let document = RepositoriesDocument {
            paths: vec!["/a".into(), "/b".into()],
            active: Some("/a".into()),
            bookmarks: RepositoryPreferences {
                groups: vec![RepositoryGroup {
                    name: "Team".into(),
                    paths: vec!["/a".into()],
                }],
                selected_group: Some("Team".into()),
                ..Default::default()
            },
        };
        let dir = std::env::temp_dir().join(format!("sannygit-repos-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(REPOSITORIES_FILE);
        write_json(path.clone(), &document).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"Team\""));
        let restored: RepositoriesDocument = read_json(path).unwrap();
        assert_eq!(restored.paths, document.paths);
        assert_eq!(restored.bookmarks.groups[0].name, "Team");
        fs::remove_dir_all(dir).ok();
    }
}
