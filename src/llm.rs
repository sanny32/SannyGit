use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
pub const DEFAULT_MODEL: &str = "gpt-4.1-mini";
const MAX_DIFF_BYTES: usize = 256 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const KEYRING_SERVICE: &str = "SannyGit";
#[cfg(target_os = "macos")]
const PROTECTED_KEYRING_SERVICE: &str = "SannyGit LLM API keys";
const LEGACY_KEYRING_USER: &str = "llm-api-key";

pub struct ProviderPreset {
    pub name: &'static str,
    pub endpoint: &'static str,
    pub model: &'static str,
}

pub const PROVIDER_PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        name: "OpenAI",
        endpoint: DEFAULT_ENDPOINT,
        model: DEFAULT_MODEL,
    },
    ProviderPreset {
        name: "Kimi",
        endpoint: "https://api.kimi.ai/coding/v1/chat/completions",
        model: "kimi-for-coding",
    },
    ProviderPreset {
        name: "Google Gemini",
        endpoint: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
        model: "gemini-2.5-flash",
    },
    ProviderPreset {
        name: "DeepSeek",
        endpoint: "https://api.deepseek.com/v1/chat/completions",
        model: "deepseek-chat",
    },
    ProviderPreset {
        name: "OpenRouter",
        endpoint: "https://openrouter.ai/api/v1/chat/completions",
        model: "openai/gpt-4.1-mini",
    },
    ProviderPreset {
        name: "xAI",
        endpoint: "https://api.x.ai/v1/chat/completions",
        model: "grok-3-mini",
    },
    ProviderPreset {
        name: "Groq",
        endpoint: "https://api.groq.com/openai/v1/chat/completions",
        model: "llama-3.3-70b-versatile",
    },
    ProviderPreset {
        name: "Mistral AI",
        endpoint: "https://api.mistral.ai/v1/chat/completions",
        model: "mistral-small-latest",
    },
    ProviderPreset {
        name: "Ollama (local)",
        endpoint: "http://localhost:11434/v1/chat/completions",
        model: "",
    },
    ProviderPreset {
        name: "LM Studio (local)",
        endpoint: "http://localhost:1234/v1/chat/completions",
        model: "",
    },
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub endpoint: String,
    pub model: String,
    pub instructions: String,
    pub store_api_key: bool,
    #[serde(skip)]
    pub api_key: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.into(),
            model: DEFAULT_MODEL.into(),
            instructions: "Write a concise Git commit message in English. Use an imperative subject of at most 72 characters. Add a short body only when it helps explain why. Return only the commit message.".into(),
            store_api_key: true,
            api_key: String::new(),
        }
    }
}

// The model belongs to the credential, since provider defaults may not work
// with a particular API key. Plain secrets from older versions remain readable.
#[derive(Deserialize, Serialize)]
struct SavedCredential {
    model: String,
    api_key: String,
}

fn restore_credential(config: &mut Config, secret: String) -> Result<(), String> {
    if secret.starts_with('{') {
        let saved: SavedCredential = serde_json::from_str(&secret)
            .map_err(|_| "The saved LLM credential is invalid".to_owned())?;
        config.model = saved.model;
        config.api_key = saved.api_key;
    } else {
        config.api_key = secret;
    }
    Ok(())
}

fn credential_user(endpoint: &str, repository: Option<&str>) -> String {
    let endpoint = chat_completions_endpoint(endpoint);
    match repository {
        Some(path) => format!("llm-api-key:repository:{}:{path}:{endpoint}", path.len()),
        None => format!("llm-api-key:{endpoint}"),
    }
}

fn credential_entry(user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, user)
        .map_err(|error| format!("Cannot access the OS credential manager: {error}"))
}

#[cfg(target_os = "macos")]
mod macos_credentials {
    use security_framework::access_control::{ProtectionMode, SecAccessControl};
    use security_framework::passwords::{
        delete_generic_password_options, generic_password, set_generic_password_options,
        AccessControlOptions, PasswordOptions,
    };
    use std::process::{Command, Output};

    use super::{
        credential_entry, KEYRING_SERVICE, LEGACY_KEYRING_USER, PROTECTED_KEYRING_SERVICE,
    };

    const ITEM_NOT_FOUND: i32 = -25_300;
    const MISSING_ENTITLEMENT: i32 = -34_018;
    const SECURITY_ITEM_NOT_FOUND: i32 = 44;

    fn options(user: &str) -> PasswordOptions {
        let mut options = PasswordOptions::new_generic_password(PROTECTED_KEYRING_SERVICE, user);
        options.use_protected_keychain();
        options
    }

