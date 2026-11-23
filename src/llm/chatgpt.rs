use super::{read_response_body, Config};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use reqwest::{Client, RequestBuilder, Url};
use ring::{digest, rand, signature};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::Mutex,
    time::{Duration, Instant},
};

const ISSUER: &str = "https://auth.openai.com";
const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
const RESOURCE: &str = "https://api.openai.com/v1";
const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const HOST: &str = "chatgpt-oauth-host";
const CONNECTION: &str = "chatgpt-oauth-session";
const ACCESS: &str = "chatgpt-oauth-access";
const CALLBACK: &str = "/auth/callback";

static SESSION: Mutex<Option<Access>> = Mutex::new(None);

#[derive(Clone, Serialize, Deserialize)]
struct Access {
    grant: String,
    token: String,
    expires: i64,
}

fn lock_session() -> Result<File, String> {
    let directory = eframe::storage_dir("SannyGit").ok_or("Cannot locate SannyGit settings")?;
    fs::create_dir_all(&directory).map_err(|_| "Cannot prepare the ChatGPT session directory")?;
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("chatgpt-session.lock"))
        .map_err(|_| "Cannot open the ChatGPT session lock")?;
    file.try_lock()
        .map_err(|_| "Another SannyGit window is using the ChatGPT session. Try again shortly.")?;
    Ok(file)
}

#[derive(Serialize, Deserialize)]
struct Connection {
    client_id: String,
    subject: String,
    refresh_token: String,
}

impl Connection {
    fn grant(&self) -> String {
        URL_SAFE_NO_PAD.encode(digest::digest(
            &digest::SHA256,
            self.refresh_token.as_bytes(),
        ))
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    id_token: String,
    scope: Option<String>,
    token_type: String,
    expires_in: u32,
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Cannot initialize the ChatGPT connection".into())
}

fn client() -> Result<Client, String> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|_| "Cannot initialize the ChatGPT HTTP client".into())
}

fn random<const N: usize>() -> Result<[u8; N], String> {
    use ring::rand::SecureRandom;
    let mut bytes = [0; N];
    rand::SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "Cannot generate secure random data for ChatGPT sign-in")?;
    Ok(bytes)
}

fn host_id() -> Result<String, String> {
    if let Some(host) = load_secret(HOST)? {
        return Ok(host);
    }
    let mut bytes = random::<16>()?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let value = format!("{:032x}", u128::from_be_bytes(bytes));
    let host = format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &value[..8],
        &value[8..12],
        &value[12..16],
        &value[16..20],
        &value[20..],
    );
    save_secret(HOST, &host)?;
    Ok(host)
}

fn load_connection() -> Result<Option<Connection>, String> {
    load_secret(CONNECTION)?
        .map(|data| serde_json::from_str(&data).map_err(|_| "Invalid saved ChatGPT session".into()))
        .transpose()
}

pub fn has_saved_session() -> Result<bool, String> {
    load_connection().map(|connection| connection.is_some())
}

fn save_connection(connection: &Connection) -> Result<(), String> {
    let data =
        serde_json::to_string(connection).map_err(|_| "Cannot encode the ChatGPT session")?;
    save_secret(CONNECTION, &data)
}

#[cfg(target_os = "macos")]
fn load_secret(key: &str) -> Result<Option<String>, String> {
    super::macos_credentials::load_unprotected(key)
        .map_err(|_| "Cannot read ChatGPT credentials from Keychain".into())
}

#[cfg(target_os = "macos")]
fn save_secret(key: &str, value: &str) -> Result<(), String> {
    security_framework::passwords::set_generic_password(
        super::KEYRING_SERVICE,
        key,
        value.as_bytes(),
    )
    .map_err(|_| "Cannot save ChatGPT credentials in Keychain".into())
}

#[cfg(target_os = "macos")]
fn delete_secret(key: &str) -> Result<(), String> {
    super::macos_credentials::delete_unprotected(key)
        .map_err(|_| "Cannot remove the ChatGPT session from Keychain".into())
}

#[cfg(not(target_os = "macos"))]
fn load_secret(key: &str) -> Result<Option<String>, String> {
    match super::credential_entry(key)?.get_secret() {
        Ok(secret) => String::from_utf8(secret)
            .map(Some)
            .map_err(|_| "Invalid saved ChatGPT credentials".into()),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("Cannot read ChatGPT credentials from the OS credential manager".into()),
    }
}

