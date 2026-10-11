//! Account, connections and tokens - all served by the ProjectRem Cloudflare
//! Worker.
//!
//! The desktop app never stores a password itself: it talks to
//! `https://projectrem-api.<subdomain>.workers.dev` and keeps only the session
//! token locally.

use serde::{Deserialize, Serialize};

use crate::paths;

/// Overridable so a local Worker can be pointed at during development.
pub fn base_url() -> String {
    std::env::var("PROJECTREM_API_URL")
        .unwrap_or_else(|_| "https://projectrem-api.schoneniklas79.workers.dev".to_owned())
}

/// The three services a profile picture can come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Discord,
    Huggingface,
    Github,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Discord, Provider::Huggingface, Provider::Github];

    pub const fn code(self) -> &'static str {
        match self {
            Provider::Discord => "discord",
            Provider::Huggingface => "huggingface",
            Provider::Github => "github",
        }
    }

    pub const fn display(self) -> &'static str {
        match self {
            Provider::Discord => "Discord",
            Provider::Huggingface => "Hugging Face",
            Provider::Github => "GitHub",
        }
    }

    /// Callback path registered with the provider.
    pub const fn callback_path(self) -> &'static str {
        match self {
            Provider::Discord => "/callback/dc",
            Provider::Huggingface => "/callback/hf",
            Provider::Github => "/callback/gh",
        }
    }

    #[allow(dead_code, reason = "inverse of callback_path, used by tests")]
    pub fn from_callback_path(path: &str) -> Option<Provider> {
        match path {
            "/callback/dc" => Some(Provider::Discord),
            "/callback/hf" => Some(Provider::Huggingface),
            "/callback/gh" => Some(Provider::Github),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub provider: Provider,
    pub username: Option<String>,
    pub avatar_url: Option<String>,
    pub connected_at: String,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStatus {
    pub provider: Provider,
    pub stored: bool,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bot {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub provider: String,
    pub model: Option<String>,
    pub system_prompt: String,
    pub temperature: f64,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
// `ok` and `error` are decoded so a malformed reply surfaces as an error
// rather than as a silently empty response.
#[allow(dead_code)]
struct Envelope<T> {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    token: Option<String>,
    user: Option<T>,
    connections: Option<Vec<Connection>>,
    bots: Option<Vec<Bot>>,
}

/// Everything the Account tab needs to render.
#[derive(Debug, Default, Clone)]
pub struct AccountState {
    pub token: Option<String>,
    pub user: Option<User>,
    pub connections: Vec<Connection>,
    pub bots: Vec<Bot>,
    pub hf_token_stored: bool,
    pub hf_token_stored_at: Option<String>,
}

impl AccountState {
    pub fn signed_in(&self) -> bool {
        self.token.is_some() && self.user.is_some()
    }

    /// The connection that a given provider is connected through.
    #[allow(dead_code, reason = "convenience accessor")]
    pub fn connection(&self, provider: Provider) -> Option<&Connection> {
        self.connections.iter().find(|c| c.provider == provider)
    }

    /// Every connected service that offers a profile picture.
    pub fn avatar_choices(&self) -> Vec<(Provider, String, String)> {
        self.connections
            .iter()
            .filter_map(|c| {
                let url = c.avatar_url.clone()?;
                let name = c.username.clone().unwrap_or_else(|| c.provider.display().to_owned());
                Some((c.provider, name, url))
            })
            .collect()
    }
}

#[derive(Debug)]
pub enum ApiError {
    Network(String),
    Server(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Network(e) => write!(f, "{e}"),
            ApiError::Server(e) => write!(f, "{e}"),
        }
    }
}

type Result<T> = std::result::Result<T, ApiError>;

/// Blocking HTTP calls to the Worker.
///
/// Every call is short; the GUI runs them off the UI thread.
pub struct Api {
    client: reqwest::blocking::Client,
}

impl Api {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    fn request(&self, method: reqwest::Method, path: &str, token: Option<&str>) -> RequestBuilder {
        let url = format!("{}{path}", base_url());
        let mut builder = self.client.request(method, &url);
        if let Some(token) = token {
            builder = builder.bearer_auth(token);
        }
        builder
    }

    fn send<T: serde::de::DeserializeOwned>(&self, builder: RequestBuilder) -> Result<T> {
        let response = builder.send().map_err(|e| ApiError::Network(e.to_string()))?;
        let status = response.status();
        let text = response.text().map_err(|e| ApiError::Network(e.to_string()))?;

        if status.is_client_error() || status.is_server_error() {
            // The Worker always answers with {"ok":false,"error":"..."}.
            if let Ok(env) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(message) = env.get("error").and_then(|v| v.as_str()) {
                    return Err(ApiError::Server(message.to_owned()));
                }
            }
            return Err(ApiError::Server(format!("HTTP {}", status.as_u16())));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Server(format!("bad reply: {e}")))
    }

    fn send_ok(&self, builder: RequestBuilder) -> Result<()> {
        let response = builder.send().map_err(|e| ApiError::Network(e.to_string()))?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let text = response.text().unwrap_or_default();
        if let Ok(env) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(message) = env.get("error").and_then(|v| v.as_str()) {
                return Err(ApiError::Server(message.to_owned()));
            }
        }
        Err(ApiError::Server(format!("HTTP {}", status.as_u16())))
    }

    #[allow(dead_code, reason = "used by diagnostics")]
    pub fn health(&self) -> Result<bool> {
        let value: serde_json::Value = self.send(self.request(reqwest::Method::GET, "/api/health", None))?;
        Ok(value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false))
    }

    pub fn register(&self, username: &str, password: &str) -> Result<User> {
        let env: Envelope<User> = self.send(
            self.request(reqwest::Method::POST, "/api/auth/register", None)
                .json(&serde_json::json!({ "username": username, "password": password })),
        )?;
        env.user.ok_or_else(|| ApiError::Server("register returned no user".into()))
    }

    pub fn login(&self, username: &str, password: &str) -> Result<(String, User)> {
        let env: Envelope<User> = self.send(
            self.request(reqwest::Method::POST, "/api/auth/login", None)
                .json(&serde_json::json!({ "username": username, "password": password })),
        )?;
        let token = env.token.ok_or_else(|| ApiError::Server("login returned no token".into()))?;
        let user = env.user.ok_or_else(|| ApiError::Server("login returned no user".into()))?;
        Ok((token, user))
    }

    pub fn logout(&self, token: &str) -> Result<()> {
        self.send_ok(self.request(reqwest::Method::POST, "/api/auth/logout", Some(token)).json(&serde_json::json!({})))
    }

    pub fn me(&self, token: &str) -> Result<User> {
        let env: Envelope<User> =
            self.send(self.request(reqwest::Method::GET, "/api/auth/me", Some(token)))?;
        env.user.ok_or_else(|| ApiError::Server("no user in reply".into()))
    }

    pub fn connections(&self, token: &str) -> Result<Vec<Connection>> {
        let env: Envelope<Connection> =
            self.send(self.request(reqwest::Method::GET, "/api/connections", Some(token)))?;
        Ok(env.connections.unwrap_or_default())
    }

    pub fn save_connection(
        &self,
        token: &str,
        provider: Provider,
        username: Option<&str>,
        avatar_url: Option<&str>,
        access_token: Option<&str>,
    ) -> Result<()> {
        self.send_ok(
            self.request(reqwest::Method::POST, &format!("/api/connections/{}", provider.code()), Some(token))
                .json(&serde_json::json!({
                    "username": username,
                    "avatar_url": avatar_url,
                    "access_token": access_token,
                })),
        )
    }

    pub fn disconnect(&self, token: &str, provider: Provider) -> Result<()> {
        self.send_ok(self.request(
            reqwest::Method::DELETE,
            &format!("/api/connections/{}", provider.code()),
            Some(token),
        ))
    }

    pub fn set_avatar(&self, token: &str, avatar_url: Option<&str>) -> Result<User> {
        let env: Envelope<User> = self.send(
            self.request(reqwest::Method::PATCH, "/api/profile", Some(token))
                .json(&serde_json::json!({ "avatar_url": avatar_url })),
        )?;
        env.user.ok_or_else(|| ApiError::Server("no user in reply".into()))
    }

    pub fn store_token(&self, token: &str, provider: Provider, value: &str) -> Result<()> {
        self.send_ok(
            self.request(reqwest::Method::PUT, &format!("/api/tokens/{}", provider.code()), Some(token))
                .json(&serde_json::json!({ "token": value })),
        )
    }

    pub fn token_status(&self, token: &str, provider: Provider) -> Result<TokenStatus> {
        self.send(self.request(reqwest::Method::GET, &format!("/api/tokens/{}", provider.code()), Some(token)))
    }

    pub fn bots(&self, token: &str) -> Result<Vec<Bot>> {
        let env: Envelope<Bot> =
            self.send(self.request(reqwest::Method::GET, "/api/bots", Some(token)))?;
        Ok(env.bots.unwrap_or_default())
    }
}

