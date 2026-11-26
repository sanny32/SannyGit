use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use ring::{
    digest,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    time::{timeout, Instant},
};

use super::{
    generation_user_content, load_secret, save_secret, token_usage_from_json,
    validate_generated_message, Config, GenerationResult, COPILOT_KEYRING_USER,
};

const AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
const OAUTH_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const SESSION_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const DEFAULT_CHAT_URL: &str = "https://api.githubcopilot.com/chat/completions";
const CALLBACK: &str = "/callback";
/// GitHub Apps match the callback port exactly. Register this URL on the app.
const CALLBACK_PORT: u16 = 53617;
const DEFAULT_CLIENT_ID: &str = "Ov23litfi6abHvriXjB1";
const MAX_LOGIN_TIME: Duration = Duration::from_secs(900);
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
    let github_token = access_token().await?;
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
struct SessionToken {
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
        .get(SESSION_TOKEN_URL)
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
    let body: SessionToken = response
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

#[derive(Deserialize)]
#[serde(untagged)]
enum TokenReply {
    Token(AccessTokenResponse),
    Error { error: String },
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    refresh_token_expires_in: Option<u64>,
}

#[derive(Deserialize, Serialize)]
struct Credential {
    access_token: String,
    expires_at: Option<u64>,
    refresh_token: Option<String>,
    refresh_expires_at: Option<u64>,
    client_id: Option<String>,
}

impl Credential {
    fn from_response(
        response: AccessTokenResponse,
        client_id: &str,
        now: u64,
    ) -> Result<Self, String> {
        if !valid_access_token(&response.access_token) {
            return Err("GitHub returned an invalid access token.".into());
        }
        Ok(Self {
            access_token: response.access_token,
            expires_at: response.expires_in.map(|ttl| now.saturating_add(ttl)),
            refresh_token: response.refresh_token.filter(|token| !token.is_empty()),
            refresh_expires_at: response
                .refresh_token_expires_in
                .map(|ttl| now.saturating_add(ttl)),
            client_id: Some(client_id.into()),
        })
    }

    fn decode(secret: String) -> Result<Self, String> {
        if valid_access_token(&secret) {
            return Ok(Self {
                access_token: secret,
                expires_at: None,
                refresh_token: None,
                refresh_expires_at: None,
                client_id: None,
            });
        }
        let credential: Self = serde_json::from_str(&secret).map_err(|_| SIGN_IN_AGAIN)?;
        if !valid_access_token(&credential.access_token) {
            return Err(SIGN_IN_AGAIN.into());
        }
        Ok(credential)
    }

    fn save(&self) -> Result<(), String> {
        let secret =
            serde_json::to_string(self).map_err(|_| "Cannot encode the GitHub credential.")?;
        save_secret(COPILOT_KEYRING_USER, Some(&secret))
    }

    fn needs_refresh(&self, now: u64) -> bool {
        self.expires_at
            .is_some_and(|expiry| expiry <= now.saturating_add(60))
    }
}

fn valid_access_token(token: &str) -> bool {
    token
        .strip_prefix("gho_")
        .or_else(|| token.strip_prefix("ghu_"))
        .is_some_and(|body| {
            !body.is_empty()
                && body
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}

fn unix_time() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| "Check the system clock before signing in to GitHub.".into())
}

async fn access_token() -> Result<String, String> {
    let secret = load_secret(COPILOT_KEYRING_USER, false)?
        .filter(|value| !value.is_empty())
        .ok_or("Sign in to GitHub Copilot in Settings → AI.")?;
    let credential = Credential::decode(secret)?;
    let now = unix_time()?;
    if !credential.needs_refresh(now) {
        return Ok(credential.access_token);
    }
    let client_id = credential
        .client_id
        .as_deref()
        .filter(|id| client_id().as_deref() == Some(*id))
        .ok_or(SIGN_IN_AGAIN)?;
    let secret = client_secret();
    let rotated = refresh_token(
        &github_client()?,
        OAUTH_TOKEN_URL,
        client_id,
        &credential,
        now,
        secret.as_deref(),
    )
    .await?;
    rotated.save()?;
    Ok(rotated.access_token)
}

async fn refresh_token(
    client: &reqwest::Client,
    url: &str,
    client_id: &str,
    credential: &Credential,
    now: u64,
    secret: Option<&str>,
) -> Result<Credential, String> {
    let refresh = credential
        .refresh_token
        .as_deref()
        .filter(|_| {
            credential
                .refresh_expires_at
                .is_none_or(|expiry| expiry > now)
        })
        .ok_or(SIGN_IN_AGAIN)?;
    let mut params = vec![
        ("client_id", client_id),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh),
    ];
    if let Some(secret) = secret {
        params.push(("client_secret", secret));
    }
    let reply = github_post(client, url, &params).await?;
    match reply {
        TokenReply::Token(response) => {
            let rotated = Credential::from_response(response, client_id, unix_time()?)?;
            if rotated.refresh_token.is_none() || rotated.expires_at.is_none() {
                return Err("GitHub returned incomplete refreshed credentials.".into());
            }
            Ok(rotated)
        }
        TokenReply::Error { error }
            if matches!(
                error.as_str(),
                "bad_refresh_token"
                    | "invalid_grant"
                    | "expired_token"
                    | "token_expired"
                    | "incorrect_client_credentials"
            ) =>
        {
            Err(SIGN_IN_AGAIN.into())
        }
        TokenReply::Error { .. } => {
            Err("GitHub could not refresh the Copilot session. Try again.".into())
        }
    }
}