#[cfg(not(target_os = "macos"))]
fn save_secret(key: &str, value: &str) -> Result<(), String> {
    super::credential_entry(key)?
        .set_secret(value.as_bytes())
        .map_err(|_| "Cannot save ChatGPT credentials in the OS credential manager".into())
}

#[cfg(not(target_os = "macos"))]
fn delete_secret(key: &str) -> Result<(), String> {
    match super::credential_entry(key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("Cannot remove the ChatGPT session from the OS credential manager".into()),
    }
}

pub fn sign_in() -> Result<String, String> {
    let mut session = SESSION
        .lock()
        .map_err(|_| "Cannot access the ChatGPT session")?;
    let _lock = lock_session()?;
    let previous = load_connection()?;
    let host = host_id()?;
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|_| "Cannot listen for ChatGPT sign-in on this computer")?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "Cannot initialize the sign-in listener")?;
    let port = listener
        .local_addr()
        .map_err(|_| "Cannot read the sign-in listener address")?
        .port();
    let redirect = format!("http://127.0.0.1:{port}{CALLBACK}");

    let state = URL_SAFE_NO_PAD.encode(random::<32>()?);
    let nonce = URL_SAFE_NO_PAD.encode(random::<32>()?);
    let verifier = URL_SAFE_NO_PAD.encode(random::<32>()?);
    let challenge = URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, verifier.as_bytes()));
    let mut url = Url::parse(&format!("{ISSUER}/api/accounts/authorize"))
        .map_err(|_| "Invalid ChatGPT authorization URL")?;
    let existing_id = previous.as_ref().map(|session| session.client_id.as_str());
    url.query_pairs_mut().extend_pairs([
        ("client_id", existing_id.unwrap_or("dynamic_agent_client")),
        ("ext_agent_host_id", &host),
        ("response_type", "code"),
        ("redirect_uri", &redirect),
        ("scope", SCOPES),
        ("resource", RESOURCE),
        ("state", &state),
        ("nonce", &nonce),
        ("code_challenge_method", "S256"),
        ("code_challenge", &challenge),
    ]);
    if existing_id.is_none() {
        url.query_pairs_mut()
            .append_pair("agent_name_hint", "SannyGit");
    }
    webbrowser::open(url.as_str()).map_err(|_| "Cannot open the browser for ChatGPT sign-in")?;
    let (code, client_id) = wait_for_callback(&listener, &state, existing_id)?;
    runtime()?.block_on(async {
        let client = client()?;
        let tokens = exchange(
            &client,
            &[
                ("grant_type", "authorization_code"),
                ("client_id", &client_id),
                ("code", &code),
                ("code_verifier", &verifier),
                ("redirect_uri", &redirect),
                ("resource", RESOURCE),
            ],
            false,
        )
        .await?;
        let jwks: Value = read_json(client.get(format!("{ISSUER}/.well-known/jwks.json"))).await?;
        let subject = validate_id_token(&tokens.id_token, &jwks, &client_id, &nonce)
            .ok_or("ChatGPT returned an invalid identity token")?;
        if previous
            .as_ref()
            .is_some_and(|previous| previous.subject != subject)
        {
            return Err(
                "ChatGPT returned a different account. Sign out before changing accounts.".into(),
            );
        }
        if tokens.refresh_token.is_empty() {
            return Err("ChatGPT did not return a renewable session".into());
        }
        let connection = Connection {
            client_id,
            subject,
            refresh_token: tokens.refresh_token.clone(),
        };
        save_connection(&connection)?;
        let access = tokens.into_access(connection.grant());
        access.cache();
        *session = Some(access);
        Ok("ChatGPT connected".into())
    })
}

fn wait_for_callback(
    listener: &TcpListener,
    state: &str,
    client: Option<&str>,
) -> Result<(String, String), String> {
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(_) => return Err("The ChatGPT sign-in listener stopped".into()),
        };
        if stream.set_nonblocking(false).is_err()
            || stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .is_err()
            || stream
                .set_write_timeout(Some(Duration::from_secs(1)))
                .is_err()
        {
            continue;
        }
        let mut line = String::new();
        if BufReader::new((&stream).take(8192))
            .read_line(&mut line)
            .is_err()
        {
            continue;
        }
        let result = parse_callback(&line, state, client);
        let success = result.is_some();
        let status = if success { "200 OK" } else { "400 Bad Request" };
        let page = callback_page(success);
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\n\
             Connection: close\r\n\
             Cache-Control: no-store\r\n\
             Referrer-Policy: no-referrer\r\n\
             X-Content-Type-Options: nosniff\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {page}",
            page.len()
        );
        if let Some(result) = result {
            return result;
        }
    }
    Err("ChatGPT sign-in timed out. Select Login to try again.".into())
}