    fn security(arguments: &[&str]) -> Result<Output, String> {
        Command::new("/usr/bin/security")
            .args(arguments)
            .output()
            .map_err(|error| format!("Cannot access the macOS Keychain: {error}"))
    }

    fn legacy_security_tool_owned() -> Result<Option<bool>, String> {
        let output = security(&[
            "find-generic-password",
            "-s",
            KEYRING_SERVICE,
            "-a",
            LEGACY_KEYRING_USER,
        ])?;
        if output.status.code() == Some(SECURITY_ITEM_NOT_FOUND) {
            return Ok(None);
        }
        if !output.status.success() {
            return Err("Cannot inspect the legacy LLM API key in the macOS Keychain".into());
        }
        let metadata = String::from_utf8(output.stdout)
            .map_err(|_| "Cannot inspect the legacy LLM API key metadata".to_owned())?;
        Ok(Some(metadata.contains("\"crtr\"<uint32>=\"aapl\"")))
    }

    pub fn load_unprotected(user: &str) -> Result<Option<String>, String> {
        match credential_entry(user)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(format!(
                "Cannot read the legacy LLM API key from the OS credential manager: {error}"
            )),
        }
    }

    pub fn load_legacy() -> Result<Option<String>, String> {
        let Some(security_tool_owned) = legacy_security_tool_owned()? else {
            return Ok(None);
        };
        if !security_tool_owned {
            return load_unprotected(LEGACY_KEYRING_USER);
        }
        let output = security(&[
            "find-generic-password",
            "-s",
            KEYRING_SERVICE,
            "-a",
            LEGACY_KEYRING_USER,
            "-w",
        ])?;
        if output.status.code() == Some(SECURITY_ITEM_NOT_FOUND) {
            return Ok(None);
        }
        if !output.status.success() {
            return Err("Cannot read the legacy LLM API key from the macOS Keychain".into());
        }
        let mut secret = String::from_utf8(output.stdout)
            .map_err(|_| "The saved LLM API key is not valid UTF-8".to_owned())?;
        if secret.ends_with('\n') {
            secret.pop();
            if secret.ends_with('\r') {
                secret.pop();
            }
        }
        Ok(Some(secret))
    }

    pub fn delete_legacy() -> Result<(), String> {
        delete_unprotected(LEGACY_KEYRING_USER)
    }

    pub fn delete_unprotected(user: &str) -> Result<(), String> {
        // keyring::Entry::delete_credential reads the secret again before
        // deleting it. SecItemDelete only needs the service/account query.
        delete_password(PasswordOptions::new_generic_password(KEYRING_SERVICE, user))
    }

    pub fn save(user: &str, secret: &str) -> Result<(), String> {
        let mut options = options(user);
        // Keychain itself authenticates the read; a separate LAContext policy
        // evaluation cannot authorize access to a legacy login-keychain ACL.
        let access = SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly),
            AccessControlOptions::BIOMETRY_CURRENT_SET.bits(),
        )
        .map_err(|error| format!("Cannot configure Touch ID protection: {error}"))?;
        options.set_access_control(access);
        options.set_label("SannyGit LLM API key");
        set_generic_password_options(secret.as_bytes(), options).map_err(|error| {
            if error.code() == MISSING_ENTITLEMENT {
                signing_error()
            } else {
                format!("Cannot save the LLM API key with Touch ID protection: {error}")
            }
        })
    }

    fn signing_error() -> String {
        "Touch ID key storage requires a signed SannyGit application with Keychain entitlements. This build cannot access the data protection keychain. Use a properly signed build, or turn off secure storage and enter the API key for this session.".into()
    }

    pub fn load(user: &str) -> Result<Option<String>, String> {
        match generic_password(options(user)) {
            Ok(secret) => String::from_utf8(secret)
                .map(Some)
                .map_err(|_| "The saved LLM API key is not valid UTF-8".into()),
            Err(error) if error.code() == ITEM_NOT_FOUND => Ok(None),
            // Do not fall back to the login keychain after a protected read
            // fails: it can prompt for a password after a Touch ID attempt.
            Err(error) if error.code() == MISSING_ENTITLEMENT => Err(signing_error()),
            Err(error) => Err(format!(
                "Cannot unlock the LLM API key with Touch ID: {error}"
            )),
        }
    }

    pub fn delete(user: &str) -> Result<(), String> {
        delete_password(options(user))
    }

    fn delete_password(options: PasswordOptions) -> Result<(), String> {
        match delete_generic_password_options(options) {
            Ok(()) => Ok(()),
            Err(error) if matches!(error.code(), ITEM_NOT_FOUND | MISSING_ENTITLEMENT) => Ok(()),
            Err(error) => Err(format!("Cannot remove the saved LLM API key: {error}")),
        }
    }
}

