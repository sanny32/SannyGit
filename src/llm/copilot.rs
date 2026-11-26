use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    generation_user_content, token_usage_from_json, validate_generated_message, Config,
    GenerationResult,
};

mod oauth;
pub use oauth::{can_sign_in, has_saved_session, sign_in, sign_out};

const TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const DEFAULT_CHAT_URL: &str = "https://api.githubcopilot.com/chat/completions";
const MODEL: &str = "gpt-4.1";
/// Identity the Copilot chat endpoint accepts for an app OAuth token.
const CLIENT_IDENTITY: &str = "copilot/1.0.114";
const SIGN_IN_AGAIN: &str = "Sign in to GitHub Copilot again in Settings → AI.";

pub async fn generate(
    config: &Config,
    patch: &str,
    branch: Option<&str>,
    cancel: &AtomicBool,
) -> Result<GenerationResult, String> {
    let github_token = oauth::access_token().await?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Commit message generation cancelled".into());
    }
    let request = async {
        let session = copilot_session(&github_token).await?;
        if cancel.load(Ordering::Relaxed) {
            return Err("Commit message generation cancelled".into());
        }
        let client = api_client()?;
        let response = client
            .post(&session.chat_url)
            .bearer_auth(&session.token)
            .headers(identity_headers(true))
            .json(&request_body(config, patch, branch))
            .send()
            .await
            .map_err(|error| format!("GitHub Copilot request failed: {}", error.without_url()))?;
        parse_reply(
            &super::read_api_response(response)
                .await
                .map_err(copilot_api_error)?,
        )
    };
    let mut request = std::pin::pin!(tokio::time::timeout(Duration::from_secs(60), request));
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Commit message generation cancelled".into());
        }
        if let Ok(result) = tokio::time::timeout(Duration::from_millis(50), &mut request).await {
            return result.unwrap_or_else(|_| Err("GitHub Copilot timed out.".into()));
        }
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
    endpoints: Option<TokenEndpoints>,
}

#[derive(Deserialize)]
struct TokenEndpoints {
    api: Option<String>,
}

struct CopilotSession {
    token: String,
    chat_url: String,
}

fn identity_headers(chat: bool) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let mut insert = |name: &'static str, value: &'static str| {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    };
    insert("user-agent", CLIENT_IDENTITY);
    insert("editor-version", CLIENT_IDENTITY);
    insert("copilot-integration-id", "copilot-developer-cli");
    insert("copilot-harness-id", "copilot-sdk");
    if chat {
        insert("openai-intent", "conversation-agent");
        insert("x-initiator", "user");
        insert("x-github-api-version", "2026-08-01");
    }
    headers
}

fn direct_session(github_token: &str) -> CopilotSession {
    CopilotSession {
        token: github_token.to_owned(),
        chat_url: DEFAULT_CHAT_URL.to_owned(),
    }
}

async fn copilot_session(github_token: &str) -> Result<CopilotSession, String> {
    // This app's OAuth token cannot mint a Copilot session token. Individual
    // plans answer that exchange with 403, while the chat endpoint accepts the
    // same token when the request carries the Copilot CLI identity.
    let response = api_client()?
        .get(TOKEN_URL)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("token {github_token}"),
        )
        .header(reqwest::header::ACCEPT, "application/json")
        .headers(identity_headers(false))
        .send()
        .await
        .map_err(|_| "Cannot reach GitHub Copilot. Check your connection.".to_string())?;
    if !response.status().is_success() {
        return Ok(direct_session(github_token));
    }
    let body: TokenResponse = response
        .json()
        .await
        .map_err(|_| "GitHub Copilot returned an invalid session.".to_string())?;
    if body.token.trim().is_empty() {
        return Err("GitHub Copilot returned an invalid session.".into());
    }
    let chat_url = chat_url(body.endpoints.and_then(|endpoints| endpoints.api))?;
    Ok(CopilotSession {
        token: body.token,
        chat_url,
    })
}