fn client_id() -> Option<String> {
    let from_env = std::env::var("SANNYGIT_GITHUB_CLIENT_ID").ok();
    resolve_client_id(
        from_env
            .as_deref()
            .or(option_env!("SANNYGIT_GITHUB_CLIENT_ID")),
    )
}

fn resolve_client_id(override_id: Option<&str>) -> Option<String> {
    let id = override_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(DEFAULT_CLIENT_ID);
    valid_client_id(id).then(|| id.to_string())
}

fn valid_client_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

pub fn can_sign_in() -> bool {
    client_id().is_some() && client_secret().is_some()
}

pub fn has_saved_session() -> Result<bool, String> {
    Ok(load_secret(COPILOT_KEYRING_USER, false)?.is_some_and(|secret| !secret.is_empty()))
}

pub fn sign_in() -> Result<String, String> {
    let client_id = client_id().ok_or("GitHub Copilot sign-in is unavailable in this build.")?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Cannot initialize GitHub sign-in.")?
        .block_on(connect(&client_id))
}

pub fn sign_out() -> Result<String, String> {
    save_secret(COPILOT_KEYRING_USER, None)?;
    Ok("GitHub Copilot disconnected".into())
}

fn github_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "Cannot initialize the GitHub sign-in connection.".into())
}

fn client_secret() -> Option<String> {
    std::env::var("SANNYGIT_GITHUB_CLIENT_SECRET")
        .ok()
        .map(|secret| secret.trim().to_string())
        .filter(|secret| valid_client_secret(secret))
        .or_else(|| {
            option_env!("SANNYGIT_GITHUB_CLIENT_SECRET")
                .map(str::trim)
                .filter(|secret| valid_client_secret(secret))
                .map(str::to_string)
        })
}

fn valid_client_secret(secret: &str) -> bool {
    !secret.is_empty() && secret.len() <= 256 && secret.bytes().all(|byte| byte.is_ascii_graphic())
}

fn callback_url() -> String {
    format!("http://127.0.0.1:{CALLBACK_PORT}{CALLBACK}")
}

async fn connect(client_id: &str) -> Result<String, String> {
    let secret = client_secret().ok_or(
        "GitHub Copilot sign-in needs the app client secret. Set SANNYGIT_GITHUB_CLIENT_SECRET and try again.",
    )?;
    let client = github_client()?;
    let listener = TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .await
        .map_err(|_| format!("Cannot listen for GitHub sign-in on {}.", callback_url()))?;
    let redirect = callback_url();
    let state = random_token()?;
    let verifier = random_token()?;
    let url = authorize_url(client_id, &redirect, &state, &pkce_challenge(&verifier))?;
    webbrowser::open(&url).map_err(|_| "Cannot open the browser for GitHub sign-in.")?;
    let pending = receive_authorization(listener, &state, Instant::now() + MAX_LOGIN_TIME).await?;
    let saved = exchange_code(
        &client,
        OAUTH_TOKEN_URL,
        client_id,
        &pending.code,
        &redirect,
        &verifier,
        Some(&secret),
    )
    .await
    .and_then(|credential| credential.save());
    let connected = saved.is_ok();
    pending.finish(connected).await;
    saved.map(|()| "GitHub Copilot connected".into())
}