type RequestBuilder = reqwest::blocking::RequestBuilder;

// ------------------------------------------------------------ local session

/// The session token we keep between runs so the user is not asked to sign in
/// every time.
pub fn load_token() -> Option<String> {
    let raw = std::fs::read_to_string(paths::data_dir().join("session.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    value.get("token")?.as_str().map(str::to_owned)
}

pub fn save_token(token: &str) {
    let path = paths::data_dir().join("session.json");
    let json = serde_json::json!({ "token": token, "saved_at": crate::version::build_stamp() });
    if let Ok(text) = serde_json::to_string_pretty(&json) {
        let _ = std::fs::write(path, text);
    }
}

pub fn clear_token() {
    let _ = std::fs::remove_file(paths::data_dir().join("session.json"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_paths_match_the_registered_callbacks() {
        assert_eq!(Provider::Huggingface.callback_path(), "/callback/hf");
        assert_eq!(Provider::Discord.callback_path(), "/callback/dc");
        assert_eq!(Provider::Github.callback_path(), "/callback/gh");
    }

    #[test]
    fn callbacks_round_trip() {
        for p in Provider::ALL {
            assert_eq!(Provider::from_callback_path(p.callback_path()), Some(p));
        }
        assert_eq!(Provider::from_callback_path("/callback/nope"), None);
    }

    #[test]
    fn avatar_choices_only_include_connected_services() {
        let state = AccountState {
            connections: vec![
                Connection {
                    provider: Provider::Huggingface,
                    username: Some("hf-user".into()),
                    avatar_url: Some("https://hf/a.png".into()),
                    connected_at: "now".into(),
                    connected: true,
                },
                Connection {
                    provider: Provider::Github,
                    username: Some("gh".into()),
                    avatar_url: None,
                    connected_at: "now".into(),
                    connected: true,
                },
            ],
            ..Default::default()
        };
        let choices = state.avatar_choices();
        assert_eq!(choices.len(), 1, "a connection without an avatar offers nothing");
        assert_eq!(choices[0].0, Provider::Huggingface);
    }
}