#[cfg(any(target_os = "macos", test))]
fn load_api_key_from_stores(
    protected: impl FnOnce() -> Result<Option<String>, String>,
    scoped: impl FnOnce() -> Result<Option<String>, String>,
    legacy: impl FnOnce() -> Result<Option<String>, String>,
) -> Result<Option<String>, String> {
    if let Some(secret) = protected()? {
        return Ok(Some(secret));
    }
    if let Some(secret) = scoped()? {
        return Ok(Some(secret));
    }
    legacy()
}

#[cfg(any(target_os = "macos", test))]
fn load_and_migrate_legacy_api_key(
    load: impl FnOnce() -> Result<Option<String>, String>,
    save: impl FnOnce(&str) -> Result<(), String>,
    delete: impl FnOnce() -> Result<(), String>,
) -> Result<Option<String>, String> {
    let Some(secret) = load()? else {
        return Ok(None);
    };
    // Keep the source until the destination is safely written, then return
    // the value already read instead of querying either entry again.
    save(&secret)?;
    delete()?;
    Ok(Some(secret))
}

fn load_api_key(config: &Config, repository: Option<&str>) -> Result<Option<String>, String> {
    if !config.store_api_key {
        return Ok(None);
    }
    let endpoint = chat_completions_endpoint(&config.endpoint);
    let user = credential_user(&endpoint, repository);
    #[cfg(target_os = "macos")]
    {
        load_api_key_from_stores(
            || macos_credentials::load(&user),
            || {
                load_and_migrate_legacy_api_key(
                    || macos_credentials::load_unprotected(&user),
                    |secret| macos_credentials::save(&user, secret),
                    || macos_credentials::delete_unprotected(&user),
                )
            },
            || {
                if repository.is_none() && endpoint == DEFAULT_ENDPOINT {
                    load_and_migrate_legacy_api_key(
                        macos_credentials::load_legacy,
                        |secret| macos_credentials::save(&user, secret),
                        macos_credentials::delete_legacy,
                    )
                } else {
                    Ok(None)
                }
            },
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let entry = credential_entry(&user)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry)
                if repository.is_none() && endpoint == DEFAULT_ENDPOINT =>
            {
                match credential_entry(LEGACY_KEYRING_USER)?.get_password() {
                    Ok(secret) => Ok(Some(secret)),
                    Err(keyring::Error::NoEntry) => Ok(None),
                    Err(error) => Err(format!(
                        "Cannot read the LLM API key from the OS credential manager: {error}"
                    )),
                }
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(format!(
                "Cannot read the LLM API key from the OS credential manager: {error}"
            )),
        }
    }
}

pub fn load_api_key_if_needed(config: &mut Config, repository: Option<&str>) -> Result<(), String> {
    if config.api_key.is_empty() {
        if let Some(api_key) = load_api_key(config, repository)? {
            restore_credential(config, api_key)?;
        }
    }
    Ok(())
}

pub fn save_api_key(config: &Config, repository: Option<&str>) -> Result<(), String> {
    let endpoint = chat_completions_endpoint(&config.endpoint);
    let user = credential_user(&endpoint, repository);
    #[cfg(not(target_os = "macos"))]
    let entry = credential_entry(&user)?;
    if config.store_api_key && !config.api_key.is_empty() {
        let secret = serde_json::to_string(&SavedCredential {
            model: config.model.trim().to_owned(),
            api_key: config.api_key.clone(),
        })
        .map_err(|_| "Cannot encode the LLM credential".to_owned())?;
        #[cfg(target_os = "macos")]
        {
            macos_credentials::save(&user, &secret)?;
            macos_credentials::delete_unprotected(&user)?;
        }
        #[cfg(not(target_os = "macos"))]
        entry
            .set_password(&secret)
            .map_err(|error| format!("Cannot save the LLM API key securely: {error}"))?;
    } else {
        #[cfg(target_os = "macos")]
        {
            macos_credentials::delete(&user)?;
            macos_credentials::delete_unprotected(&user)?;
        }
        #[cfg(not(target_os = "macos"))]
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(error) => {
                return Err(format!("Cannot remove the saved LLM API key: {error}"));
            }
        }
    }
    if repository.is_none() && endpoint == DEFAULT_ENDPOINT {
        #[cfg(target_os = "macos")]
        {
            macos_credentials::delete_legacy()
        }
        #[cfg(not(target_os = "macos"))]
        {
            match credential_entry(LEGACY_KEYRING_USER)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(format!("Cannot remove the saved LLM API key: {error}")),
            }
        }
    } else {
        Ok(())
    }
}

