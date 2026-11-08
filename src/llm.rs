use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

pub const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
pub const DEFAULT_MODEL: &str = "gpt-4.1-mini";
const MAX_DIFF_BYTES: usize = 256 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const KEYRING_SERVICE: &str = "SannyGit";
const KEYRING_USER: &str = "llm-api-key";

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

pub fn load_api_key(config: &Config) -> Result<Option<String>, String> {
    if !config.store_api_key {
        return Ok(None);
    }
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|error| format!("Cannot access the OS credential manager: {error}"))?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!(
            "Cannot read the LLM API key from the OS credential manager: {error}"
        )),
    }
}

pub fn save_api_key(config: &Config) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|error| format!("Cannot access the OS credential manager: {error}"))?;
    if config.store_api_key && !config.api_key.is_empty() {
        entry
            .set_password(&config.api_key)
            .map_err(|error| format!("Cannot save the LLM API key securely: {error}"))
    } else {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(format!("Cannot remove the saved LLM API key: {error}")),
        }
    }
}

pub fn credential_store_label() -> &'static str {
    #[cfg(target_os = "macos")]
    return "macOS Keychain";
    #[cfg(target_os = "windows")]
    return "Windows Credential Manager";
    #[cfg(target_os = "linux")]
    return "Secret Service";
    #[allow(unreachable_code)]
    "OS credential manager"
}

impl Config {
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

pub fn generate(config: &Config, patch: &str) -> Result<String, String> {
    config.validate()?;
    if patch.trim().is_empty() {
        return Err("The staged diff is empty".into());
    }
    if patch.len() > MAX_DIFF_BYTES {
        return Err(format!(
            "The staged diff exceeds the {} KiB LLM request limit",
            MAX_DIFF_BYTES / 1024
        ));
    }

    let request = request_body(config, patch);
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| format!("Cannot initialize the LLM client: {error}"))?;
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
        .map_err(|error| format!("LLM request failed: {error}"))?;
    let status = response.status();
    let mut response = std::io::Read::take(response, MAX_RESPONSE_BYTES as u64 + 1);
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut response, &mut bytes)
        .map_err(|error| format!("Cannot read the LLM response: {error}"))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err("LLM response exceeds the 1 MiB limit".into());
    }
    if !status.is_success() {
        return Err(api_error(status.as_u16(), &bytes));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("LLM API returned invalid JSON: {error}"))?;
    parse_message(&value)
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
        "messages": [
            { "role": "system", "content": config.instructions.trim() },
            { "role": "user", "content": format!("Generate a commit message for this staged diff:\n\n{patch}") }
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