fn chat_url(api: Option<String>) -> Result<String, String> {
    let Some(api) = api.filter(|api| !api.trim().is_empty()) else {
        return Ok(DEFAULT_CHAT_URL.to_owned());
    };
    let mut url = reqwest::Url::parse(api.trim())
        .map_err(|_| "GitHub Copilot returned an unsupported API endpoint.".to_owned())?;
    let host = url.host_str().unwrap_or_default();
    let allowed = host == "githubcopilot.com" || host.ends_with(".githubcopilot.com");
    if url.scheme() != "https"
        || !allowed
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("GitHub Copilot returned an unsupported API endpoint.".into());
    }
    let path = url.path().trim_end_matches('/');
    let path = if path.is_empty() || path == "/" {
        "/chat/completions".to_owned()
    } else {
        format!("{path}/chat/completions")
    };
    url.set_path(&path);
    Ok(url.into())
}

fn request_body(config: &Config, patch: &str, branch: Option<&str>) -> Value {
    json!({
        "model": MODEL,
        "stream": false,
        "messages": [
            { "role": "system", "content": config.instructions.trim() },
            { "role": "user", "content": generation_user_content(config, patch, branch) }
        ]
    })
}

fn parse_reply(value: &Value) -> Result<GenerationResult, String> {
    let message = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .ok_or("GitHub Copilot returned no commit message.")?;
    Ok(GenerationResult {
        message: validate_generated_message(message)?,
        usage: value.get("usage").and_then(token_usage_from_json),
    })
}

fn copilot_api_error(error: String) -> String {
    if error.starts_with("LLM API returned HTTP 401")
        || error.starts_with("LLM API returned HTTP 403")
    {
        SIGN_IN_AGAIN.to_owned()
    } else {
        "GitHub Copilot could not generate a message. Check your connection and Copilot access."
            .into()
    }
}

fn api_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|_| "Cannot initialize the GitHub Copilot connection.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_url_uses_the_public_endpoint_or_the_returned_api_host() {
        assert_eq!(chat_url(None).unwrap(), DEFAULT_CHAT_URL);
        assert_eq!(
            chat_url(Some("https://api.individual.githubcopilot.com/".into())).unwrap(),
            "https://api.individual.githubcopilot.com/chat/completions"
        );
        assert_eq!(
            chat_url(Some("https://api.business.githubcopilot.com/v1".into())).unwrap(),
            "https://api.business.githubcopilot.com/v1/chat/completions"
        );
        assert!(chat_url(Some("http://api.githubcopilot.com".into())).is_err());
        assert!(chat_url(Some("https://evil.example/chat".into())).is_err());
        assert!(chat_url(Some("https://user:secret@api.githubcopilot.com".into())).is_err());
    }

    #[test]
    fn chat_headers_use_the_copilot_client_identity() {
        let exchange = identity_headers(false);
        let chat = identity_headers(true);
        assert_eq!(exchange["copilot-integration-id"], "copilot-developer-cli");
        assert!(exchange.get("x-github-api-version").is_none());
        assert_eq!(chat["editor-version"], CLIENT_IDENTITY);
        assert_eq!(chat["copilot-harness-id"], "copilot-sdk");
        assert_eq!(chat["x-github-api-version"], "2026-08-01");
        assert_eq!(chat["x-initiator"], "user");
    }

    #[test]
    fn direct_session_keeps_the_github_token_on_the_public_chat_endpoint() {
        let session = direct_session("gho_example");
        assert_eq!(session.token, "gho_example");
        assert_eq!(session.chat_url, DEFAULT_CHAT_URL);
    }

    #[test]
    fn request_sends_instructions_and_the_diff_without_streaming() {
        let config = Config {
            instructions: "Write a subject.".into(),
            ..Config::default()
        };
        let body = request_body(&config, "diff", Some("main"));
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["content"], "Write a subject.");
        assert!(body["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("diff"));
    }
}