impl Config {
    pub fn is_local(&self) -> bool {
        reqwest::Url::parse(self.endpoint.trim())
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .is_some_and(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host
                        .trim_matches(['[', ']'])
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|address| address.is_loopback())
            })
    }

    pub fn validate(&self) -> Result<(), String> {
        let endpoint = self.endpoint.trim();
        if !(endpoint.starts_with("https://") || endpoint.starts_with("http://")) {
            return Err("LLM endpoint must start with http:// or https://".into());
        }
        if self.model.trim().is_empty() {
            return Err("Enter an LLM model name".into());
        }
        if self.instructions.trim().is_empty() {
            return Err("Enter instructions for commit message generation".into());
        }
        Ok(())
    }
}

pub fn generate(config: &Config, patch: &str, cancel: &AtomicBool) -> Result<String, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Commit message generation cancelled".into());
    }
    config.validate()?;
    if patch.trim().is_empty() {
        return Err("The staged diff is empty".into());
    }
    if patch.len() > MAX_DIFF_BYTES {
        return Err(format!(
            "The commit diff exceeds the {} KiB LLM request limit",
            MAX_DIFF_BYTES / 1024
        ));
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Cannot initialize the LLM runtime: {error}"))?;
    runtime.block_on(async {
        let request = generate_response(config, patch);
        let mut request = std::pin::pin!(request);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Commit message generation cancelled".into());
            }
            // Dropping the future on cancellation closes the HTTP request,
            // including while waiting for headers or reading the response body.
            if let Ok(result) = tokio::time::timeout(Duration::from_millis(50), &mut request).await
            {
                return result;
            }
        }
    })
}

async fn generate_response(config: &Config, patch: &str) -> Result<String, String> {
    let request = request_body(config, patch);
    let client = api_client(config, if config.is_local() { 300 } else { 60 })?;
    let request = client
        .post(chat_completions_endpoint(&config.endpoint))
        .json(&request);
    let request = if config.api_key.trim().is_empty() {
        request
    } else {
        request.bearer_auth(config.api_key.trim())
    };
    let response = request
        .send()
        .await
        .map_err(|error| format!("LLM request failed: {error}"))?;
    parse_message(&read_api_response(response).await?)
}

/// Fetch the provider's OpenAI-compatible model catalogue without generating text.
pub fn list_models(config: &Config) -> Result<Vec<String>, String> {
    let endpoint = models_endpoint(&config.endpoint)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Cannot initialize the LLM runtime: {error}"))?;
    runtime.block_on(async {
        let client = api_client(config, 20)?;
        let request = client.get(endpoint);
        let request = if config.api_key.trim().is_empty() {
            request
        } else {
            request.bearer_auth(config.api_key.trim())
        };
        let response = request
            .send()
            .await
            .map_err(|error| format!("Cannot load models: {}", error.without_url()))?;
        parse_models(&read_api_response(response).await?)
    })
}

pub fn models_endpoint(endpoint: &str) -> Result<String, String> {
    let mut url = reqwest::Url::parse(endpoint.trim())
        .map_err(|_| "Enter a valid LLM endpoint URL".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("LLM endpoint must start with http:// or https://".into());
    }
    let path = url.path().trim_end_matches('/');
    let base = path.strip_suffix("/chat/completions").unwrap_or(path);
    let path = format!("{base}/models");
    url.set_path(&path);
    url.set_fragment(None);
    Ok(url.into())
}

fn parse_models(value: &Value) -> Result<Vec<String>, String> {
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or("LLM API response does not contain a model list")?;
    let mut models = Vec::with_capacity(data.len());
    for model in data {
        let id = model
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or("LLM API model list contains an invalid model ID")?;
        models.push(id.to_owned());
    }
    models.sort();
    models.dedup();
    Ok(models)
}

fn api_client(config: &Config, timeout_seconds: u64) -> Result<Client, String> {
    let local = config.is_local();
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(timeout_seconds));
    // Local diffs must not pass through an environment-configured proxy or
    // follow a server redirect to a different host.
    if local {
        client
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
    } else {
        client
    }
    .build()
    .map_err(|error| format!("Cannot initialize the LLM client: {error}"))
}