fn callback_page(success: bool) -> String {
    let (heading, message, note, tone) = if success {
        (
            "Sign-in received",
            "Return to SannyGit to see the result of ChatGPT sign-in.",
            "You can close this tab.",
            "ok",
        )
    } else {
        (
            "Sign-in was not accepted",
            "This request does not match the pending sign-in.",
            "Return to SannyGit and try again.",
            "bad",
        )
    };
    include_str!("../../assets/chatgpt-callback.html")
        .replace("__TONE__", tone)
        .replace("__HEADING__", heading)
        .replace("__MESSAGE__", message)
        .replace("__NOTE__", note)
}

fn parse_callback(
    line: &str,
    state: &str,
    existing: Option<&str>,
) -> Option<Result<(String, String), String>> {
    let mut parts = line.strip_suffix("\r\n")?.split(' ');
    if parts.next()? != "GET" {
        return None;
    }
    let query = parts.next()?.strip_prefix(CALLBACK)?.strip_prefix('?')?;
    if !matches!(parts.next()?, "HTTP/1.1" | "HTTP/1.0") || parts.next().is_some() {
        return None;
    }
    let url = Url::parse(&format!("http://127.0.0.1{CALLBACK}?{query}")).ok()?;
    let mut params = HashMap::new();
    for (name, value) in url.query_pairs() {
        if params
            .insert(name.into_owned(), value.into_owned())
            .is_some()
        {
            return None;
        }
    }
    if params.get("state").map(String::as_str) != Some(state) {
        return None;
    }
    if params.contains_key("error") {
        return Some(Err(
            "ChatGPT sign-in was declined or could not be completed".into(),
        ));
    }
    let supplied = params.get("client_id").map(String::as_str);
    let client = match (existing, supplied) {
        (Some(existing), Some(supplied)) if existing != supplied => None,
        (Some(existing), _) => Some(existing),
        (None, Some(supplied)) if supplied.starts_with("oaiapp_") => Some(supplied),
        _ => None,
    };
    Some(match (params.get("code"), client) {
        (Some(code), Some(client)) if !code.is_empty() => Ok((code.clone(), client.into())),
        _ => Err("ChatGPT returned an incomplete or mismatched authorization".into()),
    })
}

fn decode_json(encoded: &str) -> Option<Value> {
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded).ok()?).ok()
}

fn validate_id_token(token: &str, jwks: &Value, client: &str, nonce: &str) -> Option<String> {
    let (signed, signature) = token.rsplit_once('.')?;
    let (header, claims) = signed.split_once('.')?;
    let header = decode_json(header)?;
    if header["alg"].as_str()? != "RS256" {
        return None;
    }
    let kid = header["kid"].as_str()?;
    let key = jwks["keys"]
        .as_array()?
        .iter()
        .find(|key| key["kid"].as_str() == Some(kid) && key["kty"].as_str() == Some("RSA"))?;
    let rsa = signature::RsaPublicKeyComponents {
        n: URL_SAFE_NO_PAD.decode(key["n"].as_str()?).ok()?,
        e: URL_SAFE_NO_PAD.decode(key["e"].as_str()?).ok()?,
    };
    rsa.verify(
        &signature::RSA_PKCS1_2048_8192_SHA256,
        signed.as_bytes(),
        &URL_SAFE_NO_PAD.decode(signature).ok()?,
    )
    .ok()?;
    let claims = decode_json(claims)?;
    if claims["iss"].as_str()? != ISSUER
        || claims["nonce"].as_str()? != nonce
        || claims["exp"].as_i64()? <= chrono::Utc::now().timestamp()
    {
        return None;
    }
    let audience = &claims["aud"];
    if audience.as_str() != Some(client)
        && !audience.as_array().is_some_and(|values| {
            values.iter().any(|value| value.as_str() == Some(client))
                && (values.len() == 1 || claims["azp"].as_str() == Some(client))
        })
    {
        return None;
    }
    claims["sub"]
        .as_str()
        .filter(|subject| !subject.is_empty())
        .map(str::to_owned)
}