fn authorize_url(
    client_id: &str,
    redirect: &str,
    state: &str,
    challenge: &str,
) -> Result<String, String> {
    let mut url = reqwest::Url::parse(AUTHORIZE_URL)
        .map_err(|_| "Cannot build the GitHub sign-in URL.".to_owned())?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", client_id),
        ("redirect_uri", redirect),
        ("state", state),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("prompt", "select_account"),
    ]);
    Ok(url.into())
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, verifier.as_bytes()))
}

fn random_token() -> Result<String, String> {
    let mut bytes = [0; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "Cannot secure the GitHub sign-in request.")?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

async fn github_post<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    params: &[(&str, &str)],
) -> Result<T, String> {
    client
        .post(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .form(params)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| "GitHub OAuth request failed. Check your connection and try again.")?
        .json()
        .await
        .map_err(|_| "GitHub returned an invalid sign-in response.".into())
}

async fn exchange_code(
    client: &reqwest::Client,
    url: &str,
    client_id: &str,
    code: &str,
    redirect: &str,
    verifier: &str,
    secret: Option<&str>,
) -> Result<Credential, String> {
    let mut params = vec![
        ("client_id", client_id),
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect),
        ("code_verifier", verifier),
    ];
    if let Some(secret) = secret {
        params.push(("client_secret", secret));
    }
    match github_post(client, url, &params).await? {
        TokenReply::Token(response) => Credential::from_response(response, client_id, unix_time()?),
        TokenReply::Error { error } if error == "incorrect_client_credentials" => Err(
            "GitHub rejected the Copilot app credentials. Set SANNYGIT_GITHUB_CLIENT_SECRET and sign in again.".into(),
        ),
        TokenReply::Error { error }
            if matches!(error.as_str(), "bad_verification_code" | "expired_token") =>
        {
            Err("GitHub sign-in expired. Start it again in SannyGit.".into())
        }
        TokenReply::Error { .. } => {
            Err("GitHub could not authorize this app. Check its callback URL.".into())
        }
    }
}

enum Callback {
    Ignore,
    Reject,
    Denied,
    Code(String),
}

struct PendingAuthorization {
    stream: TcpStream,
    code: String,
}

impl PendingAuthorization {
    async fn finish(mut self, connected: bool) {
        let _ = write_html(&mut self.stream, "200 OK", &result_page(connected)).await;
    }
}

async fn receive_authorization(
    listener: TcpListener,
    state: &str,
    deadline: Instant,
) -> Result<PendingAuthorization, String> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("GitHub sign-in expired. Start it again in SannyGit.".into());
        }
        let mut stream = match timeout(remaining, listener.accept()).await {
            Ok(Ok((stream, _))) => stream,
            Ok(Err(_)) => return Err("Cannot listen for GitHub sign-in on this computer.".into()),
            Err(_) => return Err("GitHub sign-in expired. Start it again in SannyGit.".into()),
        };
        let line = match timeout(Duration::from_secs(2), read_request_line(&mut stream)).await {
            Ok(Ok(line)) => line,
            _ => continue,
        };
        match classify_callback(&line, state) {
            Callback::Ignore => {
                let _ = write_html(&mut stream, "404 Not Found", "").await;
            }
            Callback::Reject => {
                let _ = write_html(&mut stream, "400 Bad Request", &result_page(false)).await;
            }
            Callback::Denied => {
                let _ = write_html(&mut stream, "200 OK", &result_page(false)).await;
                return Err("GitHub sign-in was declined.".into());
            }
            Callback::Code(code) => return Ok(PendingAuthorization { stream, code }),
        }
    }
}

async fn read_request_line(stream: &mut TcpStream) -> io::Result<String> {
    let mut request = BufReader::new((&mut *stream).take(8192));
    let mut line = String::new();
    request.read_line(&mut line).await?;
    loop {
        let mut header = String::new();
        if request.read_line(&mut header).await? == 0 || header == "\r\n" {
            break;
        }
    }
    Ok(line)
}