async fn read_api_response(mut response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Cannot read the LLM response: {error}"))?
    {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err("LLM response exceeds the 1 MiB limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if !status.is_success() {
        return Err(api_error(status.as_u16(), &bytes));
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("LLM API returned invalid JSON: {error}"))
}

fn chat_completions_endpoint(endpoint: &str) -> String {
    let endpoint = endpoint.trim().trim_end_matches('/');
    if endpoint.ends_with("/chat/completions") {
        endpoint.to_owned()
    } else {
        format!("{endpoint}/chat/completions")
    }
}

fn request_body(config: &Config, patch: &str) -> Value {
    json!({
        "model": config.model.trim(),
        "stream": false,
        "messages": [
            { "role": "system", "content": config.instructions.trim() },
            { "role": "user", "content": format!("Generate a commit message for these changes:\n\n{patch}") }
        ]
    })
}

fn api_error(status: u16, bytes: &[u8]) -> String {
    let detail = serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .or_else(|| std::str::from_utf8(bytes).ok().map(str::to_owned));
    let detail = detail
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty());
    match detail {
        Some(detail) => {
            let mut chars = detail.chars();
            let message: String = chars.by_ref().take(2000).collect();
            let suffix = if chars.next().is_some() { "…" } else { "" };
            format!("LLM API returned HTTP {status}: {message}{suffix}")
        }
        None => format!(
            "LLM API returned HTTP {status}. Check the endpoint, model, API key and provider limits."
        ),
    }
}