async fn send(request: RequestBuilder) -> Result<reqwest::Response, String> {
    request
        .send()
        .await
        .map_err(|_| "Cannot reach ChatGPT. Check your network connection.".into())
}

async fn read_json<T: DeserializeOwned>(request: RequestBuilder) -> Result<T, String> {
    let mut response = send(request).await?;
    if !response.status().is_success() {
        return Err(http_error(response.status()));
    }
    serde_json::from_slice(&read_response_body(&mut response).await?)
        .map_err(|_| "ChatGPT returned an invalid response".into())
}

async fn exchange(
    client: &Client,
    form: &[(&str, &str)],
    refresh: bool,
) -> Result<TokenResponse, String> {
    let mut response = send(client.post(TOKEN_URL).form(form)).await?;
    let status = response.status();
    let body = read_response_body(&mut response).await?;
    if !status.is_success() {
        if serde_json::from_slice::<Value>(&body)
            .ok()
            .is_some_and(|value| value["error"] == "invalid_grant")
        {
            return Err(
                "The ChatGPT authorization expired or was revoked. Select Login to reconnect."
                    .into(),
            );
        }
        return Err(http_error(status));
    }
    let tokens: TokenResponse =
        serde_json::from_slice(&body).map_err(|_| "ChatGPT returned an invalid token response")?;
    let granted = tokens
        .scope
        .as_deref()
        .map(|scope| scope.split_whitespace().any(|item| item == PLAN_SCOPE));
    if !granted.unwrap_or(refresh) {
        return Err("ChatGPT plan access was not granted. Check your account, plan and workspace, then reconnect.".into());
    }
    if !tokens.token_type.eq_ignore_ascii_case("bearer")
        || tokens.access_token.is_empty()
        || tokens.expires_in == 0
    {
        return Err("ChatGPT returned an invalid access token".into());
    }
    Ok(tokens)
}

impl TokenResponse {
    fn into_access(self, grant: String) -> Access {
        Access {
            grant,
            token: self.access_token,
            expires: chrono::Utc::now().timestamp() + i64::from(self.expires_in),
        }
    }
}

impl Access {
    fn cache(&self) {
        // The renewable grant is stored separately so a platform credential
        // size limit on this short-lived cache cannot discard a rotated grant.
        if let Ok(data) = serde_json::to_string(self) {
            let _ = save_secret(ACCESS, &data);
        }
    }
}

pub(super) fn access_token(runtime: &tokio::runtime::Runtime) -> Result<String, String> {
    let mut session = SESSION
        .lock()
        .map_err(|_| "Cannot access the ChatGPT session")?;
    let _lock = lock_session()?;
    let mut connection =
        load_connection()?.ok_or("Connect your ChatGPT account in AI settings first")?;
    let grant = connection.grant();
    let valid = |access: &Access| {
        access.grant == grant && access.expires > chrono::Utc::now().timestamp() + 30
    };
    if let Some(cached) = session.as_ref().filter(|access| valid(access)) {
        return Ok(cached.token.clone());
    }
    if let Some(cached) = load_secret(ACCESS)?
        .and_then(|data| serde_json::from_str::<Access>(&data).ok())
        .filter(valid)
    {
        let token = cached.token.clone();
        *session = Some(cached);
        return Ok(token);
    }
    let tokens = runtime.block_on(async {
        exchange(
            &client()?,
            &[
                ("grant_type", "refresh_token"),
                ("client_id", &connection.client_id),
                ("refresh_token", &connection.refresh_token),
                ("resource", RESOURCE),
            ],
            true,
        )
        .await
    })?;
    if !tokens.refresh_token.is_empty() && tokens.refresh_token != connection.refresh_token {
        connection.refresh_token.clone_from(&tokens.refresh_token);
        save_connection(&connection)?;
    }
    let fresh = tokens.into_access(connection.grant());
    fresh.cache();
    let token = fresh.token.clone();
    *session = Some(fresh);
    Ok(token)
}