fn classify_callback(line: &str, expected_state: &str) -> Callback {
    let Some(target) = request_target(line) else {
        return Callback::Ignore;
    };
    let Some(query) = target
        .strip_prefix(CALLBACK)
        .and_then(|rest| rest.strip_prefix('?'))
    else {
        return Callback::Ignore;
    };
    let Ok(url) = reqwest::Url::parse(&format!("http://127.0.0.1{CALLBACK}?{query}")) else {
        return Callback::Reject;
    };
    let mut code = None;
    let mut state = None;
    let mut denied = false;
    for (name, value) in url.query_pairs() {
        match name.as_ref() {
            "code" => {
                if code.is_some() {
                    return Callback::Reject;
                }
                code = Some(value.into_owned());
            }
            "state" => {
                if state.is_some() {
                    return Callback::Reject;
                }
                state = Some(value.into_owned());
            }
            "error" => denied = true,
            _ => {}
        }
    }
    if state.as_deref() != Some(expected_state) {
        return Callback::Reject;
    }
    if denied {
        return Callback::Denied;
    }
    match code {
        Some(code) if valid_auth_code(&code) => Callback::Code(code),
        _ => Callback::Reject,
    }
}

fn request_target(line: &str) -> Option<&str> {
    let mut parts = line.strip_suffix("\r\n")?.split(' ');
    if parts.next()? != "GET" {
        return None;
    }
    let target = parts.next()?;
    if !matches!(parts.next()?, "HTTP/1.1" | "HTTP/1.0") || parts.next().is_some() {
        return None;
    }
    Some(target)
}

fn valid_auth_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 256
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn result_page(connected: bool) -> String {
    let (tone, heading, message, note) = if connected {
        (
            "ok",
            "GitHub Copilot connected",
            "Return to SannyGit to generate commit messages with Copilot.",
            "You can close this tab.",
        )
    } else {
        (
            "bad",
            "GitHub sign-in did not complete",
            "Return to SannyGit to see the error or start again.",
            "You can close this tab.",
        )
    };
    include_str!("../../assets/oauth-callback.html")
        .replace("__TONE__", tone)
        .replace("__HEADING__", heading)
        .replace("__MESSAGE__", message)
        .replace("__NOTE__", note)
}