fn parse_message(value: &Value) -> Result<String, String> {
    let message = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .ok_or("LLM API response does not contain a commit message")?;
    crate::git::validate_message(message)?;
    Ok(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_urls_accept_base_and_chat_endpoints() {
        for endpoint in [
            " https://example.test/coding/v1/ ",
            "https://example.test/coding/v1/chat/completions",
            "https://example.test/coding/v1/chat/completions/",
        ] {
            assert_eq!(
                models_endpoint(endpoint).unwrap(),
                "https://example.test/coding/v1/models"
            );
        }
        assert_eq!(
            models_endpoint("http://localhost:11434/v1").unwrap(),
            "http://localhost:11434/v1/models"
        );
        assert_eq!(
            models_endpoint("https://example.test/v1/chat/completions?version=1").unwrap(),
            "https://example.test/v1/models?version=1"
        );
        for endpoint in ["", "invalid", "file:///tmp/models", "ftp://example.test/v1"] {
            assert!(models_endpoint(endpoint).is_err());
        }
    }

    #[test]
    fn model_list_is_sorted_unique_and_validated() {
        assert_eq!(
            parse_models(
                &json!({"data": [{"id": "z-model"}, {"id": "a-model"}, {"id": " a-model "}]})
            )
            .unwrap(),
            ["a-model", "z-model"]
        );
        assert!(parse_models(&json!({"data": []})).unwrap().is_empty());
        for value in [
            json!({}),
            json!({"data": {}}),
            json!({"data": [{"id": ""}]}),
            json!({"data": [{"id": 1}]}),
            json!({"data": [{}]}),
        ] {
            assert!(parse_models(&value).is_err());
        }
    }

    #[test]
    fn models_are_loaded_with_optional_auth_and_bounded_responses() {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
        };
        for (status, body, key, expected) in [
            (
                200,
                r#"{"data":[{"id":"installed-model"}]}"#.to_owned(),
                "",
                Ok(vec!["installed-model".to_owned()]),
            ),
            (200, r#"{"data":[]}"#.to_owned(), "test-key", Ok(vec![])),
            (
                401,
                r#"{"error":{"message":"Invalid key"}}"#.to_owned(),
                "test-key",
                Err("HTTP 401"),
            ),
            (404, "Not found".to_owned(), "", Err("HTTP 404")),
            (200, "not json".to_owned(), "", Err("invalid JSON")),
            (
                200,
                " ".repeat(MAX_RESPONSE_BYTES + 1),
                "",
                Err("1 MiB limit"),
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let server = std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "No model list request received"
                            );
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("Cannot accept test request: {error}"),
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut headers = String::new();
                let mut reader = BufReader::new(&mut socket);
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    headers.push_str(&line);
                }
                write!(socket, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                headers
            });
            let config = Config {
                endpoint: format!("http://{address}/v1/chat/completions"),
                model: String::new(),
                instructions: String::new(),
                api_key: key.into(),
                store_api_key: false,
            };
            let result = list_models(&config);
            let headers = server.join().unwrap();
            assert!(headers.starts_with("GET /v1/models HTTP/1.1\r\n"));
            let headers = headers.to_ascii_lowercase();
            if key.is_empty() {
                assert!(!headers.contains("authorization:"));
            } else {
                assert!(headers.contains("authorization: bearer test-key\r\n"));
            }
            match expected {
                Ok(models) => assert_eq!(result.unwrap(), models),
                Err(detail) => assert!(result.unwrap_err().contains(detail)),
            }
        }
    }

    #[test]
    fn cancellation_closes_requests_waiting_for_headers_or_body() {
        use std::{
            io::{BufRead, BufReader, Read, Write},
            net::TcpListener,
            sync::{mpsc, Arc},
        };

        for partial_body in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let (ready_sender, ready_receiver) = mpsc::channel();
            let server = std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(std::time::Instant::now() < deadline);
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("Cannot accept test request: {error}"),
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&mut socket);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                reader.read_exact(&mut vec![0; length]).unwrap();
                if partial_body {
                    socket
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{")
                        .unwrap();
                    socket.flush().unwrap();
                }
                ready_sender.send(()).unwrap();
                let result = socket.read(&mut [0; 1]);
                assert!(
                    matches!(result, Ok(0))
                        || matches!(result, Err(ref error) if error.kind() == std::io::ErrorKind::ConnectionReset),
                    "Request was not closed: {result:?}"
                );
            });
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            let (sender, receiver) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let config = Config {
                    endpoint: format!("http://{address}/v1"),
                    model: "test-model".into(),
                    ..Config::default()
                };
                sender
                    .send(generate(&config, "test patch", &worker_cancel))
                    .unwrap();
            });
            ready_receiver.recv_timeout(Duration::from_secs(5)).unwrap();
            cancel.store(true, Ordering::Relaxed);
            let result = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(result.unwrap_err(), "Commit message generation cancelled");
            worker.join().unwrap();
            server.join().unwrap();
        }
    }

    #[test]
    fn recognizes_only_loopback_servers_as_local() {
        for endpoint in [
            "http://localhost:11434/v1",
            " http://127.0.0.1:1234/v1/ ",
            "http://127.0.0.2:8080/v1",
            "http://[::1]:8080/v1",
        ] {
            assert!(Config {
                endpoint: endpoint.into(),
                ..Config::default()
            }
            .is_local());
        }
        for endpoint in [
            DEFAULT_ENDPOINT,
            "http://localhost.example/v1",
            "http://192.168.1.2/v1",
            "invalid",
        ] {
            assert!(!Config {
                endpoint: endpoint.into(),
                ..Config::default()
            }
            .is_local());
        }
    }

    #[test]
    fn local_generation_sends_patch_without_auth_and_handles_server_failures() {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpListener;

        for (status, body, expected) in [
            (
                200,
                r#"{"choices":[{"message":{"content":"  Update local generation\n"}}]}"#,
                Ok("Update local generation"),
            ),
            (
                404,
                r#"{"error":{"message":"model is not installed"}}"#,
                Err("model is not installed"),
            ),
            (
                200,
                r#"{"choices":[{"message":{"content":" "}}]}"#,
                Err("does not contain a commit message"),
            ),
            (200, "invalid json", Err("invalid JSON")),
            (302, "", Err("HTTP 302")),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let server = std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "No local LLM request received"
                            );
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("Cannot accept test request: {error}"),
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&mut socket);
                let mut headers = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                    headers.push_str(&line);
                }
                let mut request = vec![0; length];
                reader.read_exact(&mut request).unwrap();
                write!(socket, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                (headers, serde_json::from_slice::<Value>(&request).unwrap())
            });
            let config = Config {
                endpoint: format!("http://{address}/v1"),
                model: "installed-local-model".into(),
                store_api_key: false,
                ..Config::default()
            };
            let result = generate(&config, "diff --git a/file b/file", &AtomicBool::new(false));
            let (headers, request) = server.join().unwrap();
            assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
            assert!(!headers.to_ascii_lowercase().contains("authorization:"));
            assert_eq!(request["model"], "installed-local-model");
            assert_eq!(request["stream"], false);
            assert!(request["messages"][1]["content"]
                .as_str()
                .unwrap()
                .contains("diff --git a/file b/file"));
            match expected {
                Ok(message) => assert_eq!(result.unwrap(), message),
                Err(detail) => assert!(result.unwrap_err().contains(detail)),
            }
        }
    }

    #[test]
    fn parses_openai_compatible_response() {
        let value = json!({"choices": [{"message": {"content": "  Add LLM commit messages\n"}}]});
        assert_eq!(parse_message(&value).unwrap(), "Add LLM commit messages");
        assert!(parse_message(&json!({"choices": []})).is_err());
    }

    #[test]
    fn accepts_base_urls_and_full_chat_completions_endpoints() {
        for (input, expected) in [
            (
                "https://api.kimi.ai/coding/v1",
                "https://api.kimi.ai/coding/v1/chat/completions",
            ),
            (
                "https://api.kimi.ai/coding/v1/",
                "https://api.kimi.ai/coding/v1/chat/completions",
            ),
            (
                " https://api.kimi.ai/coding/v1/chat/completions ",
                "https://api.kimi.ai/coding/v1/chat/completions",
            ),
            (DEFAULT_ENDPOINT, DEFAULT_ENDPOINT),
        ] {
            assert_eq!(chat_completions_endpoint(input), expected);
        }
    }

    #[test]
    fn provider_presets_have_unique_endpoints_and_models() {
        let mut endpoints = std::collections::HashSet::new();
        for provider in PROVIDER_PRESETS {
            assert!(!provider.name.is_empty());
            assert!(!provider.model.is_empty() || provider.name.ends_with("(local)"));
            assert!(provider.endpoint.ends_with("/chat/completions"));
            assert!(endpoints.insert(provider.endpoint));
        }
        assert_eq!(PROVIDER_PRESETS[0].endpoint, DEFAULT_ENDPOINT);
        assert_eq!(PROVIDER_PRESETS[0].model, DEFAULT_MODEL);
    }

    #[test]
    fn api_key_credentials_are_scoped_to_normalized_endpoints() {
        assert_eq!(
            credential_user(" https://api.kimi.ai/coding/v1/ ", None),
            credential_user("https://api.kimi.ai/coding/v1/chat/completions", None)
        );
        assert_ne!(
            credential_user(DEFAULT_ENDPOINT, None),
            credential_user("https://api.deepseek.com/v1/chat/completions", None)
        );
    }

    #[test]
    fn repository_credentials_are_isolated_from_global_and_other_repositories() {
        let global = credential_user(DEFAULT_ENDPOINT, None);
        let first = credential_user(DEFAULT_ENDPOINT, Some("/work/first"));
        let second = credential_user(DEFAULT_ENDPOINT, Some("/work/second"));
        assert_eq!(global, format!("llm-api-key:{DEFAULT_ENDPOINT}"));
        assert_ne!(global, first);
        assert_ne!(first, second);
        assert_eq!(
            first,
            credential_user("https://api.openai.com/v1/", Some("/work/first"))
        );
        assert_ne!(
            first,
            credential_user("https://other.example/v1", Some("/work/first"))
        );
    }

    #[test]
    fn protected_key_does_not_read_the_login_keychain() {
        let result = load_api_key_from_stores(
            || Ok(Some("protected-key".into())),
            || panic!("must not read the login keychain after Touch ID"),
            || panic!("must not migrate after Touch ID"),
        );
        assert_eq!(result, Ok(Some("protected-key".into())));
    }

    #[test]
    fn protected_read_failures_never_fall_back_to_password_authentication() {
        for error in [
            "Touch ID cancelled",
            "missing entitlement",
            "authentication failed",
        ] {
            let result = load_api_key_from_stores(
                || Err(error.into()),
                || panic!("must not ask for the login keychain password"),
                || panic!("must not migrate after a protected read error"),
            );
            assert_eq!(result, Err(error.into()));
        }
    }

    #[test]
    fn migration_only_runs_when_the_protected_key_is_absent() {
        let calls = std::cell::RefCell::new(Vec::new());
        let result = load_api_key_from_stores(
            || {
                calls.borrow_mut().push("protected");
                Ok(None)
            },
            || {
                calls.borrow_mut().push("scoped");
                Ok(None)
            },
            || {
                calls.borrow_mut().push("legacy");
                Ok(Some("migrated-key".into()))
            },
        );
        assert_eq!(result, Ok(Some("migrated-key".into())));
        assert_eq!(*calls.borrow(), ["protected", "scoped", "legacy"]);
    }

    #[test]
    fn scoped_migration_success_or_failure_does_not_read_an_unrelated_key() {
        for result in [
            Ok(Some("scoped-key".into())),
            Err("migration cancelled".into()),
        ] {
            let expected = result.clone();
            assert_eq!(
                load_api_key_from_stores(
                    || Ok(None),
                    || result,
                    || panic!("must not read a different key after scoped migration"),
                ),
                expected,
            );
        }
    }

    #[test]
    fn migration_reads_once_and_saves_before_deleting() {
        let calls = std::cell::RefCell::new(Vec::new());
        let secret = load_and_migrate_legacy_api_key(
            || {
                calls.borrow_mut().push("read");
                Ok(Some("legacy-key".into()))
            },
            |secret| {
                assert_eq!(secret, "legacy-key");
                calls.borrow_mut().push("save");
                Ok(())
            },
            || {
                calls.borrow_mut().push("delete");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(secret.as_deref(), Some("legacy-key"));
        assert_eq!(*calls.borrow(), ["read", "save", "delete"]);
    }

    #[test]
    fn missing_or_cancelled_legacy_read_does_not_migrate() {
        for result in [Ok(None), Err("authentication cancelled".into())] {
            let expected = result.clone();
            assert_eq!(
                load_and_migrate_legacy_api_key(
                    || result,
                    |_| panic!("must not save without a secret"),
                    || panic!("must not delete without a secret"),
                ),
                expected,
            );
        }
    }

    #[test]
    fn failed_migration_preserves_the_source() {
        let result = load_and_migrate_legacy_api_key(
            || Ok(Some("legacy-key".into())),
            |_| Err("storage unavailable".into()),
            || panic!("must not delete after a failed save"),
        );
        assert_eq!(result, Err("storage unavailable".into()));
    }

    #[test]
    fn migration_reports_cleanup_failure_without_reading_again() {
        let result = load_and_migrate_legacy_api_key(
            || Ok(Some("legacy-key".into())),
            |_| Ok(()),
            || Err("cannot remove source".into()),
        );
        assert_eq!(result, Err("cannot remove source".into()));
    }

    #[test]
    fn saved_credentials_restore_the_model_and_key_together() {
        let secret = serde_json::to_string(&SavedCredential {
            model: "custom-model".into(),
            api_key: "test-key".into(),
        })
        .unwrap();
        let mut config = Config::default();
        restore_credential(&mut config, secret).unwrap();
        assert_eq!(config.model, "custom-model");
        assert_eq!(config.api_key, "test-key");
    }

    #[test]
    fn legacy_keys_keep_the_configured_model() {
        let mut config = Config {
            model: "existing-model".into(),
            ..Config::default()
        };
        restore_credential(&mut config, "legacy-key".into()).unwrap();
        assert_eq!(config.model, "existing-model");
        assert_eq!(config.api_key, "legacy-key");
    }

    #[test]
    fn invalid_saved_credentials_do_not_change_the_configuration_or_expose_secrets() {
        let mut config = Config::default();
        let original = config.clone();
        let error = restore_credential(&mut config, r#"{"api_key":"secret"}"#.into()).unwrap_err();
        assert_eq!(config, original);
        assert!(!error.contains("secret"));
    }

    #[test]
    fn explicitly_provided_api_key_is_not_replaced() {
        let mut config = Config {
            api_key: "from-environment".into(),
            ..Config::default()
        };
        load_api_key_if_needed(&mut config, None).unwrap();
        assert_eq!(config.api_key, "from-environment");

        config.api_key.clear();
        config.store_api_key = false;
        load_api_key_if_needed(&mut config, None).unwrap();
        assert!(config.api_key.is_empty());
    }

    #[test]
    fn omits_temperature_from_generation_requests() {
        let body = request_body(&Config::default(), "diff --git a/file b/file");
        assert!(body.get("temperature").is_none());
        assert_eq!(body.get("model"), Some(&json!(DEFAULT_MODEL)));
    }

    #[test]
    fn includes_api_error_details_in_failures() {
        assert_eq!(
            api_error(
                400,
                br#"{"error":{"message":"temperature must be omitted"}}"#
            ),
            "LLM API returned HTTP 400: temperature must be omitted"
        );
        assert_eq!(
            api_error(429, b"  rate limit exceeded\n"),
            "LLM API returned HTTP 429: rate limit exceeded"
        );
        assert_eq!(
            api_error(500, b""),
            "LLM API returned HTTP 500. Check the endpoint, model, API key and provider limits."
        );
        let truncated = api_error(400, "x".repeat(2001).as_bytes());
        assert!(truncated.ends_with('…'));
        assert_eq!(
            truncated.chars().count(),
            "LLM API returned HTTP 400: ".chars().count() + 2001
        );
    }

    #[test]
    fn validates_configuration_without_exposing_key() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());
        config.api_key = "secret".into();
        assert!(config.validate().is_ok());
        let saved = serde_json::to_string(&config).unwrap();
        assert!(!saved.contains("secret"));
        let saved: Value = serde_json::from_str(&saved).unwrap();
        assert!(saved.get("api_key").is_none());
        assert_eq!(saved.get("store_api_key"), Some(&Value::Bool(true)));
    }
}