pub fn sign_out() -> Result<String, String> {
    let mut session = SESSION
        .lock()
        .map_err(|_| "Cannot access the ChatGPT session")?;
    let _lock = lock_session()?;
    *session = None;
    let Some(connection) = load_connection()? else {
        delete_secret(ACCESS)?;
        return Ok("ChatGPT disconnected".into());
    };
    let revoke = runtime()?.block_on(async {
        let client = client()?;
        let metadata: Value =
            read_json(client.get(format!("{ISSUER}/.well-known/openid-configuration"))).await?;
        let endpoint = metadata["revocation_endpoint"]
            .as_str()
            .and_then(|url| Url::parse(url).ok())
            .filter(|url| {
                url.origin().ascii_serialization() == ISSUER
                    && url.username().is_empty()
                    && url.password().is_none()
            })
            .ok_or("ChatGPT did not return a valid revocation endpoint")?;
        let response = send(client.post(endpoint).form(&[
            ("token", connection.refresh_token.as_str()),
            ("token_type_hint", "refresh_token"),
            ("client_id", &connection.client_id),
        ]))
        .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(http_error(response.status()))
        }
    });
    delete_secret(CONNECTION)?;
    delete_secret(ACCESS)?;
    revoke.map_err(|_| {
        "Disconnected locally. To revoke access on the server, open ChatGPT Settings > Usage."
            .to_owned()
    })?;
    Ok("ChatGPT disconnected".into())
}

fn http_error(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        401 => "ChatGPT rejected the session. Select Login to reconnect.".into(),
        403 => "Your ChatGPT plan or workspace does not allow this request.".into(),
        429 => "The ChatGPT usage limit was reached. Check ChatGPT Settings > Usage.".into(),
        _ => format!("ChatGPT returned HTTP {}", status.as_u16()),
    }
}

pub const PROVIDER_ID: &str = "openai-chatgpt";

pub struct Model {
    pub slug: String,
    pub display_name: String,
    pub reasoning_efforts: Vec<String>,
    pub(crate) input_cost_rank: u32,
}

const EFFORT_COST_ORDER: &[&str] = &["none", "minimal", "low", "medium", "high", "xhigh", "max"];

pub fn list_models() -> Result<Vec<Model>, String> {
    let runtime = runtime()?;
    let token = access_token(&runtime)?;
    runtime.block_on(async { fetch_models(&client()?, &token).await })
}

async fn fetch_models(client: &Client, token: &str) -> Result<Vec<Model>, String> {
    let catalog = read_json(client.get(format!("{RESOURCE}/models")).bearer_auth(token)).await?;
    parse_models(&catalog)
}

fn parse_models(catalog: &Value) -> Result<Vec<Model>, String> {
    let models: Vec<_> = catalog["models"]
        .as_array()
        .ok_or("ChatGPT returned an invalid model list")?
        .iter()
        .filter(|entry| entry["visibility"] == "list")
        .filter_map(|entry| {
            let slug = entry["slug"].as_str().filter(|slug| !slug.is_empty())?;
            let display_name = entry["display_name"].as_str().unwrap_or(slug);
            let reasoning_efforts =
                if let Some(levels) = entry["supported_reasoning_levels"].as_array() {
                    levels
                        .iter()
                        .filter_map(|level| level.as_str().or_else(|| level["effort"].as_str()))
                        .filter(|effort| {
                            matches!(
                                *effort,
                                "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
                            )
                        })
                        .map(str::to_owned)
                        .collect()
                } else {
                    documented_reasoning_efforts(slug)
                        .iter()
                        .map(|effort| (*effort).to_owned())
                        .collect()
                };
            Some(Model {
                slug: slug.into(),
                display_name: display_name.into(),
                reasoning_efforts,
                input_cost_rank: parse_input_cost_rank(entry, slug),
            })
        })
        .collect();
    if models.is_empty() {
        return Err("No eligible models were returned for this ChatGPT account".into());
    }
    Ok(models)
}

fn parse_input_cost_rank(entry: &Value, slug: &str) -> u32 {
    for key in ["input_cost_per_million", "input_price_per_million"] {
        if let Some(price) = entry.get(key).and_then(Value::as_f64) {
            return price.clamp(0.0, f64::from(u32::MAX)) as u32;
        }
    }
    if let Some(price) = entry
        .get("pricing")
        .and_then(|pricing| pricing.get("input_per_million"))
        .and_then(Value::as_f64)
    {
        return price.clamp(0.0, f64::from(u32::MAX)) as u32;
    }
    documented_model_cost_rank(slug)
}