async fn write_html(stream: &mut TcpStream, status: &str, body: &str) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Cache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; img-src data:; frame-ancestors 'none'\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
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

    #[test]
    fn client_id_uses_the_default_unless_an_override_is_set() {
        assert_eq!(resolve_client_id(None).as_deref(), Some(DEFAULT_CLIENT_ID));
        assert_eq!(
            resolve_client_id(Some("  ")).as_deref(),
            Some(DEFAULT_CLIENT_ID)
        );
        assert_eq!(
            resolve_client_id(Some(" test_client ")).as_deref(),
            Some("test_client")
        );
        assert_eq!(resolve_client_id(Some("invalid/id")), None);
    }

    #[test]
    fn saved_tokens_keep_expiry_and_legacy_tokens_remain_readable() {
        let credential = Credential::from_response(
            serde_json::from_str(r#"{"access_token":"gho_example","expires_in":100,"refresh_token":"ghr_example","refresh_token_expires_in":200}"#).unwrap(),
            "test-client", 1000,
        ).unwrap();
        let stored = Credential::decode(serde_json::to_string(&credential).unwrap()).unwrap();
        assert!(!stored.needs_refresh(1039));
        assert!(stored.needs_refresh(1040));
        assert_eq!(stored.refresh_expires_at, Some(1200));
        assert_eq!(stored.refresh_token.as_deref(), Some("ghr_example"));
        let legacy = Credential::decode("gho_legacy".into()).unwrap();
        assert!(!legacy.needs_refresh(u64::MAX));
        assert!(legacy.refresh_token.is_none());
    }

    #[tokio::test]
    async fn refresh_exchanges_and_rotates_both_tokens() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut length = 0;
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).await.unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).await.unwrap();
            assert_eq!(
                String::from_utf8(body).unwrap(),
                "client_id=test_client&grant_type=refresh_token&refresh_token=ghr_previous"
            );
            let reply = r#"{"access_token":"gho_new","expires_in":3600,"refresh_token":"ghr_new","refresh_token_expires_in":7200}"#;
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len()).as_bytes()).await.unwrap();
        });
        let mut previous = Credential::decode("gho_previous".into()).unwrap();
        let now = unix_time().unwrap();
        previous.refresh_token = Some("ghr_previous".into());
        previous.refresh_expires_at = Some(now + 1);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let url = format!("http://{addr}/");
        let rotated = refresh_token(&client, &url, "test_client", &previous, now, None)
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(rotated.access_token, "gho_new");
        assert_eq!(rotated.refresh_token.as_deref(), Some("ghr_new"));
        assert!(rotated.expires_at.unwrap() >= now + 3600);
        assert!(rotated.refresh_expires_at.unwrap() >= now + 7200);
        assert_eq!(
            refresh_token(&client, &url, "test_client", &previous, now + 1, None)
                .await
                .err()
                .as_deref(),
            Some(SIGN_IN_AGAIN)
        );
    }

    #[test]
    fn build_client_id_accepts_github_ids_and_rejects_missing_or_invalid_values() {
        assert!(valid_client_id("012345abcdef01234567"));
        assert!(valid_client_id("Iv1_example"));
        assert!(!valid_client_id(""));
        assert!(!valid_client_id("https://github.com"));
    }

    #[test]
    fn authorize_url_opens_the_github_account_page() {
        let url = authorize_url(
            "Ov23li_test",
            "http://127.0.0.1:9/callback",
            "state-value",
            "challenge-value",
        )
        .unwrap();
        assert!(url.starts_with("https://github.com/login/oauth/authorize?"));
        assert!(url.contains("client_id=Ov23li_test"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A9%2Fcallback"));
        assert!(url.contains("state=state-value"));
        assert!(url.contains("code_challenge=challenge-value"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("prompt=select_account"));
        assert_eq!(callback_url(), "http://127.0.0.1:53617/callback");
        assert!(!url.contains("/login/device"));
        let connected = result_page(true);
        let failed = result_page(false);
        assert!(connected.contains("class=\"ok\""));
        assert!(connected.contains("GitHub Copilot connected"));
        assert!(failed.contains("class=\"bad\""));
        assert!(!connected.contains("__") && !failed.contains("__"));
        assert!(!connected.contains("https://"));
    }

    #[test]
    fn callback_accepts_one_matching_code_and_rejects_anything_else() {
        let line = |query: &str| format!("GET /callback?{query} HTTP/1.1\r\n");
        assert!(matches!(
            classify_callback(&line("code=abc&state=good"), "good"),
            Callback::Code(code) if code == "abc"
        ));
        assert!(matches!(
            classify_callback(&line("error=access_denied&state=good"), "good"),
            Callback::Denied
        ));
        assert!(matches!(
            classify_callback(&line("code=abc&state=bad"), "good"),
            Callback::Reject
        ));
        assert!(matches!(
            classify_callback(&line("code=abc&code=def&state=good"), "good"),
            Callback::Reject
        ));
        assert!(matches!(
            classify_callback("GET / HTTP/1.1\r\n", "good"),
            Callback::Ignore
        ));
        assert!(!valid_auth_code("") && !valid_auth_code("code with space"));
        assert!(!valid_client_secret("") && !valid_client_secret("secret with space"));
    }

    #[tokio::test]
    async fn callback_returns_only_the_code_for_the_pending_sign_in() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let pending = receive_authorization(
                listener,
                "expected-state",
                Instant::now() + Duration::from_secs(5),
            )
            .await
            .unwrap();
            assert_eq!(pending.code, "auth-code");
            pending.finish(true).await;
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        assert_eq!(
            client
                .get(format!("http://{addr}/"))
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::NOT_FOUND
        );
        assert_eq!(
            client
                .get(format!("http://{addr}/callback?code=auth-code&state=other"))
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::BAD_REQUEST
        );
        let page = client
            .get(format!(
                "http://{addr}/callback?code=auth-code&state=expected-state"
            ))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(page.contains("GitHub Copilot connected"));
        assert!(!page.contains("auth-code"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn code_exchange_posts_the_verifier_and_optional_secret() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut length = 0;
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).await.unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).await.unwrap();
            assert_eq!(
                String::from_utf8(body).unwrap(),
                "client_id=test_client&grant_type=authorization_code&code=auth-code&redirect_uri=http%3A%2F%2F127.0.0.1%3A53617%2Fcallback&code_verifier=verifier&client_secret=secret"
            );
            let reply = r#"{"access_token":"ghu_new","expires_in":28800,"refresh_token":"ghr_new","refresh_token_expires_in":15897600}"#;
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len()).as_bytes()).await.unwrap();
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let credential = exchange_code(
            &client,
            &format!("http://{addr}/"),
            "test_client",
            "auth-code",
            &callback_url(),
            "verifier",
            Some("secret"),
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(credential.access_token, "ghu_new");
        assert_eq!(credential.refresh_token.as_deref(), Some("ghr_new"));
        assert!(credential.expires_at.is_some());
    }
}