fn documented_model_cost_rank(slug: &str) -> u32 {
    match slug {
        "gpt-6-luna" => 10,
        "gpt-6.1-terra" => 50,
        "gpt-6.2-sol" | "gpt-6.1-sol" | "gpt-6-sol" => 200,
        "gpt-6.2-astra" | "gpt-6-astra" => 800,
        _ => u32::MAX,
    }
}

impl Model {
    fn cost_rank(&self) -> u32 {
        self.input_cost_rank
    }
}

pub fn cheapest_model(models: &[Model]) -> Option<&Model> {
    models
        .iter()
        .min_by_key(|model| (model.cost_rank(), model.slug.as_str()))
}

pub fn cheapest_effort(model: &Model) -> Option<&str> {
    model
        .reasoning_efforts
        .iter()
        .min_by_key(|effort| {
            EFFORT_COST_ORDER
                .iter()
                .position(|level| level == effort)
                .unwrap_or(EFFORT_COST_ORDER.len())
        })
        .map(String::as_str)
}

fn documented_reasoning_efforts(slug: &str) -> &'static [&'static str] {
    match slug {
        "gpt-6.2-sol" | "gpt-6.2-astra" | "gpt-6.1-sol" => {
            &["low", "medium", "high", "xhigh", "max"]
        }
        "gpt-6.1-terra" | "gpt-6-luna" | "gpt-6-sol" => {
            &["none", "low", "medium", "high", "xhigh", "max"]
        }
        _ => &[],
    }
}

pub fn selected_model<'a>(models: &'a [Model], slug: &str) -> Option<&'a Model> {
    if slug.is_empty() {
        cheapest_model(models)
    } else {
        models.iter().find(|model| model.slug == slug)
    }
}

pub(super) async fn generate(config: &Config, patch: &str, token: &str) -> Result<String, String> {
    let client = client()?;
    let models = fetch_models(&client, token).await?;
    let body = request_body(config, patch, &models)?;
    let mut response = send(
        client
            .post(format!("{RESOURCE}/responses"))
            .bearer_auth(token)
            .json(&body),
    )
    .await?;
    if !response.status().is_success() {
        return Err(http_error(response.status()));
    }
    parse_response(&read_response_body(&mut response).await?)
}

fn request_body(config: &Config, patch: &str, models: &[Model]) -> Result<Value, String> {
    let settings = config.subscription_models.get(PROVIDER_ID);
    let slug = settings.map_or("", |settings| settings.model.as_str());
    let model = selected_model(models, slug)
        .ok_or("The selected ChatGPT model is unavailable. Choose another model in AI settings.")?;
    let effort = if slug.is_empty() {
        cheapest_effort(model).unwrap_or("")
    } else {
        settings.map_or("", |settings| settings.reasoning_effort.as_str())
    };
    let mut body = json!({
        "model": model.slug,
        "instructions": config.instructions.trim(),
        "input": [{
            "role": "user",
            "content": format!("Generate a commit message for these changes:\n\n{patch}")
        }],
        "store": false,
        "stream": true
    });
    if !effort.is_empty() {
        if !model
            .reasoning_efforts
            .iter()
            .any(|allowed| allowed == effort)
        {
            return Err("The selected ChatGPT model does not support that reasoning effort. Choose another level in AI settings.".into());
        }
        body["reasoning"] = json!({ "effort": effort });
    }
    Ok(body)
}

fn parse_response(body: &[u8]) -> Result<String, String> {
    let mut message = String::new();
    let body = std::str::from_utf8(body).map_err(|_| "ChatGPT returned invalid response text")?;
    for data in body.lines().filter_map(|line| line.strip_prefix("data:")) {
        let event: Value = serde_json::from_str(data.trim())
            .map_err(|_| "ChatGPT returned an invalid response event")?;
        match event["type"].as_str() {
            Some("response.output_text.delta") => {
                message.push_str(
                    event["delta"]
                        .as_str()
                        .ok_or("ChatGPT returned invalid response text")?,
                );
            }
            Some("response.completed") => {
                let message = message.trim();
                if message.is_empty() {
                    return Err("ChatGPT did not return a commit message".into());
                }
                crate::git::validate_message(message)?;
                return Ok(message.into());
            }
            Some("response.failed" | "response.incomplete" | "error") => {
                let code = event
                    .pointer("/response/error/code")
                    .or_else(|| event.get("code"))
                    .and_then(Value::as_str);
                return Err(match code {
                    Some("subscription_sharing_usage_limit_exceeded" | "subscription_sharing_usage_unavailable") =>
                        "ChatGPT plan usage is unavailable or its limit was reached. Check ChatGPT Settings > Usage.",
                    _ => "ChatGPT could not complete the commit message.",
                }.into());
            }
            _ => {}
        }
    }
    Err("The ChatGPT response ended before the commit message was complete".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_the_selected_providers_model_and_reasoning() {
        let models = parse_models(&json!({
            "models": [
                {"slug": "gpt-6.2-astra", "visibility": "list"},
                {"slug": "hidden", "visibility": "hide"},
                {"slug": "gpt-6-luna", "display_name": "Luna", "visibility": "list",
                 "supported_reasoning_levels": [{"effort": "none"}, {"effort": "low"}, {"effort": "high"}]},
                {"slug": "second", "display_name": "Second", "visibility": "list",
                 "supported_reasoning_levels": [{"effort": "low"}, {"effort": "high"}]}
            ]
        }))
        .unwrap();
        let mut config = Config::default();
        config.subscription_models.insert(
            "another-provider".into(),
            super::super::SubscriptionModelSettings {
                model: "hidden".into(),
                reasoning_effort: "max".into(),
            },
        );
        let default = request_body(&config, "patch", &models).unwrap();
        assert_eq!(default["model"], "gpt-6-luna");
        assert_eq!(default["reasoning"]["effort"], "none");
        assert_eq!(models.len(), 3);
        assert_eq!(models[1].display_name, "Luna");
        let selection = config
            .subscription_models
            .entry(PROVIDER_ID.into())
            .or_default();
        selection.model = "second".into();
        selection.reasoning_effort = "high".into();
        let selected = request_body(&config, "patch", &models).unwrap();
        assert_eq!(selected["model"], "second");
        assert_eq!(selected["reasoning"]["effort"], "high");
        config
            .subscription_models
            .get_mut(PROVIDER_ID)
            .unwrap()
            .reasoning_effort = "max".into();
        assert!(request_body(&config, "patch", &models).is_err());
    }

    #[test]
    fn callback_page_keeps_the_sign_in_instruction_local() {
        let success = callback_page(true);
        let rejected = callback_page(false);
        assert!(success.contains("<span>Sanny</span><span class=\"git\">Git</span>"));
        assert!(success.contains("class=\"ok\""));
        assert!(success.contains("Sign-in received"));
        assert!(success.contains("Return to SannyGit to see the result of ChatGPT sign-in."));
        assert!(success.contains("You can close this tab."));
        assert!(rejected.contains("class=\"bad\""));
        assert!(rejected.contains("Sign-in was not accepted"));
        assert!(rejected.contains("This request does not match the pending sign-in."));
        assert!(!success.contains("__"));
        assert!(!rejected.contains("__"));
        for page in [&success, &rejected] {
            assert!(page.starts_with("<!DOCTYPE html>"));
            assert!(!page.contains("https://") && !page.contains("http://"));
        }
    }

    #[test]
    fn callback_checks_state_and_registered_client() {
        assert!(parse_callback(
            "GET /auth/callback?state=bad&code=abc&client_id=oaiapp_test HTTP/1.1\r\n",
            "good",
            None
        )
        .is_none());
        assert!(parse_callback(
            "GET /auth/callback?state=good&state=bad&code=abc HTTP/1.1\r\n",
            "good",
            Some("oaiapp_test")
        )
        .is_none());
        assert_eq!(
            parse_callback(
                "GET /auth/callback?state=good&code=abc&client_id=oaiapp_test HTTP/1.1\r\n",
                "good",
                None
            ),
            Some(Ok(("abc".into(), "oaiapp_test".into())))
        );
        assert!(parse_callback(
            "GET /auth/callback?state=good&code=abc&client_id=oaiapp_other HTTP/1.1\r\n",
            "good",
            Some("oaiapp_test")
        )
        .unwrap()
        .is_err());
    }

    #[test]
    fn response_requires_completion_and_reassembles_text() {
        let partial = b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"Fix \"}\r\n\r\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"parsing\"}\n\n";
        assert!(parse_response(partial).is_err());
        let mut complete = partial.to_vec();
        complete.extend(b"data: {\"type\":\"response.completed\"}\n\n");
        assert_eq!(parse_response(&complete).unwrap(), "Fix parsing");
    }
}
